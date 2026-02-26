//! Poly-Discover — Standalone Discovery Agent for strategy backtesting
//!
//! Usage:
//!   poly-discover serve --port 3001        — Launch web server with UI
//!   poly-discover run --symbols BTCUSDT    — Run discovery from CLI

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::Utc;
use clap::{Parser, Subcommand};
use engine::{
    analyze_leaderboard, analyze_profile, run_continuous_discovery, run_discovery, run_optimization,
    run_orderbook_backtest, run_orderbook_collector, run_trade_watcher, BinanceClient,
    DiscoveryProgress, DiscoveryRequest, DiscoveryResult, DiscoveryStatus,
    LeaderboardProgress, ObBacktestProgress, ObCollectorProgress,
    OptimizeProgress, OptimizeRequest, OptimizeStatus, PolymarketDataClient, ProfileProgress,
    ProfileStatus, SizingMode, WatcherProgress,
};
use persistence::repository::{
    DiscoveryRepository, LeaderboardRepository, OrderbookRepository, ProfileRepository,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{error, info};

const APP_VERSION: &str = concat!("1.0.", env!("BUILD_NUMBER"), "-", env!("GIT_HASH"));

#[derive(Parser)]
#[command(name = "poly-discover")]
#[command(about = "Standalone Discovery Agent for strategy backtesting", long_about = None)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch the discovery web server
    Serve {
        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        /// Port to listen on
        #[arg(short, long, default_value_t = 3001)]
        port: u16,
    },
    /// Run a discovery scan from CLI (no web server)
    Run {
        /// Symbols to scan (comma-separated)
        #[arg(long, value_delimiter = ',')]
        symbols: Vec<String>,
        /// Number of days of historical data
        #[arg(long, default_value_t = 365)]
        days: u32,
        /// Number of top results to return
        #[arg(long, default_value_t = 10)]
        top_n: usize,
        /// Sizing mode: fixed, kelly, confidence
        #[arg(long, default_value = "fixed")]
        sizing: String,
        /// Optional JSON export path
        #[arg(long)]
        export: Option<String>,
        /// Run continuously until Ctrl+C
        #[arg(long)]
        continuous: bool,
    },
    /// Cleanup DB: keep top N best results per strategy (positive PnL only), delete the rest
    Cleanup {
        /// Number of best results to keep per strategy_name (default 3)
        #[arg(long, default_value_t = 3)]
        keep: i64,
    },
}

#[derive(Clone)]
struct AppState {
    binance: Arc<BinanceClient>,
    polymarket: Arc<PolymarketDataClient>,
    db: Arc<persistence::Database>,
    discovery_progress: Arc<DiscoveryProgress>,
    optimize_progress: Arc<OptimizeProgress>,
    leaderboard_progress: Arc<LeaderboardProgress>,
    watcher_progress: Arc<WatcherProgress>,
    profile_progress: Arc<ProfileProgress>,
    ob_backtest_progress: Arc<ObBacktestProgress>,
    ob_collector_progress: Arc<ObCollectorProgress>,
}

fn init_logging(verbose: bool) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = if verbose {
        EnvFilter::new("debug,engine=debug,poly_discover=debug")
    } else {
        EnvFilter::new("info,engine=info,poly_discover=info")
    };

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false).compact())
        .with(filter)
        .init();
}

fn parse_sizing_mode(s: &str) -> SizingMode {
    match s.to_lowercase().as_str() {
        "kelly" => SizingMode::Kelly,
        "confidence" => SizingMode::ConfidenceWeighted,
        _ => SizingMode::Fixed,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);
    dotenvy::dotenv().ok();

    match cli.command {
        Commands::Serve { host, port } => {
            cmd_serve(&host, port).await?;
        }
        Commands::Run {
            symbols,
            days,
            top_n,
            sizing,
            export,
            continuous,
        } => {
            cmd_run(symbols, days, top_n, sizing, export, continuous).await?;
        }
        Commands::Cleanup { keep } => {
            cmd_cleanup(keep).await?;
        }
    }

    Ok(())
}

// ============================================================================
// Serve command — Axum web server
// ============================================================================

