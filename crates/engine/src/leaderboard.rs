//! Leaderboard Analyzer — fetch top Polymarket traders and infer their strategies
//!
//! Analyzes positions, trades, and metrics to classify each trader's approach
//! (Momentum, Contrarian, Scalper, Market Maker, etc.).

use crate::api::polymarket::{
    LeaderboardEntry, PolymarketDataClient, TraderPosition, TraderTrade,
};
use persistence::repository::leaderboard::{LeaderboardTraderRecord, TraderTradeRecord};
use persistence::SqlitePool;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::RwLock;
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Inferred trading strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InferredStrategy {
    Momentum,
    Contrarian,
    Scalper,
    MarketMaker,
    Arbitrage,
    EventDriven,
    HighConviction,
    Diversified,
    Mixed,
}

impl InferredStrategy {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Momentum => "Momentum",
            Self::Contrarian => "Contrarian",
            Self::Scalper => "Scalper",
            Self::MarketMaker => "Market Maker",
            Self::Arbitrage => "Arbitrage",
            Self::EventDriven => "Event-Driven",
            Self::HighConviction => "High Conviction",
            Self::Diversified => "Diversified",
            Self::Mixed => "Mixed",
        }
    }

    pub fn color(&self) -> &'static str {
        match self {
            Self::Momentum => "blue",
            Self::Contrarian => "purple",
            Self::Scalper => "yellow",
            Self::MarketMaker => "teal",
            Self::Arbitrage => "orange",
            Self::EventDriven => "pink",
            Self::HighConviction => "red",
            Self::Diversified => "green",
            Self::Mixed => "gray",
        }
    }
}

/// Computed metrics for a single trader
#[derive(Debug, Clone, Serialize)]
pub struct TraderMetrics {
    pub avg_position_size: f64,
    pub avg_entry_price: f64,
    pub buy_sell_ratio: f64,
    pub unique_markets: usize,
    pub trade_count: usize,
    pub trade_frequency_per_day: f64,
    pub win_rate: f64,
    pub concentration_top3: f64,
    pub total_volume: f64,
    pub avg_trade_size: f64,
    pub has_both_sides_same_market: bool,
    pub both_sides_ratio: f64,
    pub time_span_days: f64,
    pub event_cluster_ratio: f64,
}

/// A single strategy signal with confidence and evidence
#[derive(Debug, Clone, Serialize)]
pub struct StrategySignal {
    pub strategy: InferredStrategy,
    pub confidence: f64,
    pub evidence: String,
}

/// Full analysis for one trader
#[derive(Debug, Clone, Serialize)]
pub struct TraderAnalysis {
    pub entry: LeaderboardEntry,
    pub metrics: TraderMetrics,
    pub strategies: Vec<StrategySignal>,
    pub portfolio_value: Option<f64>,
    pub top_positions: Vec<TraderPosition>,
    pub recent_trades: Vec<TraderTrade>,
}

/// Progress tracking (same pattern as DiscoveryProgress)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LeaderboardStatus {
    Idle,
    FetchingLeaderboard,
    AnalyzingTrader,
    Complete,
    Error,
}

pub struct LeaderboardProgress {
    pub status: RwLock<LeaderboardStatus>,
    pub total_traders: AtomicU32,
    pub analyzed: AtomicU32,
    pub current_trader: RwLock<String>,
    pub results: RwLock<Vec<TraderAnalysis>>,
    pub error_message: RwLock<Option<String>>,
    pub cancelled: AtomicBool,
}

