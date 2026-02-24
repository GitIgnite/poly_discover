//! Live Orderbook Collector — WebSocket-based real-time orderbook recording
//!
//! Connects to Polymarket's WebSocket feed and records orderbook snapshots
//! for the current BTC 15-minute market.

use crate::api::polymarket::PolymarketDataClient;
use futures_util::{SinkExt, StreamExt};
use persistence::repository::orderbook::{ObSnapshotRecord, OrderbookRepository};
use persistence::SqlitePool;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::RwLock;
use tokio_tungstenite::connect_async;
use tracing::{debug, info, warn};

const WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

// ---------------------------------------------------------------------------
// Progress tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CollectorStatus {
    Idle,
    Connecting,
    Collecting,
    Reconnecting,
    Error,
}

pub struct ObCollectorProgress {
    pub status: RwLock<CollectorStatus>,
    pub cancelled: AtomicBool,
    pub markets_watched: AtomicU32,
    pub snapshots_recorded: AtomicU32,
    pub current_market: RwLock<String>,
    pub last_snapshot_time: RwLock<Option<String>>,
    pub error_message: RwLock<Option<String>>,
}

impl ObCollectorProgress {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(CollectorStatus::Idle),
            cancelled: AtomicBool::new(false),
            markets_watched: AtomicU32::new(0),
            snapshots_recorded: AtomicU32::new(0),
            current_market: RwLock::new(String::new()),
            last_snapshot_time: RwLock::new(None),
            error_message: RwLock::new(None),
        }
    }

    pub fn reset(&self) {
        *self.status.write().unwrap() = CollectorStatus::Idle;
        self.cancelled.store(false, Ordering::Relaxed);
        self.markets_watched.store(0, Ordering::Relaxed);
        self.snapshots_recorded.store(0, Ordering::Relaxed);
        *self.current_market.write().unwrap() = String::new();
        *self.last_snapshot_time.write().unwrap() = None;
        *self.error_message.write().unwrap() = None;
    }

    pub fn is_running(&self) -> bool {
        let s = *self.status.read().unwrap();
        matches!(s, CollectorStatus::Connecting | CollectorStatus::Collecting | CollectorStatus::Reconnecting)
    }

    fn set_status(&self, s: CollectorStatus) {
        *self.status.write().unwrap() = s;
    }

    pub fn set_error(&self, msg: String) {
        *self.error_message.write().unwrap() = Some(msg);
        *self.status.write().unwrap() = CollectorStatus::Error;
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

impl Default for ObCollectorProgress {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// WebSocket message types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct WsBookEvent {
    pub market: Option<String>,
    pub asset_id: Option<String>,
    pub bids: Option<Vec<WsLevel>>,
    pub asks: Option<Vec<WsLevel>>,
    pub timestamp: Option<String>,
    #[serde(rename = "type")]
    pub event_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsLevel {
    pub price: String,
    pub size: String,
}

/// Parse a WebSocket book event into a snapshot record.
pub fn parse_book_event(
    event: &WsBookEvent,
    condition_id: &str,
    market_start_time: i64,
) -> Option<ObSnapshotRecord> {
    let bids = event.bids.as_ref()?;
    let asks = event.asks.as_ref()?;
    let token_id = event.asset_id.as_ref()?.clone();

    let best_bid = bids
        .first()
        .and_then(|l| l.price.parse::<f64>().ok());
    let best_ask = asks
        .first()
        .and_then(|l| l.price.parse::<f64>().ok());

    let spread = match (best_bid, best_ask) {
        (Some(b), Some(a)) => Some(a - b),
        _ => None,
    };

    let mid_price = match (best_bid, best_ask) {
        (Some(b), Some(a)) => Some((a + b) / 2.0),
        _ => None,
    };

    let bid_depth: f64 = bids
        .iter()
        .filter_map(|l| l.size.parse::<f64>().ok())
        .sum();
    let ask_depth: f64 = asks
        .iter()
        .filter_map(|l| l.size.parse::<f64>().ok())
        .sum();

    let total = bid_depth + ask_depth;
    let depth_imbalance = if total > 0.0 {
        Some((bid_depth - ask_depth) / total)
    } else {
        None
    };

    let now_ms = chrono::Utc::now().timestamp_millis();
    let elapsed = (now_ms / 1000 - market_start_time) as f64;

    Some(ObSnapshotRecord {
        id: None,
        condition_id: condition_id.to_string(),
        token_id,
        timestamp_ms: now_ms,
        elapsed_seconds: elapsed,
        best_bid,
        best_ask,
        spread,
        mid_price,
        bid_depth_total: Some(bid_depth),
        ask_depth_total: Some(ask_depth),
        depth_imbalance,
        bid_levels: Some(bids.len() as i64),
        ask_levels: Some(asks.len() as i64),
        created_at: None,
    })
}

// ---------------------------------------------------------------------------
// Collector main loop
// ---------------------------------------------------------------------------

pub async fn run_orderbook_collector(
    progress: &ObCollectorProgress,
    client: &PolymarketDataClient,
    db_pool: SqlitePool,
) {
    progress.reset();

    loop {
        if progress.is_cancelled() {
            info!("Orderbook collector cancelled");
            progress.set_status(CollectorStatus::Idle);
            return;
        }

        // 1. Find active BTC 15-min market
        progress.set_status(CollectorStatus::Connecting);
        *progress.current_market.write().unwrap() = "Searching for active market...".into();

        let market = match client.get_active_btc_15min_market().await {
            Ok(Some(m)) => m,
            Ok(None) => {
                debug!("No active BTC 15-min market found, waiting 30s...");
                *progress.current_market.write().unwrap() = "Waiting for next market...".into();
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }
            Err(e) => {
                warn!("Error searching for active market: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            }
        };

        let condition_id = match market.condition_id {
            Some(ref cid) => cid.clone(),
            None => continue,
        };

        let question = market.question.unwrap_or_else(|| "Unknown".to_string());
        *progress.current_market.write().unwrap() = question.clone();
        info!(condition_id = %condition_id, "Watching market: {}", question);

        // Estimate market start time (end_date - 15min)
        let market_start_time = market
            .end_date
            .as_ref()
            .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
            .map(|dt| dt.timestamp() - 900)
            .unwrap_or_else(|| chrono::Utc::now().timestamp());

        // 2. Connect WebSocket
        progress.set_status(CollectorStatus::Connecting);

        let ws_result = connect_async(WS_URL).await;
        let (mut ws_stream, _) = match ws_result {
            Ok(conn) => conn,
            Err(e) => {
                warn!("WebSocket connection failed: {}", e);
                progress.set_status(CollectorStatus::Reconnecting);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        // 3. Subscribe to market
        let subscribe_msg = serde_json::json!({
            "type": "market",
            "assets_ids": [&condition_id]
        });

        if let Err(e) = ws_stream
            .send(tokio_tungstenite::tungstenite::Message::Text(
                subscribe_msg.to_string(),
            ))
            .await
        {
            warn!("WebSocket subscribe failed: {}", e);
            continue;
        }

        info!("WebSocket connected and subscribed to {}", condition_id);
        progress.set_status(CollectorStatus::Collecting);
        progress.markets_watched.fetch_add(1, Ordering::Relaxed);

        // 4. Receive and buffer events
        let mut buffer: Vec<ObSnapshotRecord> = Vec::new();
        let mut last_flush = std::time::Instant::now();

        loop {
            if progress.is_cancelled() {
                info!("Collector cancelled during collection");
                progress.set_status(CollectorStatus::Idle);
                return;
            }

            // Use timeout to periodically check cancelled and flush
            let msg = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                ws_stream.next(),
            )
            .await;

            match msg {
                Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text)))) => {
                    if let Ok(event) = serde_json::from_str::<WsBookEvent>(&text) {
                        if let Some(snapshot) =
                            parse_book_event(&event, &condition_id, market_start_time)
                        {
                            buffer.push(snapshot);
                            progress.snapshots_recorded.fetch_add(1, Ordering::Relaxed);
                            *progress.last_snapshot_time.write().unwrap() =
                                Some(chrono::Utc::now().format("%H:%M:%S").to_string());
                        }
                    }
                }
                Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))) => {
                    info!("WebSocket closed by server");
                    break;
                }
                Ok(Some(Err(e))) => {
                    warn!("WebSocket error: {}", e);
                    break;
                }
                Ok(None) => {
                    info!("WebSocket stream ended");
                    break;
                }
                Err(_) => {
                    // Timeout — flush buffer and continue
                }
                _ => {}
            }

            // Flush buffer every 5 seconds or 50 records
            if buffer.len() >= 50 || (last_flush.elapsed().as_secs() >= 5 && !buffer.is_empty()) {
                match OrderbookRepository::save_snapshots_batch(&db_pool, &buffer).await {
                    Ok(n) => debug!(saved = n, "Flushed snapshot buffer"),
                    Err(e) => warn!("Failed to save snapshots: {}", e),
                }
                buffer.clear();
                last_flush = std::time::Instant::now();
            }

            // Check if market has likely ended (>15 min elapsed)
            let elapsed = chrono::Utc::now().timestamp() - market_start_time;
            if elapsed > 960 {
                // 16 minutes — market should be closed
                info!("Market likely ended ({}s elapsed), moving to next", elapsed);
                break;
            }
        }

        // Flush remaining buffer
        if !buffer.is_empty() {
            let _ = OrderbookRepository::save_snapshots_batch(&db_pool, &buffer).await;
        }

        // Wait a bit before looking for the next market
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_book_event() {
        let event = WsBookEvent {
            market: Some("0x123".to_string()),
            asset_id: Some("token_up".to_string()),
            bids: Some(vec![
                WsLevel { price: "0.55".to_string(), size: "100.0".to_string() },
                WsLevel { price: "0.54".to_string(), size: "200.0".to_string() },
            ]),
            asks: Some(vec![
                WsLevel { price: "0.57".to_string(), size: "150.0".to_string() },
                WsLevel { price: "0.58".to_string(), size: "250.0".to_string() },
            ]),
            timestamp: None,
            event_type: Some("book".to_string()),
        };

        let snapshot = parse_book_event(&event, "cid_test", 1000000).unwrap();
        assert_eq!(snapshot.condition_id, "cid_test");
        assert_eq!(snapshot.token_id, "token_up");
        assert!((snapshot.best_bid.unwrap() - 0.55).abs() < 0.01);
        assert!((snapshot.best_ask.unwrap() - 0.57).abs() < 0.01);
        assert!((snapshot.spread.unwrap() - 0.02).abs() < 0.01);
        assert!((snapshot.mid_price.unwrap() - 0.56).abs() < 0.01);
        assert!((snapshot.bid_depth_total.unwrap() - 300.0).abs() < 0.01);
        assert!((snapshot.ask_depth_total.unwrap() - 400.0).abs() < 0.01);
        assert_eq!(snapshot.bid_levels, Some(2));
        assert_eq!(snapshot.ask_levels, Some(2));
    }

    #[test]
    fn test_parse_book_event_invalid() {
        let event = WsBookEvent {
            market: None,
            asset_id: None,
            bids: None,
            asks: None,
            timestamp: None,
            event_type: None,
        };

        let snapshot = parse_book_event(&event, "cid_test", 1000000);
        assert!(snapshot.is_none());
    }

    #[test]
    fn test_collector_progress_new() {
        let progress = ObCollectorProgress::new();
        assert_eq!(*progress.status.read().unwrap(), CollectorStatus::Idle);
        assert!(!progress.is_running());
        assert_eq!(progress.snapshots_recorded.load(Ordering::Relaxed), 0);
    }
}
