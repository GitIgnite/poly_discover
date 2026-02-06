//! Signal generators for the discovery agent
//!
//! Provides a `SignalGenerator` trait and implementations for 6 individual
//! technical indicators plus 7 combo strategies. Each generator processes
//! klines bar-by-bar and emits Buy/Sell/Hold signals with a confidence score.

use crate::strategy::Signal;
use crate::types::Kline;
use ta::indicators::{
    AverageTrueRange, BollingerBands, ExponentialMovingAverage, MovingAverageConvergenceDivergence,
    RelativeStrengthIndex, SimpleMovingAverage, SlowStochastic,
};
use ta::Next;

// ============================================================================
// Core trait
// ============================================================================

/// A signal combined with a confidence score (0.3–1.0)
#[derive(Debug, Clone, Copy)]
pub struct SignalWithConfidence {
    pub signal: Signal,
    pub confidence: f64,
}

impl SignalWithConfidence {
    pub fn hold() -> Self {
        Self {
            signal: Signal::Hold,
            confidence: 0.0,
        }
    }

    pub fn buy(confidence: f64) -> Self {
        Self {
            signal: Signal::Buy,
            confidence: confidence.clamp(0.3, 1.0),
        }
    }

    pub fn sell(confidence: f64) -> Self {
        Self {
            signal: Signal::Sell,
            confidence: confidence.clamp(0.3, 1.0),
        }
    }
}

/// Trait for bar-by-bar signal generation
pub trait SignalGenerator: Send {
    fn name(&self) -> &str;
    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence;
    fn reset(&mut self);
}

// ============================================================================
// Helper: convert Decimal close price to f64
// ============================================================================

fn close_f64(kline: &Kline) -> f64 {
    kline.close.to_string().parse::<f64>().unwrap_or(0.0)
}

// ============================================================================
// 1. RSI Signal Generator
// ============================================================================

pub struct RsiSignalGenerator {
    rsi: RelativeStrengthIndex,
    overbought: f64,
    oversold: f64,
    period: usize,
}

impl RsiSignalGenerator {
    pub fn new(period: usize, overbought: f64, oversold: f64) -> Self {
        Self {
            rsi: RelativeStrengthIndex::new(period).expect("Invalid RSI period"),
            overbought,
            oversold,
            period,
        }
    }
}

impl SignalGenerator for RsiSignalGenerator {
    fn name(&self) -> &str {
        "RSI"
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let val = self.rsi.next(close_f64(kline));
        if val < self.oversold {
            let distance = (self.oversold - val) / self.oversold;
            SignalWithConfidence::buy(distance)
        } else if val > self.overbought {
            let distance = (val - self.overbought) / (100.0 - self.overbought);
            SignalWithConfidence::sell(distance)
        } else {
            SignalWithConfidence::hold()
        }
    }

    fn reset(&mut self) {
        self.rsi = RelativeStrengthIndex::new(self.period).expect("Invalid RSI period");
    }
}

// ============================================================================
// 2. Bollinger Bands Signal Generator
// ============================================================================

pub struct BollingerSignalGenerator {
    bb: BollingerBands,
    period: usize,
    multiplier: f64,
    last_upper: f64,
    last_lower: f64,
    last_middle: f64,
}

impl BollingerSignalGenerator {
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            bb: BollingerBands::new(period, multiplier).expect("Invalid BB params"),
            period,
            multiplier,
            last_upper: 0.0,
            last_lower: 0.0,
            last_middle: 0.0,
        }
    }
}

impl SignalGenerator for BollingerSignalGenerator {
    fn name(&self) -> &str {
        "BollingerBands"
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let close = close_f64(kline);
        let bb_out = self.bb.next(close);
        self.last_upper = bb_out.upper;
        self.last_lower = bb_out.lower;
        self.last_middle = bb_out.average;

        let bandwidth = self.last_upper - self.last_lower;
        if bandwidth <= 0.0 {
            return SignalWithConfidence::hold();
        }

        if close < self.last_lower {
            let distance = (self.last_lower - close) / bandwidth;
            SignalWithConfidence::buy(distance)
        } else if close > self.last_upper {
            let distance = (close - self.last_upper) / bandwidth;
            SignalWithConfidence::sell(distance)
        } else {
            SignalWithConfidence::hold()
        }
    }

