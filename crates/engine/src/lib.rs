//! Poly Discover Engine — backtesting, discovery, and optimization
//!
//! Self-contained crate extracted from poly_bot's trading-engine.
//! Provides:
//! - RSI and Gabagool backtesting engines
//! - 14-strategy Discovery Agent with 2-phase scanning
//! - Automatic parameter optimizer (grid search)
//! - Binance public API client for market data

pub mod api;
pub mod discovery;
pub mod engine;
pub mod fees;
pub mod gabagool;
pub mod indicators;
pub mod leaderboard;
pub mod optimizer;
pub mod strategy;
pub mod types;
pub mod watcher;
pub mod web_strategies;

// Re-exports for convenience
pub use api::BinanceClient;
pub use api::PolymarketDataClient;
pub use leaderboard::{analyze_leaderboard, LeaderboardProgress, LeaderboardStatus, TraderAnalysis};
pub use watcher::{run_trade_watcher, TradeAlert, WatcherProgress, WatcherStatus};
pub use discovery::{
    run_continuous_discovery, run_discovery, DiscoveryProgress, DiscoveryRequest, DiscoveryResult,
    DiscoveryStatus, DiscoveryStrategyType, SizingMode,
};
pub use engine::BacktestEngine;
pub use fees::{calculate_taker_fee, PolymarketFeeConfig};
pub use gabagool::{
    GabagoolBacktestConfig, GabagoolBacktestEngine, GabagoolBacktestResult, GabagoolWindowResult,
};
pub use indicators::{build_signal_generator, SignalGenerator, SignalWithConfidence};
pub use optimizer::{
    run_optimization, OptimizeProgress, OptimizeRequest, OptimizeStatus, OptimizeStrategy,
    ScoredResult,
};
pub use strategy::{RsiStrategy, Signal};
pub use types::*;
pub use web_strategies::{get_catalog, WebStrategyCatalogEntry, WebStrategyId, WebStrategyParams};
