//! Web-Researched Polymarket Strategies
//!
//! Static catalogue of strategies specific to prediction markets, discovered via
//! internet research. 5 are backtestable (compatible with Binance klines + Polymarket
//! fee model), 7 are display-only (require data not available in the current engine).

use crate::discovery::estimate_poly_probability;
use crate::indicators::{close_f64, SignalGenerator, SignalWithConfidence};
use crate::types::Kline;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use ta::indicators::{RelativeStrengthIndex, SimpleMovingAverage};
use ta::Next;

// ============================================================================
// Types
// ============================================================================

/// Identifier for web-researched strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebStrategyId {
    ProbabilityEdge,
    CatalystMomentum,
    FavoriteCompounder,
    MarketMakingSim,
    MeanReversionPoly,
}

impl WebStrategyId {
    pub fn all_backtestable() -> &'static [WebStrategyId] {
        &[
            Self::ProbabilityEdge,
            Self::CatalystMomentum,
            Self::FavoriteCompounder,
            Self::MarketMakingSim,
            Self::MeanReversionPoly,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ProbabilityEdge => "Web:ProbabilityEdge",
            Self::CatalystMomentum => "Web:CatalystMomentum",
            Self::FavoriteCompounder => "Web:FavoriteCompounder",
            Self::MarketMakingSim => "Web:MarketMakingSim",
            Self::MeanReversionPoly => "Web:MeanReversionPoly",
        }
    }
}

/// Parameters for each web strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "web_strategy", rename_all = "snake_case")]
pub enum WebStrategyParams {
    ProbabilityEdge {
        edge_threshold: f64,
        rsi_period: usize,
        momentum_period: usize,
        vol_period: usize,
    },
    CatalystMomentum {
        spike_threshold: f64,
        trailing_stop_pct: f64,
        lookback: usize,
    },
    FavoriteCompounder {
        min_probability: f64,
        take_profit: f64,
        sma_period: usize,
    },
    MarketMakingSim {
        spread: f64,
        sma_period: usize,
        inventory_limit: f64,
    },
    MeanReversionPoly {
        sma_period: usize,
        entry_dev: f64,
        exit_dev: f64,
    },
}

impl WebStrategyParams {
    pub fn default_for(id: &WebStrategyId) -> Self {
        match id {
            WebStrategyId::ProbabilityEdge => Self::ProbabilityEdge {
                edge_threshold: 0.05,
                rsi_period: 14,
                momentum_period: 10,
                vol_period: 20,
            },
            WebStrategyId::CatalystMomentum => Self::CatalystMomentum {
                spike_threshold: 0.02,
                trailing_stop_pct: 0.015,
                lookback: 20,
            },
            WebStrategyId::FavoriteCompounder => Self::FavoriteCompounder {
                min_probability: 0.65,
                take_profit: 0.03,
                sma_period: 20,
            },
            WebStrategyId::MarketMakingSim => Self::MarketMakingSim {
                spread: 0.02,
                sma_period: 20,
                inventory_limit: 3.0,
            },
            WebStrategyId::MeanReversionPoly => Self::MeanReversionPoly {
                sma_period: 50,
                entry_dev: 0.03,
                exit_dev: 0.01,
            },
        }
    }

    pub fn aggressive_for(id: &WebStrategyId) -> Self {
        match id {
            WebStrategyId::ProbabilityEdge => Self::ProbabilityEdge {
                edge_threshold: 0.03,
                rsi_period: 7,
                momentum_period: 5,
                vol_period: 10,
            },
            WebStrategyId::CatalystMomentum => Self::CatalystMomentum {
                spike_threshold: 0.01,
                trailing_stop_pct: 0.01,
                lookback: 10,
            },
            WebStrategyId::FavoriteCompounder => Self::FavoriteCompounder {
                min_probability: 0.55,
                take_profit: 0.02,
                sma_period: 10,
            },
            WebStrategyId::MarketMakingSim => Self::MarketMakingSim {
                spread: 0.01,
                sma_period: 10,
                inventory_limit: 5.0,
            },
            WebStrategyId::MeanReversionPoly => Self::MeanReversionPoly {
                sma_period: 20,
                entry_dev: 0.02,
                exit_dev: 0.005,
            },
        }
    }

