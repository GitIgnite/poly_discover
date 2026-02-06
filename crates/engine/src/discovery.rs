//! Discovery Agent — autonomous strategy discovery via exhaustive backtesting
//!
//! Tests ~2000 parameter combinations across 14 strategy types × N symbols,
//! in 2 phases: Phase 1 broad scan → Phase 2 refinement of top results.
//! Uses Polymarket fees and composite scoring to rank results.

use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, RwLock,
};

use chrono::Utc;
use persistence::repository::discovery::{DiscoveryBacktestRecord, DiscoveryRepository};
use persistence::SqlitePool;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::api::BinanceClient;
use crate::fees::{calculate_taker_fee, PolymarketFeeConfig};
use crate::gabagool::{GabagoolBacktestConfig, GabagoolBacktestEngine};
use crate::indicators::{build_signal_generator, SignalGenerator};
use crate::types::{BacktestTrade, Kline, TradeSide};

// ============================================================================
// Types
// ============================================================================

/// All strategy types the discovery agent can explore
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DiscoveryStrategyType {
    // === Singles (6) ===
    Rsi {
        period: usize,
        overbought: f64,
        oversold: f64,
    },
    BollingerBands {
        period: usize,
        multiplier: f64,
    },
    Macd {
        fast: usize,
        slow: usize,
        signal: usize,
    },
    EmaCrossover {
        fast_period: usize,
        slow_period: usize,
    },
    Stochastic {
        period: usize,
        overbought: f64,
        oversold: f64,
    },
    AtrMeanReversion {
        atr_period: usize,
        sma_period: usize,
        multiplier: f64,
    },
    // === Combos (7) ===
    RsiBollinger {
        rsi_period: usize,
        rsi_ob: f64,
        rsi_os: f64,
        bb_period: usize,
        bb_mult: f64,
    },
    MacdRsi {
        macd_fast: usize,
        macd_slow: usize,
        macd_signal: usize,
        rsi_period: usize,
        rsi_ob: f64,
        rsi_os: f64,
    },
    EmaRsi {
        ema_fast: usize,
        ema_slow: usize,
        rsi_period: usize,
        rsi_ob: f64,
        rsi_os: f64,
    },
    StochRsi {
        stoch_period: usize,
        stoch_ob: f64,
        stoch_os: f64,
        rsi_period: usize,
        rsi_ob: f64,
        rsi_os: f64,
    },
    MacdBollinger {
        macd_fast: usize,
        macd_slow: usize,
        macd_signal: usize,
        bb_period: usize,
        bb_mult: f64,
    },
    TripleRsiMacdBb {
        rsi_period: usize,
        rsi_ob: f64,
        rsi_os: f64,
        macd_fast: usize,
        macd_slow: usize,
        macd_signal: usize,
        bb_period: usize,
        bb_mult: f64,
    },
    TripleEmaRsiStoch {
        ema_fast: usize,
        ema_slow: usize,
        rsi_period: usize,
        rsi_ob: f64,
        rsi_os: f64,
        stoch_period: usize,
        stoch_ob: f64,
        stoch_os: f64,
    },
    // === Arbitrage (1) ===
    Gabagool {
        max_pair_cost: Decimal,
        bid_offset: Decimal,
        spread_multiplier: Decimal,
    },
}

impl DiscoveryStrategyType {
    pub fn name(&self) -> &str {
        match self {
            Self::Rsi { .. } => "RSI",
            Self::BollingerBands { .. } => "Bollinger Bands",
            Self::Macd { .. } => "MACD",
            Self::EmaCrossover { .. } => "EMA Crossover",
            Self::Stochastic { .. } => "Stochastic",
            Self::AtrMeanReversion { .. } => "ATR Mean Reversion",
            Self::RsiBollinger { .. } => "RSI+Bollinger",
            Self::MacdRsi { .. } => "MACD+RSI",
            Self::EmaRsi { .. } => "EMA+RSI",
            Self::StochRsi { .. } => "Stoch+RSI",
            Self::MacdBollinger { .. } => "MACD+Bollinger",
            Self::TripleRsiMacdBb { .. } => "Triple:RSI+MACD+BB",
            Self::TripleEmaRsiStoch { .. } => "Triple:EMA+RSI+Stoch",
            Self::Gabagool { .. } => "Gabagool",
        }
    }

    fn is_gabagool(&self) -> bool {
        matches!(self, Self::Gabagool { .. })
    }
}

/// Position sizing mode
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizingMode {
    #[default]
    Fixed,
    Kelly,
    ConfidenceWeighted,
}

/// Request to start a discovery scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryRequest {
    pub symbols: Vec<String>,
    #[serde(default = "default_days")]
    pub days: u32,
    pub top_n: Option<usize>,
    pub sizing_mode: Option<SizingMode>,
}

fn default_days() -> u32 {
    90
}

/// A single scored discovery result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResult {
    pub rank: usize,
    pub strategy_type: DiscoveryStrategyType,
    pub strategy_name: String,
    pub symbol: String,
    pub sizing_mode: SizingMode,
    pub composite_score: Decimal,
    pub net_pnl: Decimal,
    pub gross_pnl: Decimal,
    pub total_fees: Decimal,
    pub win_rate: Decimal,
    pub total_trades: u32,
    pub sharpe_ratio: Decimal,
    pub max_drawdown_pct: Decimal,
    pub profit_factor: Decimal,
    pub avg_trade_pnl: Decimal,
    // Gabagool-specific
    pub hit_rate: Option<Decimal>,
    pub avg_locked_profit: Option<Decimal>,
}

/// Discovery scan status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryStatus {
    Idle,
    FetchingData,
    Phase1BroadScan,
    Phase2Refinement,
    Complete,
    Error,
}

/// Shared progress tracker for the discovery agent
pub struct DiscoveryProgress {
    pub status: RwLock<DiscoveryStatus>,
    pub phase: RwLock<String>,
    pub current_strategy: RwLock<String>,
    pub current_symbol: RwLock<String>,
    pub total_combinations: AtomicU32,
    pub completed: AtomicU32,
    pub skipped: AtomicU32,
    pub cancelled: AtomicBool,
    pub best_so_far: RwLock<Vec<DiscoveryResult>>,
    pub final_results: RwLock<Vec<DiscoveryResult>>,
    pub error_message: RwLock<Option<String>>,
    pub started_at: RwLock<Option<String>>,
}

