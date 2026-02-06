//! Types for the backtesting engine

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A single candlestick (OHLCV)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kline {
    pub open_time: i64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub close_time: i64,
}

/// Configuration for a backtest run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    pub symbol: String,
    pub interval: String,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub initial_capital: Decimal,
    /// Position size as percentage of capital (e.g., 10.0 = 10%)
    pub position_size_pct: Decimal,
    /// RSI period (default: 14)
    pub rsi_period: usize,
    /// RSI overbought threshold (default: 70)
    pub rsi_overbought: f64,
    /// RSI oversold threshold (default: 30)
    pub rsi_oversold: f64,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            interval: "15m".to_string(),
            start_time: None,
            end_time: None,
            initial_capital: Decimal::from(10000),
            position_size_pct: Decimal::from(10),
            rsi_period: 14,
            rsi_overbought: 70.0,
            rsi_oversold: 30.0,
        }
    }
}

/// Side of a trade
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TradeSide {
    Buy,
    Sell,
}

/// A single trade executed during backtest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestTrade {
    pub entry_time: i64,
    pub exit_time: i64,
    pub side: TradeSide,
    pub entry_price: Decimal,
    pub exit_price: Decimal,
    pub size: Decimal,
    pub pnl: Decimal,
    pub pnl_pct: Decimal,
}

/// A point on the equity curve
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPoint {
    pub time: i64,
    pub equity: Decimal,
}

/// Result of a backtest run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub symbol: String,
    pub interval: String,
    pub start_time: i64,
    pub end_time: i64,
    pub initial_capital: Decimal,
    pub final_equity: Decimal,
    pub total_pnl: Decimal,
    pub total_pnl_pct: Decimal,
    pub total_trades: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub win_rate: Decimal,
    pub max_drawdown: Decimal,
    pub max_drawdown_pct: Decimal,
    pub sharpe_ratio: Decimal,
    pub profit_factor: Decimal,
    pub trades: Vec<BacktestTrade>,
    pub equity_curve: Vec<EquityPoint>,
    pub klines: Vec<Kline>,
}
