//! Discovery backtests repository — knowledge base for strategy discovery

use crate::DbResult;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// A single discovery backtest record stored in the knowledge base
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DiscoveryBacktestRecord {
    pub id: Option<i64>,
    pub params_hash: String,
    pub strategy_type: String,
    pub strategy_name: String,
    pub strategy_params: String,
    pub symbol: String,
    pub days: i64,
    pub sizing_mode: String,
    pub composite_score: String,
    pub net_pnl: String,
    pub gross_pnl: String,
    pub total_fees: String,
    pub win_rate: String,
    pub total_trades: i64,
    pub sharpe_ratio: String,
    pub max_drawdown_pct: String,
    pub profit_factor: String,
    pub avg_trade_pnl: String,
    pub hit_rate: Option<String>,
    pub avg_locked_profit: Option<String>,
    pub discovery_run_id: Option<String>,
    pub phase: Option<String>,
    // Advanced metrics (added via migration)
    pub sortino_ratio: Option<String>,
    pub max_consecutive_losses: Option<i64>,
    pub avg_win_pnl: Option<String>,
    pub avg_loss_pnl: Option<String>,
    pub total_volume: Option<String>,
    pub annualized_return_pct: Option<String>,
    pub annualized_sharpe: Option<String>,
    pub strategy_confidence: Option<String>,
}

/// Aggregated stats for the knowledge base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBaseStats {
    pub total_backtests: i64,
    pub unique_strategies: i64,
    pub unique_symbols: i64,
    pub best_win_rate: String,
    pub best_net_pnl: String,
    pub best_strategy_name: String,
    pub total_discovery_runs: i64,
}

