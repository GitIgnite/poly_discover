# Plan — Profile Analyzer Polymarket

## Objectif

Créer une fonctionnalité d'**analyse de profil Polymarket** : l'utilisateur entre un **nom d'utilisateur Polymarket** (ex: `Fredi9999`, `SilverBullet`), le système résout le username en `proxyWallet` via l'API leaderboard, puis récupère **toute l'activité** sur ~1 an et produit une analyse complète :
- Positions actuelles (ouvertes)
- Positions fermées (historique)
- Tous les trades, regroupés par marché/event
- Stratégie inférée par marché et globale
- Métriques PnL, volume, win rate, etc.

### Flux de résolution Username → Wallet

1. L'utilisateur saisit un **nom d'utilisateur** Polymarket dans l'interface
2. Le backend appelle `GET /v1/leaderboard?userName={username}` sur `data-api.polymarket.com`
3. L'API retourne le `proxyWallet` associé au username
4. Toutes les requêtes suivantes utilisent le `proxyWallet` comme identifiant `user=`

---

## Phase 1 — Étendre le client API Polymarket

**Fichier:** `crates/engine/src/api/polymarket.rs`

### 1.1 Résolution username → wallet (nouvel endpoint)

| Endpoint | Méthode | But |
|----------|---------|-----|
| `/v1/leaderboard?userName={name}` | GET | Résoudre un username en `proxyWallet` |

La méthode `resolve_username(username: &str) -> Result<(String, String)>` retourne `(proxyWallet, userName)`.
Si le username n'est pas trouvé, retourne une erreur claire.

### 1.2 Nouveaux endpoints Data API (`data-api.polymarket.com`)

| Endpoint | Méthode | Pagination | Max Limit |
|----------|---------|------------|-----------|
| `/activity?user=` | GET | offset/limit | 500 |
| `/closed-positions?user=` | GET | offset/limit | 50 |
| `/positions?user=` | GET (existe déjà, améliorer pagination) | offset/limit | 500 |
| `/trades?user=` | GET (existe déjà, améliorer pagination) | offset/limit | 10000 |
| `/value?user=` | GET (existe déjà) | — | — |

### 1.3 Nouveau client Gamma API (`gamma-api.polymarket.com`)

| Endpoint | Méthode | But |
|----------|---------|-----|
| `GET /markets?condition_ids=` | Batch | Metadata marchés (titre, catégorie, tags, event) |
| `GET /events?id=` | Batch | Metadata events (titre, catégorie, dates) |

### 1.4 Nouvelles structs de données

```rust
// Activity
struct UserActivity {
    proxy_wallet: String,
    timestamp: i64,
    condition_id: String,
    activity_type: String,  // TRADE, SPLIT, MERGE, REDEEM, REWARD, etc.
    size: f64,
    usdc_size: f64,
    transaction_hash: String,
    price: f64,
    asset: String,
    side: String,
    outcome_index: u32,
    title: String,
    slug: String,
    event_slug: String,
    outcome: String,
}

// Closed Position (nouveau)
struct ClosedPosition {
    proxy_wallet: String,
    asset: String,
    condition_id: String,
    avg_price: f64,
    total_bought: f64,
    realized_pnl: f64,
    cur_price: f64,
    timestamp: i64,
    title: String,
    slug: String,
    event_slug: String,
    outcome: String,
    outcome_index: u32,
    end_date: Option<String>,
}

// Market metadata (Gamma API)
struct MarketInfo {
    condition_id: String,
    question: String,
    category: String,
    slug: String,
    event_slug: String,
    end_date: Option<String>,
    closed: bool,
    liquidity: f64,
    volume: f64,
    outcomes: Vec<String>,
    outcome_prices: Vec<f64>,
}

// Event metadata (Gamma API)
struct EventInfo {
    id: String,
    title: String,
    slug: String,
    category: String,
    closed: bool,
    volume: f64,
    liquidity: f64,
    start_date: Option<String>,
    end_date: Option<String>,
    markets: Vec<String>,  // condition_ids
}
```

### 1.5 Pagination générique

Créer une fonction helper `fetch_all_paginated<T>()` qui :
- Appelle l'endpoint avec `offset=0, limit=MAX`
- Si le résultat contient `limit` items, incrémente `offset` et recommence
- Continue jusqu'à ce que `response.len() < limit`
- Respecte le rate limit (200ms entre chaque requête)
- Retourne `Vec<T>` complet

---

## Phase 2 — Nouveau module Profile Analyzer

**Nouveau fichier:** `crates/engine/src/profile.rs`

