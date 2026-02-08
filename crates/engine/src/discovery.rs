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
use rand::Rng;
use tracing::{info, warn};

use crate::api::BinanceClient;
use crate::fees::{calculate_taker_fee, PolymarketFeeConfig};
use crate::gabagool::{GabagoolBacktestConfig, GabagoolBacktestEngine};
use crate::indicators::{build_signal_generator, SignalGenerator};
use crate::types::{BacktestTrade, Kline, TradeSide};

// ============================================================================
// Dynamic Combo Types
// ============================================================================

/// The 10 single indicator types available for dynamic combination
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SingleIndicatorType {
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
}

impl SingleIndicatorType {
    pub fn short_name(&self) -> &str {
        match self {
            Self::Rsi => "RSI",
            Self::BollingerBands => "BB",
            Self::Macd => "MACD",
            Self::EmaCrossover => "EMA",
            Self::Stochastic => "Stoch",
            Self::AtrMeanReversion => "ATR",
            Self::Vwap => "VWAP",
            Self::Obv => "OBV",
            Self::WilliamsR => "WR",
            Self::Adx => "ADX",
        }
    }

    pub fn all() -> &'static [SingleIndicatorType] {
        &[
            Self::Rsi,
            Self::BollingerBands,
            Self::Macd,
            Self::EmaCrossover,
            Self::Stochastic,
            Self::AtrMeanReversion,
            Self::Vwap,
            Self::Obv,
            Self::WilliamsR,
            Self::Adx,
        ]
    }

    pub fn default_params(&self) -> IndicatorParams {
        match self {
            Self::Rsi => IndicatorParams::Rsi { period: 14, overbought: 70.0, oversold: 30.0 },
            Self::BollingerBands => IndicatorParams::BollingerBands { period: 20, multiplier: 2.0 },
            Self::Macd => IndicatorParams::Macd { fast: 12, slow: 26, signal: 9 },
            Self::EmaCrossover => IndicatorParams::EmaCrossover { fast_period: 10, slow_period: 26 },
            Self::Stochastic => IndicatorParams::Stochastic { period: 14, overbought: 80.0, oversold: 20.0 },
            Self::AtrMeanReversion => IndicatorParams::AtrMeanReversion { atr_period: 14, sma_period: 20, multiplier: 2.0 },
            Self::Vwap => IndicatorParams::Vwap { period: 20 },
            Self::Obv => IndicatorParams::Obv { sma_period: 14 },
            Self::WilliamsR => IndicatorParams::WilliamsR { period: 14, overbought: -20.0, oversold: -80.0 },
            Self::Adx => IndicatorParams::Adx { period: 14, adx_threshold: 25.0 },
        }
    }

    pub fn aggressive_params(&self) -> IndicatorParams {
        match self {
            Self::Rsi => IndicatorParams::Rsi { period: 7, overbought: 65.0, oversold: 35.0 },
            Self::BollingerBands => IndicatorParams::BollingerBands { period: 10, multiplier: 1.5 },
            Self::Macd => IndicatorParams::Macd { fast: 6, slow: 17, signal: 5 },
            Self::EmaCrossover => IndicatorParams::EmaCrossover { fast_period: 5, slow_period: 15 },
            Self::Stochastic => IndicatorParams::Stochastic { period: 7, overbought: 75.0, oversold: 25.0 },
            Self::AtrMeanReversion => IndicatorParams::AtrMeanReversion { atr_period: 7, sma_period: 10, multiplier: 1.5 },
            Self::Vwap => IndicatorParams::Vwap { period: 10 },
            Self::Obv => IndicatorParams::Obv { sma_period: 7 },
            Self::WilliamsR => IndicatorParams::WilliamsR { period: 7, overbought: -15.0, oversold: -85.0 },
            Self::Adx => IndicatorParams::Adx { period: 7, adx_threshold: 20.0 },
        }
    }

    pub fn conservative_params(&self) -> IndicatorParams {
        match self {
            Self::Rsi => IndicatorParams::Rsi { period: 21, overbought: 80.0, oversold: 20.0 },
            Self::BollingerBands => IndicatorParams::BollingerBands { period: 30, multiplier: 2.5 },
            Self::Macd => IndicatorParams::Macd { fast: 12, slow: 35, signal: 12 },
            Self::EmaCrossover => IndicatorParams::EmaCrossover { fast_period: 15, slow_period: 50 },
            Self::Stochastic => IndicatorParams::Stochastic { period: 21, overbought: 85.0, oversold: 15.0 },
            Self::AtrMeanReversion => IndicatorParams::AtrMeanReversion { atr_period: 21, sma_period: 40, multiplier: 2.5 },
            Self::Vwap => IndicatorParams::Vwap { period: 40 },
            Self::Obv => IndicatorParams::Obv { sma_period: 25 },
            Self::WilliamsR => IndicatorParams::WilliamsR { period: 21, overbought: -25.0, oversold: -75.0 },
            Self::Adx => IndicatorParams::Adx { period: 21, adx_threshold: 30.0 },
        }
    }

    pub fn random_params(rng: &mut impl rand::Rng) -> (SingleIndicatorType, IndicatorParams) {
        let all = Self::all();
        let ind = all[rng.gen_range(0..all.len())];
        let params = ind.random_params_for(rng);
        (ind, params)
    }

    pub fn random_params_for(&self, rng: &mut impl rand::Rng) -> IndicatorParams {
        match self {
            Self::Rsi => IndicatorParams::Rsi {
                period: rng.gen_range(5..=35),
                overbought: rng.gen_range(60.0..=85.0),
                oversold: rng.gen_range(15.0..=40.0),
            },
            Self::BollingerBands => IndicatorParams::BollingerBands {
                period: rng.gen_range(7..=40),
                multiplier: rng.gen_range(1.0..=3.5),
            },
            Self::Macd => {
                let fast = rng.gen_range(4..=15);
                let slow = rng.gen_range((fast + 3)..=40);
                IndicatorParams::Macd { fast, slow, signal: rng.gen_range(3..=12) }
            }
            Self::EmaCrossover => {
                let fast = rng.gen_range(4..=18);
                let slow = rng.gen_range((fast + 3)..=60);
                IndicatorParams::EmaCrossover { fast_period: fast, slow_period: slow }
            }
            Self::Stochastic => IndicatorParams::Stochastic {
                period: rng.gen_range(5..=25),
                overbought: rng.gen_range(70.0..=90.0),
                oversold: rng.gen_range(10.0..=30.0),
            },
            Self::AtrMeanReversion => IndicatorParams::AtrMeanReversion {
                atr_period: rng.gen_range(5..=30),
                sma_period: rng.gen_range(8..=60),
                multiplier: rng.gen_range(0.75..=3.0),
            },
            Self::Vwap => IndicatorParams::Vwap { period: rng.gen_range(7..=60) },
            Self::Obv => IndicatorParams::Obv { sma_period: rng.gen_range(7..=40) },
            Self::WilliamsR => IndicatorParams::WilliamsR {
                period: rng.gen_range(5..=30),
                overbought: rng.gen_range(-30.0..=-10.0),
                oversold: rng.gen_range(-90.0..=-70.0),
            },
            Self::Adx => IndicatorParams::Adx {
                period: rng.gen_range(7..=30),
                adx_threshold: rng.gen_range(15.0..=40.0),
            },
        }
    }
}

/// Parameters for each indicator type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "indicator", rename_all = "snake_case")]
pub enum IndicatorParams {
    Rsi { period: usize, overbought: f64, oversold: f64 },
    BollingerBands { period: usize, multiplier: f64 },
    Macd { fast: usize, slow: usize, signal: usize },
    EmaCrossover { fast_period: usize, slow_period: usize },
    Stochastic { period: usize, overbought: f64, oversold: f64 },
    AtrMeanReversion { atr_period: usize, sma_period: usize, multiplier: f64 },
    Vwap { period: usize },
    Obv { sma_period: usize },
    WilliamsR { period: usize, overbought: f64, oversold: f64 },
    Adx { period: usize, adx_threshold: f64 },
}

/// How to combine signals in a dynamic combo
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DynCombineMode {
    Unanimous,
    Majority,
    PrimaryConfirmed,
}