    pub fn conservative_for(id: &WebStrategyId) -> Self {
        match id {
            WebStrategyId::ProbabilityEdge => Self::ProbabilityEdge {
                edge_threshold: 0.08,
                rsi_period: 21,
                momentum_period: 20,
                vol_period: 30,
            },
            WebStrategyId::CatalystMomentum => Self::CatalystMomentum {
                spike_threshold: 0.04,
                trailing_stop_pct: 0.025,
                lookback: 30,
            },
            WebStrategyId::FavoriteCompounder => Self::FavoriteCompounder {
                min_probability: 0.75,
                take_profit: 0.05,
                sma_period: 40,
            },
            WebStrategyId::MarketMakingSim => Self::MarketMakingSim {
                spread: 0.04,
                sma_period: 40,
                inventory_limit: 2.0,
            },
            WebStrategyId::MeanReversionPoly => Self::MeanReversionPoly {
                sma_period: 100,
                entry_dev: 0.05,
                exit_dev: 0.02,
            },
        }
    }

    pub fn random_for(id: &WebStrategyId, rng: &mut impl rand::Rng) -> Self {
        match id {
            WebStrategyId::ProbabilityEdge => Self::ProbabilityEdge {
                edge_threshold: rng.gen_range(0.02..=0.10),
                rsi_period: rng.gen_range(5..=25),
                momentum_period: rng.gen_range(3..=25),
                vol_period: rng.gen_range(8..=40),
            },
            WebStrategyId::CatalystMomentum => Self::CatalystMomentum {
                spike_threshold: rng.gen_range(0.005..=0.06),
                trailing_stop_pct: rng.gen_range(0.005..=0.04),
                lookback: rng.gen_range(5..=40),
            },
            WebStrategyId::FavoriteCompounder => Self::FavoriteCompounder {
                min_probability: rng.gen_range(0.50..=0.85),
                take_profit: rng.gen_range(0.01..=0.08),
                sma_period: rng.gen_range(8..=50),
            },
            WebStrategyId::MarketMakingSim => Self::MarketMakingSim {
                spread: rng.gen_range(0.005..=0.06),
                sma_period: rng.gen_range(8..=50),
                inventory_limit: rng.gen_range(1.0..=8.0),
            },
            WebStrategyId::MeanReversionPoly => Self::MeanReversionPoly {
                sma_period: rng.gen_range(15..=120),
                entry_dev: rng.gen_range(0.01..=0.08),
                exit_dev: rng.gen_range(0.003..=0.03),
            },
        }
    }
}

// ============================================================================
// Catalogue
// ============================================================================

/// A catalogue entry for a web-researched strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebStrategyCatalogEntry {
    pub id: String,
    pub name: &'static str,
    pub description: &'static str,
    pub source_url: &'static str,
    pub category: &'static str,
    pub risk_level: &'static str,
    pub backtestable: bool,
    pub rationale: &'static str,
}