async fn cmd_serve(host: &str, port: u16) -> anyhow::Result<()> {
    info!("Poly-Discover v{} starting...", APP_VERSION);

    let db_path =
        std::env::var("POLY_DISCOVERY_DB_PATH").unwrap_or_else(|_| "data/discovery.db".to_string());
    let db = persistence::Database::new(&db_path).await.map_err(|e| {
        error!("Failed to initialize database: {}", e);
        anyhow::anyhow!("Database initialization failed: {}", e)
    })?;
    info!("Database initialized: {}", db_path);

    let state = AppState {
        binance: Arc::new(BinanceClient::new()),
        polymarket: Arc::new(PolymarketDataClient::new()),
        db: Arc::new(db),
        discovery_progress: Arc::new(DiscoveryProgress::new()),
        optimize_progress: Arc::new(OptimizeProgress::new()),
        leaderboard_progress: Arc::new(LeaderboardProgress::new()),
        watcher_progress: Arc::new(WatcherProgress::new()),
        profile_progress: Arc::new(ProfileProgress::new()),
        ob_backtest_progress: Arc::new(ObBacktestProgress::new()),
        ob_collector_progress: Arc::new(ObCollectorProgress::new()),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Determine static files directory
    let exe_path = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe_path.parent().unwrap_or(std::path::Path::new("."));
    let dist_dir = exe_dir.join("dist");
    let static_dir = if dist_dir.exists() {
        dist_dir
    } else {
        std::path::PathBuf::from("dist")
    };

    let api_routes = Router::new()
        .route("/health", get(api_health))
        .route("/discover", post(api_start_discovery))
        .route("/discover/status", get(api_discovery_status))
        .route("/discover/cancel", post(api_cancel_discovery))
        .route("/knowledge", get(api_knowledge_base))
        .route("/knowledge/top-strategies", get(api_top_strategies))
        .route("/knowledge/stats", get(api_knowledge_stats))
        .route("/export", get(api_export))
        .route("/optimize", post(api_start_optimization))
        .route("/optimize/status", get(api_optimize_status))
        .route("/binance/klines", get(api_binance_klines))
        .route("/leaderboard", post(api_analyze_leaderboard))
        .route("/leaderboard/status", get(api_leaderboard_status))
        .route("/leaderboard/traders", get(api_leaderboard_traders))
        .route("/watcher/start", post(api_start_watcher))
        .route("/watcher/stop", post(api_stop_watcher))
        .route("/watcher/status", get(api_watcher_status))
        .route("/strategies/catalog", get(api_strategies_catalog))
        .route("/profile/analyze", post(api_start_profile_analysis))
        .route("/profile/status", get(api_profile_status))
        .route("/profile/cancel", post(api_cancel_profile_analysis))
        .route("/profile/history", get(api_profile_history))
        .route("/orderbook/analyze", post(api_start_ob_backtest))
        .route("/orderbook/status", get(api_ob_backtest_status))
        .route("/orderbook/cancel", post(api_cancel_ob_backtest))
        .route("/orderbook/patterns", get(api_ob_patterns))
        .route("/orderbook/stats", get(api_ob_stats))
        .route("/orderbook/collector/start", post(api_start_ob_collector))
        .route("/orderbook/collector/stop", post(api_stop_ob_collector))
        .route("/orderbook/collector/status", get(api_ob_collector_status))
        .route("/orderbook/cleanup", post(api_ob_cleanup))
        .with_state(state);

    let app = Router::new()
        .nest("/api", api_routes)
        .fallback_service(ServeDir::new(&static_dir))
        .layer(cors);

    let addr: std::net::SocketAddr = format!("{}:{}", host, port).parse()?;
    println!("\n=== Poly-Discover v{} ===", APP_VERSION);
    println!("Discovery Agent Server");
    println!("Listening on http://{}", addr);
    println!("\nEndpoints:");
    println!("  GET  /api/health              - Health check");
    println!("  POST /api/discover            - Start discovery scan");
    println!("  GET  /api/discover/status     - Poll discovery progress");
    println!("  POST /api/discover/cancel     - Cancel running discovery");
    println!("  GET  /api/knowledge           - Knowledge base (paginated)");
    println!("  GET  /api/knowledge/top-strategies - Top unique strategies");
    println!("  GET  /api/knowledge/stats     - Knowledge base stats");
    println!("  GET  /api/export              - Export results as JSON");
    println!("  POST /api/optimize            - Start parameter optimization");
    println!("  GET  /api/optimize/status     - Poll optimization progress");
    println!("  GET  /api/binance/klines      - Fetch Binance klines (proxy)");
    println!("  POST /api/leaderboard         - Analyze top Polymarket traders");
    println!("  GET  /api/leaderboard/status  - Poll leaderboard analysis progress");
    println!("  GET  /api/leaderboard/traders - Get persisted traders from DB");
    println!("  POST /api/watcher/start       - Start trade watcher");
    println!("  POST /api/watcher/stop        - Stop trade watcher");
    println!("  GET  /api/watcher/status      - Poll trade watcher status + alerts");
    println!("  GET  /api/strategies/catalog  - Web-researched strategies catalog");
    println!("  POST /api/profile/analyze     - Analyze a Polymarket user profile");
    println!("  GET  /api/profile/status      - Poll profile analysis progress");
    println!("  POST /api/profile/cancel      - Cancel profile analysis");
    println!("  GET  /api/profile/history     - List past profile analyses");
    println!("  POST /api/orderbook/analyze  - Start orderbook backtest analysis");
    println!("  GET  /api/orderbook/status   - Poll orderbook backtest progress");
    println!("  POST /api/orderbook/cancel   - Cancel orderbook backtest");
    println!("  GET  /api/orderbook/patterns - Get detected patterns");
    println!("  GET  /api/orderbook/stats    - Get orderbook analysis stats");
    println!("  POST /api/orderbook/collector/start  - Start live collector");
    println!("  POST /api/orderbook/collector/stop   - Stop live collector");
    println!("  GET  /api/orderbook/collector/status  - Poll collector status");
    println!("  POST /api/orderbook/cleanup  - Manual data cleanup");
    println!("\n  Database: {}", db_path);
    println!("\nPress Ctrl+C to stop\n");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ============================================================================
// Run command — CLI mode (no web server)
// ============================================================================

async fn cmd_run(
    symbols: Vec<String>,
    days: u32,
    top_n: usize,
    sizing: String,
    export: Option<String>,
    continuous: bool,
) -> anyhow::Result<()> {
    println!("\n=== Poly-Discover v{} ===", APP_VERSION);

    let db_path =
        std::env::var("POLY_DISCOVERY_DB_PATH").unwrap_or_else(|_| "data/discovery.db".to_string());
    let db = persistence::Database::new(&db_path)
        .await
        .map_err(|e| anyhow::anyhow!("Database initialization failed: {}", e))?;

    // Check cached count
    let repo = DiscoveryRepository::new(db.pool());
    let cached_stats = repo.get_stats().await;
    let total_cached = cached_stats
        .as_ref()
        .map(|s| s.total_backtests)
        .unwrap_or(0);
    println!("Database: {} ({} backtests cached)", db_path, total_cached);
    println!(
        "Symbols: {}",
        if symbols.is_empty() {
            "BTC, ETH, SOL, XRP (default)".to_string()
        } else {
            symbols.join(", ")
        }
    );
    println!(
        "Days: {} | Sizing: {} | Top N: {} | Mode: {}",
        days,
        sizing,
        top_n,
        if continuous { "CONTINUOUS" } else { "single" }
    );
    if continuous {
        println!("Press Ctrl+C to stop");
    }
    println!();

    let binance = Arc::new(BinanceClient::new());
    let progress = Arc::new(DiscoveryProgress::new());
    let db_pool = Some(db.pool_clone());

    let sizing_mode = parse_sizing_mode(&sizing);
    let request = DiscoveryRequest {
        symbols: if symbols.is_empty() {
            vec![
                "BTCUSDT".to_string(),
                "ETHUSDT".to_string(),
                "SOLUSDT".to_string(),
                "XRPUSDT".to_string(),
            ]
        } else {
            symbols
        },
        days,
        top_n: Some(top_n),
        sizing_mode: Some(sizing_mode),
        continuous: Some(continuous),
    };

    // Set up Ctrl+C handler for continuous mode
    let progress_for_ctrlc = progress.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Ctrl+C received, requesting cancel...");
        progress_for_ctrlc
            .cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    });

    // Spawn discovery in background and monitor progress
    let progress_clone = progress.clone();
    let discovery_handle = if continuous {
        tokio::spawn(async move {
            run_continuous_discovery(request, binance, progress_clone, db_pool).await;
        })
    } else {
        tokio::spawn(async move {
            run_discovery(request, binance, progress_clone, db_pool).await;
        })
    };

    // Progress display loop
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let status = progress.status.read().unwrap().clone();
        let phase = progress.phase.read().unwrap().clone();
        let completed = progress
            .completed
            .load(std::sync::atomic::Ordering::Relaxed);
        let total = progress
            .total_combinations
            .load(std::sync::atomic::Ordering::Relaxed);
        let skipped = progress.skipped.load(std::sync::atomic::Ordering::Relaxed);
        let pct = progress.progress_pct();

        match status {
            DiscoveryStatus::FetchingData => {
                print!("\r  Fetching market data...                                      ");
            }
            DiscoveryStatus::Phase1BroadScan
            | DiscoveryStatus::Phase2Refinement
            | DiscoveryStatus::Phase3Exploration => {
                let current_cycle = progress
                    .current_cycle
                    .load(std::sync::atomic::Ordering::Relaxed);
                let total_all = progress
                    .total_tested_all_cycles
                    .load(std::sync::atomic::Ordering::Relaxed);
                let new_this_cycle = progress
                    .total_new_this_cycle
                    .load(std::sync::atomic::Ordering::Relaxed);
                let best_score = progress
                    .best_so_far
                    .read()
                    .unwrap()
                    .first()
                    .map(|r| r.composite_score)
                    .unwrap_or_default();

                let bar_len = 30;
                let filled = (pct as usize * bar_len) / 100;
                let empty = bar_len - filled;
                let bar: String = "=".repeat(filled) + &" ".repeat(empty);

                if continuous {
                    print!(
                        "\r  Cycle {} — {} [{}] {:.0}% ({}/{}, {} cached) — {}   ",
                        current_cycle, phase, bar, pct, completed, total, skipped, phase
                    );
                    print!(
                        "\n  Total: {} tested | {} new this cycle | Best: {:.1}           \x1b[1A",
                        total_all, new_this_cycle, best_score
                    );
                } else {
                    let phase_label = if matches!(status, DiscoveryStatus::Phase1BroadScan) {
                        "Phase 1: Broad Scan"
                    } else {
                        "Phase 2: Refinement"
                    };
                    print!(
                        "\r  {} [{}] {:.0}% ({}/{}, {} cached) — {}   ",
                        phase_label, bar, pct, completed, total, skipped, phase
                    );
                }
            }
            DiscoveryStatus::Complete => {
                let total_all = progress
                    .total_tested_all_cycles
                    .load(std::sync::atomic::Ordering::Relaxed);
                if continuous {
                    println!(
                        "\r  Complete! Total: {} tested ({} computed, {} cached)                              ",
                        total_all,
                        completed - skipped,
                        skipped
                    );
                } else {
                    println!(
                        "\r  Complete! ({} computed, {} cached)                              ",
                        completed - skipped,
                        skipped
                    );
                }
                break;
            }
            DiscoveryStatus::Error => {
                let err = progress.error_message.read().unwrap().clone();
                println!(
                    "\r  Error: {}                                      ",
                    err.unwrap_or_default()
                );
                break;
            }
            _ => {}
        }
    }

    // Wait for task to finish
    let _ = discovery_handle.await;

    // Display results
    let results = progress.final_results.read().unwrap().clone();
    if results.is_empty() {
        // In continuous mode, also check best_so_far
        let best = progress.best_so_far.read().unwrap().clone();
        if best.is_empty() {
            println!("\nNo results found.");
            return Ok(());
        }
        print_results(&best, top_n);
    } else {
        print_results(&results, top_n);
    }

    // Export if requested
    if let Some(export_path) = export {
        let results = progress.final_results.read().unwrap().clone();
        let export_data = build_export_json(&results, top_n, None);
        let json = serde_json::to_string_pretty(&export_data)?;
        std::fs::write(&export_path, &json)?;
        println!("\nResults exported to {}", export_path);
    }

    Ok(())
}