impl DiscoveryProgress {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(DiscoveryStatus::Idle),
            phase: RwLock::new(String::new()),
            current_strategy: RwLock::new(String::new()),
            current_symbol: RwLock::new(String::new()),
            total_combinations: AtomicU32::new(0),
            completed: AtomicU32::new(0),
            skipped: AtomicU32::new(0),
            cancelled: AtomicBool::new(false),
            best_so_far: RwLock::new(Vec::new()),
            final_results: RwLock::new(Vec::new()),
            error_message: RwLock::new(None),
            started_at: RwLock::new(None),
        }
    }

    pub fn reset(&self) {
        *self.status.write().unwrap() = DiscoveryStatus::FetchingData;
        *self.phase.write().unwrap() = "Fetching market data...".to_string();
        *self.current_strategy.write().unwrap() = String::new();
        *self.current_symbol.write().unwrap() = String::new();
        self.total_combinations.store(0, Ordering::Relaxed);
        self.completed.store(0, Ordering::Relaxed);
        self.skipped.store(0, Ordering::Relaxed);
        self.cancelled.store(false, Ordering::Relaxed);
        *self.best_so_far.write().unwrap() = Vec::new();
        *self.final_results.write().unwrap() = Vec::new();
        *self.error_message.write().unwrap() = None;
        *self.started_at.write().unwrap() = Some(Utc::now().to_rfc3339());
    }

    pub fn progress_pct(&self) -> f32 {
        let total = self.total_combinations.load(Ordering::Relaxed);
        let done = self.completed.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            (done as f32 / total as f32) * 100.0
        }
    }

    pub fn is_running(&self) -> bool {
        let s = self.status.read().unwrap();
        matches!(
            *s,
            DiscoveryStatus::FetchingData
                | DiscoveryStatus::Phase1BroadScan
                | DiscoveryStatus::Phase2Refinement
        )
    }
}

