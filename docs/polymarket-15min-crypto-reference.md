# Polymarket — Marchés Crypto 15 Minutes : Référence Complète

> Document de référence pour l'optimisation de stratégies de trading automatisé sur les marchés Crypto 15 minutes de Polymarket. À fournir comme contexte à Claude Code pour le développement du bot `poly_bot`.

---

## 1. Vue d'ensemble de la plateforme

### 1.1 Qu'est-ce que Polymarket ?

Polymarket est un marché de prédiction décentralisé où les utilisateurs tradent des **shares** (parts) liées à la probabilité d'événements futurs. Les résultats sont réglés on-chain sur **Polygon** en **USDC**. La plateforme utilise un **CLOB** (Central Limit Order Book) hybride — les ordres sont créés et matchés off-chain, puis exécutés on-chain via smart contracts.

### 1.2 Marchés Crypto 15 Minutes

Les marchés Crypto 15 minutes sont des paris binaires à haute fréquence : **"[Crypto] Up or Down"** dans les 15 prochaines minutes.

**Cryptos disponibles :** BTC, ETH, SOL, XRP

**Fonctionnement :**
- Un nouveau marché s'ouvre toutes les 15 minutes
- Le marché précédent se résout simultanément
- Le prix de référence est fourni par **Chainlink** (oracle décentralisé)
- Les shares gagnantes paient **$1.00**, les perdantes **$0.00**

**Caractéristiques distinctives :**
- Seuls marchés Polymarket avec des **taker fees** (tous les autres marchés sont fee-free)
- Haute activité, volume important, résolution rapide
- Ciblés par des stratégies de trading automatisé (bots)

---

## 2. Glossaire des termes clés

### 2.1 Share (Part)

Unité de base échangée sur Polymarket. Chaque marché binaire possède deux types de shares :
- **UP share** : vaut $1.00 si le prix monte, $0.00 sinon
- **DOWN share** : vaut $1.00 si le prix descend, $0.00 sinon

**Règle fondamentale :** `prix_UP + prix_DOWN ≈ $1.00`

### 2.2 Price / Odds (Prix / Cotes)

Le prix d'une share reflète la **probabilité perçue par le marché** (crowd positioning), PAS une prédiction objective.

- Si UP = $0.35, le marché estime ~35% de chance que le prix monte
- Coût d'achat = prix de la share
- Profit potentiel si gain = $1.00 - prix d'achat

**Attention :** Les odds ne sont pas des prédictions fiables. Quand les odds paraissent "évidentes" (ex: 70/30), le mouvement est souvent déjà pricé. Sur les marchés 15 min, les foules sont émotionnelles, impatientes et souvent en retard.

### 2.3 Order Book (Carnet d'ordres)

Polymarket utilise un **CLOB (Central Limit Order Book)**, pas un AMM.

**Structure :**
- **Bids** (vert) : ordres d'achat — prix maximum qu'un acheteur est prêt à payer
- **Asks** (rouge) : ordres de vente — prix minimum qu'un vendeur accepte
- **Last** : prix de la dernière transaction exécutée

**Affichage du prix du marché :**
- Si le spread est ≤ 10 cents → le prix affiché = midpoint du bid/ask
- Si le spread est > 10 cents → le prix affiché = prix du dernier trade

**Unified Order Book :** Bien que l'interface montre des onglets séparés pour UP et DOWN, les ordres d'un côté apparaissent comme inverses dans l'autre book. Un ordre d'achat UP à $0.40 = un ordre de vente DOWN à $0.60.

### 2.4 Spread

Écart entre le meilleur bid et le meilleur ask.

```
spread = meilleur_ask - meilleur_bid
```

- **Spread serré** (ex: $0.01) = marché liquide, facile à trader
- **Spread large** (ex: $0.05+) = marché illiquide, attention au slippage
- Sur les marchés 15 min, le spread moyen est passé de ~4.5% (2023) à ~1.2% (2025) grâce au Maker Rebates Program

### 2.5 Depth (Profondeur)

Volume cumulé en USDC disponible à chaque niveau de prix dans l'order book. Indique combien on peut trader sans déplacer significativement le prix. Visible dans la colonne "Total" du book.