fn print_results(results: &[DiscoveryResult], top_n: usize) {
    println!("\nTop {} Results:", results.len().min(top_n));
    println!(
        "  {:>3}  {:<20} {:<10} {:>8} {:>8} {:>10} {:>7}",
        "#", "Strategy", "Symbol", "Score", "WR%", "PnL", "Sharpe"
    );
    println!("  {}", "-".repeat(75));
    for (i, r) in results.iter().take(top_n).enumerate() {
        println!(
            "  {:>3}  {:<20} {:<10} {:>8.1} {:>7.1}% {:>+10.2} {:>7.2}",
            i + 1,
            r.strategy_name,
            r.symbol,
            r.composite_score,
            r.win_rate,
            r.net_pnl,
            r.sharpe_ratio,
        );
    }
}

// ============================================================================
// API Handlers — Discovery
// ============================================================================

/// GET /api/health
async fn api_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "poly-discover",
        "version": APP_VERSION,
    }))
}

/// POST /api/discover — start a discovery scan
async fn api_start_discovery(
    State(state): State<AppState>,
    Json(request): Json<DiscoveryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if state.discovery_progress.is_running() {
        let pct = state.discovery_progress.progress_pct();
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": format!("Discovery agent already running ({:.0}% complete)", pct),
        })));
    }

    let is_continuous = request.continuous.unwrap_or(false);

    info!(
        symbols = ?request.symbols,
        days = request.days,
        continuous = is_continuous,
        "Starting discovery agent"
    );

    state.discovery_progress.reset();

    let binance = state.binance.clone();
    let progress = state.discovery_progress.clone();
    let db_pool = Some(state.db.pool_clone());

    if is_continuous {
        tokio::spawn(async move {
            run_continuous_discovery(request, binance, progress, db_pool).await;
        });
    } else {
        tokio::spawn(async move {
            run_discovery(request, binance, progress, db_pool).await;
        });
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": if is_continuous { "Continuous discovery started" } else { "Discovery agent started" },
        "continuous": is_continuous,
    })))
}

