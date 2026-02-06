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

use crate::engine::BacktestEngine;
use crate::fees::{calculate_taker_fee, PolymarketFeeConfig};
use crate::gabagool::{GabagoolBacktestConfig, GabagoolBacktestEngine, GabagoolBacktestResult};
use crate::types::{BacktestConfig, BacktestResult, Kline};

// ============================================================================
// Types
// ============================================================================

/// Strategy type for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizeStrategy {
    Rsi,
    Gabagool,
}

impl std::fmt::Display for OptimizeStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptimizeStrategy::Rsi => write!(f, "RSI"),
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
}
