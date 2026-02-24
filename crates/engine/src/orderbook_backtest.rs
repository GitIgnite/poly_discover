//! Orderbook Backtest — Analyse des marchés BTC 15 minutes Polymarket
//!
//! Détecte des patterns dans l'activité prix/orderbook durant les premières
//! 90-120 secondes d'un marché pour prédire si le marché finira UP ou DOWN.

use crate::api::polymarket::{DataSource, GammaMarket, PolymarketDataClient};
use chrono::Utc;
use persistence::repository::orderbook::{
    ObFeatureRecord, ObMarketRecord, ObPatternRecord, OrderbookRepository,
};
use persistence::SqlitePool;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Time windows (seconds) at which features are extracted
pub const TIME_WINDOWS: &[i64] = &[30, 60, 90, 120, 180, 300];

// ---------------------------------------------------------------------------
// Progress tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ObBacktestStatus {
    Idle,
    Probing,
    DiscoveringMarkets,
    FetchingData,
    ExtractingFeatures,
    DetectingPatterns,
    Cleanup,
    Complete,
    Error,
}

pub struct ObBacktestProgress {
    pub status: RwLock<ObBacktestStatus>,
    pub data_source: RwLock<DataSource>,
    pub total_markets: AtomicU32,
    pub markets_discovered: AtomicU32,
    pub markets_fetched: AtomicU32,
    pub features_extracted: AtomicU32,
    pub patterns_found: AtomicU32,
    pub current_step: RwLock<String>,
    pub error_message: RwLock<Option<String>>,
    pub cancelled: AtomicBool,
    pub best_patterns: RwLock<Vec<DetectedPattern>>,
    pub stats: RwLock<ObBacktestStats>,
    pub logs: RwLock<Vec<String>>,
}