/// POST /api/discover/cancel — cancel running discovery
async fn api_cancel_discovery(State(state): State<AppState>) -> Json<serde_json::Value> {
    state
        .discovery_progress
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    info!("Discovery cancel requested via API");
    Json(serde_json::json!({
        "success": true,
        "message": "Cancel requested"
    }))
}

/// GET /api/discover/status — poll discovery progress
async fn api_discovery_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let progress = &state.discovery_progress;
    let status = progress.status.read().unwrap().clone();
    let phase = progress.phase.read().unwrap().clone();
    let current_strategy = progress.current_strategy.read().unwrap().clone();
    let current_symbol = progress.current_symbol.read().unwrap().clone();
    let total = progress
        .total_combinations
        .load(std::sync::atomic::Ordering::Relaxed);
    let completed = progress
        .completed
        .load(std::sync::atomic::Ordering::Relaxed);
    let skipped = progress.skipped.load(std::sync::atomic::Ordering::Relaxed);
    let pct = progress.progress_pct();
    let best_so_far = progress.best_so_far.read().unwrap().clone();
    let final_results = progress.final_results.read().unwrap().clone();
    let error = progress.error_message.read().unwrap().clone();
    let started_at = progress.started_at.read().unwrap().clone();
    let current_cycle = progress
        .current_cycle
        .load(std::sync::atomic::Ordering::Relaxed);
    let total_tested_all_cycles = progress
        .total_tested_all_cycles
        .load(std::sync::atomic::Ordering::Relaxed);
    let total_new_this_cycle = progress
        .total_new_this_cycle
        .load(std::sync::atomic::Ordering::Relaxed);
    let is_continuous = progress
        .is_continuous
        .load(std::sync::atomic::Ordering::Relaxed);

    let results = if matches!(status, DiscoveryStatus::Complete) {
        &final_results
    } else {
        &best_so_far
    };

    Json(serde_json::json!({
        "status": status,
        "phase": phase,
        "current_strategy": current_strategy,
        "current_symbol": current_symbol,
        "progress_pct": pct,
        "completed": completed,
        "skipped": skipped,
        "total": total,
        "best_so_far": best_so_far,
        "results": results,
        "error": error,
        "started_at": started_at,
        "current_cycle": current_cycle,
        "total_tested_all_cycles": total_tested_all_cycles,
        "total_new_this_cycle": total_new_this_cycle,
        "is_continuous": is_continuous,
    }))
}

// ============================================================================
// API Handlers — Knowledge Base
// ============================================================================

/// GET /api/knowledge — paginated discovery backtest results with filters
async fn api_knowledge_base(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let limit: i64 = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let offset: i64 = params
        .get("offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let strategy_type = params.get("strategy_type").map(|s| s.as_str());
    let symbol = params.get("symbol").map(|s| s.as_str());
    let min_win_rate: Option<f64> = params.get("min_win_rate").and_then(|s| s.parse().ok());
    let sort_by = params.get("sort_by").map(|s| s.as_str());

    let repo = DiscoveryRepository::new(state.db.pool());
    match repo
        .get_all_paginated(limit, offset, strategy_type, symbol, min_win_rate, sort_by)
        .await
    {
        Ok((records, total)) => Json(serde_json::json!({
            "success": true,
            "data": records,
            "total": total,
            "limit": limit,
            "offset": offset,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to query knowledge base: {}", e),
            "data": [],
            "total": 0,
        })),
    }
}

/// GET /api/knowledge/stats — aggregated knowledge base statistics
async fn api_knowledge_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let repo = DiscoveryRepository::new(state.db.pool());
    match repo.get_stats().await {
        Ok(stats) => Json(serde_json::json!({
            "success": true,
            "stats": stats,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to get knowledge base stats: {}", e),
        })),
    }
}