### 2.6 Slippage

Différence entre le prix attendu et le prix réel d'exécution. Se produit quand un ordre est trop gros par rapport à la liquidité disponible au meilleur prix. L'ordre "walk the book" — il remonte les niveaux de prix pour se remplir.

### 2.7 Market Order

Ordre exécuté **immédiatement** au meilleur prix disponible.
- **Avantage :** exécution garantie (si liquidité existe)
- **Inconvénient :** on paie le spread + potentiel slippage + **taker fees** sur les marchés 15 min

### 2.8 Limit Order

Ordre placé à un **prix spécifique** défini par le trader. Reste dans le book jusqu'à ce qu'un match arrive ou expiration.
- **Avantage :** meilleur contrôle du prix, pas de taker fee si l'ordre reste dans le book (maker)
- **Inconvénient :** pas de garantie d'exécution — critique sur les marchés 15 min où le temps est compté

### 2.9 Maker vs Taker

| Rôle | Description | Fees sur marchés 15 min |
|------|-------------|------------------------|
| **Maker** | Place un limit order qui reste dans le book → ajoute de la liquidité | **Aucun fee** + éligible aux **rebates** |
| **Taker** | Exécute un market order ou un limit order qui matche immédiatement → retire de la liquidité | **Paie les taker fees** |

### 2.10 Resolution

Moment où le marché se ferme et les résultats sont déterminés.
- **Marchés 15 min :** résolution automatique via l'oracle **Chainlink** (prix spot vérifié)
- Shares gagnantes → paiement de **$1.00** par share
- Shares perdantes → **$0.00**
- Le trading cesse immédiatement à la résolution

### 2.11 Minting

Création de nouvelles shares. Quand un ordre d'achat UP à $X et un ordre d'achat DOWN à $(1-X) se rencontrent, le système **mint** (crée) des shares UP et DOWN car leur somme = $1.00. Pas d'échange entre utilisateurs — nouvelles shares créées.

### 2.12 Merging

Destruction de shares complémentaires. Si un trader possède 1 share UP + 1 share DOWN, il peut les merger pour récupérer $1.00 immédiatement, sans attendre la résolution.

---

## 3. Système de fees (marchés 15 min uniquement)

### 3.1 Structure des taker fees

Les taker fees sont **dynamiques** et calculés selon la formule :

```
fee = C × feeRate × (p × (1 - p))^exponent
```

**Paramètres actuels :**
- `C` = nombre de shares tradées
- `feeRate` = 0.25
- `exponent` = 2
- `p` = prix de la share (entre 0 et 1)

### 3.2 Table de référence des fees

| Prix (1 share) | Fee (1 share) | Prix (100 shares) | Fee (100 shares) |
|----------------|---------------|--------------------|--------------------|
| $0.01 | $0.0000 | $1.00 | $0.0025 |
| $0.05 | $0.0006 | $5.00 | $0.0564 |
| $0.10 | $0.0020 | $10.00 | $0.2025 |
| $0.20 | $0.0064 | $20.00 | $0.6400 |
| $0.30 | $0.0110 | $30.00 | $1.1025 |
| $0.40 | $0.0144 | $40.00 | $1.4400 |
| $0.50 | $0.0156 | $50.00 | $1.5625 |
| $0.60 | $0.0144 | $60.00 | $1.4400 |
| $0.70 | $0.0110 | $70.00 | $1.1025 |
| $0.80 | $0.0064 | $80.00 | $0.6400 |
| $0.90 | $0.0020 | $90.00 | $0.2025 |
| $0.99 | $0.0000 | $99.00 | $0.0025 |

### 3.3 Propriétés clés de la courbe de fees

- **Fee maximum à 50/50** : ~$0.0156/share soit ~3.12% → conçu pour tuer l'arbitrage de latence
- **Fee minimum aux extrêmes** : quasi nul à <5% ou >95%
- **Symétrique** : fee identique à $0.30 et $0.70
- **Précision** : 4 décimales, minimum chargé = $0.0001 USDC. En dessous → arrondi à zéro

