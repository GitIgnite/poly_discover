# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Poly Discover est un projet de **backtesting de stratégies crypto** fonctionnant avec l'**API Polymarket**. Il combine un backend Rust (Axum + SQLite) avec un frontend Svelte 5 (Vite + Tailwind CSS 4). Le système découvre et optimise automatiquement des stratégies de trading sur des paires crypto en appliquant le modèle de frais Polymarket et des indicateurs techniques sur des données Binance (klines 15 minutes).

Le système fonctionne en mode **ML-guided continuous discovery** : un seul bouton Start lance un processus infini qui apprend de ses résultats passés pour guider l'exploration de nouvelles stratégies (algorithme évolutionnaire simplifié).

## Documentation de référence

Le dossier `docs/` contient la documentation essentielle du projet. **Consulter ces fichiers avant toute modification significative** :

- `docs/BINANCE_API.md` — Référence de l'API Binance (endpoints klines, ticker, depth, rate limits)
- `docs/POLYMARKET_API.md` — Référence de l'API Polymarket (endpoints, authentification, structures de données)
- `docs/POLYMARKET_COMPLETE.md` — Documentation complète du système Polymarket
- `docs/POLYMARKET_KEYS_SETUP.md` — Configuration des clés API Polymarket
- `docs/REVERSE_ENGINEERING.md` — Analyses techniques et reverse engineering du protocole
- `docs/DEPLOIEMENT_AWS.md` — Guide de déploiement AWS
- `docs/PREREQUIS_INSTALLATION.md` — Prérequis et installation
- `docs/benchmark_polymarket_bot_stack.md` — Benchmarks de performance
- `docs/new_polymarket_doc.md` — Documentation Polymarket complémentaire
- `docs/polymarket-15min-crypto-reference.md` — Référence stratégie crypto 15 minutes

## Build & Run Commands

### Backend (Rust)
```bash
cargo build                          # Debug build
cargo build --release                # Release build
cargo test --all                     # Run all workspace tests (48 tests)
cargo test -p engine                 # Tests for engine crate only
cargo run -- serve --port 3001       # Start web server
cargo run -- run --symbols BTCUSDT   # Run discovery headless (CLI mode)
cargo run -- run --continuous --symbols BTCUSDT  # Continuous mode CLI
cargo run -- -v serve --port 3001    # Verbose logging
```

### Frontend (Svelte/Vite)
```bash
npm install                          # Install frontend dependencies
npm run dev                          # Dev server on localhost:5174
npm run build                        # Production build → dist/
npm run preview                      # Preview production build
```

### Environment Setup
Copy `.env.example` to `.env`. Two variables:
- `POLY_DISCOVERY_DB_PATH` — SQLite path (default: `data/discovery.db`, auto-created)
- `RUST_LOG` — Log level filter (default: `info`, use `debug` or `engine=debug` for verbose)

## Architecture

### Rust Workspace (3 crates)

```
crates/
├── engine/        Pure business logic — no I/O dependencies except Binance HTTP client
├── persistence/   SQLite data layer (sqlx, compile-time checked queries)
└── server/        Axum HTTP server + Clap CLI entry point — glues engine + persistence
```

**engine** is the core crate. Key modules:
- `discovery.rs` — ML-guided continuous discovery agent with evolutionary exploration (exploitation/crossover/exploration)
- `indicators.rs` — `SignalGenerator` trait + 21 implementations (10 single indicators, 11 combos)
- `engine.rs` — Bar-by-bar backtest simulator with equity tracking
- `optimizer.rs` — Grid-search parameter optimization (supports all 11 strategies)
- `fees.rs` — Polymarket taker fee formula (unit tested)
- `gabagool.rs` — Binary arbitrage backtest on synthetic Polymarket-style markets
- `api/binance.rs` — Binance public klines API client

**persistence** has a single table `discovery_backtests` with a `params_hash` (SHA256) uniqueness constraint. WAL mode, 5-connection pool.

**server** exposes REST endpoints and a CLI with two subcommands: `serve` (web server) and `run` (headless discovery).

### Frontend (Svelte 5)

```
src/
├── App.svelte              Page router + global discovery polling (every 2s)
├── lib/api.js              All backend HTTP calls (discover, cancel, knowledge, optimize, binance)
├── lib/stores.js           Svelte writable stores (currentPage, serverHealth, discoveryStatus)
├── pages/
│   ├── Discovery.svelte    Start/Stop button, reads global discoveryStatus store
│   ├── KnowledgeBase.svelte Auto-refresh when discovery running, LIVE badge
│   └── Optimizer.svelte    Parameter optimization UI
└── components/
    ├── Layout.svelte       Main layout wrapper
    └── Sidebar.svelte      Navigation + discovery running indicator (pulse dot + counter)
```

The frontend is a SPA served from `dist/` by the Rust server. During development, Vite runs on port 5174 and proxies API calls to the Rust backend on port 3001.

### UX Architecture — Global State