impl Default for DiscoveryProgress {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Parameter Grids — Phase 1 (Broad Scan)
// ============================================================================

fn generate_phase1_grid() -> Vec<DiscoveryStrategyType> {
    let mut grid = Vec::with_capacity(500);

    // 1. RSI: 5 periods × 4 ob × 4 os = 80
    for &period in &[5usize, 9, 14, 21, 28] {
        for &ob in &[65.0, 70.0, 75.0, 80.0] {
            for &os in &[20.0, 25.0, 30.0, 35.0] {
                if os < ob {
                    grid.push(DiscoveryStrategyType::Rsi {
                        period,
                        overbought: ob,
                        oversold: os,
                    });
                }
            }
        }
    }

    // 2. Bollinger Bands: 5 periods × 4 multipliers = 20
    for &period in &[10usize, 15, 20, 25, 30] {
        for &mult in &[1.5, 2.0, 2.5, 3.0] {
            grid.push(DiscoveryStrategyType::BollingerBands {
                period,
                multiplier: mult,
            });
        }
    }

    // 3. MACD: 3 fast × 3 slow × 2 signal = 18
    for &fast in &[5usize, 8, 12] {
        for &slow in &[17usize, 21, 26] {
            for &signal in &[5usize, 9] {
                if fast < slow {
                    grid.push(DiscoveryStrategyType::Macd { fast, slow, signal });
                }
            }
        }
    }

    // 4. EMA Crossover: 5 fast × 4 slow = 20
    for &fast in &[5usize, 8, 10, 13, 15] {
        for &slow in &[20usize, 26, 30, 50] {
            if fast < slow {
                grid.push(DiscoveryStrategyType::EmaCrossover {
                    fast_period: fast,
                    slow_period: slow,
                });
            }
        }
    }

    // 5. Stochastic: 4 periods × 3 ob × 3 os = 36
    for &period in &[5usize, 9, 14, 21] {
        for &ob in &[75.0, 80.0, 85.0] {
            for &os in &[15.0, 20.0, 25.0] {
                grid.push(DiscoveryStrategyType::Stochastic {
                    period,
                    overbought: ob,
                    oversold: os,
                });
            }
        }
    }

    // 6. ATR Mean Reversion: 3 atr × 4 sma × 2 mult = 24
    for &atr in &[7usize, 14, 21] {
        for &sma in &[10usize, 20, 30, 50] {
            for &mult in &[1.5, 2.0] {
                grid.push(DiscoveryStrategyType::AtrMeanReversion {
                    atr_period: atr,
                    sma_period: sma,
                    multiplier: mult,
                });
            }
        }
    }

    // 7. RSI+Bollinger: rsi(3×2×2) × bb(3×2) = 36
    for &rp in &[9usize, 14, 21] {
        for &rob in &[70.0, 80.0] {
            for &ros in &[20.0, 30.0] {
                for &bp in &[15usize, 20, 25] {
                    for &bm in &[2.0, 2.5] {
                        grid.push(DiscoveryStrategyType::RsiBollinger {
                            rsi_period: rp,
                            rsi_ob: rob,
                            rsi_os: ros,
                            bb_period: bp,
                            bb_mult: bm,
                        });
                    }
                }
            }
        }
    }

    // 8. MACD+RSI: macd(6) × rsi(2×3) = 36
    for &mf in &[8usize, 12] {
        for &ms in &[21usize, 26] {
            for &msig in &[5usize, 9] {
                if mf < ms {
                    for &rp in &[9usize, 14] {
                        for &rob in &[70.0, 75.0, 80.0] {
                            grid.push(DiscoveryStrategyType::MacdRsi {
                                macd_fast: mf,
                                macd_slow: ms,
                                macd_signal: msig,
                                rsi_period: rp,
                                rsi_ob: rob,
                                rsi_os: 30.0,
                            });
                        }
                    }
                }
            }
        }
    }

    // 9. EMA+RSI: ema(4) × rsi(3×2) = 24
    for &ef in &[8usize, 13] {
        for &es in &[26usize, 50] {
            for &rp in &[9usize, 14, 21] {
                for &ros in &[25.0, 30.0] {
                    grid.push(DiscoveryStrategyType::EmaRsi {
                        ema_fast: ef,
                        ema_slow: es,
                        rsi_period: rp,
                        rsi_ob: 70.0,
                        rsi_os: ros,
                    });
                }
            }
        }
    }

    // 10. Stoch+RSI: stoch(2) × rsi(3×4) = 24
    for &sp in &[9usize, 14] {
        for &rp in &[9usize, 14, 21] {
            for &rob in &[70.0, 75.0, 80.0, 85.0] {
                grid.push(DiscoveryStrategyType::StochRsi {
                    stoch_period: sp,
                    stoch_ob: 80.0,
                    stoch_os: 20.0,
                    rsi_period: rp,
                    rsi_ob: rob,
                    rsi_os: 30.0,
                });
            }
        }
    }

    // 11. MACD+Bollinger: macd(4) × bb(3×2) = 24
    for &mf in &[8usize, 12] {
        for &ms in &[21usize, 26] {
            if mf < ms {
                for &bp in &[15usize, 20, 25] {
                    for &bm in &[2.0, 2.5] {
                        grid.push(DiscoveryStrategyType::MacdBollinger {
                            macd_fast: mf,
                            macd_slow: ms,
                            macd_signal: 9,
                            bb_period: bp,
                            bb_mult: bm,
                        });
                    }
                }
            }
        }
    }

    // 12. Triple RSI+MACD+BB: rsi(2×2) × macd(2) × bb(3) = 24
    for &rp in &[9usize, 14] {
        for &ros in &[25.0, 30.0] {
            for &ms in &[21usize, 26] {
                for &bp in &[15usize, 20, 25] {
                    grid.push(DiscoveryStrategyType::TripleRsiMacdBb {
                        rsi_period: rp,
                        rsi_ob: 70.0,
                        rsi_os: ros,
                        macd_fast: 12,
                        macd_slow: ms,
                        macd_signal: 9,
                        bb_period: bp,
                        bb_mult: 2.0,
                    });
                }
            }
        }
    }

    // 13. Triple EMA+RSI+Stoch: ema(2) × rsi(2×2) × stoch(2) = 16
    for &ef in &[8usize, 13] {
        for &rp in &[9usize, 14] {
            for &ros in &[25.0, 30.0] {
                for &sp in &[9usize, 14] {
                    grid.push(DiscoveryStrategyType::TripleEmaRsiStoch {
                        ema_fast: ef,
                        ema_slow: 26,
                        rsi_period: rp,
                        rsi_ob: 70.0,
                        rsi_os: ros,
                        stoch_period: sp,
                        stoch_ob: 80.0,
                        stoch_os: 20.0,
                    });
                }
            }
        }
    }

    // 14. Gabagool: 4 mpc × 4 bo × 3 sm = 48
    for mpc in &[dec!(0.92), dec!(0.94), dec!(0.96), dec!(0.98)] {
        for bo in &[dec!(0.005), dec!(0.01), dec!(0.02), dec!(0.03)] {
            for sm in &[dec!(2), dec!(3), dec!(5)] {
                grid.push(DiscoveryStrategyType::Gabagool {
                    max_pair_cost: *mpc,
                    bid_offset: *bo,
                    spread_multiplier: *sm,
                });
            }
        }
    }

    grid
}

// ============================================================================
// Generic Backtest Engine (for indicator-based strategies)
// ============================================================================

#[allow(dead_code)]
struct GenericBacktestResult {
    total_pnl: Decimal,
    total_fees: Decimal,
    total_trades: u32,
    winning_trades: u32,
    losing_trades: u32,
    win_rate: Decimal,
    sharpe_ratio: Decimal,
    max_drawdown_pct: Decimal,
    profit_factor: Decimal,
    avg_trade_pnl: Decimal,
}

struct OpenPosition {
    entry_price: Decimal,
    size: Decimal,
}

fn run_generic_backtest(
    generator: &mut dyn SignalGenerator,
    klines: &[Kline],
    initial_capital: Decimal,
    base_position_pct: Decimal,
    sizing_mode: SizingMode,
    fee_config: &PolymarketFeeConfig,
) -> GenericBacktestResult {
    let hundred = dec!(100);
    let poly_price = dec!(0.50); // Conservative: max fee at p=0.50
    let mut equity = initial_capital;
    let mut peak_equity = equity;
    let mut max_drawdown_pct = Decimal::ZERO;
    let mut position: Option<OpenPosition> = None;
    let mut trades: Vec<BacktestTrade> = Vec::new();

    // Sliding window for Kelly
    let mut recent_wins = 0u32;
    let mut recent_total = 0u32;
    let mut recent_avg_win = Decimal::ZERO;
    let mut recent_avg_loss = Decimal::ZERO;

    for kline in klines {
        let sig = generator.on_bar(kline);

        match sig.signal {
            crate::strategy::Signal::Buy => {
                if position.is_none() {
                    // Calculate position size
                    let size_pct = match sizing_mode {
                        SizingMode::Fixed => base_position_pct,
                        SizingMode::Kelly => {
                            if recent_total >= 10 && recent_avg_loss > Decimal::ZERO {
                                let p = Decimal::from(recent_wins) / Decimal::from(recent_total);
                                let b = recent_avg_win / recent_avg_loss;
                                let q = Decimal::ONE - p;
                                let kelly = if b > Decimal::ZERO {
                                    ((p * b - q) / b) * hundred
                                } else {
                                    Decimal::ZERO
                                };
                                kelly.max(Decimal::ZERO).min(dec!(25)) // Cap at 25%
                            } else {
                                base_position_pct
                            }
                        }
                        SizingMode::ConfidenceWeighted => {
                            let conf = Decimal::from_str_exact(&format!("{:.4}", sig.confidence))
                                .unwrap_or(Decimal::ONE);
                            base_position_pct * conf
                        }
                    };

                    if size_pct <= Decimal::ZERO {
                        continue;
                    }

                    let position_value = equity * size_pct / hundred;
                    let shares = position_value / kline.close;

                    // Entry fee
                    let entry_fee = calculate_taker_fee(shares, poly_price, fee_config);
                    equity -= entry_fee;

                    position = Some(OpenPosition {
                        entry_price: kline.close,
                        size: shares,
                    });
                }
            }
            crate::strategy::Signal::Sell => {
                if let Some(pos) = position.take() {
                    let pnl = (kline.close - pos.entry_price) * pos.size;
                    let exit_fee = calculate_taker_fee(pos.size, poly_price, fee_config);

                    let pnl_pct = if pos.entry_price > Decimal::ZERO {
                        (kline.close - pos.entry_price) / pos.entry_price * hundred
                    } else {
                        Decimal::ZERO
                    };

                    equity += pnl - exit_fee;

                    trades.push(BacktestTrade {
                        entry_time: 0,
                        exit_time: kline.open_time,
                        side: TradeSide::Buy,
                        entry_price: pos.entry_price,
                        exit_price: kline.close,
                        size: pos.size,
                        pnl,
                        pnl_pct,
                    });

                    // Update Kelly stats
                    recent_total += 1;
                    if pnl > Decimal::ZERO {
                        recent_wins += 1;
                        recent_avg_win = if recent_wins > 0 {
                            (recent_avg_win * Decimal::from(recent_wins - 1) + pnl)
                                / Decimal::from(recent_wins)
                        } else {
                            pnl
                        };
                    } else {
                        let losses = recent_total - recent_wins;
                        recent_avg_loss = if losses > 0 {
                            (recent_avg_loss * Decimal::from(losses - 1) + pnl.abs())
                                / Decimal::from(losses)
                        } else {
                            pnl.abs()
                        };
                    }
                }
            }
            crate::strategy::Signal::Hold => {}
        }

        // Track drawdown
        let unrealized = position
            .as_ref()
            .map(|pos| (kline.close - pos.entry_price) * pos.size)
            .unwrap_or(Decimal::ZERO);
        let current_equity = equity + unrealized;

        if current_equity > peak_equity {
            peak_equity = current_equity;
        }
        if peak_equity > Decimal::ZERO {
            let dd_pct = (peak_equity - current_equity) / peak_equity * hundred;
            if dd_pct > max_drawdown_pct {
                max_drawdown_pct = dd_pct;
            }
        }
    }

    // Close any remaining position at last bar price
    if let Some(pos) = position.take() {
        if let Some(last) = klines.last() {
            let pnl = (last.close - pos.entry_price) * pos.size;
            let exit_fee = calculate_taker_fee(pos.size, poly_price, fee_config);
            equity += pnl - exit_fee;
            trades.push(BacktestTrade {
                entry_time: 0,
                exit_time: last.open_time,
                side: TradeSide::Buy,
                entry_price: pos.entry_price,
                exit_price: last.close,
                size: pos.size,
                pnl,
                pnl_pct: if pos.entry_price > Decimal::ZERO {
                    (last.close - pos.entry_price) / pos.entry_price * hundred
                } else {
                    Decimal::ZERO
                },
            });
        }
    }

    // Calculate metrics
    let total_trades = trades.len() as u32;
    let winning_trades = trades.iter().filter(|t| t.pnl > Decimal::ZERO).count() as u32;
    let losing_trades = total_trades - winning_trades;

    let win_rate = if total_trades > 0 {
        Decimal::from(winning_trades) / Decimal::from(total_trades) * hundred
    } else {
        Decimal::ZERO
    };

    let total_pnl = equity - initial_capital;

    // Calculate total fees from entry+exit on each trade
    let total_fees = {
        let mut fees = Decimal::ZERO;
        for trade in &trades {
            fees += calculate_taker_fee(trade.size, poly_price, fee_config) * dec!(2);
        }
        fees
    };

    let gross_profits: Decimal = trades
        .iter()
        .filter(|t| t.pnl > Decimal::ZERO)
        .map(|t| t.pnl)
        .sum();
    let gross_losses: Decimal = trades
        .iter()
        .filter(|t| t.pnl < Decimal::ZERO)
        .map(|t| t.pnl.abs())
        .sum();
    let profit_factor = if gross_losses > Decimal::ZERO {
        gross_profits / gross_losses
    } else if gross_profits > Decimal::ZERO {
        dec!(999.99)
    } else {
        Decimal::ZERO
    };

    let sharpe_ratio = calculate_sharpe(&trades);

    let avg_trade_pnl = if total_trades > 0 {
        total_pnl / Decimal::from(total_trades)
    } else {
        Decimal::ZERO
    };

    GenericBacktestResult {
        total_pnl,
        total_fees,
        total_trades,
        winning_trades,
        losing_trades,
        win_rate,
        sharpe_ratio,
        max_drawdown_pct,
        profit_factor,
        avg_trade_pnl,
    }
}

fn calculate_sharpe(trades: &[BacktestTrade]) -> Decimal {
    if trades.len() < 2 {
        return Decimal::ZERO;
    }

    let returns: Vec<f64> = trades
        .iter()
        .map(|t| t.pnl_pct.to_string().parse::<f64>().unwrap_or(0.0))
        .collect();

    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std_dev = variance.sqrt();

    if std_dev < 1e-10 {
        return Decimal::ZERO;
    }

    let sharpe = mean / std_dev;
    Decimal::from_str_exact(&format!("{:.2}", sharpe)).unwrap_or(Decimal::ZERO)
}

// ============================================================================
// Scoring
// ============================================================================

fn score_result(result: &DiscoveryResult, initial_capital: Decimal) -> Decimal {
    // Minimum 5 trades for statistical significance
    if result.total_trades < 5 {
        return dec!(-9999);
    }

    let net_pnl = result.net_pnl;

    // Win rate bonus: >=70% → ×3, >=55% → ×2, else ×1
    let win_rate_bonus = if result.win_rate >= dec!(70) {
        (result.win_rate - dec!(50)) * dec!(3)
    } else if result.win_rate >= dec!(55) {
        (result.win_rate - dec!(50)) * dec!(2)
    } else {
        (result.win_rate - dec!(50)) * Decimal::ONE
    };

    // Sharpe bonus
    let sharpe_bonus = result.sharpe_ratio * dec!(100);

    // Drawdown penalty
    let drawdown_penalty = result.max_drawdown_pct * dec!(3);

    // Profit factor bonus
    let pf_bonus = if result.profit_factor > Decimal::ONE {
        (result.profit_factor - Decimal::ONE) * dec!(50)
    } else {
        (result.profit_factor - Decimal::ONE) * dec!(100) // Heavier penalty
    };

    // Explosive PnL bonus (>20% of capital)
    let explosive_bonus =
        if initial_capital > Decimal::ZERO && net_pnl > initial_capital * dec!(0.20) {
            dec!(200)
        } else {
            Decimal::ZERO
        };

    net_pnl + win_rate_bonus + sharpe_bonus - drawdown_penalty + pf_bonus + explosive_bonus
}

// ============================================================================
// Phase 2 Refinement
// ============================================================================

fn generate_refinement_grid(strategy: &DiscoveryStrategyType) -> Vec<DiscoveryStrategyType> {
    let mut variants = Vec::new();

    match strategy {
        DiscoveryStrategyType::Rsi {
            period,
            overbought,
            oversold,
        } => {
            for dp in [-2i32, -1, 0, 1, 2] {
                for dob in [-2.5f64, 0.0, 2.5] {
                    for dos in [-2.5f64, 0.0, 2.5] {
                        let p = (*period as i32 + dp).max(3) as usize;
                        let ob = overbought + dob;
                        let os = oversold + dos;
                        if os < ob && ob <= 90.0 && os >= 10.0 {
                            variants.push(DiscoveryStrategyType::Rsi {
                                period: p,
                                overbought: ob,
                                oversold: os,
                            });
                        }
                    }
                }
            }
        }
        DiscoveryStrategyType::BollingerBands { period, multiplier } => {
            for dp in [-2i32, 0, 2] {
                for dm in [-0.25f64, 0.0, 0.25] {
                    let p = (*period as i32 + dp).max(5) as usize;
                    let m = multiplier + dm;
                    if m > 0.5 {
                        variants.push(DiscoveryStrategyType::BollingerBands {
                            period: p,
                            multiplier: m,
                        });
                    }
                }
            }
        }
        DiscoveryStrategyType::Macd { fast, slow, signal } => {
            for df in [-1i32, 0, 1] {
                for ds in [-2i32, 0, 2] {
                    for dsig in [-1i32, 0, 1] {
                        let f = (*fast as i32 + df).max(3) as usize;
                        let s = (*slow as i32 + ds).max(5) as usize;
                        let sig = (*signal as i32 + dsig).max(2) as usize;
                        if f < s {
                            variants.push(DiscoveryStrategyType::Macd {
                                fast: f,
                                slow: s,
                                signal: sig,
                            });
                        }
                    }
                }
            }
        }
        DiscoveryStrategyType::EmaCrossover {
            fast_period,
            slow_period,
        } => {
            for df in [-2i32, 0, 2] {
                for ds in [-3i32, 0, 3] {
                    let f = (*fast_period as i32 + df).max(3) as usize;
                    let s = (*slow_period as i32 + ds).max(5) as usize;
                    if f < s {
                        variants.push(DiscoveryStrategyType::EmaCrossover {
                            fast_period: f,
                            slow_period: s,
                        });
                    }
                }
            }
        }
        DiscoveryStrategyType::Gabagool {
            max_pair_cost,
            bid_offset,
            spread_multiplier,
        } => {
            for dmpc in &[dec!(-0.01), dec!(0), dec!(0.01)] {
                for dbo in &[dec!(-0.005), dec!(0), dec!(0.005)] {
                    for dsm in &[dec!(-0.5), dec!(0), dec!(0.5)] {
                        let mpc = (*max_pair_cost + dmpc).max(dec!(0.85)).min(dec!(0.99));
                        let bo = (*bid_offset + dbo).max(dec!(0.001));
                        let sm = (*spread_multiplier + dsm).max(dec!(1));
                        variants.push(DiscoveryStrategyType::Gabagool {
                            max_pair_cost: mpc,
                            bid_offset: bo,
                            spread_multiplier: sm,
                        });
                    }
                }
            }
        }
        // For combos and other types, just return the original (no refinement)
        other => {
            variants.push(other.clone());
        }
    }

    variants
}

// ============================================================================
// Persistence Helpers
// ============================================================================

/// Compute a deterministic hash for deduplication of backtest params
fn compute_params_hash(
    strategy: &DiscoveryStrategyType,
    symbol: &str,
    days: u32,
    sizing: SizingMode,
) -> String {
    let json = serde_json::to_string(strategy).unwrap_or_default();
    let input = format!("{}:{}:{}:{:?}", json, symbol, days, sizing);
    let hash = Sha256::digest(input.as_bytes());
    format!("{:x}", hash)
}

/// Convert a DiscoveryResult to a DiscoveryBacktestRecord for DB storage
fn result_to_record(
    result: &DiscoveryResult,
    hash: &str,
    run_id: &str,
    phase: &str,
    days: u32,
) -> DiscoveryBacktestRecord {
    let strategy_type_tag = match &result.strategy_type {
        DiscoveryStrategyType::Rsi { .. } => "rsi",
        DiscoveryStrategyType::BollingerBands { .. } => "bollinger_bands",
        DiscoveryStrategyType::Macd { .. } => "macd",
        DiscoveryStrategyType::EmaCrossover { .. } => "ema_crossover",
        DiscoveryStrategyType::Stochastic { .. } => "stochastic",
        DiscoveryStrategyType::AtrMeanReversion { .. } => "atr_mean_reversion",
        DiscoveryStrategyType::RsiBollinger { .. } => "rsi_bollinger",
        DiscoveryStrategyType::MacdRsi { .. } => "macd_rsi",
        DiscoveryStrategyType::EmaRsi { .. } => "ema_rsi",
        DiscoveryStrategyType::StochRsi { .. } => "stoch_rsi",
        DiscoveryStrategyType::MacdBollinger { .. } => "macd_bollinger",
        DiscoveryStrategyType::TripleRsiMacdBb { .. } => "triple_rsi_macd_bb",
        DiscoveryStrategyType::TripleEmaRsiStoch { .. } => "triple_ema_rsi_stoch",
        DiscoveryStrategyType::Gabagool { .. } => "gabagool",
    };

    DiscoveryBacktestRecord {
        id: None,
        params_hash: hash.to_string(),
        strategy_type: strategy_type_tag.to_string(),
        strategy_name: result.strategy_name.clone(),
        strategy_params: serde_json::to_string(&result.strategy_type).unwrap_or_default(),
        symbol: result.symbol.clone(),
        days: days as i64,
        sizing_mode: format!("{:?}", result.sizing_mode),
        composite_score: result.composite_score.to_string(),
        net_pnl: result.net_pnl.to_string(),
        gross_pnl: result.gross_pnl.to_string(),
        total_fees: result.total_fees.to_string(),
        win_rate: result.win_rate.to_string(),
        total_trades: result.total_trades as i64,
        sharpe_ratio: result.sharpe_ratio.to_string(),
        max_drawdown_pct: result.max_drawdown_pct.to_string(),
        profit_factor: result.profit_factor.to_string(),
        avg_trade_pnl: result.avg_trade_pnl.to_string(),
        hit_rate: result.hit_rate.map(|d| d.to_string()),
        avg_locked_profit: result.avg_locked_profit.map(|d| d.to_string()),
        discovery_run_id: Some(run_id.to_string()),
        phase: Some(phase.to_string()),
    }
}

/// Convert a DB record back to a DiscoveryResult
fn record_to_result(record: DiscoveryBacktestRecord) -> DiscoveryResult {
    let strategy_type: DiscoveryStrategyType = serde_json::from_str(&record.strategy_params)
        .unwrap_or(DiscoveryStrategyType::Rsi {
            period: 14,
            overbought: 70.0,
            oversold: 30.0,
        });

    let parse_dec = |s: &str| -> Decimal { Decimal::from_str_exact(s).unwrap_or(Decimal::ZERO) };

    let sizing_mode = match record.sizing_mode.to_lowercase().as_str() {
        "kelly" => SizingMode::Kelly,
        "confidenceweighted" => SizingMode::ConfidenceWeighted,
        _ => SizingMode::Fixed,
    };

    DiscoveryResult {
        rank: 0,
        strategy_type,
        strategy_name: record.strategy_name,
        symbol: record.symbol,
        sizing_mode,
        composite_score: parse_dec(&record.composite_score),
        net_pnl: parse_dec(&record.net_pnl),
        gross_pnl: parse_dec(&record.gross_pnl),
        total_fees: parse_dec(&record.total_fees),
        win_rate: parse_dec(&record.win_rate),
        total_trades: record.total_trades as u32,
        sharpe_ratio: parse_dec(&record.sharpe_ratio),
        max_drawdown_pct: parse_dec(&record.max_drawdown_pct),
        profit_factor: parse_dec(&record.profit_factor),
        avg_trade_pnl: parse_dec(&record.avg_trade_pnl),
        hit_rate: record.hit_rate.as_deref().map(parse_dec),
        avg_locked_profit: record.avg_locked_profit.as_deref().map(parse_dec),
    }
}

// ============================================================================
// Main Discovery Runner
// ============================================================================

pub async fn run_discovery(
    request: DiscoveryRequest,
    binance: Arc<BinanceClient>,
    progress: Arc<DiscoveryProgress>,
    db_pool: Option<SqlitePool>,
) {
    let top_n = request.top_n.unwrap_or(10);
    let sizing_mode = request.sizing_mode.unwrap_or_default();
    let initial_capital = dec!(10000);
    let base_position_pct = dec!(10);
    let fee_config = PolymarketFeeConfig::default();

    let run_id = Utc::now().timestamp_millis().to_string();

    info!(
        symbols = ?request.symbols,
        days = request.days,
        sizing = ?sizing_mode,
        run_id = %run_id,
        "Starting discovery agent"
    );

    // ── Phase 0: Fetch klines ───────────────────────────────────────────
    let end_time = chrono::Utc::now().timestamp_millis();
    let start_time = end_time - (request.days as i64 * 24 * 60 * 60 * 1000);

    let mut symbol_klines: Vec<(String, Vec<Kline>)> = Vec::new();

    for symbol in &request.symbols {
        if progress.cancelled.load(Ordering::Relaxed) {
            return;
        }
        *progress.current_symbol.write().unwrap() = symbol.clone();

        match binance
            .get_klines_paginated(symbol, "15m", start_time, end_time)
            .await
        {
            Ok(klines) => {
                info!(symbol = %symbol, bars = klines.len(), "Fetched klines");
                symbol_klines.push((symbol.clone(), klines));
            }
            Err(e) => {
                warn!(symbol = %symbol, error = %e, "Failed to fetch klines, skipping");
            }
        }
    }

    if symbol_klines.is_empty() {
        *progress.error_message.write().unwrap() =
            Some("Failed to fetch klines for any symbol".to_string());
        *progress.status.write().unwrap() = DiscoveryStatus::Error;
        return;
    }

    // ── Phase 1: Broad Scan ─────────────────────────────────────────────
    *progress.status.write().unwrap() = DiscoveryStatus::Phase1BroadScan;
    *progress.phase.write().unwrap() = "Phase 1: Broad Scan".to_string();

    let grid = generate_phase1_grid();
    let total_phase1 = grid.len() as u32 * symbol_klines.len() as u32;

    // Estimate phase 2 — top 20 × ~27 variants = ~540
    let estimated_phase2 = 20u32 * 27;
    let total_all = total_phase1 + estimated_phase2;
    progress
        .total_combinations
        .store(total_all, Ordering::Relaxed);

    info!(
        grid_size = grid.len(),
        symbols = symbol_klines.len(),
        total_combos = total_phase1,
        "Phase 1 starting"
    );

    let mut all_results: Vec<DiscoveryResult> = Vec::new();
    let mut global_idx = 0u32;

    for (symbol, klines) in &symbol_klines {
        for strategy_type in &grid {
            if progress.cancelled.load(Ordering::Relaxed) {
                info!("Discovery cancelled by user");
                *progress.status.write().unwrap() = DiscoveryStatus::Idle;
                return;
            }

            // Update progress
            if global_idx.is_multiple_of(50) {
                *progress.current_strategy.write().unwrap() = strategy_type.name().to_string();
                *progress.current_symbol.write().unwrap() = symbol.clone();
            }

            // Check DB cache before running backtest
            let hash = compute_params_hash(strategy_type, symbol, request.days, sizing_mode);
            if let Some(pool) = &db_pool {
                let repo = DiscoveryRepository::new(pool);
                if let Ok(Some(existing)) = repo.get_by_hash(&hash).await {
                    all_results.push(record_to_result(existing));
                    global_idx += 1;
                    progress.completed.store(global_idx, Ordering::Relaxed);
                    progress.skipped.fetch_add(1, Ordering::Relaxed);
                    if global_idx.is_multiple_of(50) {
                        update_best_so_far(&all_results, initial_capital, top_n, &progress);
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    }
                    continue;
                }
            }

            let result = run_single_backtest(
                strategy_type,
                klines,
                symbol,
                initial_capital,
                base_position_pct,
                sizing_mode,
                &fee_config,
            );

            // Save to DB
            if let Some(pool) = &db_pool {
                let record = result_to_record(&result, &hash, &run_id, "phase1", request.days);
                let repo = DiscoveryRepository::new(pool);
                let _ = repo.save(&record).await;
            }

            all_results.push(result);

            global_idx += 1;
            progress.completed.store(global_idx, Ordering::Relaxed);

            // Sleep every 50 iterations to let trading strategies breathe
            if global_idx.is_multiple_of(50) {
                update_best_so_far(&all_results, initial_capital, top_n, &progress);
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        }
    }

    // Final update of best_so_far after phase 1
    update_best_so_far(&all_results, initial_capital, top_n, &progress);

    info!(results = all_results.len(), "Phase 1 complete");

    // ── Phase 2: Refinement ─────────────────────────────────────────────
    *progress.status.write().unwrap() = DiscoveryStatus::Phase2Refinement;
    *progress.phase.write().unwrap() = "Phase 2: Refinement".to_string();

    // Take top 20 from phase 1
    let mut phase1_scored = all_results.clone();
    phase1_scored.sort_by(|a, b| {
        let sa = score_result(a, initial_capital);
        let sb = score_result(b, initial_capital);
        sb.cmp(&sa)
    });
    let top_for_refinement: Vec<DiscoveryResult> = phase1_scored.into_iter().take(20).collect();

    info!(
        top_count = top_for_refinement.len(),
        "Phase 2: refining top results"
    );

    for top_result in &top_for_refinement {
        if progress.cancelled.load(Ordering::Relaxed) {
            *progress.status.write().unwrap() = DiscoveryStatus::Idle;
            return;
        }

        let refinement_grid = generate_refinement_grid(&top_result.strategy_type);

        // Find klines for this symbol
        let klines_opt = symbol_klines
            .iter()
            .find(|(s, _)| *s == top_result.symbol)
            .map(|(_, k)| k);

        let klines = match klines_opt {
            Some(k) => k,
            None => continue,
        };

        *progress.current_strategy.write().unwrap() =
            format!("{} (refine)", top_result.strategy_name);
        *progress.current_symbol.write().unwrap() = top_result.symbol.clone();

        for variant in &refinement_grid {
            // Check DB cache before running backtest
            let hash = compute_params_hash(variant, &top_result.symbol, request.days, sizing_mode);
            if let Some(pool) = &db_pool {
                let repo = DiscoveryRepository::new(pool);
                if let Ok(Some(existing)) = repo.get_by_hash(&hash).await {
                    all_results.push(record_to_result(existing));
                    global_idx += 1;
                    progress.completed.store(global_idx, Ordering::Relaxed);
                    progress.skipped.fetch_add(1, Ordering::Relaxed);
                    if global_idx.is_multiple_of(50) {
                        update_best_so_far(&all_results, initial_capital, top_n, &progress);
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    }
                    continue;
                }
            }

            let result = run_single_backtest(
                variant,
                klines,
                &top_result.symbol,
                initial_capital,
                base_position_pct,
                sizing_mode,
                &fee_config,
            );

            // Save to DB
            if let Some(pool) = &db_pool {
                let record = result_to_record(&result, &hash, &run_id, "phase2", request.days);
                let repo = DiscoveryRepository::new(pool);
                let _ = repo.save(&record).await;
            }

            all_results.push(result);

            global_idx += 1;
            progress.completed.store(global_idx, Ordering::Relaxed);

            if global_idx.is_multiple_of(50) {
                update_best_so_far(&all_results, initial_capital, top_n, &progress);
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        }
    }

    // ── Finalize ────────────────────────────────────────────────────────
    // Score all, sort, rank
    let mut scored_results: Vec<(Decimal, DiscoveryResult)> = all_results
        .into_iter()
        .map(|r| {
            let score = score_result(&r, initial_capital);
            (score, r)
        })
        .collect();

    scored_results.sort_by(|a, b| b.0.cmp(&a.0));

    // Deduplicate: keep best score per (strategy_name, symbol) to avoid near-identical results
    let mut seen = std::collections::HashSet::new();
    let mut final_results = Vec::new();
    for (score, mut result) in scored_results {
        let key = format!(
            "{}:{}:{}",
            result.strategy_name, result.symbol, result.total_trades
        );
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        result.composite_score = score;
        result.rank = final_results.len() + 1;
        final_results.push(result);
        if final_results.len() >= top_n {
            break;
        }
    }

    // Update total to actual completed count
    progress
        .total_combinations
        .store(global_idx, Ordering::Relaxed);
    progress.completed.store(global_idx, Ordering::Relaxed);

    let skipped_count = progress.skipped.load(Ordering::Relaxed);
    info!(
        final_count = final_results.len(),
        total_tested = global_idx,
        skipped = skipped_count,
        best_score = %final_results.first().map(|r| r.composite_score).unwrap_or_default(),
        "Discovery complete"
    );

    *progress.final_results.write().unwrap() = final_results;
    *progress.status.write().unwrap() = DiscoveryStatus::Complete;
}

// ============================================================================
// Helpers
// ============================================================================

fn run_single_backtest(
    strategy_type: &DiscoveryStrategyType,
    klines: &[Kline],
    symbol: &str,
    initial_capital: Decimal,
    base_position_pct: Decimal,
    sizing_mode: SizingMode,
    fee_config: &PolymarketFeeConfig,
) -> DiscoveryResult {
    if strategy_type.is_gabagool() {
        run_gabagool_backtest_for_discovery(strategy_type, klines, symbol, fee_config, sizing_mode)
    } else {
        run_indicator_backtest_for_discovery(
            strategy_type,
            klines,
            symbol,
            initial_capital,
            base_position_pct,
            sizing_mode,
            fee_config,
        )
    }
}

fn run_indicator_backtest_for_discovery(
    strategy_type: &DiscoveryStrategyType,
    klines: &[Kline],
    symbol: &str,
    initial_capital: Decimal,
    base_position_pct: Decimal,
    sizing_mode: SizingMode,
    fee_config: &PolymarketFeeConfig,
) -> DiscoveryResult {
    let mut generator = build_signal_generator(strategy_type);

    let bt = run_generic_backtest(
        generator.as_mut(),
        klines,
        initial_capital,
        base_position_pct,
        sizing_mode,
        fee_config,
    );

    DiscoveryResult {
        rank: 0,
        strategy_type: strategy_type.clone(),
        strategy_name: strategy_type.name().to_string(),
        symbol: symbol.to_string(),
        sizing_mode,
        composite_score: Decimal::ZERO, // Will be computed in scoring
        net_pnl: bt.total_pnl,
        gross_pnl: bt.total_pnl + bt.total_fees,
        total_fees: bt.total_fees,
        win_rate: bt.win_rate,
        total_trades: bt.total_trades,
        sharpe_ratio: bt.sharpe_ratio,
        max_drawdown_pct: bt.max_drawdown_pct,
        profit_factor: bt.profit_factor,
        avg_trade_pnl: bt.avg_trade_pnl,
        hit_rate: None,
        avg_locked_profit: None,
    }
}

fn run_gabagool_backtest_for_discovery(
    strategy_type: &DiscoveryStrategyType,
    klines: &[Kline],
    symbol: &str,
    fee_config: &PolymarketFeeConfig,
    sizing_mode: SizingMode,
) -> DiscoveryResult {
    let (max_pair_cost, bid_offset, spread_multiplier) = match strategy_type {
        DiscoveryStrategyType::Gabagool {
            max_pair_cost,
            bid_offset,
            spread_multiplier,
        } => (*max_pair_cost, *bid_offset, *spread_multiplier),
        _ => unreachable!(),
    };

    let config = GabagoolBacktestConfig {
        symbol: symbol.to_string(),
        days: 90,
        size_per_side: dec!(10),
        max_pair_cost,
        bid_offset,
        spread_multiplier,
    };

    let result = GabagoolBacktestEngine::run(&config, klines);

    // Calculate gabagool fees
    let mut total_fees = Decimal::ZERO;
    for window in &result.windows {
        if window.traded {
            total_fees += calculate_taker_fee(config.size_per_side, window.yes_fill, fee_config);
            total_fees += calculate_taker_fee(config.size_per_side, window.no_fill, fee_config);
        }
    }

    let net_pnl = result.total_locked_profit - total_fees;

    DiscoveryResult {
        rank: 0,
        strategy_type: strategy_type.clone(),
        strategy_name: "Gabagool".to_string(),
        symbol: symbol.to_string(),
        sizing_mode,
        composite_score: Decimal::ZERO,
        net_pnl,
        gross_pnl: result.total_locked_profit,
        total_fees,
        win_rate: result.hit_rate,
        total_trades: result.traded_windows,
        sharpe_ratio: Decimal::ZERO,
        max_drawdown_pct: Decimal::ZERO,
        profit_factor: if total_fees > Decimal::ZERO {
            result.total_locked_profit / total_fees
        } else {
            dec!(999.99)
        },
        avg_trade_pnl: result.avg_locked_profit,
        hit_rate: Some(result.hit_rate),
        avg_locked_profit: Some(result.avg_locked_profit),
    }
}

fn update_best_so_far(
    results: &[DiscoveryResult],
    initial_capital: Decimal,
    top_n: usize,
    progress: &Arc<DiscoveryProgress>,
) {
    let mut scored: Vec<(Decimal, &DiscoveryResult)> = results
        .iter()
        .map(|r| (score_result(r, initial_capital), r))
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));

    let best: Vec<DiscoveryResult> = scored
        .into_iter()
        .take(top_n)
        .enumerate()
        .map(|(i, (score, r))| {
            let mut r = r.clone();
            r.rank = i + 1;
            r.composite_score = score;
            r
        })
        .collect();

    *progress.best_so_far.write().unwrap() = best;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn make_klines(prices: &[f64]) -> Vec<Kline> {
        prices
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                let price = Decimal::from_str_exact(&format!("{:.2}", p)).unwrap();
                Kline {
                    open_time: (i as i64) * 900_000,
                    open: price,
                    high: price + dec!(1),
                    low: price - dec!(1),
                    close: price,
                    volume: dec!(100),
                    close_time: ((i + 1) as i64) * 900_000 - 1,
                }
            })
            .collect()
    }

    #[test]
    fn test_phase1_grid_size() {
        let grid = generate_phase1_grid();
        // Should be roughly 400-500 per symbol
        assert!(grid.len() > 350, "Grid too small: {}", grid.len());
        assert!(grid.len() < 600, "Grid too large: {}", grid.len());
    }

    #[test]
    fn test_phase1_grid_has_all_strategy_types() {
        let grid = generate_phase1_grid();
        let has_rsi = grid
            .iter()
            .any(|s| matches!(s, DiscoveryStrategyType::Rsi { .. }));
        let has_bb = grid
            .iter()
            .any(|s| matches!(s, DiscoveryStrategyType::BollingerBands { .. }));
        let has_macd = grid
            .iter()
            .any(|s| matches!(s, DiscoveryStrategyType::Macd { .. }));
        let has_ema = grid
            .iter()
            .any(|s| matches!(s, DiscoveryStrategyType::EmaCrossover { .. }));
        let has_stoch = grid
            .iter()
            .any(|s| matches!(s, DiscoveryStrategyType::Stochastic { .. }));
        let has_atr = grid
            .iter()
            .any(|s| matches!(s, DiscoveryStrategyType::AtrMeanReversion { .. }));
        let has_gabagool = grid
            .iter()
            .any(|s| matches!(s, DiscoveryStrategyType::Gabagool { .. }));
        let has_combo = grid
            .iter()
            .any(|s| matches!(s, DiscoveryStrategyType::RsiBollinger { .. }));

        assert!(has_rsi, "Missing RSI strategies");
        assert!(has_bb, "Missing Bollinger strategies");
        assert!(has_macd, "Missing MACD strategies");
        assert!(has_ema, "Missing EMA strategies");
        assert!(has_stoch, "Missing Stochastic strategies");
        assert!(has_atr, "Missing ATR strategies");
        assert!(has_gabagool, "Missing Gabagool strategies");
        assert!(has_combo, "Missing combo strategies");
    }

    #[test]
    fn test_generic_backtest_produces_trades() {
        // Trending data: down then up
        let mut prices = Vec::new();
        for i in 0..30 {
            prices.push(100.0 - (i as f64) * 2.0);
        }
        for i in 0..30 {
            prices.push(40.0 + (i as f64) * 3.0);
        }
        let klines = make_klines(&prices);

        let strategy_type = DiscoveryStrategyType::Rsi {
            period: 14,
            overbought: 70.0,
            oversold: 30.0,
        };
        let mut gen = build_signal_generator(&strategy_type);
        let fee_config = PolymarketFeeConfig::default();

        let result = run_generic_backtest(
            gen.as_mut(),
            &klines,
            dec!(10000),
            dec!(10),
            SizingMode::Fixed,
            &fee_config,
        );

        assert!(
            result.total_trades > 0,
            "Should have executed at least one trade"
        );
    }

    #[test]
    fn test_scoring_penalizes_few_trades() {
        let result = DiscoveryResult {
            rank: 1,
            strategy_type: DiscoveryStrategyType::Rsi {
                period: 14,
                overbought: 70.0,
                oversold: 30.0,
            },
            strategy_name: "RSI".to_string(),
            symbol: "BTCUSDT".to_string(),
            sizing_mode: SizingMode::Fixed,
            composite_score: Decimal::ZERO,
            net_pnl: dec!(1000),
            gross_pnl: dec!(1100),
            total_fees: dec!(100),
            win_rate: dec!(80),
            total_trades: 3, // Below minimum
            sharpe_ratio: dec!(2),
            max_drawdown_pct: dec!(5),
            profit_factor: dec!(3),
            avg_trade_pnl: dec!(333),
            hit_rate: None,
            avg_locked_profit: None,
        };

        let score = score_result(&result, dec!(10000));
        assert_eq!(score, dec!(-9999));
    }

    #[test]
    fn test_scoring_rewards_high_win_rate() {
        let high_wr = DiscoveryResult {
            rank: 1,
            strategy_type: DiscoveryStrategyType::Rsi {
                period: 14,
                overbought: 70.0,
                oversold: 30.0,
            },
            strategy_name: "RSI".to_string(),
            symbol: "BTCUSDT".to_string(),
            sizing_mode: SizingMode::Fixed,
            composite_score: Decimal::ZERO,
            net_pnl: dec!(500),
            gross_pnl: dec!(600),
            total_fees: dec!(100),
            win_rate: dec!(75),
            total_trades: 20,
            sharpe_ratio: dec!(1.5),
            max_drawdown_pct: dec!(3),
            profit_factor: dec!(2.5),
            avg_trade_pnl: dec!(25),
            hit_rate: None,
            avg_locked_profit: None,
        };

        let low_wr = DiscoveryResult {
            win_rate: dec!(45),
            profit_factor: dec!(0.8),
            ..high_wr.clone()
        };

        let score_high = score_result(&high_wr, dec!(10000));
        let score_low = score_result(&low_wr, dec!(10000));

        assert!(
            score_high > score_low,
            "High WR ({}) should score higher than low WR ({})",
            score_high,
            score_low
        );
    }

    #[test]
    fn test_refinement_grid_produces_variants() {
        let strategy = DiscoveryStrategyType::Rsi {
            period: 14,
            overbought: 70.0,
            oversold: 30.0,
        };
        let variants = generate_refinement_grid(&strategy);
        assert!(variants.len() > 10, "Should produce multiple variants");
        assert!(variants.len() < 200, "Should not explode in size");
    }

    #[test]
    fn test_discovery_progress_new() {
        let progress = DiscoveryProgress::new();
        assert!(!progress.is_running());
        assert_eq!(progress.progress_pct(), 0.0);
    }

    #[test]
    fn test_discovery_progress_reset() {
        let progress = DiscoveryProgress::new();
        progress.reset();
        assert!(progress.is_running());
        assert!(progress.started_at.read().unwrap().is_some());
    }

    #[test]
    fn test_gabagool_backtest_for_discovery() {
        let klines = make_klines(&[50000.0; 50]);
        let strategy = DiscoveryStrategyType::Gabagool {
            max_pair_cost: dec!(0.98),
            bid_offset: dec!(0.01),
            spread_multiplier: dec!(3),
        };
        let fee_config = PolymarketFeeConfig::default();

        let result = run_gabagool_backtest_for_discovery(
            &strategy,
            &klines,
            "BTCUSDT",
            &fee_config,
            SizingMode::Fixed,
        );

        assert_eq!(result.strategy_name, "Gabagool");
        assert_eq!(result.symbol, "BTCUSDT");
        assert!(result.hit_rate.is_some());
        assert!(result.avg_locked_profit.is_some());
    }
}