### 3.4 Calcul du fee pour un trade d'arbitrage (deux legs)

Pour un arbitrage gabagool (achat UP + DOWN) :

```
fee_total = fee(C, p_up) + fee(C, p_down)
```

Où `p_up + p_down ≈ 1.0`, donc si `p_up = 0.50` et `p_down = 0.50` :
```
fee_total = 2 × C × 0.25 × (0.50 × 0.50)^2
          = 2 × C × 0.25 × 0.0625
          = C × 0.03125
```
→ **~3.125% du montant total** pour 100 shares à 50/50

**Seuil de rentabilité pour l'arbitrage :**
```
profit = (1.00 - prix_up_ask - prix_down_ask) × C - fee(C, p_up) - fee(C, p_down)
profit > 0 requis pour trade rentable
```

### 3.5 Implémentation en code (pseudo-code)

```rust
fn calculate_taker_fee(shares: f64, price: f64) -> f64 {
    let fee_rate: f64 = 0.25;
    let exponent: f64 = 2.0;
    let raw_fee = shares * fee_rate * (price * (1.0 - price)).powf(exponent);
    // Arrondir à 4 décimales, minimum $0.0001
    let rounded = (raw_fee * 10000.0).floor() / 10000.0;
    if rounded < 0.0001 { 0.0 } else { rounded }
}

fn is_arbitrage_profitable(
    up_ask: f64,
    down_ask: f64,
    shares: f64,
) -> (bool, f64) {
    let total_cost = (up_ask + down_ask) * shares;
    let payout = 1.0 * shares;
    let fee_up = calculate_taker_fee(shares, up_ask);
    let fee_down = calculate_taker_fee(shares, down_ask);
    let profit = payout - total_cost - fee_up - fee_down;
    (profit > 0.0, profit)
}
```

---

## 4. Maker Rebates Program

### 4.1 Principe

100% des taker fees collectés sur les marchés 15 min sont redistribués quotidiennement en USDC aux market makers. Polymarket ne garde rien.

### 4.2 Fonctionnement détaillé

1. **Collecte :** chaque taker trade génère un fee → accumulé dans un pool quotidien
2. **Calcul :** à minuit UTC, les rebates de chaque maker sont calculés
3. **Distribution :** payés quotidiennement en USDC, directement dans le wallet du maker
4. **Base de calcul :** proportionnel à la **part de liquidité exécutée** (fillée) du maker

### 4.3 Critères d'éligibilité

- Placer des **limit orders** qui restent dans l'order book (= maker)
- Ces ordres doivent être **effectivement exécutés** (fillés par des takers)
- Simple fait de poser des ordres non fillés **ne génère pas** de rebates

### 4.4 Formule de rebate

Les rebates utilisent la **même formule** que les taker fees :

```
rebate_maker_i = (maker_i_filled_volume_weighted / total_filled_volume_weighted) × total_fee_pool
```