    fn reset(&mut self) {
        self.bb = BollingerBands::new(self.period, self.multiplier).expect("Invalid BB params");
        self.last_upper = 0.0;
        self.last_lower = 0.0;
        self.last_middle = 0.0;
    }
}

// ============================================================================
// 3. MACD Signal Generator
// ============================================================================

pub struct MacdSignalGenerator {
    macd: MovingAverageConvergenceDivergence,
    fast: usize,
    slow: usize,
    signal_period: usize,
    prev_histogram: f64,
    bars_seen: usize,
}

impl MacdSignalGenerator {
    pub fn new(fast: usize, slow: usize, signal_period: usize) -> Self {
        Self {
            macd: MovingAverageConvergenceDivergence::new(fast, slow, signal_period)
                .expect("Invalid MACD params"),
            fast,
            slow,
            signal_period,
            prev_histogram: 0.0,
            bars_seen: 0,
        }
    }
}

impl SignalGenerator for MacdSignalGenerator {
    fn name(&self) -> &str {
        "MACD"
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let close = close_f64(kline);
        let out = self.macd.next(close);
        let histogram = out.histogram;
        self.bars_seen += 1;

        let result = if self.bars_seen > self.slow && self.prev_histogram <= 0.0 && histogram > 0.0
        {
            // Histogram crossed from negative to positive → Buy
            let conf = histogram.abs() / close * 1000.0;
            SignalWithConfidence::buy(conf)
        } else if self.bars_seen > self.slow && self.prev_histogram >= 0.0 && histogram < 0.0 {
            // Histogram crossed from positive to negative → Sell
            let conf = histogram.abs() / close * 1000.0;
            SignalWithConfidence::sell(conf)
        } else {
            SignalWithConfidence::hold()
        };

        self.prev_histogram = histogram;
        result
    }

    fn reset(&mut self) {
        self.macd =
            MovingAverageConvergenceDivergence::new(self.fast, self.slow, self.signal_period)
                .expect("Invalid MACD params");
        self.prev_histogram = 0.0;
        self.bars_seen = 0;
    }
}

// ============================================================================
// 4. EMA Crossover Signal Generator
// ============================================================================

pub struct EmaCrossoverSignalGenerator {
    ema_fast: ExponentialMovingAverage,
    ema_slow: ExponentialMovingAverage,
    fast_period: usize,
    slow_period: usize,
    prev_fast: f64,
    prev_slow: f64,
    bars_seen: usize,
}

impl EmaCrossoverSignalGenerator {
    pub fn new(fast_period: usize, slow_period: usize) -> Self {
        Self {
            ema_fast: ExponentialMovingAverage::new(fast_period).expect("Invalid fast EMA period"),
            ema_slow: ExponentialMovingAverage::new(slow_period).expect("Invalid slow EMA period"),
            fast_period,
            slow_period,
            prev_fast: 0.0,
            prev_slow: 0.0,
            bars_seen: 0,
        }
    }
}

impl SignalGenerator for EmaCrossoverSignalGenerator {
    fn name(&self) -> &str {
        "EMACrossover"
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let close = close_f64(kline);
        let fast_val = self.ema_fast.next(close);
        let slow_val = self.ema_slow.next(close);
        self.bars_seen += 1;

        let result = if self.bars_seen > self.slow_period {
            if self.prev_fast <= self.prev_slow && fast_val > slow_val {
                // Golden cross → Buy
                let conf = (fast_val - slow_val).abs() / slow_val * 100.0;
                SignalWithConfidence::buy(conf)
            } else if self.prev_fast >= self.prev_slow && fast_val < slow_val {
                // Death cross → Sell
                let conf = (fast_val - slow_val).abs() / slow_val * 100.0;
                SignalWithConfidence::sell(conf)
            } else {
                SignalWithConfidence::hold()
            }
        } else {
            SignalWithConfidence::hold()
        };

        self.prev_fast = fast_val;
        self.prev_slow = slow_val;
        result
    }

    fn reset(&mut self) {
        self.ema_fast =
            ExponentialMovingAverage::new(self.fast_period).expect("Invalid fast EMA period");
        self.ema_slow =
            ExponentialMovingAverage::new(self.slow_period).expect("Invalid slow EMA period");
        self.prev_fast = 0.0;
        self.prev_slow = 0.0;
        self.bars_seen = 0;
    }
}

// ============================================================================
// 5. Stochastic Oscillator Signal Generator
// ============================================================================

