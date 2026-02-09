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
CREATE INDEX IF NOT EXISTS idx_discovery_run ON discovery_backtests(discovery_run_id);
CREATE INDEX IF NOT EXISTS idx_discovery_name_trades ON discovery_backtests(strategy_name, total_trades);

-- Expression indexes for fast CAST-based sorts (critical with large datasets)
CREATE INDEX IF NOT EXISTS idx_disc_composite_real ON discovery_backtests(CAST(composite_score AS REAL) DESC);
CREATE INDEX IF NOT EXISTS idx_disc_win_rate_real ON discovery_backtests(CAST(win_rate AS REAL) DESC);
CREATE INDEX IF NOT EXISTS idx_disc_net_pnl_real ON discovery_backtests(CAST(net_pnl AS REAL) DESC);
-- Covering index for top-strategies CTE: PARTITION BY strategy_name ORDER BY win_rate
CREATE INDEX IF NOT EXISTS idx_disc_name_winrate ON discovery_backtests(strategy_name, CAST(win_rate AS REAL) DESC) WHERE total_trades >= 5;
CREATE INDEX IF NOT EXISTS idx_disc_name_pnl ON discovery_backtests(strategy_name, CAST(net_pnl AS REAL) DESC) WHERE total_trades >= 5;
CREATE INDEX IF NOT EXISTS idx_disc_name_score ON discovery_backtests(strategy_name, CAST(composite_score AS REAL) DESC) WHERE total_trades >= 5;

-- Leaderboard traders (persisted analysis results)
CREATE TABLE IF NOT EXISTS leaderboard_traders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    proxy_wallet TEXT NOT NULL,
    user_name TEXT,
    rank TEXT,
    pnl REAL DEFAULT 0,
    volume REAL DEFAULT 0,
    portfolio_value REAL DEFAULT 0,
    primary_strategy TEXT,
    primary_confidence REAL DEFAULT 0,
    strategies_json TEXT,
    metrics_json TEXT,
    top_positions_json TEXT,
    trade_count INTEGER DEFAULT 0,
    unique_markets INTEGER DEFAULT 0,
    win_rate REAL DEFAULT 0,
    avg_entry_price REAL DEFAULT 0,
    analyzed_at INTEGER DEFAULT (strftime('%s', 'now')),
    UNIQUE(proxy_wallet)
);

-- Trader trades (history for watcher)
CREATE TABLE IF NOT EXISTS trader_trades (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    proxy_wallet TEXT NOT NULL,
    trade_hash TEXT NOT NULL UNIQUE,
    side TEXT NOT NULL,
    condition_id TEXT,
    asset TEXT,
    size REAL,
    price REAL,
    title TEXT,
    outcome TEXT,
    event_slug TEXT,
    timestamp REAL,
    transaction_hash TEXT,
    alerted INTEGER DEFAULT 0,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_trader_trades_wallet ON trader_trades(proxy_wallet, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_trader_trades_hash ON trader_trades(trade_hash)
"#;

/// SQL migrations to add new columns (idempotent — ignores "duplicate column" errors)
pub const MIGRATIONS: &[&str] = &[
    "ALTER TABLE discovery_backtests ADD COLUMN sortino_ratio TEXT DEFAULT '0'",
    "ALTER TABLE discovery_backtests ADD COLUMN max_consecutive_losses INTEGER DEFAULT 0",
    "ALTER TABLE discovery_backtests ADD COLUMN avg_win_pnl TEXT DEFAULT '0'",
    "ALTER TABLE discovery_backtests ADD COLUMN avg_loss_pnl TEXT DEFAULT '0'",
    "ALTER TABLE discovery_backtests ADD COLUMN total_volume TEXT DEFAULT '0'",
    "ALTER TABLE discovery_backtests ADD COLUMN annualized_return_pct TEXT DEFAULT '0'",
    "ALTER TABLE discovery_backtests ADD COLUMN annualized_sharpe TEXT DEFAULT '0'",
    "ALTER TABLE discovery_backtests ADD COLUMN strategy_confidence TEXT DEFAULT '0'",
];