Où le "volume weighted" applique le même poids que la courbe de fees : fournir de la liquidité près de 50% rapporte plus de rebates (car c'est là que les fees sont les plus élevés).

### 4.5 Impact mesuré

- Spreads bid-ask : **4.5% → 1.2%** (2023 → 2025)
- Profondeur d'order book moyenne : **~$2.1M** en Q3 2025
- Budget annuel de rebates : **~$12M** en 2025
- Réduction du wash trading : de **25% à 5%** du volume

### 4.6 Implications stratégiques pour le bot

**Stratégie maker bot potentielle :**
- Placer des limit orders des deux côtés (UP et DOWN) avec un spread contrôlé
- Les ordres se font filler par les takers
- Revenus = spread naturel de market making + rebates quotidiens USDC
- Risque principal = exposition directionnelle si un seul côté se fill avant résolution

---

## 5. Types de matching sur Polymarket

### 5.1 Direct Matching (Swap)

Échange direct entre deux utilisateurs. Un vendeur de shares UP matche avec un acheteur de shares UP. Pas de création ni destruction de shares.

### 5.2 Minting (Création)

Quand un acheteur UP à $X et un acheteur DOWN à $(1-X) se rencontrent :
- Le système crée (mint) de nouvelles shares UP et DOWN
- L'acheteur UP reçoit ses shares UP, l'acheteur DOWN reçoit ses shares DOWN
- Possible car la règle `1 UP + 1 DOWN = $1.00` est respectée

### 5.3 Merging (Destruction)

Si un trader possède des shares UP et DOWN complémentaires :
- Le système les fusionne et retourne $1.00 par paire
- Permet de sortir d'une position sans attendre la résolution

---

## 6. Architecture technique

### 6.1 Stack

- **Blockchain :** Polygon (PoS)
- **Stablecoin :** USDC
- **Oracle :** Chainlink (prix spot pour résolution des marchés crypto)
- **Order Book :** CLOB hybride (off-chain matching, on-chain settlement)
- **Smart Contracts :** CTF (Conditional Token Framework) + UMA Optimistic Oracle (disputes)
- **API :** REST (Gamma API pour metadata) + WebSocket (order book en temps réel)

### 6.2 APIs disponibles

| API | Usage |
|-----|-------|
| **CLOB API** | Trading : placement d'ordres, order book, historique des trades |
| **Gamma API** | Metadata : informations marché, catégories, volumes indexés, résolution |
| **Data API** | Données historiques, prix, volumes |
| **WebSocket** | Flux temps réel : order book updates, trades, prix |

### 6.3 Tick size

Le tick size minimum change dynamiquement :
- **Normal :** tick standard
- **Extrêmes :** quand `price > 0.96` ou `price < 0.04`, le tick size change

---

## 7. Stratégies de trading applicables aux marchés 15 min

### 7.1 Arbitrage pur (Gabagool v1)

**Principe :** Acheter UP + DOWN quand `coût_total < $1.00` → profit garanti à la résolution.

**Conditions de rentabilité post-fees :**
```
profit = (1.00 × C) - (up_ask × C) - (down_ask × C) - fee(C, up_ask) - fee(C, down_ask)
profit > 0
```

**Zones optimales :**
- Aux **extrêmes de probabilité** (10/90, 20/80) où les fees sont quasi nuls
- Les opportunités à 50/50 sont quasi impossibles à cause du fee de ~3.12% par leg

**Exécution :**
- Utiliser l'order book asks (pas le last trade price)
- Vérifier la profondeur (depth) pour s'assurer que le volume souhaité peut se fill
- Exécuter les deux legs le plus rapidement possible (risque de partial fill)

### 7.2 Dump-Hedge (Gabagool v2)

**Principe :** Acheter le côté qui dump fortement, puis hedger quand le coût combiné ≤ target (ex: $0.95).

**Avantage :** ne nécessite pas un spread < $1.00 au même instant, exploite les mouvements de prix intra-period.

**Risque :** exposition directionnelle temporaire avant le hedge.

### 7.3 Pre-placed Limit Orders (Gabagool v3)

**Principe :** Pré-placer des limit orders pour les marchés à venir (ex: les deux côtés à $0.45).

**Avantage :** maker orders (pas de taker fees), positionnement avant l'ouverture.

**Risque :** les ordres peuvent ne pas se fill, ou un seul côté se fill.

### 7.4 Market Making (Maker Bot)

**Principe :** Poster continuellement des limit orders bid/ask des deux côtés avec un spread contrôlé.

**Revenus :**
- Spread capturé entre bid et ask
- Rebates quotidiens du Maker Rebates Program

**Risques :**
- Inventory risk (accumulation d'un côté)
- Adverse selection (les takers informés tradent contre toi)
- Résolution dans 15 min max → nécessité de gérer les positions avant clôture

### 7.5 Stratégie directionnelle (Multi-indicateurs)

**Principe :** Utiliser des indicateurs techniques/fondamentaux pour prédire la direction du prix et acheter le côté correspondant.

**Facteurs à considérer :**
- Les odds Polymarket = crowd positioning, pas des prédictions
- Quand les odds deviennent extrêmes tôt dans la fenêtre → prudence (souvent déjà pricé)
- Timing = critique. Mauvais timing sur 15 min = fatal
- Il faut que la probabilité estimée dépasse suffisamment le prix + fees pour avoir un edge positif

---

## 8. Calculs essentiels pour le bot

### 8.1 Expected Value (EV) d'un trade directionnel

```
EV = (probabilité_estimée × gain_potentiel) - ((1 - probabilité_estimée) × perte) - fee

Où :
- gain_potentiel = (1.00 - prix_share) × C
- perte = prix_share × C
- fee = calculate_taker_fee(C, prix_share)
```

Un trade est rentable si `EV > 0`.

### 8.2 Seuil de probabilité minimum pour un trade rentable

Pour acheter une share à prix `p` avec un fee `f` :

```
prob_minimum = (p + f) / 1.00 = p + f
```

Exemple : acheter UP à $0.50 avec fee $0.0156 → il faut estimer >51.56% de chance que UP gagne.

### 8.3 Profit d'arbitrage net

```
profit_net = C × (1.00 - ask_up - ask_down) - fee(C, ask_up) - fee(C, ask_down)
```

### 8.4 ROI par trade

```
ROI = profit_net / (ask_up + ask_down) × C × 100
```

---

## 9. Risques et contraintes

### 9.1 Risques de timing
- Fenêtre de 15 min = exécution ultra-rapide requise
- Partial fills possibles (un leg se fill, pas l'autre)
- Les marchés ferment à heure fixe — ne pas accumuler de positions ouvertes

### 9.2 Risques de liquidité
- Profondeur insuffisante peut causer du slippage
- En période de volatilité, la liquidité peut s'évaporer temporairement
- Toujours vérifier la depth avant d'exécuter

### 9.3 Risques techniques
- Latence réseau (off-chain → on-chain)
- Défaillance de l'oracle Chainlink (rare mais possible)
- Rate limiting des APIs

### 9.4 Risque de fee change
- Les paramètres de fee (feeRate, exponent) sont à la discrétion de Polymarket et peuvent changer sans préavis
- Le bot doit pouvoir s'adapter dynamiquement ou vérifier les paramètres régulièrement

---

## 10. Paramètres de configuration recommandés pour le bot

```toml
[polymarket.fees]
fee_rate = 0.25
exponent = 2.0
min_fee = 0.0001  # USDC, 4 décimales

[polymarket.arbitrage]
# Seuil minimum de profit après fees pour exécuter un arbitrage
min_profit_per_share = 0.005  # $0.005 minimum
# Taille max d'ordre (limiter le risque de partial fill)
max_order_size = 500  # shares
# Vérifier la depth minimum avant exécution
min_depth_required = 1000  # USDC de chaque côté

[polymarket.directional]
# Edge minimum requis au-dessus du prix + fees
min_edge = 0.05  # 5% d'edge minimum
# Kelly fraction pour le sizing
kelly_fraction = 0.25  # quart-Kelly pour être conservateur

[polymarket.market_making]
# Spread minimum à poster
min_spread = 0.02  # $0.02 de chaque côté du mid
# Taille des ordres
order_size = 100  # shares par niveau
# Nombre de niveaux de chaque côté
levels = 3
# Temps max avant refresh des ordres
refresh_interval_seconds = 10
```

---

## 11. Résumé des points critiques

1. **Les odds ≠ des prédictions.** Ce sont des positions de foule, souvent en retard et émotionnelles.
2. **Les fees changent tout pour l'arbitrage.** À 50/50, le fee combiné ~6% rend le gabagool quasi impossible. Cibler les extrêmes.
3. **Maker > Taker.** Post-fees, les stratégies maker (limit orders + rebates) sont favorisées par la plateforme.
4. **Timing = survie.** Sur 15 min, un mauvais timing est immédiatement puni. Pas de marge de manœuvre.
5. **Toujours intégrer les fees dans le calcul de rentabilité.** Jamais de trade sans calcul du profit net post-fees.
6. **Vérifier la depth avant chaque trade.** Le prix affiché ne garantit pas l'exécution à ce prix.
7. **Les paramètres de fees peuvent changer.** Monitorer la documentation Polymarket et adapter le bot.
8. **Le Maker Rebates Program est un revenue stream.** À considérer sérieusement comme stratégie complémentaire.
