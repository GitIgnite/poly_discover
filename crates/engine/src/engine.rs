//! Bar-by-bar backtesting engine

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::{debug, info};

use crate::strategy::{RsiStrategy, Signal};
use crate::types::*;

/// Position state during simulation
struct OpenPosition {
    entry_time: i64,
    entry_price: Decimal,
    size: Decimal,
    side: TradeSide,
}

/// Backtesting engine that simulates bar-by-bar execution
pub struct BacktestEngine;

impl BacktestEngine {
    /// Run a backtest on the given klines with the specified config
    pub fn run(config: &BacktestConfig, klines: &[Kline]) -> BacktestResult {
        let mut strategy = RsiStrategy::new(
            config.rsi_period,
            config.rsi_overbought,
            config.rsi_oversold,
        );

        let mut equity = config.initial_capital;
        let mut peak_equity = equity;
        let mut max_drawdown = Decimal::ZERO;
        let mut max_drawdown_pct = Decimal::ZERO;

        let mut trades: Vec<BacktestTrade> = Vec::new();
        let mut equity_curve: Vec<EquityPoint> = Vec::new();
        let mut position: Option<OpenPosition> = None;

        let hundred = dec!(100);

        info!(
            symbol = %config.symbol,
            bars = klines.len(),
            capital = %config.initial_capital,
            rsi_period = config.rsi_period,
            "Starting backtest"
        );

        for kline in klines {
            let signal = strategy.on_bar(kline);

            match signal {
                Signal::Buy => {
                    if position.is_none() {
                        // Open a long position
                        let position_value = equity * config.position_size_pct / hundred;
                        let size = position_value / kline.close;

                        position = Some(OpenPosition {
                            entry_time: kline.open_time,
                            entry_price: kline.close,
                            size,
                            side: TradeSide::Buy,
                        });

                        debug!(
                            price = %kline.close,
                            size = %size,
                            time = kline.open_time,
                            "Opened LONG position"
                        );
                    }
                }
                Signal::Sell => {
                    if let Some(pos) = position.take() {
                        // Close the position
                        let pnl = match pos.side {
                            TradeSide::Buy => (kline.close - pos.entry_price) * pos.size,
                            TradeSide::Sell => (pos.entry_price - kline.close) * pos.size,
                        };

                        let pnl_pct = if pos.entry_price > Decimal::ZERO {
                            (kline.close - pos.entry_price) / pos.entry_price * hundred
                        } else {
                            Decimal::ZERO
                        };

                        equity += pnl;

                        trades.push(BacktestTrade {
                            entry_time: pos.entry_time,
                            exit_time: kline.open_time,
                            side: pos.side,
                            entry_price: pos.entry_price,
                            exit_price: kline.close,
                            size: pos.size,
                            pnl,
                            pnl_pct,
                        });

                        debug!(
                            entry = %pos.entry_price,
                            exit = %kline.close,
                            pnl = %pnl,
                            "Closed position"
                        );
                    }
                }
                Signal::Hold => {}
            }

            // Track equity curve
            let unrealized = if let Some(ref pos) = position {
                match pos.side {
                    TradeSide::Buy => (kline.close - pos.entry_price) * pos.size,
                    TradeSide::Sell => (pos.entry_price - kline.close) * pos.size,
                }
            } else {
                Decimal::ZERO
            };

            let current_equity = equity + unrealized;

            equity_curve.push(EquityPoint {
                time: kline.open_time,
                equity: current_equity,
            });

            // Track max drawdown
            if current_equity > peak_equity {
                peak_equity = current_equity;
            }
            let drawdown = peak_equity - current_equity;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
                if peak_equity > Decimal::ZERO {
                    max_drawdown_pct = drawdown / peak_equity * hundred;
                }
            }
        }

