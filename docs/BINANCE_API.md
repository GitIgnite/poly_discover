# Binance Public API Reference

Reference documentation for the Binance REST API endpoints used by Poly Discover.

Base URL: `https://api.binance.com`

## Endpoints Used

### GET `/api/v3/klines` (Primary)

Fetches candlestick (OHLCV) data. This is the main data source for all backtesting.

**Parameters:**

| Parameter   | Type   | Required | Description                        |
|-------------|--------|----------|------------------------------------|
| `symbol`    | STRING | Yes      | Trading pair (e.g., `BTCUSDT`)     |
| `interval`  | STRING | Yes      | Kline interval (`15m` for Poly)    |
| `startTime` | LONG   | No       | Start time in ms (inclusive)       |
| `endTime`   | LONG   | No       | End time in ms (inclusive)         |
| `limit`     | INT    | No       | Number of results (default 500, max 1000) |

**Response:** Array of arrays:

```json
[
  [
    1499040000000,      // Open time
    "0.01634000",       // Open
    "0.80000000",       // High
    "0.01575800",       // Low
    "0.01577100",       // Close
    "148976.11427815",  // Volume
    1499644799999,      // Close time
    "2434.19055334",    // Quote asset volume
    308,                // Number of trades
    "1756.87402397",    // Taker buy base asset volume
    "28.46694368",      // Taker buy quote asset volume
    "17928899.62484339" // Ignore
  ]
]
```

**Pagination:** Max 1000 klines per request. The client paginates automatically using `startTime` increments for longer periods. 90 days of 15m data = ~8640 klines (9 requests).

### GET `/api/v3/ticker/price`

Current price for a symbol.

**Parameters:**

| Parameter | Type   | Required | Description |
|-----------|--------|----------|-------------|
| `symbol`  | STRING | No       | If omitted, returns all symbols |

**Response:**

```json
{
  "symbol": "BTCUSDT",
  "price": "67432.50000000"
}
```

### GET `/api/v3/ticker/24hr`

24-hour rolling statistics.

**Parameters:**

| Parameter | Type   | Required | Description |
|-----------|--------|----------|-------------|
| `symbol`  | STRING | No       | If omitted, returns all symbols |

**Response (key fields):**

```json
{
  "symbol": "BTCUSDT",
  "priceChange": "1234.00",
  "priceChangePercent": "1.85",
  "weightedAvgPrice": "66543.21",
  "lastPrice": "67432.50",
  "volume": "23456.789",
  "quoteVolume": "1567890123.45",
  "highPrice": "68000.00",
  "lowPrice": "65500.00"
}
```

### GET `/api/v3/depth` (Available, not yet used)

Order book depth. Useful for future slippage estimation.

**Parameters:**

| Parameter | Type   | Required | Description |
|-----------|--------|----------|-------------|
| `symbol`  | STRING | Yes      | Trading pair |
| `limit`   | INT    | No       | Depth levels (5, 10, 20, 50, 100, 500, 1000, 5000) |

**Response:**

```json
{
  "lastUpdateId": 1027024,
  "bids": [["67430.00", "1.234"]],
  "asks": [["67431.00", "0.567"]]
}
```

## Rate Limits

| Type            | Limit              |
|-----------------|--------------------|
| Request weight  | 6000 per minute    |
| Orders          | 10 per second      |
| Raw requests    | 61000 per 5 min    |

**Klines endpoint weight:** 2 per request (at limit=1000).

Our paginated klines fetch for 90 days uses ~9 requests = weight 18, well within limits.

## Supported Symbols

For Polymarket 15-min crypto markets:

| Symbol     | Asset          |
|------------|----------------|
| `BTCUSDT`  | Bitcoin        |
| `ETHUSDT`  | Ethereum       |
| `SOLUSDT`  | Solana         |
| `XRPUSDT`  | Ripple         |

## Kline Intervals

| Interval | Description |
|----------|-------------|
| `1m`     | 1 minute    |
| `3m`     | 3 minutes   |
| `5m`     | 5 minutes   |
| `15m`    | 15 minutes (**used by Poly Discover**) |
| `30m`    | 30 minutes  |
| `1h`     | 1 hour      |
| `4h`     | 4 hours     |
| `1d`     | 1 day       |

## Error Handling

HTTP 429: Rate limit exceeded. Response includes `Retry-After` header.

HTTP 418: IP banned (repeated rate limit violations). Duration in response.

The Binance client in `crates/engine/src/api/binance.rs` handles pagination and basic error reporting via `anyhow::Result`.