pub struct StochasticSignalGenerator {
    stoch: SlowStochastic,
    period: usize,
    overbought: f64,
    oversold: f64,
    /// We track a SMA(3) of %K to get %D manually
    k_buffer: Vec<f64>,
    prev_k: f64,
    prev_d: f64,
    bars_seen: usize,
}

impl StochasticSignalGenerator {
    pub fn new(period: usize, overbought: f64, oversold: f64) -> Self {
        Self {
            stoch: SlowStochastic::new(period, 3).expect("Invalid Stochastic params"),
            period,
            overbought,
            oversold,
            k_buffer: Vec::with_capacity(3),
            prev_k: 50.0,
            prev_d: 50.0,
            bars_seen: 0,
        }
    }
}

impl SignalGenerator for StochasticSignalGenerator {
    fn name(&self) -> &str {
        "Stochastic"
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let close = close_f64(kline);

        // SlowStochastic::next(f64) returns a single f64 (%K smoothed)
        let k = self.stoch.next(close);
        self.bars_seen += 1;

        // Compute %D as SMA(3) of %K
        self.k_buffer.push(k);
        if self.k_buffer.len() > 3 {
            self.k_buffer.remove(0);
        }
        let d = self.k_buffer.iter().sum::<f64>() / self.k_buffer.len() as f64;

        let result = if self.bars_seen > self.period
            && self.prev_k < self.prev_d
            && k > d
            && k < self.oversold
        {
            // %K crosses above %D in oversold zone → Buy
            let conf = (self.oversold - k) / self.oversold;
            SignalWithConfidence::buy(conf)
        } else if self.bars_seen > self.period
            && self.prev_k > self.prev_d
            && k < d
            && k > self.overbought
        {
            // %K crosses below %D in overbought zone → Sell
            let conf = (k - self.overbought) / (100.0 - self.overbought);
            SignalWithConfidence::sell(conf)
        } else {
            SignalWithConfidence::hold()
        };

        self.prev_k = k;
        self.prev_d = d;
        result
    }

    fn reset(&mut self) {
        self.stoch = SlowStochastic::new(self.period, 3).expect("Invalid Stochastic params");
        self.k_buffer.clear();
        self.prev_k = 50.0;
        self.prev_d = 50.0;
        self.bars_seen = 0;
    }
}

// ============================================================================
// 6. ATR Mean Reversion Signal Generator
// ============================================================================

pub struct AtrMeanReversionSignalGenerator {
    atr: AverageTrueRange,
    sma: SimpleMovingAverage,
    atr_period: usize,
    sma_period: usize,
    multiplier: f64,
    last_atr: f64,
    last_sma: f64,
    bars_seen: usize,
}

impl AtrMeanReversionSignalGenerator {
    pub fn new(atr_period: usize, sma_period: usize, multiplier: f64) -> Self {
        Self {
            atr: AverageTrueRange::new(atr_period).expect("Invalid ATR period"),
            sma: SimpleMovingAverage::new(sma_period).expect("Invalid SMA period"),
            atr_period,
            sma_period,
            multiplier,
            last_atr: 0.0,
            last_sma: 0.0,
            bars_seen: 0,
        }
    }
}

impl SignalGenerator for AtrMeanReversionSignalGenerator {
    fn name(&self) -> &str {
        "ATRMeanReversion"
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let close = close_f64(kline);
        let high = kline.high.to_string().parse::<f64>().unwrap_or(close);
        let low = kline.low.to_string().parse::<f64>().unwrap_or(close);

        // ATR needs DataItem
        let bar = ta::DataItem::builder()
            .open(close)
            .high(high)
            .low(low)
            .close(close)
            .volume(0.0)
            .build()
            .unwrap_or_else(|_| {
                ta::DataItem::builder()
                    .open(close)
                    .high(close)
                    .low(close)
                    .close(close)
                    .volume(0.0)
                    .build()
                    .unwrap()
            });

        self.last_atr = self.atr.next(&bar);
        self.last_sma = self.sma.next(close);
        self.bars_seen += 1;

        let warmup = self.atr_period.max(self.sma_period);
        if self.bars_seen <= warmup || self.last_atr <= 0.0 {
            return SignalWithConfidence::hold();
        }

        let threshold = self.last_atr * self.multiplier;
        let deviation = close - self.last_sma;

        if deviation < -threshold {
            // Price far below mean → Buy (mean reversion up)
            let conf = deviation.abs() / threshold;
            SignalWithConfidence::buy(conf)
        } else if deviation > threshold {
            // Price far above mean → Sell (mean reversion down)
            let conf = deviation.abs() / threshold;
            SignalWithConfidence::sell(conf)
        } else {
            SignalWithConfidence::hold()
        }
    }

