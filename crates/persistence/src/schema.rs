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
CREATE INDEX IF NOT EXISTS idx_trader_trades_hash ON trader_trades(trade_hash);

-- Profile analyses (user profile analysis results)
CREATE TABLE IF NOT EXISTS profile_analyses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet TEXT NOT NULL,
    username TEXT,
    portfolio_value REAL DEFAULT 0,
    total_pnl REAL DEFAULT 0,
    total_volume REAL DEFAULT 0,
    total_trades INTEGER DEFAULT 0,
    unique_markets INTEGER DEFAULT 0,
    win_rate REAL DEFAULT 0,
    primary_strategy TEXT,
    strategy_confidence REAL DEFAULT 0,
    open_positions_json TEXT,
    closed_positions_json TEXT,
    markets_json TEXT,
    category_breakdown_json TEXT,
    activity_timeline_json TEXT,
    strategy_signals_json TEXT,
    avg_hold_duration REAL DEFAULT 0,
    best_trade_pnl REAL DEFAULT 0,
    worst_trade_pnl REAL DEFAULT 0,
    max_drawdown REAL DEFAULT 0,
    active_days INTEGER DEFAULT 0,
    avg_position_size REAL DEFAULT 0,
    analyzed_at INTEGER DEFAULT (strftime('%s', 'now')),
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(wallet)
);
CREATE INDEX IF NOT EXISTS idx_profile_wallet ON profile_analyses(wallet);

-- Profile trades (all trades for analyzed profiles)
CREATE TABLE IF NOT EXISTS profile_trades (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet TEXT NOT NULL,
    trade_hash TEXT NOT NULL UNIQUE,
    side TEXT NOT NULL,
    condition_id TEXT NOT NULL,
    asset TEXT,
    size REAL NOT NULL,
    price REAL NOT NULL,
    title TEXT,
    outcome TEXT,
    event_slug TEXT,
    timestamp REAL NOT NULL,
    transaction_hash TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_profile_trades_wallet ON profile_trades(wallet, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_profile_trades_market ON profile_trades(wallet, condition_id);

-- ========== ORDERBOOK BACKTEST TABLES ==========

-- BTC 15-min markets discovered from Polymarket (permanent)
CREATE TABLE IF NOT EXISTS ob_markets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    condition_id TEXT NOT NULL UNIQUE,
    question TEXT,
    slug TEXT,
    token_id_up TEXT,
    token_id_down TEXT,
    start_time INTEGER NOT NULL,
    end_time INTEGER NOT NULL,
    outcome TEXT,
    outcome_price_up REAL,
    outcome_price_down REAL,
    volume REAL DEFAULT 0,
    data_fetched INTEGER DEFAULT 0,
    data_points_count INTEGER DEFAULT 0,
    created_at INTEGER DEFAULT (strftime('%s','now'))
);

-- Market price/trade data (purgeable after feature extraction)
CREATE TABLE IF NOT EXISTS ob_market_prices (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    market_id INTEGER NOT NULL,
    timestamp_ms INTEGER NOT NULL,
    elapsed_seconds REAL NOT NULL,
    price REAL NOT NULL,
    side TEXT,
    size REAL,
    UNIQUE(market_id, timestamp_ms)
);
CREATE INDEX IF NOT EXISTS idx_ob_prices_market ON ob_market_prices(market_id, elapsed_seconds);

-- Extracted features per market per time window (permanent)
CREATE TABLE IF NOT EXISTS ob_market_features (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    market_id INTEGER NOT NULL,
    time_window INTEGER NOT NULL,
    last_price REAL,
    vwap REAL,
    price_change REAL,
    price_volatility REAL,
    momentum REAL,
    max_price REAL,
    min_price REAL,
    price_range REAL,
    data_points INTEGER,
    buy_volume REAL,
    sell_volume REAL,
    volume_imbalance REAL,
    trade_count INTEGER,
    avg_trade_size REAL,
    large_trade_ratio REAL,
    avg_spread REAL,
    depth_imbalance REAL,
    avg_bid_depth REAL,
    avg_ask_depth REAL,
    outcome_is_up INTEGER,
    UNIQUE(market_id, time_window)
);

-- Live orderbook snapshots (retention 30 days)
CREATE TABLE IF NOT EXISTS ob_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    condition_id TEXT NOT NULL,
    token_id TEXT NOT NULL,
    timestamp_ms INTEGER NOT NULL,
    elapsed_seconds REAL NOT NULL,
    best_bid REAL,
    best_ask REAL,
    spread REAL,
    mid_price REAL,
    bid_depth_total REAL,
    ask_depth_total REAL,
    depth_imbalance REAL,
    bid_levels INTEGER,
    ask_levels INTEGER,
    created_at INTEGER DEFAULT (strftime('%s','now'))
);
CREATE INDEX IF NOT EXISTS idx_ob_snap_cid ON ob_snapshots(condition_id, timestamp_ms);
CREATE INDEX IF NOT EXISTS idx_ob_snap_created ON ob_snapshots(created_at);

-- Detected patterns from statistical analysis (permanent)
CREATE TABLE IF NOT EXISTS ob_patterns (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern_name TEXT NOT NULL,
    pattern_type TEXT NOT NULL,
    time_window INTEGER NOT NULL,
    direction TEXT NOT NULL,
    features_used TEXT NOT NULL,
    threshold_json TEXT NOT NULL,
    accuracy REAL NOT NULL,
    precision_pct REAL,
    recall_pct REAL,
    f1_score REAL,
    sample_size INTEGER NOT NULL,
    up_count INTEGER NOT NULL,
    down_count INTEGER NOT NULL,
    confidence_95_low REAL,
    confidence_95_high REAL,
    first_half_accuracy REAL,
    second_half_accuracy REAL,
    stability_score REAL,
    analysis_run_id TEXT,
    created_at INTEGER DEFAULT (strftime('%s','now'))
);

-- Backtest process state (key-value store for incremental resume)
CREATE TABLE IF NOT EXISTS ob_backtest_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER DEFAULT (strftime('%s','now'))
)
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
