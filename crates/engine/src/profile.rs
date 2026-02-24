//! Profile Analyzer — deep analysis of a Polymarket user's trading activity
//!
//! Given a username, resolves to proxyWallet, fetches all trades/positions/activity,
//! groups by market, infers per-market strategy, and builds a full profile report.

use crate::api::polymarket::{
    ClosedPosition, GammaMarket, PolymarketDataClient, TraderPosition, TraderTrade,
};
use persistence::repository::profile::{ProfileAnalysisRecord, ProfileTradeRecord};
use persistence::SqlitePool;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::RwLock;
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Per-market inferred strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum MarketStrategy {
    Scalping,
    Momentum,
    Contrarian,
    MarketMaking,
    EventDriven,
    HoldToResolution,
    SwingTrading,
    Accumulation,
    DCA,
}

impl MarketStrategy {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Scalping => "Scalping",
            Self::Momentum => "Momentum",
            Self::Contrarian => "Contrarian",
            Self::MarketMaking => "Market Making",
            Self::EventDriven => "Event-Driven",
            Self::HoldToResolution => "Hold to Resolution",
            Self::SwingTrading => "Swing Trading",
            Self::Accumulation => "Accumulation",
            Self::DCA => "DCA",
        }
    }

    pub fn color(&self) -> &'static str {
        match self {
            Self::Scalping => "yellow",
            Self::Momentum => "blue",
            Self::Contrarian => "purple",
            Self::MarketMaking => "teal",
            Self::EventDriven => "pink",
            Self::HoldToResolution => "green",
            Self::SwingTrading => "orange",
            Self::Accumulation => "indigo",
            Self::DCA => "cyan",
        }
    }
}

/// Analysis of a single market (trades grouped by condition_id)
#[derive(Debug, Clone, Serialize)]
pub struct MarketAnalysis {
    pub condition_id: String,
    pub title: String,
    pub event_slug: String,
    pub category: String,
    pub outcome: String,

    // Trades on this market
    pub trade_count: usize,
    pub buy_count: usize,
    pub sell_count: usize,
    pub first_trade_ts: f64,
    pub last_trade_ts: f64,

    // Computed metrics
    pub total_bought: f64,
    pub total_sold: f64,
    pub net_position: f64,
    pub avg_buy_price: f64,
    pub avg_sell_price: f64,
    pub realized_pnl: f64,
    pub volume: f64,

    // Inferred strategy for THIS market
    pub inferred_strategy: MarketStrategy,
    pub strategy_confidence: f64,

    // Raw trades (for frontend expansion)
    pub trades: Vec<MarketTradeSummary>,
}

/// Compact trade for frontend display
#[derive(Debug, Clone, Serialize)]
pub struct MarketTradeSummary {
    pub side: String,
    pub size: f64,
    pub price: f64,
    pub timestamp: f64,
    pub transaction_hash: String,
}

/// Category-level statistics
#[derive(Debug, Clone, Serialize)]
pub struct CategoryStats {
    pub category: String,
    pub trade_count: usize,
    pub volume: f64,
    pub pnl: f64,
    pub win_rate: f64,
    pub market_count: usize,
}

/// Daily activity period
#[derive(Debug, Clone, Serialize)]
pub struct ActivityPeriod {
    pub date: String, // YYYY-MM-DD
    pub trade_count: usize,
    pub volume: f64,
}

/// Strategy signal with confidence
#[derive(Debug, Clone, Serialize)]
pub struct ProfileStrategySignal {
    pub strategy: MarketStrategy,
    pub confidence: f64,
    pub evidence: String,
    pub market_count: usize,
}

/// Full profile analysis result
#[derive(Debug, Clone, Serialize)]
pub struct ProfileAnalysis {
    pub wallet: String,
    pub username: String,

    // Overview
    pub portfolio_value: f64,
    pub total_pnl: f64,
    pub total_volume: f64,
    pub total_trades: usize,
    pub unique_markets: usize,
    pub win_rate: f64,

    // Positions
    pub open_positions: Vec<TraderPosition>,
    pub closed_positions: Vec<ClosedPosition>,

    // Market analysis (grouped trades)
    pub markets: Vec<MarketAnalysis>,

    // Category breakdown
    pub category_breakdown: Vec<CategoryStats>,

    // Activity timeline
    pub activity_timeline: Vec<ActivityPeriod>,

    // Global strategy
    pub primary_strategy: MarketStrategy,
    pub strategy_confidence: f64,
    pub strategy_signals: Vec<ProfileStrategySignal>,

    // Advanced metrics
    pub avg_hold_duration_days: f64,
    pub best_trade_pnl: f64,
    pub worst_trade_pnl: f64,
    pub max_drawdown: f64,
    pub active_days: usize,
    pub avg_position_size: f64,
}

// ---------------------------------------------------------------------------
// Progress tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProfileStatus {
    Idle,
    ResolvingUsername,
    FetchingTrades,
    FetchingPositions,
    FetchingClosedPositions,
    FetchingActivity,
    FetchingMarketData,
    AnalyzingMarkets,
    Complete,
    Error,
}

pub struct ProfileProgress {
    pub status: RwLock<ProfileStatus>,
    pub total_steps: AtomicU32,
    pub completed_steps: AtomicU32,
    pub current_step: RwLock<String>,
    pub result: RwLock<Option<ProfileAnalysis>>,
    pub error_message: RwLock<Option<String>>,
    pub cancelled: AtomicBool,
    pub username: RwLock<String>,
    pub wallet: RwLock<String>,
}

