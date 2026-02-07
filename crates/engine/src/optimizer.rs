//! Automatic parameter optimization for backtesting strategies
//!
//! Runs a grid search over parameter combinations, applies Polymarket fees,
//! scores results with a composite metric, and returns the top N configurations.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, RwLock,
};
use tracing::{info, warn};

use crate::discovery::DiscoveryStrategyType;
use crate::engine::BacktestEngine;
use crate::fees::{calculate_taker_fee, PolymarketFeeConfig};
use crate::gabagool::{GabagoolBacktestConfig, GabagoolBacktestEngine, GabagoolBacktestResult};
use crate::indicators::build_signal_generator;
use crate::types::{BacktestConfig, BacktestResult, Kline};

// ============================================================================
// Types
// ============================================================================

/// Strategy type for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizeStrategy {
    Rsi,
    BollingerBands,
    Macd,
    EmaCrossover,
    Stochastic,
    AtrMeanReversion,
    Vwap,
    Obv,
    WilliamsR,
    Adx,
    Gabagool,
}

impl std::fmt::Display for OptimizeStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptimizeStrategy::Rsi => write!(f, "RSI"),
            OptimizeStrategy::BollingerBands => write!(f, "Bollinger Bands"),
            OptimizeStrategy::Macd => write!(f, "MACD"),
            OptimizeStrategy::EmaCrossover => write!(f, "EMA Crossover"),
            OptimizeStrategy::Stochastic => write!(f, "Stochastic"),
            OptimizeStrategy::AtrMeanReversion => write!(f, "ATR Mean Reversion"),
            OptimizeStrategy::Vwap => write!(f, "VWAP"),
            OptimizeStrategy::Obv => write!(f, "OBV"),
            OptimizeStrategy::WilliamsR => write!(f, "Williams %R"),
            OptimizeStrategy::Adx => write!(f, "ADX"),
            OptimizeStrategy::Gabagool => write!(f, "Gabagool"),
        }
    }
}

/// Request to start an optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizeRequest {
    pub strategy: OptimizeStrategy,
    pub symbol: String,
    pub days: u32,
    /// Number of top results to return (default 2)
    pub top_n: Option<usize>,
}

/// RSI parameter set for grid search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiParamSet {
    pub rsi_period: usize,
    pub rsi_overbought: f64,
    pub rsi_oversold: f64,
}

/// Gabagool parameter set for grid search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GabagoolParamSet {
    pub max_pair_cost: Decimal,
    pub bid_offset: Decimal,
    pub spread_multiplier: Decimal,
}

/// A scored optimization result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredResult {
    pub rank: usize,
    pub composite_score: Decimal,
    pub params: serde_json::Value,
    pub net_pnl: Decimal,
    pub gross_pnl: Decimal,
    pub total_fees: Decimal,
    pub win_rate: Decimal,
    pub sharpe_ratio: Decimal,
    pub max_drawdown_pct: Decimal,
    pub profit_factor: Decimal,
    pub total_trades: u32,
    /// Gabagool-specific: hit rate
    pub hit_rate: Option<Decimal>,
    /// Gabagool-specific: average locked profit per window
    pub avg_locked_profit: Option<Decimal>,
}

/// Optimization run status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizeStatus {
    Idle,
    Running,
    Complete,
    Error,
}

/// Shared progress tracker between API handler and background task
pub struct OptimizeProgress {
    pub status: RwLock<OptimizeStatus>,
    pub total_combinations: AtomicU32,
    pub completed: AtomicU32,
    pub cancelled: AtomicBool,
    pub results: RwLock<Vec<ScoredResult>>,
    pub error_message: RwLock<Option<String>>,
    pub strategy: RwLock<Option<OptimizeStrategy>>,
}