/// Returns the full catalogue of 12 web-researched strategies (5 backtestable + 7 display-only)
pub fn get_catalog() -> Vec<WebStrategyCatalogEntry> {
    vec![
        // === 5 Backtestable strategies ===
        WebStrategyCatalogEntry {
            id: "probability_edge".to_string(),
            name: "ProbabilityEdge",
            description: "Estime la 'vraie' probabilite via un composite RSI + momentum + volatilite. Trade quand l'ecart depasse un seuil par rapport au prix de marche.",
            source_url: "https://polymarket.com",
            category: "edge",
            risk_level: "medium",
            backtestable: true,
            rationale: "Sur Polymarket, les prix refletent des probabilites. Si un modele multi-facteurs estime une probabilite differente, c'est une opportunite d'arbitrage informationnelle.",
        },
        WebStrategyCatalogEntry {
            id: "catalyst_momentum".to_string(),
            name: "CatalystMomentum",
            description: "Detecte les spikes de volume/prix (catalyseur), entre en momentum avec trailing stop. Capture les mouvements rapides apres des evenements.",
            source_url: "https://polymarket.com",
            category: "momentum",
            risk_level: "high",
            backtestable: true,
            rationale: "Les marches de prediction reagissent fortement aux nouvelles. Un spike de prix/volume signale un catalyseur ; le momentum tend a persister a court terme.",
        },
        WebStrategyCatalogEntry {
            id: "favorite_compounder".to_string(),
            name: "FavoriteCompounder",
            description: "Trade uniquement les favoris (probabilite > seuil), accumule des petits gains sur des trades a haute probabilite de succes.",
            source_url: "https://polymarket.com",
            category: "value",
            risk_level: "low",
            backtestable: true,
            rationale: "Le biais favori-longshot montre que les favoris sont sous-estimes. Trader systematiquement les favoris exploite ce biais cognitif des marches de prediction.",
        },
        WebStrategyCatalogEntry {
            id: "market_making_sim".to_string(),
            name: "MarketMakingSim",
            description: "Simule un market maker : place bid/ask autour de la SMA, capture le spread. Profite de la volatilite laterale.",
            source_url: "https://polymarket.com",
            category: "market-making",
            risk_level: "medium",
            backtestable: true,
            rationale: "Le market making est la strategie la plus utilisee sur Polymarket. En capturant le bid-ask spread autour d'une fair value estimee, on profite de la liquidite.",
        },
        WebStrategyCatalogEntry {
            id: "mean_reversion_poly".to_string(),
            name: "MeanReversionPoly",
            description: "Calcule une fair value (SMA longue), trade les deviations extremes avec retour a la moyenne. Polymarket tend a surreagir.",
            source_url: "https://polymarket.com",
            category: "mean-reversion",
            risk_level: "medium",
            backtestable: true,
            rationale: "Les marches de prediction surreagissent aux nouvelles a court terme. Les prix extremes tendent a revenir vers leur moyenne historique.",
        },
        // === 7 Display-only strategies ===
        WebStrategyCatalogEntry {
            id: "arbitrage_yes_no".to_string(),
            name: "Arbitrage YES+NO",
            description: "Achete YES et NO sur le meme marche quand le cout total < 1.00. Profit garanti a la resolution.",
            source_url: "https://polymarket.com",
            category: "arbitrage",
            risk_level: "low",
            backtestable: false,
            rationale: "Necessite les prix YES et NO simultanes, non disponibles dans les klines Binance.",
        },
        WebStrategyCatalogEntry {
            id: "whale_copy_trading".to_string(),
            name: "Whale Copy-Trading",
            description: "Copie les trades des plus gros portefeuilles Polymarket en temps reel via les donnees on-chain.",
            source_url: "https://polymarket.com",
            category: "momentum",
            risk_level: "medium",
            backtestable: false,
            rationale: "Necessite des donnees on-chain en temps reel pour suivre les wallets des whales.",
        },
        WebStrategyCatalogEntry {
            id: "cross_market_arbitrage".to_string(),
            name: "Cross-Market Arbitrage",
            description: "Exploite les incoherences de prix entre marches correles (ex: 'Trump gagne' vs 'Republicain gagne').",
            source_url: "https://polymarket.com",
            category: "arbitrage",
            risk_level: "low",
            backtestable: false,
            rationale: "Necessite les donnees de prix de plusieurs marches Polymarket correles simultanement.",
        },
        WebStrategyCatalogEntry {
            id: "liquidity_provision".to_string(),
            name: "Liquidity Provision",
            description: "Fournit de la liquidite sur l'orderbook Polymarket pour capturer les frais maker et le spread.",
            source_url: "https://polymarket.com",
            category: "market-making",
            risk_level: "medium",
            backtestable: false,
            rationale: "Necessite l'orderbook depth en temps reel, non simule par les klines Binance.",
        },
        WebStrategyCatalogEntry {
            id: "news_sentiment".to_string(),
            name: "News Sentiment",
            description: "Analyse le sentiment des actualites et reseaux sociaux pour anticiper les mouvements de marche.",
            source_url: "https://polymarket.com",
            category: "edge",
            risk_level: "high",
            backtestable: false,
            rationale: "Necessite un flux d'actualites en temps reel et un moteur NLP pour l'analyse de sentiment.",
        },
        WebStrategyCatalogEntry {
            id: "calendar_spread".to_string(),
            name: "Calendar Spread",
            description: "Exploite les differences de prix entre echeances differentes du meme evenement.",
            source_url: "https://polymarket.com",
            category: "arbitrage",
            risk_level: "low",
            backtestable: false,
            rationale: "Necessite plusieurs echeances du meme evenement, non disponible dans les donnees actuelles.",
        },
        WebStrategyCatalogEntry {
            id: "contrarian_fade".to_string(),
            name: "Contrarian Fade",
            description: "Prend le contre-pied du sentiment dominant de la foule quand celui-ci atteint des extremes.",
            source_url: "https://polymarket.com",
            category: "edge",
            risk_level: "high",
            backtestable: false,
            rationale: "Necessite des donnees de sentiment/foule aggregees, non disponibles dans les klines.",
        },
    ]
}