impl ProfileProgress {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(ProfileStatus::Idle),
            total_steps: AtomicU32::new(8),
            completed_steps: AtomicU32::new(0),
            current_step: RwLock::new(String::new()),
            result: RwLock::new(None),
            error_message: RwLock::new(None),
            cancelled: AtomicBool::new(false),
            username: RwLock::new(String::new()),
            wallet: RwLock::new(String::new()),
        }
    }

    pub fn reset(&self, username: &str) {
        *self.status.write().unwrap() = ProfileStatus::ResolvingUsername;
        self.total_steps.store(8, Ordering::Relaxed);
        self.completed_steps.store(0, Ordering::Relaxed);
        *self.current_step.write().unwrap() = "Resolving username...".to_string();
        *self.result.write().unwrap() = None;
        *self.error_message.write().unwrap() = None;
        self.cancelled.store(false, Ordering::Relaxed);
        *self.username.write().unwrap() = username.to_string();
        *self.wallet.write().unwrap() = String::new();
    }

    pub fn is_running(&self) -> bool {
        let status = self.status.read().unwrap();
        !matches!(*status, ProfileStatus::Idle | ProfileStatus::Complete | ProfileStatus::Error)
    }

    fn set_step(&self, status: ProfileStatus, step_name: &str) {
        *self.status.write().unwrap() = status;
        *self.current_step.write().unwrap() = step_name.to_string();
    }

    fn advance(&self) {
        self.completed_steps.fetch_add(1, Ordering::Relaxed);
    }

    fn set_error(&self, msg: String) {
        *self.status.write().unwrap() = ProfileStatus::Error;
        *self.error_message.write().unwrap() = Some(msg);
    }
}