Le statut discovery est géré au niveau **App.svelte** (pas dans Discovery.svelte) pour persister entre les changements de page :
- **`discoveryStatus` store** (`stores.js`) : contient running, phase, progress, cycle, best_so_far, etc.
- **Polling global** (App.svelte) : toutes les 2s, appelle `/api/discover/status` et met à jour le store
- **Discovery.svelte** : lit le store, affiche Start/Stop, aucun polling local
- **KnowledgeBase.svelte** : auto-refresh toutes les 5s quand `$discoveryStatus.running === true`, badge LIVE
- **Sidebar.svelte** : pulse dot animé + compteur quand discovery tourne

## Strategy Catalog

### Single Indicators (10)

| # | Strategy | Key Parameters | Signal Logic |
|---|----------|----------------|--------------|
| 1 | RSI | period, overbought, oversold | Buy < oversold, Sell > overbought |
| 2 | Bollinger Bands | period, multiplier | Buy < lower, Sell > upper |
| 3 | MACD | fast, slow, signal | Buy on histogram cross up, Sell on cross down |
| 4 | EMA Crossover | fast_period, slow_period | Buy on golden cross, Sell on death cross |
| 5 | Stochastic | period, overbought, oversold | %K/%D cross in zones |
| 6 | ATR Mean Reversion | atr_period, sma_period, multiplier | Buy far below mean, Sell far above |
| 7 | VWAP | period | Buy < VWAP, Sell > VWAP |
| 8 | OBV | sma_period | Buy when OBV > SMA(OBV), Sell when < |
| 9 | Williams %R | period, overbought, oversold | Buy < oversold (-80), Sell > overbought (-20) |
| 10 | ADX | period, adx_threshold | Buy when +DI > -DI (if ADX strong), Sell inverse |

### Combo Strategies (11)

| # | Strategy | Mode | Components |
|---|----------|------|------------|
| 1 | RSI+Bollinger | Unanimous | RSI + Bollinger Bands |
| 2 | MACD+RSI | PrimaryConfirmed | MACD primary, RSI confirms |
| 3 | EMA+RSI | PrimaryConfirmed | EMA primary, RSI confirms |
| 4 | Stoch+RSI | Unanimous | Stochastic + RSI |
| 5 | MACD+Bollinger | PrimaryConfirmed | MACD primary, BB confirms |
| 6 | Triple:RSI+MACD+BB | Majority | 3-indicator majority vote |
| 7 | Triple:EMA+RSI+Stoch | Majority | 3-indicator majority vote |
| 8 | VWAP+RSI | PrimaryConfirmed | VWAP primary, RSI confirms |
| 9 | OBV+MACD | PrimaryConfirmed | MACD primary, OBV confirms volume |
| 10 | ADX+EMA | PrimaryConfirmed | EMA primary, ADX filters weak trends |
| 11 | Williams%R+Stoch | Unanimous | Double confirmation oversold/overbought |

### Arbitrage (1)

| # | Strategy | Description |
|---|----------|-------------|
| 1 | Gabagool | Binary arbitrage on synthetic Polymarket-style markets |

## Polymarket Context

Polymarket propose des marchés de prédiction crypto avec des fenêtres de 15 minutes. Le modèle de frais taker est :
- `fee = C × feeRate × (p × (1-p))^exponent`
- Default : feeRate=0.25, exponent=2
- Les fees sont maximum à p=0.50 et diminuent vers les extrêmes

Le système utilise une estimation dynamique de probabilité basée sur le changement de prix vs baseline pour calculer des fees plus réalistes au lieu du pire cas p=0.50.

## Key Design Patterns

**SignalGenerator trait** — All indicators implement `on_bar(&mut self, kline: &Kline) -> SignalWithConfidence`. Combo strategies compose multiple generators internally via `ComboSignalGenerator` with three combine modes: `Unanimous`, `Majority`, `PrimaryConfirmed`.

**Continuous Discovery with ML-Guided Exploration** — Le discovery fonctionne en mode continu infini :
- **Cycle 0** : Phase 1 broad scan (~800+ combos × symboles)
- **Cycle 1** : Fine interpolation (valeurs intermédiaires entre les points de la grille)
- **Cycle 2** : Extended ranges (paramètres plus larges)
- **Cycle 3+** : **ML-Guided Exploration** (algorithme évolutionnaire) :
  - **60% exploitation** : mutations (±15%) autour des 30 meilleurs résultats
  - **20% crossover** : mélange de paramètres entre paires de bons résultats du même type
  - **20% exploration** : combinaisons purement aléatoires pour éviter les optima locaux
  - Budget croissant : `300 + cycle × 50` (max 1000)
- Cycle 0 inclut aussi une Phase 2 de refinement des top 20
- Les résultats sont persistés en DB (SQLite) avec déduplication par hash SHA256
- Re-fetch des klines toutes les 6h

**Composite Scoring** — Results are ranked by a composite metric combining net PnL, win rate, Sharpe ratio, max drawdown, and profit factor.

**Dynamic Fee Model** — Fees are calculated using `estimate_poly_probability()` which maps Binance price changes to Polymarket probability estimates, giving more realistic fee calculations than the fixed p=0.50 approach.