/// GET /api/knowledge/top-strategies — top unique strategies (deduplicated by strategy_name)
async fn api_top_strategies(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let limit: i64 = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let sort_by = params.get("sort_by").map(|s| s.as_str());

    let repo = DiscoveryRepository::new(state.db.pool());
    match repo.get_top_unique_strategies(limit, sort_by).await {
        Ok(records) => Json(serde_json::json!({
            "success": true,
            "data": records,
            "total": records.len(),
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to query top strategies: {}", e),
            "data": [],
            "total": 0,
        })),
    }
}

// ============================================================================
// API Handlers — Optimizer
// ============================================================================

/// POST /api/optimize — Start parameter optimization in background
async fn api_start_optimization(
    State(state): State<AppState>,
    Json(request): Json<OptimizeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if state.optimize_progress.is_running() {
        let pct = state.optimize_progress.progress_pct();
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": format!("Optimization already in progress ({:.0}% complete)", pct),
        })));
    }

    info!(
        strategy = %request.strategy,
        symbol = %request.symbol,
        days = request.days,
        "Starting parameter optimization"
    );

    state.optimize_progress.reset(request.strategy.clone());

    // Calculate time range
    let end_time = Utc::now().timestamp_millis();
    let start_time = end_time - (request.days as i64 * 24 * 60 * 60 * 1000);

    // Fetch klines from Binance
    let klines = match state
        .binance
        .get_klines_paginated(&request.symbol, "15m", start_time, end_time)
        .await
    {
        Ok(k) => k,
        Err(e) => {
            error!("Failed to fetch klines for optimization: {}", e);
            *state.optimize_progress.status.write().unwrap() = OptimizeStatus::Error;
            *state.optimize_progress.error_message.write().unwrap() =
                Some(format!("Failed to fetch klines: {}", e));
            return Ok(Json(serde_json::json!({
                "success": false,
                "message": format!("Failed to fetch market data: {}", e),
            })));
        }
    };

    info!(
        klines = klines.len(),
        "Klines fetched, spawning optimization task"
    );

    let progress = state.optimize_progress.clone();
    tokio::spawn(async move {
        run_optimization(request, klines, progress).await;
    });

    let total = state
        .optimize_progress
        .total_combinations
        .load(std::sync::atomic::Ordering::Relaxed);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Optimization started ({} combinations)", total),
        "total_combinations": total,
    })))
}

/// GET /api/optimize/status — Poll optimization progress
async fn api_optimize_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let progress = &state.optimize_progress;
    let status = progress.status.read().unwrap().clone();
    let total = progress
        .total_combinations
        .load(std::sync::atomic::Ordering::Relaxed);
    let completed = progress
        .completed
        .load(std::sync::atomic::Ordering::Relaxed);
    let pct = progress.progress_pct();
    let results = progress.results.read().unwrap().clone();
    let error = progress.error_message.read().unwrap().clone();
    let strategy = progress.strategy.read().unwrap().clone();

    Json(serde_json::json!({
        "status": status,
        "strategy": strategy,
        "progress_pct": pct,
        "completed": completed,
        "total": total,
        "results": results,
        "error": error,
    }))
}

// ============================================================================
// API Handlers — Binance Proxy
// ============================================================================

/// GET /api/binance/klines — Proxy endpoint for Binance klines
async fn api_binance_klines(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let symbol = params
        .get("symbol")
        .cloned()
        .unwrap_or_else(|| "BTCUSDT".to_string());
    let interval = params
        .get("interval")
        .cloned()
        .unwrap_or_else(|| "15m".to_string());
    let start_time: Option<i64> = params.get("start_time").and_then(|s| s.parse().ok());
    let end_time: Option<i64> = params.get("end_time").and_then(|s| s.parse().ok());
    let limit: Option<u32> = params.get("limit").and_then(|s| s.parse().ok());

    let result = if let (Some(start), Some(end)) = (start_time, end_time) {
        state
            .binance
            .get_klines_paginated(&symbol, &interval, start, end)
            .await
    } else {
        state
            .binance
            .get_klines(&symbol, &interval, start_time, end_time, limit)
            .await
    };

    match result {
        Ok(klines) => Json(serde_json::json!({
            "success": true,
            "symbol": symbol,
            "interval": interval,
            "count": klines.len(),
            "klines": klines,
        })),
        Err(e) => {
            error!("Binance klines error: {}", e);
            Json(serde_json::json!({
                "success": false,
                "message": format!("Failed to fetch klines: {}", e),
            }))
        }
    }
}

// ============================================================================
// API Handlers — Export
// ============================================================================

/// Query params for export endpoint
#[derive(Deserialize)]
struct ExportParams {
    #[serde(default = "default_top_n")]
    top_n: usize,
    min_win_rate: Option<f64>,
}

fn default_top_n() -> usize {
    20
}

