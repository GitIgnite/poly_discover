//! Polymarket Data API client — public endpoints, no authentication required
//!
//! Uses `data-api.polymarket.com` for leaderboard, positions, trades, and portfolio value.
//! Uses `gamma-api.polymarket.com` for market/event metadata.

use anyhow::Result;
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::debug;

/// Deserialize a price that can be either a JSON number (0.505) or a string ("0.505").
fn deserialize_price<'de, D>(deserializer: D) -> std::result::Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct PriceVisitor;

    impl<'de> de::Visitor<'de> for PriceVisitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a number or string representing a price")
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<f64, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<f64, E> {
            Ok(v as f64)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<f64, E> {
            Ok(v as f64)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<f64, E> {
            v.parse::<f64>().map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_any(PriceVisitor)
}

const BASE_URL: &str = "https://data-api.polymarket.com";
const GAMMA_URL: &str = "https://gamma-api.polymarket.com";
const CLOB_URL: &str = "https://clob.polymarket.com";

/// Rate-limit delay between paginated requests (ms)
const PAGINATION_DELAY_MS: u64 = 200;

/// Polymarket Data API client
#[derive(Clone)]
pub struct PolymarketDataClient {
    client: Client,
}

// ---------------------------------------------------------------------------
// Deserialization structs — Data API
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardEntry {
    pub rank: Option<String>,
    pub proxy_wallet: Option<String>,
    pub user_name: Option<String>,
    pub vol: Option<f64>,
    pub pnl: Option<f64>,
    pub profile_image: Option<String>,
    pub x_username: Option<String>,
    pub verified_badge: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderPosition {
    pub proxy_wallet: Option<String>,
    pub asset: Option<String>,
    pub condition_id: Option<String>,
    pub size: Option<f64>,
    pub avg_price: Option<f64>,
    pub initial_value: Option<f64>,
    pub current_value: Option<f64>,
    pub cash_pnl: Option<f64>,
    pub percent_pnl: Option<f64>,
    pub total_bought: Option<f64>,
    pub realized_pnl: Option<f64>,
    pub percent_realized_pnl: Option<f64>,
    pub cur_price: Option<f64>,
    pub redeemable: Option<bool>,
    pub mergeable: Option<bool>,
    pub title: Option<String>,
    pub slug: Option<String>,
    pub icon: Option<String>,
    pub event_slug: Option<String>,
    pub outcome: Option<String>,
    pub outcome_index: Option<f64>,
    pub opposite_outcome: Option<String>,
    pub opposite_asset: Option<String>,
    pub end_date: Option<String>,
    pub negative_risk: Option<bool>,
    pub resolving: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderTrade {
    pub proxy_wallet: Option<String>,
    pub side: Option<String>,
    pub asset: Option<String>,
    pub condition_id: Option<String>,
    pub size: Option<f64>,
    pub price: Option<f64>,
    pub timestamp: Option<f64>,
    pub title: Option<String>,
    pub slug: Option<String>,
    pub event_slug: Option<String>,
    pub outcome: Option<String>,
    pub outcome_index: Option<f64>,
    pub transaction_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderValue {
    pub user: Option<String>,
    pub value: Option<f64>,
}

/// Closed/resolved position
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedPosition {
    pub proxy_wallet: Option<String>,
    pub asset: Option<String>,
    pub condition_id: Option<String>,
    pub avg_price: Option<f64>,
    pub total_bought: Option<f64>,
    pub realized_pnl: Option<f64>,
    pub cur_price: Option<f64>,
    pub timestamp: Option<f64>,
    pub title: Option<String>,
    pub slug: Option<String>,
    pub icon: Option<String>,
    pub event_slug: Option<String>,
    pub outcome: Option<String>,
    pub outcome_index: Option<f64>,
    pub opposite_outcome: Option<String>,
    pub opposite_asset: Option<String>,
    pub end_date: Option<String>,
}

/// User activity record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserActivity {
    pub proxy_wallet: Option<String>,
    pub timestamp: Option<f64>,
    pub condition_id: Option<String>,
    #[serde(rename = "type")]
    pub activity_type: Option<String>,
    pub size: Option<f64>,
    pub usdc_size: Option<f64>,
    pub transaction_hash: Option<String>,
    pub price: Option<f64>,
    pub asset: Option<String>,
    pub side: Option<String>,
    pub outcome_index: Option<f64>,
    pub title: Option<String>,
    pub slug: Option<String>,
    pub icon: Option<String>,
    pub event_slug: Option<String>,
    pub outcome: Option<String>,
    pub name: Option<String>,
    pub pseudonym: Option<String>,
}

// ---------------------------------------------------------------------------
// Deserialization structs — Gamma API (market/event metadata)
// ---------------------------------------------------------------------------

/// Market metadata from Gamma API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GammaMarket {
    pub id: Option<String>,
    pub condition_id: Option<String>,
    pub question: Option<String>,
    pub slug: Option<String>,
    pub end_date: Option<String>,
    pub category: Option<String>,
    pub closed: Option<bool>,
    pub active: Option<bool>,
    pub liquidity: Option<String>,
    pub volume: Option<String>,
    pub outcomes: Option<String>,        // JSON string like "[\"Yes\",\"No\"]"
    pub outcome_prices: Option<String>,  // JSON string like "[0.65,0.35]"
    pub event_slug: Option<String>,
    pub description: Option<String>,
    pub clob_token_ids: Option<String>, // JSON string like "[\"token1\", \"token2\"]"
}

/// Event metadata from Gamma API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GammaEvent {
    pub id: Option<String>,
    pub title: Option<String>,
    pub slug: Option<String>,
    pub category: Option<String>,
    pub closed: Option<bool>,
    pub active: Option<bool>,
    pub volume: Option<f64>,
    pub liquidity: Option<f64>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

// ---------------------------------------------------------------------------
// Deserialization structs — CLOB API (orderbook, prices, trades)
// ---------------------------------------------------------------------------

/// Price history point from CLOB prices-history endpoint.
/// The `p` field can be a number (0.505) or a string ("0.505") depending on the API version.
#[derive(Debug, Clone, Deserialize)]
pub struct PriceHistoryPoint {
    pub t: i64,
    #[serde(deserialize_with = "deserialize_price")]
    pub p: f64,
}

/// A single trade from the CLOB trades endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct MarketTrade {
    pub price: String,
    pub size: String,
    pub side: String,
    pub timestamp: String,
}

/// Orderbook snapshot from CLOB book endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct OrderbookSnapshot {
    pub market: Option<String>,
    pub asset_id: Option<String>,
    pub bids: Option<Vec<OrderbookLevel>>,
    pub asks: Option<Vec<OrderbookLevel>>,
}

/// Single level in the orderbook
#[derive(Debug, Clone, Deserialize)]
pub struct OrderbookLevel {
    pub price: String,
    pub size: String,
}

/// Which data source is available for historical market data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DataSource {
    PricesHistory,
    ClobTrades,
    DataApiTrades,
    None,
}

