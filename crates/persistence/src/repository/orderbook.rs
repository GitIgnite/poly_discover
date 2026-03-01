//! Orderbook repository — persistence for BTC 15-min market analysis

use crate::DbResult;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Record structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ObMarketRecord {
    pub id: Option<i64>,
    pub condition_id: String,
    pub question: Option<String>,
    pub slug: Option<String>,
    pub token_id_up: Option<String>,
    pub token_id_down: Option<String>,
    pub start_time: i64,
    pub end_time: i64,
    pub outcome: Option<String>,
    pub outcome_price_up: Option<f64>,
    pub outcome_price_down: Option<f64>,
    pub volume: Option<f64>,
    pub data_fetched: Option<i64>,
    pub data_points_count: Option<i64>,
    pub created_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ObPriceRecord {
    pub id: Option<i64>,
    pub market_id: i64,
    pub timestamp_ms: i64,
    pub elapsed_seconds: f64,
    pub price: f64,
    pub side: Option<String>,
    pub size: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ObFeatureRecord {
    pub id: Option<i64>,
    pub market_id: i64,
    pub time_window: i64,
    pub last_price: Option<f64>,
    pub vwap: Option<f64>,
    pub price_change: Option<f64>,
    pub price_volatility: Option<f64>,
    pub momentum: Option<f64>,
    pub max_price: Option<f64>,
    pub min_price: Option<f64>,
    pub price_range: Option<f64>,
    pub data_points: Option<i64>,
    pub buy_volume: Option<f64>,
    pub sell_volume: Option<f64>,
    pub volume_imbalance: Option<f64>,
    pub trade_count: Option<i64>,
    pub avg_trade_size: Option<f64>,
    pub large_trade_ratio: Option<f64>,
    pub avg_spread: Option<f64>,
    pub depth_imbalance: Option<f64>,
    pub avg_bid_depth: Option<f64>,
    pub avg_ask_depth: Option<f64>,
    pub outcome_is_up: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ObSnapshotRecord {
    pub id: Option<i64>,
    pub condition_id: String,
    pub token_id: String,
    pub timestamp_ms: i64,
    pub elapsed_seconds: f64,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub spread: Option<f64>,
    pub mid_price: Option<f64>,
    pub bid_depth_total: Option<f64>,
    pub ask_depth_total: Option<f64>,
    pub depth_imbalance: Option<f64>,
    pub bid_levels: Option<i64>,
    pub ask_levels: Option<i64>,
    pub created_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ObPatternRecord {
    pub id: Option<i64>,
    pub pattern_name: String,
    pub pattern_type: String,
    pub time_window: i64,
    pub direction: String,
    pub features_used: String,
    pub threshold_json: String,
    pub accuracy: f64,
    pub precision_pct: Option<f64>,
    pub recall_pct: Option<f64>,
    pub f1_score: Option<f64>,
    pub sample_size: i64,
    pub up_count: i64,
    pub down_count: i64,
    pub confidence_95_low: Option<f64>,
    pub confidence_95_high: Option<f64>,
    pub first_half_accuracy: Option<f64>,
    pub second_half_accuracy: Option<f64>,
    pub stability_score: Option<f64>,
    pub analysis_run_id: Option<String>,
    pub created_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObMarketStats {
    pub total_markets: i64,
    pub fetched_markets: i64,
    pub features_extracted: i64,
    pub total_prices: i64,
    pub total_features: i64,
    pub total_snapshots: i64,
    pub total_patterns: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DbSizeStats {
    pub ob_markets: i64,
    pub ob_market_prices: i64,
    pub ob_market_features: i64,
    pub ob_snapshots: i64,
    pub ob_patterns: i64,
}

/// Resume stats: counts of markets in each state for incremental resume.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResumeStats {
    pub total: i64,
    pub unfetched: i64,
    pub fetched: i64,
    pub extracted: i64,
    pub patterns: i64,
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

pub struct OrderbookRepository;

impl OrderbookRepository {
    // =======================================================================
    // Markets
    // =======================================================================

    /// Batch insert markets (INSERT OR IGNORE by condition_id UNIQUE).
    /// Returns the number of newly inserted rows.
    pub async fn save_markets_batch(pool: &SqlitePool, markets: &[ObMarketRecord]) -> DbResult<usize> {
        let mut inserted = 0usize;
        for chunk in markets.chunks(500) {
            let mut tx = pool.begin().await?;
            for m in chunk {
                let result = sqlx::query(
                    r#"INSERT OR IGNORE INTO ob_markets
                        (condition_id, question, slug, token_id_up, token_id_down,
                         start_time, end_time, outcome, outcome_price_up, outcome_price_down, volume)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
                )
                .bind(&m.condition_id)
                .bind(&m.question)
                .bind(&m.slug)
                .bind(&m.token_id_up)
                .bind(&m.token_id_down)
                .bind(m.start_time)
                .bind(m.end_time)
                .bind(&m.outcome)
                .bind(m.outcome_price_up)
                .bind(m.outcome_price_down)
                .bind(m.volume)
                .execute(&mut *tx)
                .await?;
                if result.rows_affected() > 0 {
                    inserted += 1;
                }
            }
            tx.commit().await?;
        }
        Ok(inserted)
    }

    /// Get markets that haven't been fetched yet (data_fetched=0).
    pub async fn get_unfetched_markets(pool: &SqlitePool, limit: i64) -> DbResult<Vec<ObMarketRecord>> {
        let records = sqlx::query_as::<_, ObMarketRecord>(
            "SELECT * FROM ob_markets WHERE data_fetched = 0 ORDER BY start_time ASC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(records)
    }

    /// Mark a market as fetched (data_fetched=1) and store data_points_count.
    pub async fn mark_market_fetched(pool: &SqlitePool, id: i64, data_points: i64) -> DbResult<()> {
        sqlx::query("UPDATE ob_markets SET data_fetched = 1, data_points_count = ?1 WHERE id = ?2")
            .bind(data_points)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Mark a market as features-extracted (data_fetched=2).
    pub async fn mark_features_extracted(pool: &SqlitePool, id: i64) -> DbResult<()> {
        sqlx::query("UPDATE ob_markets SET data_fetched = 2 WHERE id = ?1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Get total number of markets.
    pub async fn get_market_count(pool: &SqlitePool) -> DbResult<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_markets")
            .fetch_one(pool)
            .await?;
        Ok(count)
    }

    /// Get aggregate market stats.
    pub async fn get_market_stats(pool: &SqlitePool) -> DbResult<ObMarketStats> {
        let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_markets")
            .fetch_one(pool)
            .await?;
        let (fetched,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM ob_markets WHERE data_fetched >= 1")
                .fetch_one(pool)
                .await?;
        let (extracted,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM ob_markets WHERE data_fetched >= 2")
                .fetch_one(pool)
                .await?;
        let (prices,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_market_prices")
            .fetch_one(pool)
            .await?;
        let (features,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_market_features")
            .fetch_one(pool)
            .await?;
        let (snapshots,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_snapshots")
            .fetch_one(pool)
            .await?;
        let (patterns,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_patterns")
            .fetch_one(pool)
            .await?;

        Ok(ObMarketStats {
            total_markets: total,
            fetched_markets: fetched,
            features_extracted: extracted,
            total_prices: prices,
            total_features: features,
            total_snapshots: snapshots,
            total_patterns: patterns,
        })
    }

    /// Get markets with fetched data (data_fetched=1) that need feature extraction.
    pub async fn get_fetched_markets(pool: &SqlitePool, limit: i64) -> DbResult<Vec<ObMarketRecord>> {
        let records = sqlx::query_as::<_, ObMarketRecord>(
            "SELECT * FROM ob_markets WHERE data_fetched = 1 ORDER BY start_time ASC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(records)
    }

    // =======================================================================
    // Prices (batch insert)
    // =======================================================================

    /// Batch insert price records (INSERT OR IGNORE by UNIQUE(market_id, timestamp_ms)).
    pub async fn save_prices_batch(pool: &SqlitePool, prices: &[ObPriceRecord]) -> DbResult<usize> {
        let mut inserted = 0usize;
        for chunk in prices.chunks(500) {
            let mut tx = pool.begin().await?;
            for p in chunk {
                let result = sqlx::query(
                    r#"INSERT OR IGNORE INTO ob_market_prices
                        (market_id, timestamp_ms, elapsed_seconds, price, side, size)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
                )
                .bind(p.market_id)
                .bind(p.timestamp_ms)
                .bind(p.elapsed_seconds)
                .bind(p.price)
                .bind(&p.side)
                .bind(p.size)
                .execute(&mut *tx)
                .await?;
                if result.rows_affected() > 0 {
                    inserted += 1;
                }
            }
            tx.commit().await?;
        }
        Ok(inserted)
    }

    /// Get all prices for a given market, ordered by elapsed_seconds.
    pub async fn get_prices_for_market(pool: &SqlitePool, market_id: i64) -> DbResult<Vec<ObPriceRecord>> {
        let records = sqlx::query_as::<_, ObPriceRecord>(
            "SELECT * FROM ob_market_prices WHERE market_id = ?1 ORDER BY elapsed_seconds ASC",
        )
        .bind(market_id)
        .fetch_all(pool)
        .await?;
        Ok(records)
    }

    // =======================================================================
    // Features (batch insert)
    // =======================================================================

    /// Batch insert feature records (INSERT OR REPLACE by UNIQUE(market_id, time_window)).
    pub async fn save_features_batch(pool: &SqlitePool, features: &[ObFeatureRecord]) -> DbResult<usize> {
        let mut inserted = 0usize;
        for chunk in features.chunks(500) {
            let mut tx = pool.begin().await?;
            for f in chunk {
                let result = sqlx::query(
                    r#"INSERT OR REPLACE INTO ob_market_features
                        (market_id, time_window, last_price, vwap, price_change, price_volatility,
                         momentum, max_price, min_price, price_range, data_points,
                         buy_volume, sell_volume, volume_imbalance, trade_count, avg_trade_size,
                         large_trade_ratio, avg_spread, depth_imbalance, avg_bid_depth, avg_ask_depth,
                         outcome_is_up)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                               ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)"#,
                )
                .bind(f.market_id)
                .bind(f.time_window)
                .bind(f.last_price)
                .bind(f.vwap)
                .bind(f.price_change)
                .bind(f.price_volatility)
                .bind(f.momentum)
                .bind(f.max_price)
                .bind(f.min_price)
                .bind(f.price_range)
                .bind(f.data_points)
                .bind(f.buy_volume)
                .bind(f.sell_volume)
                .bind(f.volume_imbalance)
                .bind(f.trade_count)
                .bind(f.avg_trade_size)
                .bind(f.large_trade_ratio)
                .bind(f.avg_spread)
                .bind(f.depth_imbalance)
                .bind(f.avg_bid_depth)
                .bind(f.avg_ask_depth)
                .bind(f.outcome_is_up)
                .execute(&mut *tx)
                .await?;
                if result.rows_affected() > 0 {
                    inserted += 1;
                }
            }
            tx.commit().await?;
        }
        Ok(inserted)
    }

    /// Get all features for a given time window.
    pub async fn get_all_features_for_window(
        pool: &SqlitePool,
        window: i64,
    ) -> DbResult<Vec<ObFeatureRecord>> {
        let records = sqlx::query_as::<_, ObFeatureRecord>(
            "SELECT * FROM ob_market_features WHERE time_window = ?1 ORDER BY market_id ASC",
        )
        .bind(window)
        .fetch_all(pool)
        .await?;
        Ok(records)
    }

    /// Get all features grouped by market_id.
    pub async fn get_features_grouped_by_market(
        pool: &SqlitePool,
    ) -> DbResult<HashMap<i64, Vec<ObFeatureRecord>>> {
        let records = sqlx::query_as::<_, ObFeatureRecord>(
            "SELECT * FROM ob_market_features ORDER BY market_id ASC, time_window ASC",
        )
        .fetch_all(pool)
        .await?;

        let mut grouped: HashMap<i64, Vec<ObFeatureRecord>> = HashMap::new();
        for r in records {
            grouped.entry(r.market_id).or_default().push(r);
        }
        Ok(grouped)
    }

    // =======================================================================
    // Snapshots (batch insert)
    // =======================================================================

    /// Batch insert snapshot records.
    pub async fn save_snapshots_batch(pool: &SqlitePool, snaps: &[ObSnapshotRecord]) -> DbResult<usize> {
        let mut inserted = 0usize;
        for chunk in snaps.chunks(500) {
            let mut tx = pool.begin().await?;
            for s in chunk {
                sqlx::query(
                    r#"INSERT INTO ob_snapshots
                        (condition_id, token_id, timestamp_ms, elapsed_seconds,
                         best_bid, best_ask, spread, mid_price,
                         bid_depth_total, ask_depth_total, depth_imbalance,
                         bid_levels, ask_levels)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
                )
                .bind(&s.condition_id)
                .bind(&s.token_id)
                .bind(s.timestamp_ms)
                .bind(s.elapsed_seconds)
                .bind(s.best_bid)
                .bind(s.best_ask)
                .bind(s.spread)
                .bind(s.mid_price)
                .bind(s.bid_depth_total)
                .bind(s.ask_depth_total)
                .bind(s.depth_imbalance)
                .bind(s.bid_levels)
                .bind(s.ask_levels)
                .execute(&mut *tx)
                .await?;
                inserted += 1;
            }
            tx.commit().await?;
        }
        Ok(inserted)
    }

    // =======================================================================
    // Patterns
    // =======================================================================

    /// Save detected patterns (replace existing for same run_id).
    pub async fn save_patterns(
        pool: &SqlitePool,
        patterns: &[ObPatternRecord],
        run_id: &str,
    ) -> DbResult<usize> {
        // Delete previous patterns from this run
        sqlx::query("DELETE FROM ob_patterns WHERE analysis_run_id = ?1")
            .bind(run_id)
            .execute(pool)
            .await?;

        let mut inserted = 0usize;
        for chunk in patterns.chunks(500) {
            let mut tx = pool.begin().await?;
            for p in chunk {
                sqlx::query(
                    r#"INSERT INTO ob_patterns
                        (pattern_name, pattern_type, time_window, direction, features_used,
                         threshold_json, accuracy, precision_pct, recall_pct, f1_score,
                         sample_size, up_count, down_count, confidence_95_low, confidence_95_high,
                         first_half_accuracy, second_half_accuracy, stability_score, analysis_run_id)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                               ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)"#,
                )
                .bind(&p.pattern_name)
                .bind(&p.pattern_type)
                .bind(p.time_window)
                .bind(&p.direction)
                .bind(&p.features_used)
                .bind(&p.threshold_json)
                .bind(p.accuracy)
                .bind(p.precision_pct)
                .bind(p.recall_pct)
                .bind(p.f1_score)
                .bind(p.sample_size)
                .bind(p.up_count)
                .bind(p.down_count)
                .bind(p.confidence_95_low)
                .bind(p.confidence_95_high)
                .bind(p.first_half_accuracy)
                .bind(p.second_half_accuracy)
                .bind(p.stability_score)
                .bind(run_id)
                .execute(&mut *tx)
                .await?;
                inserted += 1;
            }
            tx.commit().await?;
        }
        Ok(inserted)
    }

    /// Get top patterns by accuracy.
    pub async fn get_top_patterns(pool: &SqlitePool, limit: i64) -> DbResult<Vec<ObPatternRecord>> {
        let records = sqlx::query_as::<_, ObPatternRecord>(
            "SELECT * FROM ob_patterns ORDER BY accuracy DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(records)
    }

    /// Get patterns for a specific time window.
    pub async fn get_patterns_by_window(
        pool: &SqlitePool,
        window: i64,
    ) -> DbResult<Vec<ObPatternRecord>> {
        let records = sqlx::query_as::<_, ObPatternRecord>(
            "SELECT * FROM ob_patterns WHERE time_window = ?1 ORDER BY accuracy DESC",
        )
        .bind(window)
        .fetch_all(pool)
        .await?;
        Ok(records)
    }

    // =======================================================================
    // Retention / Cleanup
    // =======================================================================

    /// Purge price data for markets where features have been extracted (data_fetched=2).
    pub async fn purge_prices_for_extracted(pool: &SqlitePool) -> DbResult<u64> {
        let result = sqlx::query(
            r#"DELETE FROM ob_market_prices WHERE market_id IN
               (SELECT id FROM ob_markets WHERE data_fetched = 2)"#,
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Purge snapshots older than N days.
    pub async fn purge_old_snapshots(pool: &SqlitePool, days: i64) -> DbResult<u64> {
        let cutoff = chrono::Utc::now().timestamp() - (days * 86400);
        let result = sqlx::query("DELETE FROM ob_snapshots WHERE created_at < ?1")
            .bind(cutoff)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Get row counts per orderbook table.
    pub async fn get_db_size_stats(pool: &SqlitePool) -> DbResult<DbSizeStats> {
        let (m,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_markets")
            .fetch_one(pool)
            .await?;
        let (p,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_market_prices")
            .fetch_one(pool)
            .await?;
        let (f,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_market_features")
            .fetch_one(pool)
            .await?;
        let (s,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_snapshots")
            .fetch_one(pool)
            .await?;
        let (pa,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_patterns")
            .fetch_one(pool)
            .await?;

        Ok(DbSizeStats {
            ob_markets: m,
            ob_market_prices: p,
            ob_market_features: f,
            ob_snapshots: s,
            ob_patterns: pa,
        })
    }

    // =======================================================================
    // Incremental Resume — State Persistence
    // =======================================================================

    /// Get resume stats: counts of markets in each processing state.
    pub async fn get_resume_stats(pool: &SqlitePool) -> DbResult<ResumeStats> {
        let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_markets")
            .fetch_one(pool)
            .await?;
        let (unfetched,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM ob_markets WHERE data_fetched = 0")
                .fetch_one(pool)
                .await?;
        let (fetched,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM ob_markets WHERE data_fetched = 1")
                .fetch_one(pool)
                .await?;
        let (extracted,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM ob_markets WHERE data_fetched = 2")
                .fetch_one(pool)
                .await?;
        let (patterns,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ob_patterns")
            .fetch_one(pool)
            .await?;
        Ok(ResumeStats {
            total,
            unfetched,
            fetched,
            extracted,
            patterns,
        })
    }

    /// Get a persisted state value by key.
    pub async fn get_state(pool: &SqlitePool, key: &str) -> DbResult<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM ob_backtest_state WHERE key = ?1")
                .bind(key)
                .fetch_optional(pool)
                .await?;
        Ok(row.map(|(v,)| v))
    }

    /// Set a persisted state value (INSERT OR REPLACE).
    pub async fn set_state(pool: &SqlitePool, key: &str, value: &str) -> DbResult<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO ob_backtest_state (key, value, updated_at) VALUES (?1, ?2, strftime('%s','now'))",
        )
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Get the latest market end_time from ob_markets (for incremental discovery).
    pub async fn get_latest_market_end_time(pool: &SqlitePool) -> DbResult<Option<i64>> {
        let row: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT MAX(end_time) FROM ob_markets")
                .fetch_optional(pool)
                .await?;
        Ok(row.and_then(|(v,)| v))
    }

    /// Reset fetch status: set all markets back to unfetched, delete features and prices.
    /// Keeps market metadata intact so they don't need to be re-discovered.
    pub async fn reset_fetch_status(pool: &SqlitePool) -> DbResult<u64> {
        let mut total = 0u64;
        total += sqlx::query("DELETE FROM ob_patterns")
            .execute(pool)
            .await?
            .rows_affected();
        total += sqlx::query("DELETE FROM ob_market_features")
            .execute(pool)
            .await?
            .rows_affected();
        total += sqlx::query("DELETE FROM ob_market_prices")
            .execute(pool)
            .await?
            .rows_affected();
        total += sqlx::query("UPDATE ob_markets SET data_fetched = 0, data_points_count = 0")
            .execute(pool)
            .await?
            .rows_affected();
        // Reset backtest state so it re-fetches
        let _ = sqlx::query("DELETE FROM ob_backtest_state WHERE key = 'last_step_completed'")
            .execute(pool)
            .await;
        Ok(total)
    }

    /// Full reset: delete ALL orderbook data from all tables.
    pub async fn full_reset(pool: &SqlitePool) -> DbResult<u64> {
        let mut total = 0u64;
        total += sqlx::query("DELETE FROM ob_patterns")
            .execute(pool)
            .await?
            .rows_affected();
        total += sqlx::query("DELETE FROM ob_market_features")
            .execute(pool)
            .await?
            .rows_affected();
        total += sqlx::query("DELETE FROM ob_market_prices")
            .execute(pool)
            .await?
            .rows_affected();
        total += sqlx::query("DELETE FROM ob_snapshots")
            .execute(pool)
            .await?
            .rows_affected();
        total += sqlx::query("DELETE FROM ob_markets")
            .execute(pool)
            .await?
            .rows_affected();
        total += sqlx::query("DELETE FROM ob_backtest_state")
            .execute(pool)
            .await?
            .rows_affected();
        Ok(total)
    }
}
