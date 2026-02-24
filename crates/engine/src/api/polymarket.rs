//! Polymarket Data API client — public endpoints, no authentication required
//!
//! Uses `data-api.polymarket.com` for leaderboard, positions, trades, and portfolio value.
//! Uses `gamma-api.polymarket.com` for market/event metadata.

use anyhow::Result;
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::debug;

const BASE_URL: &str = "https://data-api.polymarket.com";
const GAMMA_URL: &str = "https://gamma-api.polymarket.com";

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
}
