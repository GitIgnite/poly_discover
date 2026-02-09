# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Poly Discover est un projet de **backtesting de stratégies crypto** fonctionnant avec l'**API Polymarket**. Il combine un backend Rust (Axum + SQLite) avec un frontend Svelte 5 (Vite + Tailwind CSS 4). Le système découvre et optimise automatiquement des stratégies de trading sur des paires crypto en appliquant le modèle de frais Polymarket et des indicateurs techniques sur des données Binance (klines 15 minutes).

Le système fonctionne en mode **ML-guided continuous discovery** : un seul bouton Start lance un processus infini qui apprend de ses résultats passés pour guider l'exploration de nouvelles stratégies (algorithme évolutionnaire simplifié).

## Règle obligatoire — Mise à jour de CLAUDE.md

**Après chaque modification significative du code** (ajout de feature, refactoring, changement d'architecture, ajout de colonnes DB, modification d'API, etc.), **tu DOIS mettre à jour ce fichier `CLAUDE.md`** pour refléter les changements :
- Mettre à jour les sections concernées (Architecture, API Endpoints, Testing, Key Design Patterns, etc.)
- Ajouter une entrée dans **Historique des changements récents** avec la date et le résumé des modifications
- Mettre à jour les compteurs (nombre de tests, nombre de colonnes DB, etc.)
- Cette règle est **non négociable** : un changement de code sans mise à jour de CLAUDE.md est considéré incomplet.

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
cargo test --all                     # Run all workspace tests (67 tests)
cargo test -p engine                 # Tests for engine crate only
cargo run -- serve --port 3001       # Start web server
cargo run -- run --symbols BTCUSDT   # Run discovery headless (CLI mode, default 365 days)
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
- `leaderboard.rs` — Leaderboard analyzer: fetch top traders, compute metrics, infer strategies, persist to DB
- `watcher.rs` — Trade watcher: polls top trader wallets every 15s for new trades, generates alerts
- `web_strategies.rs` — Web-researched Polymarket strategies: static catalogue (12 entries), 5 backtestable SignalGenerators, param variants
- `api/binance.rs` — Binance public klines API client
- `api/polymarket.rs` — Polymarket Data API client (leaderboard, positions, trades, portfolio value)

**persistence** has 3 tables: `discovery_backtests` (30 columns), `leaderboard_traders` (17 columns), `trader_trades` (15 columns). WAL mode, 5-connection pool. Migrations are idempotent (ALTER TABLE tolerates "duplicate column name"). Two repositories: `DiscoveryRepository` and `LeaderboardRepository`.

**server** exposes REST endpoints and a CLI with two subcommands: `serve` (web server) and `run` (headless discovery).

### Frontend (Svelte 5)