### 2.1 Structures de données d'analyse

```rust
/// Résumé d'un marché avec tous les trades regroupés
struct MarketAnalysis {
    condition_id: String,
    title: String,
    event_slug: String,
    category: String,
    outcome: String,

    // Trades sur ce marché
    trades: Vec<TraderTrade>,
    trade_count: usize,
    first_trade: i64,   // timestamp
    last_trade: i64,

    // Métriques calculées
    total_bought: f64,
    total_sold: f64,
    net_position: f64,
    avg_buy_price: f64,
    avg_sell_price: f64,
    realized_pnl: f64,
    volume: f64,

    // Stratégie inférée pour CE marché
    inferred_strategy: MarketStrategy,
}

enum MarketStrategy {
    Scalping,           // Nombreux petits trades rapides
    Momentum,           // Achats progressifs dans la tendance
    Contrarian,         // Achat à bas prix, contre le consensus
    MarketMaking,       // Buy + Sell alternés, spread capture
    EventDriven,        // Trades concentrés autour de dates clés
    HoldToResolution,   // Achète et attend la résolution
    SwingTrading,       // Quelques trades sur plusieurs jours
    Accumulation,       // Achats réguliers, peu de ventes
    DCA,                // Dollar-Cost Averaging (achats réguliers même montant)
}

/// Analyse complète du profil
struct ProfileAnalysis {
    wallet: String,
    username: Option<String>,
    analyzed_at: i64,

    // Vue d'ensemble
    portfolio_value: f64,
    total_pnl: f64,
    total_volume: f64,
    total_trades: usize,
    unique_markets: usize,
    win_rate: f64,       // % de marchés profitables

    // Positions
    open_positions: Vec<TraderPosition>,
    closed_positions: Vec<ClosedPosition>,

    // Analyse par marché (regroupement principal)
    markets: Vec<MarketAnalysis>,

    // Analyse par catégorie (CRYPTO, POLITICS, etc.)
    category_breakdown: HashMap<String, CategoryStats>,

    // Timeline d'activité
    activity_timeline: Vec<ActivityPeriod>,

    // Stratégie globale inférée
    primary_strategy: MarketStrategy,
    strategy_confidence: f64,
    strategy_signals: Vec<StrategySignal>,

    // Métriques avancées
    avg_hold_duration: f64,    // durée moyenne position
    best_trade_pnl: f64,
    worst_trade_pnl: f64,
    sharpe_ratio: f64,
    max_drawdown: f64,
    active_days: usize,
    avg_position_size: f64,
}

struct CategoryStats {
    category: String,
    trade_count: usize,
    volume: f64,
    pnl: f64,
    win_rate: f64,
    market_count: usize,
}

struct ActivityPeriod {
    date: String,       // YYYY-MM-DD
    trade_count: usize,
    volume: f64,
    pnl: f64,
}
```

### 2.2 Fonction principale `analyze_profile()`

```
async fn analyze_profile(
    username: String,          // ← nom d'utilisateur Polymarket (pas wallet)
    progress: Arc<ProfileProgress>,
    poly_client: Arc<PolymarketDataClient>,
    db: Arc<Database>,
) -> Result<ProfileAnalysis>
```

**Étapes :**

1. **Résoudre le username** → `GET /v1/leaderboard?userName=` → obtient `proxyWallet`
2. **Fetch portfolio value** → `/value?user={proxyWallet}`
3. **Fetch ALL trades** (paginé, limit=10000) → `/trades?user={proxyWallet}`
4. **Fetch positions ouvertes** (paginé, limit=500) → `/positions?user={proxyWallet}`
5. **Fetch positions fermées** (paginé, limit=50) → `/closed-positions?user={proxyWallet}`
6. **Fetch activity** (paginé, limit=500) → `/activity?user={proxyWallet}`
7. **Extraire les condition_ids uniques** des trades
8. **Fetch market metadata** (batch, Gamma API) → `/markets?condition_ids=`
9. **Grouper les trades par marché** (condition_id)
10. **Pour chaque marché** : calculer métriques + inférer stratégie
11. **Agréger par catégorie** (CRYPTO, POLITICS, etc.)
12. **Construire la timeline** d'activité (trades par jour)
13. **Inférer la stratégie globale** du profil
14. **Persister en DB**

### 2.3 Inférence de stratégie par marché

Logique pour déterminer `MarketStrategy` sur un marché :