/// GET /api/export — export top results as structured JSON
async fn api_export(
    State(state): State<AppState>,
    Query(params): Query<ExportParams>,
) -> Json<serde_json::Value> {
    let repo = DiscoveryRepository::new(state.db.pool());

    let min_wr = params.min_win_rate;
    match repo
        .get_all_paginated(params.top_n as i64, 0, None, None, min_wr, Some("score"))
        .await
    {
        Ok((records, total_in_db)) => {
            let results: Vec<serde_json::Value> = records
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let params_json: serde_json::Value =
                        serde_json::from_str(&r.strategy_params).unwrap_or_default();

                    let wr: f64 = r.win_rate.parse().unwrap_or(0.0);
                    let sr: f64 = r.sharpe_ratio.parse().unwrap_or(0.0);
                    let recommendation = if wr > 70.0 && sr > 1.5 {
                        "High confidence — strong risk-adjusted returns"
                    } else if wr > 60.0 {
                        "Moderate confidence — decent win rate"
                    } else {
                        "Low confidence — review parameters carefully"
                    };

                    serde_json::json!({
                        "rank": i + 1,
                        "strategy_name": r.strategy_name,
                        "strategy_type": r.strategy_type,
                        "params": params_json,
                        "symbol": r.symbol,
                        "metrics": {
                            "composite_score": r.composite_score,
                            "net_pnl": r.net_pnl,
                            "win_rate": r.win_rate,
                            "sharpe_ratio": r.sharpe_ratio,
                            "max_drawdown_pct": r.max_drawdown_pct,
                            "profit_factor": r.profit_factor,
                            "total_trades": r.total_trades,
                            "sortino_ratio": r.sortino_ratio,
                            "max_consecutive_losses": r.max_consecutive_losses,
                            "avg_win_pnl": r.avg_win_pnl,
                            "avg_loss_pnl": r.avg_loss_pnl,
                            "total_volume": r.total_volume,
                            "annualized_return_pct": r.annualized_return_pct,
                            "annualized_sharpe": r.annualized_sharpe,
                            "strategy_confidence": r.strategy_confidence,
                        },
                        "recommendation": recommendation,
                    })
                })
                .collect();

            Json(serde_json::json!({
                "generated_at": Utc::now().to_rfc3339(),
                "total_backtests_in_db": total_in_db,
                "export_filters": {
                    "top_n": params.top_n,
                    "min_win_rate": min_wr,
                },
                "results": results,
            }))
        }
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Export failed: {}", e),
        })),
    }
}

// ============================================================================
// API Handlers — Leaderboard
// ============================================================================

/// POST /api/leaderboard — start leaderboard analysis
async fn api_analyze_leaderboard(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if state.leaderboard_progress.is_running() {
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": "Leaderboard analysis already running",
        })));
    }

    info!("Starting leaderboard analysis");
    state.leaderboard_progress.reset();

    let client = state.polymarket.clone();
    let progress = state.leaderboard_progress.clone();
    let db_pool = Some(state.db.pool_clone());

    tokio::spawn(async move {
        analyze_leaderboard(&client, &progress, 10, db_pool).await;
    });

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Leaderboard analysis started",
    })))
}

/// GET /api/leaderboard/status — poll leaderboard analysis progress
async fn api_leaderboard_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let progress = &state.leaderboard_progress;
    let status = *progress.status.read().unwrap();
    let total = progress.total_traders.load(std::sync::atomic::Ordering::Relaxed);
    let analyzed = progress.analyzed.load(std::sync::atomic::Ordering::Relaxed);
    let current_trader = progress.current_trader.read().unwrap().clone();
    let results = progress.results.read().unwrap().clone();
    let error = progress.error_message.read().unwrap().clone();

    let progress_pct = if total > 0 {
        (analyzed as f64 / total as f64 * 100.0).round()
    } else {
        0.0
    };

    Json(serde_json::json!({
        "status": status,
        "total_traders": total,
        "analyzed": analyzed,
        "progress_pct": progress_pct,
        "current_trader": current_trader,
        "results": results,
        "error": error,
    }))
}

// ============================================================================
// API Handlers — Leaderboard Traders (DB persistence)
// ============================================================================

/// GET /api/leaderboard/traders — get persisted traders from DB
async fn api_leaderboard_traders(State(state): State<AppState>) -> Json<serde_json::Value> {
    let repo = LeaderboardRepository::new(state.db.pool());
    match repo.get_all_traders().await {
        Ok(traders) => Json(serde_json::json!({
            "success": true,
            "data": traders,
            "total": traders.len(),
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to load traders: {}", e),
            "data": [],
            "total": 0,
        })),
    }
}

// ============================================================================
// API Handlers — Trade Watcher
// ============================================================================

/// POST /api/watcher/start — start the trade watcher
async fn api_start_watcher(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if state.watcher_progress.is_running() {
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": "Trade watcher is already running",
        })));
    }

    info!("Starting trade watcher");
    state.watcher_progress.reset();

    let client = state.polymarket.clone();
    let progress = state.watcher_progress.clone();
    let db_pool = state.db.pool_clone();

    tokio::spawn(async move {
        run_trade_watcher(&client, &progress, db_pool).await;
    });

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Trade watcher started",
    })))
}

/// POST /api/watcher/stop — stop the trade watcher
async fn api_stop_watcher(State(state): State<AppState>) -> Json<serde_json::Value> {
    state
        .watcher_progress
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    info!("Trade watcher stop requested via API");
    Json(serde_json::json!({
        "success": true,
        "message": "Watcher stop requested",
    }))
}

/// GET /api/watcher/status — poll trade watcher status + alerts
async fn api_watcher_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let progress = &state.watcher_progress;
    let status = *progress.status.read().unwrap();
    let alerts = progress.alerts.read().unwrap().clone();
    let watched_count = *progress.watched_count.read().unwrap();
    let error = progress.error_message.read().unwrap().clone();

    Json(serde_json::json!({
        "status": status,
        "watched_count": watched_count,
        "alerts": alerts,
        "error": error,
    }))
}

// ============================================================================
// Strategies Catalog
// ============================================================================

/// GET /api/strategies/catalog — return the web-researched strategies catalog
async fn api_strategies_catalog() -> Json<serde_json::Value> {
    let catalog = engine::get_catalog();
    Json(serde_json::json!({
        "success": true,
        "data": catalog,
        "total": catalog.len(),
    }))
}

// ============================================================================
// Profile Analysis endpoints
// ============================================================================

#[derive(Deserialize)]
struct ProfileAnalyzeRequest {
    username: String,
}