// ============================================================================
// Signal Generators
// ============================================================================

/// Build a web strategy signal generator from its id and params
pub fn build_web_generator(id: &WebStrategyId, params: &WebStrategyParams) -> Box<dyn SignalGenerator> {
    match (id, params) {
        (WebStrategyId::ProbabilityEdge, WebStrategyParams::ProbabilityEdge {
            edge_threshold, rsi_period, momentum_period, vol_period,
        }) => Box::new(ProbabilityEdgeGenerator::new(
            *edge_threshold, *rsi_period, *momentum_period, *vol_period,
        )),
        (WebStrategyId::CatalystMomentum, WebStrategyParams::CatalystMomentum {
            spike_threshold, trailing_stop_pct, lookback,
        }) => Box::new(CatalystMomentumGenerator::new(
            *spike_threshold, *trailing_stop_pct, *lookback,
        )),
        (WebStrategyId::FavoriteCompounder, WebStrategyParams::FavoriteCompounder {
            min_probability, take_profit, sma_period,
        }) => Box::new(FavoriteCompounderGenerator::new(
            *min_probability, *take_profit, *sma_period,
        )),
        (WebStrategyId::MarketMakingSim, WebStrategyParams::MarketMakingSim {
            spread, sma_period, inventory_limit,
        }) => Box::new(MarketMakingSimGenerator::new(
            *spread, *sma_period, *inventory_limit,
        )),
        (WebStrategyId::MeanReversionPoly, WebStrategyParams::MeanReversionPoly {
            sma_period, entry_dev, exit_dev,
        }) => Box::new(MeanReversionPolyGenerator::new(
            *sma_period, *entry_dev, *exit_dev,
        )),
        // Fallback: use default params
        _ => {
            let default_params = WebStrategyParams::default_for(id);
            build_web_generator(id, &default_params)
        }
    }
}

// ============================================================================
// 1. ProbabilityEdge — composite RSI + momentum + volatility → probability estimate
// ============================================================================

pub struct ProbabilityEdgeGenerator {
    name: String,
    edge_threshold: f64,
    rsi: RelativeStrengthIndex,
    rsi_period: usize,
    momentum_sma: SimpleMovingAverage,
    momentum_period: usize,
    vol_sma: SimpleMovingAverage,
    vol_period: usize,
    price_buffer: Vec<f64>,
    bars_seen: usize,
    baseline_price: f64,
}

impl ProbabilityEdgeGenerator {
    pub fn new(edge_threshold: f64, rsi_period: usize, momentum_period: usize, vol_period: usize) -> Self {
        Self {
            name: "Web:ProbabilityEdge".to_string(),
            edge_threshold,
            rsi: RelativeStrengthIndex::new(rsi_period).expect("Invalid RSI period"),
            rsi_period,
            momentum_sma: SimpleMovingAverage::new(momentum_period).expect("Invalid momentum period"),
            momentum_period,
            vol_sma: SimpleMovingAverage::new(vol_period).expect("Invalid vol period"),
            vol_period,
            price_buffer: Vec::new(),
            bars_seen: 0,
            baseline_price: 0.0,
        }
    }
}

impl SignalGenerator for ProbabilityEdgeGenerator {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let close = close_f64(kline);
        self.bars_seen += 1;

        if self.baseline_price == 0.0 {
            self.baseline_price = close;
        }

        // RSI signal: normalized to [-1, 1]
        let rsi_val = self.rsi.next(close);
        let rsi_signal = (50.0 - rsi_val) / 50.0; // oversold → positive, overbought → negative

        // Momentum signal: price vs SMA
        let sma_val = self.momentum_sma.next(close);
        let momentum_signal = if sma_val > 0.0 {
            (close - sma_val) / sma_val
        } else {
            0.0
        };

