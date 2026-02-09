//! Leaderboard repository — persistence for trader analysis and trade watcher

use crate::DbResult;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// A persisted trader analysis record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LeaderboardTraderRecord {
    pub id: Option<i64>,
    pub proxy_wallet: String,
    pub user_name: Option<String>,
    pub rank: Option<String>,
    pub pnl: Option<f64>,
    pub volume: Option<f64>,
    pub portfolio_value: Option<f64>,
    pub primary_strategy: Option<String>,
    pub primary_confidence: Option<f64>,
    pub strategies_json: Option<String>,
    pub metrics_json: Option<String>,
    pub top_positions_json: Option<String>,
    pub trade_count: Option<i64>,
    pub unique_markets: Option<i64>,
    pub win_rate: Option<f64>,
    pub avg_entry_price: Option<f64>,
    pub analyzed_at: Option<i64>,
}

/// A single trade record for the watcher
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TraderTradeRecord {
    pub id: Option<i64>,
    pub proxy_wallet: String,
    pub trade_hash: String,
    pub side: String,
    pub condition_id: Option<String>,
    pub asset: Option<String>,
    pub size: Option<f64>,
    pub price: Option<f64>,
    pub title: Option<String>,
    pub outcome: Option<String>,
    pub event_slug: Option<String>,
    pub timestamp: Option<f64>,
    pub transaction_hash: Option<String>,
    pub alerted: Option<i64>,
    pub created_at: Option<i64>,
}

/// Repository for leaderboard traders and trade watcher data
pub struct LeaderboardRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> LeaderboardRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert or update a trader analysis (upsert by proxy_wallet)
    pub async fn save_trader_analysis(&self, record: &LeaderboardTraderRecord) -> DbResult<i64> {
        let result = sqlx::query(
            r#"INSERT INTO leaderboard_traders
                (proxy_wallet, user_name, rank, pnl, volume, portfolio_value,
                 primary_strategy, primary_confidence, strategies_json, metrics_json,
                 top_positions_json, trade_count, unique_markets, win_rate, avg_entry_price,
                 analyzed_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                       strftime('%s', 'now'))
               ON CONFLICT(proxy_wallet) DO UPDATE SET
                 user_name = excluded.user_name,
                 rank = excluded.rank,
                 pnl = excluded.pnl,
                 volume = excluded.volume,
                 portfolio_value = excluded.portfolio_value,
                 primary_strategy = excluded.primary_strategy,
                 primary_confidence = excluded.primary_confidence,
                 strategies_json = excluded.strategies_json,
                 metrics_json = excluded.metrics_json,
                 top_positions_json = excluded.top_positions_json,
                 trade_count = excluded.trade_count,
                 unique_markets = excluded.unique_markets,
                 win_rate = excluded.win_rate,
                 avg_entry_price = excluded.avg_entry_price,
                 analyzed_at = strftime('%s', 'now')
            "#,
        )
        .bind(&record.proxy_wallet)
        .bind(&record.user_name)
        .bind(&record.rank)
        .bind(record.pnl)
        .bind(record.volume)
        .bind(record.portfolio_value)
        .bind(&record.primary_strategy)
        .bind(record.primary_confidence)
        .bind(&record.strategies_json)
        .bind(&record.metrics_json)
        .bind(&record.top_positions_json)
        .bind(record.trade_count)
        .bind(record.unique_markets)
        .bind(record.win_rate)
        .bind(record.avg_entry_price)
        .execute(self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get all persisted traders, ordered by PnL descending
    pub async fn get_all_traders(&self) -> DbResult<Vec<LeaderboardTraderRecord>> {
        let records = sqlx::query_as::<_, LeaderboardTraderRecord>(
            "SELECT * FROM leaderboard_traders ORDER BY pnl DESC",
        )
        .fetch_all(self.pool)
        .await?;

        Ok(records)
    }

    /// Get all watched wallet addresses (all traders in DB)
    pub async fn get_watched_wallets(&self) -> DbResult<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT proxy_wallet FROM leaderboard_traders ORDER BY pnl DESC")
                .fetch_all(self.pool)
                .await?;

        Ok(rows.into_iter().map(|(w,)| w).collect())
    }

    /// Insert trades with deduplication (INSERT OR IGNORE by trade_hash)
    /// Returns the number of newly inserted trades.
    pub async fn save_trades(&self, trades: &[TraderTradeRecord]) -> DbResult<usize> {
        let mut inserted = 0usize;
        for trade in trades {
            let result = sqlx::query(
                r#"INSERT OR IGNORE INTO trader_trades
                    (proxy_wallet, trade_hash, side, condition_id, asset, size, price,
                     title, outcome, event_slug, timestamp, transaction_hash)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                "#,
            )
            .bind(&trade.proxy_wallet)
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

    /// Get recent trades for a wallet since a given timestamp
    pub async fn get_new_trades(
        &self,
        wallet: &str,
        since_timestamp: f64,
    ) -> DbResult<Vec<TraderTradeRecord>> {
        let records = sqlx::query_as::<_, TraderTradeRecord>(
            r#"SELECT * FROM trader_trades
               WHERE proxy_wallet = ?1 AND timestamp > ?2
               ORDER BY timestamp DESC"#,
        )
        .bind(wallet)
        .bind(since_timestamp)
        .fetch_all(self.pool)
        .await?;

        Ok(records)
    }

    /// Mark trades as alerted
    pub async fn mark_alerted(&self, trade_ids: &[i64]) -> DbResult<()> {
        for id in trade_ids {
            sqlx::query("UPDATE trader_trades SET alerted = 1 WHERE id = ?1")
                .bind(id)
                .execute(self.pool)
                .await?;
        }
        Ok(())
    }

    /// Get un-alerted trades (for catch-up on restart)
    pub async fn get_unalerted_trades(&self) -> DbResult<Vec<TraderTradeRecord>> {
        let records = sqlx::query_as::<_, TraderTradeRecord>(
            "SELECT * FROM trader_trades WHERE alerted = 0 ORDER BY timestamp DESC LIMIT 50",
        )
        .fetch_all(self.pool)
        .await?;

        Ok(records)
    }
}