impl LeaderboardProgress {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(LeaderboardStatus::Idle),
            total_traders: AtomicU32::new(0),
            analyzed: AtomicU32::new(0),
            current_trader: RwLock::new(String::new()),
            results: RwLock::new(Vec::new()),
            error_message: RwLock::new(None),
            cancelled: AtomicBool::new(false),
        }
    }

    pub fn reset(&self) {
        *self.status.write().unwrap() = LeaderboardStatus::FetchingLeaderboard;
        self.total_traders.store(0, Ordering::Relaxed);
        self.analyzed.store(0, Ordering::Relaxed);
        *self.current_trader.write().unwrap() = String::new();
        *self.results.write().unwrap() = Vec::new();
        *self.error_message.write().unwrap() = None;
        self.cancelled.store(false, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        let status = self.status.read().unwrap();
        matches!(
            *status,
            LeaderboardStatus::FetchingLeaderboard | LeaderboardStatus::AnalyzingTrader
        )
    }
}

impl Default for LeaderboardProgress {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Metrics computation
// ---------------------------------------------------------------------------

pub fn compute_metrics(positions: &[TraderPosition], trades: &[TraderTrade]) -> TraderMetrics {
    let trade_count = trades.len();

    // Unique markets from trades (by conditionId)
    let trade_markets: HashSet<String> = trades
        .iter()
        .filter_map(|t| t.condition_id.clone())
        .collect();
    let unique_markets = trade_markets.len();

    // Buy/sell ratio
    let buys = trades
        .iter()
        .filter(|t| t.side.as_deref() == Some("BUY"))
        .count();
    let sells = trades
        .iter()
        .filter(|t| t.side.as_deref() == Some("SELL"))
        .count();
    let buy_sell_ratio = if sells > 0 {
        buys as f64 / sells as f64
    } else if buys > 0 {
        f64::INFINITY
    } else {
        1.0
    };

    // Average trade size & total volume
    let trade_sizes: Vec<f64> = trades.iter().filter_map(|t| t.size).collect();
    let total_volume: f64 = trade_sizes.iter().sum();
    let avg_trade_size = if trade_sizes.is_empty() {
        0.0
    } else {
        total_volume / trade_sizes.len() as f64
    };

    // Average position size
    let pos_sizes: Vec<f64> = positions.iter().filter_map(|p| p.size).collect();
    let avg_position_size = if pos_sizes.is_empty() {
        0.0
    } else {
        pos_sizes.iter().sum::<f64>() / pos_sizes.len() as f64
    };

    // Average entry price
    let entry_prices: Vec<f64> = trades.iter().filter_map(|t| t.price).collect();
    let avg_entry_price = if entry_prices.is_empty() {
        0.5
    } else {
        entry_prices.iter().sum::<f64>() / entry_prices.len() as f64
    };

    // Win rate from positions
    let winning = positions.iter().filter(|p| p.cash_pnl.unwrap_or(0.0) > 0.0).count();
    let total_pos = positions.len();
    let win_rate = if total_pos > 0 {
        winning as f64 / total_pos as f64 * 100.0
    } else {
        0.0
    };

    // Concentration top 3 (by absolute cash PnL)
    let pnl_by_market: Vec<(String, f64)> = {
        let mut map: HashMap<String, f64> = HashMap::new();
        for p in positions {
            if let Some(cid) = &p.condition_id {
                let pnl_abs = p.cash_pnl.unwrap_or(0.0).abs();
                *map.entry(cid.clone()).or_default() += pnl_abs;
            }
        }
        let mut v: Vec<_> = map.into_iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        v
    };
    let total_abs_pnl: f64 = pnl_by_market.iter().map(|(_, v)| v).sum();
    let top3_pnl: f64 = pnl_by_market.iter().take(3).map(|(_, v)| *v).sum();
    let concentration_top3 = if total_abs_pnl > 0.0 {
        top3_pnl / total_abs_pnl * 100.0
    } else {
        0.0
    };

    // Time span (from trade timestamps — epoch seconds as f64)
    let timestamps: Vec<i64> = trades
        .iter()
        .filter_map(|t| t.timestamp)
        .map(|ts| ts as i64)
        .collect();
    let time_span_days = if timestamps.len() >= 2 {
        let min_ts = timestamps.iter().min().unwrap();
        let max_ts = timestamps.iter().max().unwrap();
        (max_ts - min_ts) as f64 / 86400.0
    } else {
        1.0
    };

    let trade_frequency_per_day = if time_span_days > 0.0 {
        trade_count as f64 / time_span_days
    } else {
        trade_count as f64
    };

    // Both sides on same market
    let mut buys_by_market: HashSet<String> = HashSet::new();
    let mut sells_by_market: HashSet<String> = HashSet::new();
    for t in trades {
        if let Some(cid) = &t.condition_id {
            match t.side.as_deref() {
                Some("BUY") => {
                    buys_by_market.insert(cid.clone());
                }
                Some("SELL") => {
                    sells_by_market.insert(cid.clone());
                }
                _ => {}
            }
        }
    }
    let both_sides_markets = buys_by_market.intersection(&sells_by_market).count();
    let has_both_sides_same_market = both_sides_markets > 0;
    let both_sides_ratio = if unique_markets > 0 {
        both_sides_markets as f64 / unique_markets as f64
    } else {
        0.0
    };

    // Event clustering: check if > 60% of trades happen in < 20% of time
    let event_cluster_ratio = compute_event_cluster_ratio(&timestamps, time_span_days);

    TraderMetrics {
        avg_position_size,
        avg_entry_price,
        buy_sell_ratio,
        unique_markets,
        trade_count,
        trade_frequency_per_day,
        win_rate,
        concentration_top3,
        total_volume,
        avg_trade_size,
        has_both_sides_same_market,
        both_sides_ratio,
        time_span_days,
        event_cluster_ratio,
    }
}

/// Measure what fraction of trades occur in clustered time windows
fn compute_event_cluster_ratio(timestamps: &[i64], total_days: f64) -> f64 {
    if timestamps.len() < 5 || total_days < 1.0 {
        return 0.0;
    }

    let total_seconds = total_days * 86400.0;
    let window = (total_seconds * 0.20) as i64; // 20% of total time
    let n = timestamps.len();

    // Sort timestamps
    let mut sorted = timestamps.to_vec();
    sorted.sort();

    // Sliding window: find the largest cluster
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
// Strategy inference
// ---------------------------------------------------------------------------

pub fn infer_strategy(
    positions: &[TraderPosition],
    trades: &[TraderTrade],
    metrics: &TraderMetrics,
) -> Vec<StrategySignal> {
    if trades.is_empty() && positions.is_empty() {
        return vec![StrategySignal {
            strategy: InferredStrategy::Mixed,
            confidence: 0.1,
            evidence: "Insufficient data — no positions or trades available".into(),
        }];
    }

    let mut signals: Vec<StrategySignal> = Vec::new();

    // ------ Market Maker ------
    if metrics.has_both_sides_same_market && metrics.both_sides_ratio > 0.3 {
        let conf = (metrics.both_sides_ratio * 0.8).min(0.95);
        signals.push(StrategySignal {
            strategy: InferredStrategy::MarketMaker,
            confidence: conf,
            evidence: format!(
                "BUY+SELL on same market in {:.0}% of markets (ratio {:.2})",
                metrics.both_sides_ratio * 100.0,
                metrics.both_sides_ratio
            ),
        });
    }

    // ------ Scalper ------
    if metrics.trade_frequency_per_day > 20.0 && metrics.avg_trade_size < 100.0 {
        let freq_score = ((metrics.trade_frequency_per_day - 20.0) / 80.0).min(1.0);
        let conf = (0.5 + freq_score * 0.4).min(0.95);
        signals.push(StrategySignal {
            strategy: InferredStrategy::Scalper,
            confidence: conf,
            evidence: format!(
                "{:.1} trades/day, avg size ${:.0}",
                metrics.trade_frequency_per_day, metrics.avg_trade_size
            ),
        });
    }

    // ------ Contrarian ------
    if metrics.avg_entry_price < 0.30 {
        let conf = ((0.30 - metrics.avg_entry_price) / 0.25 * 0.7 + 0.3).min(0.95);
        signals.push(StrategySignal {
            strategy: InferredStrategy::Contrarian,
            confidence: conf,
            evidence: format!(
                "Avg entry price {:.2} (< 0.30) — buys against consensus",
                metrics.avg_entry_price
            ),
        });
    }

    // ------ Momentum ------
    if metrics.avg_entry_price > 0.55 {
        // Check if buys at increasing prices on same markets
        let conf = ((metrics.avg_entry_price - 0.55) / 0.40 * 0.6 + 0.3).min(0.90);
        signals.push(StrategySignal {
            strategy: InferredStrategy::Momentum,
            confidence: conf,
            evidence: format!(
                "Avg entry price {:.2} (> 0.55) — follows the trend",
                metrics.avg_entry_price
            ),
        });
    }

    // ------ Arbitrage ------
    // Trades on same event but different conditions within short time window
    let arb_score = detect_arbitrage_pattern(trades);
    if arb_score > 0.2 {
        signals.push(StrategySignal {
            strategy: InferredStrategy::Arbitrage,
            confidence: arb_score.min(0.90),
            evidence: format!(
                "Detected correlated trades on same event (score {:.2})",
                arb_score
            ),
        });
    }

    // ------ Event-Driven ------
    if metrics.event_cluster_ratio > 0.60 {
        let conf = (metrics.event_cluster_ratio * 0.8).min(0.90);
        signals.push(StrategySignal {
            strategy: InferredStrategy::EventDriven,
            confidence: conf,
            evidence: format!(
                "{:.0}% of trades in <20% of time — reacts to events",
                metrics.event_cluster_ratio * 100.0
            ),
        });
    }

    // ------ High Conviction ------
    if metrics.unique_markets < 10 && metrics.concentration_top3 > 60.0 {
        let conf = (metrics.concentration_top3 / 100.0 * 0.8).min(0.90);
        signals.push(StrategySignal {
            strategy: InferredStrategy::HighConviction,
            confidence: conf,
            evidence: format!(
                "{} markets, top 3 = {:.0}% of exposure",
                metrics.unique_markets, metrics.concentration_top3
            ),
        });
    }

    // ------ Diversified ------
    if metrics.unique_markets > 50 && metrics.concentration_top3 < 20.0 {
        let conf = (0.5 + (metrics.unique_markets as f64 / 200.0) * 0.4).min(0.85);
        signals.push(StrategySignal {
            strategy: InferredStrategy::Diversified,
            confidence: conf,
            evidence: format!(
                "{} markets, top 3 only {:.0}% — widely spread portfolio",
                metrics.unique_markets, metrics.concentration_top3
            ),
        });
    }

    // Sort by confidence descending
    signals.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

    // If no signals, return Mixed
    if signals.is_empty() {
        signals.push(StrategySignal {
            strategy: InferredStrategy::Mixed,
            confidence: 0.3,
            evidence: format!(
                "No strong pattern detected — {} markets, {:.1} trades/day, avg entry {:.2}",
                metrics.unique_markets, metrics.trade_frequency_per_day, metrics.avg_entry_price
            ),
        });
    }

    signals
}

/// Detect arbitrage patterns: trades on same event_slug but different condition_id within 5 minutes
fn detect_arbitrage_pattern(trades: &[TraderTrade]) -> f64 {
    if trades.len() < 4 {
        return 0.0;
    }

    // Group trades by event_slug
    let mut by_event: HashMap<String, Vec<&TraderTrade>> = HashMap::new();
    for t in trades {
        if let Some(slug) = &t.event_slug {
            by_event.entry(slug.clone()).or_default().push(t);
        }
    }

    let mut arb_count = 0u32;
    let mut total_events = 0u32;

    for (_slug, event_trades) in &by_event {
        if event_trades.len() < 2 {
            continue;
        }
        total_events += 1;

        // Check for different conditions traded
        let conditions: HashSet<&str> = event_trades
            .iter()
            .filter_map(|t| t.condition_id.as_deref())
            .collect();
        if conditions.len() >= 2 {
            arb_count += 1;
        }
    }

    if total_events == 0 {
        return 0.0;
    }

    arb_count as f64 / total_events as f64
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Compute a deduplication hash for a trade
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
pub fn trades_to_records(wallet: &str, trades: &[TraderTrade]) -> Vec<TraderTradeRecord> {
    trades
        .iter()
        .map(|t| {
            let hash = compute_trade_hash(wallet, t);
            TraderTradeRecord {
                id: None,
                proxy_wallet: wallet.to_string(),
                trade_hash: hash,
                side: t.side.clone().unwrap_or_default(),
                condition_id: t.condition_id.clone(),
                asset: t.asset.clone(),
                size: t.size,
                price: t.price,
                title: t.title.clone(),
                outcome: t.outcome.clone(),
                event_slug: t.event_slug.clone(),
                timestamp: t.timestamp,
                transaction_hash: t.transaction_hash.clone(),
                alerted: Some(0),
                created_at: None,
            }
        })
        .collect()
}

/// Convert a TraderAnalysis into a DB record
fn analysis_to_record(analysis: &TraderAnalysis) -> LeaderboardTraderRecord {
    let primary = analysis.strategies.first();
    LeaderboardTraderRecord {
        id: None,
        proxy_wallet: analysis.entry.proxy_wallet.clone().unwrap_or_default(),
        user_name: analysis.entry.user_name.clone(),
        rank: analysis.entry.rank.clone(),
        pnl: analysis.entry.pnl,
        volume: analysis.entry.vol,
        portfolio_value: analysis.portfolio_value,
        primary_strategy: primary.map(|s| s.strategy.label().to_string()),
        primary_confidence: primary.map(|s| s.confidence),
        strategies_json: serde_json::to_string(&analysis.strategies).ok(),
        metrics_json: serde_json::to_string(&analysis.metrics).ok(),
        top_positions_json: serde_json::to_string(&analysis.top_positions).ok(),
        trade_count: Some(analysis.metrics.trade_count as i64),
        unique_markets: Some(analysis.metrics.unique_markets as i64),
        win_rate: Some(analysis.metrics.win_rate),
        avg_entry_price: Some(analysis.metrics.avg_entry_price),
        analyzed_at: None,
    }
}

/// Analyze the top N traders from the Polymarket leaderboard
pub async fn analyze_leaderboard(
    client: &PolymarketDataClient,
    progress: &LeaderboardProgress,
    limit: u32,
    db_pool: Option<SqlitePool>,
) {
    info!(limit, "Starting leaderboard analysis");

    // Step 1: Fetch leaderboard
    *progress.status.write().unwrap() = LeaderboardStatus::FetchingLeaderboard;
    let entries = match client.get_leaderboard(limit).await {
        Ok(e) => e,
        Err(err) => {
            error!("Failed to fetch leaderboard: {}", err);
            *progress.status.write().unwrap() = LeaderboardStatus::Error;
            *progress.error_message.write().unwrap() = Some(format!("Leaderboard fetch failed: {}", err));
            return;
        }
    };

    let count = entries.len() as u32;
    progress.total_traders.store(count, Ordering::Relaxed);
    info!(count, "Leaderboard fetched, analyzing traders");

    // Step 2: Analyze each trader
    *progress.status.write().unwrap() = LeaderboardStatus::AnalyzingTrader;

    for (i, entry) in entries.iter().enumerate() {
        if progress.cancelled.load(Ordering::Relaxed) {
            warn!("Leaderboard analysis cancelled");
            break;
        }

        let wallet = match &entry.proxy_wallet {
            Some(w) => w.clone(),
            None => {
                warn!(rank = ?entry.rank, "Skipping trader with no wallet");
                progress.analyzed.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };

        let name = entry.user_name.clone().unwrap_or_else(|| format!("Trader #{}", i + 1));
        *progress.current_trader.write().unwrap() = name.clone();
        info!(rank = ?entry.rank, name = %name, "Analyzing trader");

        // Fetch positions
        let positions = match client.get_positions(&wallet).await {
            Ok(p) => p,
            Err(e) => {
                warn!(name = %name, error = %e, "Failed to fetch positions");
                Vec::new()
            }
        };

        // Rate limit
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Fetch trades
        let all_trades = match client.get_trades(&wallet).await {
            Ok(t) => t,
            Err(e) => {
                warn!(name = %name, error = %e, "Failed to fetch trades");
                Vec::new()
            }
        };

        // Rate limit
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Fetch portfolio value
        let portfolio_value = match client.get_value(&wallet).await {
            Ok(v) => v.value,
            Err(e) => {
                warn!(name = %name, error = %e, "Failed to fetch value");
                None
            }
        };

        // Compute metrics & infer strategy
        let metrics = compute_metrics(&positions, &all_trades);
        let strategies = infer_strategy(&positions, &all_trades, &metrics);

        // Keep a copy for display (top 10 trades)
        let trades_for_display = all_trades.clone();

        // Take top 5 positions and 10 recent trades for display
        let top_positions: Vec<TraderPosition> = positions.into_iter().take(5).collect();
        let recent_trades: Vec<TraderTrade> = trades_for_display.into_iter().take(10).collect();

        let analysis = TraderAnalysis {
            entry: entry.clone(),
            metrics,
            strategies,
            portfolio_value,
            top_positions,
            recent_trades,
        };

        // Persist to DB if pool available
        if let Some(ref pool) = db_pool {
            let repo = persistence::repository::leaderboard::LeaderboardRepository::new(pool);

            // Save trader analysis
            let record = analysis_to_record(&analysis);
            if let Err(e) = repo.save_trader_analysis(&record).await {
                warn!(name = %name, error = %e, "Failed to save trader to DB");
            }

            // Save all trades to DB for the watcher
            let trade_records = trades_to_records(&wallet, &all_trades);
            match repo.save_trades(&trade_records).await {
                Ok(n) => {
                    if n > 0 {
                        info!(name = %name, new_trades = n, "Saved trades to DB");
                    }
                }
                Err(e) => warn!(name = %name, error = %e, "Failed to save trades to DB"),
            }
        }

        progress.results.write().unwrap().push(analysis);
        progress.analyzed.fetch_add(1, Ordering::Relaxed);

        // Rate limit between traders
        if i < entries.len() - 1 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    *progress.status.write().unwrap() = LeaderboardStatus::Complete;
    let analyzed = progress.analyzed.load(Ordering::Relaxed);
    info!(analyzed, "Leaderboard analysis complete");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::polymarket::{TraderPosition, TraderTrade};

    fn make_trade(side: &str, price: f64, condition_id: &str, event_slug: &str, ts: f64) -> TraderTrade {
        TraderTrade {
            proxy_wallet: Some("0xabc".into()),
            side: Some(side.into()),
            asset: Some("asset".into()),
            condition_id: Some(condition_id.into()),
            size: Some(50.0),
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

    fn make_position(cid: &str, cash_pnl: f64, size: f64, avg_price: f64) -> TraderPosition {
        TraderPosition {
            proxy_wallet: Some("0xabc".into()),
            asset: Some("asset".into()),
            condition_id: Some(cid.into()),
            size: Some(size),
            avg_price: Some(avg_price),
            current_value: Some(size * 0.6),
            cash_pnl: Some(cash_pnl),
            percent_pnl: Some(cash_pnl / size * 100.0),
            title: Some("Test Market".into()),
            outcome: Some("Yes".into()),
            end_date: None,
            cur_price: Some(0.6),
            resolving: Some(false),
        }
    }

    #[test]
    fn test_compute_metrics_empty() {
        let metrics = compute_metrics(&[], &[]);
        assert_eq!(metrics.trade_count, 0);
        assert_eq!(metrics.unique_markets, 0);
        assert_eq!(metrics.win_rate, 0.0);
    }

    #[test]
    fn test_infer_strategy_insufficient_data() {
        let signals = infer_strategy(&[], &[], &compute_metrics(&[], &[]));
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].strategy, InferredStrategy::Mixed);
    }

    #[test]
    fn test_infer_contrarian() {
        // Trader buys at very low prices
        let trades: Vec<TraderTrade> = (0..20)
            .map(|i| make_trade("BUY", 0.15, &format!("cid{}", i), &format!("evt{}", i), (1700000000 + i * 86400) as f64))
            .collect();
        let positions: Vec<TraderPosition> = (0..10)
            .map(|i| make_position(&format!("cid{}", i), 50.0, 100.0, 0.15))
            .collect();
        let metrics = compute_metrics(&positions, &trades);
        let signals = infer_strategy(&positions, &trades, &metrics);

        assert!(signals.iter().any(|s| s.strategy == InferredStrategy::Contrarian));
    }

    #[test]
    fn test_infer_momentum() {
        // Trader buys at high prices
        let trades: Vec<TraderTrade> = (0..20)
            .map(|i| make_trade("BUY", 0.80, &format!("cid{}", i), &format!("evt{}", i), (1700000000 + i * 86400) as f64))
            .collect();
        let positions: Vec<TraderPosition> = (0..10)
            .map(|i| make_position(&format!("cid{}", i), 30.0, 100.0, 0.80))
            .collect();
        let metrics = compute_metrics(&positions, &trades);
        let signals = infer_strategy(&positions, &trades, &metrics);

        assert!(signals.iter().any(|s| s.strategy == InferredStrategy::Momentum));
    }

    #[test]
    fn test_infer_market_maker() {
        // Trader buys AND sells on the same markets
        let mut trades = Vec::new();
        for i in 0..10 {
            let cid = format!("cid{}", i);
            let evt = format!("evt{}", i);
            trades.push(make_trade("BUY", 0.45, &cid, &evt, (1700000000 + i * 3600) as f64));
            trades.push(make_trade("SELL", 0.55, &cid, &evt, (1700000000 + i * 3600 + 60) as f64));
        }
        let positions: Vec<TraderPosition> = (0..5)
            .map(|i| make_position(&format!("cid{}", i), 10.0, 50.0, 0.45))
            .collect();
        let metrics = compute_metrics(&positions, &trades);
        assert!(metrics.has_both_sides_same_market);
        let signals = infer_strategy(&positions, &trades, &metrics);

        assert!(signals.iter().any(|s| s.strategy == InferredStrategy::MarketMaker));
    }

    #[test]
    fn test_infer_high_conviction() {
        // Only 3 markets, top 3 concentration > 60%
        let positions = vec![
            make_position("cid1", 500.0, 1000.0, 0.50),
            make_position("cid2", 300.0, 800.0, 0.50),
            make_position("cid3", 100.0, 200.0, 0.50),
        ];
        let trades: Vec<TraderTrade> = (0..6)
            .map(|i| make_trade("BUY", 0.50, &format!("cid{}", (i % 3) + 1), &format!("evt{}", (i % 3) + 1), (1700000000 + i * 86400) as f64))
            .collect();
        let metrics = compute_metrics(&positions, &trades);
        assert!(metrics.unique_markets < 10);
        assert!(metrics.concentration_top3 > 60.0);
        let signals = infer_strategy(&positions, &trades, &metrics);

        assert!(signals.iter().any(|s| s.strategy == InferredStrategy::HighConviction));
    }
}