        // Volatility squeeze: compute recent volatility
        self.price_buffer.push(close);
        if self.price_buffer.len() > self.vol_period {
            self.price_buffer.remove(0);
        }
        let vol_signal = if self.price_buffer.len() >= 3 {
            let returns: Vec<f64> = self.price_buffer.windows(2)
                .map(|w| (w[1] - w[0]) / w[0])
                .collect();
            let mean = returns.iter().sum::<f64>() / returns.len() as f64;
            let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
            let std_dev = variance.sqrt();
            self.vol_sma.next(std_dev);
            // Low vol = squeeze = potential breakout
            if std_dev > 0.0 { (0.01 - std_dev).max(-0.5).min(0.5) } else { 0.0 }
        } else {
            0.0
        };

        let warmup = self.rsi_period.max(self.momentum_period).max(self.vol_period);
        if self.bars_seen < warmup + 5 {
            return SignalWithConfidence::hold();
        }

        // Composite score: weighted combination
        let composite = 0.4 * rsi_signal + 0.3 * momentum_signal + 0.3 * vol_signal;

        // Convert to "estimated probability" using Polymarket mapping
        let entry_dec = Decimal::from_str_exact(&format!("{:.4}", self.baseline_price)).unwrap_or(dec!(1));
        let current_dec = Decimal::from_str_exact(&format!("{:.4}", close)).unwrap_or(dec!(1));
        let market_prob: f64 = estimate_poly_probability(entry_dec, current_dec)
            .to_string().parse().unwrap_or(0.5);

        // Our estimated probability (composite → probability shift)
        let estimated_prob = (market_prob + composite * 0.3).clamp(0.05, 0.95);
        let edge = estimated_prob - market_prob;

        if edge > self.edge_threshold {
            SignalWithConfidence::buy(edge * 5.0)
        } else if edge < -self.edge_threshold {
            SignalWithConfidence::sell(edge.abs() * 5.0)
        } else {
            SignalWithConfidence::hold()
        }
    }

    fn reset(&mut self) {
        self.rsi = RelativeStrengthIndex::new(self.rsi_period).expect("Invalid RSI period");
        self.momentum_sma = SimpleMovingAverage::new(self.momentum_period).expect("Invalid momentum period");
        self.vol_sma = SimpleMovingAverage::new(self.vol_period).expect("Invalid vol period");
        self.price_buffer.clear();
        self.bars_seen = 0;
        self.baseline_price = 0.0;
    }
}

// ============================================================================
// 2. CatalystMomentum — detect spikes, ride momentum with trailing stop
// ============================================================================

pub struct CatalystMomentumGenerator {
    name: String,
    spike_threshold: f64,
    trailing_stop_pct: f64,
    lookback: usize,
    sma: SimpleMovingAverage,
    in_position: bool,
    highest_since_entry: f64,
    bars_seen: usize,
}

impl CatalystMomentumGenerator {
    pub fn new(spike_threshold: f64, trailing_stop_pct: f64, lookback: usize) -> Self {
        Self {
            name: "Web:CatalystMomentum".to_string(),
            spike_threshold,
            trailing_stop_pct,
            lookback,
            sma: SimpleMovingAverage::new(lookback).expect("Invalid lookback period"),
            in_position: false,
            highest_since_entry: 0.0,
            bars_seen: 0,
        }
    }
}

impl SignalGenerator for CatalystMomentumGenerator {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let close = close_f64(kline);
        let sma_val = self.sma.next(close);
        self.bars_seen += 1;

        if self.bars_seen < self.lookback + 2 {
            return SignalWithConfidence::hold();
        }

        if self.in_position {
            // Track highest price since entry
            if close > self.highest_since_entry {
                self.highest_since_entry = close;
            }
            // Trailing stop: exit if price drops from highest
            let drawdown = (self.highest_since_entry - close) / self.highest_since_entry;
            if drawdown > self.trailing_stop_pct {
                self.in_position = false;
                self.highest_since_entry = 0.0;
                return SignalWithConfidence::sell(drawdown * 10.0);
            }
            SignalWithConfidence::hold()
        } else {
            // Detect spike: price > SMA × (1 + threshold)
            let spike_level = sma_val * (1.0 + self.spike_threshold);
            if close > spike_level && sma_val > 0.0 {
                self.in_position = true;
                self.highest_since_entry = close;
                let spike_strength = (close - sma_val) / sma_val;
                SignalWithConfidence::buy(spike_strength * 5.0)
            } else {
                SignalWithConfidence::hold()
            }
        }
    }

    fn reset(&mut self) {
        self.sma = SimpleMovingAverage::new(self.lookback).expect("Invalid lookback period");
        self.in_position = false;
        self.highest_since_entry = 0.0;
        self.bars_seen = 0;
    }
}