impl OptimizeProgress {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(OptimizeStatus::Idle),
            total_combinations: AtomicU32::new(0),
            completed: AtomicU32::new(0),
            cancelled: AtomicBool::new(false),
            results: RwLock::new(Vec::new()),
            error_message: RwLock::new(None),
            strategy: RwLock::new(None),
        }
    }

    /// Reset for a new optimization run
    pub fn reset(&self, strategy: OptimizeStrategy) {
        *self.status.write().unwrap() = OptimizeStatus::Running;
        self.total_combinations.store(0, Ordering::Relaxed);
        self.completed.store(0, Ordering::Relaxed);
        self.cancelled.store(false, Ordering::Relaxed);
        *self.results.write().unwrap() = Vec::new();
        *self.error_message.write().unwrap() = None;
        *self.strategy.write().unwrap() = Some(strategy);
    }

    /// Get progress as percentage
    pub fn progress_pct(&self) -> f32 {
        let total = self.total_combinations.load(Ordering::Relaxed);
        let done = self.completed.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            (done as f32 / total as f32) * 100.0
        }
    }

    /// Check if currently running
    pub fn is_running(&self) -> bool {
        matches!(*self.status.read().unwrap(), OptimizeStatus::Running)
    }
}

impl Default for OptimizeProgress {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Grid Generation
// ============================================================================

/// Generate RSI parameter grid (~200 combinations)
pub fn generate_rsi_grid() -> Vec<RsiParamSet> {
    let periods: &[usize] = &[5, 7, 9, 11, 14, 18, 21, 28];
    let overboughts: &[f64] = &[65.0, 70.0, 75.0, 80.0, 85.0];
    let oversolds: &[f64] = &[15.0, 20.0, 25.0, 30.0, 35.0];

    let mut grid = Vec::with_capacity(200);
    for &period in periods {
        for &ob in overboughts {
            for &os in oversolds {
                if os < ob {
                    grid.push(RsiParamSet {
                        rsi_period: period,
                        rsi_overbought: ob,
                        rsi_oversold: os,
                    });
                }
            }
        }
    }
    grid
}

/// Generate Gabagool parameter grid (~240 combinations)
pub fn generate_gabagool_grid() -> Vec<GabagoolParamSet> {
    let max_pair_costs: &[Decimal] = &[
        dec!(0.90),
        dec!(0.92),
        dec!(0.94),
        dec!(0.95),
        dec!(0.96),
        dec!(0.97),
        dec!(0.98),
        dec!(0.99),
    ];
    let bid_offsets: &[Decimal] = &[dec!(0.005), dec!(0.01), dec!(0.015), dec!(0.02), dec!(0.03)];
    let spread_multipliers: &[Decimal] = &[
        dec!(1.5),
        dec!(2.0),
        dec!(2.5),
        dec!(3.0),
        dec!(4.0),
        dec!(5.0),
    ];

    let mut grid = Vec::with_capacity(240);
    for &mpc in max_pair_costs {
        for &bo in bid_offsets {
            for &sm in spread_multipliers {
                grid.push(GabagoolParamSet {
                    max_pair_cost: mpc,
                    bid_offset: bo,
                    spread_multiplier: sm,
                });
            }
        }
    }
    grid
}

/// Generate a parameter grid of DiscoveryStrategyType for a given OptimizeStrategy
fn generate_indicator_grid(strategy: &OptimizeStrategy) -> Vec<DiscoveryStrategyType> {
    let mut grid = Vec::new();

    match strategy {
        OptimizeStrategy::BollingerBands => {
            for &period in &[8usize, 10, 15, 18, 20, 22, 25, 30] {
                for &mult in &[1.5, 1.75, 2.0, 2.25, 2.5, 3.0] {
                    grid.push(DiscoveryStrategyType::BollingerBands {
                        period,
                        multiplier: mult,
                    });
                }
            }
        }
        OptimizeStrategy::Macd => {
            for &fast in &[5usize, 8, 10, 12] {
                for &slow in &[17usize, 21, 24, 26, 30] {
                    for &signal in &[5usize, 7, 9, 12] {
                        if fast < slow {
                            grid.push(DiscoveryStrategyType::Macd { fast, slow, signal });
                        }
                    }
                }
            }
        }
        OptimizeStrategy::EmaCrossover => {
            for &fast in &[5usize, 8, 10, 12, 13, 15] {
                for &slow in &[20usize, 25, 26, 30, 40, 50] {
                    if fast < slow {
                        grid.push(DiscoveryStrategyType::EmaCrossover {
                            fast_period: fast,
                            slow_period: slow,
                        });
                    }
                }
            }
        }
        OptimizeStrategy::Stochastic => {
            for &period in &[5usize, 7, 9, 14, 18, 21] {
                for &ob in &[75.0, 80.0, 85.0, 90.0] {
                    for &os in &[10.0, 15.0, 20.0, 25.0] {
                        grid.push(DiscoveryStrategyType::Stochastic {
                            period,
                            overbought: ob,
                            oversold: os,
                        });
                    }
                }
            }
        }
        OptimizeStrategy::AtrMeanReversion => {
            for &atr in &[5usize, 7, 10, 14, 21] {
                for &sma in &[10usize, 15, 20, 30, 50] {
                    for &mult in &[1.0, 1.5, 2.0, 2.5, 3.0] {
                        grid.push(DiscoveryStrategyType::AtrMeanReversion {
                            atr_period: atr,
                            sma_period: sma,
                            multiplier: mult,
                        });
                    }
                }
            }
        }
        OptimizeStrategy::Vwap => {
            for &period in &[5usize, 10, 15, 20, 25, 30, 40, 50] {
                grid.push(DiscoveryStrategyType::Vwap { period });
            }
        }
        OptimizeStrategy::Obv => {
            for &sma_period in &[5usize, 7, 10, 14, 20, 25, 30] {
                grid.push(DiscoveryStrategyType::Obv { sma_period });
            }
        }
        OptimizeStrategy::WilliamsR => {
            for &period in &[5usize, 7, 10, 14, 18, 21, 28] {
                for &ob in &[-10.0f64, -15.0, -20.0, -25.0] {
                    for &os in &[-75.0f64, -80.0, -85.0, -90.0] {
                        grid.push(DiscoveryStrategyType::WilliamsR {
                            period,
                            overbought: ob,
                            oversold: os,
                        });
                    }
                }
            }
        }
        OptimizeStrategy::Adx => {
            for &period in &[5usize, 7, 10, 14, 18, 21] {
                for &threshold in &[15.0, 20.0, 25.0, 30.0, 35.0] {
                    grid.push(DiscoveryStrategyType::Adx {
                        period,
                        adx_threshold: threshold,
                    });
                }
            }
        }
        // RSI and Gabagool use their own dedicated grids
        _ => {}
    }

    grid
}

// ============================================================================
// Fee Calculation Helpers
// ============================================================================

/// Calculate total Polymarket fees for RSI backtest trades.
fn calculate_rsi_fees(result: &BacktestResult, fee_config: &PolymarketFeeConfig) -> Decimal {
    let poly_price = dec!(0.50);
    let mut total_fees = Decimal::ZERO;

    for trade in &result.trades {
        let fee_entry = calculate_taker_fee(trade.size, poly_price, fee_config);
        let fee_exit = calculate_taker_fee(trade.size, poly_price, fee_config);
        total_fees += fee_entry + fee_exit;
    }

    total_fees
}

/// Calculate total Polymarket fees for Gabagool backtest.
fn calculate_gabagool_fees(
    result: &GabagoolBacktestResult,
    fee_config: &PolymarketFeeConfig,
) -> Decimal {
    let size = result.config.size_per_side;
    let mut total_fees = Decimal::ZERO;

    for window in &result.windows {
        if window.traded {
            let fee_yes = calculate_taker_fee(size, window.yes_fill, fee_config);
            let fee_no = calculate_taker_fee(size, window.no_fill, fee_config);
            total_fees += fee_yes + fee_no;
        }
    }

    total_fees
}

// ============================================================================
// Scoring
// ============================================================================

/// Score an RSI backtest result. Higher is better.
fn score_rsi(result: &BacktestResult, total_fees: Decimal) -> Decimal {
    let net_pnl = result.total_pnl - total_fees;

    // Minimum 5 trades for statistical significance
    if result.total_trades < 5 {
        return dec!(-9999);
    }

    let win_rate_bonus = (result.win_rate - dec!(50)) * dec!(2);
    let sharpe_bonus = result.sharpe_ratio * dec!(100);
    let drawdown_penalty = result.max_drawdown_pct * dec!(-3);
    let pf_bonus = if result.profit_factor > Decimal::ONE {
        (result.profit_factor - Decimal::ONE) * dec!(50)
    } else {
        (result.profit_factor - Decimal::ONE) * dec!(100)
    };

    net_pnl + win_rate_bonus + sharpe_bonus + drawdown_penalty + pf_bonus
}

/// Score a generic indicator backtest result. Higher is better.
fn score_indicator(
    net_pnl: Decimal,
    win_rate: Decimal,
    sharpe_ratio: Decimal,
    max_drawdown_pct: Decimal,
    profit_factor: Decimal,
    total_trades: u32,
) -> Decimal {
    if total_trades < 5 {
        return dec!(-9999);
    }

    let win_rate_bonus = (win_rate - dec!(50)) * dec!(2);
    let sharpe_bonus = sharpe_ratio * dec!(100);
    let drawdown_penalty = max_drawdown_pct * dec!(-3);
    let pf_bonus = if profit_factor > Decimal::ONE {
        (profit_factor - Decimal::ONE) * dec!(50)
    } else {
        (profit_factor - Decimal::ONE) * dec!(100)
    };

    net_pnl + win_rate_bonus + sharpe_bonus + drawdown_penalty + pf_bonus
}

/// Score a Gabagool backtest result. Higher is better.
fn score_gabagool(result: &GabagoolBacktestResult, total_fees: Decimal) -> Decimal {
    let net_profit = result.total_locked_profit - total_fees;

    // Minimum 10 traded windows for significance
    if result.traded_windows < 10 {
        return dec!(-9999);
    }

    let hit_rate_bonus = (result.hit_rate - dec!(20)) * Decimal::ONE;
    let avg_profit_bonus = result.avg_locked_profit * dec!(1000);
    let capital_efficiency = if result.total_capital_used > Decimal::ZERO {
        (net_profit / result.total_capital_used) * dec!(10000)
    } else {
        Decimal::ZERO
    };

    net_profit + hit_rate_bonus + avg_profit_bonus + capital_efficiency
}

// ============================================================================
// Main Optimization Runner
// ============================================================================

/// Run the full parameter optimization grid search.
pub async fn run_optimization(
    request: OptimizeRequest,
    klines: Vec<Kline>,
    progress: Arc<OptimizeProgress>,
) {
    let fee_config = PolymarketFeeConfig::default();
    let top_n = request.top_n.unwrap_or(2);

    info!(
        strategy = %request.strategy,
        symbol = %request.symbol,
        klines = klines.len(),
        "Starting parameter optimization"
    );

    match request.strategy {
        OptimizeStrategy::Rsi => {
            run_rsi_optimization(&request, &klines, &fee_config, top_n, &progress).await;
        }
        OptimizeStrategy::Gabagool => {
            run_gabagool_optimization(&request, &klines, &fee_config, top_n, &progress).await;
        }
        ref s @ (OptimizeStrategy::BollingerBands
        | OptimizeStrategy::Macd
        | OptimizeStrategy::EmaCrossover
        | OptimizeStrategy::Stochastic
        | OptimizeStrategy::AtrMeanReversion
        | OptimizeStrategy::Vwap
        | OptimizeStrategy::Obv
        | OptimizeStrategy::WilliamsR
        | OptimizeStrategy::Adx) => {
            run_indicator_optimization(s, &klines, &fee_config, top_n, &progress).await;
        }
    }
}

async fn run_rsi_optimization(
    request: &OptimizeRequest,
    klines: &[Kline],
    fee_config: &PolymarketFeeConfig,
    top_n: usize,
    progress: &Arc<OptimizeProgress>,
) {
    let grid = generate_rsi_grid();
    let total = grid.len() as u32;
    progress.total_combinations.store(total, Ordering::Relaxed);

    info!(combinations = total, "RSI grid generated");

    let mut scored: Vec<ScoredResult> = Vec::with_capacity(grid.len());

    for (i, params) in grid.iter().enumerate() {
        if progress.cancelled.load(Ordering::Relaxed) {
            warn!("Optimization cancelled");
            break;
        }

        let config = BacktestConfig {
            symbol: request.symbol.clone(),
            interval: "15m".to_string(),
            start_time: None,
            end_time: None,
            initial_capital: dec!(10000),
            position_size_pct: dec!(10),
            rsi_period: params.rsi_period,
            rsi_overbought: params.rsi_overbought,
            rsi_oversold: params.rsi_oversold,
        };

        let result = BacktestEngine::run(&config, klines);
        let total_fees = calculate_rsi_fees(&result, fee_config);
        let net_pnl = result.total_pnl - total_fees;
        let composite = score_rsi(&result, total_fees);

        scored.push(ScoredResult {
            rank: 0,
            composite_score: composite,
            params: serde_json::to_value(params).unwrap_or_default(),
            net_pnl,
            gross_pnl: result.total_pnl,
            total_fees,
            win_rate: result.win_rate,
            sharpe_ratio: result.sharpe_ratio,
            max_drawdown_pct: result.max_drawdown_pct,
            profit_factor: result.profit_factor,
            total_trades: result.total_trades,
            hit_rate: None,
            avg_locked_profit: None,
        });

        progress.completed.store((i + 1) as u32, Ordering::Relaxed);

        // Yield to runtime every 10 iterations
        if i % 10 == 0 {
            tokio::task::yield_now().await;
        }
    }

    finalize_results(scored, top_n, progress);
}

/// Generic indicator optimization using SignalGenerator + discovery backtest engine
async fn run_indicator_optimization(
    strategy: &OptimizeStrategy,
    klines: &[Kline],
    fee_config: &PolymarketFeeConfig,
    top_n: usize,
    progress: &Arc<OptimizeProgress>,
) {
    let grid = generate_indicator_grid(strategy);
    let total = grid.len() as u32;
    progress.total_combinations.store(total, Ordering::Relaxed);

    info!(combinations = total, strategy = %strategy, "Indicator grid generated");

    let mut scored: Vec<ScoredResult> = Vec::with_capacity(grid.len());

    let initial_capital = dec!(10000);
    let base_position_pct = dec!(10);
    let poly_price = dec!(0.50);

    for (i, strategy_type) in grid.iter().enumerate() {
        if progress.cancelled.load(Ordering::Relaxed) {
            warn!("Optimization cancelled");
            break;
        }

        let mut generator = build_signal_generator(strategy_type);

        // Run bar-by-bar backtest using the same logic as discovery
        let hundred = dec!(100);
        let mut equity = initial_capital;
        let mut peak_equity = equity;
        let mut max_drawdown_pct = Decimal::ZERO;

        struct Pos {
            entry_price: Decimal,
            size: Decimal,
        }
        let mut position: Option<Pos> = None;
        let mut trades_pnl: Vec<Decimal> = Vec::new();
        let mut trades_pnl_pct: Vec<Decimal> = Vec::new();
        let mut winning = 0u32;
        let mut total_trade_count = 0u32;
        let mut total_fees = Decimal::ZERO;

        for kline in klines {
            let sig = generator.on_bar(kline);

            match sig.signal {
                crate::strategy::Signal::Buy => {
                    if position.is_none() {
                        let position_value = equity * base_position_pct / hundred;
                        let shares = position_value / kline.close;
                        let entry_fee = calculate_taker_fee(shares, poly_price, fee_config);
                        equity -= entry_fee;
                        total_fees += entry_fee;
                        position = Some(Pos {
                            entry_price: kline.close,
                            size: shares,
                        });
                    }
                }
                crate::strategy::Signal::Sell => {
                    if let Some(pos) = position.take() {
                        let pnl = (kline.close - pos.entry_price) * pos.size;
                        let exit_fee = calculate_taker_fee(pos.size, poly_price, fee_config);
                        total_fees += exit_fee;
                        equity += pnl - exit_fee;

                        let pnl_pct = if pos.entry_price > Decimal::ZERO {
                            (kline.close - pos.entry_price) / pos.entry_price * hundred
                        } else {
                            Decimal::ZERO
                        };

                        total_trade_count += 1;
                        if pnl > Decimal::ZERO {
                            winning += 1;
                        }
                        trades_pnl.push(pnl);
                        trades_pnl_pct.push(pnl_pct);
                    }
                }
                crate::strategy::Signal::Hold => {}
            }

            // Track drawdown
            let unrealized = position
                .as_ref()
                .map(|p| (kline.close - p.entry_price) * p.size)
                .unwrap_or(Decimal::ZERO);
            let current_equity = equity + unrealized;
            if current_equity > peak_equity {
                peak_equity = current_equity;
            }
            if peak_equity > Decimal::ZERO {
                let dd = (peak_equity - current_equity) / peak_equity * hundred;
                if dd > max_drawdown_pct {
                    max_drawdown_pct = dd;
                }
            }
        }

        // Close remaining position
        if let Some(pos) = position.take() {
            if let Some(last) = klines.last() {
                let pnl = (last.close - pos.entry_price) * pos.size;
                let exit_fee = calculate_taker_fee(pos.size, poly_price, fee_config);
                total_fees += exit_fee;
                equity += pnl - exit_fee;
                total_trade_count += 1;
                if pnl > Decimal::ZERO {
                    winning += 1;
                }
                let pnl_pct = if pos.entry_price > Decimal::ZERO {
                    (last.close - pos.entry_price) / pos.entry_price * hundred
                } else {
                    Decimal::ZERO
                };
                trades_pnl.push(pnl);
                trades_pnl_pct.push(pnl_pct);
            }
        }

        let net_pnl = equity - initial_capital;
        let win_rate = if total_trade_count > 0 {
            Decimal::from(winning) / Decimal::from(total_trade_count) * hundred
        } else {
            Decimal::ZERO
        };

        let gross_profits: Decimal = trades_pnl.iter().filter(|&&p| p > Decimal::ZERO).sum();
        let gross_losses: Decimal = trades_pnl
            .iter()
            .filter(|&&p| p < Decimal::ZERO)
            .map(|p| p.abs())
            .sum();
        let profit_factor = if gross_losses > Decimal::ZERO {
            gross_profits / gross_losses
        } else if gross_profits > Decimal::ZERO {
            dec!(999.99)
        } else {
            Decimal::ZERO
        };

        // Sharpe ratio
        let sharpe_ratio = if trades_pnl_pct.len() >= 2 {
            let returns: Vec<f64> = trades_pnl_pct
                .iter()
                .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
                .collect();
            let n = returns.len() as f64;
            let mean = returns.iter().sum::<f64>() / n;
            let variance =
                returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
            let std_dev = variance.sqrt();
            if std_dev > 1e-10 {
                Decimal::from_str_exact(&format!("{:.2}", mean / std_dev))
                    .unwrap_or(Decimal::ZERO)
            } else {
                Decimal::ZERO
            }
        } else {
            Decimal::ZERO
        };

        let composite = score_indicator(
            net_pnl,
            win_rate,
            sharpe_ratio,
            max_drawdown_pct,
            profit_factor,
            total_trade_count,
        );

        scored.push(ScoredResult {
            rank: 0,
            composite_score: composite,
            params: serde_json::to_value(strategy_type).unwrap_or_default(),
            net_pnl,
            gross_pnl: net_pnl + total_fees,
            total_fees,
            win_rate,
            sharpe_ratio,
            max_drawdown_pct,
            profit_factor,
            total_trades: total_trade_count,
            hit_rate: None,
            avg_locked_profit: None,
        });

        progress.completed.store((i + 1) as u32, Ordering::Relaxed);

        if i % 10 == 0 {
            tokio::task::yield_now().await;
        }
    }

    finalize_results(scored, top_n, progress);
}

async fn run_gabagool_optimization(
    request: &OptimizeRequest,
    klines: &[Kline],
    fee_config: &PolymarketFeeConfig,
    top_n: usize,
    progress: &Arc<OptimizeProgress>,
) {
    let grid = generate_gabagool_grid();
    let total = grid.len() as u32;
    progress.total_combinations.store(total, Ordering::Relaxed);

    info!(combinations = total, "Gabagool grid generated");

    let mut scored: Vec<ScoredResult> = Vec::with_capacity(grid.len());

    for (i, params) in grid.iter().enumerate() {
        if progress.cancelled.load(Ordering::Relaxed) {
            warn!("Optimization cancelled");
            break;
        }

        let config = GabagoolBacktestConfig {
            symbol: request.symbol.clone(),
            days: request.days,
            size_per_side: dec!(10),
            max_pair_cost: params.max_pair_cost,
            bid_offset: params.bid_offset,
            spread_multiplier: params.spread_multiplier,
        };

        let result = GabagoolBacktestEngine::run(&config, klines);
        let total_fees = calculate_gabagool_fees(&result, fee_config);
        let net_profit = result.total_locked_profit - total_fees;
        let composite = score_gabagool(&result, total_fees);

        scored.push(ScoredResult {
            rank: 0,
            composite_score: composite,
            params: serde_json::to_value(params).unwrap_or_default(),
            net_pnl: net_profit,
            gross_pnl: result.total_locked_profit,
            total_fees,
            win_rate: result.hit_rate,
            sharpe_ratio: Decimal::ZERO,
            max_drawdown_pct: Decimal::ZERO,
            profit_factor: Decimal::ZERO,
            total_trades: result.traded_windows,
            hit_rate: Some(result.hit_rate),
            avg_locked_profit: Some(result.avg_locked_profit),
        });

        progress.completed.store((i + 1) as u32, Ordering::Relaxed);

        if i % 10 == 0 {
            tokio::task::yield_now().await;
        }
    }

    finalize_results(scored, top_n, progress);
}

fn finalize_results(mut scored: Vec<ScoredResult>, top_n: usize, progress: &Arc<OptimizeProgress>) {
    // Sort by composite score descending
    scored.sort_by(|a, b| b.composite_score.cmp(&a.composite_score));

    // Assign ranks and keep top N
    for (i, s) in scored.iter_mut().enumerate() {
        s.rank = i + 1;
    }
    scored.truncate(top_n);

    if let Some(best) = scored.first() {
        info!(
            rank = 1,
            score = %best.composite_score,
            net_pnl = %best.net_pnl,
            trades = best.total_trades,
            "Best configuration found"
        );
    }

    *progress.results.write().unwrap() = scored;
    *progress.status.write().unwrap() = OptimizeStatus::Complete;

    info!("Optimization complete");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_rsi_grid_generation() {
        let grid = generate_rsi_grid();
        assert!(grid.len() > 150);
        assert!(grid.len() <= 200);

        for p in &grid {
            assert!(
                p.rsi_oversold < p.rsi_overbought,
                "oversold {} must be < overbought {}",
                p.rsi_oversold,
                p.rsi_overbought
            );
        }
    }

    #[test]
    fn test_gabagool_grid_generation() {
        let grid = generate_gabagool_grid();
        assert_eq!(grid.len(), 8 * 5 * 6); // 240
    }

    #[test]
    fn test_indicator_grids_are_nonempty() {
        assert!(!generate_indicator_grid(&OptimizeStrategy::BollingerBands).is_empty());
        assert!(!generate_indicator_grid(&OptimizeStrategy::Macd).is_empty());
        assert!(!generate_indicator_grid(&OptimizeStrategy::EmaCrossover).is_empty());
        assert!(!generate_indicator_grid(&OptimizeStrategy::Stochastic).is_empty());
        assert!(!generate_indicator_grid(&OptimizeStrategy::AtrMeanReversion).is_empty());
        assert!(!generate_indicator_grid(&OptimizeStrategy::Vwap).is_empty());
        assert!(!generate_indicator_grid(&OptimizeStrategy::Obv).is_empty());
        assert!(!generate_indicator_grid(&OptimizeStrategy::WilliamsR).is_empty());
        assert!(!generate_indicator_grid(&OptimizeStrategy::Adx).is_empty());
    }

    #[test]
    fn test_rsi_scoring_penalizes_few_trades() {
        let result = BacktestResult {
            symbol: "BTCUSDT".to_string(),
            interval: "15m".to_string(),
            start_time: 0,
            end_time: 0,
            initial_capital: dec!(10000),
            final_equity: dec!(10500),
            total_pnl: dec!(500),
            total_pnl_pct: dec!(5),
            total_trades: 3, // Below minimum of 5
            winning_trades: 3,
            losing_trades: 0,
            win_rate: dec!(100),
            max_drawdown: dec!(0),
            max_drawdown_pct: dec!(0),
            sharpe_ratio: dec!(2),
            profit_factor: dec!(999.99),
            trades: vec![],
            equity_curve: vec![],
            klines: vec![],
        };

        let score = score_rsi(&result, Decimal::ZERO);
        assert_eq!(score, dec!(-9999));
    }

    #[test]
    fn test_rsi_scoring_rewards_good_result() {
        let result = BacktestResult {
            symbol: "BTCUSDT".to_string(),
            interval: "15m".to_string(),
            start_time: 0,
            end_time: 0,
            initial_capital: dec!(10000),
            final_equity: dec!(10800),
            total_pnl: dec!(800),
            total_pnl_pct: dec!(8),
            total_trades: 20,
            winning_trades: 13,
            losing_trades: 7,
            win_rate: dec!(65),
            max_drawdown: dec!(200),
            max_drawdown_pct: dec!(2),
            sharpe_ratio: dec!(1.5),
            profit_factor: dec!(2.0),
            trades: vec![],
            equity_curve: vec![],
            klines: vec![],
        };

        let score = score_rsi(&result, dec!(20));
        assert!(score > dec!(900));
    }

    #[test]
    fn test_gabagool_scoring_penalizes_few_windows() {
        let config = GabagoolBacktestConfig::default();
        let result = GabagoolBacktestResult {
            config,
            start_time: 0,
            end_time: 0,
            total_windows: 100,
            traded_windows: 5, // Below minimum of 10
            skipped_windows: 95,
            hit_rate: dec!(5),
            total_capital_used: dec!(100),
            total_locked_profit: dec!(10),
            avg_pair_cost: dec!(0.95),
            avg_locked_profit: dec!(2),
            best_pair_cost: dec!(0.90),
            worst_pair_cost: dec!(0.98),
            avg_spread: dec!(0.05),
            profit_curve: vec![],
            windows: vec![],
        };

        let score = score_gabagool(&result, Decimal::ZERO);
        assert_eq!(score, dec!(-9999));
    }

    #[test]
    fn test_indicator_scoring_penalizes_few_trades() {
        let score = score_indicator(
            dec!(500),
            dec!(80),
            dec!(2),
            dec!(5),
            dec!(3),
            3, // Below minimum
        );
        assert_eq!(score, dec!(-9999));
    }

    #[test]
    fn test_indicator_scoring_rewards_good_result() {
        let score = score_indicator(
            dec!(800),
            dec!(65),
            dec!(1.5),
            dec!(2),
            dec!(2.0),
            20,
        );
        assert!(score > dec!(900));
    }
}