impl Default for ProfileProgress {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Trade grouping & per-market analysis
// ---------------------------------------------------------------------------

/// Group trades by condition_id and compute per-market metrics
fn analyze_markets(
    trades: &[TraderTrade],
    market_metadata: &HashMap<String, GammaMarket>,
) -> Vec<MarketAnalysis> {
    // Group trades by condition_id
    let mut by_market: HashMap<String, Vec<&TraderTrade>> = HashMap::new();
    for t in trades {
        if let Some(cid) = &t.condition_id {
            by_market.entry(cid.clone()).or_default().push(t);
        }
    }

    let mut markets: Vec<MarketAnalysis> = Vec::new();

    for (condition_id, market_trades) in &by_market {
        // Get metadata if available
        let meta = market_metadata.get(condition_id);
        let title = market_trades
            .first()
            .and_then(|t| t.title.clone())
            .or_else(|| meta.and_then(|m| m.question.clone()))
            .unwrap_or_else(|| condition_id[..8.min(condition_id.len())].to_string());
        let event_slug = market_trades
            .first()
            .and_then(|t| t.event_slug.clone())
            .or_else(|| meta.and_then(|m| m.event_slug.clone()))
            .unwrap_or_default();
        let category = meta
            .and_then(|m| m.category.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let outcome = market_trades
            .first()
            .and_then(|t| t.outcome.clone())
            .unwrap_or_default();

        // Compute buy/sell metrics
        let mut buy_count = 0usize;
        let mut sell_count = 0usize;
        let mut total_buy_size = 0.0f64;
        let mut total_sell_size = 0.0f64;
        let mut total_buy_cost = 0.0f64;
        let mut total_sell_cost = 0.0f64;
        let mut volume = 0.0f64;
        let mut min_ts = f64::MAX;
        let mut max_ts = f64::MIN;

        let mut trade_summaries = Vec::new();

        for t in market_trades {
            let size = t.size.unwrap_or(0.0);
            let price = t.price.unwrap_or(0.0);
            let ts = t.timestamp.unwrap_or(0.0);

            volume += size * price;

            if ts < min_ts {
                min_ts = ts;
            }
            if ts > max_ts {
                max_ts = ts;
            }

            match t.side.as_deref() {
                Some("BUY") => {
                    buy_count += 1;
                    total_buy_size += size;
                    total_buy_cost += size * price;
                }
                Some("SELL") => {
                    sell_count += 1;
                    total_sell_size += size;
                    total_sell_cost += size * price;
                }
                _ => {}
            }

            trade_summaries.push(MarketTradeSummary {
                side: t.side.clone().unwrap_or_default(),
                size,
                price,
                timestamp: ts,
                transaction_hash: t.transaction_hash.clone().unwrap_or_default(),
            });
        }

        // Sort trades by timestamp
        trade_summaries.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap_or(std::cmp::Ordering::Equal));

        let avg_buy_price = if total_buy_size > 0.0 {
            total_buy_cost / total_buy_size
        } else {
            0.0
        };
        let avg_sell_price = if total_sell_size > 0.0 {
            total_sell_cost / total_sell_size
        } else {
            0.0
        };

        // Estimated realized PnL
        let min_matched = total_buy_size.min(total_sell_size);
        let realized_pnl = if min_matched > 0.0 {
            min_matched * (avg_sell_price - avg_buy_price)
        } else {
            0.0
        };

        let net_position = total_buy_size - total_sell_size;
        let trade_count = market_trades.len();

        if min_ts == f64::MAX {
            min_ts = 0.0;
        }
        if max_ts == f64::MIN {
            max_ts = 0.0;
        }

        // Infer strategy for this market
        let (strategy, confidence) = infer_market_strategy(
            buy_count,
            sell_count,
            trade_count,
            avg_buy_price,
            total_buy_size,
            total_sell_size,
            min_ts,
            max_ts,
            &trade_summaries,
        );

        markets.push(MarketAnalysis {
            condition_id: condition_id.clone(),
            title,
            event_slug,
            category,
            outcome,
            trade_count,
            buy_count,
            sell_count,
            first_trade_ts: min_ts,
            last_trade_ts: max_ts,
            total_bought: total_buy_size,
            total_sold: total_sell_size,
            net_position,
            avg_buy_price,
            avg_sell_price,
            realized_pnl,
            volume,
            inferred_strategy: strategy,
            strategy_confidence: confidence,
            trades: trade_summaries,
        });
    }

    // Sort by volume descending
    markets.sort_by(|a, b| b.volume.partial_cmp(&a.volume).unwrap_or(std::cmp::Ordering::Equal));
    markets
}

/// Infer trading strategy for a single market based on trade patterns
fn infer_market_strategy(
    buy_count: usize,
    sell_count: usize,
    trade_count: usize,
    avg_buy_price: f64,
    _total_buy_size: f64,
    _total_sell_size: f64,
    first_ts: f64,
    last_ts: f64,
    trades: &[MarketTradeSummary],
) -> (MarketStrategy, f64) {
    let duration_hours = (last_ts - first_ts) / 3600.0;
    let duration_days = duration_hours / 24.0;
    let sell_ratio = if buy_count + sell_count > 0 {
        sell_count as f64 / (buy_count + sell_count) as f64
    } else {
        0.0
    };

    // Market Making: both buy and sell with significant sell ratio
    if buy_count > 0 && sell_count > 0 && sell_ratio > 0.3 && sell_ratio < 0.7 {
        return (MarketStrategy::MarketMaking, 0.75 + sell_ratio * 0.2);
    }

    // Scalping: many quick trades
    if trade_count > 10 && duration_hours > 0.0 && (trade_count as f64 / duration_hours.max(1.0)) > 2.0 {
        return (MarketStrategy::Scalping, 0.7 + (trade_count as f64 / 50.0).min(0.25));
    }

    // DCA: multiple buys of similar size, no sells
    if buy_count >= 3 && sell_count == 0 {
        let buy_sizes: Vec<f64> = trades
            .iter()
            .filter(|t| t.side == "BUY")
            .map(|t| t.size)
            .collect();
        if !buy_sizes.is_empty() {
            let mean = buy_sizes.iter().sum::<f64>() / buy_sizes.len() as f64;
            let variance = buy_sizes.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / buy_sizes.len() as f64;
            let cv = if mean > 0.0 { variance.sqrt() / mean } else { 1.0 };
            if cv < 0.3 {
                return (MarketStrategy::DCA, 0.7 + (1.0 - cv) * 0.25);
            }
        }
    }

    // Accumulation: multiple buys, no/few sells
    if buy_count > 1 && sell_count == 0 {
        return (MarketStrategy::Accumulation, 0.6 + (buy_count as f64 / 20.0).min(0.3));
    }

    // Hold to Resolution: 1-2 buys, no sells
    if sell_count == 0 && trade_count <= 2 {
        return (MarketStrategy::HoldToResolution, 0.8);
    }

    // Contrarian: buys at very low prices
    if avg_buy_price < 0.30 && avg_buy_price > 0.0 {
        return (MarketStrategy::Contrarian, 0.6 + (0.30 - avg_buy_price) * 2.0);
    }

    // Momentum: buys at high prices
    if avg_buy_price > 0.70 {
        return (MarketStrategy::Momentum, 0.6 + (avg_buy_price - 0.70) * 2.0);
    }

    // Event-Driven: trades clustered in time
    if trade_count >= 3 && duration_days > 1.0 {
        let timestamps: Vec<f64> = trades.iter().map(|t| t.timestamp).collect();
        let cluster_ratio = compute_time_cluster_ratio(&timestamps, duration_days);
        if cluster_ratio > 0.6 {
            return (MarketStrategy::EventDriven, 0.6 + cluster_ratio * 0.3);
        }
    }

    // Swing Trading: few trades over long period
    if trade_count <= 5 && duration_days > 7.0 {
        return (MarketStrategy::SwingTrading, 0.6);
    }

    // Default: based on buy/sell balance
    if sell_ratio > 0.3 {
        (MarketStrategy::MarketMaking, 0.4)
    } else if avg_buy_price < 0.40 {
        (MarketStrategy::Contrarian, 0.4)
    } else {
        (MarketStrategy::Momentum, 0.4)
    }
}

/// Measure time clustering (what fraction of trades in a 20% window)
fn compute_time_cluster_ratio(timestamps: &[f64], total_days: f64) -> f64 {
    if timestamps.len() < 3 || total_days < 1.0 {
        return 0.0;
    }

    let total_seconds = total_days * 86400.0;
    let window = total_seconds * 0.20;
    let n = timestamps.len();

    let mut sorted = timestamps.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut max_in_window = 0usize;
    let mut left = 0;
    for right in 0..n {
        while sorted[right] - sorted[left] > window {
            left += 1;
        }
        max_in_window = max_in_window.max(right - left + 1);
    }

    max_in_window as f64 / n as f64
}

// ---------------------------------------------------------------------------
// Category breakdown
// ---------------------------------------------------------------------------

fn compute_category_breakdown(markets: &[MarketAnalysis]) -> Vec<CategoryStats> {
    let mut by_category: HashMap<String, Vec<&MarketAnalysis>> = HashMap::new();
    for m in markets {
        by_category.entry(m.category.clone()).or_default().push(m);
    }

    let mut stats: Vec<CategoryStats> = by_category
        .into_iter()
        .map(|(category, cat_markets)| {
            let trade_count: usize = cat_markets.iter().map(|m| m.trade_count).sum();
            let volume: f64 = cat_markets.iter().map(|m| m.volume).sum();
            let pnl: f64 = cat_markets.iter().map(|m| m.realized_pnl).sum();
            let profitable = cat_markets.iter().filter(|m| m.realized_pnl > 0.0).count();
            let win_rate = if cat_markets.is_empty() {
                0.0
            } else {
                profitable as f64 / cat_markets.len() as f64 * 100.0
            };
            let market_count = cat_markets.len();

            CategoryStats {
                category,
                trade_count,
                volume,
                pnl,
                win_rate,
                market_count,
            }
        })
        .collect();

    stats.sort_by(|a, b| b.volume.partial_cmp(&a.volume).unwrap_or(std::cmp::Ordering::Equal));
    stats
}

// ---------------------------------------------------------------------------
// Activity timeline
// ---------------------------------------------------------------------------

fn compute_activity_timeline(trades: &[TraderTrade]) -> Vec<ActivityPeriod> {
    let mut by_date: HashMap<String, (usize, f64)> = HashMap::new();

    for t in trades {
        let ts = t.timestamp.unwrap_or(0.0) as i64;
        if ts == 0 {
            continue;
        }
        // Convert epoch seconds to YYYY-MM-DD
        let date = {
            let days_since_epoch = ts / 86400;
            // Simple date formatting from epoch
            let dt = chrono::DateTime::from_timestamp(ts, 0);
            match dt {
                Some(dt) => dt.format("%Y-%m-%d").to_string(),
                None => format!("day-{}", days_since_epoch),
            }
        };
        let size = t.size.unwrap_or(0.0);
        let price = t.price.unwrap_or(0.0);
        let entry = by_date.entry(date).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += size * price;
    }

    let mut timeline: Vec<ActivityPeriod> = by_date
        .into_iter()
        .map(|(date, (trade_count, volume))| ActivityPeriod {
            date,
            trade_count,
            volume,
        })
        .collect();

    timeline.sort_by(|a, b| a.date.cmp(&b.date));
    timeline
}

// ---------------------------------------------------------------------------
// Global strategy inference
// ---------------------------------------------------------------------------

fn infer_global_strategy(markets: &[MarketAnalysis]) -> Vec<ProfileStrategySignal> {
    if markets.is_empty() {
        return vec![ProfileStrategySignal {
            strategy: MarketStrategy::HoldToResolution,
            confidence: 0.1,
            evidence: "No market data available".to_string(),
            market_count: 0,
        }];
    }

    // Count markets per strategy
    let mut strategy_counts: HashMap<MarketStrategy, Vec<&MarketAnalysis>> = HashMap::new();
    for m in markets {
        strategy_counts
            .entry(m.inferred_strategy)
            .or_default()
            .push(m);
    }

    let total = markets.len() as f64;

    let mut signals: Vec<ProfileStrategySignal> = strategy_counts
        .into_iter()
        .map(|(strategy, strat_markets)| {
            let count = strat_markets.len();
            let pct = count as f64 / total;
            let avg_confidence: f64 = strat_markets.iter().map(|m| m.strategy_confidence).sum::<f64>()
                / count as f64;
            let total_volume: f64 = strat_markets.iter().map(|m| m.volume).sum();

            ProfileStrategySignal {
                strategy,
                confidence: pct * avg_confidence,
                evidence: format!(
                    "{} markets ({:.0}%), avg conf {:.0}%, vol ${:.0}",
                    count,
                    pct * 100.0,
                    avg_confidence * 100.0,
                    total_volume
                ),
                market_count: count,
            }
        })
        .collect();

    signals.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    signals
}

// ---------------------------------------------------------------------------
// Trade hash for deduplication
// ---------------------------------------------------------------------------

fn compute_trade_hash(wallet: &str, trade: &TraderTrade) -> String {
    let mut hasher = Sha256::new();
    hasher.update(wallet.as_bytes());
    hasher.update(trade.condition_id.as_deref().unwrap_or("").as_bytes());
    hasher.update(trade.side.as_deref().unwrap_or("").as_bytes());
    hasher.update(format!("{}", trade.size.unwrap_or(0.0)).as_bytes());
    hasher.update(format!("{}", trade.price.unwrap_or(0.0)).as_bytes());
    hasher.update(format!("{}", trade.timestamp.unwrap_or(0.0)).as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Convert API trades to DB records
fn trades_to_profile_records(wallet: &str, trades: &[TraderTrade]) -> Vec<ProfileTradeRecord> {
    trades
        .iter()
        .filter_map(|t| {
            let condition_id = t.condition_id.clone()?;
            let hash = compute_trade_hash(wallet, t);
            Some(ProfileTradeRecord {
                id: None,
                wallet: wallet.to_string(),
                trade_hash: hash,
                side: t.side.clone().unwrap_or_default(),
                condition_id,
                asset: t.asset.clone(),
                size: t.size.unwrap_or(0.0),
                price: t.price.unwrap_or(0.0),
                title: t.title.clone(),
                outcome: t.outcome.clone(),
                event_slug: t.event_slug.clone(),
                timestamp: t.timestamp.unwrap_or(0.0),
                transaction_hash: t.transaction_hash.clone(),
                created_at: None,
            })
        })
        .collect()
}

/// Convert ProfileAnalysis to a DB record for persistence
fn analysis_to_record(analysis: &ProfileAnalysis) -> ProfileAnalysisRecord {
    ProfileAnalysisRecord {
        id: None,
        wallet: analysis.wallet.clone(),
        username: Some(analysis.username.clone()),
        portfolio_value: Some(analysis.portfolio_value),
        total_pnl: Some(analysis.total_pnl),
        total_volume: Some(analysis.total_volume),
        total_trades: Some(analysis.total_trades as i64),
        unique_markets: Some(analysis.unique_markets as i64),
        win_rate: Some(analysis.win_rate),
        primary_strategy: Some(analysis.primary_strategy.label().to_string()),
        strategy_confidence: Some(analysis.strategy_confidence),
        open_positions_json: serde_json::to_string(&analysis.open_positions).ok(),
        closed_positions_json: serde_json::to_string(&analysis.closed_positions).ok(),
        markets_json: serde_json::to_string(&analysis.markets).ok(),
        category_breakdown_json: serde_json::to_string(&analysis.category_breakdown).ok(),
        activity_timeline_json: serde_json::to_string(&analysis.activity_timeline).ok(),
        strategy_signals_json: serde_json::to_string(&analysis.strategy_signals).ok(),
        avg_hold_duration: Some(analysis.avg_hold_duration_days),
        best_trade_pnl: Some(analysis.best_trade_pnl),
        worst_trade_pnl: Some(analysis.worst_trade_pnl),
        max_drawdown: Some(analysis.max_drawdown),
        active_days: Some(analysis.active_days as i64),
        avg_position_size: Some(analysis.avg_position_size),
        analyzed_at: None,
        created_at: None,
    }
}

// ---------------------------------------------------------------------------
// Main orchestrator
// ---------------------------------------------------------------------------

/// Analyze a Polymarket user profile by username
pub async fn analyze_profile(
    username: String,
    progress: &ProfileProgress,
    client: &PolymarketDataClient,
    db_pool: Option<SqlitePool>,
) {
    info!(username = %username, "Starting profile analysis");

    // Step 1: Resolve username → wallet
    progress.set_step(ProfileStatus::ResolvingUsername, "Resolving username...");
    let (wallet, resolved_name) = match client.resolve_username(&username).await {
        Ok((w, n)) => (w, n),
        Err(err) => {
            error!(username = %username, error = %err, "Failed to resolve username");
            progress.set_error(format!("User not found: {}", username));
            return;
        }
    };
    *progress.wallet.write().unwrap() = wallet.clone();
    progress.advance();
    info!(wallet = %wallet, name = %resolved_name, "Username resolved");

    // Step 2: Fetch portfolio value
    progress.set_step(ProfileStatus::FetchingTrades, "Fetching portfolio value...");
    let portfolio_value = match client.get_value(&wallet).await {
        Ok(v) => v.value.unwrap_or(0.0),
        Err(e) => {
            warn!(error = %e, "Failed to fetch portfolio value");
            0.0
        }
    };

    // Step 3: Fetch ALL trades (paginated)
    progress.set_step(ProfileStatus::FetchingTrades, "Fetching all trades...");
    let all_trades = match client.get_all_trades(&wallet).await {
        Ok(t) => {
            info!(count = t.len(), "Trades fetched");
            t
        }
        Err(e) => {
            error!(error = %e, "Failed to fetch trades");
            progress.set_error(format!("Failed to fetch trades: {}", e));
            return;
        }
    };
    progress.advance();

    if progress.cancelled.load(Ordering::Relaxed) {
        warn!("Profile analysis cancelled");
        progress.set_error("Cancelled".to_string());
        return;
    }

    // Step 4: Fetch open positions
    progress.set_step(ProfileStatus::FetchingPositions, "Fetching open positions...");
    let open_positions = match client.get_all_positions(&wallet).await {
        Ok(p) => {
            info!(count = p.len(), "Open positions fetched");
            p
        }
        Err(e) => {
            warn!(error = %e, "Failed to fetch open positions");
            Vec::new()
        }
    };
    progress.advance();

    // Step 5: Fetch closed positions
    progress.set_step(ProfileStatus::FetchingClosedPositions, "Fetching closed positions...");
    let closed_positions = match client.get_all_closed_positions(&wallet).await {
        Ok(p) => {
            info!(count = p.len(), "Closed positions fetched");
            p
        }
        Err(e) => {
            warn!(error = %e, "Failed to fetch closed positions");
            Vec::new()
        }
    };
    progress.advance();

    if progress.cancelled.load(Ordering::Relaxed) {
        warn!("Profile analysis cancelled");
        progress.set_error("Cancelled".to_string());
        return;
    }

    // Step 6: Fetch market metadata from Gamma API
    progress.set_step(ProfileStatus::FetchingMarketData, "Fetching market metadata...");
    let unique_condition_ids: Vec<String> = {
        let mut ids: Vec<String> = all_trades
            .iter()
            .filter_map(|t| t.condition_id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    };
    let market_metadata_list = match client.get_markets_by_condition_ids(&unique_condition_ids).await {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "Failed to fetch market metadata (non-fatal)");
            Vec::new()
        }
    };
    let market_metadata: HashMap<String, GammaMarket> = market_metadata_list
        .into_iter()
        .filter_map(|m| {
            let cid = m.condition_id.clone()?;
            Some((cid, m))
        })
        .collect();
    progress.advance();

    // Step 7: Analyze markets (group trades, infer strategies)
    progress.set_step(ProfileStatus::AnalyzingMarkets, "Analyzing markets...");

    let markets = analyze_markets(&all_trades, &market_metadata);
    let unique_markets = markets.len();

    // Compute category breakdown
    let category_breakdown = compute_category_breakdown(&markets);

    // Compute activity timeline
    let activity_timeline = compute_activity_timeline(&all_trades);

    // Compute total PnL from closed positions + open position PnL
    let closed_pnl: f64 = closed_positions
        .iter()
        .map(|p| p.realized_pnl.unwrap_or(0.0))
        .sum();
    let open_pnl: f64 = open_positions
        .iter()
        .map(|p| p.cash_pnl.unwrap_or(0.0))
        .sum();
    let total_pnl = closed_pnl + open_pnl;

    // Total volume
    let total_volume: f64 = markets.iter().map(|m| m.volume).sum();

    // Win rate (% of markets with positive realized PnL)
    let profitable_markets = markets.iter().filter(|m| m.realized_pnl > 0.0).count();
    let win_rate = if unique_markets > 0 {
        profitable_markets as f64 / unique_markets as f64 * 100.0
    } else {
        0.0
    };

    // Global strategy inference
    let strategy_signals = infer_global_strategy(&markets);
    let primary_strategy = strategy_signals
        .first()
        .map(|s| s.strategy)
        .unwrap_or(MarketStrategy::HoldToResolution);
    let strategy_confidence = strategy_signals
        .first()
        .map(|s| s.confidence)
        .unwrap_or(0.0);

    // Advanced metrics
    let active_days = activity_timeline.len();

    let avg_position_size = if !open_positions.is_empty() {
        open_positions
            .iter()
            .filter_map(|p| p.size)
            .sum::<f64>()
            / open_positions.len() as f64
    } else {
        0.0
    };

    // Best/worst trade PnL (estimated from individual trades)
    let mut best_trade_pnl = 0.0f64;
    let mut worst_trade_pnl = 0.0f64;
    for m in &markets {
        if m.realized_pnl > best_trade_pnl {
            best_trade_pnl = m.realized_pnl;
        }
        if m.realized_pnl < worst_trade_pnl {
            worst_trade_pnl = m.realized_pnl;
        }
    }

    // Average hold duration (from closed positions with timestamps)
    let avg_hold_duration_days = {
        let durations: Vec<f64> = closed_positions
            .iter()
            .filter_map(|p| {
                let ts = p.timestamp.unwrap_or(0.0);
                if ts > 0.0 {
                    Some(ts)
                } else {
                    None
                }
            })
            .collect();
        if durations.len() >= 2 {
            let min = durations.iter().cloned().fold(f64::MAX, f64::min);
            let max = durations.iter().cloned().fold(f64::MIN, f64::max);
            (max - min) / 86400.0 / durations.len() as f64
        } else {
            0.0
        }
    };

    // Max drawdown (cumulative PnL)
    let max_drawdown = compute_max_drawdown(&markets);

    progress.advance();

    // Build final result
    let analysis = ProfileAnalysis {
        wallet: wallet.clone(),
        username: resolved_name,
        portfolio_value,
        total_pnl,
        total_volume,
        total_trades: all_trades.len(),
        unique_markets,
        win_rate,
        open_positions,
        closed_positions,
        markets,
        category_breakdown,
        activity_timeline,
        primary_strategy,
        strategy_confidence,
        strategy_signals,
        avg_hold_duration_days,
        best_trade_pnl,
        worst_trade_pnl,
        max_drawdown,
        active_days,
        avg_position_size,
    };

    // Step 8: Persist to DB
    if let Some(ref pool) = db_pool {
        let repo = persistence::repository::profile::ProfileRepository::new(pool);

        // Save analysis
        let record = analysis_to_record(&analysis);
        if let Err(e) = repo.save_analysis(&record).await {
            warn!(error = %e, "Failed to save profile analysis to DB");
        }

        // Save trades
        let trade_records = trades_to_profile_records(&wallet, &all_trades);
        match repo.save_trades(&trade_records).await {
            Ok(n) => info!(new_trades = n, "Saved profile trades to DB"),
            Err(e) => warn!(error = %e, "Failed to save profile trades to DB"),
        }
    }
    progress.advance();

    // Done
    *progress.result.write().unwrap() = Some(analysis);
    *progress.status.write().unwrap() = ProfileStatus::Complete;
    info!(wallet = %wallet, "Profile analysis complete");
}

/// Compute max drawdown from market PnLs (ordered by first trade time)
fn compute_max_drawdown(markets: &[MarketAnalysis]) -> f64 {
    let mut sorted_markets: Vec<&MarketAnalysis> = markets.iter().collect();
    sorted_markets.sort_by(|a, b| {
        a.first_trade_ts
            .partial_cmp(&b.first_trade_ts)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut peak = 0.0f64;
    let mut cumulative = 0.0f64;
    let mut max_dd = 0.0f64;

    for m in &sorted_markets {
        cumulative += m.realized_pnl;
        if cumulative > peak {
            peak = cumulative;
        }
        let drawdown = peak - cumulative;
        if drawdown > max_dd {
            max_dd = drawdown;
        }
    }

    max_dd
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::polymarket::TraderTrade;

    fn make_trade(
        side: &str,
        price: f64,
        size: f64,
        condition_id: &str,
        event_slug: &str,
        ts: f64,
    ) -> TraderTrade {
        TraderTrade {
            proxy_wallet: Some("0xabc".into()),
            side: Some(side.into()),
            asset: Some("asset".into()),
            condition_id: Some(condition_id.into()),
            size: Some(size),
            price: Some(price),
            timestamp: Some(ts),
            title: Some("Test Market".into()),
            slug: Some("test-market".into()),
            event_slug: Some(event_slug.into()),
            outcome: Some("Yes".into()),
            outcome_index: Some(0.0),
            transaction_hash: Some("0x123".into()),
        }
    }

    #[test]
    fn test_market_strategy_hold_to_resolution() {
        let trades = vec![
            make_trade("BUY", 0.50, 100.0, "cid1", "evt1", 1700000000.0),
        ];
        let markets = analyze_markets(&trades, &HashMap::new());
        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0].inferred_strategy, MarketStrategy::HoldToResolution);
    }

    #[test]
    fn test_market_strategy_accumulation() {
        let trades = vec![
            make_trade("BUY", 0.40, 50.0, "cid1", "evt1", 1700000000.0),
            make_trade("BUY", 0.45, 50.0, "cid1", "evt1", 1700003600.0),
        ];
        let markets = analyze_markets(&trades, &HashMap::new());
        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0].inferred_strategy, MarketStrategy::Accumulation);
    }