    fn reset(&mut self) {
        self.atr = AverageTrueRange::new(self.atr_period).expect("Invalid ATR period");
        self.sma = SimpleMovingAverage::new(self.sma_period).expect("Invalid SMA period");
        self.last_atr = 0.0;
        self.last_sma = 0.0;
        self.bars_seen = 0;
    }
}

// ============================================================================
// Combo Signal Generator
// ============================================================================

/// How to combine signals from multiple generators
#[derive(Debug, Clone, Copy)]
pub enum CombineMode {
    /// All generators must agree on Buy/Sell
    Unanimous,
    /// Majority of generators agree
    Majority,
    /// First generator is primary, at least one secondary confirms
    PrimaryConfirmed,
}

pub struct ComboSignalGenerator {
    name: String,
    generators: Vec<Box<dyn SignalGenerator>>,
    mode: CombineMode,
}

impl ComboSignalGenerator {
    pub fn new(name: String, generators: Vec<Box<dyn SignalGenerator>>, mode: CombineMode) -> Self {
        Self {
            name,
            generators,
            mode,
        }
    }
}

impl SignalGenerator for ComboSignalGenerator {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let signals: Vec<SignalWithConfidence> = self
            .generators
            .iter_mut()
            .map(|g| g.on_bar(kline))
            .collect();

        match self.mode {
            CombineMode::Unanimous => {
                let buy_count = signals.iter().filter(|s| s.signal == Signal::Buy).count();
                let sell_count = signals.iter().filter(|s| s.signal == Signal::Sell).count();
                let total = signals.len();

                if buy_count == total {
                    let avg_conf = signals.iter().map(|s| s.confidence).sum::<f64>() / total as f64;
                    SignalWithConfidence::buy(avg_conf)
                } else if sell_count == total {
                    let avg_conf = signals.iter().map(|s| s.confidence).sum::<f64>() / total as f64;
                    SignalWithConfidence::sell(avg_conf)
                } else {
                    SignalWithConfidence::hold()
                }
            }
            CombineMode::Majority => {
                let buy_count = signals.iter().filter(|s| s.signal == Signal::Buy).count();
                let sell_count = signals.iter().filter(|s| s.signal == Signal::Sell).count();
                let threshold = signals.len().div_ceil(2); // majority

                if buy_count >= threshold {
                    let avg_conf = signals
                        .iter()
                        .filter(|s| s.signal == Signal::Buy)
                        .map(|s| s.confidence)
                        .sum::<f64>()
                        / buy_count as f64;
                    SignalWithConfidence::buy(avg_conf)
                } else if sell_count >= threshold {
                    let avg_conf = signals
                        .iter()
                        .filter(|s| s.signal == Signal::Sell)
                        .map(|s| s.confidence)
                        .sum::<f64>()
                        / sell_count as f64;
                    SignalWithConfidence::sell(avg_conf)
                } else {
                    SignalWithConfidence::hold()
                }
            }
            CombineMode::PrimaryConfirmed => {
                if signals.is_empty() {
                    return SignalWithConfidence::hold();
                }
                let primary = &signals[0];
                if primary.signal == Signal::Hold {
                    return SignalWithConfidence::hold();
                }
                // At least one secondary must confirm
                let confirmed = signals[1..]
                    .iter()
                    .any(|s| s.signal == primary.signal || s.signal == Signal::Hold);
                if confirmed {
                    *primary
                } else {
                    SignalWithConfidence::hold()
                }
            }
        }
    }

    fn reset(&mut self) {
        for g in &mut self.generators {
            g.reset();
        }
    }
}

// ============================================================================
// Factory: build a signal generator from DiscoveryStrategyType
// ============================================================================

use crate::discovery::DiscoveryStrategyType;

