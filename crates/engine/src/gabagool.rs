//! Gabagool binary arbitrage backtest engine
//!
//! Simulates 1 month of Gabagool strategy using Binance BTC 15m klines.
//! Each 15m candle becomes a synthetic Polymarket binary market "BTC up or down?".
//! We model YES/NO prices with realistic spreads, then simulate maker buys on both
//! sides to calculate pair cost and locked profit.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::info;

use crate::types::{EquityPoint, Kline};
use serde::{Deserialize, Serialize};

/// Configuration for a Gabagool backtest run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GabagoolBacktestConfig {
    /// Trading symbol (e.g. "BTCUSDT")
    pub symbol: String,
    /// Number of days to backtest
    pub days: u32,
    /// Size in USDC per side (YES and NO)
    pub size_per_side: Decimal,
    /// Maximum pair cost threshold (e.g. 0.98)
    pub max_pair_cost: Decimal,
    /// Bid offset below mid price for maker orders
    pub bid_offset: Decimal,
    /// Spread multiplier: spread = volatility * multiplier
    pub spread_multiplier: Decimal,
}

impl Default for GabagoolBacktestConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            days: 30,
            size_per_side: dec!(10),
            max_pair_cost: dec!(0.98),
            bid_offset: dec!(0.01),
            spread_multiplier: dec!(3),
        }
    }
}

/// Result for a single 15m window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GabagoolWindowResult {
    /// Timestamp of the window
    pub time: i64,
    /// Synthetic YES fill price
    pub yes_fill: Decimal,
    /// Synthetic NO fill price
    pub no_fill: Decimal,
    /// Pair cost = yes_fill + no_fill
    pub pair_cost: Decimal,
    /// Locked profit = size * (1.00 - pair_cost), or 0 if skipped
    pub locked_profit: Decimal,
    /// Whether a trade was executed in this window
    pub traded: bool,
    /// BTC open price
    pub btc_open: Decimal,
    /// BTC close price
    pub btc_close: Decimal,
    /// Synthetic spread
    pub spread: Decimal,
}

/// Aggregated result of a Gabagool backtest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GabagoolBacktestResult {
    /// Configuration used
    pub config: GabagoolBacktestConfig,
    /// Start timestamp
    pub start_time: i64,
    /// End timestamp
    pub end_time: i64,
    /// Total number of 15m windows analyzed
    pub total_windows: u32,
    /// Windows where a trade was executed
    pub traded_windows: u32,
    /// Windows skipped (pair cost too high)
    pub skipped_windows: u32,
    /// Hit rate: traded / total (%)
    pub hit_rate: Decimal,
    /// Total capital deployed (sum of size_per_side * 2 per trade)
    pub total_capital_used: Decimal,
    /// Total locked profit across all trades
    pub total_locked_profit: Decimal,
    /// Average pair cost on executed trades
    pub avg_pair_cost: Decimal,
    /// Average locked profit per traded window
    pub avg_locked_profit: Decimal,
    /// Best (lowest) pair cost seen
    pub best_pair_cost: Decimal,
    /// Worst (highest) pair cost among traded windows
    pub worst_pair_cost: Decimal,
    /// Average spread across all windows
    pub avg_spread: Decimal,
    /// Cumulative profit curve
    pub profit_curve: Vec<EquityPoint>,
    /// Per-window detail
    pub windows: Vec<GabagoolWindowResult>,
}

/// Gabagool backtest engine
pub struct GabagoolBacktestEngine;