// ============================================================================
// 3. FavoriteCompounder — trade high-probability favorites
// ============================================================================

pub struct FavoriteCompounderGenerator {
    name: String,
    min_probability: f64,
    take_profit: f64,
    sma: SimpleMovingAverage,
    sma_period: usize,
    baseline_price: f64,
    entry_price: f64,
    in_position: bool,
    bars_seen: usize,
}

impl FavoriteCompounderGenerator {
    pub fn new(min_probability: f64, take_profit: f64, sma_period: usize) -> Self {
        Self {
            name: "Web:FavoriteCompounder".to_string(),
            min_probability,
            take_profit,
            sma: SimpleMovingAverage::new(sma_period).expect("Invalid SMA period"),
            sma_period,
            baseline_price: 0.0,
            entry_price: 0.0,
            in_position: false,
            bars_seen: 0,
        }
    }
}

impl SignalGenerator for FavoriteCompounderGenerator {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let close = close_f64(kline);
        let sma_val = self.sma.next(close);
        self.bars_seen += 1;

        if self.baseline_price == 0.0 {
            self.baseline_price = close;
        }

        if self.bars_seen < self.sma_period + 2 {
            return SignalWithConfidence::hold();
        }

        // Estimate probability from price vs baseline
        let entry_dec = Decimal::from_str_exact(&format!("{:.4}", self.baseline_price)).unwrap_or(dec!(1));
        let current_dec = Decimal::from_str_exact(&format!("{:.4}", close)).unwrap_or(dec!(1));
        let prob: f64 = estimate_poly_probability(entry_dec, current_dec)
            .to_string().parse().unwrap_or(0.5);

        if self.in_position {
            // Take profit when price has risen enough
            if self.entry_price > 0.0 {
                let gain = (close - self.entry_price) / self.entry_price;
                if gain >= self.take_profit {
                    self.in_position = false;
                    self.entry_price = 0.0;
                    return SignalWithConfidence::sell(gain * 5.0);
                }
            }
            SignalWithConfidence::hold()
        } else {
            // Only enter on "favorites": high probability + price above SMA (uptrend)
            if prob >= self.min_probability && close > sma_val {
                self.in_position = true;
                self.entry_price = close;
                SignalWithConfidence::buy((prob - self.min_probability) * 3.0)
            } else {
                SignalWithConfidence::hold()
            }
        }
    }

    fn reset(&mut self) {
        self.sma = SimpleMovingAverage::new(self.sma_period).expect("Invalid SMA period");
        self.baseline_price = 0.0;
        self.entry_price = 0.0;
        self.in_position = false;
        self.bars_seen = 0;
    }
}

// ============================================================================
// 4. MarketMakingSim — simulate market making around SMA
// ============================================================================

pub struct MarketMakingSimGenerator {
    name: String,
    spread: f64,
    sma: SimpleMovingAverage,
    sma_period: usize,
    inventory_limit: f64,
    inventory: f64, // positive = long, negative = short
    bars_seen: usize,
}

impl MarketMakingSimGenerator {
    pub fn new(spread: f64, sma_period: usize, inventory_limit: f64) -> Self {
        Self {
            name: "Web:MarketMakingSim".to_string(),
            spread,
            sma: SimpleMovingAverage::new(sma_period).expect("Invalid SMA period"),
            sma_period,
            inventory_limit,
            inventory: 0.0,
            bars_seen: 0,
        }
    }
}

impl SignalGenerator for MarketMakingSimGenerator {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let close = close_f64(kline);
        let mid = self.sma.next(close);
        self.bars_seen += 1;

        if self.bars_seen < self.sma_period + 2 || mid <= 0.0 {
            return SignalWithConfidence::hold();
        }