        // Close any remaining open position at last bar
        if let Some(pos) = position.take() {
            if let Some(last_kline) = klines.last() {
                let pnl = match pos.side {
                    TradeSide::Buy => (last_kline.close - pos.entry_price) * pos.size,
                    TradeSide::Sell => (pos.entry_price - last_kline.close) * pos.size,
                };
                let pnl_pct = if pos.entry_price > Decimal::ZERO {
                    (last_kline.close - pos.entry_price) / pos.entry_price * hundred
                } else {
                    Decimal::ZERO
                };
                equity += pnl;

                trades.push(BacktestTrade {
                    entry_time: pos.entry_time,
                    exit_time: last_kline.open_time,
                    side: pos.side,
                    entry_price: pos.entry_price,
                    exit_price: last_kline.close,
                    size: pos.size,
                    pnl,
                    pnl_pct,
                });
            }
        }

        // Calculate final metrics
        let total_trades = trades.len() as u32;
        let winning_trades = trades.iter().filter(|t| t.pnl > Decimal::ZERO).count() as u32;
        let losing_trades = trades.iter().filter(|t| t.pnl <= Decimal::ZERO).count() as u32;

        let win_rate = if total_trades > 0 {
            Decimal::from(winning_trades) / Decimal::from(total_trades) * hundred
        } else {
            Decimal::ZERO
        };

        let total_pnl = equity - config.initial_capital;
        let total_pnl_pct = if config.initial_capital > Decimal::ZERO {
            total_pnl / config.initial_capital * hundred
        } else {
            Decimal::ZERO
        };

        // Profit factor = gross profits / gross losses
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
            dec!(999.99) // Infinite profit factor capped
        } else {
            Decimal::ZERO
        };

        // Simplified Sharpe ratio: mean return / std dev of returns
        let sharpe_ratio = Self::calculate_sharpe(&trades);

        let start_time = klines.first().map(|k| k.open_time).unwrap_or(0);
        let end_time = klines.last().map(|k| k.close_time).unwrap_or(0);

        info!(
            total_trades,
            winning_trades,
            win_rate = %win_rate,
            total_pnl = %total_pnl,
            max_drawdown = %max_drawdown,
            "Backtest complete"
        );

        BacktestResult {
            symbol: config.symbol.clone(),
            interval: config.interval.clone(),
            start_time,
            end_time,
            initial_capital: config.initial_capital,
            final_equity: equity,
            total_pnl,
            total_pnl_pct,
            total_trades,
            winning_trades,
            losing_trades,
            win_rate,
            max_drawdown,
            max_drawdown_pct,
            sharpe_ratio,
            profit_factor,
            trades,
            equity_curve,
            klines: klines.to_vec(),
        }
    }

    /// Calculate simplified annualized Sharpe ratio from trade returns
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

        // Convert back to Decimal, round to 2 decimal places
        Decimal::from_str_exact(&format!("{:.2}", sharpe)).unwrap_or(Decimal::ZERO)
    }
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
                    open_time: (i as i64) * 60000,
                    open: price,
                    high: price + dec!(1),
                    low: price - dec!(1),
                    close: price,
                    volume: dec!(100),
                    close_time: ((i + 1) as i64) * 60000 - 1,
                }
            })
            .collect()
    }

    #[test]
    fn test_empty_klines() {
        let config = BacktestConfig::default();
        let result = BacktestEngine::run(&config, &[]);
        assert_eq!(result.total_trades, 0);
        assert_eq!(result.total_pnl, Decimal::ZERO);
    }

    #[test]
    fn test_backtest_runs_without_panic() {
        // Generate enough data for RSI to produce signals
        let mut prices = Vec::new();
        // Trending down to trigger oversold
        for i in 0..20 {
            prices.push(100.0 - (i as f64) * 2.0);
        }
        // Trending up to trigger overbought
        for i in 0..20 {
            prices.push(60.0 + (i as f64) * 3.0);
        }

        let klines = make_klines(&prices);
        let config = BacktestConfig {
            rsi_period: 14,
            rsi_overbought: 70.0,
            rsi_oversold: 30.0,
            initial_capital: dec!(10000),
            position_size_pct: dec!(10),
            ..Default::default()
        };

        let result = BacktestEngine::run(&config, &klines);
        assert_eq!(result.initial_capital, dec!(10000));
        assert!(!result.equity_curve.is_empty());
    }
}
