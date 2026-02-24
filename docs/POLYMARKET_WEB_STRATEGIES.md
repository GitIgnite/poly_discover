# Polymarket Web-Researched Strategies — Guide complet de reimplementation

> Ce document decrit 5 strategies de trading specifiques aux marches de prediction Polymarket,
> backtestees sur des donnees Binance (klines 15 minutes). Chaque section contient suffisamment
> de details pour qu'une IA puisse recoder la strategie de A a Z dans n'importe quel langage.

---

## Contexte commun a toutes les strategies

### Source de donnees

Les strategies operent sur des **klines (bougies) Binance en 15 minutes**. Chaque kline contient :

```
Kline {
    open_time: i64,      // Timestamp de debut (ms epoch)
    open: Decimal,       // Prix d'ouverture
    high: Decimal,       // Plus haut sur la periode
    low: Decimal,        // Plus bas sur la periode
    close: Decimal,      // Prix de cloture (valeur principale)
    volume: Decimal,     // Volume echange
    close_time: i64,     // Timestamp de fin (ms epoch)
}
```

**API Binance** : `GET https://api.binance.com/api/v3/klines?symbol=BTCUSDT&interval=15m&limit=1000`

### Modele de frais Polymarket

Les frais taker Polymarket sont calcules selon :

```
fee = C × feeRate × (p × (1 - p))^exponent
```

- `C` = nombre de shares
- `feeRate` = 0.25 (defaut)
- `exponent` = 2
- `p` = probabilite estimee du marche

Les frais sont **maximaux a p = 0.50** et diminuent vers les extremes (0 ou 1).

### Fonction utilitaire : `estimate_poly_probability`

Convertit un mouvement de prix Binance en probabilite Polymarket estimee. Utilisee par les strategies ProbabilityEdge et FavoriteCompounder.

```python
def estimate_poly_probability(entry_price: float, current_price: float) -> float:
    """
    Mappe un changement de prix (%) en probabilite Polymarket [0.05, 0.95].
    - 0% de changement → p = 0.50 (fees maximum)
    - Mouvements importants → p vers les extremes (fees faibles)
    """
    if entry_price <= 0:
        return 0.50
    change_pct = (current_price - entry_price) / entry_price * 100
    p = clamp(0.5 + change_pct * 0.05, 0.05, 0.95)
    return round(p, 4)
```

### Interface commune : SignalGenerator

Chaque strategie implemente l'interface bar-by-bar suivante :

```python
class SignalGenerator:
    def name(self) -> str:
        """Nom de la strategie"""
        ...

    def on_bar(self, kline: Kline) -> SignalWithConfidence:
        """
        Recoit une bougie, retourne un signal :
        - Buy(confidence)  : acheter (confidence entre 0.3 et 1.0)
        - Sell(confidence) : vendre
        - Hold()           : ne rien faire
        """
        ...

    def reset(self):
        """Reinitialise l'etat interne (nouveau backtest)"""
        ...
```

La confidence est automatiquement clampee a `[0.3, 1.0]`.

---

## Strategie 1 : ProbabilityEdge

### Metadata

| Champ | Valeur |
|-------|--------|
| **Nom** | `Web:ProbabilityEdge` |
| **Categorie** | Edge (avantage informationnel) |
| **Risque** | Medium |
| **Source** | Polymarket prediction strategies, arbitrage informationnel sur marches de prediction |
| **Ref. code** | `crates/engine/src/web_strategies.rs` lignes 400-511 |

### Description

Estime la "vraie" probabilite d'un evenement via un **score composite multi-facteurs** (RSI + momentum + volatilite), puis compare cette estimation a la probabilite implicite du marche. Trade quand l'ecart (edge) depasse un seuil.

### Rationnel

Sur Polymarket, les prix refletent des probabilites. Si un modele multi-facteurs estime une probabilite differente du consensus, c'est une opportunite d'arbitrage informationnelle. L'idee est que le marche sous-reagit ou surreagit a court terme.

### Parametres

| Parametre | Description | Default | Aggressive | Conservative | Range aleatoire |
|-----------|-------------|---------|------------|--------------|-----------------|
| `edge_threshold` | Ecart minimum pour declencher un trade | 0.05 | 0.03 | 0.08 | [0.02, 0.10] |
| `rsi_period` | Periode du RSI | 14 | 7 | 21 | [5, 25] |
| `momentum_period` | Periode de la SMA momentum | 10 | 5 | 20 | [3, 25] |
| `vol_period` | Periode pour le calcul de volatilite | 20 | 10 | 30 | [8, 40] |

