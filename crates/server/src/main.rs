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
    run_continuous_discovery, run_discovery, run_optimization, BinanceClient, DiscoveryProgress,
    DiscoveryRequest, DiscoveryResult, DiscoveryStatus, OptimizeProgress, OptimizeRequest,
    OptimizeStatus, SizingMode,
};
use persistence::repository::DiscoveryRepository;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{error, info};

const APP_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-", env!("GIT_HASH"));

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
    db: Arc<persistence::Database>,
    discovery_progress: Arc<DiscoveryProgress>,
    optimize_progress: Arc<OptimizeProgress>,
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
        db: Arc::new(db),
        discovery_progress: Arc::new(DiscoveryProgress::new()),
        optimize_progress: Arc::new(OptimizeProgress::new()),
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