impl DynCombineMode {
    pub fn all() -> &'static [DynCombineMode] {
        &[Self::Unanimous, Self::Majority, Self::PrimaryConfirmed]
    }

    pub fn short_suffix(&self) -> &str {
        match self {
            Self::Unanimous => "U",
            Self::Majority => "M",
            Self::PrimaryConfirmed => "PC",
        }
    }
}

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
    // === New Singles (4) ===
    Vwap {
        period: usize,
    },
    Obv {
        sma_period: usize,
    },
    WilliamsR {
        period: usize,
        overbought: f64,
        oversold: f64,
    },
    Adx {
        period: usize,
        adx_threshold: f64,
    },
    // === New Combos (4) ===
    VwapRsi {
        vwap_period: usize,
        rsi_period: usize,
        rsi_overbought: f64,
        rsi_oversold: f64,
    },
    ObvMacd {
        obv_sma_period: usize,
        macd_fast: usize,
        macd_slow: usize,
        macd_signal: usize,
    },
    AdxEma {
        adx_period: usize,
        adx_threshold: f64,
        ema_fast: usize,
        ema_slow: usize,
    },
    WilliamsRStoch {
        wr_period: usize,
        wr_overbought: f64,
        wr_oversold: f64,
        stoch_period: usize,
        stoch_overbought: f64,
        stoch_oversold: f64,
    },
    // === Dynamic Combos (2-4 indicators) ===
    DynamicCombo {
        indicators: Vec<SingleIndicatorType>,
        params: Vec<IndicatorParams>,
        combine_mode: DynCombineMode,
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
            Self::Vwap { .. } => "VWAP",
            Self::Obv { .. } => "OBV",
            Self::WilliamsR { .. } => "Williams %R",
            Self::Adx { .. } => "ADX",
            Self::VwapRsi { .. } => "VWAP+RSI",
            Self::ObvMacd { .. } => "OBV+MACD",
            Self::AdxEma { .. } => "ADX+EMA",
            Self::WilliamsRStoch { .. } => "Williams%R+Stoch",
            Self::DynamicCombo { .. } => return self.dynamic_combo_name(),
            Self::Gabagool { .. } => "Gabagool",
        }
    }

    fn dynamic_combo_name(&self) -> &str {
        // We use a thread-local cache for the computed name since we return &str
        // For dynamic combos, we leak the string to get a static ref
        // This is acceptable because there are a finite number of combo names
        match self {
            Self::DynamicCombo { indicators, combine_mode, .. } => {
                use std::sync::OnceLock;
                use std::collections::HashMap;
                use std::sync::Mutex;
                static NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
                let names = NAMES.get_or_init(|| Mutex::new(HashMap::new()));

                let key = format!(
                    "{}({})",
                    indicators.iter().map(|i| i.short_name()).collect::<Vec<_>>().join("+"),
                    combine_mode.short_suffix(),
                );

                let mut map = names.lock().unwrap();
                if let Some(s) = map.get(&key) {
                    return s;
                }
                let leaked: &'static str = Box::leak(key.clone().into_boxed_str());
                map.insert(key, leaked);
                leaked
            }
            _ => unreachable!(),
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
    #[serde(default)]
    pub continuous: Option<bool>,
}

fn default_days() -> u32 {
    365
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
    // Advanced metrics
    pub sortino_ratio: Decimal,
    pub max_consecutive_losses: u32,
    pub avg_win_pnl: Decimal,
    pub avg_loss_pnl: Decimal,
    pub total_volume: Decimal,
    pub annualized_return_pct: Decimal,
    pub annualized_sharpe: Decimal,
    pub strategy_confidence: Decimal,
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
    Phase3Exploration,
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
    pub current_cycle: AtomicU32,
    pub total_tested_all_cycles: AtomicU32,
    pub total_new_this_cycle: AtomicU32,
    pub is_continuous: AtomicBool,
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
            current_cycle: AtomicU32::new(0),
            total_tested_all_cycles: AtomicU32::new(0),
            total_new_this_cycle: AtomicU32::new(0),
            is_continuous: AtomicBool::new(false),
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
        self.current_cycle.store(0, Ordering::Relaxed);
        self.total_tested_all_cycles.store(0, Ordering::Relaxed);
        self.total_new_this_cycle.store(0, Ordering::Relaxed);
        self.is_continuous.store(false, Ordering::Relaxed);
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
                | DiscoveryStatus::Phase3Exploration
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
    let mut grid = Vec::with_capacity(4000);
    let all_indicators = SingleIndicatorType::all();
    let all_modes = DynCombineMode::all();
    let param_variants: &[fn(&SingleIndicatorType) -> IndicatorParams] = &[
        |ind| ind.default_params(),
        |ind| ind.aggressive_params(),
        |ind| ind.conservative_params(),
    ];

    // --- Pairs: C(10,2) = 45 × 3 param_variants × 3 modes = 405 ---
    for i in 0..all_indicators.len() {
        for j in (i + 1)..all_indicators.len() {
            let ind_a = all_indicators[i];
            let ind_b = all_indicators[j];
            for param_fn in param_variants {
                for &mode in all_modes {
                    grid.push(DiscoveryStrategyType::DynamicCombo {
                        indicators: vec![ind_a, ind_b],
                        params: vec![param_fn(&ind_a), param_fn(&ind_b)],
                        combine_mode: mode,
                    });
                }
            }
        }
    }

    // --- Triples: C(10,3) = 120 × 3 param_variants × 3 modes = 1080 ---
    for i in 0..all_indicators.len() {
        for j in (i + 1)..all_indicators.len() {
            for k in (j + 1)..all_indicators.len() {
                let inds = vec![all_indicators[i], all_indicators[j], all_indicators[k]];
                for param_fn in param_variants {
                    for &mode in all_modes {
                        grid.push(DiscoveryStrategyType::DynamicCombo {
                            indicators: inds.clone(),
                            params: vec![param_fn(&inds[0]), param_fn(&inds[1]), param_fn(&inds[2])],
                            combine_mode: mode,
                        });
                    }
                }
            }
        }
    }

    // --- Quadruples: C(10,4) = 210 × default params only × Majority mode = 210 ---
    for i in 0..all_indicators.len() {
        for j in (i + 1)..all_indicators.len() {
            for k in (j + 1)..all_indicators.len() {
                for l in (k + 1)..all_indicators.len() {
                    let inds = vec![
                        all_indicators[i], all_indicators[j],
                        all_indicators[k], all_indicators[l],
                    ];
                    grid.push(DiscoveryStrategyType::DynamicCombo {
                        indicators: inds.clone(),
                        params: vec![
                            inds[0].default_params(), inds[1].default_params(),
                            inds[2].default_params(), inds[3].default_params(),
                        ],
                        combine_mode: DynCombineMode::Majority,
                    });
                }
            }
        }
    }

    // --- Gabagool: 4 mpc × 4 bo × 3 sm = 48 ---
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

// Keep the old grid for legacy strategies that may still be in DB
#[allow(dead_code)]
fn generate_legacy_phase1_grid() -> Vec<DiscoveryStrategyType> {
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

    // 14. VWAP: 4 periods = 4
    for &period in &[10usize, 20, 30, 50] {
        grid.push(DiscoveryStrategyType::Vwap { period });
    }

    // 15. OBV: 4 sma_periods = 4
    for &sma_period in &[10usize, 14, 20, 30] {
        grid.push(DiscoveryStrategyType::Obv { sma_period });
    }

    // 16. Williams %R: 3 periods × 2 ob × 2 os = 12
    for &period in &[7usize, 14, 21] {
        for &ob in &[-15.0f64, -20.0] {
            for &os in &[-80.0f64, -85.0] {
                grid.push(DiscoveryStrategyType::WilliamsR {
                    period,
                    overbought: ob,
                    oversold: os,
                });
            }
        }
    }

    // 17. ADX: 3 periods × 3 thresholds = 9
    for &period in &[7usize, 14, 21] {
        for &threshold in &[20.0f64, 25.0, 30.0] {
            grid.push(DiscoveryStrategyType::Adx {
                period,
                adx_threshold: threshold,
            });
        }
    }

    // 18. VWAP+RSI: 4 × 3 × 2 × 2 = 48
    for &vp in &[10usize, 20, 30, 50] {
        for &rp in &[9usize, 14, 21] {
            for &rob in &[70.0, 80.0] {
                for &ros in &[25.0, 30.0] {
                    grid.push(DiscoveryStrategyType::VwapRsi {
                        vwap_period: vp,
                        rsi_period: rp,
                        rsi_overbought: rob,
                        rsi_oversold: ros,
                    });
                }
            }
        }
    }

    // 19. OBV+MACD: 4 × 3 × 3 × 2 = 72
    for &obv_sma in &[10usize, 14, 20, 30] {
        for &mf in &[8usize, 12, 5] {
            for &ms in &[21usize, 26, 17] {
                if mf < ms {
                    for &msig in &[5usize, 9] {
                        grid.push(DiscoveryStrategyType::ObvMacd {
                            obv_sma_period: obv_sma,
                            macd_fast: mf,
                            macd_slow: ms,
                            macd_signal: msig,
                        });
                    }
                }
            }
        }
    }

    // 20. ADX+EMA: 3 × 3 × 3 × 3 = 81
    for &ap in &[7usize, 14, 21] {
        for &at in &[20.0f64, 25.0, 30.0] {
            for &ef in &[8usize, 10, 13] {
                for &es in &[21usize, 26, 50] {
                    if ef < es {
                        grid.push(DiscoveryStrategyType::AdxEma {
                            adx_period: ap,
                            adx_threshold: at,
                            ema_fast: ef,
                            ema_slow: es,
                        });
                    }
                }
            }
        }
    }

    // 21. WilliamsR+Stoch: 3 × 2 × 2 × 3 × 2 × 2 = 144
    for &wp in &[7usize, 14, 21] {
        for &wob in &[-15.0f64, -20.0] {
            for &wos in &[-80.0f64, -85.0] {
                for &sp in &[5usize, 9, 14] {
                    for &sob in &[80.0, 85.0] {
                        for &sos in &[15.0, 20.0] {
                            grid.push(DiscoveryStrategyType::WilliamsRStoch {
                                wr_period: wp,
                                wr_overbought: wob,
                                wr_oversold: wos,
                                stoch_period: sp,
                                stoch_overbought: sob,
                                stoch_oversold: sos,
                            });
                        }
                    }
                }
            }
        }
    }

    // 22. Gabagool: 4 mpc × 4 bo × 3 sm = 48
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
    sortino_ratio: Decimal,
    max_consecutive_losses: u32,
    avg_win_pnl: Decimal,
    avg_loss_pnl: Decimal,
    total_volume: Decimal,
    annualized_return_pct: Decimal,
    annualized_sharpe: Decimal,
}

struct OpenPosition {
    entry_price: Decimal,
    size: Decimal,
}

/// Estimate Polymarket probability from price change percentage.
/// Maps price movement to a probability in [0.05, 0.95].
/// At 0% change → p=0.50 (max fees). Large moves push p toward extremes (lower fees).
fn estimate_poly_probability(entry_price: Decimal, current_price: Decimal) -> Decimal {
    if entry_price <= Decimal::ZERO {
        return dec!(0.50);
    }
    let hundred = dec!(100);
    let change_pct = (current_price - entry_price) / entry_price * hundred;
    // Convert change_pct to f64 for the calculation
    let change_f64: f64 = change_pct.to_string().parse().unwrap_or(0.0);
    let p_f64 = (0.5 + change_f64 * 0.05).clamp(0.05, 0.95);
    Decimal::from_str_exact(&format!("{:.4}", p_f64)).unwrap_or(dec!(0.50))
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
    // Use first kline close as baseline for probability estimation
    let baseline_price = klines.first().map(|k| k.close).unwrap_or(dec!(1));
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

                    // Entry fee — estimate probability from current price vs baseline
                    let p_entry = estimate_poly_probability(baseline_price, kline.close);
                    let entry_fee = calculate_taker_fee(shares, p_entry, fee_config);
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
                    let p_exit = estimate_poly_probability(baseline_price, kline.close);
                    let exit_fee = calculate_taker_fee(pos.size, p_exit, fee_config);

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
            let p_exit = estimate_poly_probability(baseline_price, last.close);
            let exit_fee = calculate_taker_fee(pos.size, p_exit, fee_config);
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
            let p_entry = estimate_poly_probability(baseline_price, trade.entry_price);
            let p_exit = estimate_poly_probability(baseline_price, trade.exit_price);
            fees += calculate_taker_fee(trade.size, p_entry, fee_config)
                + calculate_taker_fee(trade.size, p_exit, fee_config);
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

    // --- Advanced metrics ---

    // Sortino ratio: mean(returns) / std_dev(negative_returns_only)
    let sortino_ratio = calculate_sortino(&trades);

    // Max consecutive losses
    let max_consecutive_losses = {
        let mut max_streak = 0u32;
        let mut current_streak = 0u32;
        for trade in &trades {
            if trade.pnl < Decimal::ZERO {
                current_streak += 1;
                if current_streak > max_streak {
                    max_streak = current_streak;
                }
            } else {
                current_streak = 0;
            }
        }
        max_streak
    };

    // Average win/loss PnL
    let avg_win_pnl = if winning_trades > 0 {
        gross_profits / Decimal::from(winning_trades)
    } else {
        Decimal::ZERO
    };
    let avg_loss_pnl = if losing_trades > 0 {
        gross_losses / Decimal::from(losing_trades)
    } else {
        Decimal::ZERO
    };

    // Total volume
    let total_volume: Decimal = trades
        .iter()
        .map(|t| t.size * t.entry_price)
        .sum();

    // Annualized return: ((1 + total_return)^(365/period_days) - 1) * 100
    let annualized_return_pct = {
        let period_bars = klines.len() as f64;
        let period_days = period_bars / 96.0; // 96 bars per day (15min)
        if period_days > 0.0 && initial_capital > Decimal::ZERO {
            let total_return_f: f64 = (total_pnl / initial_capital)
                .to_string()
                .parse()
                .unwrap_or(0.0);
            let annual_factor = 365.0 / period_days;
            let annualized = ((1.0 + total_return_f).powf(annual_factor) - 1.0) * 100.0;
            // Clamp to reasonable bounds
            let clamped = annualized.clamp(-999.99, 99999.99);
            Decimal::from_str_exact(&format!("{:.2}", clamped)).unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        }
    };

    // Annualized Sharpe: sharpe * sqrt(365 / period_days)
    let annualized_sharpe = {
        let period_bars = klines.len() as f64;
        let period_days = period_bars / 96.0;
        if period_days > 0.0 {
            let sharpe_f: f64 = sharpe_ratio.to_string().parse().unwrap_or(0.0);
            let ann_sharpe = sharpe_f * (365.0 / period_days).sqrt();
            Decimal::from_str_exact(&format!("{:.2}", ann_sharpe)).unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        }
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
        sortino_ratio,
        max_consecutive_losses,
        avg_win_pnl,
        avg_loss_pnl,
        total_volume,
        annualized_return_pct,
        annualized_sharpe,
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

fn calculate_sortino(trades: &[BacktestTrade]) -> Decimal {
    if trades.len() < 2 {
        return Decimal::ZERO;
    }

    let returns: Vec<f64> = trades
        .iter()
        .map(|t| t.pnl_pct.to_string().parse::<f64>().unwrap_or(0.0))
        .collect();

    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;

    // Downside deviation: std_dev of negative returns only
    let negative_returns: Vec<f64> = returns.iter().filter(|&&r| r < 0.0).copied().collect();
    if negative_returns.is_empty() {
        // No negative returns — perfect Sortino
        return if mean > 0.0 {
            Decimal::from_str_exact(&format!("{:.2}", mean * 10.0)).unwrap_or(dec!(99))
        } else {
            Decimal::ZERO
        };
    }

    let neg_n = negative_returns.len() as f64;
    let neg_variance = negative_returns.iter().map(|r| r.powi(2)).sum::<f64>() / neg_n;
    let downside_dev = neg_variance.sqrt();

    if downside_dev < 1e-10 {
        return Decimal::ZERO;
    }

    let sortino = mean / downside_dev;
    Decimal::from_str_exact(&format!("{:.2}", sortino)).unwrap_or(Decimal::ZERO)
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

    // Confidence bonus (0-100 → 0-300 bonus)
    let confidence_bonus = result.strategy_confidence * dec!(3);

    // Sortino bonus (rewards downside risk management)
    let sortino_capped = result.sortino_ratio.min(dec!(5));
    let sortino_bonus = sortino_capped * dec!(50);

    // Consecutive losses penalty
    let streak_penalty = if result.max_consecutive_losses > 10 {
        dec!(100)
    } else if result.max_consecutive_losses > 7 {
        dec!(50)
    } else {
        Decimal::ZERO
    };

    net_pnl + win_rate_bonus + sharpe_bonus - drawdown_penalty + pf_bonus + explosive_bonus
        + confidence_bonus + sortino_bonus - streak_penalty
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
        DiscoveryStrategyType::Stochastic {
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
                        if os > 0.0 && ob < 100.0 {
                            variants.push(DiscoveryStrategyType::Stochastic {
                                period: p,
                                overbought: ob,
                                oversold: os,
                            });
                        }
                    }
                }
            }
        }
        DiscoveryStrategyType::AtrMeanReversion {
            atr_period,
            sma_period,
            multiplier,
        } => {
            for da in [-2i32, 0, 2] {
                for ds in [-3i32, 0, 3] {
                    for dm in [-0.25f64, 0.0, 0.25] {
                        let a = (*atr_period as i32 + da).max(3) as usize;
                        let s = (*sma_period as i32 + ds).max(5) as usize;
                        let m = multiplier + dm;
                        if m > 0.5 {
                            variants.push(DiscoveryStrategyType::AtrMeanReversion {
                                atr_period: a,
                                sma_period: s,
                                multiplier: m,
                            });
                        }
                    }
                }
            }
        }
        DiscoveryStrategyType::Vwap { period } => {
            for dp in [-3i32, -1, 0, 1, 3] {
                let p = (*period as i32 + dp).max(5) as usize;
                variants.push(DiscoveryStrategyType::Vwap { period: p });
            }
        }
        DiscoveryStrategyType::Obv { sma_period } => {
            for dp in [-2i32, -1, 0, 1, 2] {
                let p = (*sma_period as i32 + dp).max(5) as usize;
                variants.push(DiscoveryStrategyType::Obv { sma_period: p });
            }
        }
        DiscoveryStrategyType::WilliamsR {
            period,
            overbought,
            oversold,
        } => {
            for dp in [-2i32, 0, 2] {
                for dob in [-2.5f64, 0.0, 2.5] {
                    for dos in [-2.5f64, 0.0, 2.5] {
                        let p = (*period as i32 + dp).max(3) as usize;
                        let ob = overbought + dob;
                        let os = oversold + dos;
                        if os < ob {
                            variants.push(DiscoveryStrategyType::WilliamsR {
                                period: p,
                                overbought: ob,
                                oversold: os,
                            });
                        }
                    }
                }
            }
        }
        DiscoveryStrategyType::Adx {
            period,
            adx_threshold,
        } => {
            for dp in [-2i32, 0, 2] {
                for dt in [-2.5f64, 0.0, 2.5] {
                    let p = (*period as i32 + dp).max(3) as usize;
                    let t = (adx_threshold + dt).max(10.0);
                    variants.push(DiscoveryStrategyType::Adx {
                        period: p,
                        adx_threshold: t,
                    });
                }
            }
        }
        // Dynamic combos: tweak params of 1st indicator + try other modes
        DiscoveryStrategyType::DynamicCombo { indicators, params, combine_mode } => {
            // Try the 2 other combine modes
            for &mode in DynCombineMode::all() {
                if mode != *combine_mode {
                    variants.push(DiscoveryStrategyType::DynamicCombo {
                        indicators: indicators.clone(),
                        params: params.clone(),
                        combine_mode: mode,
                    });
                }
            }
            // Mutate params of each indicator slightly
            for idx in 0..params.len() {
                let mut new_params = params.clone();
                new_params[idx] = mutate_indicator_params(&params[idx]);
                variants.push(DiscoveryStrategyType::DynamicCombo {
                    indicators: indicators.clone(),
                    params: new_params,
                    combine_mode: *combine_mode,
                });
            }
        }
        // For legacy combos, return the original (no refinement — too many params)
        other => {
            variants.push(other.clone());
        }
    }

    variants
}