### Algorithme detaille

```
ETAT INTERNE :
    rsi: RSI(rsi_period)
    momentum_sma: SMA(momentum_period)
    vol_sma: SMA(vol_period)
    price_buffer: liste des N derniers close prices (taille max = vol_period)
    bars_seen: compteur de barres traitees
    baseline_price: premier close price vu (fixe au demarrage)

WARMUP :
    warmup = max(rsi_period, momentum_period, vol_period) + 5
    Retourner Hold() tant que bars_seen < warmup

A CHAQUE BARRE (kline) :
    close = kline.close
    bars_seen += 1

    1. Si baseline_price == 0 : baseline_price = close

    2. SIGNAL RSI (normalise a [-1, 1]) :
       rsi_val = RSI.next(close)             # RSI classique, valeur entre 0 et 100
       rsi_signal = (50.0 - rsi_val) / 50.0  # Oversold → positif, Overbought → negatif

    3. SIGNAL MOMENTUM (ecart prix vs SMA) :
       sma_val = SMA_momentum.next(close)
       momentum_signal = (close - sma_val) / sma_val   # Positif si prix au-dessus de SMA

    4. SIGNAL VOLATILITE (squeeze detection) :
       Ajouter close dans price_buffer (garder les vol_period derniers)
       Si price_buffer.length >= 3 :
           returns[] = pour chaque paire consecutive : (price[i+1] - price[i]) / price[i]
           mean = moyenne(returns)
           variance = moyenne((r - mean)^2 pour r dans returns)
           std_dev = sqrt(variance)
           vol_signal = clamp(0.01 - std_dev, -0.5, 0.5)
           # Low vol (squeeze) → valeur positive (signal de breakout potentiel)
       Sinon :
           vol_signal = 0.0

    5. SCORE COMPOSITE :
       composite = 0.4 * rsi_signal + 0.3 * momentum_signal + 0.3 * vol_signal

    6. ESTIMATION DE PROBABILITE :
       market_prob = estimate_poly_probability(baseline_price, close)  # Proba implicite du marche
       estimated_prob = clamp(market_prob + composite * 0.3, 0.05, 0.95)  # Notre estimation
       edge = estimated_prob - market_prob

    7. DECISION :
       Si edge > edge_threshold  → BUY  (confidence = edge * 5.0)
       Si edge < -edge_threshold → SELL (confidence = |edge| * 5.0)
       Sinon                     → HOLD
```

### Signification sur Polymarket

- **BUY** = acheter des shares YES (on pense que la probabilite vraie est plus haute que le marche)
- **SELL** = acheter des shares NO (on pense que la probabilite vraie est plus basse)
- L'edge represente l'avantage informationnel percu

---

## Strategie 2 : CatalystMomentum

### Metadata

| Champ | Valeur |
|-------|--------|
| **Nom** | `Web:CatalystMomentum` |
| **Categorie** | Momentum (suivi de tendance event-driven) |
| **Risque** | High |
| **Source** | Event-driven Polymarket trading, momentum strategies sur marches de prediction |
| **Ref. code** | `crates/engine/src/web_strategies.rs` lignes 517-590 |

### Description

Detecte les **spikes de prix** (catalyseurs), entre en position momentum, et utilise un **trailing stop** pour proteger les gains. Capture les mouvements rapides apres des evenements ou nouvelles.

### Rationnel

Les marches de prediction reagissent fortement aux nouvelles. Un spike de prix/volume signale un catalyseur (nouvelle information). Le momentum tend a persister a court terme car tous les participants n'ont pas encore ajuste leurs positions.

### Parametres

| Parametre | Description | Default | Aggressive | Conservative | Range aleatoire |
|-----------|-------------|---------|------------|--------------|-----------------|
| `spike_threshold` | % au-dessus de la SMA pour detecter un spike | 0.02 (2%) | 0.01 (1%) | 0.04 (4%) | [0.005, 0.06] |
| `trailing_stop_pct` | % de drawdown depuis le plus haut pour trigger le stop | 0.015 (1.5%) | 0.01 (1%) | 0.025 (2.5%) | [0.005, 0.04] |
| `lookback` | Periode de la SMA (reference de prix moyen) | 20 | 10 | 30 | [5, 40] |

### Algorithme detaille

