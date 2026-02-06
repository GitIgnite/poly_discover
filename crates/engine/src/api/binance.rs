//! Binance public API client for market data (no authentication required)

use anyhow::Result;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use tracing::{debug, info};

use crate::types::Kline;

const DEFAULT_BASE_URL: &str = "https://api.binance.com";
const MAX_KLINES_PER_REQUEST: u32 = 1000;

/// Binance public market data client
#[derive(Clone)]
pub struct BinanceClient {
    client: Client,
    base_url: String,
}

/// Raw kline data from Binance API (array of arrays)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawKline(
    i64,    // 0: Open time
    String, // 1: Open
    String, // 2: High
    String, // 3: Low
    String, // 4: Close
    String, // 5: Volume
    i64,    // 6: Close time
    String, // 7: Quote asset volume
    u64,    // 8: Number of trades
    String, // 9: Taker buy base
    String, // 10: Taker buy quote
    String, // 11: Ignore
);

/// Binance ticker price response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TickerPrice {
    symbol: String,
    price: String,
}

/// Binance 24h ticker statistics
#[derive(Debug, Clone, Deserialize)]
pub struct TickerStats {
    pub symbol: String,
    #[serde(rename = "priceChange")]
    pub price_change: String,
    #[serde(rename = "priceChangePercent")]
    pub price_change_percent: String,
    #[serde(rename = "highPrice")]
    pub high_price: String,
    #[serde(rename = "lowPrice")]
    pub low_price: String,
    #[serde(rename = "volume")]
    pub volume: String,
    #[serde(rename = "lastPrice")]
    pub last_price: String,
}

impl Default for BinanceClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BinanceClient {
    /// Create a new Binance client with default base URL
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// Fetch klines (candlestick data) for a symbol
    pub async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        start_time: Option<i64>,
        end_time: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Vec<Kline>> {
        let mut url = format!(
            "{}/api/v3/klines?symbol={}&interval={}",
            self.base_url, symbol, interval
        );

        if let Some(start) = start_time {
            url.push_str(&format!("&startTime={}", start));
        }
        if let Some(end) = end_time {
            url.push_str(&format!("&endTime={}", end));
        }

        let limit = limit.unwrap_or(500).min(MAX_KLINES_PER_REQUEST);
        url.push_str(&format!("&limit={}", limit));

        debug!(symbol, interval, "Fetching klines from Binance");

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Binance API error {}: {}", status, body);
        }

        let raw_klines: Vec<RawKline> = response.json().await?;

        let klines: Vec<Kline> = raw_klines
            .into_iter()
            .filter_map(|raw| {
                Some(Kline {
                    open_time: raw.0,
                    open: Decimal::from_str(&raw.1).ok()?,
                    high: Decimal::from_str(&raw.2).ok()?,
                    low: Decimal::from_str(&raw.3).ok()?,
                    close: Decimal::from_str(&raw.4).ok()?,
                    volume: Decimal::from_str(&raw.5).ok()?,
                    close_time: raw.6,
                })
            })
            .collect();

        debug!(count = klines.len(), "Fetched klines");
        Ok(klines)
    }

    /// Fetch klines with automatic pagination for ranges > 1000 bars
    pub async fn get_klines_paginated(
        &self,
        symbol: &str,
        interval: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<Kline>> {
        let mut all_klines = Vec::new();
        let mut current_start = start_time;

        info!(symbol, interval, "Fetching paginated klines from Binance");

        loop {
            if current_start >= end_time {
                break;
            }

            let klines = self
                .get_klines(
                    symbol,
                    interval,
                    Some(current_start),
                    Some(end_time),
                    Some(MAX_KLINES_PER_REQUEST),
                )
                .await?;

            if klines.is_empty() {
                break;
            }

            let last_close_time = klines.last().map(|k| k.close_time).unwrap_or(end_time);
            all_klines.extend(klines);

            // Move start to after the last candle
            current_start = last_close_time + 1;

            // Small delay to respect rate limits
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        info!(total = all_klines.len(), "Paginated kline fetch complete");
        Ok(all_klines)
    }

    /// Get current price for a symbol
    pub async fn get_price(&self, symbol: &str) -> Result<Decimal> {
        let url = format!("{}/api/v3/ticker/price?symbol={}", self.base_url, symbol);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Binance API error {}: {}", status, body);
        }

        let ticker: TickerPrice = response.json().await?;
        let price = Decimal::from_str(&ticker.price)?;
        Ok(price)
    }

    /// Get 24h ticker statistics
    pub async fn get_24h_stats(&self, symbol: &str) -> Result<TickerStats> {
        let url = format!("{}/api/v3/ticker/24hr?symbol={}", self.base_url, symbol);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Binance API error {}: {}", status, body);
        }

        let stats: TickerStats = response.json().await?;
        Ok(stats)
    }
}