**Sizing Modes** — Three position sizing strategies: `fixed`, `kelly`, `confidence`.

## Adding a New Strategy

1. **Implement `SignalGenerator`** in `crates/engine/src/indicators.rs`:
   - Create struct with parameters + internal state
   - Implement `name()`, `on_bar()`, `reset()`
   - `on_bar()` returns `SignalWithConfidence::buy(conf)`, `sell(conf)`, or `hold()`

2. **Add enum variant** in `DiscoveryStrategyType` (`discovery.rs`):
   - Add to `name()` match
   - Add to `result_to_record()` strategy_type_tag match

3. **Add to `build_signal_generator()`** in `indicators.rs`:
   - Map the new enum variant to your struct

4. **Add Phase 1 grid** in `generate_phase1_grid()` (`discovery.rs`):
   - Add parameter combinations to scan

5. **Add Phase 2 refinement** in `generate_refinement_grid()` (`discovery.rs`):
   - Add ±delta variants for fine-tuning

6. **Add ML mutation support** in `mutate_strategy()` (`discovery.rs`):
   - Add match arm for the new variant with parameter perturbation

7. **Add to `generate_random_strategies()`** (`discovery.rs`):
   - Add a case in the random strategy generator

8. **Add to `generate_exploratory_grid()`** cycles 1 and 2 (`discovery.rs`):
   - Add interpolation (cycle 1) and extended range (cycle 2) entries

9. **Add to frontend** :
   - `Discovery.svelte` → `formatDiscoveryParams()` switch case
   - `KnowledgeBase.svelte` → filter dropdown `<option>`

10. **(Optional) Add optimizer support** in `optimizer.rs`:
    - Add variant to `OptimizeStrategy` enum + `Display`
    - Add grid in `generate_indicator_grid()`
    - Add to the match in `run_optimization()`

## API Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/health` | Health check + version |
| POST | `/api/discover` | Start discovery scan (always continuous) |
| GET | `/api/discover/status` | Poll discovery progress (cycle, phase, best_so_far) |
| POST | `/api/discover/cancel` | Cancel running discovery |
| POST | `/api/optimize` | Start parameter optimization |
| GET | `/api/optimize/status` | Poll optimization progress |
| GET | `/api/knowledge` | Paginated backtest results |
| GET | `/api/knowledge/stats` | Aggregated statistics |
| GET | `/api/export` | Export results as JSON |
| GET | `/api/binance/klines` | Proxy to Binance API |

## Testing

Unit tests exist in:
- `crates/engine/src/fees.rs` — 7 tests covering edge cases, symmetry, precision
- `crates/engine/src/discovery.rs` — 15 tests for grid sizes, strategy types, scoring, progress, ML-guided exploration
- `crates/engine/src/indicators.rs` — 5 tests for signal generation, combos, clamping, reset
- `crates/engine/src/optimizer.rs` — 8 tests for grid generation, scoring
- `crates/engine/src/gabagool.rs` — 7 tests for arbitrage engine
- `crates/engine/src/engine.rs` — 2 tests for backtest engine

```bash
cargo test --all                     # Run all 48 tests
cargo test -p engine -- fees         # Run fee-specific tests
cargo test -p engine -- discovery    # Run discovery tests
cargo test -p engine -- indicators   # Run indicator tests
cargo test -p engine -- ml_guided    # Run ML-guided exploration tests
```

## Historique des changements récents

### ML Discovery System — UX Refonte + Exploration Guidée (2026-02-07)

**Problèmes résolus :**
1. Changer de page détruisait le composant Discovery → perte du suivi du processus
2. Les cycles 3+ étaient purement aléatoires sans apprentissage

**Changements frontend :**
- `src/lib/stores.js` : ajout du store `discoveryStatus` global
- `src/lib/api.js` : ajout `cancelDiscovery()`, `startDiscovery()` envoie toujours `continuous: true`
- `src/App.svelte` : polling global du discovery status (2s), persiste entre les pages
- `src/pages/Discovery.svelte` : refonte complète — un seul bouton Start/Stop, lit le store global, plus de polling local
- `src/pages/KnowledgeBase.svelte` : auto-refresh 5s quand discovery running, badge LIVE, 8 nouveaux strategy types dans le dropdown
- `src/components/Sidebar.svelte` : pulse dot animé + compteur de tests quand discovery tourne

**Changements backend :**
- `crates/engine/src/discovery.rs` :
  - `generate_ml_guided_grid()` : algorithme évolutionnaire (60% exploitation, 20% crossover, 20% exploration)
  - `mutate_strategy()` : perturbation ±15% de chaque paramètre numérique
  - `crossover_strategies()` : mélange de paramètres entre stratégies du même type
  - `generate_random_strategies()` : extraction de la génération aléatoire en fonction réutilisable
  - `perturb_usize()`, `perturb_f64()`, `perturb_decimal()` : helpers de perturbation
  - `run_continuous_discovery()` utilise ML-guided grid pour cycle 3+ au lieu du random pur
  - 3 nouveaux tests ML (total : 48 tests)