```
ETAT INTERNE :
    sma: SMA(lookback)
    in_position: bool = false         # Actuellement en position
    highest_since_entry: float = 0.0  # Plus haut prix depuis l'entree
    bars_seen: int = 0

WARMUP :
    Retourner Hold() tant que bars_seen < lookback + 2

A CHAQUE BARRE (kline) :
    close = kline.close
    sma_val = SMA.next(close)
    bars_seen += 1

    SI EN POSITION (in_position == true) :
        # Mettre a jour le plus haut
        Si close > highest_since_entry :
            highest_since_entry = close

        # Verifier le trailing stop
        drawdown = (highest_since_entry - close) / highest_since_entry

        Si drawdown > trailing_stop_pct :
            in_position = false
            highest_since_entry = 0
            → SELL (confidence = drawdown * 10.0)
        Sinon :
            → HOLD

    SI PAS EN POSITION (in_position == false) :
        # Detecter un spike
        spike_level = sma_val * (1 + spike_threshold)

        Si close > spike_level ET sma_val > 0 :
            in_position = true
            highest_since_entry = close
            spike_strength = (close - sma_val) / sma_val
            → BUY (confidence = spike_strength * 5.0)
        Sinon :
            → HOLD
```

### Signification sur Polymarket

- **BUY** = un catalyseur est detecte, le prix monte rapidement → acheter YES pour profiter du momentum
- **SELL** = le trailing stop est touche, le momentum s'essouffle → sortir de position
- La strategie est unidirectionnelle (long only), elle ne shorte pas

---

## Strategie 3 : FavoriteCompounder

### Metadata

| Champ | Valeur |
|-------|--------|
| **Nom** | `Web:FavoriteCompounder` |
| **Categorie** | Value (exploitation de biais cognitif) |
| **Risque** | Low |
| **Source** | Favorite-longshot bias, marches de prediction academiques |
| **Ref. code** | `crates/engine/src/web_strategies.rs` lignes 596-678 |

### Description

Trade uniquement les **favoris** (evenements dont la probabilite implicite est elevee), en accumulant de petits gains frequents. Basee sur le **biais favori-longshot** observe en finance comportementale.

### Rationnel

Le biais favori-longshot montre que les evenements improbables (longshots) sont systematiquement surestimes et les favoris sous-estimes dans les marches de prediction. Trader systematiquement les favoris exploite ce biais cognitif. Les gains par trade sont petits mais le taux de reussite est eleve.

### Parametres

| Parametre | Description | Default | Aggressive | Conservative | Range aleatoire |
|-----------|-------------|---------|------------|--------------|-----------------|
| `min_probability` | Probabilite minimum pour considerer un favori | 0.65 | 0.55 | 0.75 | [0.50, 0.85] |
| `take_profit` | % de gain pour prendre les benefices | 0.03 (3%) | 0.02 (2%) | 0.05 (5%) | [0.01, 0.08] |
| `sma_period` | Periode SMA pour confirmer l'uptrend | 20 | 10 | 40 | [8, 50] |

### Algorithme detaille

```
ETAT INTERNE :
    sma: SMA(sma_period)
    baseline_price: float = 0.0     # Premier close vu (pour calcul proba)
    entry_price: float = 0.0        # Prix d'entree en position
    in_position: bool = false
    bars_seen: int = 0

WARMUP :
    Retourner Hold() tant que bars_seen < sma_period + 2

A CHAQUE BARRE (kline) :
    close = kline.close
    sma_val = SMA.next(close)
    bars_seen += 1

    Si baseline_price == 0 : baseline_price = close

    # Estimation de la probabilite implicite
    prob = estimate_poly_probability(baseline_price, close)

    SI EN POSITION (in_position == true) :
        # Verifier le take profit
        Si entry_price > 0 :
            gain = (close - entry_price) / entry_price
            Si gain >= take_profit :
                in_position = false
                entry_price = 0
                → SELL (confidence = gain * 5.0)
        → HOLD

    SI PAS EN POSITION (in_position == false) :
        # Entrer uniquement sur les favoris en uptrend
        Si prob >= min_probability ET close > sma_val :
            in_position = true
            entry_price = close
            → BUY (confidence = (prob - min_probability) * 3.0)
        Sinon :
            → HOLD
```

### Signification sur Polymarket

- **BUY** = le marche indique un favori (haute probabilite) et le prix est en uptrend → acheter YES
- **SELL** = le take profit est atteint, on prend les benefices
- La strategie vise les petits gains repetitifs sur des outcomes quasi-certains

---

## Strategie 4 : MarketMakingSim

### Metadata

| Champ | Valeur |
|-------|--------|
| **Nom** | `Web:MarketMakingSim` |
| **Categorie** | Market-making (capture du spread) |
| **Risque** | Medium |
| **Source** | Polymarket market making, strategies de market maker sur orderbooks |
| **Ref. code** | `crates/engine/src/web_strategies.rs` lignes 684-746 |