impl ObBacktestProgress {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(ObBacktestStatus::Idle),
            data_source: RwLock::new(DataSource::None),
            total_markets: AtomicU32::new(0),
            markets_discovered: AtomicU32::new(0),
            markets_fetched: AtomicU32::new(0),
            features_extracted: AtomicU32::new(0),
            patterns_found: AtomicU32::new(0),
            current_step: RwLock::new(String::new()),
            error_message: RwLock::new(None),
            cancelled: AtomicBool::new(false),
            best_patterns: RwLock::new(Vec::new()),
            stats: RwLock::new(ObBacktestStats::default()),
            logs: RwLock::new(Vec::new()),
        }
    }

    pub fn reset(&self) {
        *self.status.write().unwrap() = ObBacktestStatus::Idle;
        *self.data_source.write().unwrap() = DataSource::None;
        self.total_markets.store(0, Ordering::Relaxed);
        self.markets_discovered.store(0, Ordering::Relaxed);
        self.markets_fetched.store(0, Ordering::Relaxed);
        self.features_extracted.store(0, Ordering::Relaxed);
        self.patterns_found.store(0, Ordering::Relaxed);
        *self.current_step.write().unwrap() = String::new();
        *self.error_message.write().unwrap() = None;
        self.cancelled.store(false, Ordering::Relaxed);
        *self.best_patterns.write().unwrap() = Vec::new();
        *self.stats.write().unwrap() = ObBacktestStats::default();
        self.logs.write().unwrap().clear();
    }

    pub fn is_running(&self) -> bool {
        let s = *self.status.read().unwrap();
        !matches!(s, ObBacktestStatus::Idle | ObBacktestStatus::Complete | ObBacktestStatus::Error)
    }

    pub fn set_status(&self, s: ObBacktestStatus) {
        *self.status.write().unwrap() = s;
    }

    pub fn set_step(&self, step: &str) {
        *self.current_step.write().unwrap() = step.to_string();
    }

    pub fn set_error(&self, msg: String) {
        self.add_log(&format!("ERROR: {}", msg));
        *self.error_message.write().unwrap() = Some(msg);
        *self.status.write().unwrap() = ObBacktestStatus::Error;
    }

    pub fn add_log(&self, msg: &str) {
        let timestamp = Utc::now().format("%H:%M:%S");
        let mut logs = self.logs.write().unwrap();
        logs.push(format!("[{}] {}", timestamp, msg));
        if logs.len() > 200 {
            let excess = logs.len() - 200;
            logs.drain(..excess);
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

impl Default for ObBacktestProgress {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ObBacktestStats {
    pub total_markets: u32,
    pub markets_with_data: u32,
    pub up_outcomes: u32,
    pub down_outcomes: u32,
    pub avg_data_points: f64,
    pub patterns_detected: u32,
    pub best_accuracy_90s: f64,
    pub best_accuracy_120s: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectedPattern {
    pub name: String,
    pub pattern_type: String,
    pub time_window: i64,
    pub direction: String,
    pub features_used: Vec<String>,
    pub description: String,
    pub accuracy: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub sample_size: usize,
    pub confidence_interval: (f64, f64),
    pub stability_score: f64,
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

pub async fn run_orderbook_backtest(
    progress: &ObBacktestProgress,
    client: &PolymarketDataClient,
    db_pool: SqlitePool,
) {
    progress.reset();

    // Step 1: Probe data sources
    progress.set_status(ObBacktestStatus::Probing);
    progress.set_step("Probing available data sources...");
    progress.add_log("Probing data sources...");
    info!("Orderbook backtest: probing data sources");

    // Use a known BTC 15-min market condition_id for probing
    // We'll try to discover one first, then probe with it
    let (probe_condition_id, probe_token_id) = match find_probe_market(client).await {
        Some((cid, tid)) => {
            progress.add_log(&format!("Probe market found: {} (token: {:?})", cid, tid));
            (cid, tid)
        }
        None => {
            progress.set_error("Could not find any BTC 15-min market for probing".into());
            return;
        }
    };

    let data_source = client
        .probe_best_data_source(&probe_condition_id, probe_token_id.as_deref())
        .await;
    *progress.data_source.write().unwrap() = data_source;
    progress.add_log(&format!("Best data source: {}", data_source));
    info!("Best data source: {}", data_source);

    if data_source == DataSource::None {
        progress.set_error("No data source available for historical market data".into());
        return;
    }

    if progress.is_cancelled() { return; }

    // Step 2: Discover markets
    progress.set_status(ObBacktestStatus::DiscoveringMarkets);
    progress.set_step("Discovering BTC 15-min markets via Gamma API...");
    progress.add_log("Discovering BTC 15-min markets via Gamma API...");
    info!("Orderbook backtest: discovering markets");

    let markets = match client.get_all_btc_15min_markets().await {
        Ok(m) => m,
        Err(e) => {
            progress.set_error(format!("Market discovery failed: {}", e));
            return;
        }
    };

    info!(count = markets.len(), "Markets discovered");
    progress.add_log(&format!("Found {} markets", markets.len()));
    progress.markets_discovered.store(markets.len() as u32, Ordering::Relaxed);

    // Convert to DB records and save
    let market_records: Vec<ObMarketRecord> = markets
        .iter()
        .filter_map(|m| gamma_market_to_record(m))
        .collect();

    if market_records.is_empty() {
        progress.set_error("No valid BTC 15-min markets found".into());
        return;
    }

    match OrderbookRepository::save_markets_batch(&db_pool, &market_records).await {
        Ok(n) => {
            progress.add_log(&format!("Markets saved to DB: {} inserted", n));
            info!(inserted = n, "Markets saved to DB");
        }
        Err(e) => {
            progress.set_error(format!("Failed to save markets: {}", e));
            return;
        }
    }

    if progress.is_cancelled() { return; }

    // Step 3: Fetch data for unfetched markets
    progress.set_status(ObBacktestStatus::FetchingData);
    progress.set_step("Fetching price data for markets...");
    progress.add_log("Fetching price data for unfetched markets...");

    let batch_size = 1000i64;
    loop {
        if progress.is_cancelled() { return; }

        let unfetched = match OrderbookRepository::get_unfetched_markets(&db_pool, batch_size).await {
            Ok(m) => m,
            Err(e) => {
                progress.set_error(format!("Failed to get unfetched markets: {}", e));
                return;
            }
        };

        if unfetched.is_empty() {
            break;
        }

        let total = OrderbookRepository::get_market_count(&db_pool).await.unwrap_or(0);
        progress.total_markets.store(total as u32, Ordering::Relaxed);

        for market in &unfetched {
            if progress.is_cancelled() { return; }

            let market_id = market.id.unwrap_or(0);
            progress.set_step(&format!(
                "Fetching data for market {} ({}/{})",
                market.condition_id,
                progress.markets_fetched.load(Ordering::Relaxed),
                total
            ));

            // For PricesHistory, use token_id; for trades, use condition_id
            let fetch_id = if data_source == DataSource::PricesHistory {
                market.token_id_up.as_deref().unwrap_or(&market.condition_id)
            } else {
                &market.condition_id
            };

            let prices = fetch_market_data(
                client,
                fetch_id,
                market.start_time,
                market.end_time,
                data_source,
            )
            .await;

            if !prices.is_empty() {
                let price_records: Vec<persistence::repository::orderbook::ObPriceRecord> = prices
                    .iter()
                    .map(|p| persistence::repository::orderbook::ObPriceRecord {
                        id: None,
                        market_id,
                        timestamp_ms: p.0,
                        elapsed_seconds: p.1,
                        price: p.2,
                        side: p.3.clone(),
                        size: p.4,
                    })
                    .collect();

                let data_points = price_records.len() as i64;
                if let Err(e) = OrderbookRepository::save_prices_batch(&db_pool, &price_records).await {
                    warn!("Failed to save prices for market {}: {}", market_id, e);
                }
                let _ = OrderbookRepository::mark_market_fetched(&db_pool, market_id, data_points).await;
            } else {
                // No data available — mark as fetched with 0 points
                let _ = OrderbookRepository::mark_market_fetched(&db_pool, market_id, 0).await;
            }

            progress.markets_fetched.fetch_add(1, Ordering::Relaxed);

            // Rate limit
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    if progress.is_cancelled() { return; }

    // Step 4: Extract features
    progress.set_status(ObBacktestStatus::ExtractingFeatures);
    progress.set_step("Extracting features from market data...");
    progress.add_log("Extracting features from market data...");
    info!("Orderbook backtest: extracting features");

    let batch_size = 1000i64;
    loop {
        if progress.is_cancelled() { return; }

        let fetched_markets = match OrderbookRepository::get_fetched_markets(&db_pool, batch_size).await {
            Ok(m) => m,
            Err(e) => {
                progress.set_error(format!("Failed to get fetched markets: {}", e));
                return;
            }
        };

        if fetched_markets.is_empty() {
            break;
        }

        for market in &fetched_markets {
            if progress.is_cancelled() { return; }

            let market_id = market.id.unwrap_or(0);
            let prices = match OrderbookRepository::get_prices_for_market(&db_pool, market_id).await {
                Ok(p) => p,
                Err(_) => continue,
            };

            if prices.is_empty() {
                let _ = OrderbookRepository::mark_features_extracted(&db_pool, market_id).await;
                progress.features_extracted.fetch_add(1, Ordering::Relaxed);
                continue;
            }

            let outcome_is_up = market
                .outcome
                .as_ref()
                .map(|o| o.to_lowercase().contains("up") || o.to_lowercase() == "yes")
                .unwrap_or(false);

            let feature_records = extract_features_for_market(market_id, &prices, outcome_is_up);

            if !feature_records.is_empty() {
                if let Err(e) = OrderbookRepository::save_features_batch(&db_pool, &feature_records).await {
                    warn!("Failed to save features for market {}: {}", market_id, e);
                }
            }

            let _ = OrderbookRepository::mark_features_extracted(&db_pool, market_id).await;
            progress.features_extracted.fetch_add(1, Ordering::Relaxed);
        }
    }

    if progress.is_cancelled() { return; }

    // Step 5: Detect patterns
    progress.set_status(ObBacktestStatus::DetectingPatterns);
    progress.set_step("Detecting patterns from features...");
    progress.add_log("Detecting patterns from features...");
    info!("Orderbook backtest: detecting patterns");

    let run_id = format!("run_{}", Utc::now().timestamp());
    let mut all_patterns: Vec<DetectedPattern> = Vec::new();

    // Univariate + multivariate patterns per time window
    for &window in TIME_WINDOWS {
        if progress.is_cancelled() { return; }

        let features = match OrderbookRepository::get_all_features_for_window(&db_pool, window).await {
            Ok(f) => f,
            Err(_) => continue,
        };

        if features.len() < 50 {
            debug!(window, count = features.len(), "Not enough features for pattern detection");
            continue;
        }

        let univariate = detect_univariate_patterns(&features, window);
        let multivariate = detect_multivariate_patterns(&features, &univariate, window);

        all_patterns.extend(univariate);
        all_patterns.extend(multivariate);
    }

    // Sequence patterns
    let features_grouped = match OrderbookRepository::get_features_grouped_by_market(&db_pool).await {
        Ok(f) => f,
        Err(_) => HashMap::new(),
    };
    if features_grouped.len() >= 50 {
        let sequence = detect_sequence_patterns(&features_grouped);
        all_patterns.extend(sequence);
    }

    // Sort by accuracy descending
    all_patterns.sort_by(|a, b| b.accuracy.partial_cmp(&a.accuracy).unwrap_or(std::cmp::Ordering::Equal));

    // Save patterns to DB
    let pattern_records: Vec<ObPatternRecord> = all_patterns
        .iter()
        .map(|p| pattern_to_record(p, &run_id))
        .collect();

    if !pattern_records.is_empty() {
        match OrderbookRepository::save_patterns(&db_pool, &pattern_records, &run_id).await {
            Ok(n) => {
                progress.add_log(&format!("Patterns saved to DB: {}", n));
                info!(count = n, "Patterns saved to DB");
            }
            Err(e) => warn!("Failed to save patterns: {}", e),
        }
    }

    progress.patterns_found.store(all_patterns.len() as u32, Ordering::Relaxed);
    progress.add_log(&format!("{} patterns detected total", all_patterns.len()));
    *progress.best_patterns.write().unwrap() = all_patterns.iter().take(20).cloned().collect();

    // Update stats
    {
        let stats = OrderbookRepository::get_market_stats(&db_pool).await.unwrap_or_default();
        let best_90 = all_patterns
            .iter()
            .filter(|p| p.time_window == 90)
            .map(|p| p.accuracy)
            .fold(0.0f64, f64::max);
        let best_120 = all_patterns
            .iter()
            .filter(|p| p.time_window == 120)
            .map(|p| p.accuracy)
            .fold(0.0f64, f64::max);

        let up_count = all_patterns.iter().filter(|p| p.direction == "UP").count() as u32;
        let down_count = all_patterns.iter().filter(|p| p.direction == "DOWN").count() as u32;

        *progress.stats.write().unwrap() = ObBacktestStats {
            total_markets: stats.total_markets as u32,
            markets_with_data: stats.fetched_markets as u32,
            up_outcomes: up_count,
            down_outcomes: down_count,
            avg_data_points: if stats.fetched_markets > 0 {
                stats.total_prices as f64 / stats.fetched_markets as f64
            } else {
                0.0
            },
            patterns_detected: all_patterns.len() as u32,
            best_accuracy_90s: best_90,
            best_accuracy_120s: best_120,
        };
    }

    if progress.is_cancelled() { return; }

    // Step 6: Cleanup
    progress.set_status(ObBacktestStatus::Cleanup);
    progress.set_step("Cleaning up raw price data...");
    progress.add_log("Cleaning up raw price data...");

    match OrderbookRepository::purge_prices_for_extracted(&db_pool).await {
        Ok(n) => {
            progress.add_log(&format!("Price data purged: {} rows", n));
            info!(purged = n, "Price data purged");
        }
        Err(e) => warn!("Purge failed: {}", e),
    }
    match OrderbookRepository::purge_old_snapshots(&db_pool, 30).await {
        Ok(n) => {
            progress.add_log(&format!("Old snapshots purged: {} rows", n));
            info!(purged = n, "Old snapshots purged");
        }
        Err(e) => warn!("Snapshot purge failed: {}", e),
    }

    progress.set_status(ObBacktestStatus::Complete);
    progress.set_step("Analysis complete");
    progress.add_log(&format!("Backtest complete: {} patterns detected", all_patterns.len()));
    info!(
        patterns = all_patterns.len(),
        "Orderbook backtest complete"
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns (condition_id, optional first token_id) for probing.
/// Searches newest markets first (all states) — BTC 15-min markets are always near the top.
async fn find_probe_market(client: &PolymarketDataClient) -> Option<(String, Option<String>)> {
    // Search newest markets (any state) — BTC 15-min markets cycle every 15 min
    // so they're always among the newest markets.
    for page in 0..5 {
        let offset = page * 100;
        match client.search_markets(offset, 100, None, true).await {
            Ok(markets) => {
                if markets.is_empty() {
                    break;
                }
                for m in &markets {
                    if is_btc_15min_question(m.question.as_deref()) {
                        if let Some(ref cid) = m.condition_id {
                            let (token_up, _) = parse_clob_token_ids(m.clob_token_ids.as_deref());
                            if token_up.is_some() {
                                info!(
                                    condition_id = %cid,
                                    token_id = ?token_up,
                                    question = ?m.question,
                                    closed = ?m.closed,
                                    "Found BTC 15-min probe market"
                                );
                                return Some((cid.clone(), token_up));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to search for probe market at offset {}: {}", offset, e);
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    warn!("Could not find a BTC 15-min market with token_id for probing");
    None
}

/// Check if a question matches the BTC short-term market pattern.
/// Matches "Bitcoin Up or Down - February 25, 2:45PM-3:00PM ET" style questions.
fn is_btc_15min_question(question: Option<&str>) -> bool {
    if let Some(q) = question {
        let q_lower = q.to_lowercase();
        (q_lower.contains("bitcoin") || q_lower.contains("btc"))
            && (q_lower.contains("up or down")
                || q_lower.contains("go up")
                || q_lower.contains("above")
                || q_lower.contains("higher"))
    } else {
        false
    }
}

fn gamma_market_to_record(m: &GammaMarket) -> Option<ObMarketRecord> {
    let condition_id = m.condition_id.as_ref()?.clone();

    // Parse end_date to epoch
    let end_date_str = m.end_date.as_ref()?;
    let end_time = chrono::DateTime::parse_from_rfc3339(end_date_str)
        .ok()
        .or_else(|| chrono::DateTime::parse_from_str(end_date_str, "%Y-%m-%dT%H:%M:%S%.fZ").ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    // start_time = end_time - 15 minutes (900 seconds)
    let start_time = end_time - 900;

    // Parse outcome from outcome_prices
    let (outcome, outcome_price_up, outcome_price_down) =
        parse_market_outcome(m.outcome_prices.as_deref());

    let volume = m
        .volume
        .as_ref()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    // Parse clobTokenIds: JSON string like "[\"token1\", \"token2\"]"
    let (token_id_up, token_id_down) = parse_clob_token_ids(m.clob_token_ids.as_deref());

    Some(ObMarketRecord {
        id: None,
        condition_id,
        question: m.question.clone(),
        slug: m.slug.clone(),
        token_id_up,
        token_id_down,
        start_time,
        end_time,
        outcome,
        outcome_price_up,
        outcome_price_down,
        volume: Some(volume),
        data_fetched: Some(0),
        data_points_count: Some(0),
        created_at: None,
    })
}

/// Parse clobTokenIds from Gamma API (JSON string like "[\"token1\",\"token2\"]")
fn parse_clob_token_ids(raw: Option<&str>) -> (Option<String>, Option<String>) {
    if let Some(s) = raw {
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(s) {
            return (ids.first().cloned(), ids.get(1).cloned());
        }
    }
    (None, None)
}

/// Parse outcome from outcome_prices string like "[0.95,0.05]"
pub fn parse_market_outcome(
    outcome_prices: Option<&str>,
) -> (Option<String>, Option<f64>, Option<f64>) {
    if let Some(s) = outcome_prices {
        // Parse JSON array
        if let Ok(prices) = serde_json::from_str::<Vec<f64>>(s) {
            if prices.len() >= 2 {
                let p_up = prices[0];
                let p_down = prices[1];
                let outcome = if p_up > 0.5 {
                    Some("Up".to_string())
                } else if p_down > 0.5 {
                    Some("Down".to_string())
                } else {
                    None
                };
                return (outcome, Some(p_up), Some(p_down));
            }
        }
        // Try parsing string prices
        if let Ok(prices) = serde_json::from_str::<Vec<String>>(s) {
            if prices.len() >= 2 {
                let p_up = prices[0].parse::<f64>().unwrap_or(0.0);
                let p_down = prices[1].parse::<f64>().unwrap_or(0.0);
                let outcome = if p_up > 0.5 {
                    Some("Up".to_string())
                } else if p_down > 0.5 {
                    Some("Down".to_string())
                } else {
                    None
                };
                return (outcome, Some(p_up), Some(p_down));
            }
        }
    }
    (None, None, None)
}

/// Fetch price data for a market using the given data source.
/// Returns Vec<(timestamp_ms, elapsed_seconds, price, side, size)>.
async fn fetch_market_data(
    client: &PolymarketDataClient,
    market_id: &str, // token_id for PricesHistory, condition_id for trades
    start_time: i64,
    end_time: i64,
    source: DataSource,
) -> Vec<(i64, f64, f64, Option<String>, Option<f64>)> {
    match source {
        DataSource::PricesHistory => {
            match client.get_prices_history(market_id, start_time, end_time).await {
                Ok(points) => points
                    .into_iter()
                    .filter_map(|p| {
                        let price = p.p.parse::<f64>().ok()?;
                        let ts_ms = p.t * 1000; // Assume t is seconds
                        let elapsed = (p.t - start_time) as f64;
                        Some((ts_ms, elapsed, price, None, None))
                    })
                    .collect(),
                Err(_) => Vec::new(),
            }
        }
        DataSource::ClobTrades => {
            match client.try_get_market_trades(market_id).await {
                Ok(trades) => trades
                    .into_iter()
                    .filter_map(|t| {
                        let price = t.price.parse::<f64>().ok()?;
                        let size = t.size.parse::<f64>().ok();
                        let ts = t.timestamp.parse::<f64>().ok()? as i64;
                        let ts_ms = ts * 1000;
                        let elapsed = (ts - start_time) as f64;
                        Some((ts_ms, elapsed, price, Some(t.side), size))
                    })
                    .collect(),
                Err(_) => Vec::new(),
            }
        }
        DataSource::DataApiTrades => {
            match client.try_get_data_api_market_trades(market_id).await {
                Ok(trades) => trades
                    .into_iter()
                    .filter_map(|t| {
                        let price = t.price?;
                        let size = t.size;
                        let ts = t.timestamp? as i64;
                        let ts_ms = ts * 1000;
                        let elapsed = (ts - start_time) as f64;
                        let side = t.side;
                        Some((ts_ms, elapsed, price, side, size))
                    })
                    .collect(),
                Err(_) => Vec::new(),
            }
        }
        DataSource::None => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Feature extraction
// ---------------------------------------------------------------------------

fn extract_features_for_market(
    market_id: i64,
    prices: &[persistence::repository::orderbook::ObPriceRecord],
    outcome_is_up: bool,
) -> Vec<ObFeatureRecord> {
    let mut features = Vec::new();

    for &window in TIME_WINDOWS {
        let window_prices: Vec<&persistence::repository::orderbook::ObPriceRecord> = prices
            .iter()
            .filter(|p| p.elapsed_seconds <= window as f64 && p.elapsed_seconds >= 0.0)
            .collect();

        if window_prices.is_empty() {
            continue;
        }

        let first_price = window_prices.first().map(|p| p.price).unwrap_or(0.5);
        let last_price = window_prices.last().map(|p| p.price).unwrap_or(0.5);
        let price_change = last_price - first_price;

        let price_vals: Vec<f64> = window_prices.iter().map(|p| p.price).collect();
        let vwap = compute_vwap(&window_prices);
        let volatility = compute_std_dev(&price_vals);
        let max_price = price_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min_price = price_vals.iter().cloned().fold(f64::INFINITY, f64::min);
        let price_range = max_price - min_price;

        // Momentum (linear regression slope)
        let points: Vec<(f64, f64)> = window_prices
            .iter()
            .map(|p| (p.elapsed_seconds, p.price))
            .collect();
        let momentum = compute_linear_regression_slope(&points);

        // Volume features (if trade data available)
        let has_trade_data = window_prices.iter().any(|p| p.side.is_some());
        let (buy_volume, sell_volume, volume_imbalance, trade_count, avg_trade_size, large_trade_ratio) =
            if has_trade_data {
                compute_volume_features(&window_prices)
            } else {
                (None, None, None, None, None, None)
            };

        features.push(ObFeatureRecord {
            id: None,
            market_id,
            time_window: window,
            last_price: Some(last_price),
            vwap: Some(vwap),
            price_change: Some(price_change),
            price_volatility: Some(volatility),
            momentum: Some(momentum),
            max_price: Some(max_price),
            min_price: Some(min_price),
            price_range: Some(price_range),
            data_points: Some(window_prices.len() as i64),
            buy_volume,
            sell_volume,
            volume_imbalance,
            trade_count,
            avg_trade_size,
            large_trade_ratio,
            avg_spread: None,
            depth_imbalance: None,
            avg_bid_depth: None,
            avg_ask_depth: None,
            outcome_is_up: Some(if outcome_is_up { 1 } else { 0 }),
        });
    }

    features
}

/// Compute VWAP: volume-weighted if available, otherwise time-weighted average.
pub fn compute_vwap(
    prices: &[&persistence::repository::orderbook::ObPriceRecord],
) -> f64 {
    let has_volume = prices.iter().any(|p| p.size.is_some() && p.size.unwrap_or(0.0) > 0.0);

    if has_volume {
        let mut sum_pv = 0.0;
        let mut sum_v = 0.0;
        for p in prices {
            let v = p.size.unwrap_or(0.0);
            if v > 0.0 {
                sum_pv += p.price * v;
                sum_v += v;
            }
        }
        if sum_v > 0.0 {
            return sum_pv / sum_v;
        }
    }

    // Time-weighted average
    if prices.is_empty() {
        return 0.0;
    }
    let sum: f64 = prices.iter().map(|p| p.price).sum();
    sum / prices.len() as f64
}

/// Linear regression slope: y = ax + b, returns a.
pub fn compute_linear_regression_slope(points: &[(f64, f64)]) -> f64 {
    let n = points.len() as f64;
    if n < 2.0 {
        return 0.0;
    }

    let sum_x: f64 = points.iter().map(|(x, _)| x).sum();
    let sum_y: f64 = points.iter().map(|(_, y)| y).sum();
    let sum_xy: f64 = points.iter().map(|(x, y)| x * y).sum();
    let sum_x2: f64 = points.iter().map(|(x, _)| x * x).sum();

    let denom = n * sum_x2 - sum_x * sum_x;
    if denom.abs() < 1e-15 {
        return 0.0;
    }

    (n * sum_xy - sum_x * sum_y) / denom
}

fn compute_std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

fn compute_volume_features(
    prices: &[&persistence::repository::orderbook::ObPriceRecord],
) -> (Option<f64>, Option<f64>, Option<f64>, Option<i64>, Option<f64>, Option<f64>) {
    let mut buy_vol = 0.0;
    let mut sell_vol = 0.0;
    let mut trade_sizes: Vec<f64> = Vec::new();

    for p in prices {
        let size = p.size.unwrap_or(0.0);
        match p.side.as_deref() {
            Some("BUY") | Some("buy") => buy_vol += size,
            Some("SELL") | Some("sell") => sell_vol += size,
            _ => {}
        }
        if size > 0.0 {
            trade_sizes.push(size);
        }
    }

    let total_vol = buy_vol + sell_vol;
    let imbalance = if total_vol > 0.0 {
        (buy_vol - sell_vol) / total_vol
    } else {
        0.0
    };

    let avg_size = if trade_sizes.is_empty() {
        0.0
    } else {
        trade_sizes.iter().sum::<f64>() / trade_sizes.len() as f64
    };

    // Large trade ratio: proportion of trades > 2x average
    let large_ratio = if !trade_sizes.is_empty() && avg_size > 0.0 {
        let large_count = trade_sizes.iter().filter(|&&s| s > 2.0 * avg_size).count();
        large_count as f64 / trade_sizes.len() as f64
    } else {
        0.0
    };

    (
        Some(buy_vol),
        Some(sell_vol),
        Some(imbalance),
        Some(trade_sizes.len() as i64),
        Some(avg_size),
        Some(large_ratio),
    )
}

// ---------------------------------------------------------------------------
// Pattern detection
// ---------------------------------------------------------------------------

/// Detect univariate patterns: each feature × each threshold → best accuracy.
fn detect_univariate_patterns(
    features: &[ObFeatureRecord],
    window: i64,
) -> Vec<DetectedPattern> {
    let feature_names = [
        "price_change",
        "momentum",
        "price_volatility",
        "vwap",
        "last_price",
        "price_range",
        "volume_imbalance",
    ];

    let mut patterns = Vec::new();

    for feature_name in &feature_names {
        let values_with_outcome: Vec<(f64, bool)> = features
            .iter()
            .filter_map(|f| {
                let val = get_feature_value(f, feature_name)?;
                let is_up = f.outcome_is_up? == 1;
                Some((val, is_up))
            })
            .collect();

        if values_with_outcome.len() < 100 {
            continue;
        }

        // Try for UP direction and DOWN direction
        for direction in &["UP", "DOWN"] {
            if let Some(pattern) =
                find_best_threshold(&values_with_outcome, feature_name, window, direction)
            {
                patterns.push(pattern);
            }
        }
    }

    patterns
}

fn get_feature_value(f: &ObFeatureRecord, name: &str) -> Option<f64> {
    match name {
        "price_change" => f.price_change,
        "momentum" => f.momentum,
        "price_volatility" => f.price_volatility,
        "vwap" => f.vwap,
        "last_price" => f.last_price,
        "price_range" => f.price_range,
        "volume_imbalance" => f.volume_imbalance,
        "buy_volume" => f.buy_volume,
        "sell_volume" => f.sell_volume,
        "max_price" => f.max_price,
        "min_price" => f.min_price,
        _ => None,
    }
}

fn find_best_threshold(
    values: &[(f64, bool)],
    feature_name: &str,
    window: i64,
    direction: &str,
) -> Option<DetectedPattern> {
    let predict_up = direction == "UP";

    let mut sorted_vals: Vec<f64> = values.iter().map(|(v, _)| *v).collect();
    sorted_vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = sorted_vals.len();
    let percentiles = [10, 20, 30, 40, 50, 60, 70, 80, 90];

    let mut best_accuracy = 0.0f64;
    let mut best_threshold = 0.0f64;
    let mut best_above = true;

    for pct in &percentiles {
        let idx = (n * *pct as usize) / 100;
        let threshold = sorted_vals[idx.min(n - 1)];

        // Test: feature > threshold → direction
        let (acc_above, _, _, _) = compute_classification_metrics(values, threshold, true, predict_up);
        // Test: feature < threshold → direction
        let (acc_below, _, _, _) = compute_classification_metrics(values, threshold, false, predict_up);

        if acc_above > best_accuracy {
            best_accuracy = acc_above;
            best_threshold = threshold;
            best_above = true;
        }
        if acc_below > best_accuracy {
            best_accuracy = acc_below;
            best_threshold = threshold;
            best_above = false;
        }
    }

    // Filter: accuracy > 55%, sample_size > 100
    if best_accuracy <= 0.55 || values.len() < 100 {
        return None;
    }

    // Compute full metrics at best threshold
    let (accuracy, precision, recall, f1) =
        compute_classification_metrics(values, best_threshold, best_above, predict_up);

    // Stability: accuracy on first half vs second half
    let mid = values.len() / 2;
    let first_half = &values[..mid];
    let second_half = &values[mid..];
    let (acc_first, _, _, _) = compute_classification_metrics(first_half, best_threshold, best_above, predict_up);
    let (acc_second, _, _, _) = compute_classification_metrics(second_half, best_threshold, best_above, predict_up);
    let stability = 1.0 - (acc_first - acc_second).abs();

    if stability < 0.7 {
        return None;
    }

    let ci = confidence_interval_95(accuracy, values.len());

    let op = if best_above { ">" } else { "<" };
    let name = format!("{}{}{}@{}s", feature_name, op, format_threshold(best_threshold), window);
    let desc = format!(
        "{} {} {:.4} a {}s → {} (accuracy: {:.1}%, stabilite: {:.2})",
        feature_name, op, best_threshold, window, direction, accuracy * 100.0, stability
    );

    Some(DetectedPattern {
        name,
        pattern_type: "univariate".to_string(),
        time_window: window,
        direction: direction.to_string(),
        features_used: vec![feature_name.to_string()],
        description: desc,
        accuracy,
        precision,
        recall,
        f1_score: f1,
        sample_size: values.len(),
        confidence_interval: ci,
        stability_score: stability,
    })
}

fn format_threshold(v: f64) -> String {
    if v.abs() < 0.001 {
        format!("{:.6}", v)
    } else if v.abs() < 1.0 {
        format!("{:.4}", v)
    } else {
        format!("{:.2}", v)
    }
}

fn compute_classification_metrics(
    values: &[(f64, bool)],
    threshold: f64,
    above: bool,
    predict_up: bool,
) -> (f64, f64, f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }

    let mut tp = 0u32;
    let mut fp = 0u32;
    let mut tn = 0u32;
    let mut fn_ = 0u32;

    for &(val, is_up) in values {
        let predicted = if above { val > threshold } else { val < threshold };
        let actual = if predict_up { is_up } else { !is_up };

        match (predicted, actual) {
            (true, true) => tp += 1,
            (true, false) => fp += 1,
            (false, true) => fn_ += 1,
            (false, false) => tn += 1,
        }
    }

    let total = (tp + fp + tn + fn_) as f64;
    let accuracy = if total > 0.0 { (tp + tn) as f64 / total } else { 0.0 };
    let precision = if (tp + fp) > 0 { tp as f64 / (tp + fp) as f64 } else { 0.0 };
    let recall = if (tp + fn_) > 0 { tp as f64 / (tp + fn_) as f64 } else { 0.0 };
    let f1 = if (precision + recall) > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    (accuracy, precision, recall, f1)
}

/// Detect multivariate patterns: combine top 2-3 features.
fn detect_multivariate_patterns(
    features: &[ObFeatureRecord],
    univariate: &[DetectedPattern],
    window: i64,
) -> Vec<DetectedPattern> {
    let mut patterns = Vec::new();

    // Take top 5 univariate patterns for this window
    let top: Vec<&DetectedPattern> = univariate
        .iter()
        .filter(|p| p.time_window == window)
        .take(5)
        .collect();

    if top.len() < 2 {
        return patterns;
    }

    // Try all pairs
    for i in 0..top.len() {
        for j in (i + 1)..top.len() {
            let feat1 = &top[i].features_used[0];
            let feat2 = &top[j].features_used[0];

            let values: Vec<(f64, f64, bool)> = features
                .iter()
                .filter_map(|f| {
                    let v1 = get_feature_value(f, feat1)?;
                    let v2 = get_feature_value(f, feat2)?;
                    let is_up = f.outcome_is_up? == 1;
                    Some((v1, v2, is_up))
                })
                .collect();

            if values.len() < 100 {
                continue;
            }

            // Both features agree on direction
            for direction in &["UP", "DOWN"] {
                if let Some(pattern) = find_best_multivariate_pair(
                    &values, feat1, feat2, window, direction,
                ) {
                    patterns.push(pattern);
                }
            }
        }
    }

    patterns
}

fn find_best_multivariate_pair(
    values: &[(f64, f64, bool)],
    feat1: &str,
    feat2: &str,
    window: i64,
    direction: &str,
) -> Option<DetectedPattern> {
    let predict_up = direction == "UP";
    let n = values.len();

    // Sort each dimension and get percentile thresholds
    let mut vals1: Vec<f64> = values.iter().map(|(v, _, _)| *v).collect();
    let mut vals2: Vec<f64> = values.iter().map(|(_, v, _)| *v).collect();
    vals1.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    vals2.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let percentiles = [30, 50, 70];
    let mut best_accuracy = 0.0f64;
    let mut best_t1 = 0.0f64;
    let mut best_t2 = 0.0f64;
    let mut best_a1 = true;
    let mut best_a2 = true;

    for &p1 in &percentiles {
        for &p2 in &percentiles {
            let t1 = vals1[(n * p1 as usize) / 100];
            let t2 = vals2[(n * p2 as usize) / 100];

            for &above1 in &[true, false] {
                for &above2 in &[true, false] {
                    let correct = values
                        .iter()
                        .filter(|&&(v1, v2, is_up)| {
                            let pred1 = if above1 { v1 > t1 } else { v1 < t1 };
                            let pred2 = if above2 { v2 > t2 } else { v2 < t2 };
                            let predicted = pred1 && pred2;
                            let actual = if predict_up { is_up } else { !is_up };
                            // Count where prediction matches actual or no prediction
                            if predicted { actual } else { !actual }
                        })
                        .count();

                    let acc = correct as f64 / n as f64;
                    if acc > best_accuracy {
                        best_accuracy = acc;
                        best_t1 = t1;
                        best_t2 = t2;
                        best_a1 = above1;
                        best_a2 = above2;
                    }
                }
            }
        }
    }

    if best_accuracy <= 0.58 || n < 100 {
        return None;
    }

    // Stability check
    let mid = n / 2;
    let first_half: Vec<_> = values[..mid].to_vec();
    let second_half: Vec<_> = values[mid..].to_vec();

    let acc_first = first_half
        .iter()
        .filter(|&&(v1, v2, is_up)| {
            let pred = (if best_a1 { v1 > best_t1 } else { v1 < best_t1 })
                && (if best_a2 { v2 > best_t2 } else { v2 < best_t2 });
            let actual = if predict_up { is_up } else { !is_up };
            if pred { actual } else { !actual }
        })
        .count() as f64
        / first_half.len() as f64;

    let acc_second = second_half
        .iter()
        .filter(|&&(v1, v2, is_up)| {
            let pred = (if best_a1 { v1 > best_t1 } else { v1 < best_t1 })
                && (if best_a2 { v2 > best_t2 } else { v2 < best_t2 });
            let actual = if predict_up { is_up } else { !is_up };
            if pred { actual } else { !actual }
        })
        .count() as f64
        / second_half.len() as f64;

    let stability = 1.0 - (acc_first - acc_second).abs();
    if stability < 0.7 {
        return None;
    }

    let ci = confidence_interval_95(best_accuracy, n);

    let op1 = if best_a1 { ">" } else { "<" };
    let op2 = if best_a2 { ">" } else { "<" };
    let name = format!(
        "{}{}{}&{}{}{}@{}s",
        feat1,
        op1,
        format_threshold(best_t1),
        feat2,
        op2,
        format_threshold(best_t2),
        window
    );

    Some(DetectedPattern {
        name,
        pattern_type: "multivariate".to_string(),
        time_window: window,
        direction: direction.to_string(),
        features_used: vec![feat1.to_string(), feat2.to_string()],
        description: format!(
            "{} {} {:.4} AND {} {} {:.4} a {}s → {} (accuracy: {:.1}%)",
            feat1, op1, best_t1, feat2, op2, best_t2, window, direction, best_accuracy * 100.0
        ),
        accuracy: best_accuracy,
        precision: best_accuracy, // Simplified: use accuracy as proxy
        recall: best_accuracy,
        f1_score: best_accuracy,
        sample_size: n,
        confidence_interval: ci,
        stability_score: stability,
    })
}

/// Detect sequence patterns: evolution of features across time windows.
fn detect_sequence_patterns(
    features_by_market: &HashMap<i64, Vec<ObFeatureRecord>>,
) -> Vec<DetectedPattern> {
    let mut patterns = Vec::new();

    // Build sequences: for each market, get (momentum@30, momentum@60, momentum@90, momentum@120, outcome)
    let feature_names = ["momentum", "price_change"];

    for feature_name in &feature_names {
        // Sequence: T30 → T60 → T90 → T120
        let windows = [30i64, 60, 90, 120];
        let sequences: Vec<(Vec<f64>, bool)> = features_by_market
            .values()
            .filter_map(|market_features| {
                let mut seq = Vec::new();
                let mut outcome = None;

                for &w in &windows {
                    if let Some(f) = market_features.iter().find(|f| f.time_window == w) {
                        let val = get_feature_value(f, feature_name)?;
                        seq.push(val);
                        if outcome.is_none() {
                            outcome = f.outcome_is_up.map(|v| v == 1);
                        }
                    } else {
                        return None;
                    }
                }

                Some((seq, outcome?))
            })
            .collect();

        if sequences.len() < 100 {
            continue;
        }

        // Pattern: consistently positive across windows → UP
        for direction in &["UP", "DOWN"] {
            let predict_up = *direction == "UP";

            // "All positive" pattern
            let correct = sequences
                .iter()
                .filter(|(seq, is_up)| {
                    let all_positive = seq.iter().all(|v| *v > 0.0);
                    if predict_up {
                        (all_positive && *is_up) || (!all_positive && !*is_up)
                    } else {
                        (all_positive && !*is_up) || (!all_positive && *is_up)
                    }
                })
                .count();

            let accuracy = correct as f64 / sequences.len() as f64;
            if accuracy > 0.55 {
                let mid = sequences.len() / 2;
                let acc_first = sequences[..mid]
                    .iter()
                    .filter(|(seq, is_up)| {
                        let all_positive = seq.iter().all(|v| *v > 0.0);
                        if predict_up {
                            (all_positive && *is_up) || (!all_positive && !*is_up)
                        } else {
                            (all_positive && !*is_up) || (!all_positive && *is_up)
                        }
                    })
                    .count() as f64
                    / mid as f64;

                let acc_second = sequences[mid..]
                    .iter()
                    .filter(|(seq, is_up)| {
                        let all_positive = seq.iter().all(|v| *v > 0.0);
                        if predict_up {
                            (all_positive && *is_up) || (!all_positive && !*is_up)
                        } else {
                            (all_positive && !*is_up) || (!all_positive && *is_up)
                        }
                    })
                    .count() as f64
                    / (sequences.len() - mid) as f64;

                let stability = 1.0 - (acc_first - acc_second).abs();
                if stability >= 0.7 {
                    let ci = confidence_interval_95(accuracy, sequences.len());
                    patterns.push(DetectedPattern {
                        name: format!("{}_all_positive_T30-T120→{}", feature_name, direction),
                        pattern_type: "sequence".to_string(),
                        time_window: 120,
                        direction: direction.to_string(),
                        features_used: vec![feature_name.to_string()],
                        description: format!(
                            "{} positif a T30, T60, T90, T120 → {} (accuracy: {:.1}%)",
                            feature_name, direction, accuracy * 100.0
                        ),
                        accuracy,
                        precision: accuracy,
                        recall: accuracy,
                        f1_score: accuracy,
                        sample_size: sequences.len(),
                        confidence_interval: ci,
                        stability_score: stability,
                    });
                }
            }

            // "Increasing trend" pattern: each window's value > previous
            let correct_inc = sequences
                .iter()
                .filter(|(seq, is_up)| {
                    let increasing = seq.windows(2).all(|w| w[1] > w[0]);
                    if predict_up {
                        (increasing && *is_up) || (!increasing && !*is_up)
                    } else {
                        (increasing && !*is_up) || (!increasing && *is_up)
                    }
                })
                .count();

            let acc_inc = correct_inc as f64 / sequences.len() as f64;
            if acc_inc > 0.55 {
                let ci = confidence_interval_95(acc_inc, sequences.len());
                let mid = sequences.len() / 2;
                let s1 = sequences[..mid]
                    .iter()
                    .filter(|(seq, is_up)| {
                        let increasing = seq.windows(2).all(|w| w[1] > w[0]);
                        if predict_up {
                            (increasing && *is_up) || (!increasing && !*is_up)
                        } else {
                            (increasing && !*is_up) || (!increasing && *is_up)
                        }
                    })
                    .count() as f64
                    / mid as f64;
                let s2 = sequences[mid..]
                    .iter()
                    .filter(|(seq, is_up)| {
                        let increasing = seq.windows(2).all(|w| w[1] > w[0]);
                        if predict_up {
                            (increasing && *is_up) || (!increasing && !*is_up)
                        } else {
                            (increasing && !*is_up) || (!increasing && *is_up)
                        }
                    })
                    .count() as f64
                    / (sequences.len() - mid) as f64;
                let stability = 1.0 - (s1 - s2).abs();

                if stability >= 0.7 {
                    patterns.push(DetectedPattern {
                        name: format!("{}_increasing_T30-T120→{}", feature_name, direction),
                        pattern_type: "sequence".to_string(),
                        time_window: 120,
                        direction: direction.to_string(),
                        features_used: vec![feature_name.to_string()],
                        description: format!(
                            "{} croissant de T30 a T120 → {} (accuracy: {:.1}%)",
                            feature_name, direction, acc_inc * 100.0
                        ),
                        accuracy: acc_inc,
                        precision: acc_inc,
                        recall: acc_inc,
                        f1_score: acc_inc,
                        sample_size: sequences.len(),
                        confidence_interval: ci,
                        stability_score: stability,
                    });
                }
            }
        }
    }

    patterns
}

/// Wilson score interval for binomial proportions (95% CI).
pub fn confidence_interval_95(accuracy: f64, n: usize) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }

    let z = 1.96; // 95% confidence
    let p = accuracy;
    let n_f = n as f64;

    let denominator = 1.0 + z * z / n_f;
    let center = (p + z * z / (2.0 * n_f)) / denominator;
    let margin = z * ((p * (1.0 - p) / n_f + z * z / (4.0 * n_f * n_f)).sqrt()) / denominator;

    let low = (center - margin).max(0.0);
    let high = (center + margin).min(1.0);

    (low, high)
}

fn pattern_to_record(p: &DetectedPattern, run_id: &str) -> ObPatternRecord {
    ObPatternRecord {
        id: None,
        pattern_name: p.name.clone(),
        pattern_type: p.pattern_type.clone(),
        time_window: p.time_window,
        direction: p.direction.clone(),
        features_used: serde_json::to_string(&p.features_used).unwrap_or_default(),
        threshold_json: serde_json::json!({
            "description": &p.description,
        })
        .to_string(),
        accuracy: p.accuracy,
        precision_pct: Some(p.precision),
        recall_pct: Some(p.recall),
        f1_score: Some(p.f1_score),
        sample_size: p.sample_size as i64,
        up_count: if p.direction == "UP" { p.sample_size as i64 } else { 0 },
        down_count: if p.direction == "DOWN" { p.sample_size as i64 } else { 0 },
        confidence_95_low: Some(p.confidence_interval.0),
        confidence_95_high: Some(p.confidence_interval.1),
        first_half_accuracy: None,
        second_half_accuracy: None,
        stability_score: Some(p.stability_score),
        analysis_run_id: Some(run_id.to_string()),
        created_at: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use persistence::repository::orderbook::ObPriceRecord;

    fn make_prices(data: &[(f64, f64)]) -> Vec<ObPriceRecord> {
        data.iter()
            .enumerate()
            .map(|(_i, (elapsed, price))| ObPriceRecord {
                id: None,
                market_id: 1,
                timestamp_ms: (1000.0 * elapsed) as i64,
                elapsed_seconds: *elapsed,
                price: *price,
                side: None,
                size: None,
            })
            .collect()
    }

    fn make_prices_with_volume(
        data: &[(f64, f64, &str, f64)],
    ) -> Vec<ObPriceRecord> {
        data.iter()
            .map(|(elapsed, price, side, size)| ObPriceRecord {
                id: None,
                market_id: 1,
                timestamp_ms: (1000.0 * elapsed) as i64,
                elapsed_seconds: *elapsed,
                price: *price,
                side: Some(side.to_string()),
                size: Some(*size),
            })
            .collect()
    }

    #[test]
    fn test_extract_features_basic() {
        let prices = make_prices(&[
            (0.0, 0.50),
            (10.0, 0.52),
            (20.0, 0.54),
            (30.0, 0.55),
            (60.0, 0.58),
            (90.0, 0.60),
            (120.0, 0.62),
        ]);

        let features = extract_features_for_market(1, &prices, true);
        // Should have features for windows 30, 60, 90, 120 (not 180, 300 as max elapsed is 120)
        assert!(features.len() >= 4);

        // Check 30s window
        let f30 = features.iter().find(|f| f.time_window == 30).unwrap();
        assert!(f30.last_price.unwrap() > 0.0);
        assert!(f30.price_change.unwrap() > 0.0); // Price went up
        assert!(f30.momentum.unwrap() > 0.0); // Upward slope
        assert_eq!(f30.outcome_is_up, Some(1));
    }

    #[test]
    fn test_extract_features_empty_window() {
        let prices = make_prices(&[
            (100.0, 0.50),
            (110.0, 0.52),
        ]);

        let features = extract_features_for_market(1, &prices, false);
        // No prices in T30 or T60, should only have features for T120+
        assert!(features.iter().all(|f| f.time_window >= 120));
    }

    #[test]
    fn test_extract_features_single_point() {
        let prices = make_prices(&[(15.0, 0.55)]);

        let features = extract_features_for_market(1, &prices, true);
        let f30 = features.iter().find(|f| f.time_window == 30).unwrap();
        assert_eq!(f30.data_points, Some(1));
        assert_eq!(f30.price_change.unwrap(), 0.0); // Only one point
        assert_eq!(f30.momentum.unwrap(), 0.0);     // Can't compute slope with 1 point
    }

    #[test]
    fn test_compute_momentum_upward() {
        let points = vec![
            (0.0, 0.50),
            (10.0, 0.52),
            (20.0, 0.54),
            (30.0, 0.56),
        ];
        let slope = compute_linear_regression_slope(&points);
        assert!(slope > 0.0, "Upward trend should have positive slope");
        // Expected: ~0.002 per second
        assert!((slope - 0.002).abs() < 0.001);
    }

    #[test]
    fn test_compute_momentum_flat() {
        let points = vec![
            (0.0, 0.50),
            (10.0, 0.50),
            (20.0, 0.50),
        ];
        let slope = compute_linear_regression_slope(&points);
        assert!(slope.abs() < 1e-10, "Flat prices should have zero slope");
    }

    #[test]
    fn test_compute_vwap() {
        let prices = make_prices_with_volume(&[
            (10.0, 0.50, "BUY", 100.0),
            (20.0, 0.60, "BUY", 200.0),
        ]);
        let refs: Vec<&ObPriceRecord> = prices.iter().collect();
        let vwap = compute_vwap(&refs);
        // VWAP = (0.50*100 + 0.60*200) / (100+200) = 170/300 ≈ 0.5667
        assert!((vwap - 0.5667).abs() < 0.001);
    }

    #[test]
    fn test_univariate_pattern_detection() {
        // Create synthetic features where momentum > 0 predicts UP
        let mut features = Vec::new();
        for i in 0..200 {
            let is_up = i % 3 != 0; // 2/3 are UP
            let momentum = if is_up { 0.01 + (i as f64) * 0.0001 } else { -0.01 - (i as f64) * 0.0001 };
            features.push(ObFeatureRecord {
                id: None,
                market_id: i as i64,
                time_window: 90,
                last_price: Some(0.5),
                vwap: Some(0.5),
                price_change: Some(if is_up { 0.05 } else { -0.05 }),
                price_volatility: Some(0.01),
                momentum: Some(momentum),
                max_price: Some(0.55),
                min_price: Some(0.45),
                price_range: Some(0.10),
                data_points: Some(50),
                buy_volume: None,
                sell_volume: None,
                volume_imbalance: None,
                trade_count: None,
                avg_trade_size: None,
                large_trade_ratio: None,
                avg_spread: None,
                depth_imbalance: None,
                avg_bid_depth: None,
                avg_ask_depth: None,
                outcome_is_up: Some(if is_up { 1 } else { 0 }),
            });
        }

        let patterns = detect_univariate_patterns(&features, 90);
        // Should detect at least one pattern based on momentum or price_change
        assert!(!patterns.is_empty(), "Should detect patterns from synthetic data");
    }

    #[test]
    fn test_confidence_interval_95() {
        let (low, high) = confidence_interval_95(0.60, 1000);
        assert!(low > 0.55);
        assert!(high < 0.65);
        assert!(low < 0.60);
        assert!(high > 0.60);

        // Edge case: n=0
        let (low, high) = confidence_interval_95(0.5, 0);
        assert_eq!(low, 0.0);
        assert_eq!(high, 1.0);
    }

    #[test]
    fn test_stability_score() {
        // With identical halves, stability should be 1.0
        let values: Vec<(f64, bool)> = (0..200)
            .map(|i| {
                let is_up = i % 2 == 0;
                let val = if is_up { 0.05 } else { -0.05 };
                (val, is_up)
            })
            .collect();

        // Compute accuracy on each half
        let threshold = 0.0;
        let (_acc_full, _, _, _) = compute_classification_metrics(&values, threshold, true, true);
        let mid = values.len() / 2;
        let (acc_first, _, _, _) = compute_classification_metrics(&values[..mid], threshold, true, true);
        let (acc_second, _, _, _) = compute_classification_metrics(&values[mid..], threshold, true, true);

        let stability = 1.0 - (acc_first - acc_second).abs();
        assert!(stability > 0.9, "Symmetric data should have high stability: {}", stability);
    }

    #[test]
    fn test_progress_new_is_idle() {
        let progress = ObBacktestProgress::new();
        assert_eq!(*progress.status.read().unwrap(), ObBacktestStatus::Idle);
        assert!(!progress.is_running());
    }

    #[test]
    fn test_parse_market_outcome() {
        let (outcome, p_up, p_down) = parse_market_outcome(Some("[0.95,0.05]"));
        assert_eq!(outcome, Some("Up".to_string()));
        assert!((p_up.unwrap() - 0.95).abs() < 0.01);
        assert!((p_down.unwrap() - 0.05).abs() < 0.01);

        let (outcome, _, _) = parse_market_outcome(Some("[0.1,0.9]"));
        assert_eq!(outcome, Some("Down".to_string()));

        let (outcome, _, _) = parse_market_outcome(None);
        assert_eq!(outcome, None);

        // String prices
        let (outcome, p_up, _) = parse_market_outcome(Some("[\"0.85\",\"0.15\"]"));
        assert_eq!(outcome, Some("Up".to_string()));
        assert!((p_up.unwrap() - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_multivariate_pattern_detection() {
        // Create features where momentum > 0 AND price_change > 0 correlate with UP
        let mut features = Vec::new();
        for i in 0..300 {
            let is_up = i % 4 != 0; // 75% UP
            let momentum = if is_up { 0.01 } else { -0.01 };
            let pc = if is_up { 0.05 } else { -0.05 };
            features.push(ObFeatureRecord {
                id: None,
                market_id: i as i64,
                time_window: 90,
                last_price: Some(0.5),
                vwap: Some(0.5),
                price_change: Some(pc),
                price_volatility: Some(0.01),
                momentum: Some(momentum),
                max_price: Some(0.55),
                min_price: Some(0.45),
                price_range: Some(0.10),
                data_points: Some(50),
                buy_volume: None, sell_volume: None, volume_imbalance: None,
                trade_count: None, avg_trade_size: None, large_trade_ratio: None,
                avg_spread: None, depth_imbalance: None, avg_bid_depth: None, avg_ask_depth: None,
                outcome_is_up: Some(if is_up { 1 } else { 0 }),
            });
        }

        let univariate = detect_univariate_patterns(&features, 90);
        let multivariate = detect_multivariate_patterns(&features, &univariate, 90);
        // Multivariate may or may not find patterns depending on thresholds
        // but the function should run without errors
        let _total = univariate.len() + multivariate.len(); // At least no crash
    }

    #[test]
    fn test_sequence_pattern_detection() {
        let mut features_by_market: HashMap<i64, Vec<ObFeatureRecord>> = HashMap::new();

        for i in 0..200 {
            let is_up = i % 3 != 0;
            let base = if is_up { 0.01 } else { -0.01 };
            let mut market_features = Vec::new();
            for &w in &[30i64, 60, 90, 120] {
                market_features.push(ObFeatureRecord {
                    id: None,
                    market_id: i,
                    time_window: w,
                    last_price: Some(0.5),
                    vwap: Some(0.5),
                    price_change: Some(base * (w as f64 / 30.0)),
                    price_volatility: Some(0.01),
                    momentum: Some(base),
                    max_price: Some(0.55),
                    min_price: Some(0.45),
                    price_range: Some(0.10),
                    data_points: Some(10),
                    buy_volume: None, sell_volume: None, volume_imbalance: None,
                    trade_count: None, avg_trade_size: None, large_trade_ratio: None,
                    avg_spread: None, depth_imbalance: None, avg_bid_depth: None, avg_ask_depth: None,
                    outcome_is_up: Some(if is_up { 1 } else { 0 }),
                });
            }
            features_by_market.insert(i, market_features);
        }

        let patterns = detect_sequence_patterns(&features_by_market);
        // Should detect some sequence patterns
        // The function should run without errors regardless
        let _ = patterns.len();
    }
}