```
src/
├── App.svelte              Page router + global discovery polling (every 30s)
├── lib/api.js              All backend HTTP calls (discover, cancel, knowledge, top-strategies, optimize, binance, leaderboard, watcher, strategies-catalog)
├── lib/stores.js           Svelte writable stores (currentPage, serverHealth, discoveryStatus)
├── pages/
│   ├── Discovery.svelte    Start/Stop button, reads global discoveryStatus store
│   ├── TopStrategies.svelte Top 20 unique strategies by win rate, podium, auto-refresh
│   ├── Playbook.svelte     Top 3 by win rate — Polymarket params + bot implementation guide (FR)
│   ├── KnowledgeBase.svelte Auto-refresh when discovery running, LIVE badge
│   ├── Optimizer.svelte    Parameter optimization UI
│   ├── Leaderboard.svelte  Polymarket top traders analysis + strategy inference + trade watcher
│   └── StrategyResearch.svelte  Web-researched Polymarket strategies catalog (12 strategies, filterable)
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
- **TopStrategies.svelte** : top 20 dédupliqué par `strategy_name`, auto-refresh 60s, podium top 3, badge LIVE
- **Playbook.svelte** : top 3 par win rate (via `getTopStrategies(3, 'win_rate')`), tableau "Paramètres essentiels Polymarket" + "Guide d'implémentation bot" ultra-détaillé (7 sections en FR), bouton Copy
- **KnowledgeBase.svelte** : auto-refresh toutes les 60s quand `$discoveryStatus.running === true`, badge LIVE
- **Sidebar.svelte** : pulse dot animé + compteur quand discovery tourne (8 nav items: Discovery, Top 20, Playbook, Knowledge Base, Optimizer, Leaderboard, Strategies Web)

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

### Legacy Combo Strategies (11) — still in DB, no longer explored

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

### Dynamic Combo System (NEW — active discovery)

Le système explore dynamiquement **toutes les combinaisons** possibles des 10 indicateurs :

| Type | Count | Description |
|------|-------|-------------|
| Paires | C(10,2) × 3 modes × 3 params = 405 | Toutes les paires de 2 indicateurs |
| Triples | C(10,3) × 3 modes × 3 params = 1080 | Toutes les triples de 3 indicateurs |
| Quadruples | C(10,4) × 1 mode × 1 param = 210 | Tous les quads (Majority, default) |
| **Total Cycle 0** | **~1743** | + 48 Gabagool |

**Types (`discovery.rs`) :**
- `SingleIndicatorType` — enum des 10 indicateurs (RSI, BB, MACD, EMA, Stoch, ATR, VWAP, OBV, WR, ADX)
- `IndicatorParams` — paramètres spécifiques par indicateur (tagged enum)
- `DynCombineMode` — Unanimous / Majority / PrimaryConfirmed
- `DiscoveryStrategyType::DynamicCombo { indicators, params, combine_mode }`

**Nommage :** `"RSI+MACD(M)"`, `"BB+Stoch+ADX(U)"`, `"RSI+EMA+VWAP+OBV(PC)"`

**3 variantes de paramètres par indicateur :**
- `default_params()` — standard textbook values
- `aggressive_params()` — short periods, tight thresholds
- `conservative_params()` — long periods, wide thresholds
- `random_params_for(rng)` — fully random within valid ranges

**Cycles :**
- Cycle 0 : toutes les paires/triples (3 variants × 3 modes) + quads (default × Majority) + Gabagool
- Cycle 1 : quads avec Unanimous + PrimaryConfirmed + aggressive params
- Cycle 2 : mixed params (aggressive A + conservative B) + random combos
- Cycle 3+ : ML-guided evolutionary (60% mutation, 20% crossover, 20% random)

### Web-Researched Polymarket Strategies (5 backtestable + 7 display-only)

| # | Strategy | Category | Logic | Backtestable |
|---|----------|----------|-------|-------------|
| 1 | ProbabilityEdge | edge | Composite RSI+momentum+volatility → probability estimate → trade when edge > threshold | Yes |
| 2 | CatalystMomentum | momentum | Detect price/volume spikes, enter momentum with trailing stop | Yes |
| 3 | FavoriteCompounder | value | Trade high-probability favorites (price > threshold), accumulate small gains | Yes |
| 4 | MarketMakingSim | market-making | Simulate market maker: bid/ask around SMA, capture spread | Yes |
| 5 | MeanReversionPoly | mean-reversion | Fair value (long SMA), trade extreme deviations with mean reversion | Yes |
| 6 | Arbitrage YES+NO | arbitrage | Requires YES+NO prices simultaneously | No |
| 7 | Whale Copy-Trading | momentum | Requires on-chain data in real-time | No |
| 8 | Cross-Market Arbitrage | arbitrage | Requires correlated markets data | No |
| 9 | Liquidity Provision | market-making | Requires orderbook depth | No |
| 10 | News Sentiment | edge | Requires news feed + NLP | No |
| 11 | Calendar Spread | arbitrage | Requires multiple expirations | No |
| 12 | Contrarian Fade | edge | Requires crowd sentiment data | No |

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

**Continuous Discovery with Dynamic Combos** — Le discovery explore dynamiquement toutes les combinaisons d'indicateurs :
- **Cycle 0** : ~1743 DynamicCombo (paires × 3 params × 3 modes + triples + quads) + 48 Gabagool + Phase 2 refinement des top 20
- **Cycle 1** : Quads avec modes Unanimous/PrimaryConfirmed + aggressive params (~648)
- **Cycle 2** : Mixed param variants (aggressive A + conservative B) + 200 random combos
- **Cycle 3+** : **ML-Guided Exploration** (algorithme évolutionnaire) :
  - **60% exploitation** : mutations (±15%) autour des 30 meilleurs résultats (avec `perturb_indicator_params()`)
  - **20% crossover** : mélange de paramètres entre paires de DynamicCombo du même set d'indicateurs
  - **20% exploration** : DynamicCombo aléatoires (2-4 indicateurs) pour éviter les optima locaux
  - Budget croissant : `300 + cycle × 50` (max 1000)
- Les résultats sont persistés en DB (SQLite) avec déduplication par hash SHA256
- Re-fetch des klines toutes les 6h
- Les anciens backtests (legacy singles/combos) restent en DB, visibles mais non ré-explorés

**Composite Scoring** — Results are ranked by a composite metric combining net PnL, win rate, Sharpe ratio, max drawdown, profit factor, strategy confidence (0-300 bonus), Sortino ratio (0-250 bonus), and consecutive loss penalty (-50/-100).

**Dynamic Fee Model** — Fees are calculated using `estimate_poly_probability()` which maps Binance price changes to Polymarket probability estimates, giving more realistic fee calculations than the fixed p=0.50 approach.

**Strategy Confidence (Quartile Analysis)** — Pour les stratégies prometteuses (net_pnl > 0 ET win_rate > 50%), le système découpe les klines en 4 quartiles, exécute le backtest sur chacun, et calcule un score 0-100% basé sur : 50% nombre de quartiles profitables, 30% consistance des win rates (faible écart-type), 20% win rate minimum.

**Advanced Metrics** — Chaque backtest calcule aussi : Sortino ratio (downside risk), max pertes consécutives, avg win/loss PnL, volume total, return annualisé (`(1+r)^(365/days)-1`), Sharpe annualisé (`sharpe × sqrt(365/days)`).

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
| GET | `/api/knowledge/top-strategies` | Top unique strategies (deduplicated, sort_by param) |
| GET | `/api/knowledge/stats` | Aggregated statistics |
| GET | `/api/export` | Export results as JSON |
| GET | `/api/binance/klines` | Proxy to Binance API |
| POST | `/api/leaderboard` | Start leaderboard analysis (top 10 traders) |
| GET | `/api/leaderboard/status` | Poll leaderboard analysis progress + results |
| GET | `/api/leaderboard/traders` | Get persisted traders from DB |
| POST | `/api/watcher/start` | Start trade watcher (polls watched wallets) |
| POST | `/api/watcher/stop` | Stop trade watcher |
| GET | `/api/watcher/status` | Poll trade watcher status + alerts |
| GET | `/api/strategies/catalog` | Web-researched strategies catalog (12 entries) |

## Testing

Unit tests exist in:
- `crates/engine/src/fees.rs` — 7 tests covering edge cases, symmetry, precision
- `crates/engine/src/discovery.rs` — 20 tests for grid sizes, strategy types, scoring, progress, ML-guided exploration, DynamicCombo naming/mutation/crossover/random
- `crates/engine/src/indicators.rs` — 5 tests for signal generation, combos, clamping, reset
- `crates/engine/src/optimizer.rs` — 8 tests for grid generation, scoring
- `crates/engine/src/gabagool.rs` — 7 tests for arbitrage engine
- `crates/engine/src/engine.rs` — 2 tests for backtest engine
- `crates/engine/src/leaderboard.rs` — 6 tests for metrics computation and strategy inference
- `crates/engine/src/web_strategies.rs` — 8 tests for catalogue, signal generators, param variants

```bash
cargo test --all                     # Run all 67 tests
cargo test -p engine -- fees         # Run fee-specific tests
cargo test -p engine -- discovery    # Run discovery tests
cargo test -p engine -- indicators   # Run indicator tests
cargo test -p engine -- ml_guided    # Run ML-guided exploration tests
cargo test -p engine -- dynamic_combo # Run DynamicCombo tests
cargo test -p engine -- leaderboard  # Run leaderboard analyzer tests
cargo test -p engine -- web_strategies # Run web strategies tests
```

## Déploiement AWS

### Connexion SSH au serveur

```bash
ssh -i "E:\developpement\conf\pair de cle aws\ubuntu-poly-bot-key.pem" ubuntu@63.35.188.77
```

### Infos serveur

| Élément | Valeur |
|---------|--------|
| IP publique | `63.35.188.77` |
| User SSH | `ubuntu` |
| Clé SSH | `E:\developpement\conf\pair de cle aws\ubuntu-poly-bot-key.pem` |
| Répertoire app | `~/poly_discover` |
| Port poly-discover | **4000** |
| Port poly-bot (autre projet) | **3000** |
| PM2 service name | `poly-discover` |
| Base de données | `~/poly_discover/data/discovery.db` |

### CI/CD — GitHub Actions

Le workflow `.github/workflows/build-deploy.yml` se déclenche sur push vers `main` ou `master` (+ dispatch manuel).

**Pipeline :** Build Rust + frontend dans GitHub Actions → SCP archive → Deploy via SSH → PM2 restart sur port 4000.

**Secrets GitHub requis** (Settings → Secrets → Actions) :
- `AWS_SSH_PRIVATE_KEY` — Contenu complet du fichier `.pem`
- `AWS_HOST` — `63.35.188.77`

### Commandes utiles sur le serveur

```bash
# Statut des services
pm2 status