### Description

Simule un **market maker** en placant des niveaux bid et ask autour de la SMA (fair value estimee). Achete quand le prix descend en dessous du bid, vend quand il monte au-dessus de l'ask. Gere un inventaire avec une limite.

### Rationnel

Le market making est la strategie la plus utilisee sur Polymarket. En capturant le bid-ask spread autour d'une fair value estimee, on profite de la volatilite laterale (le prix oscille autour d'une moyenne). La strategie est profitable tant que le prix ne trend pas trop dans une direction.

### Parametres

| Parametre | Description | Default | Aggressive | Conservative | Range aleatoire |
|-----------|-------------|---------|------------|--------------|-----------------|
| `spread` | Taille du spread total autour de la SMA (%) | 0.02 (2%) | 0.01 (1%) | 0.04 (4%) | [0.005, 0.06] |
| `sma_period` | Periode SMA pour estimer la fair value | 20 | 10 | 40 | [8, 50] |
| `inventory_limit` | Limite max d'inventaire (positif ou negatif) | 3.0 | 5.0 | 2.0 | [1.0, 8.0] |

### Algorithme detaille

```
ETAT INTERNE :
    sma: SMA(sma_period)
    inventory: float = 0.0      # Inventaire courant (positif = long, negatif = short)
    bars_seen: int = 0

WARMUP :
    Retourner Hold() tant que bars_seen < sma_period + 2 OU sma_val <= 0

A CHAQUE BARRE (kline) :
    close = kline.close
    mid = SMA.next(close)       # Fair value estimee = SMA
    bars_seen += 1

    # Calculer les niveaux bid et ask
    half_spread = spread / 2.0
    bid = mid * (1 - half_spread)   # Prix en dessous duquel on achete
    ask = mid * (1 + half_spread)   # Prix au-dessus duquel on vend

    SI close < bid ET inventory < inventory_limit :
        # Prix sous le bid → acheter
        inventory += 1.0
        depth = (bid - close) / mid
        → BUY (confidence = depth * 10.0)

    SI close > ask ET inventory > -inventory_limit :
        # Prix au-dessus de l'ask → vendre
        inventory -= 1.0
        depth = (close - ask) / mid
        → SELL (confidence = depth * 10.0)

    SINON :
        → HOLD
```

### Signification sur Polymarket

- **BUY** = le prix est en dessous de notre bid (en dessous de la fair value - spread/2) → acheter YES a bon prix
- **SELL** = le prix est au-dessus de notre ask (au-dessus de la fair value + spread/2) → vendre / acheter NO
- L'inventaire empeche de s'exposer trop dans une direction
- La profit vient de la difference repetee entre les prix d'achat (bid) et de vente (ask)

---

## Strategie 5 : MeanReversionPoly

### Metadata

| Champ | Valeur |
|-------|--------|
| **Nom** | `Web:MeanReversionPoly` |
| **Categorie** | Mean-reversion (retour a la moyenne) |
| **Risque** | Medium |
| **Source** | Mean reversion on prediction markets, surreaction a court terme |
| **Ref. code** | `crates/engine/src/web_strategies.rs` lignes 752-828 |

### Description

Calcule une **fair value** via une SMA longue, puis trade les **deviations extremes** en pariant sur un retour a la moyenne. Entre long quand le prix est trop bas, short quand trop haut, et sort quand le prix revient vers la fair value.

### Rationnel

Les marches de prediction surreagissent aux nouvelles a court terme. Les prix extremes (trop haut ou trop bas par rapport a la moyenne historique) tendent a revenir vers leur moyenne. C'est un effet bien documente en finance comportementale : les participants paniquent ou s'emballent, puis le marche corrige.

### Parametres

| Parametre | Description | Default | Aggressive | Conservative | Range aleatoire |
|-----------|-------------|---------|------------|--------------|-----------------|
| `sma_period` | Periode SMA longue pour la fair value | 50 | 20 | 100 | [15, 120] |
| `entry_dev` | Deviation minimum pour entrer en position (%) | 0.03 (3%) | 0.02 (2%) | 0.05 (5%) | [0.01, 0.08] |
| `exit_dev` | Deviation maximum pour sortir de position (%) | 0.01 (1%) | 0.005 (0.5%) | 0.02 (2%) | [0.003, 0.03] |

### Algorithme detaille

