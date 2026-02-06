//! Database schema definitions

/// SQL to create all tables
/// NOTE: All prices/amounts stored as TEXT to preserve rust_decimal::Decimal precision
pub const CREATE_TABLES: &str = r#"
-- Discovery backtest results (knowledge base)
CREATE TABLE IF NOT EXISTS discovery_backtests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    params_hash TEXT NOT NULL UNIQUE,
    strategy_type TEXT NOT NULL,
    strategy_name TEXT NOT NULL,
    strategy_params TEXT NOT NULL,
    symbol TEXT NOT NULL,
    days INTEGER NOT NULL,
    sizing_mode TEXT NOT NULL,
    composite_score TEXT NOT NULL DEFAULT '0',
    net_pnl TEXT NOT NULL DEFAULT '0',
    gross_pnl TEXT NOT NULL DEFAULT '0',
    total_fees TEXT NOT NULL DEFAULT '0',
    win_rate TEXT NOT NULL DEFAULT '0',
    total_trades INTEGER NOT NULL DEFAULT 0,
    sharpe_ratio TEXT NOT NULL DEFAULT '0',
    max_drawdown_pct TEXT NOT NULL DEFAULT '0',
    profit_factor TEXT NOT NULL DEFAULT '0',
    avg_trade_pnl TEXT NOT NULL DEFAULT '0',
    hit_rate TEXT,
    avg_locked_profit TEXT,
    discovery_run_id TEXT,
    phase TEXT,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- ========== INDEXES ==========

-- Discovery backtests indexes
CREATE INDEX IF NOT EXISTS idx_discovery_hash ON discovery_backtests(params_hash);
CREATE INDEX IF NOT EXISTS idx_discovery_strategy ON discovery_backtests(strategy_type, symbol);
CREATE INDEX IF NOT EXISTS idx_discovery_score ON discovery_backtests(composite_score DESC);
CREATE INDEX IF NOT EXISTS idx_discovery_run ON discovery_backtests(discovery_run_id)
"#;