/// Repository for discovery backtest results
pub struct DiscoveryRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> DiscoveryRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Save a backtest result (INSERT OR IGNORE — skips if params_hash already exists)
    pub async fn save(&self, record: &DiscoveryBacktestRecord) -> DbResult<i64> {
        let result = sqlx::query(
            r#"
            INSERT OR IGNORE INTO discovery_backtests (
                params_hash, strategy_type, strategy_name, strategy_params,
                symbol, days, sizing_mode,
                composite_score, net_pnl, gross_pnl, total_fees,
                win_rate, total_trades, sharpe_ratio, max_drawdown_pct,
                profit_factor, avg_trade_pnl,
                hit_rate, avg_locked_profit,
                discovery_run_id, phase,
                sortino_ratio, max_consecutive_losses, avg_win_pnl, avg_loss_pnl,
                total_volume, annualized_return_pct, annualized_sharpe, strategy_confidence
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&record.params_hash)
        .bind(&record.strategy_type)
        .bind(&record.strategy_name)
        .bind(&record.strategy_params)
        .bind(&record.symbol)
        .bind(record.days)
        .bind(&record.sizing_mode)
        .bind(&record.composite_score)
        .bind(&record.net_pnl)
        .bind(&record.gross_pnl)
        .bind(&record.total_fees)
        .bind(&record.win_rate)
        .bind(record.total_trades)
        .bind(&record.sharpe_ratio)
        .bind(&record.max_drawdown_pct)
        .bind(&record.profit_factor)
        .bind(&record.avg_trade_pnl)
        .bind(&record.hit_rate)
        .bind(&record.avg_locked_profit)
        .bind(&record.discovery_run_id)
        .bind(&record.phase)
        .bind(&record.sortino_ratio)
        .bind(record.max_consecutive_losses)
        .bind(&record.avg_win_pnl)
        .bind(&record.avg_loss_pnl)
        .bind(&record.total_volume)
        .bind(&record.annualized_return_pct)
        .bind(&record.annualized_sharpe)
        .bind(&record.strategy_confidence)
        .execute(self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Check if a backtest with this params_hash already exists
    pub async fn exists_by_hash(&self, hash: &str) -> DbResult<bool> {
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM discovery_backtests WHERE params_hash = ?")
                .bind(hash)
                .fetch_one(self.pool)
                .await?;

        Ok(row.0 > 0)
    }

    /// Get a backtest record by its params_hash
    pub async fn get_by_hash(&self, hash: &str) -> DbResult<Option<DiscoveryBacktestRecord>> {
        let record = sqlx::query_as::<_, DiscoveryBacktestRecord>(
            r#"
            SELECT id, params_hash, strategy_type, strategy_name, strategy_params,
                   symbol, days, sizing_mode,
                   composite_score, net_pnl, gross_pnl, total_fees,
                   win_rate, total_trades, sharpe_ratio, max_drawdown_pct,
                   profit_factor, avg_trade_pnl,
                   hit_rate, avg_locked_profit,
                   discovery_run_id, phase,
                   sortino_ratio, max_consecutive_losses, avg_win_pnl, avg_loss_pnl,
                   total_volume, annualized_return_pct, annualized_sharpe, strategy_confidence
            FROM discovery_backtests
            WHERE params_hash = ?
            "#,
        )
        .bind(hash)
        .fetch_optional(self.pool)
        .await?;

        Ok(record)
    }

    /// Get top results ordered by composite score, with optional filters
    pub async fn get_top_results(
        &self,
        limit: i64,
        strategy_type: Option<&str>,
        symbol: Option<&str>,
    ) -> DbResult<Vec<DiscoveryBacktestRecord>> {
        let mut sql = String::from(
            r#"
            SELECT id, params_hash, strategy_type, strategy_name, strategy_params,
                   symbol, days, sizing_mode,
                   composite_score, net_pnl, gross_pnl, total_fees,
                   win_rate, total_trades, sharpe_ratio, max_drawdown_pct,
                   profit_factor, avg_trade_pnl,
                   hit_rate, avg_locked_profit,
                   discovery_run_id, phase,
                   sortino_ratio, max_consecutive_losses, avg_win_pnl, avg_loss_pnl,
                   total_volume, annualized_return_pct, annualized_sharpe, strategy_confidence
            FROM discovery_backtests
            WHERE 1=1
            "#,
        );

        let mut binds: Vec<String> = Vec::new();

        if let Some(st) = strategy_type {
            sql.push_str(" AND strategy_type = ?");
            binds.push(st.to_string());
        }
        if let Some(sym) = symbol {
            sql.push_str(" AND symbol = ?");
            binds.push(sym.to_string());
        }

        sql.push_str(" ORDER BY CAST(composite_score AS REAL) DESC LIMIT ?");

        // Build query dynamically
        let mut query = sqlx::query_as::<_, DiscoveryBacktestRecord>(&sql);
        for b in &binds {
            query = query.bind(b);
        }
        query = query.bind(limit);

        let records = query.fetch_all(self.pool).await?;
        Ok(records)
    }

    /// Get paginated results with optional filters
    pub async fn get_all_paginated(
        &self,
        limit: i64,
        offset: i64,
        strategy_type: Option<&str>,
        symbol: Option<&str>,
        min_win_rate: Option<f64>,
        sort_by: Option<&str>,
    ) -> DbResult<(Vec<DiscoveryBacktestRecord>, i64)> {
        let mut where_clauses = vec!["1=1".to_string()];
        let mut binds: Vec<String> = Vec::new();

        if let Some(st) = strategy_type {
            where_clauses.push("strategy_type = ?".to_string());
            binds.push(st.to_string());
        }
        if let Some(sym) = symbol {
            where_clauses.push("symbol = ?".to_string());
            binds.push(sym.to_string());
        }
        if let Some(mwr) = min_win_rate {
            where_clauses.push("CAST(win_rate AS REAL) >= ?".to_string());
            binds.push(format!("{mwr}"));
        }

        let where_sql = where_clauses.join(" AND ");

        // Count total
        let count_sql = format!("SELECT COUNT(*) FROM discovery_backtests WHERE {where_sql}");
        let mut count_query = sqlx::query_as::<_, (i64,)>(&count_sql);
        for b in &binds {
            count_query = count_query.bind(b);
        }
        let (total,) = count_query.fetch_one(self.pool).await?;

        // Sort column
        let order_col = match sort_by {
            Some("win_rate") => "CAST(win_rate AS REAL)",
            Some("net_pnl") => "CAST(net_pnl AS REAL)",
            Some("sharpe_ratio") => "CAST(sharpe_ratio AS REAL)",
            Some("total_trades") => "total_trades",
            Some("max_drawdown_pct") => "CAST(max_drawdown_pct AS REAL)",
            Some("created_at") => "created_at",
            Some("strategy_confidence") => "CAST(strategy_confidence AS REAL)",
            Some("annualized_return_pct") => "CAST(annualized_return_pct AS REAL)",
            Some("sortino_ratio") => "CAST(sortino_ratio AS REAL)",
            _ => "CAST(composite_score AS REAL)",
        };

        let data_sql = format!(
            r#"
            SELECT id, params_hash, strategy_type, strategy_name, strategy_params,
                   symbol, days, sizing_mode,
                   composite_score, net_pnl, gross_pnl, total_fees,
                   win_rate, total_trades, sharpe_ratio, max_drawdown_pct,
                   profit_factor, avg_trade_pnl,
                   hit_rate, avg_locked_profit,
                   discovery_run_id, phase,
                   sortino_ratio, max_consecutive_losses, avg_win_pnl, avg_loss_pnl,
                   total_volume, annualized_return_pct, annualized_sharpe, strategy_confidence
            FROM discovery_backtests
            WHERE {where_sql}
            ORDER BY {order_col} DESC
            LIMIT ? OFFSET ?
            "#
        );

        let mut data_query = sqlx::query_as::<_, DiscoveryBacktestRecord>(&data_sql);
        for b in &binds {
            data_query = data_query.bind(b);
        }
        data_query = data_query.bind(limit).bind(offset);

        let records = data_query.fetch_all(self.pool).await?;
        Ok((records, total))
    }

    /// Get top unique strategies (one per strategy_name), deduplicated by sort_by column
    pub async fn get_top_unique_strategies(
        &self,
        limit: i64,
        sort_by: Option<&str>,
    ) -> DbResult<Vec<DiscoveryBacktestRecord>> {
        let order_col = match sort_by {
            Some("net_pnl") => "CAST(net_pnl AS REAL)",
            Some("composite_score") => "CAST(composite_score AS REAL)",
            _ => "CAST(win_rate AS REAL)",
        };

        let sql = format!(
            r#"
            WITH best_ids AS (
              SELECT id,
                ROW_NUMBER() OVER (PARTITION BY strategy_name ORDER BY {order_col} DESC) as rn
              FROM discovery_backtests
              WHERE total_trades >= 5
            )
            SELECT d.id, d.params_hash, d.strategy_type, d.strategy_name, d.strategy_params,
                   d.symbol, d.days, d.sizing_mode,
                   d.composite_score, d.net_pnl, d.gross_pnl, d.total_fees,
                   d.win_rate, d.total_trades, d.sharpe_ratio, d.max_drawdown_pct,
                   d.profit_factor, d.avg_trade_pnl,
                   d.hit_rate, d.avg_locked_profit,
                   d.discovery_run_id, d.phase,
                   d.sortino_ratio, d.max_consecutive_losses, d.avg_win_pnl, d.avg_loss_pnl,
                   d.total_volume, d.annualized_return_pct, d.annualized_sharpe, d.strategy_confidence
            FROM best_ids b
            JOIN discovery_backtests d ON d.id = b.id
            WHERE b.rn = 1
            ORDER BY {order_col} DESC
            LIMIT ?
            "#
        );

        let records = sqlx::query_as::<_, DiscoveryBacktestRecord>(&sql)
            .bind(limit)
            .fetch_all(self.pool)
            .await?;

        Ok(records)
    }

    /// Get aggregated knowledge base stats
    pub async fn get_stats(&self) -> DbResult<KnowledgeBaseStats> {
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM discovery_backtests")
            .fetch_one(self.pool)
            .await?;

        let unique_strategies: (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT strategy_type) FROM discovery_backtests")
                .fetch_one(self.pool)
                .await?;

        let unique_symbols: (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT symbol) FROM discovery_backtests")
                .fetch_one(self.pool)
                .await?;

        let total_runs: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT discovery_run_id) FROM discovery_backtests WHERE discovery_run_id IS NOT NULL",
        )
        .fetch_one(self.pool)
        .await?;

        // Best win rate (from records with at least 5 trades)
        let best_wr: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT win_rate, strategy_name
            FROM discovery_backtests
            WHERE total_trades >= 5
            ORDER BY CAST(win_rate AS REAL) DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(self.pool)
        .await?;

        // Best net PnL
        let best_pnl: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT net_pnl, strategy_name
            FROM discovery_backtests
            WHERE total_trades >= 5
            ORDER BY CAST(net_pnl AS REAL) DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(self.pool)
        .await?;

        let (best_win_rate, best_strategy_name) =
            best_wr.unwrap_or_else(|| ("0".to_string(), "N/A".to_string()));

        let best_net_pnl = best_pnl
            .map(|(pnl, _)| pnl)
            .unwrap_or_else(|| "0".to_string());

        Ok(KnowledgeBaseStats {
            total_backtests: total.0,
            unique_strategies: unique_strategies.0,
            unique_symbols: unique_symbols.0,
            best_win_rate,
            best_net_pnl,
            best_strategy_name,
            total_discovery_runs: total_runs.0,
        })
    }
}
