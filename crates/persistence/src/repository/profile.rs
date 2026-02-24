//! Profile repository — persistence for user profile analyses

use crate::DbResult;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// A persisted profile analysis record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProfileAnalysisRecord {
    pub id: Option<i64>,
    pub wallet: String,
    pub username: Option<String>,
    pub portfolio_value: Option<f64>,
    pub total_pnl: Option<f64>,
    pub total_volume: Option<f64>,
    pub total_trades: Option<i64>,
    pub unique_markets: Option<i64>,
    pub win_rate: Option<f64>,
    pub primary_strategy: Option<String>,
    pub strategy_confidence: Option<f64>,
    pub open_positions_json: Option<String>,
    pub closed_positions_json: Option<String>,
    pub markets_json: Option<String>,
    pub category_breakdown_json: Option<String>,
    pub activity_timeline_json: Option<String>,
    pub strategy_signals_json: Option<String>,
    pub avg_hold_duration: Option<f64>,
    pub best_trade_pnl: Option<f64>,
    pub worst_trade_pnl: Option<f64>,
    pub max_drawdown: Option<f64>,
    pub active_days: Option<i64>,
    pub avg_position_size: Option<f64>,
    pub analyzed_at: Option<i64>,
    pub created_at: Option<String>,
}

/// A single trade record for profile analysis
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProfileTradeRecord {
    pub id: Option<i64>,
    pub wallet: String,
    pub trade_hash: String,
    pub side: String,
    pub condition_id: String,
    pub asset: Option<String>,
    pub size: f64,
    pub price: f64,
    pub title: Option<String>,
    pub outcome: Option<String>,
    pub event_slug: Option<String>,
    pub timestamp: f64,
    pub transaction_hash: Option<String>,
    pub created_at: Option<String>,
}

/// Repository for profile analyses
pub struct ProfileRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ProfileRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert or update a profile analysis (upsert by wallet)
    pub async fn save_analysis(&self, record: &ProfileAnalysisRecord) -> DbResult<i64> {
        let result = sqlx::query(
            r#"INSERT INTO profile_analyses
                (wallet, username, portfolio_value, total_pnl, total_volume,
                 total_trades, unique_markets, win_rate, primary_strategy,
                 strategy_confidence, open_positions_json, closed_positions_json,
                 markets_json, category_breakdown_json, activity_timeline_json,
                 strategy_signals_json, avg_hold_duration, best_trade_pnl,
                 worst_trade_pnl, max_drawdown, active_days, avg_position_size,
                 analyzed_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                       ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22,
                       strftime('%s', 'now'))
               ON CONFLICT(wallet) DO UPDATE SET
                 username = excluded.username,
                 portfolio_value = excluded.portfolio_value,
                 total_pnl = excluded.total_pnl,
                 total_volume = excluded.total_volume,
                 total_trades = excluded.total_trades,
                 unique_markets = excluded.unique_markets,
                 win_rate = excluded.win_rate,
                 primary_strategy = excluded.primary_strategy,
                 strategy_confidence = excluded.strategy_confidence,
                 open_positions_json = excluded.open_positions_json,
                 closed_positions_json = excluded.closed_positions_json,
                 markets_json = excluded.markets_json,
                 category_breakdown_json = excluded.category_breakdown_json,
                 activity_timeline_json = excluded.activity_timeline_json,
                 strategy_signals_json = excluded.strategy_signals_json,
                 avg_hold_duration = excluded.avg_hold_duration,
                 best_trade_pnl = excluded.best_trade_pnl,
                 worst_trade_pnl = excluded.worst_trade_pnl,
                 max_drawdown = excluded.max_drawdown,
                 active_days = excluded.active_days,
                 avg_position_size = excluded.avg_position_size,
                 analyzed_at = strftime('%s', 'now')
            "#,
        )
        .bind(&record.wallet)
        .bind(&record.username)
        .bind(record.portfolio_value)
        .bind(record.total_pnl)
        .bind(record.total_volume)
        .bind(record.total_trades)
        .bind(record.unique_markets)
        .bind(record.win_rate)
        .bind(&record.primary_strategy)
        .bind(record.strategy_confidence)
        .bind(&record.open_positions_json)
        .bind(&record.closed_positions_json)
        .bind(&record.markets_json)
        .bind(&record.category_breakdown_json)
        .bind(&record.activity_timeline_json)
        .bind(&record.strategy_signals_json)
        .bind(record.avg_hold_duration)
        .bind(record.best_trade_pnl)
        .bind(record.worst_trade_pnl)
        .bind(record.max_drawdown)
        .bind(record.active_days)
        .bind(record.avg_position_size)
        .execute(self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get a profile analysis by wallet address
    pub async fn get_analysis(&self, wallet: &str) -> DbResult<Option<ProfileAnalysisRecord>> {
        let record = sqlx::query_as::<_, ProfileAnalysisRecord>(
            "SELECT * FROM profile_analyses WHERE wallet = ?1",
        )
        .bind(wallet)
        .fetch_optional(self.pool)
        .await?;

        Ok(record)
    }

    /// Get all profile analyses, ordered by most recent
    pub async fn get_all_analyses(&self) -> DbResult<Vec<ProfileAnalysisRecord>> {
        let records = sqlx::query_as::<_, ProfileAnalysisRecord>(
            "SELECT * FROM profile_analyses ORDER BY analyzed_at DESC",
        )
        .fetch_all(self.pool)
        .await?;

        Ok(records)
    }

    /// Insert trades with deduplication (INSERT OR IGNORE by trade_hash)
    /// Returns the number of newly inserted trades.
    pub async fn save_trades(&self, trades: &[ProfileTradeRecord]) -> DbResult<usize> {
        let mut inserted = 0usize;
        for trade in trades {
            let result = sqlx::query(
                r#"INSERT OR IGNORE INTO profile_trades
                    (wallet, trade_hash, side, condition_id, asset, size, price,
                     title, outcome, event_slug, timestamp, transaction_hash)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                "#,
            )
            .bind(&trade.wallet)
            .bind(&trade.trade_hash)
            .bind(&trade.side)
            .bind(&trade.condition_id)
            .bind(&trade.asset)
            .bind(trade.size)
            .bind(trade.price)
            .bind(&trade.title)
            .bind(&trade.outcome)
            .bind(&trade.event_slug)
            .bind(trade.timestamp)
            .bind(&trade.transaction_hash)
            .execute(self.pool)
            .await?;

            if result.rows_affected() > 0 {
                inserted += 1;
            }
        }
        Ok(inserted)
    }

    /// Get trades for a specific wallet grouped by condition_id
    pub async fn get_trades_by_market(
        &self,
        wallet: &str,
        condition_id: &str,
    ) -> DbResult<Vec<ProfileTradeRecord>> {
        let records = sqlx::query_as::<_, ProfileTradeRecord>(
            r#"SELECT * FROM profile_trades
               WHERE wallet = ?1 AND condition_id = ?2
               ORDER BY timestamp ASC"#,
        )
        .bind(wallet)
        .bind(condition_id)
        .fetch_all(self.pool)
        .await?;

        Ok(records)
    }

    /// Delete a profile analysis and its trades
    pub async fn delete_analysis(&self, wallet: &str) -> DbResult<()> {
        sqlx::query("DELETE FROM profile_trades WHERE wallet = ?1")
            .bind(wallet)
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM profile_analyses WHERE wallet = ?1")
            .bind(wallet)
            .execute(self.pool)
            .await?;
        Ok(())
    }
}