async fn api_start_profile_analysis(
    State(state): State<AppState>,
    Json(body): Json<ProfileAnalyzeRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let username = body.username.trim().to_string();
    if username.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Username is required" })),
        );
    }

    if state.profile_progress.is_running() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Profile analysis already running" })),
        );
    }

    state.profile_progress.reset(&username);

    let progress = Arc::clone(&state.profile_progress);
    let client = Arc::clone(&state.polymarket);
    let db_pool = state.db.pool_clone();

    tokio::spawn(async move {
        analyze_profile(
            username,
            &progress,
            &client,
            Some(db_pool),
        )
        .await;
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({ "success": true, "message": "Profile analysis started" })),
    )
}

async fn api_profile_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let p = &state.profile_progress;
    let status = *p.status.read().unwrap();
    let completed = p.completed_steps.load(std::sync::atomic::Ordering::Relaxed);
    let total = p.total_steps.load(std::sync::atomic::Ordering::Relaxed);
    let current_step = p.current_step.read().unwrap().clone();
    let username = p.username.read().unwrap().clone();
    let wallet = p.wallet.read().unwrap().clone();
    let error = p.error_message.read().unwrap().clone();

    let mut response = serde_json::json!({
        "status": format!("{:?}", status),
        "completed_steps": completed,
        "total_steps": total,
        "current_step": current_step,
        "username": username,
        "wallet": wallet,
        "running": p.is_running(),
    });

    if let Some(err) = error {
        response["error"] = serde_json::json!(err);
    }

    if status == ProfileStatus::Complete {
        if let Some(ref result) = *p.result.read().unwrap() {
            response["result"] = serde_json::json!(result);
        }
    }

    Json(response)
}

async fn api_cancel_profile_analysis(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    state
        .profile_progress
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({ "success": true, "message": "Cancellation requested" }))
}

async fn api_profile_history(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = ProfileRepository::new(state.db.pool());
    match repo.get_all_analyses().await {
        Ok(analyses) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "data": analyses,
                "total": analyses.len(),
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("DB error: {}", e) })),
        ),
    }
}

// ============================================================================
// Orderbook Backtest Analysis
// ============================================================================

async fn api_start_ob_backtest(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if state.ob_backtest_progress.is_running() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Orderbook backtest already running" })),
        );
    }

    // Don't call progress.reset() — the orchestrator handles incremental resume
    // Set running state BEFORE spawning so the first poll sees it
    state.ob_backtest_progress.set_status(engine::ObBacktestStatus::Probing);
    state.ob_backtest_progress.set_step("Starting orderbook backtest...");
    state.ob_backtest_progress.add_log("Orderbook backtest started");

    let progress = Arc::clone(&state.ob_backtest_progress);
    let client = Arc::clone(&state.polymarket);
    let db_pool = state.db.pool_clone();

    tokio::spawn(async move {
        run_orderbook_backtest(&progress, &client, db_pool).await;
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({ "success": true, "message": "Orderbook backtest started" })),
    )
}

async fn api_ob_backtest_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let p = &state.ob_backtest_progress;
    let status = *p.status.read().unwrap();
    let data_source = *p.data_source.read().unwrap();
    let current_step = p.current_step.read().unwrap().clone();
    let error = p.error_message.read().unwrap().clone();
    let patterns = p.best_patterns.read().unwrap().clone();
    let stats = p.stats.read().unwrap().clone();
    let logs = p.logs.read().unwrap().clone();

    let mut response = serde_json::json!({
        "status": format!("{:?}", status),
        "data_source": format!("{}", data_source),
        "running": p.is_running(),
        "current_step": current_step,
        "total_markets": p.total_markets.load(std::sync::atomic::Ordering::Relaxed),
        "markets_discovered": p.markets_discovered.load(std::sync::atomic::Ordering::Relaxed),
        "markets_fetched": p.markets_fetched.load(std::sync::atomic::Ordering::Relaxed),
        "features_extracted": p.features_extracted.load(std::sync::atomic::Ordering::Relaxed),
        "patterns_found": p.patterns_found.load(std::sync::atomic::Ordering::Relaxed),
        "stats": stats,
        "logs": logs,
    });

    if !patterns.is_empty() {
        response["best_patterns"] = serde_json::json!(patterns);
    }

    if let Some(err) = error {
        response["error"] = serde_json::json!(err);
    }

    // When not running, include db_state for frontend resume display
    if !p.is_running() {
        if let Ok(resume) = OrderbookRepository::get_resume_stats(state.db.pool()).await {
            let last_step = OrderbookRepository::get_state(state.db.pool(), "last_step_completed")
                .await
                .ok()
                .flatten();
            let data_source_state = OrderbookRepository::get_state(state.db.pool(), "data_source")
                .await
                .ok()
                .flatten();
            let last_run = OrderbookRepository::get_state(state.db.pool(), "last_run_timestamp")
                .await
                .ok()
                .flatten();
            response["db_state"] = serde_json::json!({
                "total_markets": resume.total,
                "unfetched": resume.unfetched,
                "fetched": resume.fetched,
                "extracted": resume.extracted,
                "patterns": resume.patterns,
                "last_step": last_step,
                "data_source": data_source_state,
                "last_run_timestamp": last_run,
            });
        }
    }

    Json(response)
}

async fn api_cancel_ob_backtest(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    state
        .ob_backtest_progress
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({ "success": true, "message": "Cancellation requested" }))
}

async fn api_ob_patterns(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(50);
    let window = params
        .get("window")
        .and_then(|v| v.parse::<i64>().ok());

    let result = if let Some(w) = window {
        OrderbookRepository::get_patterns_by_window(state.db.pool(), w).await
    } else {
        OrderbookRepository::get_top_patterns(state.db.pool(), limit).await
    };

    match result {
        Ok(patterns) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "data": patterns,
                "total": patterns.len(),
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("DB error: {}", e) })),
        ),
    }
}