| Critère | Stratégie |
|---------|-----------|
| `buy_count > 0 && sell_count > 0 && sell_ratio > 0.3` | MarketMaking |
| `trade_count > 10 && avg_duration < 1h` | Scalping |
| `buy_count > 3 && sell_count == 0 && similar_sizes` | DCA |
| `buy_count > 1 && sell_count == 0` | Accumulation |
| `sell_count == 0 && trade_count <= 2` | HoldToResolution |
| `avg_buy_price < 0.30` | Contrarian |
| `avg_buy_price > 0.70` | Momentum |
| `trades_clustered_in_time (>60% in <20% of span)` | EventDriven |
| `trade_count <= 5 && span > 7 days` | SwingTrading |

### 2.4 Progress tracking

Même pattern que `LeaderboardProgress` :

```rust
struct ProfileProgress {
    status: RwLock<ProfileStatus>,
    total_steps: AtomicU32,
    completed_steps: AtomicU32,
    current_step: RwLock<String>,
    result: RwLock<Option<ProfileAnalysis>>,
    error: RwLock<Option<String>>,
    cancelled: AtomicBool,
}

enum ProfileStatus {
    Idle,
    ResolvingUsername,      // ← Étape 1 : username → proxyWallet
    FetchingTrades,
    FetchingPositions,
    FetchingClosedPositions,
    FetchingActivity,
    FetchingMarketData,
    AnalyzingMarkets,
    Complete,
    Error,
}
```

---

## Phase 3 — Persistence (DB)

**Fichier:** `crates/persistence/src/schema.rs` + nouveau repository

### 3.1 Nouvelle table `profile_analyses`

```sql
CREATE TABLE IF NOT EXISTS profile_analyses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet TEXT NOT NULL,
    username TEXT,
    portfolio_value REAL DEFAULT 0,
    total_pnl REAL DEFAULT 0,
    total_volume REAL DEFAULT 0,
    total_trades INTEGER DEFAULT 0,
    unique_markets INTEGER DEFAULT 0,
    win_rate REAL DEFAULT 0,
    primary_strategy TEXT,
    strategy_confidence REAL DEFAULT 0,
    open_positions_json TEXT,      -- JSON sérialisé
    closed_positions_json TEXT,    -- JSON sérialisé
    markets_json TEXT,             -- JSON sérialisé (MarketAnalysis[])
    category_breakdown_json TEXT,  -- JSON sérialisé
    activity_timeline_json TEXT,   -- JSON sérialisé
    strategy_signals_json TEXT,    -- JSON sérialisé
    avg_hold_duration REAL DEFAULT 0,
    best_trade_pnl REAL DEFAULT 0,
    worst_trade_pnl REAL DEFAULT 0,
    sharpe_ratio REAL DEFAULT 0,
    max_drawdown REAL DEFAULT 0,
    active_days INTEGER DEFAULT 0,
    avg_position_size REAL DEFAULT 0,
    analyzed_at TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_profile_wallet ON profile_analyses(wallet);
```

### 3.2 Nouvelle table `profile_trades`

```sql
CREATE TABLE IF NOT EXISTS profile_trades (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet TEXT NOT NULL,
    trade_hash TEXT UNIQUE NOT NULL,
    side TEXT NOT NULL,
    condition_id TEXT NOT NULL,
    asset TEXT,
    size REAL NOT NULL,
    price REAL NOT NULL,
    title TEXT,
    outcome TEXT,
    event_slug TEXT,
    timestamp INTEGER NOT NULL,
    transaction_hash TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_profile_trades_wallet ON profile_trades(wallet, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_profile_trades_market ON profile_trades(wallet, condition_id);
```

### 3.3 Repository `ProfileRepository`

Méthodes :
- `save_analysis(record)` — upsert par wallet
- `get_analysis(wallet)` — retourne la dernière analyse
- `get_all_analyses()` — liste toutes les analyses
- `save_trades(trades)` — batch insert avec dédup par trade_hash
- `get_trades_by_market(wallet, condition_id)` — trades groupés par marché
- `delete_analysis(wallet)` — supprimer une analyse

---

## Phase 4 — Endpoints Server

**Fichier:** `crates/server/src/main.rs`

### 4.1 Nouveaux endpoints

| Méthode | Path | But |
|---------|------|-----|
| `POST` | `/api/profile/analyze` | Lancer l'analyse d'un username Polymarket |
| `GET` | `/api/profile/status` | Polling du progrès |
| `POST` | `/api/profile/cancel` | Annuler l'analyse en cours |
| `GET` | `/api/profile/{wallet}` | Récupérer l'analyse persistée (par wallet résolu) |
| `GET` | `/api/profile/history` | Liste des analyses passées |
| `DELETE` | `/api/profile/{wallet}` | Supprimer une analyse |