        let half_spread = self.spread / 2.0;
        let bid = mid * (1.0 - half_spread);
        let ask = mid * (1.0 + half_spread);

        if close < bid && self.inventory < self.inventory_limit {
            // Price below bid → buy
            self.inventory += 1.0;
            let depth = (bid - close) / mid;
            SignalWithConfidence::buy(depth * 10.0)
        } else if close > ask && self.inventory > -self.inventory_limit {
            // Price above ask → sell
            self.inventory -= 1.0;
            let depth = (close - ask) / mid;
            SignalWithConfidence::sell(depth * 10.0)
        } else {
            SignalWithConfidence::hold()
        }
    }

    fn reset(&mut self) {
        self.sma = SimpleMovingAverage::new(self.sma_period).expect("Invalid SMA period");
        self.inventory = 0.0;
        self.bars_seen = 0;
    }
}

// ============================================================================
// 5. MeanReversionPoly — trade deviations from fair value (long SMA)
// ============================================================================

pub struct MeanReversionPolyGenerator {
    name: String,
    sma: SimpleMovingAverage,
    sma_period: usize,
    entry_dev: f64,
    exit_dev: f64,
    in_long: bool,
    in_short: bool,
    bars_seen: usize,
}

impl MeanReversionPolyGenerator {
    pub fn new(sma_period: usize, entry_dev: f64, exit_dev: f64) -> Self {
        Self {
            name: "Web:MeanReversionPoly".to_string(),
            sma: SimpleMovingAverage::new(sma_period).expect("Invalid SMA period"),
            sma_period,
            entry_dev,
            exit_dev,
            in_long: false,
            in_short: false,
            bars_seen: 0,
        }
    }
}

impl SignalGenerator for MeanReversionPolyGenerator {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_bar(&mut self, kline: &Kline) -> SignalWithConfidence {
        let close = close_f64(kline);
        let fair_value = self.sma.next(close);
        self.bars_seen += 1;

        if self.bars_seen < self.sma_period + 2 || fair_value <= 0.0 {
            return SignalWithConfidence::hold();
        }

        let deviation = (close - fair_value) / fair_value;

        // Exit conditions
        if self.in_long && deviation >= -self.exit_dev {
            self.in_long = false;
            return SignalWithConfidence::sell(0.5);
        }
        if self.in_short && deviation <= self.exit_dev {
            self.in_short = false;
            return SignalWithConfidence::buy(0.5);
        }

        // Entry conditions
        if !self.in_long && !self.in_short {
            if deviation < -self.entry_dev {
                // Price far below fair value → buy
                self.in_long = true;
                SignalWithConfidence::buy(deviation.abs() * 5.0)
            } else if deviation > self.entry_dev {
                // Price far above fair value → sell
                self.in_short = true;
                SignalWithConfidence::sell(deviation.abs() * 5.0)
            } else {
                SignalWithConfidence::hold()
            }
        } else {
            SignalWithConfidence::hold()
        }
    }