async fn api_ob_stats(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let market_stats = OrderbookRepository::get_market_stats(state.db.pool())
        .await
        .unwrap_or_default();
    let size_stats = OrderbookRepository::get_db_size_stats(state.db.pool())
        .await
        .unwrap_or_default();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "market_stats": market_stats,
            "db_size": size_stats,
        })),
    )
}

// ============================================================================
// Orderbook Collector (Live WebSocket)
// ============================================================================

async fn api_start_ob_collector(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if state.ob_collector_progress.is_running() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Collector already running" })),
        );
    }

    state.ob_collector_progress.reset();

    let progress = Arc::clone(&state.ob_collector_progress);
    let client = Arc::clone(&state.polymarket);
    let db_pool = state.db.pool_clone();

    tokio::spawn(async move {
        run_orderbook_collector(&progress, &client, db_pool).await;
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({ "success": true, "message": "Collector started" })),
    )
}

async fn api_stop_ob_collector(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    state
        .ob_collector_progress
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({ "success": true, "message": "Collector stop requested" }))
}

async fn api_ob_collector_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let p = &state.ob_collector_progress;
    let status = *p.status.read().unwrap();
    let current_market = p.current_market.read().unwrap().clone();
    let last_snapshot = p.last_snapshot_time.read().unwrap().clone();
    let error = p.error_message.read().unwrap().clone();

    let mut response = serde_json::json!({
        "status": format!("{:?}", status),
        "running": p.is_running(),
        "markets_watched": p.markets_watched.load(std::sync::atomic::Ordering::Relaxed),
        "snapshots_recorded": p.snapshots_recorded.load(std::sync::atomic::Ordering::Relaxed),
        "current_market": current_market,
        "last_snapshot_time": last_snapshot,
    });

    if let Some(err) = error {
        response["error"] = serde_json::json!(err);
    }

    Json(response)
}

async fn api_ob_cleanup(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = state.db.pool();
    let mode = params.get("mode").map(|s| s.as_str()).unwrap_or("partial");

    if mode == "full" {
        // Full reset: delete ALL orderbook data
        let total = OrderbookRepository::full_reset(pool).await.unwrap_or(0);
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "mode": "full",
                "total_deleted": total,
            })),
        );
    }

    // Partial purge: prices for extracted markets + old snapshots
    let prices_purged = OrderbookRepository::purge_prices_for_extracted(pool)
        .await
        .unwrap_or(0);
    let snapshots_purged = OrderbookRepository::purge_old_snapshots(pool, 30)
        .await
        .unwrap_or(0);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "mode": "partial",
            "prices_purged": prices_purged,
            "snapshots_purged": snapshots_purged,
        })),
    )
}

// ============================================================================
// Helpers
// ============================================================================

/// Build export JSON from in-memory results (used by CLI run command)
fn build_export_json(
    results: &[DiscoveryResult],
    top_n: usize,
    min_win_rate: Option<f64>,
) -> serde_json::Value {
    let filtered: Vec<&DiscoveryResult> = results
        .iter()
        .filter(|r| {
            if let Some(min_wr) = min_win_rate {
                r.win_rate >= Decimal::try_from(min_wr).unwrap_or_default()
            } else {
                true
            }
        })
        .take(top_n)
        .collect();

    let items: Vec<serde_json::Value> = filtered
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let params_json = serde_json::to_value(&r.strategy_type).unwrap_or_default();

            serde_json::json!({
                "rank": i + 1,
                "strategy_name": r.strategy_name,
                "strategy_type": params_json,
                "symbol": r.symbol,
                "metrics": {
                    "composite_score": r.composite_score,
                    "net_pnl": r.net_pnl,
                    "win_rate": r.win_rate,
                    "sharpe_ratio": r.sharpe_ratio,
                    "max_drawdown_pct": r.max_drawdown_pct,
                    "profit_factor": r.profit_factor,
                    "total_trades": r.total_trades,
                    "sortino_ratio": r.sortino_ratio,
                    "max_consecutive_losses": r.max_consecutive_losses,
                    "annualized_return_pct": r.annualized_return_pct,
                    "annualized_sharpe": r.annualized_sharpe,
                    "strategy_confidence": r.strategy_confidence,
                },
            })
        })
        .collect();

    serde_json::json!({
        "generated_at": Utc::now().to_rfc3339(),
        "total_results": results.len(),
        "export_filters": {
            "top_n": top_n,
            "min_win_rate": min_win_rate,
        },
        "results": items,
    })
}

// ============================================================================
// Cleanup command — keep top N per strategy, delete the rest
// ============================================================================

async fn cmd_cleanup(keep: i64) -> anyhow::Result<()> {
    info!("Poly-Discover DB cleanup — keeping top {} per strategy (positive PnL only)", keep);

    let db_path =
        std::env::var("POLY_DISCOVERY_DB_PATH").unwrap_or_else(|_| "data/discovery.db".to_string());
    let db = persistence::Database::new(&db_path).await.map_err(|e| {
        error!("Failed to initialize database: {}", e);
        anyhow::anyhow!("Database initialization failed: {}", e)
    })?;
    info!("Database opened: {}", db_path);

    let repo = DiscoveryRepository::new(db.pool());
    let (deleted, remaining) = repo.cleanup_keep_top_n(keep).await.map_err(|e| {
        anyhow::anyhow!("Cleanup failed: {}", e)
    })?;

    info!("Running VACUUM to reclaim disk space...");
    repo.vacuum().await.map_err(|e| {
        anyhow::anyhow!("VACUUM failed: {}", e)
    })?;

    info!("Done! Deleted {} records, {} remaining.", deleted, remaining);
    Ok(())
}
