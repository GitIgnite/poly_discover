//! Polymarket Data API client — public endpoints, no authentication required
//!
//! Uses `data-api.polymarket.com` for leaderboard, positions, trades, and portfolio value.

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

const BASE_URL: &str = "https://data-api.polymarket.com";

/// Polymarket Data API client
#[derive(Clone)]
pub struct PolymarketDataClient {
    client: Client,
}

// ---------------------------------------------------------------------------
// Deserialization structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardEntry {
    pub rank: Option<i64>,
    pub proxy_wallet: Option<String>,
    pub user_name: Option<String>,
    pub vol: Option<f64>,
    pub pnl: Option<f64>,
    pub profile_image: Option<String>,
    pub x_username: Option<String>,
    pub verified_badge: Option<bool>,
}

/// Wrapper: the leaderboard endpoint returns `{ "leaderboard": [...] }`
#[derive(Debug, Deserialize)]
struct LeaderboardResponse {
    leaderboard: Vec<LeaderboardEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderPosition {
    pub proxy_wallet: Option<String>,
    pub asset: Option<String>,
    pub condition_id: Option<String>,
    pub size: Option<f64>,
    pub avg_price: Option<f64>,
    pub current_value: Option<f64>,
    pub cash_pnl: Option<f64>,
    pub percent_pnl: Option<f64>,
    pub title: Option<String>,
    pub outcome: Option<String>,
    pub end_date: Option<String>,
    pub cur_price: Option<f64>,
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
    pub timestamp: Option<String>,
    pub title: Option<String>,
    pub slug: Option<String>,
    pub event_slug: Option<String>,
    pub outcome: Option<String>,
    pub outcome_index: Option<String>,
    pub transaction_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderValue {
    pub value: Option<f64>,
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

        let wrapper: LeaderboardResponse = resp.json().await?;
        debug!(count = wrapper.leaderboard.len(), "Leaderboard fetched");
        Ok(wrapper.leaderboard)
    }

    /// GET /positions?user={address} — trader positions
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

    /// GET /trades?user={address} — trader trade history
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

        let value: TraderValue = resp.json().await?;
        Ok(value)
    }
}