    #[test]
    fn test_market_strategy_market_making() {
        let mut trades = Vec::new();
        for i in 0..10 {
            trades.push(make_trade("BUY", 0.45, 100.0, "cid1", "evt1", (1700000000 + i * 60) as f64));
            trades.push(make_trade("SELL", 0.55, 100.0, "cid1", "evt1", (1700000000 + i * 60 + 30) as f64));
        }
        let markets = analyze_markets(&trades, &HashMap::new());
        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0].inferred_strategy, MarketStrategy::MarketMaking);
    }

    #[test]
    fn test_market_strategy_contrarian() {
        // 4 buys + 1 sell at low prices, sell_ratio ~0.2 (below MM threshold 0.3)
        // Enough trades to avoid HoldToResolution, has sells to avoid Accumulation/DCA
        let trades = vec![
            make_trade("BUY", 0.10, 100.0, "cid1", "evt1", 1700000000.0),
            make_trade("BUY", 0.12, 80.0, "cid1", "evt1", 1700086400.0),
            make_trade("BUY", 0.15, 120.0, "cid1", "evt1", 1700172800.0),
            make_trade("BUY", 0.11, 90.0, "cid1", "evt1", 1700259200.0),
            make_trade("SELL", 0.20, 50.0, "cid1", "evt1", 1700345600.0),
        ];
        let markets = analyze_markets(&trades, &HashMap::new());
        assert_eq!(markets.len(), 1);
        assert!(
            markets[0].avg_buy_price < 0.30,
            "Expected low avg buy price, got {:.3}",
            markets[0].avg_buy_price
        );
        assert_eq!(markets[0].inferred_strategy, MarketStrategy::Contrarian);
    }

    #[test]
    fn test_market_strategy_dca() {
        let trades = vec![
            make_trade("BUY", 0.50, 100.0, "cid1", "evt1", 1700000000.0),
            make_trade("BUY", 0.48, 100.0, "cid1", "evt1", 1700086400.0),
            make_trade("BUY", 0.52, 100.0, "cid1", "evt1", 1700172800.0),
        ];
        let markets = analyze_markets(&trades, &HashMap::new());
        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0].inferred_strategy, MarketStrategy::DCA);
    }

    #[test]
    fn test_trade_grouping_by_market() {
        let trades = vec![
            make_trade("BUY", 0.50, 100.0, "cid1", "evt1", 1700000000.0),
            make_trade("BUY", 0.60, 50.0, "cid2", "evt2", 1700000100.0),
            make_trade("SELL", 0.70, 100.0, "cid1", "evt1", 1700001000.0),
        ];
        let markets = analyze_markets(&trades, &HashMap::new());
        assert_eq!(markets.len(), 2);

        let cid1 = markets.iter().find(|m| m.condition_id == "cid1").unwrap();
        assert_eq!(cid1.trade_count, 2);
        assert_eq!(cid1.buy_count, 1);
        assert_eq!(cid1.sell_count, 1);

        let cid2 = markets.iter().find(|m| m.condition_id == "cid2").unwrap();
        assert_eq!(cid2.trade_count, 1);
        assert_eq!(cid2.buy_count, 1);
        assert_eq!(cid2.sell_count, 0);
    }

    #[test]
    fn test_category_breakdown() {
        let markets = vec![
            MarketAnalysis {
                condition_id: "c1".into(),
                title: "Test".into(),
                event_slug: "".into(),
                category: "Crypto".into(),
                outcome: "Yes".into(),
                trade_count: 5,
                buy_count: 3,
                sell_count: 2,
                first_trade_ts: 0.0,
                last_trade_ts: 0.0,
                total_bought: 300.0,
                total_sold: 200.0,
                net_position: 100.0,
                avg_buy_price: 0.5,
                avg_sell_price: 0.6,
                realized_pnl: 20.0,
                volume: 500.0,
                inferred_strategy: MarketStrategy::MarketMaking,
                strategy_confidence: 0.8,
                trades: vec![],
            },
            MarketAnalysis {
                condition_id: "c2".into(),
                title: "Test2".into(),
                event_slug: "".into(),
                category: "Crypto".into(),
                outcome: "Yes".into(),
                trade_count: 3,
                buy_count: 2,
                sell_count: 1,
                first_trade_ts: 0.0,
                last_trade_ts: 0.0,
                total_bought: 200.0,
                total_sold: 100.0,
                net_position: 100.0,
                avg_buy_price: 0.4,
                avg_sell_price: 0.5,
                realized_pnl: -10.0,
                volume: 300.0,
                inferred_strategy: MarketStrategy::Momentum,
                strategy_confidence: 0.6,
                trades: vec![],
            },
        ];

        let breakdown = compute_category_breakdown(&markets);
        assert_eq!(breakdown.len(), 1);
        assert_eq!(breakdown[0].category, "Crypto");
        assert_eq!(breakdown[0].market_count, 2);
        assert_eq!(breakdown[0].trade_count, 8);
        assert_eq!(breakdown[0].win_rate, 50.0); // 1 of 2 profitable
    }

    #[test]
    fn test_activity_timeline() {
        let trades = vec![
            make_trade("BUY", 0.50, 100.0, "cid1", "evt1", 1700000000.0),
            make_trade("BUY", 0.60, 50.0, "cid2", "evt2", 1700000100.0),
            make_trade("SELL", 0.70, 100.0, "cid1", "evt1", 1700086500.0),
        ];
        let timeline = compute_activity_timeline(&trades);
        assert!(timeline.len() >= 1); // At least one day
        let total_trades: usize = timeline.iter().map(|p| p.trade_count).sum();
        assert_eq!(total_trades, 3);
    }

    #[test]
    fn test_global_strategy_inference() {
        let markets = vec![
            MarketAnalysis {
                condition_id: "c1".into(),
                title: "".into(),
                event_slug: "".into(),
                category: "".into(),
                outcome: "".into(),
                trade_count: 1,
                buy_count: 1,
                sell_count: 0,
                first_trade_ts: 0.0,
                last_trade_ts: 0.0,
                total_bought: 100.0,
                total_sold: 0.0,
                net_position: 100.0,
                avg_buy_price: 0.5,
                avg_sell_price: 0.0,
                realized_pnl: 0.0,
                volume: 50.0,
                inferred_strategy: MarketStrategy::HoldToResolution,
                strategy_confidence: 0.8,
                trades: vec![],
            },
            MarketAnalysis {
                condition_id: "c2".into(),
                title: "".into(),
                event_slug: "".into(),
                category: "".into(),
                outcome: "".into(),
                trade_count: 1,
                buy_count: 1,
                sell_count: 0,
                first_trade_ts: 0.0,
                last_trade_ts: 0.0,
                total_bought: 200.0,
                total_sold: 0.0,
                net_position: 200.0,
                avg_buy_price: 0.6,
                avg_sell_price: 0.0,
                realized_pnl: 0.0,
                volume: 120.0,
                inferred_strategy: MarketStrategy::HoldToResolution,
                strategy_confidence: 0.8,
                trades: vec![],
            },
        ];

        let signals = infer_global_strategy(&markets);
        assert!(!signals.is_empty());
        assert_eq!(signals[0].strategy, MarketStrategy::HoldToResolution);
        assert_eq!(signals[0].market_count, 2);
    }

    #[test]
    fn test_max_drawdown() {
        let markets = vec![
            MarketAnalysis {
                condition_id: "c1".into(),
                title: "".into(),
                event_slug: "".into(),
                category: "".into(),
                outcome: "".into(),
                trade_count: 1,
                buy_count: 1,
                sell_count: 0,
                first_trade_ts: 1.0,
                last_trade_ts: 1.0,
                total_bought: 0.0,
                total_sold: 0.0,
                net_position: 0.0,
                avg_buy_price: 0.0,
                avg_sell_price: 0.0,
                realized_pnl: 100.0,
                volume: 0.0,
                inferred_strategy: MarketStrategy::Momentum,
                strategy_confidence: 0.5,
                trades: vec![],
            },
            MarketAnalysis {
                condition_id: "c2".into(),
                title: "".into(),
                event_slug: "".into(),
                category: "".into(),
                outcome: "".into(),
                trade_count: 1,
                buy_count: 1,
                sell_count: 0,
                first_trade_ts: 2.0,
                last_trade_ts: 2.0,
                total_bought: 0.0,
                total_sold: 0.0,
                net_position: 0.0,
                avg_buy_price: 0.0,
                avg_sell_price: 0.0,
                realized_pnl: -80.0,
                volume: 0.0,
                inferred_strategy: MarketStrategy::Momentum,
                strategy_confidence: 0.5,
                trades: vec![],
            },
        ];

        let dd = compute_max_drawdown(&markets);
        assert_eq!(dd, 80.0); // peak 100, trough 20, dd = 80
    }
}