```
ETAT INTERNE :
    sma: SMA(sma_period)
    in_long: bool = false       # En position longue (a achete)
    in_short: bool = false      # En position short (a vendu)
    bars_seen: int = 0

WARMUP :
    Retourner Hold() tant que bars_seen < sma_period + 2 OU fair_value <= 0

A CHAQUE BARRE (kline) :
    close = kline.close
    fair_value = SMA.next(close)
    bars_seen += 1

    # Calculer la deviation par rapport a la fair value
    deviation = (close - fair_value) / fair_value
    # deviation > 0 → prix au-dessus de la fair value
    # deviation < 0 → prix en-dessous de la fair value

    # ===== CONDITIONS DE SORTIE (evaluees en premier) =====

    Si in_long ET deviation >= -exit_dev :
        # En long, le prix est revenu vers la fair value (ou au-dessus) → sortir
        in_long = false
        → SELL (confidence = 0.5)

    Si in_short ET deviation <= exit_dev :
        # En short, le prix est revenu vers la fair value (ou en dessous) → sortir
        in_short = false
        → BUY (confidence = 0.5)

    # ===== CONDITIONS D'ENTREE (uniquement si pas en position) =====

    Si PAS in_long ET PAS in_short :

        Si deviation < -entry_dev :
            # Prix trop bas par rapport a la fair value → acheter (mean reversion up)
            in_long = true
            → BUY (confidence = |deviation| * 5.0)

        Si deviation > +entry_dev :
            # Prix trop haut par rapport a la fair value → vendre (mean reversion down)
            in_short = true
            → SELL (confidence = |deviation| * 5.0)

    SINON :
        → HOLD
```

### Signification sur Polymarket

- **BUY (entree long)** = le marche sous-evalue un evenement (prix trop bas) → acheter YES en anticipant une correction vers la fair value
- **SELL (entree short)** = le marche surevalue un evenement (prix trop haut) → acheter NO en anticipant une correction
- **BUY (sortie short)** / **SELL (sortie long)** = la correction a eu lieu, on prend les benefices
- `entry_dev` controle l'agressivite : plus petit = plus de trades mais plus risques
- `exit_dev` controle la patience : plus petit = on sort plus tot, gains plus petits mais plus frequents

---

## Annexe : Implementation de reference

### Indicateurs techniques necessaires

| Indicateur | Utilise par | Description |
|------------|-------------|-------------|
| **RSI** (Relative Strength Index) | ProbabilityEdge | Oscillateur [0, 100], >70 = surachat, <30 = survente |
| **SMA** (Simple Moving Average) | Toutes les 5 | Moyenne glissante simple sur N periodes |

### Bibliotheques recommandees

| Langage | Bibliotheque | Lien |
|---------|-------------|------|
| Python | `ta-lib`, `pandas-ta` | https://github.com/TA-Lib/ta-lib-python |
| Rust | `ta` (crate) | https://crates.io/crates/ta |
| JavaScript | `technicalindicators` | https://www.npmjs.com/package/technicalindicators |

### Integration dans un systeme de backtesting

```python
# Pseudo-code generique pour integrer une strategie

klines = fetch_binance_klines("BTCUSDT", "15m", days=365)
strategy = MeanReversionPolyGenerator(sma_period=50, entry_dev=0.03, exit_dev=0.01)

capital = 10000
position = None

for kline in klines:
    signal = strategy.on_bar(kline)

    if signal.type == "Buy" and position is None:
        # Ouvrir une position long
        size = capital * 0.10 * signal.confidence  # 10% du capital, pondere par confidence
        fee = calculate_polymarket_fee(size, probability=0.50)
        position = {"entry": kline.close, "size": size}
        capital -= fee

    elif signal.type == "Sell" and position is not None:
        # Fermer la position
        pnl = (kline.close - position["entry"]) / position["entry"] * position["size"]
        fee = calculate_polymarket_fee(position["size"], probability=0.50)
        capital += pnl - fee
        position = None
```

### Variantes de parametres et optimisation

Chaque strategie supporte 3 jeux de parametres pre-configures + generation aleatoire :

| Variante | Description | Usage |
|----------|-------------|-------|
| **Default** | Valeurs standard (textbook) | Baseline de comparaison |
| **Aggressive** | Periodes courtes, seuils serres | Plus de trades, plus de risque |
| **Conservative** | Periodes longues, seuils larges | Moins de trades, plus selectif |
| **Random** | Parametres dans les ranges valides | Exploration par le moteur de discovery |

Le moteur de discovery explore les 3 variantes en cycle 0, puis utilise un **algorithme evolutionnaire** (mutation +-15%, crossover, random) a partir du cycle 3 pour optimiser les parametres.