# Logs poly-discover
pm2 logs poly-discover --lines 50

# Redémarrer le service
pm2 restart poly-discover

# Vérifier le port 4000
ss -tlnp | grep 4000

# Tester l'API
curl http://localhost:4000/api/health

# Recréer le service manuellement
pm2 delete poly-discover
pm2 start ~/poly_discover/poly-discover --name poly-discover -- serve --port 4000 --host 0.0.0.0
pm2 save
```

### Security Group AWS

Ports à ouvrir dans le Security Group EC2 :

| Port | Usage |
|------|-------|
| 22 | SSH (0.0.0.0/0 pour GitHub Actions) |
| 80 | Nginx (optionnel) |
| 3000 | poly-bot (autre projet) |
| 4000 | poly-discover |

## Historique des changements récents

### Web-Researched Polymarket Strategies (2026-02-09)

**Nouvelle feature** : catalogue statique de 12 strategies specifiques aux marches de prediction Polymarket, decouvertes via recherche internet. 5 sont backtestables (compatibles moteur actuel), 7 sont display-only.

**Nouveau module `crates/engine/src/web_strategies.rs` (~600 lignes) :**
- `WebStrategyId` enum (5 variants : ProbabilityEdge, CatalystMomentum, FavoriteCompounder, MarketMakingSim, MeanReversionPoly)
- `WebStrategyParams` enum (5 variants avec params specifiques par strategie)
- `WebStrategyCatalogEntry` struct + `get_catalog()` → 12 entrees (5 backtestables + 7 display-only)
- 5 `SignalGenerator` implementations : `ProbabilityEdgeGenerator`, `CatalystMomentumGenerator`, `FavoriteCompounderGenerator`, `MarketMakingSimGenerator`, `MeanReversionPolyGenerator`
- `build_web_generator(id, params)` : factory function
- Variantes de parametres : `default_for()`, `aggressive_for()`, `conservative_for()`, `random_for(rng)` par strategie
- 8 tests unitaires

**Nouveau fichier frontend `src/pages/StrategyResearch.svelte` :**
- Page catalogue des 12 strategies en cards avec filtre par categorie
- Sections separees backtestables vs display-only
- Badges categorie, risk level, backtestable
- Description FR + rationale pour chaque strategie

**Fichiers modifies (8) :**
- `crates/engine/src/discovery.rs` — +variant `WebStrategy { id, params }` dans `DiscoveryStrategyType`, `estimate_poly_probability()` rendue publique, +15 entrees dans `generate_phase1_grid()`, +match arms dans `mutate_strategy()`, `crossover_strategies()`, `generate_random_strategies()` (10% WebStrategy), `generate_refinement_grid()`, `result_to_record()`, `name()`
- `crates/engine/src/indicators.rs` — +match arm WebStrategy dans `build_signal_generator()`, `close_f64()` rendue publique
- `crates/engine/src/lib.rs` — +`pub mod web_strategies` + re-exports (`WebStrategyId`, `WebStrategyParams`, `WebStrategyCatalogEntry`, `get_catalog`)
- `crates/server/src/main.rs` — +endpoint `GET /api/strategies/catalog`
- `src/lib/api.js` — +`getStrategyCatalog()`
- `src/App.svelte` — +import StrategyResearch + route `strategy-research`
- `src/components/Sidebar.svelte` — +item "Strategies Web" (icone Globe, couleur sky)
- `src/pages/KnowledgeBase.svelte` — +`<option value="web_strategy">Web Strategies</option>` dans le filtre
- `src/pages/Discovery.svelte` — +case `web_strategy` dans `formatDiscoveryParams()`
- `CLAUDE.md` — documentation complete

**Tests : 67 total (+8 nouveaux)** — `test_catalog_has_12_entries`, `test_catalog_unique_ids`, `test_web_strategy_id_display_names`, `test_probability_edge_produces_signals`, `test_catalyst_momentum_produces_signals`, `test_market_making_sim_produces_signals`, `test_mean_reversion_poly_produces_signals`, `test_param_variants_differ`

---

### Persistence Leaderboard + Trade Watcher (2026-02-09)

**Deux nouvelles features** : persistence des analyses leaderboard en DB + Trade Watcher pour surveiller les trades des top traders en temps réel.

**Partie 1 — Persistence Leaderboard :**
- **Nouvelle table `leaderboard_traders`** (17 colonnes) : proxy_wallet (UNIQUE), user_name, rank, pnl, volume, portfolio_value, primary_strategy, primary_confidence, strategies_json, metrics_json, top_positions_json, trade_count, unique_markets, win_rate, avg_entry_price, analyzed_at
- **Nouvelle table `trader_trades`** (15 colonnes) : proxy_wallet, trade_hash (UNIQUE, SHA256 pour déduplications), side, condition_id, asset, size, price, title, outcome, event_slug, timestamp, transaction_hash, alerted (0/1), created_at
- `analyze_leaderboard()` accepte désormais `db_pool: Option<SqlitePool>` et persiste chaque trader + ses trades en DB après analyse
- Nouvel endpoint `GET /api/leaderboard/traders` pour lire les traders depuis la DB sans re-analyser
- Le frontend charge les traders persistés au mount de la page (si pas d'analyse en cours)

**Partie 2 — Trade Watcher :**
- Nouveau module `crates/engine/src/watcher.rs` : `WatcherProgress`, `WatcherStatus` (Idle/Watching/Error), `TradeAlert`, `run_trade_watcher()`
- Polling REST toutes les 15s pour chaque wallet surveillé via `GET /trades?user={wallet}`
- Détection de nouveaux trades par comparaison avec la DB (déduplications par trade_hash SHA256)
- Alertes in-memory (50 dernières) + trades complets en DB
- 3 nouveaux endpoints : `POST /api/watcher/start`, `POST /api/watcher/stop`, `GET /api/watcher/status`
- Frontend : section "Trade Watcher" dans Leaderboard.svelte avec bouton Start/Stop, badge LIVE, feed d'alertes en temps réel (polling 5s)

**Nouveaux fichiers (2) :**
- `crates/persistence/src/repository/leaderboard.rs` — `LeaderboardTraderRecord`, `TraderTradeRecord`, `LeaderboardRepository` (save_trader_analysis, get_all_traders, get_watched_wallets, save_trades, get_new_trades, mark_alerted, get_unalerted_trades)
- `crates/engine/src/watcher.rs` — `WatcherStatus`, `WatcherProgress`, `TradeAlert`, `run_trade_watcher()`, `check_new_trades()`

**Fichiers modifiés (8) :**
- `crates/persistence/src/schema.rs` — +2 CREATE TABLE + 2 indexes
- `crates/persistence/src/repository/mod.rs` — +`pub mod leaderboard` + re-export
- `crates/engine/src/leaderboard.rs` — +`db_pool: Option<SqlitePool>` param, +`compute_trade_hash()`, +`trades_to_records()`, +`analysis_to_record()`, persistence après chaque trader
- `crates/engine/src/lib.rs` — +`pub mod watcher` + re-exports (WatcherProgress, WatcherStatus, TradeAlert, run_trade_watcher)
- `crates/server/src/main.rs` — +`WatcherProgress` dans AppState, +4 endpoints (watcher/start, watcher/stop, watcher/status, leaderboard/traders), passage db_pool à analyze_leaderboard
- `src/lib/api.js` — +`getLeaderboardTraders()`, +`startWatcher()`, +`stopWatcher()`, +`getWatcherStatus()`
- `src/pages/Leaderboard.svelte` — chargement des traders depuis DB au mount, section Trade Watcher (Start/Stop, LIVE badge, feed d'alertes avec timestamp)
- `CLAUDE.md` — documentation complète

---

### Leaderboard Analyzer — Analyse des top traders Polymarket (2026-02-08)

**Nouvelle feature** : page d'analyse du leaderboard Polymarket. Récupère les top 10 traders, analyse leurs positions et trades, puis infère leur stratégie (Momentum, Contrarian, Scalper, Market Maker, Arbitrage, Event-Driven, High Conviction, Diversified, Mixed).

**Nouveaux fichiers (3) :**
- `crates/engine/src/api/polymarket.rs` — Client HTTP Polymarket Data API (`data-api.polymarket.com`), structs de désérialisation (`LeaderboardEntry`, `TraderPosition`, `TraderTrade`, `TraderValue`), méthodes `get_leaderboard()`, `get_positions()`, `get_trades()`, `get_value()`
- `crates/engine/src/leaderboard.rs` — Moteur d'analyse : `InferredStrategy` enum (9 variants), `TraderMetrics`, `StrategySignal`, `TraderAnalysis`, `LeaderboardProgress`/`LeaderboardStatus` (même pattern que `DiscoveryProgress`), fonctions `compute_metrics()`, `infer_strategy()`, `analyze_leaderboard()`, 6 tests unitaires
- `src/pages/Leaderboard.svelte` — Page frontend : bouton "Analyze Top 10", barre de progression (polling 2s), cards expandables par trader (rang, nom, PnL, volume, stratégies avec confidence bars, grille de métriques, top 5 positions, 10 derniers trades), couleurs par stratégie

**Fichiers modifiés (6) :**
- `crates/engine/src/api/mod.rs` — Ajout `pub mod polymarket` + re-export `PolymarketDataClient`
- `crates/engine/src/lib.rs` — Ajout `pub mod leaderboard` + re-exports (`PolymarketDataClient`, `analyze_leaderboard`, `LeaderboardProgress`, `LeaderboardStatus`, `TraderAnalysis`)
- `crates/server/src/main.rs` — `AppState` : +`polymarket` (PolymarketDataClient) et +`leaderboard_progress` (LeaderboardProgress). 2 nouveaux endpoints : `POST /api/leaderboard` (spawn background task) + `GET /api/leaderboard/status` (progress + résultats)
- `src/lib/api.js` — +`startLeaderboardAnalysis()` et `getLeaderboardStatus()`
- `src/App.svelte` — Import `Leaderboard` + route `leaderboard`
- `src/components/Sidebar.svelte` — Import `Crown` de lucide-svelte, nouvel item "Leaderboard" (couleur rose), ajout `rose` dans `colorMap`

**Règles d'inférence de stratégie :**

| Stratégie | Condition |
|-----------|-----------|
| Market Maker | BUY+SELL sur même conditionId (ratio > 0.3) |
| Scalper | trade_frequency > 20/jour, petites positions |
| Contrarian | avg_entry < 0.30 |
| Momentum | avg_entry > 0.55 |
| Arbitrage | Trades sur même event, différents conditions |
| Event-Driven | > 60% des trades dans < 20% du temps |
| High Conviction | < 10 marchés, concentration top 3 > 60% |
| Diversified | > 50 marchés, concentration top 3 < 20% |

**Tests : 59 total (+6 nouveaux)** — `test_compute_metrics_empty`, `test_infer_strategy_insufficient_data`, `test_infer_contrarian`, `test_infer_momentum`, `test_infer_market_maker`, `test_infer_high_conviction`

**Notes techniques :**
- API publique, aucune authentification requise
- Rate limiting : 200ms entre chaque requête (3 requêtes par trader × 10 traders ≈ 10s)
- Pas de persistence DB : résultats en mémoire (`LeaderboardProgress`), analyse à la demande
- Serde `rename_all = "camelCase"` pour les réponses API Polymarket

---

### Dynamic Combo Discovery — Exploration de combinaisons d'indicateurs 2/3/4 (2026-02-08)

**Refonte majeure** du système de découverte : passage des stratégies hardcodées (10 singles + 11 combos) à l'exploration dynamique de **toutes les combinaisons** possibles des 10 indicateurs techniques.

**Nouveaux types (`crates/engine/src/discovery.rs`) :**
- `SingleIndicatorType` enum (10 variants : Rsi, BollingerBands, Macd, EmaCrossover, Stochastic, AtrMeanReversion, Vwap, Obv, WilliamsR, Adx)
- `IndicatorParams` enum (10 variants avec params spécifiques par indicateur)
- `DynCombineMode` enum (Unanimous, Majority, PrimaryConfirmed)
- `DiscoveryStrategyType::DynamicCombo { indicators, params, combine_mode }` — nouveau variant
- Méthodes : `default_params()`, `aggressive_params()`, `conservative_params()`, `random_params_for(rng)` sur `SingleIndicatorType`
- Nommage dynamique : `"RSI+MACD(M)"`, `"BB+Stoch+ADX(U)"`, etc.

**Grid generation réécrite :**
- `generate_phase1_grid()` : ~1743 DynamicCombo (405 paires + 1080 triples + 210 quads) + 48 Gabagool
- `generate_exploratory_grid()` : Cycle 1 = quads complets, Cycle 2 = mixed params + random, Cycle 3+ = random combos
- `generate_random_strategies()` : ne génère que des DynamicCombo (95%) + Gabagool (5%)
- `generate_random_dynamic_combo(n, rng)` : helper pour créer un combo aléatoire de taille n

**ML-Guided adapté :**
- `mutate_strategy()` : nouveau match arm DynamicCombo avec `perturb_indicator_params()`
- `crossover_strategies()` : crossover entre DynamicCombo du même set d'indicateurs
- `generate_refinement_grid()` : essaie les 2 autres modes + mute chaque indicateur

**Signal generator factory (`crates/engine/src/indicators.rs`) :**
- `build_single_generator(ind, params)` : nouvelle fonction qui construit un generator depuis `SingleIndicatorType` + `IndicatorParams`
- `build_signal_generator()` : nouveau match arm `DynamicCombo` → `ComboSignalGenerator::new()`

**Rétrocompatibilité :**
- Les anciens records en DB restent intacts (champ `strategy_type` = `"rsi"`, `"macd_rsi"`, etc.)
- Les nouveaux records utilisent `strategy_type = "dynamic_combo"` et `strategy_params` contient le JSON complet
- Les legacy single/combo variants sont toujours dans l'enum pour la dé-sérialisation

**Frontend (`src/pages/KnowledgeBase.svelte`) :**
- Ajout filtre `<option value="dynamic_combo">Dynamic Combos</option>` dans le dropdown stratégie

**Tests (53 total, +5 nouveaux) :**
- `test_dynamic_combo_naming` : vérifie le format `"RSI+MACD(M)"` / `"BB+Stoch+ADX(U)"`
- `test_dynamic_combo_backtest_runs` : exécute un backtest complet sur un DynamicCombo
- `test_dynamic_combo_mutation` : vérifie que la mutation produit un DynamicCombo valide
- `test_dynamic_combo_crossover` : vérifie le crossover entre DynamicCombo du même type
- `test_random_dynamic_combo_generation` : vérifie taille, pas de doublons d'indicateurs

---

### Page Playbook — Refonte : Top 3 par Win Rate + Guide Bot Polymarket (2026-02-08)

**Refonte complète** de la page Playbook : affiche les **3 meilleures stratégies dédupliquées par win rate** (via `getTopStrategies(3, 'win_rate')`), avec pour chacune :

1. **Tableau "Paramètres essentiels Polymarket"** (en FR) :
   - Marché cible, timeframe, source de données, sizing
   - Paramètres indicateurs (période, seuils, etc.)
   - Signaux d'action : Signal ACHAT → Buy YES, Signal VENTE → Buy NO
   - Fonctions : `getPolymarketParams()`, `getIndicatorParams()`, `getSignalParams()`

2. **"Guide d'implémentation bot"** ultra-détaillé (7 sections en FR) :
   - SOURCE DE DONNÉES (API Binance klines 15min)
   - CALCUL DES INDICATEURS (formules détaillées via `getDetailedIndicatorCalc()`)
   - LOGIQUE DE SIGNAUX (pseudo-code via `getDetailedSignalLogic()`)
   - EXÉCUTION SUR POLYMARKET (Buy YES / Buy NO)
   - GESTION DES FRAIS (formule Polymarket)
   - GESTION DU RISQUE (drawdown, stop-loss)
   - BOUCLE PRINCIPALE DU BOT (architecture)
   - Bouton "Copy" pour copier le guide complet

- Médailles 1er/2e/3e avec headers colorés (or/argent/bronze)
- Couvre les 22 stratégies (10 single, 11 combos, 1 Gabagool)

**Changements backend (inchangés depuis v1) :**
- `crates/persistence/src/repository/discovery.rs` : `get_top_unique_strategies(limit, sort_by)`
- `crates/persistence/src/schema.rs` : index `idx_discovery_name_trades(strategy_name, total_trades)`
- `crates/server/src/main.rs` : endpoint `/api/knowledge/top-strategies` avec `sort_by` query param

**KnowledgeBase — Infobulles FR :**
- `src/pages/KnowledgeBase.svelte` : attributs `title` sur tous les headers de colonnes, descriptions en français

**Réduction des intervalles de polling :**
- Discovery status polling : 2s → 30s
- KnowledgeBase auto-refresh : 5s → 60s
- TopStrategies auto-refresh : 5s → 60s

---

### Page Top 20 Strategies — Classement dédupliqué par win rate (2026-02-08)

**Nouvelle page** affichant les 20 meilleures stratégies uniques par win rate, dédupliquées (1 ligne = 1 strategy_name).

**Changements backend :**
- `crates/persistence/src/repository/discovery.rs` : nouvelle méthode `get_top_unique_strategies(limit)` — requête SQL avec CTE + `ROW_NUMBER() OVER (PARTITION BY strategy_name)`, filtre `total_trades >= 5`
- `crates/server/src/main.rs` : nouveau endpoint `GET /api/knowledge/top-strategies` avec paramètre optionnel `limit` (default 20)

**Changements frontend :**
- `src/lib/api.js` : nouvelle fonction `getTopStrategies(limit)`
- `src/pages/TopStrategies.svelte` : nouvelle page — podium visuel top 3 (or/argent/bronze), tableau complet avec rank/strategy/symbol/win rate/PnL/confidence/ann. return/sortino/sharpe/drawdown/trades/params, auto-refresh 5s quand discovery tourne, badge LIVE
- `src/App.svelte` : routing `top-strategies`
- `src/components/Sidebar.svelte` : nouvel item "Top 20" avec icône Trophy (couleur yellow), import `Trophy` de lucide-svelte

---

### Backtester Avancé — Probabilités, Métriques Annualisées, IHM Enrichie (2026-02-07)

**Nouvelles fonctionnalités :**
1. Période de backtest étendue à **365 jours** (défaut, au lieu de 90)
2. **Score de confiance** par stratégie (0-100%) basé sur analyse par quartiles temporels
3. **Métriques avancées** : Sortino ratio, max pertes consécutives, avg win/loss PnL, volume total, return annualisé, Sharpe annualisé
4. **Scoring amélioré** avec bonus confiance, bonus Sortino, pénalité séries de pertes
5. **IHM enrichie** avec badges de confiance colorés (vert/jaune/rouge), nouvelles colonnes dans KnowledgeBase

**Changements backend :**
- `crates/engine/src/discovery.rs` :
  - `default_days()` : 90 → 365, `days_variants` : `[30,60,90,180,365]`
  - `GenericBacktestResult` : +7 champs (sortino, max_consecutive_losses, avg_win/loss, volume, annualized_return, annualized_sharpe)
  - `DiscoveryResult` : +8 champs (idem + strategy_confidence)
  - `calculate_sortino()` : nouveau — mean/downside_deviation
  - `calculate_strategy_confidence()` : nouveau — backtest sur 4 quartiles
  - `score_result()` : +confidence_bonus, +sortino_bonus, -streak_penalty
  - `result_to_record()` / `record_to_result()` : mapping des 8 nouveaux champs
- `crates/persistence/src/schema.rs` : 8 ALTER TABLE migrations (idempotentes)
- `crates/persistence/src/lib.rs` : `run_migrations()` tolère "duplicate column name"
- `crates/persistence/src/repository/discovery.rs` : +8 champs Option dans Record, INSERT/SELECT mis à jour, 3 nouveaux tris (confidence, annualized_return, sortino)
- `crates/server/src/main.rs` : CLI default 365j, export JSON inclut les nouvelles métriques

**Changements frontend :**
- `src/pages/Discovery.svelte` : default 365j, badges confiance (pill coloré + barre), ann. return, Sortino, max loss streak dans les résultats
- `src/pages/KnowledgeBase.svelte` : 3 nouvelles colonnes (Confidence avec barre, Ann. Return, Sortino), 3 nouveaux tris

**DB migration** : 8 nouvelles colonnes ajoutées automatiquement (DEFAULT '0'), rétro-compatible avec les anciens records.

---

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