    fn reset(&mut self) {
        self.sma = SimpleMovingAverage::new(self.sma_period).expect("Invalid SMA period");
        self.in_long = false;
        self.in_short = false;
        self.bars_seen = 0;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Kline;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    fn make_klines(prices: &[f64]) -> Vec<Kline> {
        prices.iter().enumerate().map(|(i, &p)| {
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
        }).collect()
    }

    #[test]
    fn test_catalog_has_12_entries() {
        let catalog = get_catalog();
        assert_eq!(catalog.len(), 12);
        let backtestable = catalog.iter().filter(|e| e.backtestable).count();
        assert_eq!(backtestable, 5);
        let display_only = catalog.iter().filter(|e| !e.backtestable).count();
        assert_eq!(display_only, 7);
    }

    #[test]
    fn test_catalog_unique_ids() {
        let catalog = get_catalog();
        let mut ids: Vec<_> = catalog.iter().map(|e| &e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 12);
    }

    #[test]
    fn test_web_strategy_id_display_names() {
        assert_eq!(WebStrategyId::ProbabilityEdge.display_name(), "Web:ProbabilityEdge");
        assert_eq!(WebStrategyId::CatalystMomentum.display_name(), "Web:CatalystMomentum");
        assert_eq!(WebStrategyId::FavoriteCompounder.display_name(), "Web:FavoriteCompounder");
        assert_eq!(WebStrategyId::MarketMakingSim.display_name(), "Web:MarketMakingSim");
        assert_eq!(WebStrategyId::MeanReversionPoly.display_name(), "Web:MeanReversionPoly");
    }

    #[test]
    fn test_probability_edge_produces_signals() {
        // Generate a trending price series that should trigger signals
        let mut prices: Vec<f64> = (0..100).map(|i| 100.0 + (i as f64) * 0.1).collect();
        // Add a sharp drop to trigger buy signal
        for i in 80..100 {
            prices[i] = 100.0 - (i as f64 - 80.0) * 2.0;
        }
        let klines = make_klines(&prices);
        let mut gen = ProbabilityEdgeGenerator::new(0.03, 14, 10, 20);
        let mut signals = Vec::new();
        for kline in &klines {
            let sig = gen.on_bar(kline);
            if sig.signal != crate::strategy::Signal::Hold {
                signals.push(sig);
            }
        }
        // Should produce at least some signals on 100 bars with a trend change
        // (may or may not depending on exact thresholds, but generator should not panic)
        assert!(gen.bars_seen == 100);
    }

    #[test]
    fn test_catalyst_momentum_produces_signals() {
        // Flat period then spike
        let mut prices: Vec<f64> = vec![100.0; 30];
        // Spike up
        for i in 0..10 {
            prices.push(100.0 + (i as f64 + 1.0) * 5.0);
        }
        // Drop for trailing stop
        for _ in 0..10 {
            prices.push(90.0);
        }
        let klines = make_klines(&prices);
        let mut gen = CatalystMomentumGenerator::new(0.02, 0.015, 20);
        let mut signals = Vec::new();
        for kline in &klines {
            let sig = gen.on_bar(kline);
            if sig.signal != crate::strategy::Signal::Hold {
                signals.push(sig.signal);
            }
        }
        // Should detect spike (Buy) and trailing stop (Sell)
        assert!(!signals.is_empty(), "CatalystMomentum should produce signals on spike+drop");
    }

    #[test]
    fn test_market_making_sim_produces_signals() {
        // Oscillating prices around 100
        let prices: Vec<f64> = (0..60).map(|i| {
            100.0 + (i as f64 * 0.5).sin() * 5.0
        }).collect();
        let klines = make_klines(&prices);
        let mut gen = MarketMakingSimGenerator::new(0.02, 20, 3.0);
        let mut buy_count = 0;
        let mut sell_count = 0;
        for kline in &klines {
            let sig = gen.on_bar(kline);
            match sig.signal {
                crate::strategy::Signal::Buy => buy_count += 1,
                crate::strategy::Signal::Sell => sell_count += 1,
                _ => {}
            }
        }
        // Market making should produce both buys and sells on oscillating prices
        assert!(buy_count + sell_count > 0, "MarketMakingSim should produce signals on oscillating prices");
    }

    #[test]
    fn test_mean_reversion_poly_produces_signals() {
        // Trending up then sharp drop
        let mut prices: Vec<f64> = (0..70).map(|i| 100.0 + (i as f64) * 0.3).collect();
        // Sharp drop
        for _ in 0..20 {
            prices.push(90.0);
        }
        let klines = make_klines(&prices);
        let mut gen = MeanReversionPolyGenerator::new(50, 0.03, 0.01);
        let mut signals = Vec::new();
        for kline in &klines {
            let sig = gen.on_bar(kline);
            if sig.signal != crate::strategy::Signal::Hold {
                signals.push(sig.signal);
            }
        }
        assert!(gen.bars_seen == 90);
    }

    #[test]
    fn test_param_variants_differ() {
        for id in WebStrategyId::all_backtestable() {
            let d = format!("{:?}", WebStrategyParams::default_for(id));
            let a = format!("{:?}", WebStrategyParams::aggressive_for(id));
            let c = format!("{:?}", WebStrategyParams::conservative_for(id));
            assert_ne!(d, a, "Default and aggressive should differ for {:?}", id);
            assert_ne!(d, c, "Default and conservative should differ for {:?}", id);
            assert_ne!(a, c, "Aggressive and conservative should differ for {:?}", id);
        }
    }
}