impl std::fmt::Display for DataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSource::PricesHistory => write!(f, "PricesHistory"),
            DataSource::ClobTrades => write!(f, "ClobTrades"),
            DataSource::DataApiTrades => write!(f, "DataApiTrades"),
            DataSource::None => write!(f, "None"),
        }
    }
}

// ---------------------------------------------------------------------------
// Client implementation
// ---------------------------------------------------------------------------

impl Default for PolymarketDataClient {
    fn default() -> Self {
        Self::new()
    }
}

impl PolymarketDataClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    // =======================================================================
    // Generic paginated fetch
    // =======================================================================

    /// Fetch all pages from a paginated endpoint.
    /// Returns the concatenated results from all pages.
    async fn fetch_all_paginated<T: DeserializeOwned>(
        &self,
        base_url: &str,
        max_limit: u32,
    ) -> Result<Vec<T>> {
        let mut all_items: Vec<T> = Vec::new();
        let mut offset: u32 = 0;

        loop {
            let separator = if base_url.contains('?') { '&' } else { '?' };
            let url = format!("{base_url}{separator}limit={max_limit}&offset={offset}");
            debug!("Paginated fetch: {}", url);

            let resp = self.client.get(&url).send().await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("Polymarket paginated fetch error {}: {}", status, body);
            }

            let page: Vec<T> = resp.json().await?;
            let page_len = page.len() as u32;
            all_items.extend(page);

            if page_len < max_limit {
                break; // Last page
            }

            offset += max_limit;

            // Rate-limit delay
            tokio::time::sleep(std::time::Duration::from_millis(PAGINATION_DELAY_MS)).await;
        }

        debug!(total = all_items.len(), "Paginated fetch complete");
        Ok(all_items)
    }

    // =======================================================================
    // Username resolution
    // =======================================================================

    /// Resolve a Polymarket username to a (proxyWallet, userName) pair.
    /// Uses the leaderboard endpoint with userName filter.
    pub async fn resolve_username(&self, username: &str) -> Result<(String, String)> {
        let url = format!(
            "{}/v1/leaderboard?userName={}&limit=1",
            BASE_URL, username
        );
        debug!(username, "Resolving username to wallet");

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Polymarket username resolution error {}: {}", status, body);
        }

        let entries: Vec<LeaderboardEntry> = resp.json().await?;
        let entry = entries
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("User not found: {}", username))?;

        let wallet = entry
            .proxy_wallet
            .ok_or_else(|| anyhow::anyhow!("User '{}' has no proxy wallet", username))?;
        let name = entry.user_name.unwrap_or_else(|| username.to_string());

        debug!(wallet = %wallet, name = %name, "Username resolved");
        Ok((wallet, name))
    }

    // =======================================================================
    // Leaderboard
    // =======================================================================

    /// GET /v1/leaderboard — top traders
    pub async fn get_leaderboard(&self, limit: u32) -> Result<Vec<LeaderboardEntry>> {
        let url = format!(
            "{}/v1/leaderboard?category=OVERALL&timePeriod=ALL&orderBy=PNL&limit={}",
            BASE_URL, limit
        );
        debug!("Fetching leaderboard: {}", url);

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Polymarket leaderboard error {}: {}", status, body);
        }

        let entries: Vec<LeaderboardEntry> = resp.json().await?;
        debug!(count = entries.len(), "Leaderboard fetched");
        Ok(entries)
    }

    // =======================================================================
    // Positions (open)
    // =======================================================================

    /// GET /positions?user={address} — trader positions (single page, legacy)
    pub async fn get_positions(&self, address: &str) -> Result<Vec<TraderPosition>> {
        let url = format!(
            "{}/positions?user={}&sortBy=CASHPNL&sortDirection=DESC&limit=100&sizeThreshold=0",
            BASE_URL, address
        );
        debug!(address, "Fetching positions");

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Polymarket positions error {}: {}", status, body);
        }

        let positions: Vec<TraderPosition> = resp.json().await?;
        debug!(count = positions.len(), "Positions fetched");
        Ok(positions)
    }

    /// Fetch ALL open positions (paginated, limit=500 per page)
    pub async fn get_all_positions(&self, address: &str) -> Result<Vec<TraderPosition>> {
        let url = format!(
            "{}/positions?user={}&sortBy=CASHPNL&sortDirection=DESC&sizeThreshold=0",
            BASE_URL, address
        );
        self.fetch_all_paginated(&url, 500).await
    }

    // =======================================================================
    // Closed positions
    // =======================================================================

    /// Fetch ALL closed/resolved positions (paginated, limit=50 per page)
    pub async fn get_all_closed_positions(&self, address: &str) -> Result<Vec<ClosedPosition>> {
        let url = format!(
            "{}/closed-positions?user={}&sortBy=TIMESTAMP&sortDirection=DESC",
            BASE_URL, address
        );
        self.fetch_all_paginated(&url, 50).await
    }

    // =======================================================================
    // Trades
    // =======================================================================

    /// GET /trades?user={address} — trader trade history (single page, legacy)
    pub async fn get_trades(&self, address: &str) -> Result<Vec<TraderTrade>> {
        let url = format!(
            "{}/trades?user={}&limit=500&takerOnly=false",
            BASE_URL, address
        );
        debug!(address, "Fetching trades");

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Polymarket trades error {}: {}", status, body);
        }

        let trades: Vec<TraderTrade> = resp.json().await?;
        debug!(count = trades.len(), "Trades fetched");
        Ok(trades)
    }

    /// Fetch ALL trades (paginated, limit=10000 per page)
    pub async fn get_all_trades(&self, address: &str) -> Result<Vec<TraderTrade>> {
        let url = format!(
            "{}/trades?user={}&takerOnly=false",
            BASE_URL, address
        );
        self.fetch_all_paginated(&url, 10000).await
    }

    // =======================================================================
    // Activity
    // =======================================================================

    /// Fetch ALL activity records (paginated, limit=500 per page)
    pub async fn get_all_activity(&self, address: &str) -> Result<Vec<UserActivity>> {
        let url = format!(
            "{}/activity?user={}&sortBy=TIMESTAMP&sortDirection=DESC",
            BASE_URL, address
        );
        self.fetch_all_paginated(&url, 500).await
    }

    // =======================================================================
    // Portfolio value
    // =======================================================================

    /// GET /value?user={address} — portfolio value
    pub async fn get_value(&self, address: &str) -> Result<TraderValue> {
        let url = format!("{}/value?user={}", BASE_URL, address);
        debug!(address, "Fetching portfolio value");

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Polymarket value error {}: {}", status, body);
        }

        let values: Vec<TraderValue> = resp.json().await?;
        Ok(values.into_iter().next().unwrap_or(TraderValue { user: None, value: None }))
    }

    // =======================================================================
    // Gamma API — Market metadata
    // =======================================================================

    /// Fetch market metadata for a batch of condition IDs.
    /// The Gamma API accepts comma-separated condition_ids.
    pub async fn get_markets_by_condition_ids(
        &self,
        condition_ids: &[String],
    ) -> Result<Vec<GammaMarket>> {
        if condition_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_markets = Vec::new();

        // Batch in groups of 50 to avoid URL length limits
        for chunk in condition_ids.chunks(50) {
            let ids = chunk.join(",");
            let url = format!(
                "{}/markets?condition_ids={}&limit=100",
                GAMMA_URL, ids
            );
            debug!(count = chunk.len(), "Fetching market metadata from Gamma");

            let resp = self.client.get(&url).send().await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                debug!("Gamma API error {}: {} (non-fatal)", status, body);
                continue; // Non-fatal: we can still work without metadata
            }

            let markets: Vec<GammaMarket> = resp.json().await.unwrap_or_default();
            all_markets.extend(markets);

            if condition_ids.chunks(50).count() > 1 {
                tokio::time::sleep(std::time::Duration::from_millis(PAGINATION_DELAY_MS)).await;
            }
        }

        debug!(total = all_markets.len(), "Market metadata fetched");
        Ok(all_markets)
    }

    /// Fetch event metadata by slug
    pub async fn get_event_by_slug(&self, slug: &str) -> Result<Option<GammaEvent>> {
        let url = format!("{}/events?slug={}&limit=1", GAMMA_URL, slug);
        debug!(slug, "Fetching event metadata from Gamma");

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(None); // Non-fatal
        }

        let events: Vec<GammaEvent> = resp.json().await.unwrap_or_default();
        Ok(events.into_iter().next())
    }

    // =======================================================================
    // CLOB API — Orderbook Backtest endpoints
    // =======================================================================

    /// Probe the best available data source by testing all 3 on a known market.
    /// `test_token_id` is optional — if provided, also tries prices-history with token_id.
    pub async fn probe_best_data_source(
        &self,
        test_condition_id: &str,
        test_token_id: Option<&str>,
    ) -> DataSource {
        use tracing::{info, warn};

        // Try prices-history with condition_id first (per official docs)
        let url = format!(
            "{}/prices-history?market={}&interval=max&fidelity=5",
            CLOB_URL, test_condition_id
        );
        info!("Probing prices-history (condition_id): {}", url);
        match self.client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                info!("prices-history response status: {}", status);
                if status.is_success() {
                    if let Ok(body) = resp.text().await {
                        let preview = &body[..body.len().min(300)];
                        info!("prices-history body preview ({}B): {}", body.len(), preview);
                        if body.len() > 10 && (body.contains("\"t\"") || body.contains("\"p\"")) {
                            info!("prices-history available (condition_id)");
                            return DataSource::PricesHistory;
                        }
                        if body.contains("history") && body.len() > 20 {
                            info!("prices-history available (history field)");
                            return DataSource::PricesHistory;
                        }
                    }
                }
            }
            Err(e) => warn!("prices-history request failed: {}", e),
        }

        // Try prices-history with token_id as fallback
        if let Some(token_id) = test_token_id {
            let url = format!(
                "{}/prices-history?market={}&interval=max&fidelity=5",
                CLOB_URL, token_id
            );
            info!("Probing prices-history (token_id): {}", url);
            match self.client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    info!("prices-history (token_id) response status: {}", status);
                    if status.is_success() {
                        if let Ok(body) = resp.text().await {
                            let preview = &body[..body.len().min(300)];
                            info!("prices-history (token_id) body preview ({}B): {}", body.len(), preview);
                            if body.len() > 10 && (body.contains("\"t\"") || body.contains("\"p\"")) {
                                info!("prices-history available (token_id)");
                                return DataSource::PricesHistory;
                            }
                            if body.contains("history") && body.len() > 20 {
                                info!("prices-history available (token_id, history field)");
                                return DataSource::PricesHistory;
                            }
                        }
                    }
                }
                Err(e) => warn!("prices-history (token_id) request failed: {}", e),
            }
        }

        // Try CLOB trades
        let url = format!("{}/trades?market={}", CLOB_URL, test_condition_id);
        info!("Probing CLOB trades: {}", url);
        match self.client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                info!("CLOB trades response status: {}", status);
                if status.is_success() {
                    if let Ok(body) = resp.text().await {
                        let preview = &body[..body.len().min(300)];
                        info!("CLOB trades body preview ({}B): {}", body.len(), preview);
                        if let Ok(trades) = serde_json::from_str::<Vec<MarketTrade>>(&body) {
                            if !trades.is_empty() {
                                info!(count = trades.len(), "CLOB trades available");
                                return DataSource::ClobTrades;
                            }
                        }
                    }
                }
            }
            Err(e) => warn!("CLOB trades request failed: {}", e),
        }

        // Try Data API trades (public, by market)
        let url = format!("{}/trades?market={}", BASE_URL, test_condition_id);
        info!("Probing Data API trades: {}", url);
        match self.client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                info!("Data API trades response status: {}", status);
                if status.is_success() {
                    if let Ok(body) = resp.text().await {
                        let preview = &body[..body.len().min(300)];
                        info!("Data API trades body preview ({}B): {}", body.len(), preview);
                        if let Ok(trades) = serde_json::from_str::<Vec<TraderTrade>>(&body) {
                            if !trades.is_empty() {
                                info!(count = trades.len(), "Data API trades available");
                                return DataSource::DataApiTrades;
                            }
                        }
                    }
                }
            }
            Err(e) => warn!("Data API trades request failed: {}", e),
        }

        warn!("No data source available for condition_id={} token_id={:?}", test_condition_id, test_token_id);
        DataSource::None
    }

    /// Search for BTC 15-minute markets via Gamma API (single page, closed only, default order).
    pub async fn search_btc_15min_markets(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<GammaMarket>> {
        self.search_markets(offset, limit, Some(true), false).await
    }

    /// Generic market search via Gamma API.
    /// - `closed`: `Some(true)` = closed only, `Some(false)` = active only, `None` = all.
    /// - `newest_first`: sort by id descending.
    pub async fn search_markets(
        &self,
        offset: u32,
        limit: u32,
        closed: Option<bool>,
        newest_first: bool,
    ) -> Result<Vec<GammaMarket>> {
        let closed_param = match closed {
            Some(true) => "&closed=true",
            Some(false) => "&closed=false",
            None => "",
        };
        let order_param = if newest_first {
            "&order=id&ascending=false"
        } else {
            ""
        };
        let url = format!(
            "{}/markets?limit={}&offset={}{}{}",
            GAMMA_URL, limit, offset, closed_param, order_param
        );
        debug!("Searching markets: {}", url);

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gamma API search error {}: {}", status, body);
        }

        let markets: Vec<GammaMarket> = resp.json().await.unwrap_or_default();
        Ok(markets)
    }

    /// Auto-paginate closed BTC short-term markets, searching newest-first.
    /// Stops after `max_empty_pages` consecutive pages with no BTC markets,
    /// or after collecting `max_markets` total.
    /// `on_progress` is called after each page with (total_found, offset).
    pub async fn get_all_btc_15min_markets(
        &self,
        cancelled: &std::sync::atomic::AtomicBool,
        on_progress: impl Fn(usize, u32),
    ) -> Result<Vec<GammaMarket>> {
        use tracing::info;

        let mut all_markets = Vec::new();
        let mut offset: u32 = 0;
        let page_limit: u32 = 100;
        let max_markets: usize = 35_000;
        let max_empty_pages: u32 = 50; // stop after 50 consecutive pages with 0 BTC markets
        let mut consecutive_empty: u32 = 0;

        loop {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                info!(total = all_markets.len(), "Discovery cancelled by user");
                break;
            }

            let page = self.search_markets(offset, page_limit, Some(true), true).await?;
            let page_len = page.len() as u32;

            // Client-side filter for BTC short-term markets
            // Questions look like: "Bitcoin Up or Down - February 25, 2:45PM-3:00PM ET"
            let filtered: Vec<GammaMarket> = page
                .into_iter()
                .filter(|m| {
                    if let Some(ref q) = m.question {
                        let q_lower = q.to_lowercase();
                        (q_lower.contains("bitcoin") || q_lower.contains("btc"))
                            && (q_lower.contains("up or down")
                                || q_lower.contains("go up")
                                || q_lower.contains("above")
                                || q_lower.contains("higher"))
                    } else {
                        false
                    }
                })
                .collect();

            let found = filtered.len();
            all_markets.extend(filtered);

            on_progress(all_markets.len(), offset);

            if found == 0 {
                consecutive_empty += 1;
            } else {
                consecutive_empty = 0;
            }

            // Log progress every 20 pages
            if (offset / page_limit) % 20 == 0 {
                info!(
                    offset,
                    total_found = all_markets.len(),
                    page_found = found,
                    consecutive_empty,
                    "Discovering BTC markets..."
                );
            }

            if page_len < page_limit {
                break;
            }

            if all_markets.len() >= max_markets {
                info!(total = all_markets.len(), "Reached max markets limit");
                break;
            }

            if consecutive_empty >= max_empty_pages {
                info!(
                    offset,
                    total = all_markets.len(),
                    "Stopping: {} consecutive empty pages",
                    max_empty_pages
                );
                break;
            }

            offset += page_limit;

            // Rate limit (minimal for faster discovery)
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Hard safety limit
            if offset > 500_000 {
                info!("Reached hard pagination limit at offset {}", offset);
                break;
            }
        }

        info!(total = all_markets.len(), "BTC short-term markets found");
        Ok(all_markets)
    }

    /// Incremental discovery: only fetch NEW BTC 15-min markets since `since_end_time`.
    /// Paginates newest-first and stops early when hitting markets older than the cutoff.
    /// Returns only markets newer than `since_end_time`.
    pub async fn get_new_btc_15min_markets(
        &self,
        since_end_time: i64,
        cancelled: &std::sync::atomic::AtomicBool,
        on_progress: impl Fn(usize, u32),
    ) -> Result<Vec<GammaMarket>> {
        use tracing::info;

        let mut new_markets = Vec::new();
        let mut offset: u32 = 0;
        let page_limit: u32 = 100;
        let max_consecutive_old: u32 = 5; // stop after 5 pages where ALL BTC markets are old
        let mut consecutive_old_pages: u32 = 0;

        info!(
            since_end_time,
            "Incremental discovery: searching for markets newer than cutoff"
        );

        loop {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                info!(total = new_markets.len(), "Incremental discovery cancelled by user");
                break;
            }

            let page = self
                .search_markets(offset, page_limit, Some(true), true)
                .await?;
            let page_len = page.len() as u32;

            // Client-side filter for BTC short-term markets
            let filtered: Vec<GammaMarket> = page
                .into_iter()
                .filter(|m| {
                    if let Some(ref q) = m.question {
                        let q_lower = q.to_lowercase();
                        (q_lower.contains("bitcoin") || q_lower.contains("btc"))
                            && (q_lower.contains("up or down")
                                || q_lower.contains("go up")
                                || q_lower.contains("above")
                                || q_lower.contains("higher"))
                    } else {
                        false
                    }
                })
                .collect();

            if filtered.is_empty() {
                // No BTC markets on this page — may still have newer pages ahead
                // (non-BTC markets mixed in), so just continue
                offset += page_limit;
                if page_len < page_limit {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                // Safety limit
                if offset > 100_000 {
                    break;
                }
                continue;
            }

            // Check how many of these BTC markets are newer than our cutoff
            let mut any_new = false;
            for m in &filtered {
                let end_time = m
                    .end_date
                    .as_ref()
                    .and_then(|d| {
                        chrono::DateTime::parse_from_rfc3339(d)
                            .ok()
                            .or_else(|| {
                                chrono::DateTime::parse_from_str(d, "%Y-%m-%dT%H:%M:%S%.fZ").ok()
                            })
                            .map(|dt| dt.timestamp())
                    })
                    .unwrap_or(0);

                if end_time > since_end_time {
                    any_new = true;
                }
            }

            // Keep ALL from this page (INSERT OR IGNORE handles duplicates in DB)
            new_markets.extend(filtered);
            on_progress(new_markets.len(), offset);

            if any_new {
                consecutive_old_pages = 0;
            } else {
                consecutive_old_pages += 1;
            }

            if consecutive_old_pages >= max_consecutive_old {
                info!(
                    offset,
                    total = new_markets.len(),
                    "Incremental discovery: {} consecutive pages with only old markets, stopping",
                    max_consecutive_old
                );
                break;
            }

            if page_len < page_limit {
                break;
            }

            offset += page_limit;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Safety limit
            if offset > 100_000 {
                info!("Incremental discovery: reached safety pagination limit");
                break;
            }
        }

        info!(
            total = new_markets.len(),
            "Incremental discovery complete"
        );
        Ok(new_markets)
    }

    /// CLOB: Get price history for a market. `market_id` should be a token_id.
    pub async fn get_prices_history(
        &self,
        market_id: &str,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<PriceHistoryPoint>> {
        let url = format!(
            "{}/prices-history?market={}&startTs={}&endTs={}&interval=max&fidelity=5",
            CLOB_URL, market_id, start_ts, end_ts
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("CLOB prices-history error {}: {}", status, body);
        }

        // The response may be a raw array or a {history: [...]} wrapper
        let body = resp.text().await?;

        // Try parsing as {history: [...]} first
        #[derive(Deserialize)]
        struct HistoryWrapper {
            history: Vec<PriceHistoryPoint>,
        }
        if let Ok(wrapper) = serde_json::from_str::<HistoryWrapper>(&body) {
            return Ok(wrapper.history);
        }

        // Try raw array
        if let Ok(points) = serde_json::from_str::<Vec<PriceHistoryPoint>>(&body) {
            return Ok(points);
        }

        Ok(Vec::new())
    }

    /// CLOB: Try to get trades for a market (may fail if auth is required).
    pub async fn try_get_market_trades(&self, condition_id: &str) -> Result<Vec<MarketTrade>> {
        let url = format!("{}/trades?market={}", CLOB_URL, condition_id);

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let trades: Vec<MarketTrade> = resp.json().await.unwrap_or_default();
        Ok(trades)
    }

    /// Data API: Try to get trades by market (without user param).
    pub async fn try_get_data_api_market_trades(
        &self,
        condition_id: &str,
    ) -> Result<Vec<TraderTrade>> {
        let url = format!("{}/trades?market={}&limit=10000", BASE_URL, condition_id);

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let trades: Vec<TraderTrade> = resp.json().await.unwrap_or_default();
        Ok(trades)
    }

    /// CLOB: Get live orderbook for a token.
    pub async fn get_orderbook(&self, token_id: &str) -> Result<OrderbookSnapshot> {
        let url = format!("{}/book?token_id={}", CLOB_URL, token_id);

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("CLOB orderbook error {}: {}", status, body);
        }

        let snapshot: OrderbookSnapshot = resp.json().await?;
        Ok(snapshot)
    }

    /// Search for currently active BTC 15-min market (not closed).
    pub async fn get_active_btc_15min_market(&self) -> Result<Option<GammaMarket>> {
        let url = format!(
            "{}/markets?closed=false&active=true&limit=20",
            GAMMA_URL
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(None);
        }

        let markets: Vec<GammaMarket> = resp.json().await.unwrap_or_default();

        let found = markets.into_iter().find(|m| {
            if let Some(ref q) = m.question {
                let q_lower = q.to_lowercase();
                (q_lower.contains("bitcoin") || q_lower.contains("btc"))
                    && (q_lower.contains("15 min")
                        || q_lower.contains("15-min")
                        || q_lower.contains("15min"))
            } else {
                false
            }
        });

        Ok(found)
    }
}
