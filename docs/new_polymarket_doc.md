# Documentation Technique - Bot de Trading Polymarket
## Trading Automatique sur les Marchés Crypto 15 Minutes (BTC, ETH, SOL, XRP)

---

## Table des Matières

1. [Vue d'Ensemble](#1-vue-densemble)
2. [Architecture des APIs Polymarket](#2-architecture-des-apis-polymarket)
3. [Gamma API - Données de Marché](#3-gamma-api---données-de-marché)
4. [CLOB API - Trading](#4-clob-api---trading)
5. [WebSocket & RTDS - Données Temps Réel](#5-websocket--rtds---données-temps-réel)
6. [Authentification](#6-authentification)
7. [Marchés Crypto 15 Minutes](#7-marchés-crypto-15-minutes)
8. [Architecture du Bot](#8-architecture-du-bot)
9. [Implémentation Python](#9-implémentation-python)
10. [Stratégie de Trading Up/Down](#10-stratégie-de-trading-updown)
11. [Gestion des Risques](#11-gestion-des-risques)
12. [Ressources et Références](#12-ressources-et-références)

---

## 1. Vue d'Ensemble

### 1.1 Qu'est-ce que Polymarket ?

Polymarket est la plus grande plateforme de marchés prédictifs décentralisée au monde. Elle permet de trader sur les résultats d'événements futurs en utilisant des tokens sur la blockchain Polygon.

### 1.2 Concepts Clés

| Concept | Description |
|---------|-------------|
| **Event** | Un événement (ex: "Bitcoin Up or Down - 15 min") |
| **Market** | Un marché binaire avec outcomes YES/NO |
| **Token** | Représentation blockchain d'un outcome (ERC-1155) |
| **USDC** | Collateral utilisé pour trader (sur Polygon) |
| **Price** | Prix entre 0.00 et 1.00 = probabilité |

### 1.3 Fonctionnement des Prix

- Prix YES à 0.65 = 65% de probabilité que l'événement se produise
- Acheter YES à 0.65 → Si YES gagne, reçoit 1.00 USDC (profit de 0.35)
- Prix YES + Prix NO = 1.00 USDC (toujours)

### 1.4 Architecture Hybride

Polymarket utilise un système hybride :
- **Off-chain** : Matching des ordres via le CLOB (Central Limit Order Book)
- **On-chain** : Settlement/exécution sur Polygon via smart contracts

---

## 2. Architecture des APIs Polymarket

### 2.1 Vue d'Ensemble des APIs

```
┌─────────────────────────────────────────────────────────────────────┐
│                     POLYMARKET API ECOSYSTEM                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐ │
│  │   GAMMA API     │  │    CLOB API     │  │     DATA API        │ │
│  │  (Métadonnées)  │  │    (Trading)    │  │  (Données User)     │ │
│  └────────┬────────┘  └────────┬────────┘  └──────────┬──────────┘ │
│           │                    │                      │            │
│  ┌────────▼────────────────────▼──────────────────────▼──────────┐ │
│  │                     WebSocket / RTDS                          │ │
│  │                   (Temps Réel)                                │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                    POLYGON BLOCKCHAIN                         │ │
│  │              (Settlement & Smart Contracts)                   │ │
│  └───────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Tableau des APIs

| API | URL Base | Authentification | Usage |
|-----|----------|------------------|-------|
| **Gamma API** | `https://gamma-api.polymarket.com` | Aucune (lecture) | Métadonnées marchés, events, tags |
| **CLOB API** | `https://clob.polymarket.com` | L1/L2 Auth | Trading, orderbook, prix |
| **Data API** | `https://data-api.polymarket.com` | API Key | Positions, historique trades |
| **RTDS WebSocket** | `wss://rtds.polymarket.com` | Aucune | Prix crypto temps réel |
| **CLOB WebSocket** | `wss://clob.polymarket.com/ws` | API Key | Orderbook temps réel |

---

## 3. Gamma API - Données de Marché

### 3.1 Endpoints Principaux

```
GET https://gamma-api.polymarket.com/events
GET https://gamma-api.polymarket.com/markets
GET https://gamma-api.polymarket.com/tags
```

### 3.2 Récupérer les Events

```python
import requests

# Récupérer les events actifs
response = requests.get(
    "https://gamma-api.polymarket.com/events",
    params={
        "closed": "false",
        "limit": 50,
        "offset": 0,
        "order": "volume24hr",
        "ascending": "false"
    }
)
events = response.json()
```

### 3.3 Récupérer les Markets par Tag

```python
# Récupérer les marchés crypto 15 minutes
# Tag ID pour "15 Min" à identifier via /tags endpoint
response = requests.get(
    "https://gamma-api.polymarket.com/markets",
    params={
        "tag_slug": "15-min",  # ou tag_id
        "closed": "false",
        "limit": 25
    }
)
markets = response.json()
```

### 3.4 Structure d'un Market

```json
{
  "id": "0x1234...",
  "question": "Bitcoin Up or Down - February 4, 3:45 PM ET?",
  "conditionId": "0xabcd...",
  "slug": "btc-up-down-feb-4-345pm",
  "outcomes": ["Up", "Down"],
  "outcomePrices": ["0.55", "0.45"],
  "tokens": [
    {
      "token_id": "12345678901234567890...",
      "outcome": "Up",
      "price": "0.55"
    },
    {
      "token_id": "98765432109876543210...",
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
| `token_id` | ID unique du token (YES ou NO) | **Requis pour trader** |
| `conditionId` | ID de la condition sur la blockchain | Identification unique |
| `outcomePrices` | Prix actuels des outcomes | Analyse de marché |
| `minimum_tick_size` | Incrément minimum de prix | Validation des ordres |
| `endDate` | Date de résolution | Timing du trade |
| `neg_risk` | Type de marché | Configuration CLOB |

---

## 4. CLOB API - Trading

### 4.1 Endpoints Principaux

| Endpoint | Méthode | Description | Auth |
|----------|---------|-------------|------|
| `/price` | GET | Prix actuel d'un token | Public |
| `/midpoint` | GET | Prix médian | Public |
| `/book` | GET | Orderbook complet | Public |
| `/order` | POST | Placer un ordre | L2 |
| `/order` | DELETE | Annuler un ordre | L2 |
| `/orders` | GET | Lister ses ordres | L2 |
| `/trades` | GET | Historique trades | L2 |

### 4.2 Récupérer le Prix

```python
import requests

TOKEN_ID = "12345678901234567890..."

# Prix actuel
response = requests.get(
    f"https://clob.polymarket.com/price",
    params={"token_id": TOKEN_ID, "side": "BUY"}
)
price = response.json()  # {"price": "0.55"}

# Midpoint (prix médian)
response = requests.get(
    f"https://clob.polymarket.com/midpoint",
    params={"token_id": TOKEN_ID}
)
midpoint = response.json()  # {"mid": "0.545"}
```

### 4.3 Récupérer l'Orderbook

```python
response = requests.get(
    f"https://clob.polymarket.com/book",
    params={"token_id": TOKEN_ID}
)
orderbook = response.json()
# {
#   "market": "0x...",
#   "asset_id": "12345...",
#   "bids": [{"price": "0.54", "size": "100"}, ...],
#   "asks": [{"price": "0.56", "size": "150"}, ...],
#   "hash": "...",
#   "timestamp": "1707000000000"
# }
```

### 4.4 Types d'Ordres

| Type | Code | Description |
|------|------|-------------|
| **GTC** | `OrderType.GTC` | Good Till Cancelled - Reste dans l'orderbook |
| **FOK** | `OrderType.FOK` | Fill Or Kill - Exécution immédiate totale ou annulation |
| **GTD** | `OrderType.GTD` | Good Till Date - Expire à une date donnée |

### 4.5 Placer un Ordre (via py-clob-client)

```python
from py_clob_client.client import ClobClient
from py_clob_client.clob_types import OrderArgs, MarketOrderArgs, OrderType
from py_clob_client.order_builder.constants import BUY, SELL

# Initialisation du client (voir section Authentification)
client = ClobClient(...)

# LIMIT ORDER - Ordre avec prix spécifique
order_args = OrderArgs(
    token_id="12345678901234567890...",
    price=0.55,      # Prix limite
    size=10.0,       # Nombre de shares
    side=BUY         # ou SELL
)
signed_order = client.create_order(order_args)
response = client.post_order(signed_order, OrderType.GTC)

# MARKET ORDER - Exécution immédiate au meilleur prix
market_order = MarketOrderArgs(
    token_id="12345678901234567890...",
    amount=25.0,     # Montant en USDC (pour BUY) ou shares (pour SELL)
    side=BUY
)
signed_market = client.create_market_order(market_order)
response = client.post_order(signed_market, OrderType.FOK)
```

### 4.6 Annuler un Ordre

```python
# Annuler un ordre spécifique
client.cancel(order_id="order-uuid-here")

# Annuler tous les ordres
client.cancel_all()

# Annuler tous les ordres d'un marché
client.cancel_market_orders(market="0x...")
```

---

## 5. WebSocket & RTDS - Données Temps Réel

### 5.1 RTDS - Prix Crypto en Temps Réel

Le Real-Time Data Socket fournit les prix des cryptos depuis Binance et Chainlink.

```python
import websocket
import json

def on_message(ws, message):
    data = json.loads(message)
    print(f"Prix {data['payload']['symbol']}: {data['payload']['value']}")

def on_open(ws):
    # S'abonner aux prix BTC, ETH, SOL, XRP
    subscribe_msg = {
        "action": "subscribe",
        "subscriptions": [
            {
                "topic": "crypto_prices",
                "type": "update",
                "filters": "btcusdt,ethusdt,solusdt,xrpusdt"
            }
        ]
    }
    ws.send(json.dumps(subscribe_msg))

ws = websocket.WebSocketApp(
    "wss://rtds.polymarket.com/v1/ws",
    on_message=on_message,
    on_open=on_open
)
ws.run_forever()
```

### 5.2 Format des Messages RTDS

```json
{
  "topic": "crypto_prices",
  "type": "update",
  "timestamp": 1707000000000,
  "payload": {
    "symbol": "btcusdt",
    "timestamp": 1707000000000,
    "value": 97234.50
  }
}
```

### 5.3 CLOB WebSocket - Orderbook Temps Réel

```python
import websocket
import json

def on_message(ws, message):
    data = json.loads(message)
    event_type = data.get("event_type")
    
    if event_type == "book":
        # Mise à jour de l'orderbook
        print(f"Book update: {data}")
    elif event_type == "price_change":
        # Changement de prix
        print(f"Price change: {data}")
    elif event_type == "last_trade_price":
        # Dernier trade
        print(f"Last trade: {data}")

def on_open(ws):
    # S'abonner à un marché
    subscribe_msg = {
        "type": "market",
        "assets_ids": [
            "12345678901234567890...",  # Token ID UP
            "98765432109876543210..."   # Token ID DOWN
        ]
    }
    ws.send(json.dumps(subscribe_msg))

ws = websocket.WebSocketApp(
    "wss://clob.polymarket.com/ws/market",
    on_message=on_message,
    on_open=on_open
)
ws.run_forever()
```

### 5.4 Types d'Events WebSocket CLOB

| Event | Description |
|-------|-------------|
| `book` | Mise à jour complète de l'orderbook |
| `price_change` | Changement de prix (bid/ask) |
| `tick_size_change` | Changement du tick size |
| `last_trade_price` | Prix du dernier trade exécuté |
| `best_bid_ask` | Meilleurs bid/ask |

---

## 6. Authentification

### 6.1 Niveaux d'Authentification

```
┌─────────────────────────────────────────────────────────────────┐
│                  NIVEAUX D'AUTHENTIFICATION                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  PUBLIC        Lecture données marché, prix, orderbook          │
│      │                                                          │
│      ▼                                                          │
│  L1 AUTH       Créer/Dériver API Keys (signature EIP-712)       │
│      │         Requiert: Private Key                            │
│      ▼                                                          │
│  L2 AUTH       Trading (ordres, annulations, historique)        │
│      │         Requiert: API Key + Secret + Passphrase          │
│      ▼                                                          │
│  BUILDER       Transactions gasless, attribution ordres         │
│                Requiert: Builder API Credentials                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 6.2 Configuration Initiale

```python
from py_clob_client.client import ClobClient
import os

HOST = "https://clob.polymarket.com"
CHAIN_ID = 137  # Polygon Mainnet
PRIVATE_KEY = os.getenv("PRIVATE_KEY")

# Étape 1: Créer client temporaire pour générer les credentials
temp_client = ClobClient(
    host=HOST,
    chain_id=CHAIN_ID,
    key=PRIVATE_KEY
)

# Étape 2: Créer ou dériver les API credentials
# (createOrDeriveApiKey essaie d'abord de dériver, sinon crée)
api_creds = temp_client.create_or_derive_api_creds()

# Sauvegarder les credentials (à stocker de façon sécurisée!)
print(f"API Key: {api_creds['apiKey']}")
print(f"Secret: {api_creds['secret']}")
print(f"Passphrase: {api_creds['passphrase']}")
```

### 6.3 Client Complet pour Trading

```python
from py_clob_client.client import ClobClient
from py_clob_client.clob_types import ApiCreds

# Configuration
HOST = "https://clob.polymarket.com"
CHAIN_ID = 137
PRIVATE_KEY = os.getenv("PRIVATE_KEY")
FUNDER_ADDRESS = os.getenv("POLYMARKET_WALLET_ADDRESS")  # Adresse proxy Polymarket

# Credentials sauvegardées
api_creds = ApiCreds(
    api_key=os.getenv("POLY_API_KEY"),
    api_secret=os.getenv("POLY_API_SECRET"),
    api_passphrase=os.getenv("POLY_API_PASSPHRASE")
)

# Signature Type:
# 0 = EOA direct (MetaMask, Coinbase Wallet)
# 1 = Email/Magic wallet proxy
# 2 = Safe proxy wallet déployé

client = ClobClient(
    host=HOST,
    chain_id=CHAIN_ID,
    key=PRIVATE_KEY,
    creds=api_creds,
    signature_type=1,  # Ajuster selon votre type de wallet
    funder=FUNDER_ADDRESS  # Adresse qui détient les fonds
)
```

### 6.4 Trouver son Funder Address

L'adresse `funder` est l'adresse de votre proxy wallet Polymarket :
- Visible sur `https://polymarket.com/settings`
- C'est l'adresse où vous envoyez vos USDC pour trader

### 6.5 Builder Program (Gasless)

Pour des transactions sans gas, rejoindre le Builder Program :

```python
from py_clob_client.client import ClobClient

# Avec Builder credentials
client = ClobClient(
    host=HOST,
    chain_id=CHAIN_ID,
    key=PRIVATE_KEY,
    creds=api_creds,
    signature_type=2,
    funder=FUNDER_ADDRESS
)

# Les transactions sont relayées via Polymarket (gasless)
```

Variables d'environnement Builder :
```bash
POLY_BUILDER_API_KEY=your_key
POLY_BUILDER_API_SECRET=your_secret
POLY_BUILDER_API_PASSPHRASE=your_passphrase
```

---

## 7. Marchés Crypto 15 Minutes

### 7.1 Structure des Marchés 15 Min

Les marchés "Up or Down" 15 minutes sont des paris binaires sur la direction du prix d'une crypto dans les 15 prochaines minutes.

| Crypto | Format Question |
|--------|-----------------|
| BTC | "Bitcoin Up or Down - [Date], [Time] ET?" |
| ETH | "Ethereum Up or Down - [Date], [Time] ET?" |
| SOL | "Solana Up or Down - [Date], [Time] ET?" |
| XRP | "XRP Up or Down - [Date], [Time] ET?" |

### 7.2 Trouver les Marchés 15 Minutes

```python
import requests
from datetime import datetime, timezone

def find_15min_crypto_markets():
    """Trouve les marchés crypto 15 minutes actifs"""
    
    # Rechercher par tag "15 Min" ou dans la catégorie crypto
    response = requests.get(
        "https://gamma-api.polymarket.com/markets",
        params={
            "closed": "false",
            "active": "true",
            "limit": 100
        }
    )
    markets = response.json()
    
    # Filtrer les marchés 15 minutes crypto
    crypto_keywords = ["bitcoin", "ethereum", "solana", "xrp", "btc", "eth", "sol"]
    fifteen_min_markets = []
    
    for market in markets:
        question = market.get("question", "").lower()
        
        # Vérifier si c'est un marché crypto Up/Down
        is_crypto = any(kw in question for kw in crypto_keywords)
        is_updown = "up or down" in question
        
        if is_crypto and is_updown:
            # Vérifier si c'est un marché 15 min (par les tags ou le timing)
            fifteen_min_markets.append({
                "id": market["id"],
                "question": market["question"],
                "tokens": market.get("tokens", []),
                "end_date": market.get("endDate"),
                "tick_size": market.get("minimum_tick_size", "0.01"),
                "neg_risk": market.get("neg_risk", False)
            })
    
    return fifteen_min_markets

# Utilisation
markets = find_15min_crypto_markets()
for m in markets:
    print(f"{m['question']}")
    for token in m['tokens']:
        print(f"  - {token['outcome']}: {token['token_id'][:20]}...")
```

### 7.3 Identifier UP vs DOWN Token

```python
def get_up_down_tokens(market):
    """Extrait les token IDs pour UP et DOWN d'un marché"""
    tokens = market.get("tokens", [])
    
    up_token = None
    down_token = None
    
    for token in tokens:
        outcome = token.get("outcome", "").lower()
        if outcome in ["up", "yes"]:
            up_token = token["token_id"]
        elif outcome in ["down", "no"]:
            down_token = token["token_id"]
    
    return up_token, down_token
```

### 7.4 Prix d'Entrée et Sorties

```python
def analyze_market(client, up_token_id, down_token_id):
    """Analyse les prix d'un marché Up/Down"""
    
    # Prix UP
    up_book = client.get_order_book(up_token_id)
    up_best_bid = float(up_book["bids"][0]["price"]) if up_book["bids"] else 0
    up_best_ask = float(up_book["asks"][0]["price"]) if up_book["asks"] else 1
    up_mid = (up_best_bid + up_best_ask) / 2
    
    # Prix DOWN (devrait être ~= 1 - UP)
    down_book = client.get_order_book(down_token_id)
    down_best_bid = float(down_book["bids"][0]["price"]) if down_book["bids"] else 0
    down_best_ask = float(down_book["asks"][0]["price"]) if down_book["asks"] else 1
    down_mid = (down_best_bid + down_best_ask) / 2
    
    return {
        "up": {
            "bid": up_best_bid,
            "ask": up_best_ask,
            "mid": up_mid,
            "spread": up_best_ask - up_best_bid
        },
        "down": {
            "bid": down_best_bid,
            "ask": down_best_ask,
            "mid": down_mid,
            "spread": down_best_ask - down_best_bid
        },
        "implied_probability_up": up_mid,
        "implied_probability_down": down_mid
    }
```

---

## 8. Architecture du Bot

### 8.1 Diagramme d'Architecture

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

### 8.2 Structure des Fichiers

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
│   │   └── orderbook.py       # WebSocket orderbook
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
│   │   ├── updown_15min.py    # Stratégie Up/Down 15 min
│   │   └── signals.py         # Génération de signaux
│   │
│   └── utils/
│       ├── __init__.py
│       ├── logger.py          # Logging
│       └── risk.py            # Gestion des risques
│
├── tests/
│   └── ...
│
├── config/
│   └── settings.yaml          # Configuration
│
├── .env                       # Variables d'environnement
├── requirements.txt
└── main.py                    # Point d'entrée
```

---

## 9. Implémentation Python

### 9.1 Configuration (config.py)

```python
import os
from dataclasses import dataclass
from dotenv import load_dotenv

load_dotenv()

@dataclass
class Config:
    # API Endpoints
    CLOB_HOST: str = "https://clob.polymarket.com"
    GAMMA_HOST: str = "https://gamma-api.polymarket.com"
    RTDS_WS: str = "wss://rtds.polymarket.com/v1/ws"
    CLOB_WS: str = "wss://clob.polymarket.com/ws"
    
    # Blockchain
    CHAIN_ID: int = 137  # Polygon
    
    # Authentication
    PRIVATE_KEY: str = os.getenv("PRIVATE_KEY", "")
    FUNDER_ADDRESS: str = os.getenv("POLYMARKET_WALLET_ADDRESS", "")
    SIGNATURE_TYPE: int = int(os.getenv("SIGNATURE_TYPE", "1"))
    
    API_KEY: str = os.getenv("POLY_API_KEY", "")
    API_SECRET: str = os.getenv("POLY_API_SECRET", "")
    API_PASSPHRASE: str = os.getenv("POLY_API_PASSPHRASE", "")
    
    # Builder (optionnel - gasless)
    BUILDER_API_KEY: str = os.getenv("POLY_BUILDER_API_KEY", "")
    BUILDER_SECRET: str = os.getenv("POLY_BUILDER_SECRET", "")
    BUILDER_PASSPHRASE: str = os.getenv("POLY_BUILDER_PASSPHRASE", "")
    
    # Trading
    MAX_POSITION_SIZE: float = 100.0  # USDC
    STOP_LOSS_PCT: float = 0.05       # 5%
    TAKE_PROFIT_PCT: float = 0.10     # 10%
    
    # Cryptos à trader
    CRYPTO_SYMBOLS: list = None
    
    def __post_init__(self):
        if self.CRYPTO_SYMBOLS is None:
            self.CRYPTO_SYMBOLS = ["btcusdt", "ethusdt", "solusdt", "xrpusdt"]

config = Config()
```

### 9.2 Client CLOB (client.py)

```python
from py_clob_client.client import ClobClient
from py_clob_client.clob_types import ApiCreds, OrderArgs, MarketOrderArgs, OrderType
from py_clob_client.order_builder.constants import BUY, SELL
from .config import config
import logging

logger = logging.getLogger(__name__)

class TradingClient:
    def __init__(self):
        self.client = None
        self._initialize_client()
    
    def _initialize_client(self):
        """Initialise le client CLOB avec authentification"""
        api_creds = ApiCreds(
            api_key=config.API_KEY,
            api_secret=config.API_SECRET,
            api_passphrase=config.API_PASSPHRASE
        )
        
        self.client = ClobClient(
            host=config.CLOB_HOST,
            chain_id=config.CHAIN_ID,
            key=config.PRIVATE_KEY,
            creds=api_creds,
            signature_type=config.SIGNATURE_TYPE,
            funder=config.FUNDER_ADDRESS
        )
        logger.info("CLOB Client initialized")
    
    def get_price(self, token_id: str, side: str = "BUY") -> float:
        """Récupère le prix actuel"""
        response = self.client.get_price(token_id, side)
        return float(response.get("price", 0))
    
    def get_orderbook(self, token_id: str) -> dict:
        """Récupère l'orderbook"""
        return self.client.get_order_book(token_id)
    
    def place_limit_order(
        self, 
        token_id: str, 
        price: float, 
        size: float, 
        side: str,
        tick_size: str = "0.01",
        neg_risk: bool = False
    ) -> dict:
        """Place un ordre limite"""
        order_side = BUY if side.upper() == "BUY" else SELL
        
        order_args = OrderArgs(
            token_id=token_id,
            price=price,
            size=size,
            side=order_side
        )
        
        signed_order = self.client.create_order(order_args)
        response = self.client.post_order(
            signed_order, 
            OrderType.GTC,
            tick_size=tick_size,
            neg_risk=neg_risk
        )
        
        logger.info(f"Limit order placed: {response}")
        return response
    
    def place_market_order(
        self, 
        token_id: str, 
        amount: float, 
        side: str
    ) -> dict:
        """Place un ordre market (FOK)"""
        order_side = BUY if side.upper() == "BUY" else SELL
        
        market_order = MarketOrderArgs(
            token_id=token_id,
            amount=amount,
            side=order_side
        )
        
        signed_order = self.client.create_market_order(market_order)
        response = self.client.post_order(signed_order, OrderType.FOK)
        
        logger.info(f"Market order placed: {response}")
        return response
    
    def cancel_order(self, order_id: str) -> dict:
        """Annule un ordre"""
        return self.client.cancel(order_id)
    
    def get_open_orders(self) -> list:
        """Récupère les ordres ouverts"""
        return self.client.get_orders()
    
    def get_trades(self) -> list:
        """Récupère l'historique des trades"""
        return self.client.get_trades()
```

### 9.3 Client Gamma (gamma_client.py)

```python
import requests
from typing import List, Optional, Dict
import logging

logger = logging.getLogger(__name__)

class GammaClient:
    BASE_URL = "https://gamma-api.polymarket.com"
    
    def get_events(
        self, 
        closed: bool = False, 
        limit: int = 50,
        offset: int = 0
    ) -> List[dict]:
        """Récupère les events"""
        response = requests.get(
            f"{self.BASE_URL}/events",
            params={
                "closed": str(closed).lower(),
                "limit": limit,
                "offset": offset
            }
        )
        response.raise_for_status()
        return response.json()
    
    def get_markets(
        self, 
        closed: bool = False,
        active: bool = True,
        limit: int = 100,
        tag_slug: Optional[str] = None
    ) -> List[dict]:
        """Récupère les markets"""
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
    
    def get_market_by_slug(self, slug: str) -> Optional[dict]:
        """Récupère un market par son slug"""
        response = requests.get(f"{self.BASE_URL}/markets", params={"slug": slug})
        response.raise_for_status()
        markets = response.json()
        return markets[0] if markets else None
    
    def find_15min_crypto_markets(self, crypto: str = "btc") -> List[dict]:
        """Trouve les marchés 15 minutes pour une crypto"""
        markets = self.get_markets(closed=False, active=True, limit=200)
        
        crypto_map = {
            "btc": ["bitcoin", "btc"],
            "eth": ["ethereum", "eth"],
            "sol": ["solana", "sol"],
            "xrp": ["xrp"]
        }
        
        keywords = crypto_map.get(crypto.lower(), [crypto.lower()])
        filtered = []
        
        for market in markets:
            question = market.get("question", "").lower()
            if any(kw in question for kw in keywords) and "up or down" in question:
                filtered.append(self._format_market(market))
        
        return filtered
    
    def _format_market(self, market: dict) -> dict:
        """Formate un market pour le bot"""
        tokens = market.get("tokens", [])
        up_token = None
        down_token = None
        
        for token in tokens:
            outcome = token.get("outcome", "").lower()
            if outcome in ["up", "yes"]:
                up_token = {
                    "token_id": token["token_id"],
                    "price": float(token.get("price", 0))
                }
            elif outcome in ["down", "no"]:
                down_token = {
                    "token_id": token["token_id"],
                    "price": float(token.get("price", 0))
                }
        
        return {
            "id": market.get("id"),
            "question": market.get("question"),
            "slug": market.get("slug"),
            "condition_id": market.get("conditionId"),
            "up_token": up_token,
            "down_token": down_token,
            "end_date": market.get("endDate"),
            "tick_size": market.get("minimum_tick_size", "0.01"),
            "neg_risk": market.get("neg_risk", False),
            "volume_24h": float(market.get("volume24hr", 0)),
            "liquidity": float(market.get("liquidity", 0))
        }
```

### 9.4 Client RTDS WebSocket (rtds_client.py)

```python
import websocket
import json
import threading
from typing import Callable, Dict, List
import logging

logger = logging.getLogger(__name__)

class RTDSClient:
    """Client WebSocket pour les prix crypto en temps réel"""
    
    WS_URL = "wss://rtds.polymarket.com/v1/ws"
    
    def __init__(self, on_price_update: Callable[[str, float], None]):
        self.on_price_update = on_price_update
        self.ws = None
        self.running = False
        self.prices: Dict[str, float] = {}
        self._thread = None
    
    def connect(self, symbols: List[str]):
        """Connecte au WebSocket et s'abonne aux symboles"""
        self.symbols = symbols
        self.running = True
        
        self.ws = websocket.WebSocketApp(
            self.WS_URL,
            on_message=self._on_message,
            on_error=self._on_error,
            on_close=self._on_close,
            on_open=lambda ws: self._on_open(ws, symbols)
        )
        
        self._thread = threading.Thread(target=self.ws.run_forever)
        self._thread.daemon = True
        self._thread.start()
        logger.info(f"RTDS WebSocket connecting for symbols: {symbols}")
    
    def _on_open(self, ws, symbols: List[str]):
        """Callback à l'ouverture de la connexion"""
        subscribe_msg = {
            "action": "subscribe",
            "subscriptions": [
                {
                    "topic": "crypto_prices",
                    "type": "update",
                    "filters": ",".join(symbols)
                }
            ]
        }
        ws.send(json.dumps(subscribe_msg))
        logger.info("RTDS WebSocket connected and subscribed")
        
        # Démarrer le ping/pong
        self._start_ping()
    
    def _on_message(self, ws, message):
        """Callback à la réception d'un message"""
        try:
            data = json.loads(message)
            if data.get("topic") == "crypto_prices":
                payload = data.get("payload", {})
                symbol = payload.get("symbol")
                price = payload.get("value")
                
                if symbol and price:
                    self.prices[symbol] = float(price)
                    self.on_price_update(symbol, float(price))
        except Exception as e:
            logger.error(f"Error processing RTDS message: {e}")
    
    def _on_error(self, ws, error):
        """Callback en cas d'erreur"""
        logger.error(f"RTDS WebSocket error: {error}")
    
    def _on_close(self, ws, close_status_code, close_msg):
        """Callback à la fermeture"""
        logger.warning(f"RTDS WebSocket closed: {close_status_code} - {close_msg}")
        if self.running:
            # Reconnexion automatique
            self.connect(self.symbols)
    
    def _start_ping(self):
        """Envoie des pings réguliers pour maintenir la connexion"""
        def ping():
            while self.running and self.ws:
                try:
                    self.ws.send(json.dumps({"action": "ping"}))
                except:
                    pass
                threading.Event().wait(5)  # Ping toutes les 5 secondes
        
        ping_thread = threading.Thread(target=ping)
        ping_thread.daemon = True
        ping_thread.start()
    
    def get_price(self, symbol: str) -> float:
        """Retourne le dernier prix connu pour un symbole"""
        return self.prices.get(symbol, 0)
    
    def disconnect(self):
        """Déconnecte du WebSocket"""
        self.running = False
        if self.ws:
            self.ws.close()
```

### 9.5 Stratégie Up/Down 15 Min (updown_15min.py)

```python
from dataclasses import dataclass
from typing import Optional, Tuple
from enum import Enum
import time
import logging

logger = logging.getLogger(__name__)

class Signal(Enum):
    BUY_UP = "buy_up"
    BUY_DOWN = "buy_down"
    HOLD = "hold"
    EXIT = "exit"

@dataclass
class TradeSignal:
    signal: Signal
    token_id: str
    price: float
    size: float
    confidence: float
    reason: str

class UpDownStrategy:
    """
    Stratégie de trading pour les marchés Up/Down 15 minutes
    
    Logique:
    - Compare le prix actuel de la crypto avec le prix de départ du marché
    - Identifie la tendance sur les dernières minutes
    - Génère un signal d'achat UP ou DOWN basé sur le momentum
    """
    
    def __init__(
        self,
        price_history_window: int = 60,  # Secondes d'historique
        momentum_threshold: float = 0.001,  # 0.1% de mouvement minimum
        min_probability_edge: float = 0.05  # Edge minimum sur le prix
    ):
        self.price_history_window = price_history_window
        self.momentum_threshold = momentum_threshold
        self.min_probability_edge = min_probability_edge
        
        # Historique des prix (symbol -> [(timestamp, price), ...])
        self.price_history = {}
    
    def update_price(self, symbol: str, price: float):
        """Met à jour l'historique des prix"""
        current_time = time.time()
        
        if symbol not in self.price_history:
            self.price_history[symbol] = []
        
        self.price_history[symbol].append((current_time, price))
        
        # Nettoyer l'historique ancien
        cutoff = current_time - self.price_history_window
        self.price_history[symbol] = [
            (t, p) for t, p in self.price_history[symbol] if t > cutoff
        ]
    
    def calculate_momentum(self, symbol: str) -> Tuple[float, float]:
        """
        Calcule le momentum du prix
        Retourne: (momentum, confidence)
        - momentum > 0 = tendance haussière
        - momentum < 0 = tendance baissière
        - confidence = fiabilité du signal (0-1)
        """
        history = self.price_history.get(symbol, [])
        
        if len(history) < 5:
            return 0, 0
        
        # Prix de début et de fin
        start_price = history[0][1]
        end_price = history[-1][1]
        
        # Momentum = variation en pourcentage
        momentum = (end_price - start_price) / start_price
        
        # Calcul de la confiance basé sur la consistance de la tendance
        price_changes = []
        for i in range(1, len(history)):
            change = (history[i][1] - history[i-1][1]) / history[i-1][1]
            price_changes.append(change)
        
        # Si la majorité des changements vont dans la même direction
        positive_changes = sum(1 for c in price_changes if c > 0)
        negative_changes = sum(1 for c in price_changes if c < 0)
        
        total_changes = len(price_changes)
        if total_changes == 0:
            return momentum, 0
        
        # Confiance = proportion de changements dans la direction dominante
        if momentum > 0:
            confidence = positive_changes / total_changes
        else:
            confidence = negative_changes / total_changes
        
        return momentum, confidence
    
    def generate_signal(
        self,
        symbol: str,
        up_token: dict,
        down_token: dict,
        trade_size: float
    ) -> Optional[TradeSignal]:
        """
        Génère un signal de trading
        
        Args:
            symbol: Symbole crypto (ex: "btcusdt")
            up_token: {"token_id": "...", "price": 0.55}
            down_token: {"token_id": "...", "price": 0.45}
            trade_size: Taille du trade en USDC
        
        Returns:
            TradeSignal ou None si pas de signal
        """
        momentum, confidence = self.calculate_momentum(symbol)
        
        # Pas assez de momentum
        if abs(momentum) < self.momentum_threshold:
            return TradeSignal(
                signal=Signal.HOLD,
                token_id="",
                price=0,
                size=0,
                confidence=confidence,
                reason=f"Momentum insuffisant: {momentum:.4f}"
            )
        
        # Confiance trop faible
        if confidence < 0.6:
            return TradeSignal(
                signal=Signal.HOLD,
                token_id="",
                price=0,
                size=0,
                confidence=confidence,
                reason=f"Confiance insuffisante: {confidence:.2f}"
            )
        
        # Déterminer la direction
        if momentum > 0:
            # Tendance haussière -> acheter UP
            target_price = up_token["price"]
            
            # Vérifier qu'on a un edge
            # Si momentum fort mais prix UP déjà élevé, pas d'edge
            expected_probability = 0.5 + abs(momentum) * 10  # Approximation
            expected_probability = min(expected_probability, 0.95)
            
            edge = expected_probability - target_price
            
            if edge < self.min_probability_edge:
                return TradeSignal(
                    signal=Signal.HOLD,
                    token_id="",
                    price=0,
                    size=0,
                    confidence=confidence,
                    reason=f"Edge insuffisant: {edge:.2f} (expected: {expected_probability:.2f}, price: {target_price:.2f})"
                )
            
            return TradeSignal(
                signal=Signal.BUY_UP,
                token_id=up_token["token_id"],
                price=target_price,
                size=trade_size / target_price,  # Convertir USDC en shares
                confidence=confidence,
                reason=f"Momentum UP: {momentum:.4f}, edge: {edge:.2f}"
            )
        
        else:
            # Tendance baissière -> acheter DOWN
            target_price = down_token["price"]
            
            expected_probability = 0.5 + abs(momentum) * 10
            expected_probability = min(expected_probability, 0.95)
            
            edge = expected_probability - target_price
            
            if edge < self.min_probability_edge:
                return TradeSignal(
                    signal=Signal.HOLD,
                    token_id="",
                    price=0,
                    size=0,
                    confidence=confidence,
                    reason=f"Edge insuffisant: {edge:.2f}"
                )
            
            return TradeSignal(
                signal=Signal.BUY_DOWN,
                token_id=down_token["token_id"],
                price=target_price,
                size=trade_size / target_price,
                confidence=confidence,
                reason=f"Momentum DOWN: {momentum:.4f}, edge: {edge:.2f}"
            )
```

### 9.6 Main Bot (main.py)

```python
import asyncio
import logging
from datetime import datetime, timezone
import time

from src.config import config
from src.client import TradingClient
from src.data.gamma_client import GammaClient
from src.data.rtds_client import RTDSClient
from src.strategy.updown_15min import UpDownStrategy, Signal

# Configuration logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

class TradingBot:
    def __init__(self):
        self.trading_client = TradingClient()
        self.gamma_client = GammaClient()
        self.strategy = UpDownStrategy()
        self.rtds_client = None
        
        self.active_markets = {}  # {symbol: market_info}
        self.positions = {}  # {token_id: position_info}
        
    def on_price_update(self, symbol: str, price: float):
        """Callback appelé à chaque mise à jour de prix"""
        # Mettre à jour la stratégie
        self.strategy.update_price(symbol, price)
        
        # Vérifier si on a un marché actif pour ce symbole
        if symbol in self.active_markets:
            self._process_market(symbol, price)
    
    def _process_market(self, symbol: str, current_price: float):
        """Traite un marché et génère potentiellement un trade"""
        market = self.active_markets[symbol]
        
        # Générer un signal
        signal = self.strategy.generate_signal(
            symbol=symbol,
            up_token=market["up_token"],
            down_token=market["down_token"],
            trade_size=config.MAX_POSITION_SIZE
        )
        
        if signal and signal.signal in [Signal.BUY_UP, Signal.BUY_DOWN]:
            logger.info(f"Signal généré: {signal}")
            
            # Vérifier qu'on n'a pas déjà une position
            if signal.token_id not in self.positions:
                self._execute_trade(signal, market)
    
    def _execute_trade(self, signal, market):
        """Exécute un trade"""
        try:
            # Placer un ordre market (FOK)
            response = self.trading_client.place_market_order(
                token_id=signal.token_id,
                amount=signal.size * signal.price,  # USDC
                side="BUY"
            )
            
            if response.get("success"):
                self.positions[signal.token_id] = {
                    "entry_price": signal.price,
                    "size": signal.size,
                    "entry_time": time.time(),
                    "market": market
                }
                logger.info(f"Trade exécuté: {response}")
            else:
                logger.warning(f"Trade échoué: {response}")
                
        except Exception as e:
            logger.error(f"Erreur lors du trade: {e}")
    
    def refresh_markets(self):
        """Rafraîchit la liste des marchés actifs"""
        for crypto in ["btc", "eth", "sol", "xrp"]:
            markets = self.gamma_client.find_15min_crypto_markets(crypto)
            
            if markets:
                # Prendre le prochain marché à expirer
                market = sorted(markets, key=lambda m: m["end_date"])[0]
                symbol = f"{crypto}usdt"
                self.active_markets[symbol] = market
                logger.info(f"Marché actif pour {symbol}: {market['question']}")
    
    def start(self):
        """Démarre le bot"""
        logger.info("Démarrage du bot...")
        
        # Initialiser les marchés
        self.refresh_markets()
        
        # Connecter au WebSocket RTDS
        self.rtds_client = RTDSClient(self.on_price_update)
        self.rtds_client.connect(config.CRYPTO_SYMBOLS)
        
        # Boucle principale
        try:
            while True:
                # Rafraîchir les marchés toutes les 5 minutes
                self.refresh_markets()
                time.sleep(300)
                
        except KeyboardInterrupt:
            logger.info("Arrêt du bot...")
            if self.rtds_client:
                self.rtds_client.disconnect()

if __name__ == "__main__":
    bot = TradingBot()
    bot.start()
```

---

## 10. Stratégie de Trading Up/Down

### 10.1 Logique de Base

```
┌─────────────────────────────────────────────────────────────────┐
│              STRATÉGIE UP/DOWN 15 MINUTES                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. COLLECTE DE DONNÉES                                         │
│     • Prix crypto en temps réel (RTDS)                          │
│     • Historique des 60 dernières secondes                      │
│                                                                 │
│  2. ANALYSE                                                     │
│     • Calcul du momentum (variation %)                          │
│     • Calcul de la consistance (confiance)                      │
│     • Identification de la tendance                             │
│                                                                 │
│  3. DÉCISION                                                    │
│     • Momentum > seuil ET confiance > 60%                       │
│     • Edge (expected - market price) > 5%                       │
│                                                                 │
│  4. EXÉCUTION                                                   │
│     • Ordre market (FOK) pour exécution immédiate               │
│     • Taille limitée par config                                 │
│                                                                 │
│  5. GESTION                                                     │
│     • Laisser l'ordre jusqu'à résolution                        │
│     • Ou exit si retournement de tendance                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 10.2 Paramètres de la Stratégie

| Paramètre | Valeur Suggérée | Description |
|-----------|-----------------|-------------|
| `price_history_window` | 60s | Fenêtre d'historique des prix |
| `momentum_threshold` | 0.001 (0.1%) | Mouvement minimum pour signal |
| `min_probability_edge` | 0.05 (5%) | Edge minimum requis |
| `confidence_threshold` | 0.6 (60%) | Confiance minimum |

### 10.3 Calcul du Momentum

```python
def calculate_momentum(prices: list) -> float:
    """
    prices = [(timestamp, price), ...]
    """
    if len(prices) < 2:
        return 0
    
    start_price = prices[0][1]
    end_price = prices[-1][1]
    
    momentum = (end_price - start_price) / start_price
    return momentum  # Positif = hausse, Négatif = baisse
```

### 10.4 Calcul de l'Edge

```python
def calculate_edge(momentum: float, market_price: float) -> float:
    """
    Estime la probabilité réelle basée sur le momentum
    et compare avec le prix du marché
    """
    # Approximation: momentum de 0.5% = +5% de probabilité
    expected_probability = 0.5 + (momentum * 10)
    expected_probability = max(0.1, min(0.9, expected_probability))
    
    edge = expected_probability - market_price
    return edge
```

---

## 11. Gestion des Risques

### 11.1 Limites de Position

```python
class RiskManager:
    def __init__(
        self,
        max_position_size: float = 100,    # USDC max par trade
        max_total_exposure: float = 500,   # USDC total max
        max_positions: int = 5,            # Nombre max de positions
        stop_loss_pct: float = 0.15,       # 15% stop loss
        take_profit_pct: float = 0.30      # 30% take profit
    ):
        self.max_position_size = max_position_size
        self.max_total_exposure = max_total_exposure
        self.max_positions = max_positions
        self.stop_loss_pct = stop_loss_pct
        self.take_profit_pct = take_profit_pct
        
        self.current_positions = {}
    
    def can_open_position(self, size: float) -> bool:
        """Vérifie si on peut ouvrir une nouvelle position"""
        if len(self.current_positions) >= self.max_positions:
            return False
        
        current_exposure = sum(
            p["size"] * p["entry_price"] 
            for p in self.current_positions.values()
        )
        
        if current_exposure + size > self.max_total_exposure:
            return False
        
        if size > self.max_position_size:
            return False
        
        return True
    
    def check_exit_conditions(self, position: dict, current_price: float) -> str:
        """
        Vérifie les conditions de sortie
        Retourne: "hold", "stop_loss", "take_profit"
        """
        entry_price = position["entry_price"]
        
        # Stop loss
        if current_price <= entry_price * (1 - self.stop_loss_pct):
            return "stop_loss"
        
        # Take profit
        if current_price >= entry_price * (1 + self.take_profit_pct):
            return "take_profit"
        
        return "hold"
```

### 11.2 Règles de Trading

1. **Taille de position**: Maximum 100 USDC par trade
2. **Exposition totale**: Maximum 500 USDC en positions ouvertes
3. **Diversification**: Maximum 5 positions simultanées
4. **Stop Loss**: -15% de la valeur d'entrée
5. **Take Profit**: +30% de la valeur d'entrée

### 11.3 Checklist Avant Trading

- [ ] Wallet Polygon funded avec USDC
- [ ] Petit montant de MATIC/POL pour le gas (si pas Builder)
- [ ] API credentials générées et sauvegardées
- [ ] Funder address correcte
- [ ] Connexion WebSocket stable
- [ ] Markets crypto 15 min disponibles

---

## 12. Ressources et Références

### 12.1 Documentation Officielle

| Ressource | URL |
|-----------|-----|
| Documentation Polymarket | https://docs.polymarket.com |
| py-clob-client GitHub | https://github.com/Polymarket/py-clob-client |
| clob-client (TypeScript) | https://github.com/Polymarket/clob-client |
| Polymarket Agents | https://github.com/Polymarket/agents |

### 12.2 SDKs et Librairies

```bash
# Python
pip install py-clob-client

# TypeScript/JavaScript
npm install @polymarket/clob-client
npm install @polymarket/builder-relayer-client
npm install @polymarket/builder-signing-sdk
```

### 12.3 Endpoints de Référence

| Service | Endpoint |
|---------|----------|
| CLOB API | https://clob.polymarket.com |
| Gamma API | https://gamma-api.polymarket.com |
| Data API | https://data-api.polymarket.com |
| Relayer | https://relayer-v2.polymarket.com |
| RTDS WebSocket | wss://rtds.polymarket.com/v1/ws |
| CLOB WebSocket | wss://clob.polymarket.com/ws/market |

### 12.4 Adresses de Contrats (Polygon)

| Contrat | Adresse |
|---------|---------|
| USDC | 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174 |
| CTF Exchange (Current) | 0xC5d563A36AE78145C45a50134d48A1215220f80a |
| CTF Exchange (Legacy) | 0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E |

### 12.5 Exporter sa Private Key (Magic/Email Login)

Si tu utilises un compte email Polymarket:
1. Va sur https://reveal.magic.link/polymarket
2. Connecte-toi avec le même email
3. Exporte ta private key

### 12.6 Rate Limits API

| API | Limite |
|-----|--------|
| CLOB (lecture) | 100 req/min |
| CLOB (trading) | Variable selon tier |
| Gamma | 1000 req/heure |
| Data | Variable |

---

## Notes Importantes

### Restrictions Géographiques

⚠️ **Polymarket est interdit aux résidents américains et de certaines autres juridictions.** L'utilisation de l'API est soumise aux mêmes restrictions.

### Avertissement sur les Risques

Le trading automatisé comporte des risques significatifs:
- Perte potentielle de capital
- Risques techniques (bugs, déconnexions)
- Volatilité des marchés
- Risques de liquidité

**Ne tradez jamais plus que ce que vous pouvez vous permettre de perdre.**

---

*Documentation générée le 4 février 2025*
*Version 1.0*