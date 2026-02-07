# Documentation Complète API Polymarket

> Documentation technique extraite de https://docs.polymarket.com (Janvier 2025)

## Table des Matières

1. [Architecture Générale](#1-architecture-générale)
2. [APIs Principales](#2-apis-principales)
3. [Endpoints CLOB](#3-endpoints-clob-principaux)
4. [Authentification](#4-authentification)
5. [Types d'Ordres](#5-types-dordres)
6. [WebSocket Market Channel](#6-websocket-market-channel)
7. [WebSocket User Channel](#7-websocket-user-channel)
8. [Builder Program (Gasless Trading)](#8-builder-program-gasless-trading)
9. [Relayer Client](#9-relayer-client)
10. [CTF (Conditional Token Framework)](#10-ctf-conditional-token-framework)
11. [Gamma API](#11-gamma-api---structure-données)
12. [Data API](#12-data-api)
13. [Negative Risk](#13-negative-risk)
14. [RTDS (Real-Time Data Socket)](#14-rtds-real-time-data-socket)
15. [Frais](#15-frais)
16. [Best Practices Market Making](#16-best-practices-market-making)
17. [Liquidity Rewards](#17-liquidity-rewards)
18. [SDK Disponibles](#18-sdk-disponibles)
19. [Contrats Smart Contracts](#19-contrats-smart-contracts-polygon)
20. [Glossaire](#20-glossaire)

---

## 1. Architecture Générale

Polymarket utilise un modèle **hybrid-décentralisé** :

- **Off-chain** : Matching des ordres via le CLOB (Central Limit Order Book)
- **On-chain** : Settlement sur Polygon via smart contracts CTF (Conditional Token Framework)

### Flux de Trading

```
1. Utilisateur signe ordre (EIP-712)
2. Ordre envoyé au CLOB (off-chain)
3. Matching par l'opérateur
4. Settlement on-chain via Exchange contract
5. Tokens transférés de manière non-custodiale
```

### Caractéristiques Clés

- **Non-custodial** : Les utilisateurs gardent le contrôle de leurs fonds
- **Gasless** : Via Builder Program et Relayer
- **Binary outcomes** : Marchés YES/NO tokenisés en ERC1155
- **Collateral** : USDCe sur Polygon

---

## 2. APIs Principales

| API | Base URL | Usage | Auth |
|-----|----------|-------|------|
| **CLOB API** | `https://clob.polymarket.com` | Trading, orderbook, prix | L1/L2 |
| **Gamma API** | `https://gamma-api.polymarket.com` | Découverte marchés, metadata | Non |
| **Data API** | `https://data-api.polymarket.com` | Positions, activité utilisateur | Non |
| **WebSocket CLOB** | `wss://ws-subscriptions-clob.polymarket.com/ws/` | Orderbook temps réel | API Key (user channel) |
| **RTDS** | `wss://ws-live-data.polymarket.com` | Prix crypto, commentaires | Non |
| **Relayer** | `https://relayer-v2.polymarket.com/` | Transactions gasless | Builder credentials |

---

## 3. Endpoints CLOB Principaux

### Prix et Orderbook (Public)

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

### Trading (Authentifié L2)

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
```

### API Keys (Authentifié L1)

```bash
# Créer/dériver API key
POST /auth/api-key
POST /auth/derive-api-key
```

---

## 4. Authentification

### L1 - Private Key Authentication

Utilisé pour créer des API credentials et signer des ordres localement.

**Headers requis :**
```
POLY_ADDRESS: Adresse Polygon du signer
POLY_SIGNATURE: Signature EIP-712
POLY_TIMESTAMP: Timestamp UNIX actuel
POLY_NONCE: Nonce (défaut 0)
```

**Structure EIP-712 :**
```typescript
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

### L2 - API Credentials Authentication

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

**Génération HMAC :**
```python
import hmac
import hashlib
import base64

def build_hmac_signature(secret: str, timestamp: int, method: str, path: str, body: str = "") -> str:
    message = f"{timestamp}{method}{path}{body}"
    hmac_obj = hmac.new(
        base64.b64decode(secret),
        message.encode('utf-8'),
        hashlib.sha256
    )
    return base64.b64encode(hmac_obj.digest()).decode('utf-8')
```

### Signature Types

| Type | Valeur | Description |
|------|--------|-------------|
| EOA | 0 | Wallet Ethereum standard |
| POLY_PROXY | 1 | Magic Link / Google login |
| GNOSIS_SAFE | 2 | Proxy wallet (recommandé) |

---

## 5. Types d'Ordres

### GTC - Good-Til-Cancelled

```python
# Ordre limite classique, reste actif jusqu'à exécution ou annulation
order_args = OrderArgs(
    price=0.65,
    size=100,
    side=BUY,
    token_id="token_id"
)
signed_order = client.create_order(order_args)
resp = client.post_order(signed_order, OrderType.GTC)
```

### GTD - Good-Til-Date

```python
# Ordre avec expiration automatique
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

### FOK - Fill-Or-Kill

```python
# Ordre marché - exécution totale immédiate ou annulation
order_args = MarketOrderArgs(
    token_id="token_id",
    amount=100,  # Montant en dollars pour BUY, en shares pour SELL
    side=BUY
)
signed_order = client.create_market_order(order_args)
resp = client.post_order(signed_order, OrderType.FOK)
```

### FAK - Fill-And-Kill

```python
# Exécution partielle acceptée, reste annulé
signed_order = client.create_market_order(order_args)
resp = client.post_order(signed_order, OrderType.FAK)
```

### Post-Only Orders

```python
# Ordre qui ne peut pas cross le spread (maker only)
resp = client.post_order(signed_order, OrderType.GTC, post_only=True)
# Rejeté si marketable immédiatement
# Incompatible avec FOK/FAK
```

---

## 6. WebSocket Market Channel

### Connection

```
URL: wss://ws-subscriptions-clob.polymarket.com/ws/market
```

### Subscription

```json
{
  "type": "market",
  "assets_ids": ["token_id_1", "token_id_2"]
}
```

**Limite** : Maximum 500 instruments par connexion WebSocket.

### Message Types

#### 1. `book` - Orderbook Snapshot

Reçu à la subscription initiale et lors de trades affectant l'orderbook.

```json
{
  "event_type": "book",
  "asset_id": "token_id",
  "market": "condition_id",
  "timestamp": 1706000000000,
  "bids": [{"price": "0.64", "size": "100"}, ...],
  "asks": [{"price": "0.66", "size": "150"}, ...],
  "hash": "orderbook_hash"
}
```

#### 2. `price_change` - Changement de Prix

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

#### 3. `tick_size_change` - Changement Tick Size

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

#### 4. `last_trade_price` - Dernier Trade

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

### Exemple Python

```python
import asyncio
import websockets
import json

async def subscribe_market():
    uri = "wss://ws-subscriptions-clob.polymarket.com/ws/market"

    async with websockets.connect(uri) as websocket:
        # Subscribe
        subscription = {
            "type": "market",
            "assets_ids": ["token_id_yes", "token_id_no"]
        }
        await websocket.send(json.dumps(subscription))

        # Listen
        async for message in websocket:
            data = json.loads(message)
            event_type = data.get("event_type")

            if event_type == "book":
                print(f"Orderbook: bids={len(data['bids'])}, asks={len(data['asks'])}")
            elif event_type == "price_change":
                print(f"Price change: {data['changes']}")
            elif event_type == "last_trade_price":
                print(f"Trade: {data['price']} @ {data['size']}")

asyncio.run(subscribe_market())
```

---

## 7. WebSocket User Channel

### Connection (Authentifiée)

```
URL: wss://ws-subscriptions-clob.polymarket.com/ws/user
```

Requiert authentification via API key.

### Subscription

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

### Message Types

#### Trade Messages

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

#### Order Messages

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

---

## 8. Builder Program (Gasless Trading)

### Vue d'Ensemble

Le Builder Program permet aux développeurs de :
- **Transactions gasless** via Relayer
- **Order attribution** et tracking sur leaderboard
- **Fee share** sur volume routé

### Tiers et Limites

| Tier | Transactions/jour | API Rate Limits | Avantages |
|------|-------------------|-----------------|-----------|
| **Unverified** | 100 | Standard | API keys instant, gasless CLOB |
| **Verified** | 1,500 | Standard | RevShare, rewards USDC hebdo, Telegram access |
| **Partner** | Unlimited | Highest | Support prioritaire, fee split, grants |

**Upgrade** : Contacter builder@polymarket.com

### Configuration

1. **Créer API Keys** sur polymarket.com/settings
2. **Configurer CLOB Client** avec headers builder
3. **Implémenter Relayer Client** pour opérations gasless

### Headers Builder

```
POLY_BUILDER_API_KEY: Builder API key
POLY_BUILDER_TIMESTAMP: Unix timestamp
POLY_BUILDER_PASSPHRASE: Builder passphrase
POLY_BUILDER_SIGNATURE: HMAC signature
```

### Génération Signature Builder

```typescript
import crypto from 'crypto';

function buildHmacSignature(
  secret: string,
  timestamp: number,
  method: string,
  path: string,
  body: string = ""
): string {
  const message = `${timestamp}${method}${path}${body}`;
  return crypto
    .createHmac('sha256', Buffer.from(secret, 'base64'))
    .update(message)
    .digest('base64');
}
```

---

## 9. Relayer Client

### Installation

```bash
# Python
pip install py-builder-relayer-client

# TypeScript
npm install @polymarket/builder-relayer-client
```

### Initialisation

```python
from py_builder_relayer_client import RelayerClient

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

### Opérations Principales

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
        "to": "0x...",  # Contract address
        "data": "0x...",  # Encoded function call
        "value": "0"     # MATIC amount
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

### États Transaction

| État | Description |
|------|-------------|
| `STATE_NEW` | Reçu par relayer |
| `STATE_EXECUTED` | Soumis on-chain |
| `STATE_MINED` | Inclus dans un bloc |
| `STATE_CONFIRMED` | Final (succès) |
| `STATE_FAILED` | Rejeté (échec) |
| `STATE_INVALID` | Format invalide |

---

## 10. CTF (Conditional Token Framework)

### Vue d'Ensemble

Le CTF de Gnosis permet de tokeniser des outcomes binaires en ERC1155.

### Hiérarchie des IDs

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

### Split Position

Convertit du collateral en tokens outcome.

```
1 USDCe → 1 YES + 1 NO
```

**Fonction :**
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

# Prepend function selector
function_selector = "0x72ce4275"  # splitPosition
calldata = function_selector + split_data.hex()
```

### Merge Positions

Convertit des tokens outcome en collateral.

```
1 YES + 1 NO → 1 USDCe
```

**Fonction :**
```solidity
function mergePositions(
    IERC20 collateralToken,
    bytes32 parentCollectionId,
    bytes32 conditionId,
    uint[] partition,
    uint amount
) external;
```

### Redeem Positions

Après résolution, échange les tokens gagnants contre du collateral.

```
1 YES gagnant → 1 USDCe (si YES a gagné)
1 NO perdant → 0 USDCe
```

**Fonction :**
```solidity
function redeemPositions(
    IERC20 collateralToken,
    bytes32 parentCollectionId,
    bytes32[] conditionIds,
    uint[] indexSets
) external;
```

---

## 11. Gamma API - Structure Données

### Modèle de Données

```
Event (collection de marchés liés)
├── id: "event_id"
├── slug: "event-slug-url"
├── title: "Question principale"
├── negRisk: boolean
├── markets: [Market, ...]
└── tags: [Tag, ...]

Market (outcome tradable)
├── id: "market_id"
├── question: "Question du marché"
├── conditionId: "0x..."
├── questionId: "0x..."
├── outcomes: ["Yes", "No"]
├── outcomePrices: ["0.65", "0.35"]
├── clobTokenIds: ["token_yes", "token_no"]
├── volume: "1000000"
├── liquidity: "50000"
└── closed: boolean
```

### Endpoints Principaux

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

### Filtres Utiles

| Paramètre | Description | Exemple |
|-----------|-------------|---------|
| `active` | Événements actifs | `true` |
| `closed` | Marchés fermés | `false` |
| `tag_id` | Filtrer par catégorie | `100639` |
| `limit` | Résultats par page | `100` |
| `offset` | Pagination | `0` |
| `order` | Tri | `id` |
| `ascending` | Direction tri | `false` |

### Exemple Python

```python
import requests

GAMMA_URL = "https://gamma-api.polymarket.com"

def get_active_markets():
    response = requests.get(
        f"{GAMMA_URL}/events",
        params={
            "active": "true",
            "closed": "false",
            "limit": 100
        }
    )
    return response.json()

def get_market_by_slug(slug: str):
    response = requests.get(f"{GAMMA_URL}/markets/slug/{slug}")
    market = response.json()

    # Extraire token IDs pour le trading
    token_yes = market["clobTokenIds"][0]
    token_no = market["clobTokenIds"][1]

    return market, token_yes, token_no
```

---

## 12. Data API

### Base URL

```
https://data-api.polymarket.com
```

### Endpoints

```bash
# Positions d'un utilisateur
GET /positions?user={WALLET_ADDRESS}

# Activité utilisateur
GET /activity?user={WALLET_ADDRESS}

# Trades
GET /trades?user={WALLET_ADDRESS}&market={CONDITION_ID}

# Valeur totale positions
GET /value?user={WALLET_ADDRESS}

# Top holders d'un marché
GET /top-holders?market={CONDITION_ID}
```

---

## 13. Negative Risk

### Concept

Les événements "winner-take-all" où un seul outcome peut gagner.

**Exemple** : "Qui gagnera l'élection ?" avec options A, B, C, D
- Un seul candidat gagne
- Les NO de chaque option sont liés

### Fonctionnement

```
1 NO share (marché A) ↔ 1 YES share (marchés B + C + D)
```

### Identification

```python
# Via Gamma API
event = get_event(event_id)
if event["negRisk"]:
    # C'est un événement negative risk
    pass

# Augmented Negative Risk
if event["enableNegRisk"] and event["negRiskAugmented"]:
    # Peut avoir des outcomes ajoutés dynamiquement
    pass
```

### Contract

```
Negative Adapter: 0xd91E80cF2E7be2e162c6513ceD06f1dD0dA35296
```

---

## 14. RTDS (Real-Time Data Socket)

### Connection

```
URL: wss://ws-live-data.polymarket.com
```

### Ping/Pong

Envoyer des messages PING toutes les 5 secondes pour maintenir la connexion.

### Topics Disponibles

#### Crypto Prices (Binance)

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

#### Crypto Prices (Chainlink)

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

#### Comments

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

### Exemple Python

```python
import asyncio
import websockets
import json

async def stream_crypto_prices():
    uri = "wss://ws-live-data.polymarket.com"

    async with websockets.connect(uri) as ws:
        # Subscribe
        await ws.send(json.dumps({
            "action": "subscribe",
            "subscriptions": [{
                "topic": "crypto_prices",
                "type": "update",
                "filters": "btcusdt,ethusdt"
            }]
        }))

        # Ping task
        async def ping():
            while True:
                await ws.send("PING")
                await asyncio.sleep(5)

        asyncio.create_task(ping())

        # Listen
        async for message in ws:
            if message != "PONG":
                data = json.loads(message)
                print(f"{data['payload']['symbol']}: ${data['payload']['value']}")

asyncio.run(stream_crypto_prices())
```

---

## 15. Frais

### Structure Actuelle

| Type | Frais |
|------|-------|
| Maker | 0 bps (0%) |
| Taker | 0 bps (0%) |

### Calcul des Frais

```python
def calculate_fee(base_rate_bps: int, price: float, size: float) -> float:
    """
    base_rate_bps: Taux en basis points (actuellement 0)
    price: Prix de l'ordre (0-1)
    size: Taille de l'ordre
    """
    rate = base_rate_bps / 10000
    return rate * min(price, 1 - price) * size
```

---

## 16. Best Practices Market Making

### Two-Sided Quoting

```python
# Poster des ordres des deux côtés pour maximiser les rewards
async def quote_two_sided(client, token_id, mid_price, spread, size):
    bid_price = mid_price - spread / 2
    ask_price = mid_price + spread / 2

    orders = [
        client.create_order(OrderArgs(
            price=bid_price,
            size=size,
            side=BUY,
            token_id=token_id
        )),
        client.create_order(OrderArgs(
            price=ask_price,
            size=size,
            side=SELL,
            token_id=token_id
        ))
    ]

    return await client.post_orders(orders, OrderType.GTC)
```

### Batch Orders

```python
# Utiliser postOrders() au lieu de multiples createAndPostOrder()
# Maximum 15 ordres par batch
orders = []
for market in markets:
    orders.append(create_quote(market))
    if len(orders) >= 15:
        await client.post_orders(orders, OrderType.GTC)
        orders = []
```

### Price Guards

```python
def validate_price(price: float, midpoint: float, max_deviation: float = 0.1) -> bool:
    """Rejeter les prix trop éloignés du midpoint"""
    deviation = abs(price - midpoint) / midpoint
    return deviation <= max_deviation
```

### Kill Switch

```python
async def emergency_cancel(client):
    """Annuler tous les ordres en cas d'erreur"""
    try:
        await client.cancel_all()
        print("All orders cancelled")
    except Exception as e:
        print(f"Emergency cancel failed: {e}")
```

### WebSocket vs Polling

```python
# PRÉFÉRER : WebSocket pour données temps réel
async def monitor_orderbook_ws(token_id):
    async with websockets.connect(WS_URL) as ws:
        await ws.send(json.dumps({
            "type": "market",
            "assets_ids": [token_id]
        }))
        async for msg in ws:
            process_update(json.loads(msg))

# ÉVITER : Polling REST
async def monitor_orderbook_polling(token_id):
    while True:
        book = await client.get_order_book(token_id)
        process_book(book)
        await asyncio.sleep(1)  # Rate limit risk!
```

### GTD pour Événements

```python
# Utiliser GTD avant événements significatifs
def create_event_aware_order(client, token_id, event_time: int):
    # Expire 5 minutes avant l'événement
    expiration = event_time - 300 + 60  # +60s buffer

    return client.create_order(
        OrderArgs(price=0.5, size=100, side=BUY, token_id=token_id),
        expiration=expiration
    )
```

---

## 17. Liquidity Rewards

### Éligibilité

- Ordres limite dans le "max spread" du midpoint
- Si midpoint < $0.10 : ordres des deux côtés requis

### Calcul

Les rewards dépendent de :
1. **Proximité** : Plus proche du midpoint = plus de rewards
2. **Qualité** : Taille et pricing vs concurrence
3. **Compétitivité** : Ordres compétitifs = plus de gains

### Paiements

- **Fréquence** : Quotidien (minuit UTC)
- **Minimum** : $1 (montants inférieurs non payés)
- **Tracking** : Page Rewards sur Polymarket

### Vérifier le Scoring

```bash
GET /order-scoring?order_id={ORDER_ID}
Response: { "scoring": true }
```

---

## 18. SDK Disponibles

### Python

```bash
pip install py-clob-client
pip install py-builder-relayer-client
```

```python
from py_clob_client.client import ClobClient
from py_clob_client.clob_types import OrderArgs, OrderType
from py_clob_client.constants import POLYGON

# Client basique (EOA)
client = ClobClient(
    host="https://clob.polymarket.com",
    chain_id=POLYGON,
    key=private_key
)

# Avec proxy wallet
client = ClobClient(
    host="https://clob.polymarket.com",
    chain_id=POLYGON,
    key=private_key,
    creds=api_creds,
    signature_type=2,  # GNOSIS_SAFE
    funder=proxy_wallet_address
)
```

### TypeScript

```bash
npm install @polymarket/clob-client ethers
npm install @polymarket/builder-relayer-client
```

```typescript
import { ClobClient } from "@polymarket/clob-client";
import { Wallet } from "ethers";

const signer = new Wallet(process.env.PRIVATE_KEY);

// Client basique
const client = new ClobClient(
  "https://clob.polymarket.com",
  137,
  signer
);

// Avec credentials
const client = new ClobClient(
  "https://clob.polymarket.com",
  137,
  signer,
  apiCreds,
  2,  // GNOSIS_SAFE
  proxyWalletAddress
);
```

### GitHub Repositories

- [py-clob-client](https://github.com/Polymarket/py-clob-client)
- [clob-client (TypeScript)](https://github.com/Polymarket/clob-client)
- [real-time-data-client](https://github.com/Polymarket/real-time-data-client)

---

## 19. Contrats Smart Contracts (Polygon)

| Contract | Address |
|----------|---------|
| **USDCe** | `0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174` |
| **CTF** | `0x4d97dcd97ec945f40cf65f87097ace5ea0476045` |
| **CTF Exchange** | `0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E` |
| **Neg Risk CTF Exchange** | `0xC5d563A36AE78145C45a50134d48A1215220f80a` |
| **Neg Risk Adapter** | `0xd91E80cF2E7be2e162c6513ceD06f1dD0dA35296` |
| **UMA Adapter V2** (Oracle) | `0x...` (varie par marché) |

**Chain ID** : 137 (Polygon Mainnet)

---

## 20. Glossaire

| Terme | Définition |
|-------|------------|
| **CLOB** | Central Limit Order Book - Système de matching off-chain |
| **CTF** | Conditional Token Framework - Smart contracts pour tokens outcome |
| **Event** | Collection de marchés liés |
| **Market** | Outcome tradable binaire (Yes/No) |
| **Token ID** | Identifiant unique du token outcome |
| **Condition ID** | Identifiant on-chain pour résolution |
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

---

## Sources

- [Polymarket Documentation](https://docs.polymarket.com)
- [CLOB Introduction](https://docs.polymarket.com/developers/CLOB/introduction)
- [Authentication](https://docs.polymarket.com/developers/CLOB/authentication)
- [Place Single Order](https://docs.polymarket.com/developers/CLOB/orders/create-order)
- [WebSocket Market Channel](https://docs.polymarket.com/developers/CLOB/websocket/market-channel)
- [Builder Program](https://docs.polymarket.com/developers/builders/builder-intro)
- [Relayer Client](https://docs.polymarket.com/developers/builders/relayer-client)
- [CTF Overview](https://docs.polymarket.com/developers/CTF/overview)
- [Gamma API](https://docs.polymarket.com/developers/gamma-markets-api/overview)
- [Market Makers Guide](https://docs.polymarket.com/developers/market-makers/introduction)
- [Negative Risk](https://docs.polymarket.com/developers/neg-risk/overview)
- [RTDS Overview](https://docs.polymarket.com/developers/RTDS/RTDS-overview)
- [Liquidity Rewards](https://docs.polymarket.com/polymarket-learn/trading/liquidity-rewards)
- [Glossary](https://docs.polymarket.com/quickstart/reference/glossary)
- [GitHub py-clob-client](https://github.com/Polymarket/py-clob-client)

---

*Document généré le 30 janvier 2025*