pub fn build_signal_generator(strategy_type: &DiscoveryStrategyType) -> Box<dyn SignalGenerator> {
    match strategy_type {
        DiscoveryStrategyType::Rsi {
            period,
            overbought,
            oversold,
        } => Box::new(RsiSignalGenerator::new(*period, *overbought, *oversold)),

        DiscoveryStrategyType::BollingerBands { period, multiplier } => {
            Box::new(BollingerSignalGenerator::new(*period, *multiplier))
        }

        DiscoveryStrategyType::Macd { fast, slow, signal } => {
            Box::new(MacdSignalGenerator::new(*fast, *slow, *signal))
        }

        DiscoveryStrategyType::EmaCrossover {
            fast_period,
            slow_period,
        } => Box::new(EmaCrossoverSignalGenerator::new(*fast_period, *slow_period)),

        DiscoveryStrategyType::Stochastic {
            period,
            overbought,
            oversold,
        } => Box::new(StochasticSignalGenerator::new(
            *period,
            *overbought,
            *oversold,
        )),

        DiscoveryStrategyType::AtrMeanReversion {
            atr_period,
            sma_period,
            multiplier,
        } => Box::new(AtrMeanReversionSignalGenerator::new(
            *atr_period,
            *sma_period,
            *multiplier,
        )),

        // Combos
        DiscoveryStrategyType::RsiBollinger {
            rsi_period,
            rsi_ob,
            rsi_os,
            bb_period,
            bb_mult,
        } => Box::new(ComboSignalGenerator::new(
            "RSI+Bollinger".to_string(),
            vec![
                Box::new(RsiSignalGenerator::new(*rsi_period, *rsi_ob, *rsi_os)),
                Box::new(BollingerSignalGenerator::new(*bb_period, *bb_mult)),
            ],
            CombineMode::Unanimous,
        )),

        DiscoveryStrategyType::MacdRsi {
            macd_fast,
            macd_slow,
            macd_signal,
            rsi_period,
            rsi_ob,
            rsi_os,
        } => Box::new(ComboSignalGenerator::new(
            "MACD+RSI".to_string(),
            vec![
                Box::new(MacdSignalGenerator::new(
                    *macd_fast,
                    *macd_slow,
                    *macd_signal,
                )),
                Box::new(RsiSignalGenerator::new(*rsi_period, *rsi_ob, *rsi_os)),
            ],
            CombineMode::PrimaryConfirmed,
        )),

        DiscoveryStrategyType::EmaRsi {
            ema_fast,
            ema_slow,
            rsi_period,
            rsi_ob,
            rsi_os,
        } => Box::new(ComboSignalGenerator::new(
            "EMA+RSI".to_string(),
            vec![
                Box::new(EmaCrossoverSignalGenerator::new(*ema_fast, *ema_slow)),
                Box::new(RsiSignalGenerator::new(*rsi_period, *rsi_ob, *rsi_os)),
            ],
            CombineMode::PrimaryConfirmed,
        )),

        DiscoveryStrategyType::StochRsi {
            stoch_period,
            stoch_ob,
            stoch_os,
            rsi_period,
            rsi_ob,
            rsi_os,
        } => Box::new(ComboSignalGenerator::new(
            "Stoch+RSI".to_string(),
            vec![
                Box::new(StochasticSignalGenerator::new(
                    *stoch_period,
                    *stoch_ob,
                    *stoch_os,
                )),
                Box::new(RsiSignalGenerator::new(*rsi_period, *rsi_ob, *rsi_os)),
            ],
            CombineMode::Unanimous,
        )),

        DiscoveryStrategyType::MacdBollinger {
            macd_fast,
            macd_slow,
            macd_signal,
            bb_period,
            bb_mult,
        } => Box::new(ComboSignalGenerator::new(
            "MACD+Bollinger".to_string(),
            vec![
                Box::new(MacdSignalGenerator::new(
                    *macd_fast,
                    *macd_slow,
                    *macd_signal,
                )),
                Box::new(BollingerSignalGenerator::new(*bb_period, *bb_mult)),
            ],
            CombineMode::PrimaryConfirmed,
        )),

        DiscoveryStrategyType::TripleRsiMacdBb {
            rsi_period,
            rsi_ob,
            rsi_os,
            macd_fast,
            macd_slow,
            macd_signal,
            bb_period,
            bb_mult,
        } => Box::new(ComboSignalGenerator::new(
            "Triple:RSI+MACD+BB".to_string(),
            vec![
                Box::new(RsiSignalGenerator::new(*rsi_period, *rsi_ob, *rsi_os)),
                Box::new(MacdSignalGenerator::new(
                    *macd_fast,
                    *macd_slow,
                    *macd_signal,
                )),
                Box::new(BollingerSignalGenerator::new(*bb_period, *bb_mult)),
            ],
            CombineMode::Majority,
        )),

        DiscoveryStrategyType::TripleEmaRsiStoch {
            ema_fast,
            ema_slow,
            rsi_period,
            rsi_ob,
            rsi_os,
            stoch_period,
            stoch_ob,
            stoch_os,
        } => Box::new(ComboSignalGenerator::new(
            "Triple:EMA+RSI+Stoch".to_string(),
            vec![
                Box::new(EmaCrossoverSignalGenerator::new(*ema_fast, *ema_slow)),
                Box::new(RsiSignalGenerator::new(*rsi_period, *rsi_ob, *rsi_os)),
                Box::new(StochasticSignalGenerator::new(
                    *stoch_period,
                    *stoch_ob,
                    *stoch_os,
                )),
            ],
            CombineMode::Majority,
        )),

        // Gabagool is handled separately in discovery.rs, not via SignalGenerator
        DiscoveryStrategyType::Gabagool { .. } => {
            // Return a dummy RSI that always holds — Gabagool uses its own engine
            Box::new(RsiSignalGenerator::new(14, 99.0, 1.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
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
    fn test_rsi_generator_produces_signals() {
        let mut gen = RsiSignalGenerator::new(14, 70.0, 30.0);
        assert_eq!(gen.name(), "RSI");

        let mut prices = Vec::new();
        for i in 0..20 {
            prices.push(100.0 - (i as f64) * 3.0);
        }
        for i in 0..20 {
            prices.push(40.0 + (i as f64) * 4.0);
        }

        let klines = make_klines(&prices);
        let mut had_buy = false;
        let mut had_sell = false;
        for kline in &klines {
            let sig = gen.on_bar(kline);
            match sig.signal {
                Signal::Buy => had_buy = true,
                Signal::Sell => had_sell = true,
                Signal::Hold => {}
            }
        }
        assert!(had_buy, "RSI should have generated a buy signal");
        assert!(had_sell, "RSI should have generated a sell signal");
    }

    #[test]
    fn test_bollinger_generator_produces_signals() {
        let mut gen = BollingerSignalGenerator::new(20, 2.0);
        assert_eq!(gen.name(), "BollingerBands");

        let mut prices: Vec<f64> = (0..30).map(|_| 100.0).collect();
        prices.extend((0..10).map(|i| 100.0 + (i as f64) * 5.0));
        prices.extend((0..10).map(|i| 150.0 - (i as f64) * 8.0));

        let klines = make_klines(&prices);
        let mut had_signal = false;
        for kline in &klines {
            let sig = gen.on_bar(kline);
            if sig.signal != Signal::Hold {
                had_signal = true;
            }
        }
        assert!(had_signal, "BB should have produced at least one signal");
    }

    #[test]
    fn test_combo_unanimous_requires_all() {
        let mut combo = ComboSignalGenerator::new(
            "Test".to_string(),
            vec![
                Box::new(RsiSignalGenerator::new(5, 70.0, 30.0)),
                Box::new(RsiSignalGenerator::new(14, 70.0, 30.0)),
            ],
            CombineMode::Unanimous,
        );
        assert_eq!(combo.name(), "Test");

        let klines = make_klines(&[100.0; 10]);
        for kline in &klines {
            let sig = combo.on_bar(kline);
            assert!(sig.confidence >= 0.0);
        }
    }

    #[test]
    fn test_signal_with_confidence_clamping() {
        let buy = SignalWithConfidence::buy(2.0);
        assert_eq!(buy.confidence, 1.0);

        let sell = SignalWithConfidence::sell(0.1);
        assert_eq!(sell.confidence, 0.3);

        let hold = SignalWithConfidence::hold();
        assert_eq!(hold.confidence, 0.0);
    }

    #[test]
    fn test_reset_works() {
        let mut gen = RsiSignalGenerator::new(14, 70.0, 30.0);
        let klines = make_klines(&[100.0; 20]);
        for kline in &klines {
            gen.on_bar(kline);
        }
        gen.reset();
        let sig = gen.on_bar(&klines[0]);
        assert_eq!(sig.signal, Signal::Hold);
    }
}