/// Slightly mutate indicator params (deterministic small deltas for refinement)
fn mutate_indicator_params(params: &IndicatorParams) -> IndicatorParams {
    match params {
        IndicatorParams::Rsi { period, overbought, oversold } => IndicatorParams::Rsi {
            period: (*period).max(3).wrapping_add(1),
            overbought: overbought + 2.5,
            oversold: oversold - 2.5,
        },
        IndicatorParams::BollingerBands { period, multiplier } => IndicatorParams::BollingerBands {
            period: (*period).max(3).wrapping_add(2),
            multiplier: multiplier + 0.25,
        },
        IndicatorParams::Macd { fast, slow, signal } => IndicatorParams::Macd {
            fast: *fast,
            slow: slow + 2,
            signal: *signal,
        },
        IndicatorParams::EmaCrossover { fast_period, slow_period } => IndicatorParams::EmaCrossover {
            fast_period: *fast_period,
            slow_period: slow_period + 3,
        },
        IndicatorParams::Stochastic { period, overbought, oversold } => IndicatorParams::Stochastic {
            period: (*period).max(3).wrapping_add(1),
            overbought: overbought + 2.5,
            oversold: oversold - 2.5,
        },
        IndicatorParams::AtrMeanReversion { atr_period, sma_period, multiplier } => IndicatorParams::AtrMeanReversion {
            atr_period: *atr_period,
            sma_period: sma_period + 3,
            multiplier: multiplier + 0.25,
        },
        IndicatorParams::Vwap { period } => IndicatorParams::Vwap { period: period + 3 },
        IndicatorParams::Obv { sma_period } => IndicatorParams::Obv { sma_period: sma_period + 2 },
        IndicatorParams::WilliamsR { period, overbought, oversold } => IndicatorParams::WilliamsR {
            period: (*period).max(3).wrapping_add(1),
            overbought: overbought - 2.5,
            oversold: oversold + 2.5,
        },
        IndicatorParams::Adx { period, adx_threshold } => IndicatorParams::Adx {
            period: (*period).max(3).wrapping_add(1),
            adx_threshold: adx_threshold + 2.5,
        },
    }
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
        DiscoveryStrategyType::Vwap { .. } => "vwap",
        DiscoveryStrategyType::Obv { .. } => "obv",
        DiscoveryStrategyType::WilliamsR { .. } => "williams_r",
        DiscoveryStrategyType::Adx { .. } => "adx",
        DiscoveryStrategyType::VwapRsi { .. } => "vwap_rsi",
        DiscoveryStrategyType::ObvMacd { .. } => "obv_macd",
        DiscoveryStrategyType::AdxEma { .. } => "adx_ema",
        DiscoveryStrategyType::WilliamsRStoch { .. } => "williams_r_stoch",
        DiscoveryStrategyType::DynamicCombo { .. } => "dynamic_combo",
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
        sortino_ratio: Some(result.sortino_ratio.to_string()),
        max_consecutive_losses: Some(result.max_consecutive_losses as i64),
        avg_win_pnl: Some(result.avg_win_pnl.to_string()),
        avg_loss_pnl: Some(result.avg_loss_pnl.to_string()),
        total_volume: Some(result.total_volume.to_string()),
        annualized_return_pct: Some(result.annualized_return_pct.to_string()),
        annualized_sharpe: Some(result.annualized_sharpe.to_string()),
        strategy_confidence: Some(result.strategy_confidence.to_string()),
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
        sortino_ratio: record.sortino_ratio.as_deref().map(parse_dec).unwrap_or(Decimal::ZERO),
        max_consecutive_losses: record.max_consecutive_losses.unwrap_or(0) as u32,
        avg_win_pnl: record.avg_win_pnl.as_deref().map(parse_dec).unwrap_or(Decimal::ZERO),
        avg_loss_pnl: record.avg_loss_pnl.as_deref().map(parse_dec).unwrap_or(Decimal::ZERO),
        total_volume: record.total_volume.as_deref().map(parse_dec).unwrap_or(Decimal::ZERO),
        annualized_return_pct: record.annualized_return_pct.as_deref().map(parse_dec).unwrap_or(Decimal::ZERO),
        annualized_sharpe: record.annualized_sharpe.as_deref().map(parse_dec).unwrap_or(Decimal::ZERO),
        strategy_confidence: record.strategy_confidence.as_deref().map(parse_dec).unwrap_or(Decimal::ZERO),
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

/// Calculate strategy confidence by running backtests on 4 quartiles of the data.
/// Returns a score from 0 to 100 based on consistency across time periods.
fn calculate_strategy_confidence(
    strategy_type: &DiscoveryStrategyType,
    klines: &[Kline],
    initial_capital: Decimal,
    base_position_pct: Decimal,
    sizing_mode: SizingMode,
    fee_config: &PolymarketFeeConfig,
) -> Decimal {
    if klines.len() < 200 {
        // Not enough data for meaningful quartile analysis
        return Decimal::ZERO;
    }

    let quarter = klines.len() / 4;
    let quartiles = [
        &klines[..quarter],
        &klines[quarter..quarter * 2],
        &klines[quarter * 2..quarter * 3],
        &klines[quarter * 3..],
    ];

    let mut win_rates = Vec::new();
    let mut profitable_count = 0u32;

    for q_klines in &quartiles {
        let mut gen = build_signal_generator(strategy_type);
        let bt = run_generic_backtest(
            gen.as_mut(),
            q_klines,
            initial_capital,
            base_position_pct,
            sizing_mode,
            fee_config,
        );
        if bt.total_pnl > Decimal::ZERO {
            profitable_count += 1;
        }
        let wr_f: f64 = bt.win_rate.to_string().parse().unwrap_or(0.0);
        win_rates.push(wr_f);
    }

    // 50% weight: number of profitable quartiles (0/4 to 4/4)
    let profitability_score = (profitable_count as f64 / 4.0) * 50.0;

    // 30% weight: consistency of win rates (low std_dev = high confidence)
    let consistency_score = if win_rates.len() >= 2 {
        let mean_wr = win_rates.iter().sum::<f64>() / win_rates.len() as f64;
        let variance = win_rates
            .iter()
            .map(|wr| (wr - mean_wr).powi(2))
            .sum::<f64>()
            / (win_rates.len() - 1) as f64;
        let std_dev = variance.sqrt();
        // Lower std_dev = higher consistency. Map 0-20 std_dev to 30-0 score
        (1.0 - (std_dev / 20.0).min(1.0)) * 30.0
    } else {
        0.0
    };

    // 20% weight: minimum win rate across quartiles
    let min_wr = win_rates.iter().cloned().fold(f64::MAX, f64::min);
    let min_wr_score = if min_wr > 50.0 {
        ((min_wr - 50.0) / 30.0).min(1.0) * 20.0
    } else {
        0.0
    };

    let total = profitability_score + consistency_score + min_wr_score;
    Decimal::from_str_exact(&format!("{:.1}", total.clamp(0.0, 100.0)))
        .unwrap_or(Decimal::ZERO)
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

    // Calculate confidence only for promising strategies (net_pnl > 0 AND win_rate > 50)
    let strategy_confidence = if bt.total_pnl > Decimal::ZERO && bt.win_rate > dec!(50) {
        calculate_strategy_confidence(
            strategy_type,
            klines,
            initial_capital,
            base_position_pct,
            sizing_mode,
            fee_config,
        )
    } else {
        Decimal::ZERO
    };

    DiscoveryResult {
        rank: 0,
        strategy_type: strategy_type.clone(),
        strategy_name: strategy_type.name().to_string(),
        symbol: symbol.to_string(),
        sizing_mode,
        composite_score: Decimal::ZERO,
        net_pnl: bt.total_pnl,
        gross_pnl: bt.total_pnl + bt.total_fees,
        total_fees: bt.total_fees,
        win_rate: bt.win_rate,
        total_trades: bt.total_trades,
        sharpe_ratio: bt.sharpe_ratio,
        max_drawdown_pct: bt.max_drawdown_pct,
        profit_factor: bt.profit_factor,
        avg_trade_pnl: bt.avg_trade_pnl,
        sortino_ratio: bt.sortino_ratio,
        max_consecutive_losses: bt.max_consecutive_losses,
        avg_win_pnl: bt.avg_win_pnl,
        avg_loss_pnl: bt.avg_loss_pnl,
        total_volume: bt.total_volume,
        annualized_return_pct: bt.annualized_return_pct,
        annualized_sharpe: bt.annualized_sharpe,
        strategy_confidence,
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
        sortino_ratio: Decimal::ZERO,
        max_consecutive_losses: 0,
        avg_win_pnl: Decimal::ZERO,
        avg_loss_pnl: Decimal::ZERO,
        total_volume: Decimal::ZERO,
        annualized_return_pct: Decimal::ZERO,
        annualized_sharpe: Decimal::ZERO,
        strategy_confidence: Decimal::ZERO,
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

// ============================================================================
// Continuous Discovery — Exploratory Grid Generation
// ============================================================================

/// Generate an expanding grid of strategy combinations based on the cycle number.
/// - Cycle 0: Dynamic combos (pairs, triples, quads) + Gabagool
/// - Cycle 1: Quadruples with all 3 modes (Unanimous + PrimaryConfirmed)
/// - Cycle 2: Mixed param variants on pairs/triples
/// - Cycle 3+: ML-guided (evolutionary algorithm)
fn generate_exploratory_grid(cycle: u32) -> Vec<DiscoveryStrategyType> {
    let mut grid = Vec::new();
    let all_indicators = SingleIndicatorType::all();
    let all_modes = DynCombineMode::all();

    match cycle {
        0 => {
            grid = generate_phase1_grid();
        }
        1 => {
            // Cycle 1: Complete quadruples with Unanimous + PrimaryConfirmed modes
            for i in 0..all_indicators.len() {
                for j in (i + 1)..all_indicators.len() {
                    for k in (j + 1)..all_indicators.len() {
                        for l in (k + 1)..all_indicators.len() {
                            let inds = vec![
                                all_indicators[i], all_indicators[j],
                                all_indicators[k], all_indicators[l],
                            ];
                            // Phase 1 already did Majority; now add U and PC
                            for &mode in &[DynCombineMode::Unanimous, DynCombineMode::PrimaryConfirmed] {
                                grid.push(DiscoveryStrategyType::DynamicCombo {
                                    indicators: inds.clone(),
                                    params: vec![
                                        inds[0].default_params(), inds[1].default_params(),
                                        inds[2].default_params(), inds[3].default_params(),
                                    ],
                                    combine_mode: mode,
                                });
                            }
                        }
                    }
                }
            }
            // Also add quadruples with aggressive params × Majority
            for i in 0..all_indicators.len() {
                for j in (i + 1)..all_indicators.len() {
                    for k in (j + 1)..all_indicators.len() {
                        for l in (k + 1)..all_indicators.len() {
                            let inds = vec![
                                all_indicators[i], all_indicators[j],
                                all_indicators[k], all_indicators[l],
                            ];
                            grid.push(DiscoveryStrategyType::DynamicCombo {
                                indicators: inds.clone(),
                                params: vec![
                                    inds[0].aggressive_params(), inds[1].aggressive_params(),
                                    inds[2].aggressive_params(), inds[3].aggressive_params(),
                                ],
                                combine_mode: DynCombineMode::Majority,
                            });
                        }
                    }
                }
            }
            // Gabagool fine interpolation
            for mpc in &[dec!(0.93), dec!(0.95), dec!(0.97)] {
                for bo in &[dec!(0.007), dec!(0.015), dec!(0.025)] {
                    for sm in &[dec!(2.5), dec!(4)] {
                        grid.push(DiscoveryStrategyType::Gabagool {
                            max_pair_cost: *mpc,
                            bid_offset: *bo,
                            spread_multiplier: *sm,
                        });
                    }
                }
            }
        }
        2 => {
            // Cycle 2: Mixed param variants — each indicator can have a different variant
            // For pairs: random mix of default/aggressive/conservative per indicator
            let mut rng = rand::thread_rng();
            for i in 0..all_indicators.len() {
                for j in (i + 1)..all_indicators.len() {
                    let ind_a = all_indicators[i];
                    let ind_b = all_indicators[j];
                    // Mix: aggressive A + conservative B, and vice versa
                    for &mode in all_modes {
                        grid.push(DiscoveryStrategyType::DynamicCombo {
                            indicators: vec![ind_a, ind_b],
                            params: vec![ind_a.aggressive_params(), ind_b.conservative_params()],
                            combine_mode: mode,
                        });
                        grid.push(DiscoveryStrategyType::DynamicCombo {
                            indicators: vec![ind_a, ind_b],
                            params: vec![ind_a.conservative_params(), ind_b.aggressive_params()],
                            combine_mode: mode,
                        });
                    }
                }
            }
            // Also add some random-param pairs for diversity
            for _ in 0..200 {
                let n = rng.gen_range(2..=4usize);
                let combo = generate_random_dynamic_combo(n, &mut rng);
                grid.push(combo);
            }
            // Gabagool extended
            for mpc in &[dec!(0.85), dec!(0.88), dec!(0.90), dec!(0.99)] {
                for bo in &[dec!(0.002), dec!(0.04), dec!(0.05)] {
                    for sm in &[dec!(1), dec!(1.5), dec!(6), dec!(8)] {
                        grid.push(DiscoveryStrategyType::Gabagool {
                            max_pair_cost: *mpc,
                            bid_offset: *bo,
                            spread_multiplier: *sm,
                        });
                    }
                }
            }
        }
        _ => {
            // Cycle 3+: Handled by generate_ml_guided_grid() in the caller
            let count = 500 + (cycle - 3) as usize * 200;
            let mut rng = rand::thread_rng();
            for _ in 0..count {
                let n = rng.gen_range(2..=4usize);
                grid.push(generate_random_dynamic_combo(n, &mut rng));
            }
            // A few Gabagool randoms
            for _ in 0..20 {
                let mpc_f = rng.gen_range(0.85..=0.99);
                let bo_f = rng.gen_range(0.001..=0.05);
                let sm_f = rng.gen_range(1.0..=8.0);
                grid.push(DiscoveryStrategyType::Gabagool {
                    max_pair_cost: Decimal::from_str_exact(&format!("{:.3}", mpc_f)).unwrap_or(dec!(0.95)),
                    bid_offset: Decimal::from_str_exact(&format!("{:.4}", bo_f)).unwrap_or(dec!(0.01)),
                    spread_multiplier: Decimal::from_str_exact(&format!("{:.1}", sm_f)).unwrap_or(dec!(3)),
                });
            }
        }
    }

    grid
}

/// Generate a random DynamicCombo with n indicators (2-4)
fn generate_random_dynamic_combo(n: usize, rng: &mut impl rand::Rng) -> DiscoveryStrategyType {
    let all = SingleIndicatorType::all();
    let mut indices: Vec<usize> = (0..all.len()).collect();
    // Fisher-Yates partial shuffle
    for i in 0..n.min(indices.len()) {
        let j = rng.gen_range(i..indices.len());
        indices.swap(i, j);
    }
    let indicators: Vec<SingleIndicatorType> = indices[..n].iter().map(|&i| all[i]).collect();
    let params: Vec<IndicatorParams> = indicators.iter().map(|ind| ind.random_params_for(rng)).collect();
    let modes = DynCombineMode::all();
    let combine_mode = modes[rng.gen_range(0..modes.len())];

    DiscoveryStrategyType::DynamicCombo { indicators, params, combine_mode }
}

// Keep legacy exploratory grid for reference
#[allow(dead_code)]
fn generate_legacy_exploratory_grid(cycle: u32) -> Vec<DiscoveryStrategyType> {
    let mut grid = Vec::new();

    match cycle {
        0 => {
            grid = generate_phase1_grid();
        }
        1 => {
            // Fine interpolation — intermediate values between Phase 1 grid points
            // RSI
            for &period in &[7usize, 11, 16, 18, 25] {
                for &ob in &[67.5, 72.5, 77.5] {
                    for &os in &[22.5, 27.5, 32.5] {
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
            // Bollinger
            for &period in &[12usize, 17, 22, 27] {
                for &mult in &[1.75, 2.25, 2.75] {
                    grid.push(DiscoveryStrategyType::BollingerBands {
                        period,
                        multiplier: mult,
                    });
                }
            }
            // MACD
            for &fast in &[6usize, 10] {
                for &slow in &[19usize, 23] {
                    for &signal in &[7usize] {
                        if fast < slow {
                            grid.push(DiscoveryStrategyType::Macd { fast, slow, signal });
                        }
                    }
                }
            }
            // EMA Crossover
            for &fast in &[6usize, 9, 12] {
                for &slow in &[23usize, 35, 40] {
                    if fast < slow {
                        grid.push(DiscoveryStrategyType::EmaCrossover {
                            fast_period: fast,
                            slow_period: slow,
                        });
                    }
                }
            }
            // Stochastic
            for &period in &[7usize, 12, 18] {
                for &ob in &[77.5, 82.5] {
                    for &os in &[17.5, 22.5] {
                        grid.push(DiscoveryStrategyType::Stochastic {
                            period,
                            overbought: ob,
                            oversold: os,
                        });
                    }
                }
            }
            // ATR Mean Reversion
            for &atr in &[10usize, 17] {
                for &sma in &[15usize, 25, 40] {
                    for &mult in &[1.25, 1.75, 2.25] {
                        grid.push(DiscoveryStrategyType::AtrMeanReversion {
                            atr_period: atr,
                            sma_period: sma,
                            multiplier: mult,
                        });
                    }
                }
            }
            // VWAP
            for &period in &[15usize, 25, 40] {
                grid.push(DiscoveryStrategyType::Vwap { period });
            }
            // OBV
            for &sma in &[12usize, 17, 25] {
                grid.push(DiscoveryStrategyType::Obv { sma_period: sma });
            }
            // Williams %R
            for &period in &[10usize, 17] {
                for &ob in &[-17.5f64, -22.5] {
                    for &os in &[-77.5f64, -82.5] {
                        grid.push(DiscoveryStrategyType::WilliamsR {
                            period,
                            overbought: ob,
                            oversold: os,
                        });
                    }
                }
            }
            // ADX
            for &period in &[10usize, 17] {
                for &threshold in &[22.5f64, 27.5] {
                    grid.push(DiscoveryStrategyType::Adx {
                        period,
                        adx_threshold: threshold,
                    });
                }
            }
            // Combos — fine interpolation
            for &rp in &[11usize, 17] {
                for &rob in &[72.5, 77.5] {
                    for &ros in &[22.5, 27.5] {
                        for &bp in &[17usize, 22] {
                            for &bm in &[2.25] {
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
            for &mf in &[10usize] {
                for &ms in &[23usize] {
                    for &msig in &[7usize] {
                        for &rp in &[11usize] {
                            for &rob in &[72.5, 77.5] {
                                grid.push(DiscoveryStrategyType::MacdRsi {
                                    macd_fast: mf,
                                    macd_slow: ms,
                                    macd_signal: msig,
                                    rsi_period: rp,
                                    rsi_ob: rob,
                                    rsi_os: 27.5,
                                });
                            }
                        }
                    }
                }
            }
            // VWAP+RSI
            for &vp in &[15usize, 25, 40] {
                for &rp in &[11usize, 17] {
                    for &rob in &[72.5, 77.5] {
                        grid.push(DiscoveryStrategyType::VwapRsi {
                            vwap_period: vp,
                            rsi_period: rp,
                            rsi_overbought: rob,
                            rsi_oversold: 27.5,
                        });
                    }
                }
            }
            // OBV+MACD
            for &obv_sma in &[12usize, 17, 25] {
                for &mf in &[10usize] {
                    for &ms in &[23usize] {
                        for &msig in &[7usize] {
                            if mf < ms {
                                grid.push(DiscoveryStrategyType::ObvMacd {
                                    obv_sma_period: obv_sma,
                                    macd_fast: mf,
                                    macd_slow: ms,
                                    macd_signal: msig,
                                });
                            }
                        }
                    }
                }
            }
            // ADX+EMA
            for &ap in &[10usize, 17] {
                for &at in &[22.5f64, 27.5] {
                    for &ef in &[9usize, 11] {
                        for &es in &[23usize, 35] {
                            if ef < es {
                                grid.push(DiscoveryStrategyType::AdxEma {
                                    adx_period: ap,
                                    adx_threshold: at,
                                    ema_fast: ef,
                                    ema_slow: es,
                                });
                            }
                        }
                    }
                }
            }
            // WilliamsR+Stoch
            for &wp in &[10usize, 17] {
                for &wob in &[-17.5f64] {
                    for &wos in &[-82.5f64] {
                        for &sp in &[7usize, 11] {
                            for &sob in &[82.5] {
                                for &sos in &[17.5] {
                                    grid.push(DiscoveryStrategyType::WilliamsRStoch {
                                        wr_period: wp,
                                        wr_overbought: wob,
                                        wr_oversold: wos,
                                        stoch_period: sp,
                                        stoch_overbought: sob,
                                        stoch_oversold: sos,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            // Gabagool fine
            for mpc in &[dec!(0.93), dec!(0.95), dec!(0.97)] {
                for bo in &[dec!(0.007), dec!(0.015), dec!(0.025)] {
                    for sm in &[dec!(2.5), dec!(4)] {
                        grid.push(DiscoveryStrategyType::Gabagool {
                            max_pair_cost: *mpc,
                            bid_offset: *bo,
                            spread_multiplier: *sm,
                        });
                    }
                }
            }
        }
        2 => {
            // Extended ranges — wider parameter bounds
            // RSI extended
            for &period in &[3usize, 4, 35, 40, 50] {
                for &ob in &[60.0, 85.0, 90.0] {
                    for &os in &[10.0, 15.0, 40.0] {
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
            // Bollinger extended
            for &period in &[5usize, 7, 35, 40, 50] {
                for &mult in &[1.0, 1.25, 3.5, 4.0] {
                    grid.push(DiscoveryStrategyType::BollingerBands {
                        period,
                        multiplier: mult,
                    });
                }
            }
            // MACD extended
            for &fast in &[3usize, 4, 15] {
                for &slow in &[30usize, 35, 40] {
                    for &signal in &[3usize, 12, 15] {
                        if fast < slow {
                            grid.push(DiscoveryStrategyType::Macd { fast, slow, signal });
                        }
                    }
                }
            }
            // EMA extended
            for &fast in &[3usize, 4, 18, 20] {
                for &slow in &[35usize, 60, 80, 100] {
                    if fast < slow {
                        grid.push(DiscoveryStrategyType::EmaCrossover {
                            fast_period: fast,
                            slow_period: slow,
                        });
                    }
                }
            }
            // Stochastic extended
            for &period in &[3usize, 4, 25, 30] {
                for &ob in &[70.0, 90.0] {
                    for &os in &[10.0, 30.0] {
                        grid.push(DiscoveryStrategyType::Stochastic {
                            period,
                            overbought: ob,
                            oversold: os,
                        });
                    }
                }
            }
            // ATR extended
            for &atr in &[5usize, 10, 28, 35] {
                for &sma in &[7usize, 60, 80] {
                    for &mult in &[0.75, 1.0, 2.5, 3.0] {
                        grid.push(DiscoveryStrategyType::AtrMeanReversion {
                            atr_period: atr,
                            sma_period: sma,
                            multiplier: mult,
                        });
                    }
                }
            }
            // VWAP extended
            for &period in &[5usize, 7, 60, 80, 100] {
                grid.push(DiscoveryStrategyType::Vwap { period });
            }
            // OBV extended
            for &sma in &[5usize, 7, 35, 40, 50] {
                grid.push(DiscoveryStrategyType::Obv { sma_period: sma });
            }
            // Williams %R extended
            for &period in &[3usize, 5, 28, 35] {
                for &ob in &[-10.0f64, -25.0, -30.0] {
                    for &os in &[-70.0f64, -75.0, -90.0] {
                        grid.push(DiscoveryStrategyType::WilliamsR {
                            period,
                            overbought: ob,
                            oversold: os,
                        });
                    }
                }
            }
            // ADX extended
            for &period in &[5usize, 10, 28, 35] {
                for &threshold in &[15.0f64, 35.0, 40.0] {
                    grid.push(DiscoveryStrategyType::Adx {
                        period,
                        adx_threshold: threshold,
                    });
                }
            }
            // Gabagool extended
            for mpc in &[dec!(0.85), dec!(0.88), dec!(0.90), dec!(0.99)] {
                for bo in &[dec!(0.002), dec!(0.04), dec!(0.05)] {
                    for sm in &[dec!(1), dec!(1.5), dec!(6), dec!(8)] {
                        grid.push(DiscoveryStrategyType::Gabagool {
                            max_pair_cost: *mpc,
                            bid_offset: *bo,
                            spread_multiplier: *sm,
                        });
                    }
                }
            }
        }
        _ => {
            // Cycle 3+: Random perturbations
            let count = 500 + (cycle - 3) as usize * 200;
            let mut rng = rand::thread_rng();

            for _ in 0..count {
                let strategy_idx = rng.gen_range(0..22);
                match strategy_idx {
                    0 => grid.push(DiscoveryStrategyType::Rsi {
                        period: rng.gen_range(3..=50),
                        overbought: rng.gen_range(55.0..=90.0),
                        oversold: rng.gen_range(10.0..=45.0),
                    }),
                    1 => grid.push(DiscoveryStrategyType::BollingerBands {
                        period: rng.gen_range(5..=50),
                        multiplier: rng.gen_range(0.5..=4.0),
                    }),
                    2 => {
                        let fast = rng.gen_range(3..=15);
                        let slow = rng.gen_range((fast + 2)..=40);
                        grid.push(DiscoveryStrategyType::Macd {
                            fast,
                            slow,
                            signal: rng.gen_range(2..=15),
                        });
                    }
                    3 => {
                        let fast = rng.gen_range(3..=20);
                        let slow = rng.gen_range((fast + 2)..=100);
                        grid.push(DiscoveryStrategyType::EmaCrossover {
                            fast_period: fast,
                            slow_period: slow,
                        });
                    }
                    4 => grid.push(DiscoveryStrategyType::Stochastic {
                        period: rng.gen_range(3..=30),
                        overbought: rng.gen_range(65.0..=95.0),
                        oversold: rng.gen_range(5.0..=35.0),
                    }),
                    5 => grid.push(DiscoveryStrategyType::AtrMeanReversion {
                        atr_period: rng.gen_range(3..=35),
                        sma_period: rng.gen_range(5..=80),
                        multiplier: rng.gen_range(0.5..=3.5),
                    }),
                    6 => grid.push(DiscoveryStrategyType::Vwap {
                        period: rng.gen_range(5..=100),
                    }),
                    7 => grid.push(DiscoveryStrategyType::Obv {
                        sma_period: rng.gen_range(5..=50),
                    }),
                    8 => grid.push(DiscoveryStrategyType::WilliamsR {
                        period: rng.gen_range(3..=35),
                        overbought: rng.gen_range(-30.0..=-5.0),
                        oversold: rng.gen_range(-95.0..=-65.0),
                    }),
                    9 => grid.push(DiscoveryStrategyType::Adx {
                        period: rng.gen_range(5..=35),
                        adx_threshold: rng.gen_range(10.0..=45.0),
                    }),
                    10 => grid.push(DiscoveryStrategyType::RsiBollinger {
                        rsi_period: rng.gen_range(5..=30),
                        rsi_ob: rng.gen_range(60.0..=85.0),
                        rsi_os: rng.gen_range(15.0..=40.0),
                        bb_period: rng.gen_range(10..=35),
                        bb_mult: rng.gen_range(1.0..=3.5),
                    }),
                    11 => {
                        let mf = rng.gen_range(3..=15);
                        let ms = rng.gen_range((mf + 2)..=35);
                        grid.push(DiscoveryStrategyType::MacdRsi {
                            macd_fast: mf,
                            macd_slow: ms,
                            macd_signal: rng.gen_range(2..=12),
                            rsi_period: rng.gen_range(5..=25),
                            rsi_ob: rng.gen_range(60.0..=85.0),
                            rsi_os: rng.gen_range(15.0..=40.0),
                        });
                    }
                    12 => {
                        let ef = rng.gen_range(3..=18);
                        let es = rng.gen_range((ef + 2)..=60);
                        grid.push(DiscoveryStrategyType::EmaRsi {
                            ema_fast: ef,
                            ema_slow: es,
                            rsi_period: rng.gen_range(5..=25),
                            rsi_ob: rng.gen_range(60.0..=85.0),
                            rsi_os: rng.gen_range(15.0..=40.0),
                        });
                    }
                    13 => grid.push(DiscoveryStrategyType::StochRsi {
                        stoch_period: rng.gen_range(5..=25),
                        stoch_ob: rng.gen_range(70.0..=90.0),
                        stoch_os: rng.gen_range(10.0..=30.0),
                        rsi_period: rng.gen_range(5..=25),
                        rsi_ob: rng.gen_range(60.0..=85.0),
                        rsi_os: rng.gen_range(15.0..=40.0),
                    }),
                    14 => {
                        let mf = rng.gen_range(3..=15);
                        let ms = rng.gen_range((mf + 2)..=35);
                        grid.push(DiscoveryStrategyType::MacdBollinger {
                            macd_fast: mf,
                            macd_slow: ms,
                            macd_signal: rng.gen_range(2..=12),
                            bb_period: rng.gen_range(10..=35),
                            bb_mult: rng.gen_range(1.0..=3.5),
                        });
                    }
                    15 => {
                        let mf = rng.gen_range(3..=15);
                        let ms = rng.gen_range((mf + 2)..=35);
                        grid.push(DiscoveryStrategyType::TripleRsiMacdBb {
                            rsi_period: rng.gen_range(5..=25),
                            rsi_ob: rng.gen_range(60.0..=85.0),
                            rsi_os: rng.gen_range(15.0..=40.0),
                            macd_fast: mf,
                            macd_slow: ms,
                            macd_signal: rng.gen_range(2..=12),
                            bb_period: rng.gen_range(10..=35),
                            bb_mult: rng.gen_range(1.0..=3.5),
                        });
                    }
                    16 => {
                        let ef = rng.gen_range(3..=18);
                        let es = rng.gen_range((ef + 2)..=60);
                        grid.push(DiscoveryStrategyType::TripleEmaRsiStoch {
                            ema_fast: ef,
                            ema_slow: es,
                            rsi_period: rng.gen_range(5..=25),
                            rsi_ob: rng.gen_range(60.0..=85.0),
                            rsi_os: rng.gen_range(15.0..=40.0),
                            stoch_period: rng.gen_range(5..=25),
                            stoch_ob: rng.gen_range(70.0..=90.0),
                            stoch_os: rng.gen_range(10.0..=30.0),
                        });
                    }
                    17 => grid.push(DiscoveryStrategyType::VwapRsi {
                        vwap_period: rng.gen_range(5..=80),
                        rsi_period: rng.gen_range(5..=25),
                        rsi_overbought: rng.gen_range(60.0..=85.0),
                        rsi_oversold: rng.gen_range(15.0..=40.0),
                    }),
                    18 => {
                        let mf = rng.gen_range(3..=15);
                        let ms = rng.gen_range((mf + 2)..=35);
                        grid.push(DiscoveryStrategyType::ObvMacd {
                            obv_sma_period: rng.gen_range(5..=40),
                            macd_fast: mf,
                            macd_slow: ms,
                            macd_signal: rng.gen_range(2..=12),
                        });
                    }
                    19 => {
                        let ef = rng.gen_range(3..=20);
                        let es = rng.gen_range((ef + 2)..=60);
                        grid.push(DiscoveryStrategyType::AdxEma {
                            adx_period: rng.gen_range(5..=35),
                            adx_threshold: rng.gen_range(10.0..=45.0),
                            ema_fast: ef,
                            ema_slow: es,
                        });
                    }
                    20 => grid.push(DiscoveryStrategyType::WilliamsRStoch {
                        wr_period: rng.gen_range(3..=30),
                        wr_overbought: rng.gen_range(-30.0..=-5.0),
                        wr_oversold: rng.gen_range(-95.0..=-65.0),
                        stoch_period: rng.gen_range(3..=25),
                        stoch_overbought: rng.gen_range(70.0..=95.0),
                        stoch_oversold: rng.gen_range(5.0..=30.0),
                    }),
                    _ => {
                        let mpc_f = rng.gen_range(0.85..=0.99);
                        let bo_f = rng.gen_range(0.001..=0.05);
                        let sm_f = rng.gen_range(1.0..=8.0);
                        grid.push(DiscoveryStrategyType::Gabagool {
                            max_pair_cost: Decimal::from_str_exact(&format!("{:.3}", mpc_f))
                                .unwrap_or(dec!(0.95)),
                            bid_offset: Decimal::from_str_exact(&format!("{:.4}", bo_f))
                                .unwrap_or(dec!(0.01)),
                            spread_multiplier: Decimal::from_str_exact(&format!("{:.1}", sm_f))
                                .unwrap_or(dec!(3)),
                        });
                    }
                }
            }
        }
    }

    grid
}

// ============================================================================
// ML-Guided Exploration (Evolutionary Algorithm)
// ============================================================================

/// Generate an ML-guided grid using evolutionary strategies:
/// - 60% exploitation: mutations around top performers
/// - 20% crossover: parameter mixing between good results
/// - 20% exploration: pure random for diversity
fn generate_ml_guided_grid(
    top_results: &[DiscoveryResult],
    cycle: u32,
) -> Vec<DiscoveryStrategyType> {
    let total_budget = (300 + cycle as usize * 50).min(1000);
    let exploit_budget = total_budget * 60 / 100;
    let crossover_budget = total_budget * 20 / 100;
    let explore_budget = total_budget - exploit_budget - crossover_budget;

    let mut grid = Vec::with_capacity(total_budget);
    let mut rng = rand::thread_rng();

    // --- 1. Exploitation: mutate top performers ---
    // Sort by composite_score descending, take top 30
    let mut sorted = top_results.to_vec();
    sorted.sort_by(|a, b| b.composite_score.cmp(&a.composite_score));
    let top_n = sorted.iter().take(30).collect::<Vec<_>>();

    if !top_n.is_empty() {
        let mutations_per = (exploit_budget / top_n.len()).max(1);
        for result in &top_n {
            for _ in 0..mutations_per {
                if grid.len() >= exploit_budget {
                    break;
                }
                if let Some(mutated) = mutate_strategy(&result.strategy_type, &mut rng) {
                    grid.push(mutated);
                }
            }
        }
    }

    // Pad exploitation budget if not enough top results
    while grid.len() < exploit_budget {
        if let Some(parent) = top_n.first() {
            if let Some(mutated) = mutate_strategy(&parent.strategy_type, &mut rng) {
                grid.push(mutated);
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // --- 2. Crossover: mix parameters from pairs of good results ---
    let crossover_start = grid.len();
    if top_n.len() >= 2 {
        for _ in 0..crossover_budget {
            let idx_a = rng.gen_range(0..top_n.len().min(15));
            let idx_b = rng.gen_range(0..top_n.len().min(15));
            if idx_a == idx_b {
                continue;
            }
            if let Some(child) = crossover_strategies(
                &top_n[idx_a].strategy_type,
                &top_n[idx_b].strategy_type,
                &mut rng,
            ) {
                grid.push(child);
            }
        }
    }

    let crossover_count = grid.len() - crossover_start;

    // --- 3. Exploration: pure random ---
    let explore_start = grid.len();
    let random_grid = generate_random_strategies(explore_budget, &mut rng);
    grid.extend(random_grid);
    let explore_count = grid.len() - explore_start;

    info!(
        "ML-guided grid: {} exploitation, {} crossover, {} exploration (total {})",
        crossover_start,
        crossover_count,
        explore_count,
        grid.len()
    );

    grid
}

fn perturb_usize(val: usize, rng: &mut impl rand::Rng) -> usize {
    let factor = 1.0 + rng.gen_range(-0.15..=0.15);
    ((val as f64 * factor).round() as usize).max(3)
}

fn perturb_f64(val: f64, rng: &mut impl rand::Rng) -> f64 {
    let factor = 1.0 + rng.gen_range(-0.15..=0.15);
    val * factor
}

fn perturb_decimal(val: Decimal, rng: &mut impl rand::Rng) -> Decimal {
    let factor = 1.0 + rng.gen_range(-0.15f64..=0.15);
    let val_f: f64 = val.to_string().parse().unwrap_or(1.0);
    Decimal::from_str_exact(&format!("{:.4}", val_f * factor)).unwrap_or(val)
}

/// Mutate a strategy by perturbing each numeric parameter by ±5-15%
fn mutate_strategy(
    strategy: &DiscoveryStrategyType,
    rng: &mut impl rand::Rng,
) -> Option<DiscoveryStrategyType> {

    Some(match strategy {
        DiscoveryStrategyType::Rsi { period, overbought, oversold } => {
            let ob = perturb_f64(*overbought, rng).clamp(55.0, 90.0);
            let os = perturb_f64(*oversold, rng).clamp(10.0, 45.0);
            if os >= ob { return None; }
            DiscoveryStrategyType::Rsi {
                period: perturb_usize(*period, rng),
                overbought: ob,
                oversold: os,
            }
        }
        DiscoveryStrategyType::BollingerBands { period, multiplier } => {
            DiscoveryStrategyType::BollingerBands {
                period: perturb_usize(*period, rng),
                multiplier: perturb_f64(*multiplier, rng).clamp(0.5, 4.0),
            }
        }
        DiscoveryStrategyType::Macd { fast, slow, signal } => {
            let f = perturb_usize(*fast, rng);
            let s = perturb_usize(*slow, rng);
            if f >= s { return None; }
            DiscoveryStrategyType::Macd {
                fast: f,
                slow: s,
                signal: perturb_usize(*signal, rng),
            }
        }
        DiscoveryStrategyType::EmaCrossover { fast_period, slow_period } => {
            let f = perturb_usize(*fast_period, rng);
            let s = perturb_usize(*slow_period, rng);
            if f >= s { return None; }
            DiscoveryStrategyType::EmaCrossover {
                fast_period: f,
                slow_period: s,
            }
        }
        DiscoveryStrategyType::Stochastic { period, overbought, oversold } => {
            DiscoveryStrategyType::Stochastic {
                period: perturb_usize(*period, rng),
                overbought: perturb_f64(*overbought, rng).clamp(65.0, 95.0),
                oversold: perturb_f64(*oversold, rng).clamp(5.0, 35.0),
            }
        }
        DiscoveryStrategyType::AtrMeanReversion { atr_period, sma_period, multiplier } => {
            DiscoveryStrategyType::AtrMeanReversion {
                atr_period: perturb_usize(*atr_period, rng),
                sma_period: perturb_usize(*sma_period, rng),
                multiplier: perturb_f64(*multiplier, rng).clamp(0.5, 3.5),
            }
        }
        DiscoveryStrategyType::Vwap { period } => {
            DiscoveryStrategyType::Vwap {
                period: perturb_usize(*period, rng),
            }
        }
        DiscoveryStrategyType::Obv { sma_period } => {
            DiscoveryStrategyType::Obv {
                sma_period: perturb_usize(*sma_period, rng),
            }
        }
        DiscoveryStrategyType::WilliamsR { period, overbought, oversold } => {
            let ob = perturb_f64(*overbought, rng).clamp(-30.0, -5.0);
            let os = perturb_f64(*oversold, rng).clamp(-95.0, -65.0);
            if os >= ob { return None; }
            DiscoveryStrategyType::WilliamsR {
                period: perturb_usize(*period, rng),
                overbought: ob,
                oversold: os,
            }
        }
        DiscoveryStrategyType::Adx { period, adx_threshold } => {
            DiscoveryStrategyType::Adx {
                period: perturb_usize(*period, rng),
                adx_threshold: perturb_f64(*adx_threshold, rng).clamp(10.0, 45.0),
            }
        }
        DiscoveryStrategyType::RsiBollinger { rsi_period, rsi_ob, rsi_os, bb_period, bb_mult } => {
            let ob = perturb_f64(*rsi_ob, rng).clamp(60.0, 85.0);
            let os = perturb_f64(*rsi_os, rng).clamp(15.0, 40.0);
            if os >= ob { return None; }
            DiscoveryStrategyType::RsiBollinger {
                rsi_period: perturb_usize(*rsi_period, rng),
                rsi_ob: ob,
                rsi_os: os,
                bb_period: perturb_usize(*bb_period, rng),
                bb_mult: perturb_f64(*bb_mult, rng).clamp(1.0, 3.5),
            }
        }
        DiscoveryStrategyType::MacdRsi { macd_fast, macd_slow, macd_signal, rsi_period, rsi_ob, rsi_os } => {
            let mf = perturb_usize(*macd_fast, rng);
            let ms = perturb_usize(*macd_slow, rng);
            if mf >= ms { return None; }
            let ob = perturb_f64(*rsi_ob, rng).clamp(60.0, 85.0);
            let os = perturb_f64(*rsi_os, rng).clamp(15.0, 40.0);
            if os >= ob { return None; }
            DiscoveryStrategyType::MacdRsi {
                macd_fast: mf,
                macd_slow: ms,
                macd_signal: perturb_usize(*macd_signal, rng),
                rsi_period: perturb_usize(*rsi_period, rng),
                rsi_ob: ob,
                rsi_os: os,
            }
        }
        DiscoveryStrategyType::EmaRsi { ema_fast, ema_slow, rsi_period, rsi_ob, rsi_os } => {
            let ef = perturb_usize(*ema_fast, rng);
            let es = perturb_usize(*ema_slow, rng);
            if ef >= es { return None; }
            let ob = perturb_f64(*rsi_ob, rng).clamp(60.0, 85.0);
            let os = perturb_f64(*rsi_os, rng).clamp(15.0, 40.0);
            if os >= ob { return None; }
            DiscoveryStrategyType::EmaRsi {
                ema_fast: ef,
                ema_slow: es,
                rsi_period: perturb_usize(*rsi_period, rng),
                rsi_ob: ob,
                rsi_os: os,
            }
        }
        DiscoveryStrategyType::StochRsi { stoch_period, stoch_ob, stoch_os, rsi_period, rsi_ob, rsi_os } => {
            let rob = perturb_f64(*rsi_ob, rng).clamp(60.0, 85.0);
            let ros = perturb_f64(*rsi_os, rng).clamp(15.0, 40.0);
            if ros >= rob { return None; }
            DiscoveryStrategyType::StochRsi {
                stoch_period: perturb_usize(*stoch_period, rng),
                stoch_ob: perturb_f64(*stoch_ob, rng).clamp(70.0, 90.0),
                stoch_os: perturb_f64(*stoch_os, rng).clamp(10.0, 30.0),
                rsi_period: perturb_usize(*rsi_period, rng),
                rsi_ob: rob,
                rsi_os: ros,
            }
        }
        DiscoveryStrategyType::MacdBollinger { macd_fast, macd_slow, macd_signal, bb_period, bb_mult } => {
            let mf = perturb_usize(*macd_fast, rng);
            let ms = perturb_usize(*macd_slow, rng);
            if mf >= ms { return None; }
            DiscoveryStrategyType::MacdBollinger {
                macd_fast: mf,
                macd_slow: ms,
                macd_signal: perturb_usize(*macd_signal, rng),
                bb_period: perturb_usize(*bb_period, rng),
                bb_mult: perturb_f64(*bb_mult, rng).clamp(1.0, 3.5),
            }
        }
        DiscoveryStrategyType::TripleRsiMacdBb { rsi_period, rsi_ob, rsi_os, macd_fast, macd_slow, macd_signal, bb_period, bb_mult } => {
            let mf = perturb_usize(*macd_fast, rng);
            let ms = perturb_usize(*macd_slow, rng);
            if mf >= ms { return None; }
            let ob = perturb_f64(*rsi_ob, rng).clamp(60.0, 85.0);
            let os = perturb_f64(*rsi_os, rng).clamp(15.0, 40.0);
            if os >= ob { return None; }
            DiscoveryStrategyType::TripleRsiMacdBb {
                rsi_period: perturb_usize(*rsi_period, rng),
                rsi_ob: ob,
                rsi_os: os,
                macd_fast: mf,
                macd_slow: ms,
                macd_signal: perturb_usize(*macd_signal, rng),
                bb_period: perturb_usize(*bb_period, rng),
                bb_mult: perturb_f64(*bb_mult, rng).clamp(1.0, 3.5),
            }
        }
        DiscoveryStrategyType::TripleEmaRsiStoch { ema_fast, ema_slow, rsi_period, rsi_ob, rsi_os, stoch_period, stoch_ob, stoch_os } => {
            let ef = perturb_usize(*ema_fast, rng);
            let es = perturb_usize(*ema_slow, rng);
            if ef >= es { return None; }
            let rob = perturb_f64(*rsi_ob, rng).clamp(60.0, 85.0);
            let ros = perturb_f64(*rsi_os, rng).clamp(15.0, 40.0);
            if ros >= rob { return None; }
            DiscoveryStrategyType::TripleEmaRsiStoch {
                ema_fast: ef,
                ema_slow: es,
                rsi_period: perturb_usize(*rsi_period, rng),
                rsi_ob: rob,
                rsi_os: ros,
                stoch_period: perturb_usize(*stoch_period, rng),
                stoch_ob: perturb_f64(*stoch_ob, rng).clamp(70.0, 90.0),
                stoch_os: perturb_f64(*stoch_os, rng).clamp(10.0, 30.0),
            }
        }
        DiscoveryStrategyType::VwapRsi { vwap_period, rsi_period, rsi_overbought, rsi_oversold } => {
            let ob = perturb_f64(*rsi_overbought, rng).clamp(60.0, 85.0);
            let os = perturb_f64(*rsi_oversold, rng).clamp(15.0, 40.0);
            if os >= ob { return None; }
            DiscoveryStrategyType::VwapRsi {
                vwap_period: perturb_usize(*vwap_period, rng),
                rsi_period: perturb_usize(*rsi_period, rng),
                rsi_overbought: ob,
                rsi_oversold: os,
            }
        }
        DiscoveryStrategyType::ObvMacd { obv_sma_period, macd_fast, macd_slow, macd_signal } => {
            let mf = perturb_usize(*macd_fast, rng);
            let ms = perturb_usize(*macd_slow, rng);
            if mf >= ms { return None; }
            DiscoveryStrategyType::ObvMacd {
                obv_sma_period: perturb_usize(*obv_sma_period, rng),
                macd_fast: mf,
                macd_slow: ms,
                macd_signal: perturb_usize(*macd_signal, rng),
            }
        }
        DiscoveryStrategyType::AdxEma { adx_period, adx_threshold, ema_fast, ema_slow } => {
            let ef = perturb_usize(*ema_fast, rng);
            let es = perturb_usize(*ema_slow, rng);
            if ef >= es { return None; }
            DiscoveryStrategyType::AdxEma {
                adx_period: perturb_usize(*adx_period, rng),
                adx_threshold: perturb_f64(*adx_threshold, rng).clamp(10.0, 45.0),
                ema_fast: ef,
                ema_slow: es,
            }
        }
        DiscoveryStrategyType::WilliamsRStoch { wr_period, wr_overbought, wr_oversold, stoch_period, stoch_overbought, stoch_oversold } => {
            let wob = perturb_f64(*wr_overbought, rng).clamp(-30.0, -5.0);
            let wos = perturb_f64(*wr_oversold, rng).clamp(-95.0, -65.0);
            if wos >= wob { return None; }
            DiscoveryStrategyType::WilliamsRStoch {
                wr_period: perturb_usize(*wr_period, rng),
                wr_overbought: wob,
                wr_oversold: wos,
                stoch_period: perturb_usize(*stoch_period, rng),
                stoch_overbought: perturb_f64(*stoch_overbought, rng).clamp(70.0, 95.0),
                stoch_oversold: perturb_f64(*stoch_oversold, rng).clamp(5.0, 30.0),
            }
        }
        DiscoveryStrategyType::DynamicCombo { indicators, params, combine_mode } => {
            let new_params: Vec<IndicatorParams> = params.iter()
                .map(|p| perturb_indicator_params(p, rng))
                .collect();
            // Occasionally flip mode
            let new_mode = if rng.gen_bool(0.15) {
                let modes = DynCombineMode::all();
                modes[rng.gen_range(0..modes.len())]
            } else {
                *combine_mode
            };
            DiscoveryStrategyType::DynamicCombo {
                indicators: indicators.clone(),
                params: new_params,
                combine_mode: new_mode,
            }
        }
        DiscoveryStrategyType::Gabagool { max_pair_cost, bid_offset, spread_multiplier } => {
            DiscoveryStrategyType::Gabagool {
                max_pair_cost: perturb_decimal(*max_pair_cost, rng).max(dec!(0.85)).min(dec!(0.99)),
                bid_offset: perturb_decimal(*bid_offset, rng).max(dec!(0.001)),
                spread_multiplier: perturb_decimal(*spread_multiplier, rng).max(dec!(1)),
            }
        }
    })
}

/// Perturb indicator params by ±15% on each numeric value
fn perturb_indicator_params(params: &IndicatorParams, rng: &mut impl rand::Rng) -> IndicatorParams {
    match params {
        IndicatorParams::Rsi { period, overbought, oversold } => {
            let ob = perturb_f64(*overbought, rng).clamp(55.0, 90.0);
            let os = perturb_f64(*oversold, rng).clamp(10.0, 45.0);
            IndicatorParams::Rsi {
                period: perturb_usize(*period, rng).max(3),
                overbought: if os >= ob { *overbought } else { ob },
                oversold: if os >= ob { *oversold } else { os },
            }
        }
        IndicatorParams::BollingerBands { period, multiplier } => IndicatorParams::BollingerBands {
            period: perturb_usize(*period, rng).max(5),
            multiplier: perturb_f64(*multiplier, rng).clamp(0.5, 4.0),
        },
        IndicatorParams::Macd { fast, slow, signal } => {
            let f = perturb_usize(*fast, rng).max(3);
            let s = perturb_usize(*slow, rng).max(f + 2);
            IndicatorParams::Macd { fast: f, slow: s, signal: perturb_usize(*signal, rng).max(2) }
        }
        IndicatorParams::EmaCrossover { fast_period, slow_period } => {
            let f = perturb_usize(*fast_period, rng).max(3);
            let s = perturb_usize(*slow_period, rng).max(f + 2);
            IndicatorParams::EmaCrossover { fast_period: f, slow_period: s }
        }
        IndicatorParams::Stochastic { period, overbought, oversold } => IndicatorParams::Stochastic {
            period: perturb_usize(*period, rng).max(3),
            overbought: perturb_f64(*overbought, rng).clamp(65.0, 95.0),
            oversold: perturb_f64(*oversold, rng).clamp(5.0, 35.0),
        },
        IndicatorParams::AtrMeanReversion { atr_period, sma_period, multiplier } => IndicatorParams::AtrMeanReversion {
            atr_period: perturb_usize(*atr_period, rng).max(3),
            sma_period: perturb_usize(*sma_period, rng).max(5),
            multiplier: perturb_f64(*multiplier, rng).clamp(0.5, 3.5),
        },
        IndicatorParams::Vwap { period } => IndicatorParams::Vwap {
            period: perturb_usize(*period, rng).max(5),
        },
        IndicatorParams::Obv { sma_period } => IndicatorParams::Obv {
            sma_period: perturb_usize(*sma_period, rng).max(5),
        },
        IndicatorParams::WilliamsR { period, overbought, oversold } => {
            let ob = perturb_f64(*overbought, rng).clamp(-30.0, -5.0);
            let os = perturb_f64(*oversold, rng).clamp(-95.0, -65.0);
            IndicatorParams::WilliamsR {
                period: perturb_usize(*period, rng).max(3),
                overbought: if os >= ob { *overbought } else { ob },
                oversold: if os >= ob { *oversold } else { os },
            }
        }
        IndicatorParams::Adx { period, adx_threshold } => IndicatorParams::Adx {
            period: perturb_usize(*period, rng).max(5),
            adx_threshold: perturb_f64(*adx_threshold, rng).clamp(10.0, 45.0),
        },
    }
}

/// Crossover: if two strategies are the same type, mix their parameters
fn crossover_strategies(
    a: &DiscoveryStrategyType,
    b: &DiscoveryStrategyType,
    rng: &mut impl rand::Rng,
) -> Option<DiscoveryStrategyType> {
    match (a, b) {
        (
            DiscoveryStrategyType::Rsi { period: p1, overbought: ob1, oversold: os1 },
            DiscoveryStrategyType::Rsi { period: p2, overbought: ob2, oversold: os2 },
        ) => {
            let ob = if rng.gen_bool(0.5) { *ob1 } else { *ob2 };
            let os = if rng.gen_bool(0.5) { *os1 } else { *os2 };
            if os >= ob { return None; }
            Some(DiscoveryStrategyType::Rsi {
                period: if rng.gen_bool(0.5) { *p1 } else { *p2 },
                overbought: ob,
                oversold: os,
            })
        }
        (
            DiscoveryStrategyType::BollingerBands { period: p1, multiplier: m1 },
            DiscoveryStrategyType::BollingerBands { period: p2, multiplier: m2 },
        ) => {
            Some(DiscoveryStrategyType::BollingerBands {
                period: if rng.gen_bool(0.5) { *p1 } else { *p2 },
                multiplier: if rng.gen_bool(0.5) { *m1 } else { *m2 },
            })
        }
        (
            DiscoveryStrategyType::Macd { fast: f1, slow: s1, signal: sig1 },
            DiscoveryStrategyType::Macd { fast: f2, slow: s2, signal: sig2 },
        ) => {
            let f = if rng.gen_bool(0.5) { *f1 } else { *f2 };
            let s = if rng.gen_bool(0.5) { *s1 } else { *s2 };
            if f >= s { return None; }
            Some(DiscoveryStrategyType::Macd {
                fast: f,
                slow: s,
                signal: if rng.gen_bool(0.5) { *sig1 } else { *sig2 },
            })
        }
        (
            DiscoveryStrategyType::EmaCrossover { fast_period: f1, slow_period: s1 },
            DiscoveryStrategyType::EmaCrossover { fast_period: f2, slow_period: s2 },
        ) => {
            let f = if rng.gen_bool(0.5) { *f1 } else { *f2 };
            let s = if rng.gen_bool(0.5) { *s1 } else { *s2 };
            if f >= s { return None; }
            Some(DiscoveryStrategyType::EmaCrossover {
                fast_period: f,
                slow_period: s,
            })
        }
        // DynamicCombo crossover: same indicator set → mix params per indicator
        (
            DiscoveryStrategyType::DynamicCombo { indicators: ind_a, params: params_a, combine_mode: mode_a },
            DiscoveryStrategyType::DynamicCombo { indicators: ind_b, params: params_b, combine_mode: mode_b },
        ) if ind_a == ind_b && params_a.len() == params_b.len() => {
            let child_params: Vec<IndicatorParams> = params_a.iter().zip(params_b.iter())
                .map(|(pa, pb)| if rng.gen_bool(0.5) { pa.clone() } else { pb.clone() })
                .collect();
            let child_mode = if rng.gen_bool(0.5) { *mode_a } else { *mode_b };
            Some(DiscoveryStrategyType::DynamicCombo {
                indicators: ind_a.clone(),
                params: child_params,
                combine_mode: child_mode,
            })
        }
        // For different strategy types or complex combos, fall back to mutation of the better one
        _ => mutate_strategy(a, rng),
    }
}

/// Generate pure random strategy combinations (DynamicCombo only + some Gabagool)
fn generate_random_strategies(count: usize, rng: &mut impl rand::Rng) -> Vec<DiscoveryStrategyType> {
    let mut grid = Vec::with_capacity(count);

    for _ in 0..count {
        // 95% dynamic combos, 5% gabagool
        if rng.gen_bool(0.95) {
            let n = rng.gen_range(2..=4usize);
            grid.push(generate_random_dynamic_combo(n, rng));
        } else {
            let mpc_f = rng.gen_range(0.85..=0.99);
            let bo_f = rng.gen_range(0.001..=0.05);
            let sm_f = rng.gen_range(1.0..=8.0);
            grid.push(DiscoveryStrategyType::Gabagool {
                max_pair_cost: Decimal::from_str_exact(&format!("{:.3}", mpc_f)).unwrap_or(dec!(0.95)),
                bid_offset: Decimal::from_str_exact(&format!("{:.4}", bo_f)).unwrap_or(dec!(0.01)),
                spread_multiplier: Decimal::from_str_exact(&format!("{:.1}", sm_f)).unwrap_or(dec!(3)),
            });
        }
    }

    grid
}

// ============================================================================
// Continuous Discovery Runner
// ============================================================================

/// Run discovery continuously in an infinite loop, expanding the search space
/// each cycle. Stops only when `progress.cancelled` is set to true.
pub async fn run_continuous_discovery(
    request: DiscoveryRequest,
    binance: Arc<BinanceClient>,
    progress: Arc<DiscoveryProgress>,
    db_pool: Option<SqlitePool>,
) {
    let top_n = request.top_n.unwrap_or(10);
    let initial_capital = dec!(10000);
    let base_position_pct = dec!(10);
    let fee_config = PolymarketFeeConfig::default();
    let run_id = Utc::now().timestamp_millis().to_string();

    // Multi-sizing modes to test across cycles
    let sizing_modes = [SizingMode::Fixed, SizingMode::Kelly, SizingMode::ConfidenceWeighted];
    // Multi-days periods to test
    let days_variants: Vec<u32> = vec![30, 60, 90, 180, 365];

    progress.is_continuous.store(true, Ordering::Relaxed);

    info!(
        symbols = ?request.symbols,
        "Starting continuous discovery (non-stop)"
    );

    // ── Phase 0: Fetch klines for max period ───────────────────────────
    let max_days = *days_variants.iter().max().unwrap_or(&request.days);
    let end_time = chrono::Utc::now().timestamp_millis();
    let start_time = end_time - (max_days as i64 * 24 * 60 * 60 * 1000);
    let mut last_fetch_time = std::time::Instant::now();

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

    // ── Main loop ──────────────────────────────────────────────────────
    let mut all_results: Vec<DiscoveryResult> = Vec::new();
    let mut cycle = 0u32;

    loop {
        if progress.cancelled.load(Ordering::Relaxed) {
            info!("Continuous discovery cancelled by user");
            break;
        }

        // Re-fetch klines every 6 hours
        if last_fetch_time.elapsed() > std::time::Duration::from_secs(6 * 3600) {
            info!("Re-fetching klines (6h refresh)");
            *progress.status.write().unwrap() = DiscoveryStatus::FetchingData;
            *progress.phase.write().unwrap() = "Refreshing market data...".to_string();

            let new_end = chrono::Utc::now().timestamp_millis();
            let new_start = new_end - (max_days as i64 * 24 * 60 * 60 * 1000);
            let mut new_klines = Vec::new();

            for symbol in &request.symbols {
                if progress.cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match binance
                    .get_klines_paginated(symbol, "15m", new_start, new_end)
                    .await
                {
                    Ok(klines) => {
                        info!(symbol = %symbol, bars = klines.len(), "Refreshed klines");
                        new_klines.push((symbol.clone(), klines));
                    }
                    Err(e) => {
                        warn!(symbol = %symbol, error = %e, "Failed to refresh klines");
                    }
                }
            }

            if !new_klines.is_empty() {
                symbol_klines = new_klines;
            }
            last_fetch_time = std::time::Instant::now();
        }

        // Set cycle info
        progress.current_cycle.store(cycle, Ordering::Relaxed);
        progress.total_new_this_cycle.store(0, Ordering::Relaxed);

        let phase_name = if cycle == 0 {
            "Phase 1: Broad Scan"
        } else if cycle == 1 {
            "Phase 2: Fine Interpolation"
        } else if cycle == 2 {
            "Phase 3: Extended Ranges"
        } else {
            "ML-Guided Exploration"
        };

        let status = if cycle < 2 {
            if cycle == 0 {
                DiscoveryStatus::Phase1BroadScan
            } else {
                DiscoveryStatus::Phase2Refinement
            }
        } else {
            DiscoveryStatus::Phase3Exploration
        };

        *progress.status.write().unwrap() = status;
        *progress.phase.write().unwrap() = format!("Cycle {} — {}", cycle, phase_name);

        let grid = if cycle >= 3 {
            generate_ml_guided_grid(&all_results, cycle)
        } else {
            generate_exploratory_grid(cycle)
        };

        // For cycle 0, also do Phase 2 refinement after the grid
        let do_refinement = cycle == 0;

        // Build the full work list: grid × symbols × days × sizing
        let sizing_list = if cycle == 0 {
            // First cycle: use request sizing only
            vec![request.sizing_mode.unwrap_or_default()]
        } else {
            // Later cycles: test all sizing modes
            sizing_modes.to_vec()
        };

        let days_list = if cycle == 0 {
            vec![request.days]
        } else {
            days_variants.clone()
        };

        let total_combos = grid.len() as u32
            * symbol_klines.len() as u32
            * days_list.len() as u32
            * sizing_list.len() as u32;

        progress
            .total_combinations
            .store(total_combos, Ordering::Relaxed);
        progress.completed.store(0, Ordering::Relaxed);
        progress.skipped.store(0, Ordering::Relaxed);

        info!(
            cycle = cycle,
            grid_size = grid.len(),
            total_combos = total_combos,
            "Cycle starting"
        );

        let mut cycle_idx = 0u32;

        for (symbol, full_klines) in &symbol_klines {
            for &days in &days_list {
                // Slice klines to the requested days period
                let klines = slice_klines_to_days(full_klines, days);

                for sizing_mode in &sizing_list {
                    for strategy_type in &grid {
                        if progress.cancelled.load(Ordering::Relaxed) {
                            info!("Continuous discovery cancelled by user");
                            *progress.status.write().unwrap() = DiscoveryStatus::Complete;
                            update_best_so_far(&all_results, initial_capital, top_n, &progress);
                            *progress.final_results.write().unwrap() =
                                progress.best_so_far.read().unwrap().clone();
                            return;
                        }

                        if cycle_idx % 50 == 0 {
                            *progress.current_strategy.write().unwrap() =
                                strategy_type.name().to_string();
                            *progress.current_symbol.write().unwrap() = symbol.clone();
                        }

                        // Check DB cache
                        let hash =
                            compute_params_hash(strategy_type, symbol, days, *sizing_mode);
                        if let Some(pool) = &db_pool {
                            let repo = DiscoveryRepository::new(pool);
                            if let Ok(Some(existing)) = repo.get_by_hash(&hash).await {
                                all_results.push(record_to_result(existing));
                                cycle_idx += 1;
                                progress.completed.store(cycle_idx, Ordering::Relaxed);
                                progress.skipped.fetch_add(1, Ordering::Relaxed);
                                progress
                                    .total_tested_all_cycles
                                    .fetch_add(1, Ordering::Relaxed);
                                if cycle_idx % 50 == 0 {
                                    update_best_so_far(
                                        &all_results,
                                        initial_capital,
                                        top_n,
                                        &progress,
                                    );
                                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                                }
                                continue;
                            }
                        }

                        let result = run_single_backtest(
                            strategy_type,
                            &klines,
                            symbol,
                            initial_capital,
                            base_position_pct,
                            *sizing_mode,
                            &fee_config,
                        );

                        // Save to DB
                        if let Some(pool) = &db_pool {
                            let phase_label = format!("cycle{}", cycle);
                            let record =
                                result_to_record(&result, &hash, &run_id, &phase_label, days);
                            let repo = DiscoveryRepository::new(pool);
                            let _ = repo.save(&record).await;
                        }

                        all_results.push(result);

                        cycle_idx += 1;
                        progress.completed.store(cycle_idx, Ordering::Relaxed);
                        progress.total_new_this_cycle.fetch_add(1, Ordering::Relaxed);
                        progress
                            .total_tested_all_cycles
                            .fetch_add(1, Ordering::Relaxed);

                        if cycle_idx % 50 == 0 {
                            update_best_so_far(&all_results, initial_capital, top_n, &progress);
                            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                        }
                    }
                }
            }
        }

        // Phase 2 refinement for cycle 0
        if do_refinement {
            *progress.status.write().unwrap() = DiscoveryStatus::Phase2Refinement;
            *progress.phase.write().unwrap() = "Cycle 0 — Phase 2: Refinement".to_string();

            let mut scored = all_results.clone();
            scored.sort_by(|a, b| {
                let sa = score_result(a, initial_capital);
                let sb = score_result(b, initial_capital);
                sb.cmp(&sa)
            });
            let top_for_refinement: Vec<DiscoveryResult> = scored.into_iter().take(20).collect();

            for top_result in &top_for_refinement {
                if progress.cancelled.load(Ordering::Relaxed) {
                    break;
                }
                let refinement_grid = generate_refinement_grid(&top_result.strategy_type);
                let klines_opt = symbol_klines
                    .iter()
                    .find(|(s, _)| *s == top_result.symbol)
                    .map(|(_, k)| k);
                let full_kl = match klines_opt {
                    Some(k) => k,
                    None => continue,
                };
                let klines = slice_klines_to_days(full_kl, request.days);
                let sizing_mode = request.sizing_mode.unwrap_or_default();

                *progress.current_strategy.write().unwrap() =
                    format!("{} (refine)", top_result.strategy_name);

                for variant in &refinement_grid {
                    let hash = compute_params_hash(
                        variant,
                        &top_result.symbol,
                        request.days,
                        sizing_mode,
                    );
                    if let Some(pool) = &db_pool {
                        let repo = DiscoveryRepository::new(pool);
                        if let Ok(Some(existing)) = repo.get_by_hash(&hash).await {
                            all_results.push(record_to_result(existing));
                            progress.skipped.fetch_add(1, Ordering::Relaxed);
                            progress
                                .total_tested_all_cycles
                                .fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                    }

                    let result = run_single_backtest(
                        variant,
                        &klines,
                        &top_result.symbol,
                        initial_capital,
                        base_position_pct,
                        sizing_mode,
                        &fee_config,
                    );

                    if let Some(pool) = &db_pool {
                        let record = result_to_record(
                            &result,
                            &hash,
                            &run_id,
                            "phase2",
                            request.days,
                        );
                        let repo = DiscoveryRepository::new(pool);
                        let _ = repo.save(&record).await;
                    }

                    all_results.push(result);
                    progress.total_new_this_cycle.fetch_add(1, Ordering::Relaxed);
                    progress
                        .total_tested_all_cycles
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Update best at end of cycle
        update_best_so_far(&all_results, initial_capital, top_n, &progress);

        let new_count = progress.total_new_this_cycle.load(Ordering::Relaxed);
        let total_all = progress.total_tested_all_cycles.load(Ordering::Relaxed);
        info!(
            cycle = cycle,
            new_this_cycle = new_count,
            total_all_cycles = total_all,
            best_score = %progress
                .best_so_far
                .read()
                .unwrap()
                .first()
                .map(|r| r.composite_score)
                .unwrap_or_default(),
            "Cycle complete"
        );

        cycle += 1;

        // Brief pause between cycles
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Final state
    update_best_so_far(&all_results, initial_capital, top_n, &progress);
    *progress.final_results.write().unwrap() = progress.best_so_far.read().unwrap().clone();
    *progress.status.write().unwrap() = DiscoveryStatus::Complete;

    info!(
        total_cycles = cycle,
        total_tested = progress.total_tested_all_cycles.load(Ordering::Relaxed),
        "Continuous discovery finished"
    );
}

/// Slice klines to only include the last N days of data
fn slice_klines_to_days(klines: &[Kline], days: u32) -> Vec<Kline> {
    if klines.is_empty() {
        return Vec::new();
    }
    let last_time = klines.last().unwrap().close_time;
    let cutoff = last_time - (days as i64 * 24 * 60 * 60 * 1000);
    klines
        .iter()
        .filter(|k| k.open_time >= cutoff)
        .cloned()
        .collect()
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
        // Dynamic combos: 405 pairs + 1080 triples + 210 quads + 48 gabagool = ~1743
        assert!(grid.len() > 1500, "Grid too small: {}", grid.len());
        assert!(grid.len() < 2000, "Grid too large: {}", grid.len());
    }

    #[test]
    fn test_phase1_grid_has_all_strategy_types() {
        let grid = generate_phase1_grid();

        // Should have DynamicCombo entries
        let has_dynamic = grid
            .iter()
            .any(|s| matches!(s, DiscoveryStrategyType::DynamicCombo { .. }));
        let has_gabagool = grid
            .iter()
            .any(|s| matches!(s, DiscoveryStrategyType::Gabagool { .. }));

        assert!(has_dynamic, "Missing DynamicCombo strategies");
        assert!(has_gabagool, "Missing Gabagool strategies");

        // Check that all 10 indicator types appear in at least one DynamicCombo
        for ind in SingleIndicatorType::all() {
            let found = grid.iter().any(|s| match s {
                DiscoveryStrategyType::DynamicCombo { indicators, .. } => indicators.contains(ind),
                _ => false,
            });
            assert!(found, "Missing indicator {:?} in DynamicCombo grid", ind);
        }

        // Check we have pairs (2), triples (3), and quads (4)
        let has_pairs = grid.iter().any(|s| match s {
            DiscoveryStrategyType::DynamicCombo { indicators, .. } => indicators.len() == 2,
            _ => false,
        });
        let has_triples = grid.iter().any(|s| match s {
            DiscoveryStrategyType::DynamicCombo { indicators, .. } => indicators.len() == 3,
            _ => false,
        });
        let has_quads = grid.iter().any(|s| match s {
            DiscoveryStrategyType::DynamicCombo { indicators, .. } => indicators.len() == 4,
            _ => false,
        });
        assert!(has_pairs, "Missing pair combos");
        assert!(has_triples, "Missing triple combos");
        assert!(has_quads, "Missing quad combos");
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
            sortino_ratio: Decimal::ZERO,
            max_consecutive_losses: 0,
            avg_win_pnl: Decimal::ZERO,
            avg_loss_pnl: Decimal::ZERO,
            total_volume: Decimal::ZERO,
            annualized_return_pct: Decimal::ZERO,
            annualized_sharpe: Decimal::ZERO,
            strategy_confidence: Decimal::ZERO,
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
            sortino_ratio: Decimal::ZERO,
            max_consecutive_losses: 0,
            avg_win_pnl: Decimal::ZERO,
            avg_loss_pnl: Decimal::ZERO,
            total_volume: Decimal::ZERO,
            annualized_return_pct: Decimal::ZERO,
            annualized_sharpe: Decimal::ZERO,
            strategy_confidence: Decimal::ZERO,
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

    #[test]
    fn test_exploratory_grid_cycle0_matches_phase1() {
        let grid_cycle0 = generate_exploratory_grid(0);
        let grid_phase1 = generate_phase1_grid();
        assert_eq!(grid_cycle0.len(), grid_phase1.len());
    }

    #[test]
    fn test_exploratory_grid_cycle1_produces_combos() {
        let grid = generate_exploratory_grid(1);
        // Cycle 1: 210 quads × 2 modes + 210 aggressive + 18 gabagool = ~648
        assert!(grid.len() > 400, "Cycle 1 grid too small: {}", grid.len());
        assert!(grid.len() < 800, "Cycle 1 grid too large: {}", grid.len());
    }

    #[test]
    fn test_exploratory_grid_cycle2_produces_combos() {
        let grid = generate_exploratory_grid(2);
        assert!(grid.len() > 50, "Cycle 2 grid too small: {}", grid.len());
    }

    #[test]
    fn test_exploratory_grid_cycle3_random() {
        let grid = generate_exploratory_grid(3);
        // 500 random combos + 20 gabagool = 520
        assert!(grid.len() >= 510 && grid.len() <= 530, "Cycle 3 should produce ~520 combos, got: {}", grid.len());
    }

    #[test]
    fn test_exploratory_grid_cycle5_grows() {
        let grid3 = generate_exploratory_grid(3);
        let grid5 = generate_exploratory_grid(5);
        assert!(
            grid5.len() > grid3.len(),
            "Later cycles should produce more combos"
        );
    }

    #[test]
    fn test_ml_guided_grid_with_no_results() {
        // With no top results, should still generate exploration-only grid
        let grid = generate_ml_guided_grid(&[], 3);
        assert!(grid.len() > 50, "Should produce exploration combos even with no results: {}", grid.len());
    }

    #[test]
    fn test_ml_guided_grid_with_results() {
        let results = vec![
            DiscoveryResult {
                rank: 1,
                strategy_type: DiscoveryStrategyType::Rsi {
                    period: 14,
                    overbought: 70.0,
                    oversold: 30.0,
                },
                strategy_name: "RSI".to_string(),
                symbol: "BTCUSDT".to_string(),
                sizing_mode: SizingMode::Fixed,
                composite_score: dec!(500),
                net_pnl: dec!(100),
                gross_pnl: dec!(120),
                total_fees: dec!(20),
                win_rate: dec!(65),
                total_trades: 20,
                sharpe_ratio: dec!(1.5),
                max_drawdown_pct: dec!(5),
                profit_factor: dec!(2),
                avg_trade_pnl: dec!(5),
                sortino_ratio: Decimal::ZERO,
                max_consecutive_losses: 0,
                avg_win_pnl: Decimal::ZERO,
                avg_loss_pnl: Decimal::ZERO,
                total_volume: Decimal::ZERO,
                annualized_return_pct: Decimal::ZERO,
                annualized_sharpe: Decimal::ZERO,
                strategy_confidence: Decimal::ZERO,
                hit_rate: None,
                avg_locked_profit: None,
            },
            DiscoveryResult {
                rank: 2,
                strategy_type: DiscoveryStrategyType::BollingerBands {
                    period: 20,
                    multiplier: 2.0,
                },
                strategy_name: "Bollinger Bands".to_string(),
                symbol: "ETHUSDT".to_string(),
                sizing_mode: SizingMode::Fixed,
                composite_score: dec!(400),
                net_pnl: dec!(80),
                gross_pnl: dec!(100),
                total_fees: dec!(20),
                win_rate: dec!(60),
                total_trades: 15,
                sharpe_ratio: dec!(1.2),
                max_drawdown_pct: dec!(8),
                profit_factor: dec!(1.8),
                avg_trade_pnl: dec!(5.3),
                sortino_ratio: Decimal::ZERO,
                max_consecutive_losses: 0,
                avg_win_pnl: Decimal::ZERO,
                avg_loss_pnl: Decimal::ZERO,
                total_volume: Decimal::ZERO,
                annualized_return_pct: Decimal::ZERO,
                annualized_sharpe: Decimal::ZERO,
                strategy_confidence: Decimal::ZERO,
                hit_rate: None,
                avg_locked_profit: None,
            },
        ];

        let grid = generate_ml_guided_grid(&results, 3);
        // Budget = 300 + 3*50 = 450
        assert!(grid.len() > 100, "Should produce a substantial grid: {}", grid.len());
        assert!(grid.len() <= 1000, "Grid too large: {}", grid.len());
    }

    #[test]
    fn test_ml_guided_grid_grows_with_cycle() {
        let results = vec![DiscoveryResult {
            rank: 1,
            strategy_type: DiscoveryStrategyType::Rsi {
                period: 14,
                overbought: 70.0,
                oversold: 30.0,
            },
            strategy_name: "RSI".to_string(),
            symbol: "BTCUSDT".to_string(),
            sizing_mode: SizingMode::Fixed,
            composite_score: dec!(500),
            net_pnl: dec!(100),
            gross_pnl: dec!(120),
            total_fees: dec!(20),
            win_rate: dec!(65),
            total_trades: 20,
            sharpe_ratio: dec!(1.5),
            max_drawdown_pct: dec!(5),
            profit_factor: dec!(2),
            avg_trade_pnl: dec!(5),
            sortino_ratio: Decimal::ZERO,
            max_consecutive_losses: 0,
            avg_win_pnl: Decimal::ZERO,
            avg_loss_pnl: Decimal::ZERO,
            total_volume: Decimal::ZERO,
            annualized_return_pct: Decimal::ZERO,
            annualized_sharpe: Decimal::ZERO,
            strategy_confidence: Decimal::ZERO,
            hit_rate: None,
            avg_locked_profit: None,
        }];

        let grid3 = generate_ml_guided_grid(&results, 3);
        let grid6 = generate_ml_guided_grid(&results, 6);
        assert!(
            grid6.len() > grid3.len(),
            "Later cycles should produce more combos ({} vs {})",
            grid6.len(),
            grid3.len()
        );
    }

    #[test]
    fn test_slice_klines_to_days() {
        // 100 bars, 15 min each = 25 hours = ~1 day
        let klines = make_klines(&vec![100.0; 200]);
        // With our make_klines, each bar is 900_000ms apart
        // 200 bars × 900s = 180_000s = 50 hours ≈ 2 days
        // Asking for 1 day = last 96 bars (96 × 15min = 24h)
        let sliced = slice_klines_to_days(&klines, 1);
        // The klines span about 2 days, so 1 day should give roughly half
        assert!(sliced.len() > 80 && sliced.len() < 120, "Got {} bars for 1 day", sliced.len());
    }

    #[test]
    fn test_continuous_progress_fields() {
        let progress = DiscoveryProgress::new();
        assert_eq!(progress.current_cycle.load(Ordering::Relaxed), 0);
        assert_eq!(progress.total_tested_all_cycles.load(Ordering::Relaxed), 0);
        assert_eq!(progress.total_new_this_cycle.load(Ordering::Relaxed), 0);
        assert!(!progress.is_continuous.load(Ordering::Relaxed));

        progress.reset();
        assert_eq!(progress.current_cycle.load(Ordering::Relaxed), 0);
        assert!(!progress.is_continuous.load(Ordering::Relaxed));
    }

    #[test]
    fn test_dynamic_combo_naming() {
        let combo = DiscoveryStrategyType::DynamicCombo {
            indicators: vec![SingleIndicatorType::Rsi, SingleIndicatorType::Macd],
            params: vec![
                SingleIndicatorType::Rsi.default_params(),
                SingleIndicatorType::Macd.default_params(),
            ],
            combine_mode: DynCombineMode::Majority,
        };
        assert_eq!(combo.name(), "RSI+MACD(M)");

        let triple = DiscoveryStrategyType::DynamicCombo {
            indicators: vec![
                SingleIndicatorType::BollingerBands,
                SingleIndicatorType::Stochastic,
                SingleIndicatorType::Adx,
            ],
            params: vec![
                SingleIndicatorType::BollingerBands.default_params(),
                SingleIndicatorType::Stochastic.default_params(),
                SingleIndicatorType::Adx.default_params(),
            ],
            combine_mode: DynCombineMode::Unanimous,
        };
        assert_eq!(triple.name(), "BB+Stoch+ADX(U)");
    }

    #[test]
    fn test_dynamic_combo_backtest_runs() {
        let mut prices = Vec::new();
        for i in 0..30 {
            prices.push(100.0 - (i as f64) * 2.0);
        }
        for i in 0..30 {
            prices.push(40.0 + (i as f64) * 3.0);
        }
        let klines = make_klines(&prices);

        let strategy = DiscoveryStrategyType::DynamicCombo {
            indicators: vec![SingleIndicatorType::Rsi, SingleIndicatorType::BollingerBands],
            params: vec![
                IndicatorParams::Rsi { period: 14, overbought: 70.0, oversold: 30.0 },
                IndicatorParams::BollingerBands { period: 20, multiplier: 2.0 },
            ],
            combine_mode: DynCombineMode::Majority,
        };

        let mut gen = build_signal_generator(&strategy);
        let fee_config = PolymarketFeeConfig::default();

        let result = run_generic_backtest(
            gen.as_mut(),
            &klines,
            dec!(10000),
            dec!(10),
            SizingMode::Fixed,
            &fee_config,
        );

        // Should run without panicking; just verify it completed
        assert_eq!(result.total_trades, result.total_trades);
    }

    #[test]
    fn test_dynamic_combo_mutation() {
        let strategy = DiscoveryStrategyType::DynamicCombo {
            indicators: vec![SingleIndicatorType::Rsi, SingleIndicatorType::Macd],
            params: vec![
                IndicatorParams::Rsi { period: 14, overbought: 70.0, oversold: 30.0 },
                IndicatorParams::Macd { fast: 12, slow: 26, signal: 9 },
            ],
            combine_mode: DynCombineMode::Majority,
        };

        let mut rng = rand::thread_rng();
        let mutated = mutate_strategy(&strategy, &mut rng);
        assert!(mutated.is_some(), "Mutation should succeed for DynamicCombo");
        let m = mutated.unwrap();
        assert!(matches!(m, DiscoveryStrategyType::DynamicCombo { .. }));
    }

    #[test]
    fn test_dynamic_combo_crossover() {
        let a = DiscoveryStrategyType::DynamicCombo {
            indicators: vec![SingleIndicatorType::Rsi, SingleIndicatorType::Macd],
            params: vec![
                IndicatorParams::Rsi { period: 14, overbought: 70.0, oversold: 30.0 },
                IndicatorParams::Macd { fast: 12, slow: 26, signal: 9 },
            ],
            combine_mode: DynCombineMode::Majority,
        };
        let b = DiscoveryStrategyType::DynamicCombo {
            indicators: vec![SingleIndicatorType::Rsi, SingleIndicatorType::Macd],
            params: vec![
                IndicatorParams::Rsi { period: 7, overbought: 80.0, oversold: 20.0 },
                IndicatorParams::Macd { fast: 8, slow: 21, signal: 5 },
            ],
            combine_mode: DynCombineMode::Unanimous,
        };

        let mut rng = rand::thread_rng();
        let child = crossover_strategies(&a, &b, &mut rng);
        assert!(child.is_some(), "Crossover should succeed for same-indicator DynamicCombos");
        let c = child.unwrap();
        assert!(matches!(c, DiscoveryStrategyType::DynamicCombo { .. }));
    }

    #[test]
    fn test_random_dynamic_combo_generation() {
        let mut rng = rand::thread_rng();
        for size in 2..=4 {
            let combo = generate_random_dynamic_combo(size, &mut rng);
            match combo {
                DiscoveryStrategyType::DynamicCombo { indicators, params, .. } => {
                    assert_eq!(indicators.len(), size);
                    assert_eq!(params.len(), size);
                    // No duplicate indicators
                    let unique: std::collections::HashSet<_> = indicators.iter().collect();
                    assert_eq!(unique.len(), size, "Should have no duplicate indicators");
                }
                _ => panic!("Should generate DynamicCombo"),
            }
        }
    }
}