impl GabagoolBacktestEngine {
    /// Run the Gabagool backtest on the provided klines
    pub fn run(config: &GabagoolBacktestConfig, klines: &[Kline]) -> GabagoolBacktestResult {
        let mut windows = Vec::with_capacity(klines.len());
        let mut profit_curve = Vec::with_capacity(klines.len());
        let mut cumulative_profit = Decimal::ZERO;
        let mut total_capital_used = Decimal::ZERO;

        let mut traded_count: u32 = 0;
        let mut pair_cost_sum = Decimal::ZERO;
        let mut profit_sum = Decimal::ZERO;
        let mut spread_sum = Decimal::ZERO;
        let mut best_pair_cost = Decimal::MAX;
        let mut worst_traded_pair_cost = Decimal::ZERO;

        let hundred = dec!(100);
        let one = Decimal::ONE;
        let half = dec!(0.50);
        let two = dec!(2);
        let clamp_min = dec!(-0.40);
        let clamp_max = dec!(0.40);
        let spread_floor = dec!(0.02);
        let spread_ceil = dec!(0.10);
        let fill_floor = dec!(0.05);
        let fill_ceil = dec!(0.95);
        // Scale factor for converting price move % to YES price deviation
        let move_scale = dec!(5);

        info!(
            symbol = %config.symbol,
            bars = klines.len(),
            max_pair_cost = %config.max_pair_cost,
            "Starting Gabagool backtest"
        );

        for kline in klines {
            // 1. Calculate price move percentage
            let price_move_pct = if kline.open > Decimal::ZERO {
                (kline.close - kline.open) / kline.open
            } else {
                Decimal::ZERO
            };

            // 2. Calculate volatility from high-low range
            let volatility = if kline.open > Decimal::ZERO {
                (kline.high - kline.low) / kline.open
            } else {
                Decimal::ZERO
            };

            // 3. YES mid price: 0.50 + clamp(price_move * 5, -0.40, 0.40)
            let deviation = (price_move_pct * move_scale).max(clamp_min).min(clamp_max);
            let yes_mid = half + deviation;
            let no_mid = one - yes_mid;

            // 4. Spread based on volatility
            let spread = (volatility * config.spread_multiplier)
                .max(spread_floor)
                .min(spread_ceil);

            // 5. Maker fill prices (bid below mid)
            let yes_fill = (yes_mid - spread / two - config.bid_offset)
                .max(fill_floor)
                .min(fill_ceil);
            let no_fill = (no_mid - spread / two - config.bid_offset)
                .max(fill_floor)
                .min(fill_ceil);

            let pair_cost = yes_fill + no_fill;

            // Track best pair cost overall
            if pair_cost < best_pair_cost {
                best_pair_cost = pair_cost;
            }

            spread_sum += spread;

            // 6. Decide whether to trade
            let traded = pair_cost < config.max_pair_cost;
            let locked_profit = if traded {
                let profit = config.size_per_side * (one - pair_cost);
                traded_count += 1;
                pair_cost_sum += pair_cost;
                profit_sum += profit;
                total_capital_used += config.size_per_side * two;
                cumulative_profit += profit;

                if pair_cost > worst_traded_pair_cost {
                    worst_traded_pair_cost = pair_cost;
                }

                profit
            } else {
                Decimal::ZERO
            };

            windows.push(GabagoolWindowResult {
                time: kline.open_time,
                yes_fill,
                no_fill,
                pair_cost,
                locked_profit,
                traded,
                btc_open: kline.open,
                btc_close: kline.close,
                spread,
            });

            profit_curve.push(EquityPoint {
                time: kline.open_time,
                equity: cumulative_profit,
            });
        }

        let total_windows = klines.len() as u32;
        let skipped_windows = total_windows - traded_count;

        let hit_rate = if total_windows > 0 {
            Decimal::from(traded_count) / Decimal::from(total_windows) * hundred
        } else {
            Decimal::ZERO
        };

        let avg_pair_cost = if traded_count > 0 {
            pair_cost_sum / Decimal::from(traded_count)
        } else {
            Decimal::ZERO
        };

        let avg_locked_profit = if traded_count > 0 {
            profit_sum / Decimal::from(traded_count)
        } else {
            Decimal::ZERO
        };

        let avg_spread = if total_windows > 0 {
            spread_sum / Decimal::from(total_windows)
        } else {
            Decimal::ZERO
        };

        // If no windows at all, reset best_pair_cost
        if best_pair_cost == Decimal::MAX {
            best_pair_cost = Decimal::ZERO;
        }

        let start_time = klines.first().map(|k| k.open_time).unwrap_or(0);
        let end_time = klines.last().map(|k| k.close_time).unwrap_or(0);

        info!(
            total_windows,
            traded_count,
            hit_rate = %hit_rate,
            total_locked_profit = %cumulative_profit,
            avg_pair_cost = %avg_pair_cost,
            "Gabagool backtest complete"
        );

        GabagoolBacktestResult {
            config: config.clone(),
            start_time,
            end_time,
            total_windows,
            traded_windows: traded_count,
            skipped_windows,
            hit_rate,
            total_capital_used,
            total_locked_profit: cumulative_profit,
            avg_pair_cost,
            avg_locked_profit,
            best_pair_cost,
            worst_pair_cost: worst_traded_pair_cost,
            avg_spread,
            profit_curve,
            windows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn make_kline(open: f64, close: f64, high: f64, low: f64, time_idx: i64) -> Kline {
        Kline {
            open_time: time_idx * 900_000, // 15 min intervals
            open: Decimal::from_str_exact(&format!("{:.2}", open)).unwrap(),
            high: Decimal::from_str_exact(&format!("{:.2}", high)).unwrap(),
            low: Decimal::from_str_exact(&format!("{:.2}", low)).unwrap(),
            close: Decimal::from_str_exact(&format!("{:.2}", close)).unwrap(),
            volume: dec!(100),
            close_time: (time_idx + 1) * 900_000 - 1,
        }
    }

    use rust_decimal::Decimal;

    #[test]
    fn test_empty_klines() {
        let config = GabagoolBacktestConfig::default();
        let result = GabagoolBacktestEngine::run(&config, &[]);
        assert_eq!(result.total_windows, 0);
        assert_eq!(result.traded_windows, 0);
        assert_eq!(result.total_locked_profit, Decimal::ZERO);
        assert_eq!(result.hit_rate, Decimal::ZERO);
    }

    #[test]
    fn test_single_flat_candle() {
        // Flat candle: open == close, minimal range
        let klines = vec![make_kline(50000.0, 50000.0, 50010.0, 49990.0, 0)];
        let config = GabagoolBacktestConfig::default();
        let result = GabagoolBacktestEngine::run(&config, &klines);

        assert_eq!(result.total_windows, 1);
        assert!(result.windows[0].pair_cost < dec!(0.98));
        assert!(result.windows[0].traded);
        assert!(result.total_locked_profit > Decimal::ZERO);
    }

    #[test]
    fn test_pair_cost_always_below_one() {
        let klines = vec![
            make_kline(50000.0, 51000.0, 51500.0, 49500.0, 0), // strong up
            make_kline(50000.0, 49000.0, 50500.0, 48500.0, 1), // strong down
            make_kline(50000.0, 50000.0, 50010.0, 49990.0, 2), // flat
            make_kline(50000.0, 50250.0, 50500.0, 49800.0, 3), // mild up
        ];
        let config = GabagoolBacktestConfig::default();
        let result = GabagoolBacktestEngine::run(&config, &klines);

        for w in &result.windows {
            assert!(
                w.pair_cost < Decimal::ONE,
                "pair_cost {} should be < 1.0",
                w.pair_cost
            );
        }
    }

    #[test]
    fn test_high_max_pair_cost_trades_everything() {
        let klines = vec![
            make_kline(50000.0, 51000.0, 51500.0, 49500.0, 0),
            make_kline(50000.0, 49000.0, 50500.0, 48500.0, 1),
            make_kline(50000.0, 50100.0, 50200.0, 49900.0, 2),
        ];
        let config = GabagoolBacktestConfig {
            max_pair_cost: dec!(0.999),
            ..Default::default()
        };
        let result = GabagoolBacktestEngine::run(&config, &klines);

        assert_eq!(result.traded_windows, result.total_windows);
    }

    #[test]
    fn test_very_low_max_pair_cost_skips_all() {
        let klines = vec![
            make_kline(50000.0, 50100.0, 50200.0, 49900.0, 0),
            make_kline(50000.0, 49900.0, 50200.0, 49800.0, 1),
        ];
        let config = GabagoolBacktestConfig {
            max_pair_cost: dec!(0.50), // Impossibly low
            ..Default::default()
        };
        let result = GabagoolBacktestEngine::run(&config, &klines);

        assert_eq!(result.traded_windows, 0);
        assert_eq!(result.total_locked_profit, Decimal::ZERO);
    }

    #[test]
    fn test_profit_curve_is_monotonic() {
        let klines: Vec<Kline> = (0..20)
            .map(|i| {
                let open = 50000.0 + (i as f64) * 100.0;
                let close = open + 50.0;
                make_kline(open, close, open + 200.0, open - 200.0, i)
            })
            .collect();

        let config = GabagoolBacktestConfig::default();
        let result = GabagoolBacktestEngine::run(&config, &klines);

        for i in 1..result.profit_curve.len() {
            assert!(
                result.profit_curve[i].equity >= result.profit_curve[i - 1].equity,
                "Profit curve must be monotonically non-decreasing"
            );
        }
    }

    #[test]
    fn test_aggregates_consistency() {
        let klines: Vec<Kline> = (0..10)
            .map(|i| make_kline(50000.0, 50000.0 + (i as f64) * 50.0, 50300.0, 49700.0, i))
            .collect();

        let config = GabagoolBacktestConfig::default();
        let result = GabagoolBacktestEngine::run(&config, &klines);

        assert_eq!(
            result.total_windows,
            result.traded_windows + result.skipped_windows
        );

        if result.traded_windows > 0 {
            let expected = result.avg_locked_profit * Decimal::from(result.traded_windows);
            let diff = (result.total_locked_profit - expected).abs();
            assert!(
                diff < dec!(0.0001),
                "Aggregate consistency: total={} expected={}",
                result.total_locked_profit,
                expected
            );
        }
    }
}
