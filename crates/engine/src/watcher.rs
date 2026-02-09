//! Trade Watcher — monitor top traders via REST polling and generate alerts
//!
//! Polls `GET /trades?user={wallet}` every 15 seconds for each watched wallet,
//! detects new trades by comparing with DB, and broadcasts alerts.

use crate::api::polymarket::PolymarketDataClient;
use crate::leaderboard::trades_to_records;
use persistence::repository::leaderboard::LeaderboardRepository;
use persistence::SqlitePool;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use tracing::{error, info, warn};

const POLL_INTERVAL_SECS: u64 = 15;
const MAX_ALERTS: usize = 50;
const RATE_LIMIT_MS: u64 = 200;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WatcherStatus {
    Idle,
    Watching,
    Error,
}

/// A single trade alert for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct TradeAlert {
    pub proxy_wallet: String,
    pub user_name: String,
    pub side: String,
    pub title: String,
    pub outcome: String,
    pub size: f64,
    pub price: f64,
    pub timestamp: f64,
}

/// Shared progress/state for the watcher (same pattern as DiscoveryProgress)
pub struct WatcherProgress {
    pub status: RwLock<WatcherStatus>,
    pub cancelled: AtomicBool,
    pub alerts: RwLock<Vec<TradeAlert>>,
    pub watched_count: RwLock<usize>,
    pub error_message: RwLock<Option<String>>,
}

impl WatcherProgress {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(WatcherStatus::Idle),
            cancelled: AtomicBool::new(false),
            alerts: RwLock::new(Vec::new()),
            watched_count: RwLock::new(0),
            error_message: RwLock::new(None),
        }
    }

    pub fn reset(&self) {
        *self.status.write().unwrap() = WatcherStatus::Watching;
        self.cancelled.store(false, Ordering::Relaxed);
        *self.alerts.write().unwrap() = Vec::new();
        *self.watched_count.write().unwrap() = 0;
        *self.error_message.write().unwrap() = None;
    }

    pub fn is_running(&self) -> bool {
        matches!(*self.status.read().unwrap(), WatcherStatus::Watching)
    }

    fn push_alert(&self, alert: TradeAlert) {
        let mut alerts = self.alerts.write().unwrap();
        alerts.insert(0, alert);
        alerts.truncate(MAX_ALERTS);
    }
}

impl Default for WatcherProgress {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Watcher loop
// ---------------------------------------------------------------------------

/// Run the trade watcher: polls each watched wallet every 15s for new trades.
pub async fn run_trade_watcher(
    client: &PolymarketDataClient,
    progress: &WatcherProgress,
    db_pool: SqlitePool,
) {
    info!("Trade watcher starting");
    *progress.status.write().unwrap() = WatcherStatus::Watching;

    // Load watched wallets from DB
    let repo = LeaderboardRepository::new(&db_pool);
    let wallets = match repo.get_watched_wallets().await {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to load watched wallets: {}", e);
            *progress.status.write().unwrap() = WatcherStatus::Error;
            *progress.error_message.write().unwrap() =
                Some(format!("Failed to load wallets: {}", e));
            return;
        }
    };

    if wallets.is_empty() {
        warn!("No wallets to watch — run leaderboard analysis first");
        *progress.status.write().unwrap() = WatcherStatus::Error;
        *progress.error_message.write().unwrap() =
            Some("No wallets to watch. Run leaderboard analysis first.".into());
        return;
    }

    // Load wallet -> username mapping
    let traders = repo.get_all_traders().await.unwrap_or_default();
    let wallet_names: std::collections::HashMap<String, String> = traders
        .into_iter()
        .map(|t| {
            let name = t.user_name.unwrap_or_else(|| "Unknown".into());
            (t.proxy_wallet, name)
        })
        .collect();

    *progress.watched_count.write().unwrap() = wallets.len();
    info!(count = wallets.len(), "Watching wallets");

    // Main polling loop
    loop {
        if progress.cancelled.load(Ordering::Relaxed) {
            info!("Trade watcher cancelled");
            break;
        }

        for wallet in &wallets {
            if progress.cancelled.load(Ordering::Relaxed) {
                break;
            }

            match check_new_trades(client, wallet, &db_pool).await {
                Ok(new_trades) => {
                    let user_name = wallet_names
                        .get(wallet)
                        .cloned()
                        .unwrap_or_else(|| "Unknown".into());

                    for trade_rec in &new_trades {
                        let alert = TradeAlert {
                            proxy_wallet: wallet.clone(),
                            user_name: user_name.clone(),
                            side: trade_rec.side.clone(),
                            title: trade_rec.title.clone().unwrap_or_default(),
                            outcome: trade_rec.outcome.clone().unwrap_or_default(),
                            size: trade_rec.size.unwrap_or(0.0),
                            price: trade_rec.price.unwrap_or(0.0),
                            timestamp: trade_rec.timestamp.unwrap_or(0.0),
                        };
                        info!(
                            user = %user_name,
                            side = %alert.side,
                            size = alert.size,
                            price = alert.price,
                            title = %alert.title,
                            "New trade detected"
                        );
                        progress.push_alert(alert);
                    }

                    // Mark new trades as alerted
                    if !new_trades.is_empty() {
                        let ids: Vec<i64> = new_trades
                            .iter()
                            .filter_map(|t| t.id)
                            .collect();
                        if !ids.is_empty() {
                            let repo = LeaderboardRepository::new(&db_pool);
                            if let Err(e) = repo.mark_alerted(&ids).await {
                                warn!(error = %e, "Failed to mark trades as alerted");
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(wallet = %wallet, error = %e, "Failed to check trades");
                }
            }

            // Rate limit between wallets
            tokio::time::sleep(std::time::Duration::from_millis(RATE_LIMIT_MS)).await;
        }

        // Wait before next polling round
        for _ in 0..(POLL_INTERVAL_SECS * 2) {
            if progress.cancelled.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    *progress.status.write().unwrap() = WatcherStatus::Idle;
    info!("Trade watcher stopped");
}

/// Check for new trades for a given wallet.
/// Fetches recent trades from API, inserts into DB (dedup by hash), returns newly inserted ones.
async fn check_new_trades(
    client: &PolymarketDataClient,
    wallet: &str,
    db_pool: &SqlitePool,
) -> anyhow::Result<Vec<persistence::repository::leaderboard::TraderTradeRecord>> {
    // Fetch latest trades from API
    let api_trades = client.get_trades(wallet).await?;

    if api_trades.is_empty() {
        return Ok(Vec::new());
    }

    // Convert to DB records
    let records = trades_to_records(wallet, &api_trades);

    // Insert (dedup via trade_hash UNIQUE constraint)
    let repo = LeaderboardRepository::new(db_pool);
    let newly_inserted = repo.save_trades(&records).await.map_err(|e| {
        anyhow::anyhow!("DB save_trades failed: {}", e)
    })?;

    if newly_inserted == 0 {
        return Ok(Vec::new());
    }

    // Fetch un-alerted trades for this wallet to return as alerts
    let unalerted = repo.get_unalerted_trades().await.map_err(|e| {
        anyhow::anyhow!("DB get_unalerted_trades failed: {}", e)
    })?;

    // Filter to just this wallet
    let wallet_unalerted: Vec<_> = unalerted
        .into_iter()
        .filter(|t| t.proxy_wallet == wallet)
        .collect();

    Ok(wallet_unalerted)
}
