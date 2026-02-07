# Documentation Complète Polymarket - Bot de Trading Automatisé

> Documentation technique fusionnée pour créer un bot de trading robuste sur Polymarket
> Sources: docs.polymarket.com + implémentations Python & Rust
> Dernière mise à jour: Février 2025

---

## Table des Matières

1. [Vue d'Ensemble](#1-vue-densemble)
2. [Architecture des APIs](#2-architecture-des-apis)
3. [Gamma API - Découverte des Marchés](#3-gamma-api---découverte-des-marchés)
4. [CLOB API - Trading](#4-clob-api---trading)
5. [Data API - Données Utilisateur](#5-data-api---données-utilisateur)
6. [WebSocket Market Channel](#6-websocket-market-channel)
7. [WebSocket User Channel](#7-websocket-user-channel)
8. [RTDS - Prix Crypto Temps Réel](#8-rtds---prix-crypto-temps-réel)
9. [Authentification](#9-authentification)
10. [Types d'Ordres](#10-types-dordres)
11. [Builder Program & Relayer](#11-builder-program--relayer)
12. [CTF - Conditional Token Framework](#12-ctf---conditional-token-framework)
13. [Negative Risk](#13-negative-risk)
14. [Marchés Crypto 15 Minutes](#14-marchés-crypto-15-minutes)
15. [Architecture du Bot](#15-architecture-du-bot)
16. [Implémentation Python](#16-implémentation-python)
17. [Implémentation Rust](#17-implémentation-rust)
18. [Stratégies de Trading](#18-stratégies-de-trading)
19. [Gestion des Risques](#19-gestion-des-risques)
20. [Best Practices Market Making](#20-best-practices-market-making)
21. [Liquidity Rewards](#21-liquidity-rewards)
22. [Frais](#22-frais)
23. [Smart Contracts](#23-smart-contracts)
24. [SDK et Ressources](#24-sdk-et-ressources)
25. [Glossaire](#25-glossaire)

---

## 1. Vue d'Ensemble

### 1.1 Qu'est-ce que Polymarket ?

Polymarket est la plus grande plateforme de marchés prédictifs décentralisée au monde. Elle permet de trader sur les résultats d'événements futurs en utilisant des tokens sur la blockchain Polygon.

### 1.2 Architecture Hybride

Polymarket utilise un modèle **hybrid-décentralisé** :

```
┌─────────────────────────────────────────────────────────────────┐
│                    FLUX DE TRADING                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. Utilisateur signe ordre (EIP-712)                          │
│           │                                                     │
│           ▼                                                     │
│  2. Ordre envoyé au CLOB (off-chain)                           │
│           │                                                     │
│           ▼                                                     │
│  3. Matching par l'opérateur                                   │
│           │                                                     │
│           ▼                                                     │
│  4. Settlement on-chain via Exchange contract                  │
│           │                                                     │
│           ▼                                                     │
│  5. Tokens transférés (non-custodial)                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 1.3 Concepts Clés

| Concept | Description |
|---------|-------------|
| **Event** | Collection de marchés liés (ex: "Élection présidentielle 2024") |
| **Market** | Outcome tradable binaire avec YES/NO ou UP/DOWN |
| **Token** | Représentation blockchain d'un outcome (ERC-1155) |
| **Token ID** | Identifiant unique du token outcome (très long nombre) |
| **Condition ID** | Identifiant on-chain pour résolution (0x...) |
| **Question ID** | Lien vers oracle UMA |
| **USDC/USDCe** | Collateral utilisé pour trader (sur Polygon, 6 décimales) |
| **Price** | Prix entre 0.00 et 1.00 = probabilité |
| **Tick Size** | Incrément prix minimum (0.01 ou 0.001) |

### 1.4 Fonctionnement des Prix

```
Prix YES à 0.65 = 65% de probabilité que l'événement se produise

Acheter YES à 0.65:
  • Si YES gagne → reçoit 1.00 USDC (profit: +0.35)
  • Si YES perd  → reçoit 0.00 USDC (perte: -0.65)

Règle fondamentale:
  Prix YES + Prix NO = 1.00 USDC (toujours)
```

### 1.5 Caractéristiques Clés

- **Non-custodial** : Les utilisateurs gardent le contrôle de leurs fonds
- **Gasless** : Via Builder Program et Relayer
- **Binary outcomes** : Marchés YES/NO tokenisés en ERC1155
- **Collateral** : USDCe sur Polygon

---

## 2. Architecture des APIs

### 2.1 Vue d'Ensemble

```
┌─────────────────────────────────────────────────────────────────────┐
│                     POLYMARKET API ECOSYSTEM                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐ │
│  │   GAMMA API     │  │    CLOB API     │  │     DATA API        │ │
│  │  (Métadonnées)  │  │    (Trading)    │  │  (Données User)     │ │
│  │   Public        │  │   L1/L2 Auth    │  │     Public          │ │
│  └────────┬────────┘  └────────┬────────┘  └──────────┬──────────┘ │
│           │                    │                      │            │
│  ┌────────▼────────────────────▼──────────────────────▼──────────┐ │
│  │                     WebSocket Channels                        │ │
│  │  • Market (public) - Orderbook temps réel                     │ │
│  │  • User (auth) - Ordres et trades personnels                  │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                         RTDS                                  │ │
│  │        Prix crypto temps réel (Binance/Chainlink)             │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                    RELAYER (Builder Program)                  │ │
│  │                   Transactions gasless                        │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                    POLYGON BLOCKCHAIN                         │ │
│  │              (Settlement & Smart Contracts CTF)               │ │
│  └───────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Tableau des APIs

| API | Base URL | Auth | Usage |
|-----|----------|------|-------|
| **CLOB API** | `https://clob.polymarket.com` | L1/L2 | Trading, orderbook, prix |
| **Gamma API** | `https://gamma-api.polymarket.com` | Non | Découverte marchés, metadata |
| **Data API** | `https://data-api.polymarket.com` | Non | Positions, activité utilisateur |
| **Relayer** | `https://relayer-v2.polymarket.com/` | Builder | Transactions gasless |

### 2.3 WebSocket Endpoints

| WebSocket | URL | Auth | Usage |
|-----------|-----|------|-------|
| **CLOB Market** | `wss://ws-subscriptions-clob.polymarket.com/ws/market` | Public | Orderbook temps réel |
| **CLOB User** | `wss://ws-subscriptions-clob.polymarket.com/ws/user` | API Key | Trades, ordres utilisateur |
| **RTDS** | `wss://ws-live-data.polymarket.com` | Public | Prix crypto Binance/Chainlink |

---

## 3. Gamma API - Découverte des Marchés

### 3.1 Endpoints Principaux

```bash
# Liste des événements actifs
GET /events?active=true&closed=false&limit=100&offset=0

# Événement par slug
GET /events/slug/{slug}

# Liste des marchés
GET /markets?closed=false&limit=100

# Marché par slug
GET /markets/slug/{slug}

# Tags/Catégories
GET /tags?limit=100

# Sports
GET /sports

# Recherche
GET /search?query={query}
```

### 3.2 Filtres Disponibles

| Paramètre | Description | Exemple |
|-----------|-------------|---------|
| `active` | Événements actifs | `true` |
| `closed` | Marchés fermés | `false` |
| `tag_id` | Filtrer par catégorie | `100639` |
| `tag_slug` | Filtrer par tag slug | `15-min` |
| `limit` | Résultats par page | `100` |
| `offset` | Pagination | `0` |
| `order` | Tri | `volume24hr`, `id` |
| `ascending` | Direction tri | `false` |

### 3.3 Structure d'un Event

```json
{
  "id": "event_id",
  "slug": "event-slug-url",
  "title": "Question principale",
  "negRisk": false,
  "enableNegRisk": false,
  "negRiskAugmented": false,
  "markets": [
    { /* Market object */ }
  ],
  "tags": [
    { "id": "123", "slug": "crypto", "label": "Crypto" }
  ]
}
```

### 3.4 Structure d'un Market

```json
{
  "id": "0x1234...",
  "question": "Bitcoin Up or Down - February 4, 3:45 PM ET?",
  "conditionId": "0xabcd...",
  "questionId": "0xefgh...",
  "slug": "btc-up-down-feb-4-345pm",
  "outcomes": ["Up", "Down"],
  "outcomePrices": ["0.55", "0.45"],
  "clobTokenIds": ["123456...", "789012..."],
  "tokens": [
    {
      "token_id": "123456789...",
      "outcome": "Up",
      "price": "0.55"
    },
    {
      "token_id": "987654321...",
      "outcome": "Down",
      "price": "0.45"
    }
  ],
  "volume": "125000",
  "volume24hr": "15000",
  "liquidity": "50000",
  "startDate": "2025-02-04T20:45:00Z",
  "endDate": "2025-02-04T21:00:00Z",
  "closed": false,
  "active": true,
  "neg_risk": false,
  "minimum_tick_size": "0.01"
}
```

### 3.5 Champs Importants pour le Bot

| Champ | Description | Usage |
|-------|-------------|-------|
| `clobTokenIds` ou `tokens[].token_id` | ID unique du token | **Requis pour trader** |
| `conditionId` | ID de la condition blockchain | Identification unique |
| `outcomePrices` | Prix actuels des outcomes | Analyse de marché |
| `minimum_tick_size` | Incrément minimum de prix | Validation des ordres |
| `endDate` | Date de résolution | Timing du trade |
| `neg_risk` | Type de marché | Configuration CLOB |
| `volume24hr` | Volume 24h | Sélection de marchés |
| `liquidity` | Liquidité disponible | Sélection de marchés |

---

## 4. CLOB API - Trading

### 4.1 Endpoints Publics (Sans Auth)

```bash
# Prix actuel d'un token
GET /price?token_id={TOKEN_ID}&side=BUY|SELL
Response: { "price": "0.65" }

# Orderbook complet
GET /book?token_id={TOKEN_ID}
Response: {
  "market": "condition_id",
  "asset_id": "token_id",
  "bids": [{"price": "0.64", "size": "100"}],
  "asks": [{"price": "0.66", "size": "150"}],
  "tick_size": "0.01",
  "min_order_size": "5"
}

# Prix midpoint
GET /midpoint?token_id={TOKEN_ID}
Response: { "mid": "0.65" }

# Spread
GET /spread?token_id={TOKEN_ID}
Response: { "spread": "0.02" }

# Dernier trade
GET /last-trade-price?token_id={TOKEN_ID}
Response: { "price": "0.65", "side": "BUY" }

# Historique prix
GET /prices-history?market={CONDITION_ID}&startTs={TS}&endTs={TS}&interval=max|1w|1d|6h|1h
```

### 4.2 Endpoints Trading (Auth L2)

```bash
# Placer un ordre
POST /order
Body: { "order": SignedOrder, "owner": "api_key", "orderType": "GTC|GTD|FOK|FAK" }

# Placer plusieurs ordres (max 15)
POST /orders
Body: [{ "order": SignedOrder, "owner": "api_key", "orderType": "GTC" }, ...]

# Annuler un ordre
DELETE /order
Body: { "orderID": "order_id" }

# Annuler plusieurs ordres
DELETE /orders
Body: ["order_id_1", "order_id_2"]

# Annuler tous les ordres
DELETE /cancel-all

# Annuler ordres d'un marché
DELETE /cancel-market-orders?market={CONDITION_ID}&asset_id={TOKEN_ID}

# Obtenir ordres ouverts
GET /orders?market={CONDITION_ID}

# Obtenir un ordre spécifique
GET /order/{ORDER_ID}

# Obtenir trades
GET /trades?market={CONDITION_ID}

# Vérifier si ordre est scoré (rewards)
GET /order-scoring?order_id={ORDER_ID}
Response: { "scoring": true }
```

### 4.3 Endpoints API Keys (Auth L1)

```bash
# Créer API key
POST /auth/api-key

# Dériver API key existante
POST /auth/derive-api-key
```

### 4.4 Structure d'un Ordre Signé

```json
{
  "salt": "random_number",
  "maker": "wallet_address",
  "signer": "signer_address",
  "taker": "0x0000...",
  "tokenId": "token_id",
  "makerAmount": "100000000",
  "takerAmount": "65000000",
  "expiration": "1707000000",
  "nonce": "0",
  "feeRateBps": "0",
  "side": "BUY",
  "signatureType": 2,
  "signature": "0x..."
}
```

---

## 5. Data API - Données Utilisateur

### 5.1 Base URL

```
https://data-api.polymarket.com
```

### 5.2 Endpoints

```bash
# Positions d'un utilisateur
GET /positions?user={WALLET_ADDRESS}

# Activité utilisateur
GET /activity?user={WALLET_ADDRESS}

# Trades d'un utilisateur
GET /trades?user={WALLET_ADDRESS}&market={CONDITION_ID}

# Valeur totale des positions
GET /value?user={WALLET_ADDRESS}

# Top holders d'un marché
GET /top-holders?market={CONDITION_ID}
```

---

## 6. WebSocket Market Channel

### 6.1 Connection

```
URL: wss://ws-subscriptions-clob.polymarket.com/ws/market
```

**Limite** : Maximum **500 instruments** par connexion WebSocket.

### 6.2 Subscription

```json
{
  "type": "market",
  "assets_ids": ["token_id_1", "token_id_2"]
}
```

### 6.3 Message Types

#### 6.3.1 `book` - Orderbook Snapshot

Reçu à la subscription initiale et lors de trades affectant l'orderbook.

```json
{
  "event_type": "book",
  "asset_id": "token_id",
  "market": "condition_id",
  "timestamp": 1706000000000,
  "bids": [{"price": "0.64", "size": "100"}],
  "asks": [{"price": "0.66", "size": "150"}],
  "hash": "orderbook_hash"
}
```

#### 6.3.2 `price_change` - Changement de Prix

Émis lors de placement/annulation d'ordres.

```json
{
  "event_type": "price_change",
  "asset_id": "token_id",
  "changes": [
    {
      "price": "0.65",
      "size": "50",
      "side": "BUY",
      "order_hash": "hash"
    }
  ],
  "best_bid": "0.64",
  "best_ask": "0.66"
}
```

> **Note** : Le schema `price_change` sera mis à jour le 15 septembre 2025 à 23h UTC.

#### 6.3.3 `tick_size_change` - Changement Tick Size

Émis quand le prix atteint les bornes (>0.96 ou <0.04).

```json
{
  "event_type": "tick_size_change",
  "asset_id": "token_id",
  "old_tick_size": "0.01",
  "new_tick_size": "0.001",
  "side": "BUY",
  "timestamp": 1706000000000
}
```

#### 6.3.4 `last_trade_price` - Dernier Trade

Émis lors d'un match maker/taker.

```json
{
  "event_type": "last_trade_price",
  "asset_id": "token_id",
  "price": "0.65",
  "side": "BUY",
  "size": "25",
  "fee_rate_bps": "0",
  "timestamp": 1706000000000
}
```

---

## 7. WebSocket User Channel

### 7.1 Connection (Authentifiée)

```
URL: wss://ws-subscriptions-clob.polymarket.com/ws/user
```

Requiert authentification via API key.

### 7.2 Subscription

```json
{
  "type": "user",
  "auth": {
    "apiKey": "your_api_key",
    "secret": "your_secret",
    "passphrase": "your_passphrase"
  }
}
```

### 7.3 Message Types

#### 7.3.1 Trade Messages

Dispatché lors de :
- Exécution d'ordres marché (MATCHED)
- Fill d'ordres limite
- Transitions de statut (MINED, CONFIRMED, RETRYING, FAILED)

```json
{
  "event_type": "trade",
  "trade_id": "trade_id",
  "market": "condition_id",
  "asset_id": "token_id",
  "price": "0.65",
  "size": "100",
  "side": "BUY",
  "status": "CONFIRMED",
  "maker_orders": [...],
  "timestamp": 1706000000000
}
```

**Statuts de trade :**

| Status | Description |
|--------|-------------|
| `MATCHED` | Ordre matché off-chain |
| `MINED` | Transaction incluse dans un bloc |
| `CONFIRMED` | Transaction confirmée (final) |
| `RETRYING` | Retry en cours |
| `FAILED` | Échec de la transaction |

#### 7.3.2 Order Messages

Dispatché lors de :
- Placement d'ordres (PLACEMENT)
- Fills partiels (UPDATE)
- Annulations (CANCELLATION)

```json
{
  "event_type": "order",
  "order_id": "order_id",
  "asset_id": "token_id",
  "price": "0.65",
  "original_size": "100",
  "size_matched": "50",
  "outcome": "Yes",
  "side": "BUY",
  "status": "LIVE",
  "timestamp": 1706000000000
}
```

**Statuts d'ordre :**

| Status | Description |
|--------|-------------|
| `LIVE` | Ordre actif dans l'orderbook |
| `MATCHED` | Ordre complètement exécuté |
| `CANCELLED` | Ordre annulé |

---

## 8. RTDS - Prix Crypto Temps Réel

### 8.1 Connection

```
URL: wss://ws-live-data.polymarket.com
```

**Important** : Envoyer des messages PING toutes les 5 secondes pour maintenir la connexion.

### 8.2 Topics Disponibles

#### 8.2.1 Crypto Prices (Binance)

```json
{
  "action": "subscribe",
  "subscriptions": [{
    "topic": "crypto_prices",
    "type": "update",
    "filters": "btcusdt,ethusdt,solusdt,xrpusdt"
  }]
}
```

**Response :**
```json
{
  "topic": "crypto_prices",
  "type": "update",
  "timestamp": 1706000000000,
  "payload": {
    "symbol": "btcusdt",
    "value": "42000.50",
    "timestamp": 1706000000000
  }
}
```

#### 8.2.2 Crypto Prices (Chainlink)

```json
{
  "action": "subscribe",
  "subscriptions": [{
    "topic": "crypto_prices_chainlink",
    "type": "*",
    "filters": "{\"symbol\":\"btc/usd\"}"
  }]
}
```

#### 8.2.3 Comments

```json
{
  "action": "subscribe",
  "subscriptions": [{
    "topic": "comments",
    "type": "*",
    "gamma_auth": {
      "address": "wallet_address"
    }
  }]
}
```

---

## 9. Authentification

### 9.1 Niveaux d'Authentification

```
┌─────────────────────────────────────────────────────────────────┐
│                  NIVEAUX D'AUTHENTIFICATION                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  PUBLIC        Lecture données marché, prix, orderbook          │
│      │         Pas de headers requis                            │
│      ▼                                                          │
│  L1 AUTH       Créer/Dériver API Keys                           │
│      │         Signature EIP-712 avec Private Key               │
│      ▼                                                          │
│  L2 AUTH       Trading (ordres, annulations, historique)        │
│      │         API Key + Secret + Passphrase (HMAC)             │
│      ▼                                                          │
│  BUILDER       Transactions gasless, attribution ordres         │
│                Builder API Credentials + Relayer                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 9.2 L1 - Private Key Authentication

Utilisé pour créer des API credentials et signer des ordres localement.

**Headers requis :**
```
POLY_ADDRESS: Adresse Polygon du signer
POLY_SIGNATURE: Signature EIP-712
POLY_TIMESTAMP: Timestamp UNIX actuel
POLY_NONCE: Nonce (défaut 0)
```

**Structure EIP-712 :**
```javascript
const domain = {
    name: "ClobAuthDomain",
    version: "1",
    chainId: 137
};

const types = {
    ClobAuth: [
        { name: "address", type: "address" },
        { name: "timestamp", type: "string" },
        { name: "nonce", type: "uint256" },
        { name: "message", type: "string" }
    ]
};

const value = {
    address: walletAddress,
    timestamp: timestamp.toString(),
    nonce: 0,
    message: "This message attests that I control the given wallet"
};
```

### 9.3 L2 - API Credentials Authentication

Utilisé pour toutes les opérations de trading (POST/DELETE orders).

**Credentials :**
- `apiKey` : UUID identifiant
- `secret` : String base64 pour HMAC
- `passphrase` : String aléatoire

**Headers requis :**
```
POLY_ADDRESS: Adresse Polygon du signer
POLY_SIGNATURE: Signature HMAC-SHA256
POLY_TIMESTAMP: Timestamp UNIX actuel
POLY_API_KEY: Valeur apiKey
POLY_PASSPHRASE: Valeur passphrase
```

**Génération HMAC (Python) :**
```python
import hmac
import hashlib
import base64

def build_hmac_signature(
    secret: str,
    timestamp: int,
    method: str,
    path: str,
    body: str = ""
) -> str:
    message = f"{timestamp}{method}{path}{body}"
    hmac_obj = hmac.new(
        base64.b64decode(secret),
        message.encode('utf-8'),
        hashlib.sha256
    )
    return base64.b64encode(hmac_obj.digest()).decode('utf-8')
```

**Génération HMAC (Rust) :**
```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{encode, decode};

fn build_hmac_signature(
    secret: &str,
    timestamp: i64,
    method: &str,
    path: &str,
    body: &str
) -> String {
    let message = format!("{}{}{}{}", timestamp, method, path, body);
    let key = decode(secret).unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).unwrap();
    mac.update(message.as_bytes());
    encode(mac.finalize().into_bytes())
}
```

### 9.4 Signature Types

| Type | Valeur | Description |
|------|--------|-------------|
| EOA | 0 | Wallet Ethereum standard (MetaMask, Coinbase) |
| POLY_PROXY | 1 | Magic Link / Google login (Email wallet) |
| GNOSIS_SAFE | 2 | Proxy wallet déployé (recommandé pour bots) |

### 9.5 Obtenir ses Credentials

**Via l'interface Polymarket :**
1. Connecter wallet sur polymarket.com
2. Settings → API Keys
3. Créer ou dériver les credentials

**Via l'API (avec private key) :**
```bash
POST /auth/derive-api-key

Headers:
  POLY_ADDRESS: 0x...
  POLY_SIGNATURE: (EIP-712 signature)
  POLY_TIMESTAMP: 1707000000
  POLY_NONCE: 0
```

**Exporter Private Key (Magic/Email Login) :**
1. Aller sur https://reveal.magic.link/polymarket
2. Connecter avec le même email
3. Exporter la private key

---

## 10. Types d'Ordres

### 10.1 GTC - Good-Til-Cancelled

Ordre limite classique, reste actif jusqu'à exécution ou annulation.

**Python :**
```python
from py_clob_client.clob_types import OrderArgs, OrderType
from py_clob_client.order_builder.constants import BUY

order_args = OrderArgs(
    price=0.65,
    size=100,
    side=BUY,
    token_id="token_id"
)
signed_order = client.create_order(order_args)
resp = client.post_order(signed_order, OrderType.GTC)
```

**Rust :**
```rust
let order = OrderArgs {
    token_id: "...".to_string(),
    price: Decimal::from_str("0.65").unwrap(),
    size: Decimal::from(100),
    side: Side::Buy,
    expiration: None,
};
let signed = client.create_order(order);
client.post_order(signed, OrderType::GTC).await?;
```

### 10.2 GTD - Good-Til-Date

Ordre avec expiration automatique.

**Python :**
```python
import time

# IMPORTANT: Ajouter minimum 1 minute au timestamp désiré
expiration = int(time.time()) + 3600 + 60  # 1 heure + 1 min buffer

order_args = OrderArgs(
    price=0.65,
    size=100,
    side=BUY,
    token_id="token_id",
    expiration=expiration
)
signed_order = client.create_order(order_args)
resp = client.post_order(signed_order, OrderType.GTD)
```

### 10.3 FOK - Fill-Or-Kill

Ordre marché - exécution totale immédiate ou annulation complète.

**Python :**
```python
from py_clob_client.clob_types import MarketOrderArgs

# amount = USDC pour BUY, shares pour SELL
order_args = MarketOrderArgs(
    token_id="token_id",
    amount=100,  # 100 USDC
    side=BUY
)
signed_order = client.create_market_order(order_args)
resp = client.post_order(signed_order, OrderType.FOK)
```

### 10.4 FAK - Fill-And-Kill

Exécution partielle acceptée, reste annulé immédiatement.

```python
resp = client.post_order(signed_order, OrderType.FAK)
```

### 10.5 Post-Only Orders

Ordre qui ne peut pas cross le spread (maker only). Rejeté si marketable immédiatement.

```python
resp = client.post_order(signed_order, OrderType.GTC, post_only=True)
# Incompatible avec FOK/FAK
```

### 10.6 Tableau Récapitulatif

| Type | Exécution | Reste dans book | Use Case |
|------|-----------|-----------------|----------|
| **GTC** | Partielle OK | Oui | Ordres limite standard |
| **GTD** | Partielle OK | Oui (jusqu'à expiration) | Avant événements |
| **FOK** | Totale ou rien | Non | Market orders |
| **FAK** | Partielle OK | Non | Market orders flexibles |
| **Post-Only** | Maker only | Oui | Market making |

---

## 11. Builder Program & Relayer

### 11.1 Vue d'Ensemble

Le Builder Program permet aux développeurs de :
- **Transactions gasless** via Relayer
- **Order attribution** et tracking sur leaderboard
- **Fee share** sur volume routé

### 11.2 Tiers et Limites

| Tier | Transactions/jour | Avantages |
|------|-------------------|-----------|
| **Unverified** | 100 | API keys instant, gasless CLOB |
| **Verified** | 1,500 | RevShare, rewards USDC hebdo, Telegram |
| **Partner** | Unlimited | Support prioritaire, fee split, grants |

**Upgrade** : Contacter builder@polymarket.com

### 11.3 Headers Builder

```
POLY_BUILDER_API_KEY: Builder API key
POLY_BUILDER_TIMESTAMP: Unix timestamp
POLY_BUILDER_PASSPHRASE: Builder passphrase
POLY_BUILDER_SIGNATURE: HMAC signature
```

### 11.4 Relayer Client - Installation

```bash
# Python
pip install py-builder-relayer-client

# TypeScript
npm install @polymarket/builder-relayer-client
```

### 11.5 Relayer Client - Initialisation

**Python :**
```python
from py_builder_relayer_client import RelayerClient
import os

client = RelayerClient(
    chain_id=137,
    signer=wallet_client,
    builder_config={
        "key": os.environ["BUILDER_API_KEY"],
        "secret": os.environ["BUILDER_SECRET"],
        "passphrase": os.environ["BUILDER_PASSPHRASE"]
    },
    wallet_type="safe"  # ou "proxy"
)
```

### 11.6 Opérations Relayer

#### Deploy Wallet

```python
response = await client.deploy()
result = await response.wait()
print(f"Safe Address: {result.proxy_address}")
```

#### Execute Transactions

```python
transactions = [
    {
        "to": "0x...",   # Contract address
        "data": "0x...", # Encoded function call
        "value": "0"     # MATIC amount (usually 0)
    }
]

response = await client.execute(transactions, "Description")
result = await response.wait()
```

#### Approve Tokens

```python
from eth_abi import encode

# Approve USDCe pour CTF
usdc_address = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"
ctf_address = "0x4d97dcd97ec945f40cf65f87097ace5ea0476045"
max_amount = 2**256 - 1

approve_data = "0x095ea7b3" + encode(
    ["address", "uint256"],
    [ctf_address, max_amount]
).hex()

await client.execute([{
    "to": usdc_address,
    "data": approve_data,
    "value": "0"
}], "Approve USDCe")
```

### 11.7 États Transaction Relayer

| État | Description |
|------|-------------|
| `STATE_NEW` | Reçu par relayer |
| `STATE_EXECUTED` | Soumis on-chain |
| `STATE_MINED` | Inclus dans un bloc |
| `STATE_CONFIRMED` | Final (succès) |
| `STATE_FAILED` | Rejeté (échec) |
| `STATE_INVALID` | Format invalide |

---

## 12. CTF - Conditional Token Framework

### 12.1 Vue d'Ensemble

Le CTF de Gnosis permet de tokeniser des outcomes binaires en ERC1155.

### 12.2 Hiérarchie des IDs

```
ConditionId = hash(oracle, questionId, outcomeSlotCount)
    ↓
CollectionId = hash(parentCollectionId, conditionId, indexSet)
    ↓
PositionId = hash(collateralToken, collectionId)
```

**Index Sets pour marchés binaires :**
- YES : `0b01 = 1`
- NO : `0b10 = 2`

### 12.3 Split Position

Convertit du collateral en tokens outcome.

```
1 USDCe → 1 YES + 1 NO
```

```solidity
function splitPosition(
    IERC20 collateralToken,      // USDCe address
    bytes32 parentCollectionId,   // bytes32(0) pour Polymarket
    bytes32 conditionId,          // ID de la condition
    uint[] partition,             // [1, 2] pour binary
    uint amount                   // Montant à split
) external;
```

**Exemple Python :**
```python
from eth_abi import encode

ctf_address = "0x4d97dcd97ec945f40cf65f87097ace5ea0476045"
usdc_address = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"

split_data = encode(
    ["address", "bytes32", "bytes32", "uint256[]", "uint256"],
    [
        usdc_address,
        bytes(32),  # parentCollectionId = 0
        bytes.fromhex(condition_id[2:]),
        [1, 2],  # partition
        amount * 10**6  # USDC has 6 decimals
    ]
)

function_selector = "0x72ce4275"  # splitPosition
calldata = function_selector + split_data.hex()
```

### 12.4 Merge Positions

Convertit des tokens outcome en collateral.

```
1 YES + 1 NO → 1 USDCe
```

```solidity
function mergePositions(
    IERC20 collateralToken,
    bytes32 parentCollectionId,
    bytes32 conditionId,
    uint[] partition,
    uint amount
) external;
```

### 12.5 Redeem Positions

Après résolution, échange les tokens gagnants contre du collateral.

```
1 YES gagnant → 1 USDCe (si YES a gagné)
1 NO perdant → 0 USDCe
```

```solidity
function redeemPositions(
    IERC20 collateralToken,
    bytes32 parentCollectionId,
    bytes32[] conditionIds,
    uint[] indexSets
) external;
```

---

## 13. Negative Risk

### 13.1 Concept

Les événements "winner-take-all" où un seul outcome peut gagner.

**Exemple** : "Qui gagnera l'élection ?" avec options A, B, C, D
- Un seul candidat gagne
- Les NO de chaque option sont liés

### 13.2 Fonctionnement

```
1 NO share (marché A) ↔ 1 YES share (marchés B + C + D)
```

### 13.3 Identification

```python
# Via Gamma API
event = get_event(event_id)
if event["negRisk"]:
    # C'est un événement negative risk
    pass

# Augmented Negative Risk (outcomes ajoutés dynamiquement)
if event.get("enableNegRisk") and event.get("negRiskAugmented"):
    pass
```

### 13.4 Contract

```
Negative Adapter: 0xd91E80cF2E7be2e162c6513ceD06f1dD0dA35296
```

### 13.5 Trading Negative Risk

Pour les marchés negative risk, utiliser les flags appropriés :

```python
response = client.post_order(
    signed_order,
    OrderType.GTC,
    tick_size="0.01",
    neg_risk=True  # Important!
)
```

---

## 14. Marchés Crypto 15 Minutes

### 14.1 Structure

Les marchés "Up or Down" 15 minutes sont des paris binaires sur la direction du prix d'une crypto.

| Crypto | Format Question |
|--------|-----------------|
| BTC | "Bitcoin Up or Down - [Date], [Time] ET?" |
| ETH | "Ethereum Up or Down - [Date], [Time] ET?" |
| SOL | "Solana Up or Down - [Date], [Time] ET?" |
| XRP | "XRP Up or Down - [Date], [Time] ET?" |

### 14.2 Trouver les Marchés

**Python :**
```python
def find_15min_crypto_markets(gamma_client, crypto: str = "btc"):
    """Trouve les marchés crypto 15 minutes actifs"""
    markets = gamma_client.get_markets(closed=False, active=True, limit=200)

    crypto_keywords = {
        "btc": ["bitcoin", "btc"],
        "eth": ["ethereum", "eth"],
        "sol": ["solana", "sol"],
        "xrp": ["xrp"]
    }

    keywords = crypto_keywords.get(crypto.lower(), [crypto.lower()])
    filtered = []

    for market in markets:
        question = market.get("question", "").lower()
        is_crypto = any(kw in question for kw in keywords)
        is_updown = "up or down" in question

        if is_crypto and is_updown:
            filtered.append(market)

    # Trier par date de fin (le plus proche d'abord)
    filtered.sort(key=lambda m: m.get("endDate", ""))

    return filtered
```

**Rust :**
```rust
async fn find_crypto_markets(gamma: &GammaClient, coin: &str) -> Vec<Market> {
    let markets = gamma.get_markets(false, 200).await.unwrap();

    let keywords: Vec<&str> = match coin.to_uppercase().as_str() {
        "BTC" => vec!["bitcoin", "btc"],
        "ETH" => vec!["ethereum", "eth"],
        "SOL" => vec!["solana", "sol"],
        "XRP" => vec!["xrp"],
        _ => vec![coin],
    };

    markets.into_iter()
        .filter(|m| {
            let q = m.question.to_lowercase();
            keywords.iter().any(|kw| q.contains(kw)) && q.contains("up or down")
        })
        .collect()
}
```

### 14.3 Identifier UP vs DOWN Token

```python
def get_up_down_tokens(market: dict) -> tuple:
    """Extrait les token IDs pour UP et DOWN"""
    tokens = market.get("tokens", [])

    # Fallback sur clobTokenIds
    if not tokens and market.get("clobTokenIds"):
        clob_ids = market["clobTokenIds"]
        outcomes = market.get("outcomes", ["Yes", "No"])
        prices = market.get("outcomePrices", ["0.5", "0.5"])
        tokens = [
            {"token_id": clob_ids[0], "outcome": outcomes[0], "price": prices[0]},
            {"token_id": clob_ids[1], "outcome": outcomes[1], "price": prices[1]}
        ]

    up_token = None
    down_token = None

    for token in tokens:
        outcome = token.get("outcome", "").lower()
        if outcome in ["up", "yes"]:
            up_token = {
                "token_id": token["token_id"],
                "price": float(token.get("price", 0.5))
            }
        elif outcome in ["down", "no"]:
            down_token = {
                "token_id": token["token_id"],
                "price": float(token.get("price", 0.5))
            }

    return up_token, down_token
```

---

## 15. Architecture du Bot

### 15.1 Diagramme d'Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        TRADING BOT ARCHITECTURE                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌──────────────────┐     ┌──────────────────┐     ┌─────────────────┐ │
│  │   DATA COLLECTOR │     │  STRATEGY ENGINE │     │  ORDER MANAGER  │ │
│  │                  │     │                  │     │                 │ │
│  │ • RTDS WebSocket │────▶│ • Signal Analysis│────▶│ • Create Orders │ │
│  │ • CLOB WebSocket │     │ • Trend Detection│     │ • Post Orders   │ │
│  │ • Gamma API      │     │ • Entry/Exit     │     │ • Cancel Orders │ │
│  └──────────────────┘     └──────────────────┘     └─────────────────┘ │
│           │                        │                        │          │
│           ▼                        ▼                        ▼          │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                         STATE MANAGER                             │  │
│  │  • Positions • Open Orders • P&L Tracking • Market State          │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│           │                        │                        │          │
│           ▼                        ▼                        ▼          │
│  ┌──────────────────┐     ┌──────────────────┐     ┌─────────────────┐ │
│  │   RISK MANAGER   │     │     LOGGER       │     │   CONFIG        │ │
│  │                  │     │                  │     │                 │ │
│  │ • Position Limits│     │ • Trade History  │     │ • API Keys      │ │
│  │ • Stop Loss      │     │ • Error Logging  │     │ • Risk Params   │ │
│  │ • Take Profit    │     │ • Performance    │     │ • Strategy      │ │
│  └──────────────────┘     └──────────────────┘     └─────────────────┘ │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 15.2 Structure des Fichiers (Python)

```
polymarket-bot/
├── src/
│   ├── __init__.py
│   ├── config.py              # Configuration et variables d'env
│   ├── client.py              # Wrapper CLOB client
│   │
│   ├── data/
│   │   ├── __init__.py
│   │   ├── gamma_client.py    # Client Gamma API
│   │   ├── rtds_client.py     # WebSocket prix crypto
│   │   └── orderbook_ws.py    # WebSocket orderbook
│   │
│   ├── trading/
│   │   ├── __init__.py
│   │   ├── order_manager.py   # Gestion des ordres
│   │   ├── position_tracker.py # Suivi des positions
│   │   └── executor.py        # Exécution des trades
│   │
│   ├── strategy/
│   │   ├── __init__.py
│   │   ├── base.py            # Classe de base stratégie
│   │   ├── momentum.py        # Stratégie momentum
│   │   └── signals.py         # Génération de signaux
│   │
│   └── utils/
│       ├── __init__.py
│       ├── logger.py          # Logging
│       └── risk.py            # Gestion des risques
│
├── tests/
├── .env                       # Variables d'environnement
├── .env.example              # Template
├── requirements.txt
└── main.py                    # Point d'entrée
```

### 15.3 Structure des Fichiers (Rust)

```
poly_bot/
├── crates/
│   ├── common/                     # Types partagés
│   │   └── src/
│   │       ├── config.rs          # Configuration .env
│   │       └── types.rs           # Types Polymarket
│   │
│   ├── trading-engine/             # Moteur de trading
│   │   └── src/
│   │       ├── api/
│   │       │   ├── clob.rs        # Client CLOB API
│   │       │   └── gamma.rs       # Client Gamma API
│   │       ├── websocket/
│   │       │   └── mod.rs         # WebSocket CLOB
│   │       ├── strategy/
│   │       │   ├── mod.rs         # Trait Strategy
│   │       │   └── crypto_auto.rs # Stratégie crypto
│   │       └── bot.rs             # TradingBot
│   │
│   └── cli/                        # CLI + API Server
│       └── src/
│           └── main.rs            # poly-cli
│
├── src/                            # Frontend Svelte
├── .env                           # Configuration
└── Cargo.toml                     # Workspace
```

---

## 16. Implémentation Python

### 16.1 Configuration (config.py)

```python
import os
from dataclasses import dataclass, field
from typing import List
from dotenv import load_dotenv

load_dotenv()

@dataclass
class Config:
    # API Endpoints
    CLOB_HOST: str = "https://clob.polymarket.com"
    GAMMA_HOST: str = "https://gamma-api.polymarket.com"
    DATA_HOST: str = "https://data-api.polymarket.com"
    RTDS_WS: str = "wss://ws-live-data.polymarket.com"
    CLOB_WS: str = "wss://ws-subscriptions-clob.polymarket.com/ws/market"

    # Blockchain
    CHAIN_ID: int = 137  # Polygon Mainnet

    # Authentication
    PRIVATE_KEY: str = field(default_factory=lambda: os.getenv("PRIVATE_KEY", ""))
    FUNDER_ADDRESS: str = field(default_factory=lambda: os.getenv("POLYMARKET_WALLET_ADDRESS", ""))
    SIGNATURE_TYPE: int = field(default_factory=lambda: int(os.getenv("SIGNATURE_TYPE", "1")))

    API_KEY: str = field(default_factory=lambda: os.getenv("POLY_API_KEY", ""))
    API_SECRET: str = field(default_factory=lambda: os.getenv("POLY_API_SECRET", ""))
    API_PASSPHRASE: str = field(default_factory=lambda: os.getenv("POLY_API_PASSPHRASE", ""))

    # Trading Parameters
    MAX_POSITION_SIZE: float = 100.0    # USDC max par trade
    MAX_TOTAL_EXPOSURE: float = 500.0   # USDC total max
    MAX_POSITIONS: int = 5              # Positions simultanées max
    STOP_LOSS_PCT: float = 0.15         # 15%
    TAKE_PROFIT_PCT: float = 0.30       # 30%

    # Strategy Parameters
    MOMENTUM_THRESHOLD: float = 0.001   # 0.1% mouvement minimum
    MIN_PROBABILITY_EDGE: float = 0.05  # 5% edge minimum
    CONFIDENCE_THRESHOLD: float = 0.6   # 60% confiance minimum

    # Cryptos à trader
    CRYPTO_SYMBOLS: List[str] = field(
        default_factory=lambda: ["btcusdt", "ethusdt", "solusdt", "xrpusdt"]
    )

    def validate(self) -> bool:
        """Valide que toutes les configs requises sont présentes"""
        required = [self.PRIVATE_KEY, self.FUNDER_ADDRESS, self.API_KEY]
        return all(required)

config = Config()
```

### 16.2 Client CLOB (client.py)

```python
from py_clob_client.client import ClobClient
from py_clob_client.clob_types import ApiCreds, OrderArgs, MarketOrderArgs, OrderType
from py_clob_client.order_builder.constants import BUY, SELL
import logging

logger = logging.getLogger(__name__)

class TradingClient:
    def __init__(self, config):
        self.config = config
        self.client = None
        self._initialize_client()

    def _initialize_client(self):
        """Initialise le client CLOB avec authentification"""
        api_creds = ApiCreds(
            api_key=self.config.API_KEY,
            api_secret=self.config.API_SECRET,
            api_passphrase=self.config.API_PASSPHRASE
        )

        self.client = ClobClient(
            host=self.config.CLOB_HOST,
            chain_id=self.config.CHAIN_ID,
            key=self.config.PRIVATE_KEY,
            creds=api_creds,
            signature_type=self.config.SIGNATURE_TYPE,
            funder=self.config.FUNDER_ADDRESS
        )
        logger.info("CLOB Client initialized")

    def get_price(self, token_id: str, side: str = "BUY") -> float:
        response = self.client.get_price(token_id, side)
        return float(response.get("price", 0))

    def get_orderbook(self, token_id: str) -> dict:
        return self.client.get_order_book(token_id)

    def place_limit_order(self, token_id: str, price: float, size: float,
                          side: str, tick_size: str = "0.01", neg_risk: bool = False) -> dict:
        order_side = BUY if side.upper() == "BUY" else SELL
        order_args = OrderArgs(token_id=token_id, price=price, size=size, side=order_side)
        signed_order = self.client.create_order(order_args)
        return self.client.post_order(signed_order, OrderType.GTC,
                                      tick_size=tick_size, neg_risk=neg_risk)

    def place_market_order(self, token_id: str, amount: float, side: str) -> dict:
        order_side = BUY if side.upper() == "BUY" else SELL
        market_order = MarketOrderArgs(token_id=token_id, amount=amount, side=order_side)
        signed_order = self.client.create_market_order(market_order)
        return self.client.post_order(signed_order, OrderType.FOK)

    def cancel_order(self, order_id: str) -> dict:
        return self.client.cancel(order_id)

    def cancel_all_orders(self) -> dict:
        return self.client.cancel_all()

    def get_open_orders(self, market: str = None) -> list:
        return self.client.get_orders(market=market) if market else self.client.get_orders()
```

### 16.3 Client Gamma (gamma_client.py)

```python
import requests
from typing import List, Optional

class GammaClient:
    BASE_URL = "https://gamma-api.polymarket.com"

    def get_markets(self, closed: bool = False, active: bool = True,
                    limit: int = 100, tag_slug: Optional[str] = None) -> List[dict]:
        params = {
            "closed": str(closed).lower(),
            "active": str(active).lower(),
            "limit": limit
        }
        if tag_slug:
            params["tag_slug"] = tag_slug
        response = requests.get(f"{self.BASE_URL}/markets", params=params)
        response.raise_for_status()
        return response.json()

    def find_crypto_markets(self, crypto: str = "btc") -> List[dict]:
        markets = self.get_markets(closed=False, active=True, limit=200)
        crypto_map = {
            "btc": ["bitcoin", "btc"],
            "eth": ["ethereum", "eth"],
            "sol": ["solana", "sol"],
            "xrp": ["xrp"]
        }
        keywords = crypto_map.get(crypto.lower(), [crypto.lower()])
        return [m for m in markets
                if any(kw in m.get("question", "").lower() for kw in keywords)
                and "up or down" in m.get("question", "").lower()]
```

### 16.4 Client RTDS WebSocket (rtds_client.py)

```python
import asyncio
import websockets
import json
import threading
from typing import Callable, Dict, List
import logging

logger = logging.getLogger(__name__)

class RTDSClient:
    WS_URL = "wss://ws-live-data.polymarket.com"

    def __init__(self, on_price_update: Callable[[str, float], None]):
        self.on_price_update = on_price_update
        self.prices: Dict[str, float] = {}
        self.running = False
        self.symbols: List[str] = []

    def connect(self, symbols: List[str]):
        self.symbols = symbols
        self.running = True
        thread = threading.Thread(target=lambda: asyncio.run(self._connect_async()))
        thread.daemon = True
        thread.start()

    async def _connect_async(self):
        while self.running:
            try:
                async with websockets.connect(self.WS_URL) as ws:
                    # Subscribe
                    await ws.send(json.dumps({
                        "action": "subscribe",
                        "subscriptions": [{
                            "topic": "crypto_prices",
                            "type": "update",
                            "filters": ",".join(self.symbols)
                        }]
                    }))

                    # Ping task
                    ping_task = asyncio.create_task(self._ping_loop(ws))

                    try:
                        async for message in ws:
                            if message == "PONG":
                                continue
                            await self._handle_message(message)
                    finally:
                        ping_task.cancel()

            except Exception as e:
                logger.error(f"RTDS error: {e}")
                await asyncio.sleep(5)

    async def _ping_loop(self, ws):
        while True:
            try:
                await ws.send("PING")
                await asyncio.sleep(5)
            except:
                break

    async def _handle_message(self, message: str):
        try:
            data = json.loads(message)
            if data.get("topic") == "crypto_prices":
                payload = data.get("payload", {})
                symbol = payload.get("symbol")
                value = payload.get("value")
                if symbol and value:
                    self.prices[symbol] = float(value)
                    self.on_price_update(symbol, float(value))
        except Exception as e:
            logger.error(f"Message error: {e}")

    def get_price(self, symbol: str) -> float:
        return self.prices.get(symbol.lower(), 0)

    def disconnect(self):
        self.running = False
```

---

## 17. Implémentation Rust

### 17.1 Configuration (config.rs)

```rust
use std::env;
use dotenvy::dotenv;

#[derive(Debug, Clone)]
pub struct Config {
    pub clob_host: String,
    pub gamma_host: String,
    pub chain_id: u64,
    pub safe_address: String,
    pub private_key: String,
    pub api_key: String,
    pub api_secret: String,
    pub api_passphrase: String,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        dotenv().ok();

        Ok(Self {
            clob_host: env::var("POLY_CLOB_HOST")
                .unwrap_or_else(|_| "https://clob.polymarket.com".into()),
            gamma_host: env::var("POLY_GAMMA_HOST")
                .unwrap_or_else(|_| "https://gamma-api.polymarket.com".into()),
            chain_id: env::var("POLY_CHAIN_ID")
                .unwrap_or_else(|_| "137".into())
                .parse()?,
            safe_address: env::var("POLY_PROXY_WALLET")?,
            private_key: env::var("POLY_PRIVATE_KEY")?,
            api_key: env::var("POLY_BUILDER_API_KEY")?,
            api_secret: env::var("POLY_BUILDER_API_SECRET")?,
            api_passphrase: env::var("POLY_BUILDER_API_PASSPHRASE")?,
        })
    }
}
```

### 17.2 Client CLOB (clob.rs)

```rust
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub struct ClobClient {
    host: String,
    client: Client,
    credentials: ApiCredentials,
}

#[derive(Debug, Deserialize)]
pub struct OrderBook {
    pub market: String,
    pub asset_id: String,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
    pub tick_size: String,
}

#[derive(Debug, Deserialize)]
pub struct Level {
    pub price: String,
    pub size: String,
}

impl ClobClient {
    pub fn new(host: &str, credentials: ApiCredentials) -> Self {
        Self {
            host: host.to_string(),
            client: Client::new(),
            credentials,
        }
    }

    pub async fn get_order_book(&self, token_id: &str) -> Result<OrderBook, reqwest::Error> {
        let url = format!("{}/book?token_id={}", self.host, token_id);
        self.client.get(&url).send().await?.json().await
    }

    pub async fn get_price(&self, token_id: &str, side: &str) -> Result<Decimal, reqwest::Error> {
        let url = format!("{}/price?token_id={}&side={}", self.host, token_id, side);
        let response: PriceResponse = self.client.get(&url).send().await?.json().await?;
        Ok(Decimal::from_str(&response.price).unwrap_or_default())
    }

    pub async fn get_midpoint(&self, token_id: &str) -> Result<Decimal, reqwest::Error> {
        let url = format!("{}/midpoint?token_id={}", self.host, token_id);
        let response: MidpointResponse = self.client.get(&url).send().await?.json().await?;
        Ok(Decimal::from_str(&response.mid).unwrap_or_default())
    }
}
```

### 17.3 WebSocket Manager (websocket/mod.rs)

```rust
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use serde_json::json;

pub struct MarketWebSocket {
    url: String,
    token_ids: Vec<String>,
    sender: mpsc::Sender<MarketEvent>,
}

#[derive(Debug, Clone)]
pub enum MarketEvent {
    Connected,
    Disconnected,
    Book(OrderbookSnapshot),
    PriceChange { token_id: String, price: Decimal },
    Trade { token_id: String, price: Decimal, size: Decimal },
    Error(String),
}

impl MarketWebSocket {
    pub fn new(token_ids: Vec<String>, sender: mpsc::Sender<MarketEvent>) -> Self {
        Self {
            url: "wss://ws-subscriptions-clob.polymarket.com/ws/market".to_string(),
            token_ids,
            sender,
        }
    }

    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Subscribe
        let subscribe_msg = json!({
            "type": "market",
            "assets_ids": self.token_ids
        });
        write.send(Message::Text(subscribe_msg.to_string())).await?;

        self.sender.send(MarketEvent::Connected).await?;

        while let Some(msg) = read.next().await {
            match msg? {
                Message::Text(text) => {
                    if let Ok(event) = self.parse_message(&text) {
                        self.sender.send(event).await?;
                    }
                }
                Message::Close(_) => {
                    self.sender.send(MarketEvent::Disconnected).await?;
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }
}
```

### 17.4 Trait Strategy (strategy/mod.rs)

```rust
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::collections::HashMap;

#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &str;
    fn is_running(&self) -> bool;
    async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    async fn stop(&mut self);
    async fn on_tick(&mut self, prices: &HashMap<String, Decimal>);
    fn render_status(&self, prices: &HashMap<String, Decimal>) -> String;
}
```

---

## 18. Stratégies de Trading

### 18.1 Stratégie Momentum (Python)

```python
from dataclasses import dataclass
from typing import Optional, Tuple, Dict, List
from enum import Enum
import time

class Signal(Enum):
    BUY_UP = "buy_up"
    BUY_DOWN = "buy_down"
    HOLD = "hold"

@dataclass
class TradeSignal:
    signal: Signal
    token_id: str
    price: float
    size: float
    confidence: float
    reason: str

class MomentumStrategy:
    def __init__(self, momentum_threshold: float = 0.001,
                 min_edge: float = 0.05, min_confidence: float = 0.6):
        self.momentum_threshold = momentum_threshold
        self.min_edge = min_edge
        self.min_confidence = min_confidence
        self.price_history: Dict[str, List[Tuple[float, float]]] = {}

    def update_price(self, symbol: str, price: float):
        current_time = time.time()
        if symbol not in self.price_history:
            self.price_history[symbol] = []
        self.price_history[symbol].append((current_time, price))
        # Keep last 60 seconds
        cutoff = current_time - 60
        self.price_history[symbol] = [(t, p) for t, p in self.price_history[symbol] if t > cutoff]

    def calculate_momentum(self, symbol: str) -> Tuple[float, float]:
        history = self.price_history.get(symbol, [])
        if len(history) < 5:
            return 0.0, 0.0

        start_price = history[0][1]
        end_price = history[-1][1]
        momentum = (end_price - start_price) / start_price if start_price else 0

        # Confidence = consistency
        changes = [(history[i][1] - history[i-1][1]) / history[i-1][1]
                   for i in range(1, len(history)) if history[i-1][1]]
        if not changes:
            return momentum, 0.0

        positive = sum(1 for c in changes if c > 0)
        confidence = positive / len(changes) if momentum > 0 else (len(changes) - positive) / len(changes)

        return momentum, confidence

    def generate_signal(self, symbol: str, up_token: dict, down_token: dict,
                        trade_size: float) -> Optional[TradeSignal]:
        momentum, confidence = self.calculate_momentum(symbol)

        if abs(momentum) < self.momentum_threshold or confidence < self.min_confidence:
            return TradeSignal(Signal.HOLD, "", 0, 0, confidence, "Insufficient momentum/confidence")

        expected = 0.5 + (momentum * 10)
        expected = max(0.1, min(0.9, expected))

        if momentum > 0:
            target = up_token
            edge = expected - target["price"]
        else:
            target = down_token
            edge = expected - target["price"]

        if edge < self.min_edge:
            return TradeSignal(Signal.HOLD, "", 0, 0, confidence, f"Edge too low: {edge:.2%}")

        signal = Signal.BUY_UP if momentum > 0 else Signal.BUY_DOWN
        shares = trade_size / target["price"] if target["price"] else 0

        return TradeSignal(signal, target["token_id"], target["price"], shares, confidence,
                          f"Momentum: {momentum:.4f}, Edge: {edge:.2%}")
```

### 18.2 Stratégie Crypto Auto (Rust)

```rust
const TOP_COINS: &[&str] = &["BTC", "ETH", "SOL", "XRP"];

pub struct CryptoAutoConfig {
    pub coins: Vec<String>,
    pub size: Decimal,
    pub take_profit_pct: Decimal,
    pub stop_loss_pct: Decimal,
    pub min_volume: f64,
    pub max_spread: f64,
}

pub struct CryptoAutoStrategy {
    config: CryptoAutoConfig,
    running: bool,
    position: Option<Position>,
    selected_market: Option<CryptoMarket>,
}

impl CryptoAutoStrategy {
    pub fn detect_coin(question: &str) -> Option<String> {
        let q = question.to_uppercase();
        let patterns = [
            ("BTC", vec!["BTC", "BITCOIN"]),
            ("ETH", vec!["ETH", "ETHEREUM"]),
            ("SOL", vec!["SOL", "SOLANA"]),
            ("XRP", vec!["XRP"]),
        ];

        for (coin, keywords) in patterns {
            if keywords.iter().any(|kw| q.contains(kw)) {
                return Some(coin.to_string());
            }
        }
        None
    }

    pub fn filter_markets(&self, markets: &[CryptoMarket]) -> Vec<CryptoMarket> {
        markets.iter()
            .filter(|m| {
                self.config.coins.contains(&m.coin)
                    && m.volume >= self.config.min_volume
                    && m.spread <= self.config.max_spread
            })
            .cloned()
            .collect()
    }

    pub fn select_best_market(&self, markets: &[CryptoMarket]) -> Option<CryptoMarket> {
        markets.iter()
            .map(|m| (m.clone(), m.volume / (m.spread * 100.0 + 1.0)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(m, _)| m)
    }
}
```

---

## 19. Gestion des Risques

### 19.1 Risk Manager (Python)

```python
from typing import Dict

class RiskManager:
    def __init__(self, max_position_size: float = 100, max_total_exposure: float = 500,
                 max_positions: int = 5, stop_loss_pct: float = 0.15, take_profit_pct: float = 0.30):
        self.max_position_size = max_position_size
        self.max_total_exposure = max_total_exposure
        self.max_positions = max_positions
        self.stop_loss_pct = stop_loss_pct
        self.take_profit_pct = take_profit_pct
        self.positions: Dict[str, dict] = {}

    def can_open_position(self, size: float) -> bool:
        if len(self.positions) >= self.max_positions:
            return False
        current = sum(p["size"] * p["entry_price"] for p in self.positions.values())
        if current + size > self.max_total_exposure:
            return False
        if size > self.max_position_size:
            return False
        return True

    def check_exit_conditions(self, position: dict, current_price: float) -> str:
        entry = position.get("entry_price", 0)
        if entry == 0:
            return "hold"
        pnl_pct = (current_price - entry) / entry
        if pnl_pct <= -self.stop_loss_pct:
            return "stop_loss"
        if pnl_pct >= self.take_profit_pct:
            return "take_profit"
        return "hold"

    def add_position(self, token_id: str, position: dict):
        self.positions[token_id] = position

    def remove_position(self, token_id: str):
        self.positions.pop(token_id, None)
```

### 19.2 Risk Manager (Rust)

```rust
use rust_decimal::Decimal;
use std::collections::HashMap;

pub struct RiskManager {
    max_position_size: Decimal,
    max_total_exposure: Decimal,
    max_positions: usize,
    stop_loss_pct: Decimal,
    take_profit_pct: Decimal,
    positions: HashMap<String, Position>,
}

impl RiskManager {
    pub fn can_open_position(&self, size: Decimal) -> bool {
        if self.positions.len() >= self.max_positions {
            return false;
        }
        let current: Decimal = self.positions.values()
            .map(|p| p.size * p.entry_price)
            .sum();
        if current + size > self.max_total_exposure {
            return false;
        }
        size <= self.max_position_size
    }

    pub fn check_exit(&self, position: &Position, current_price: Decimal) -> Option<ExitReason> {
        let entry = position.entry_price;
        if current_price <= entry * (Decimal::ONE - self.stop_loss_pct) {
            return Some(ExitReason::StopLoss);
        }
        if current_price >= entry * (Decimal::ONE + self.take_profit_pct) {
            return Some(ExitReason::TakeProfit);
        }
        None
    }
}
```

### 19.3 Règles de Trading

| Règle | Valeur | Description |
|-------|--------|-------------|
| Position Size | 10-100 USDC | Maximum par trade |
| Exposure Max | 100-500 USDC | Total en positions ouvertes |
| Max Positions | 5 | Simultanées |
| Stop Loss | 5-15% | De la valeur d'entrée |
| Take Profit | 10-30% | De la valeur d'entrée |

---

## 20. Best Practices Market Making

### 20.1 Two-Sided Quoting

```python
async def quote_two_sided(client, token_id, mid_price, spread, size):
    bid = mid_price - spread / 2
    ask = mid_price + spread / 2
    orders = [
        client.create_order(OrderArgs(price=bid, size=size, side=BUY, token_id=token_id)),
        client.create_order(OrderArgs(price=ask, size=size, side=SELL, token_id=token_id))
    ]
    return await client.post_orders(orders, OrderType.GTC)
```

### 20.2 Batch Orders (Max 15)

```python
orders = []
for market in markets:
    orders.append(create_quote(market))
    if len(orders) >= 15:
        await client.post_orders(orders, OrderType.GTC)
        orders = []
```

### 20.3 Price Guards

```python
def validate_price(price: float, midpoint: float, max_deviation: float = 0.1) -> bool:
    if midpoint == 0:
        return False
    deviation = abs(price - midpoint) / midpoint
    return deviation <= max_deviation
```

### 20.4 Kill Switch

```python
async def emergency_cancel(client):
    try:
        await client.cancel_all()
        logger.info("All orders cancelled")
    except Exception as e:
        logger.error(f"Emergency cancel failed: {e}")
```

### 20.5 WebSocket vs Polling

```
PRÉFÉRER: WebSocket pour temps réel
  - Données instantanées
  - Pas de rate limit

ÉVITER: Polling REST
  - Risque de rate limit (100 req/min)
  - Délai entre updates
```

### 20.6 GTD pour Événements

```python
# Expire 5 minutes avant la résolution
expiration = event_time - 300 + 60  # +60s buffer requis
```

---

## 21. Liquidity Rewards

### 21.1 Éligibilité

- Ordres limite dans le "max spread" du midpoint
- Si midpoint < $0.10 : ordres des deux côtés requis

### 21.2 Calcul des Rewards

Les rewards dépendent de :
1. **Proximité** : Plus proche du midpoint = plus de rewards
2. **Qualité** : Taille et pricing vs concurrence
3. **Compétitivité** : Ordres compétitifs = plus de gains

### 21.3 Paiements

- **Fréquence** : Quotidien (minuit UTC)
- **Minimum** : $1 (montants inférieurs non payés)
- **Tracking** : Page Rewards sur Polymarket

### 21.4 Vérifier le Scoring

```bash
GET /order-scoring?order_id={ORDER_ID}
Response: { "scoring": true }
```

---

## 22. Frais

### 22.1 Structure Actuelle

| Type | Frais |
|------|-------|
| Maker | **0 bps (0%)** |
| Taker | **0 bps (0%)** |

### 22.2 Calcul des Frais

```python
def calculate_fee(base_rate_bps: int, price: float, size: float) -> float:
    rate = base_rate_bps / 10000
    return rate * min(price, 1 - price) * size
```

---

## 23. Smart Contracts

### 23.1 Adresses (Polygon Mainnet - Chain ID 137)

| Contract | Address |
|----------|---------|
| **USDCe** | `0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174` |
| **CTF** | `0x4d97dcd97ec945f40cf65f87097ace5ea0476045` |
| **CTF Exchange** | `0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E` |
| **Neg Risk CTF Exchange** | `0xC5d563A36AE78145C45a50134d48A1215220f80a` |
| **Neg Risk Adapter** | `0xd91E80cF2E7be2e162c6513ceD06f1dD0dA35296` |

---

## 24. SDK et Ressources

### 24.1 Installation

```bash
# Python
pip install py-clob-client
pip install py-builder-relayer-client

# TypeScript
npm install @polymarket/clob-client
npm install @polymarket/builder-relayer-client
```

### 24.2 Documentation Officielle

| Ressource | URL |
|-----------|-----|
| Documentation Polymarket | https://docs.polymarket.com |
| py-clob-client GitHub | https://github.com/Polymarket/py-clob-client |
| clob-client (TypeScript) | https://github.com/Polymarket/clob-client |
| Polymarket Agents | https://github.com/Polymarket/agents |

### 24.3 Rate Limits

| API | Limite |
|-----|--------|
| CLOB (lecture) | 100 req/min |
| CLOB (trading) | Variable selon tier |
| Gamma | 1000 req/heure |

### 24.4 Variables d'Environnement

```bash
# Wallet
POLY_PROXY_WALLET=0x...           # Safe Address (Polymarket)
POLY_PRIVATE_KEY=0x...            # Clé privée

# API Credentials
POLY_API_KEY=...
POLY_API_SECRET=...
POLY_API_PASSPHRASE=...

# Builder Program (gasless)
POLY_BUILDER_API_KEY=...
POLY_BUILDER_API_SECRET=...
POLY_BUILDER_API_PASSPHRASE=...

# Network
POLY_CHAIN_ID=137
POLY_CLOB_HOST=https://clob.polymarket.com
```

---

## 25. Glossaire

| Terme | Définition |
|-------|------------|
| **CLOB** | Central Limit Order Book - Système de matching off-chain |
| **CTF** | Conditional Token Framework - Smart contracts pour tokens outcome |
| **Event** | Collection de marchés liés |
| **Market** | Outcome tradable binaire (Yes/No ou Up/Down) |
| **Token ID** | Identifiant unique du token outcome (long nombre) |
| **Condition ID** | Identifiant on-chain pour résolution (0x...) |
| **Question ID** | Lien vers oracle UMA |
| **Tick Size** | Incrément prix minimum (0.01 ou 0.001) |
| **Funder** | Wallet détenant les fonds pour trading |
| **EOA** | Externally Owned Account - Wallet standard |
| **Proxy Wallet** | Wallet déployé par Polymarket (gasless) |
| **Safe Wallet** | Gnosis Safe pour trading gasless |
| **Split** | Convertir USDC en tokens YES + NO |
| **Merge** | Convertir tokens YES + NO en USDC |
| **Redeem** | Échanger tokens gagnants post-résolution |
| **Negative Risk** | Événement winner-take-all avec outcomes liés |
| **Builder** | Développeur intégrant Polymarket |
| **Relayer** | Service pour transactions gasless |
| **L1 Auth** | Authentification par signature EIP-712 |
| **L2 Auth** | Authentification par API Key + HMAC |
| **GTC** | Good-Til-Cancelled - Ordre reste actif |
| **GTD** | Good-Til-Date - Ordre avec expiration |
| **FOK** | Fill-Or-Kill - Exécution totale ou annulation |
| **FAK** | Fill-And-Kill - Exécution partielle OK |
| **Momentum** | Variation de prix sur une période |
| **Edge** | Différence entre probabilité estimée et prix marché |

---

## Avertissements

### Restrictions Géographiques

**Polymarket est interdit aux résidents américains et de certaines autres juridictions.** L'utilisation de l'API est soumise aux mêmes restrictions.

**Solution** : Déployer le bot sur un VPS dans une région autorisée (Singapore, Tokyo, London, Ireland, Canada).

### Risques

Le trading automatisé comporte des risques significatifs :
- Perte potentielle de capital
- Risques techniques (bugs, déconnexions)
- Volatilité des marchés
- Risques de liquidité

**Ne tradez jamais plus que ce que vous pouvez vous permettre de perdre.**

---

*Documentation fusionnée - Février 2025*
*Sources: docs.polymarket.com, POLYMARKET_API.md, new_polymarket_doc.md*