### 4.2 Body du POST `/api/profile/analyze`

```json
{
  "username": "Fredi9999"
}
```

Le backend résout le username en `proxyWallet` via `/v1/leaderboard?userName=` avant de lancer l'analyse.
Si le username n'est pas trouvé, retourne `404 { "error": "User not found: Fredi9999" }`.

### 4.3 AppState

Ajouter `profile_progress: Arc<ProfileProgress>` dans `AppState`.

---

## Phase 5 — Frontend : Page Profile Analysis

**Nouveau fichier:** `src/pages/ProfileAnalysis.svelte`

### 5.1 Interface utilisateur

#### Zone de saisie (header)
- Input texte pour le **nom d'utilisateur Polymarket** (placeholder: "Entrez un username Polymarket...")
- Bouton "Analyze"
- Historique des analyses récentes (dropdown ou chips cliquables avec username)

#### Barre de progression
- Status textuel (Fetching trades... Analyzing markets...)
- Barre de progression avec % (steps complétées)
- Bouton Cancel

#### Vue résultats — 4 onglets/sections

**A) Overview (résumé)**
- Cards métriques : Portfolio Value, Total PnL, Volume, Win Rate, Trades, Markets
- Stratégie principale avec badge + confidence bar
- Graphique camembert catégories (CRYPTO, POLITICS, etc.)

**B) Positions**
- Tableau positions ouvertes (title, size, avg price, current price, PnL, %)
- Tableau positions fermées (title, avg price, realized PnL, date)
- Tri par PnL, taille, date

**C) Markets (analyse par marché)** — Section principale
- Cards par marché, triables/filtrables
- Chaque card : titre du marché, catégorie badge, nombre de trades, volume, PnL
- Stratégie inférée par marché (badge coloré)
- Expandable : détail des trades, timeline, buy/sell breakdown
- Filtres : par catégorie, par stratégie, par PnL (positif/négatif)

**D) Activity Timeline**
- Graphique barres : volume/trades par jour sur la période analysée
- Hover : détails du jour (trades, PnL, marchés actifs)

### 5.2 Navigation

Ajouter dans `Sidebar.svelte` :
- Item "Profile" avec icône `UserSearch` ou `ScanSearch` (lucide-svelte)
- Couleur : violet/purple

### 5.3 API functions

Dans `src/lib/api.js` :
```javascript
startProfileAnalysis(username)   // POST /api/profile/analyze { username }
getProfileStatus()               // GET /api/profile/status
cancelProfileAnalysis()          // POST /api/profile/cancel
getProfileAnalysis(wallet)       // GET /api/profile/{wallet}
getProfileHistory()              // GET /api/profile/history
deleteProfileAnalysis(wallet)    // DELETE /api/profile/{wallet}
```

---

## Phase 6 — Mise à jour CLAUDE.md + Tests

### 6.1 Tests unitaires à ajouter

- `test_username_resolution` — résolution username → wallet
- `test_market_strategy_inference` — chaque type de stratégie
- `test_trade_grouping_by_market` — regroupement correct
- `test_category_breakdown` — agrégation par catégorie
- `test_activity_timeline` — calcul timeline
- `test_win_rate_calculation` — win rate correct
- `test_pagination_helper` — pagination complète
- `test_profile_progress_tracking` — states transitions

### 6.2 Mise à jour CLAUDE.md

- Ajouter la section Profile Analysis
- Mettre à jour les endpoints API
- Mettre à jour l'architecture frontend
- Mettre à jour le compteur de tests

---

## Ordre d'implémentation

1. **Phase 1** — API client (endpoints + pagination + structs)
2. **Phase 3** — DB schema + repository (nécessaire pour Phase 2)
3. **Phase 2** — Profile analyzer (logique métier)
4. **Phase 4** — Server endpoints
5. **Phase 5** — Frontend page
6. **Phase 6** — Tests + CLAUDE.md

---

## Ce qu'on garde / ce qu'on modifie

| Composant existant | Action |
|-------------------|--------|
| Discovery (backtest engine) | **Garde** — inchangé |
| Leaderboard | **Garde** — inchangé |
| Trade Watcher | **Garde** — inchangé |
| Polymarket API Client | **Étend** — nouveaux endpoints + pagination |
| Sidebar | **Étend** — nouvel item "Profile" |
| App.svelte | **Étend** — nouvelle route |
| api.js | **Étend** — nouvelles fonctions |
| DB schema | **Étend** — nouvelles tables |
| persistence lib | **Étend** — nouveau repository |

Aucune fonctionnalité existante n'est supprimée.
