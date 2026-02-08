<script>
  import { onDestroy } from 'svelte';
  import { getTopStrategies } from '../lib/api.js';
  import { discoveryStatus } from '../lib/stores.js';
  import { BookOpen, Loader2, Copy, Check } from 'lucide-svelte';

  let strategies = $state([]);
  let loading = $state(false);
  let autoRefreshInterval = $state(null);
  let copiedIdx = $state(-1);

  async function loadStrategies() {
    loading = true;
    const res = await getTopStrategies(3, 'win_rate');
    if (res.success !== false) {
      strategies = res.data || [];
    }
    loading = false;
  }

  const unsubscribe = discoveryStatus.subscribe(status => {
    if (status.running && !autoRefreshInterval) {
      autoRefreshInterval = setInterval(loadStrategies, 60000);
    } else if (!status.running && autoRefreshInterval) {
      clearInterval(autoRefreshInterval);
      autoRefreshInterval = null;
      loadStrategies();
    }
  });

  onDestroy(() => {
    unsubscribe();
    if (autoRefreshInterval) clearInterval(autoRefreshInterval);
  });

  async function copyDescription(text, idx) {
    await navigator.clipboard.writeText(text);
    copiedIdx = idx;
    setTimeout(() => { copiedIdx = -1; }, 2000);
  }

  function parseParams(jsonStr) {
    try {
      const obj = JSON.parse(jsonStr);
      const keys = Object.keys(obj);
      if (keys.length === 1 && typeof obj[keys[0]] === 'object') {
        return obj[keys[0]];
      }
      return obj;
    } catch {
      return {};
    }
  }

  // ============================================================================
  // Polymarket essential parameters table
  // ============================================================================
  function getPolymarketParams(row) {
    const p = parseParams(row.strategy_params);
    const type = row.strategy_type;
    const sym = row.symbol.replace('USDT', '');

    const common = [
      { param: 'Marché', value: `"${sym} up in next 15 minutes?"`, desc: 'Marché binaire Polymarket cible' },
      { param: 'Timeframe', value: '15 minutes', desc: 'Intervalle des bougies Binance (klines 15m)' },
      { param: 'Source de données', value: `Binance ${row.symbol} klines`, desc: 'Feed de prix temps réel à connecter au bot' },
      { param: 'Sizing', value: row.sizing_mode, desc: row.sizing_mode === 'fixed' ? 'Taille fixe $10 par trade' : row.sizing_mode === 'kelly' ? 'Kelly criterion: taille proportionnelle à l\'edge' : 'Pondéré par confiance du signal (0.3-1.0)' },
    ];

    const indicators = getIndicatorParams(type, p);
    const signals = getSignalParams(type, p, sym);

    return [...common, ...indicators, ...signals];
  }

  function getIndicatorParams(type, p) {
    const m = {
      rsi: () => [
        { param: 'Indicateur', value: 'RSI', desc: 'Relative Strength Index — oscillateur de momentum' },
        { param: 'RSI période', value: p.period, desc: `Calculer le RSI sur les ${p.period} dernières bougies de 15min` },
        { param: 'Seuil survente', value: p.oversold, desc: `Signal ACHAT quand RSI < ${p.oversold}` },
        { param: 'Seuil surachat', value: p.overbought, desc: `Signal VENTE quand RSI > ${p.overbought}` },
      ],
      bollinger_bands: () => [
        { param: 'Indicateur', value: 'Bollinger Bands', desc: 'Bandes de Bollinger — mesure la volatilité autour de la moyenne' },
        { param: 'BB période', value: p.period, desc: `SMA calculée sur ${p.period} bougies` },
        { param: 'BB multiplicateur', value: p.multiplier, desc: `Bandes à ${p.multiplier} écarts-types de la SMA` },
      ],
      macd: () => [
        { param: 'Indicateur', value: 'MACD', desc: 'Moving Average Convergence Divergence — momentum' },
        { param: 'EMA rapide', value: p.fast, desc: `EMA courte sur ${p.fast} bougies` },
        { param: 'EMA lente', value: p.slow, desc: `EMA longue sur ${p.slow} bougies` },
        { param: 'Signal', value: p.signal, desc: `Ligne de signal: EMA du MACD sur ${p.signal} bougies` },
      ],
      ema_crossover: () => [
        { param: 'Indicateur', value: 'EMA Crossover', desc: 'Croisement de moyennes mobiles exponentielles' },
        { param: 'EMA rapide', value: p.fast_period, desc: `EMA courte sur ${p.fast_period} bougies` },
        { param: 'EMA lente', value: p.slow_period, desc: `EMA longue sur ${p.slow_period} bougies` },
      ],
      stochastic: () => [
        { param: 'Indicateur', value: 'Stochastic', desc: 'Oscillateur stochastique — position du prix dans la range' },
        { param: 'Période', value: p.period, desc: `Lookback sur ${p.period} bougies` },
        { param: 'Seuil survente', value: p.oversold, desc: `Zone de survente: %K < ${p.oversold}` },
        { param: 'Seuil surachat', value: p.overbought, desc: `Zone de surachat: %K > ${p.overbought}` },
      ],
      atr_mean_reversion: () => [
        { param: 'Indicateur', value: 'ATR Mean Reversion', desc: 'Retour à la moyenne basé sur l\'Average True Range' },
        { param: 'ATR période', value: p.atr_period, desc: `ATR calculé sur ${p.atr_period} bougies` },
        { param: 'SMA période', value: p.sma_period, desc: `Moyenne mobile sur ${p.sma_period} bougies` },
        { param: 'Multiplicateur', value: p.multiplier, desc: `Seuil de distance: ${p.multiplier} × ATR au-dessus/dessous de la SMA` },
      ],
      vwap: () => [
        { param: 'Indicateur', value: 'VWAP', desc: 'Volume Weighted Average Price — prix moyen pondéré par volume' },
        { param: 'Période', value: p.period, desc: `VWAP calculé sur ${p.period} bougies` },
      ],
      obv: () => [
        { param: 'Indicateur', value: 'OBV', desc: 'On-Balance Volume — flux de volume cumulé' },
        { param: 'SMA période', value: p.sma_period, desc: `Lissage de l'OBV par SMA sur ${p.sma_period} bougies` },
      ],
      williams_r: () => [
        { param: 'Indicateur', value: 'Williams %R', desc: 'Williams Percent Range — oscillateur de momentum (-100 à 0)' },
        { param: 'Période', value: p.period, desc: `Lookback sur ${p.period} bougies` },
        { param: 'Seuil survente', value: p.oversold, desc: `Signal ACHAT quand %R < ${p.oversold}` },
        { param: 'Seuil surachat', value: p.overbought, desc: `Signal VENTE quand %R > ${p.overbought}` },
      ],
      adx: () => [
        { param: 'Indicateur', value: 'ADX', desc: 'Average Directional Index — force de la tendance' },
        { param: 'Période', value: p.period, desc: `ADX calculé sur ${p.period} bougies` },
        { param: 'Seuil force', value: p.adx_threshold, desc: `Ne trader que si ADX > ${p.adx_threshold} (tendance forte)` },
      ],
      rsi_bollinger: () => [
        { param: 'Indicateurs', value: 'RSI + Bollinger Bands', desc: 'Combo: les DEUX doivent être d\'accord (mode Unanime)' },
        { param: 'RSI période', value: p.rsi_period, desc: `RSI sur ${p.rsi_period} bougies` },
        { param: 'RSI survente/surachat', value: `${p.rsi_os} / ${p.rsi_ob}`, desc: 'Seuils RSI bas/haut' },
        { param: 'BB période', value: p.bb_period, desc: `Bollinger SMA sur ${p.bb_period} bougies` },
        { param: 'BB multiplicateur', value: p.bb_mult, desc: `Bandes à ${p.bb_mult} écarts-types` },
      ],
      macd_rsi: () => [
        { param: 'Indicateurs', value: 'MACD (primaire) + RSI (filtre)', desc: 'MACD donne le signal, RSI confirme' },
        { param: 'MACD fast/slow/signal', value: `${p.macd_fast}/${p.macd_slow}/${p.macd_signal}`, desc: 'Paramètres MACD' },
        { param: 'RSI période', value: p.rsi_period, desc: `RSI sur ${p.rsi_period} bougies` },
        { param: 'RSI survente/surachat', value: `${p.rsi_os} / ${p.rsi_ob}`, desc: 'RSI ne doit pas bloquer le signal' },
      ],
      ema_rsi: () => [
        { param: 'Indicateurs', value: 'EMA Cross (primaire) + RSI (filtre)', desc: 'EMA donne le signal, RSI confirme' },
        { param: 'EMA rapide/lente', value: `${p.ema_fast} / ${p.ema_slow}`, desc: 'Paramètres croisement EMA' },
        { param: 'RSI période', value: p.rsi_period, desc: `RSI sur ${p.rsi_period} bougies` },
        { param: 'RSI survente/surachat', value: `${p.rsi_os} / ${p.rsi_ob}`, desc: 'Seuils de filtrage RSI' },
      ],
      stoch_rsi: () => [
        { param: 'Indicateurs', value: 'Stochastic + RSI', desc: 'Double oscillateur — les DEUX doivent confirmer (Unanime)' },
        { param: 'Stoch période', value: p.stoch_period, desc: `Stochastique sur ${p.stoch_period} bougies` },
        { param: 'Stoch survente/surachat', value: `${p.stoch_os} / ${p.stoch_ob}`, desc: 'Seuils Stochastique' },
        { param: 'RSI période', value: p.rsi_period, desc: `RSI sur ${p.rsi_period} bougies` },
        { param: 'RSI survente/surachat', value: `${p.rsi_os} / ${p.rsi_ob}`, desc: 'Seuils RSI' },
      ],
      macd_bollinger: () => [
        { param: 'Indicateurs', value: 'MACD (primaire) + BB (filtre)', desc: 'MACD donne le signal, BB confirme la zone' },
        { param: 'MACD fast/slow/signal', value: `${p.macd_fast}/${p.macd_slow}/${p.macd_signal}`, desc: 'Paramètres MACD' },
        { param: 'BB période/mult', value: `${p.bb_period} / ${p.bb_mult}`, desc: 'Paramètres Bollinger' },
      ],
      triple_rsi_macd_bb: () => [
        { param: 'Indicateurs', value: 'RSI + MACD + BB', desc: 'Triple combo — vote majoritaire (2/3 doivent être d\'accord)' },
        { param: 'RSI', value: `p=${p.rsi_period} OS=${p.rsi_os} OB=${p.rsi_ob}`, desc: 'Paramètres RSI' },
        { param: 'MACD', value: `${p.macd_fast}/${p.macd_slow}/${p.macd_signal}`, desc: 'Paramètres MACD' },
        { param: 'BB', value: `p=${p.bb_period} m=${p.bb_mult}`, desc: 'Paramètres Bollinger' },
      ],
      triple_ema_rsi_stoch: () => [
        { param: 'Indicateurs', value: 'EMA + RSI + Stoch', desc: 'Triple combo — vote majoritaire (2/3)' },
        { param: 'EMA', value: `fast=${p.ema_fast} slow=${p.ema_slow}`, desc: 'Paramètres EMA' },
        { param: 'RSI', value: `p=${p.rsi_period} OS=${p.rsi_os} OB=${p.rsi_ob}`, desc: 'Paramètres RSI' },
        { param: 'Stoch', value: `p=${p.stoch_period} OS=${p.stoch_os} OB=${p.stoch_ob}`, desc: 'Paramètres Stochastique' },
      ],
      vwap_rsi: () => [
        { param: 'Indicateurs', value: 'VWAP (primaire) + RSI (filtre)', desc: 'VWAP donne le signal, RSI confirme' },
        { param: 'VWAP période', value: p.vwap_period, desc: `VWAP sur ${p.vwap_period} bougies` },
        { param: 'RSI', value: `p=${p.rsi_period} OS=${p.rsi_oversold} OB=${p.rsi_overbought}`, desc: 'Paramètres RSI de filtrage' },
      ],
      obv_macd: () => [
        { param: 'Indicateurs', value: 'MACD (primaire) + OBV (volume)', desc: 'MACD donne le signal, OBV confirme par le volume' },
        { param: 'MACD', value: `${p.macd_fast}/${p.macd_slow}/${p.macd_signal}`, desc: 'Paramètres MACD' },
        { param: 'OBV SMA', value: p.obv_sma_period, desc: `Lissage OBV sur ${p.obv_sma_period} bougies` },
      ],
      adx_ema: () => [
        { param: 'Indicateurs', value: 'EMA Cross (primaire) + ADX (filtre)', desc: 'EMA donne le signal, ADX filtre les tendances faibles' },
        { param: 'EMA rapide/lente', value: `${p.ema_fast} / ${p.ema_slow}`, desc: 'Paramètres EMA' },
        { param: 'ADX', value: `p=${p.adx_period} seuil=${p.adx_threshold}`, desc: `Ne trader que si ADX > ${p.adx_threshold}` },
      ],
      williams_r_stoch: () => [
        { param: 'Indicateurs', value: 'Williams %R + Stochastic', desc: 'Double oscillateur — les DEUX doivent confirmer (Unanime)' },
        { param: 'Williams %R', value: `p=${p.wr_period} OS=${p.wr_oversold} OB=${p.wr_overbought}`, desc: 'Paramètres Williams' },
        { param: 'Stoch', value: `p=${p.stoch_period} OS=${p.stoch_oversold} OB=${p.stoch_overbought}`, desc: 'Paramètres Stochastique' },
      ],
      gabagool: () => [
        { param: 'Type', value: 'Arbitrage binaire', desc: 'Non-directionnel — achète les DEUX côtés YES et NO' },
        { param: 'Max pair cost', value: p.max_pair_cost, desc: `Coût max YES+NO pour entrer (profit = 1.00 - coût)` },
        { param: 'Bid offset', value: p.bid_offset, desc: 'Décalage sous le mid-price pour ordres maker' },
        { param: 'Spread multiplier', value: p.spread_multiplier, desc: 'Multiplicateur du spread basé sur la volatilité' },
      ],
    };
    return (m[type] || (() => []))();
  }

  function getSignalParams(type, p, sym) {
    if (type === 'gabagool') {
      return [
        { param: '→ Action', value: 'BUY YES + BUY NO', desc: `Quand YES_fill + NO_fill < ${p.max_pair_cost} → profit garanti` },
        { param: '→ Skip', value: 'Pas de trade', desc: 'Quand pair_cost trop élevé, aucune opportunité d\'arbitrage' },
      ];
    }
    return [
      { param: '→ Signal ACHAT', value: `Buy YES sur "${sym} up"`, desc: 'Acheter token YES sur Polymarket (pari haussier)' },
      { param: '→ Signal VENTE', value: `Buy NO sur "${sym} up"`, desc: 'Acheter token NO sur Polymarket (pari baissier)' },
      { param: '→ Aucun signal', value: 'HOLD — ne rien faire', desc: 'Attendre le prochain signal, ne pas forcer de trade' },
    ];
  }

  // ============================================================================
  // Bot implementation description — ultra detailed
  // ============================================================================
  function generateBotDescription(row) {
    const p = parseParams(row.strategy_params);
    const type = row.strategy_type;
    const sym = row.symbol.replace('USDT', '');
    const wr = parseFloat(row.win_rate);
    const pnl = parseFloat(row.net_pnl);
    const sharpe = parseFloat(row.sharpe_ratio);
    const sortino = parseFloat(row.sortino_ratio || 0);
    const dd = parseFloat(row.max_drawdown_pct);
    const conf = parseFloat(row.strategy_confidence || 0);
    const annRet = parseFloat(row.annualized_return_pct || 0);
    const pf = parseFloat(row.profit_factor);
    const maxLoss = row.max_consecutive_losses || 0;

    let t = '';
    t += `=== STRATÉGIE DE TRADING POLYMARKET ===\n`;
    t += `Nom: ${row.strategy_name}\n`;
    t += `Marché cible: "${sym} up in next 15 minutes?" (marché binaire Polymarket)\n`;
    t += `Paire de référence: ${row.symbol} sur Binance\n`;
    t += `Timeframe: bougies de 15 minutes (klines Binance intervalle "15m")\n`;
    t += `Période de backtest: ${row.days} jours\n\n`;

    t += `=== CONFIGURATION DU BOT ===\n\n`;

    t += `1. SOURCE DE DONNÉES\n`;
    t += `   - Connecter le bot à l'API Binance WebSocket ou REST\n`;
    t += `   - Endpoint: GET /api/v3/klines?symbol=${row.symbol}&interval=15m\n`;
    t += `   - Récupérer les bougies OHLCV (Open, High, Low, Close, Volume)\n`;
    t += `   - Maintenir un buffer des ${getMaxPeriod(type, p)} dernières bougies minimum\n\n`;

    t += `2. CALCUL DES INDICATEURS\n`;
    t += getDetailedIndicatorCalc(type, p);
    t += '\n';

    t += `3. LOGIQUE DE SIGNAUX\n`;
    t += getDetailedSignalLogic(type, p, sym);
    t += '\n';

    t += `4. EXÉCUTION SUR POLYMARKET\n`;
    if (type === 'gabagool') {
      t += `   Sur signal TRADE:\n`;
      t += `     a. Calculer le mid-price YES et NO du marché "${sym} up?"\n`;
      t += `     b. Placer un ordre LIMIT BUY YES à (mid_yes - spread/2 - ${p.bid_offset})\n`;
      t += `     c. Placer un ordre LIMIT BUY NO  à (mid_no  - spread/2 - ${p.bid_offset})\n`;
      t += `     d. Si les deux ordres sont remplis et coût total < ${p.max_pair_cost} → profit = 1.00 - coût\n`;
      t += `     e. Attendre la résolution du marché (15 min) — le profit est garanti quel que soit le résultat\n`;
      t += `   Sur signal SKIP:\n`;
      t += `     - Ne rien faire, attendre la prochaine bougie\n`;
    } else {
      t += `   Sur signal ACHAT (BUY):\n`;
      t += `     a. Ouvrir le marché "${sym} up in next 15 minutes?" sur Polymarket\n`;
      t += `     b. Acheter des tokens YES au prix du marché\n`;
      t += `     c. Taille de position: ${row.sizing_mode === 'fixed' ? '$10 fixe' : row.sizing_mode === 'kelly' ? 'Kelly criterion (edge/odds)' : 'pondérée par confiance du signal'}\n`;
      t += `     d. Attendre la résolution du marché (15 min)\n`;
      t += `   Sur signal VENTE (SELL):\n`;
      t += `     a. Acheter des tokens NO sur le même marché\n`;
      t += `     b. Même taille de position que pour un ACHAT\n`;
      t += `     c. Attendre la résolution du marché\n`;
      t += `   Sur signal HOLD:\n`;
      t += `     - Ne rien faire, attendre la prochaine bougie de 15 min\n`;
    }
    t += '\n';

    t += `5. GESTION DES FRAIS POLYMARKET\n`;
    t += `   - Formule: fee = C × feeRate × (p × (1-p))^exponent\n`;
    t += `   - feeRate = 0.25, exponent = 2 (défaut Polymarket)\n`;
    t += `   - Les frais sont maximaux quand p ≈ 0.50 et diminuent vers les extrêmes\n`;
    t += `   - Estimer p dynamiquement via le changement de prix vs baseline\n`;
    t += `   - Intégrer les frais dans le calcul de rentabilité avant chaque trade\n\n`;

    t += `6. GESTION DU RISQUE\n`;
    t += `   - Max drawdown observé en backtest: ${dd.toFixed(1)}%\n`;
    t += `   - Max pertes consécutives observées: ${maxLoss}\n`;
    t += `   - Profit factor: ${pf.toFixed(2)} (ratio gains/pertes)\n`;
    t += `   - Stopper le bot si drawdown dépasse ${(dd * 1.5).toFixed(0)}% (1.5× le max historique)\n`;
    t += `   - Stopper si ${Math.max(maxLoss + 3, 10)} pertes consécutives (marge au-dessus du backtest)\n\n`;

    t += `7. BOUCLE PRINCIPALE DU BOT\n`;
    t += `   while (running) {\n`;
    t += `     1. Attendre la clôture de la bougie 15min courante\n`;
    t += `     2. Récupérer la nouvelle bougie OHLCV\n`;
    t += `     3. Mettre à jour les indicateurs techniques\n`;
    t += `     4. Évaluer le signal (ACHAT / VENTE / HOLD)\n`;
    t += `     5. Si signal != HOLD → exécuter le trade sur Polymarket\n`;
    t += `     6. Logger le trade et le résultat\n`;
    t += `     7. Vérifier les conditions de risk management\n`;
    t += `   }\n\n`;

    t += `=== RÉSULTATS DU BACKTEST ===\n`;
    t += `Win Rate: ${wr.toFixed(1)}% | Net PnL: ${pnl.toFixed(2)} USDC | Sharpe: ${sharpe.toFixed(2)}\n`;
    t += `Sortino: ${sortino.toFixed(2)} | Max Drawdown: ${dd.toFixed(1)}% | Trades: ${row.total_trades}\n`;
    t += `Rendement annualisé: ${annRet.toFixed(1)}% | Profit Factor: ${pf.toFixed(2)}\n`;
    if (conf > 0) t += `Confiance stratégie (analyse quartiles): ${conf.toFixed(0)}%\n`;

    return t;
  }

  function getMaxPeriod(type, p) {
    const vals = Object.values(p).filter(v => typeof v === 'number' && v > 0 && v < 1000);
    return Math.max(50, ...vals) + 10;
  }

  function getDetailedIndicatorCalc(type, p) {
    const calcs = {
      rsi: () =>
        `   a. Calculer le RSI sur les ${p.period} dernières bougies:\n` +
        `      - Pour chaque bougie: gain = max(close - prev_close, 0), loss = max(prev_close - close, 0)\n` +
        `      - avg_gain = SMA(gains, ${p.period}), avg_loss = SMA(losses, ${p.period})\n` +
        `      - RS = avg_gain / avg_loss\n` +
        `      - RSI = 100 - (100 / (1 + RS))\n`,
      bollinger_bands: () =>
        `   a. Calculer la SMA(close, ${p.period})\n` +
        `   b. Calculer l'écart-type σ sur ${p.period} bougies\n` +
        `   c. Bande haute = SMA + ${p.multiplier} × σ\n` +
        `   d. Bande basse = SMA - ${p.multiplier} × σ\n`,
      macd: () =>
        `   a. EMA_fast = EMA(close, ${p.fast})\n` +
        `   b. EMA_slow = EMA(close, ${p.slow})\n` +
        `   c. MACD_line = EMA_fast - EMA_slow\n` +
        `   d. Signal_line = EMA(MACD_line, ${p.signal})\n` +
        `   e. Histogram = MACD_line - Signal_line\n`,
      ema_crossover: () =>
        `   a. EMA_fast = EMA(close, ${p.fast_period})\n` +
        `   b. EMA_slow = EMA(close, ${p.slow_period})\n` +
        `   c. Stocker la valeur précédente pour détecter les croisements\n`,
      stochastic: () =>
        `   a. Highest_high = max(high) sur ${p.period} bougies\n` +
        `   b. Lowest_low = min(low) sur ${p.period} bougies\n` +
        `   c. %K = 100 × (close - Lowest_low) / (Highest_high - Lowest_low)\n` +
        `   d. %D = SMA(%K, 3)\n`,
      atr_mean_reversion: () =>
        `   a. TR = max(high-low, |high-prev_close|, |low-prev_close|)\n` +
        `   b. ATR = SMA(TR, ${p.atr_period})\n` +
        `   c. SMA_price = SMA(close, ${p.sma_period})\n` +
        `   d. Upper = SMA_price + ${p.multiplier} × ATR\n` +
        `   e. Lower = SMA_price - ${p.multiplier} × ATR\n`,
      vwap: () =>
        `   a. typical_price = (high + low + close) / 3\n` +
        `   b. VWAP = Σ(typical_price × volume) / Σ(volume) sur ${p.period} bougies\n`,
      obv: () =>
        `   a. Si close > prev_close: OBV += volume\n` +
        `   b. Si close < prev_close: OBV -= volume\n` +
        `   c. OBV_SMA = SMA(OBV, ${p.sma_period})\n`,
      williams_r: () =>
        `   a. Highest_high = max(high) sur ${p.period} bougies\n` +
        `   b. Lowest_low = min(low) sur ${p.period} bougies\n` +
        `   c. %R = -100 × (Highest_high - close) / (Highest_high - Lowest_low)\n`,
      adx: () =>
        `   a. Calculer +DM et -DM (Directional Movement)\n` +
        `   b. +DI = 100 × EMA(+DM, ${p.period}) / ATR\n` +
        `   c. -DI = 100 × EMA(-DM, ${p.period}) / ATR\n` +
        `   d. DX = 100 × |+DI - -DI| / (+DI + -DI)\n` +
        `   e. ADX = EMA(DX, ${p.period})\n`,
      gabagool: () =>
        `   a. Pour chaque bougie 15min, calculer price_change = (close - open) / open\n` +
        `   b. YES_price = 0.50 + clamp(price_change × 5, -0.40, 0.40)\n` +
        `   c. NO_price = 1.00 - YES_price\n` +
        `   d. spread = volatilité × ${p.spread_multiplier} (clampé entre 0.02 et 0.10)\n` +
        `   e. YES_fill = mid_yes - spread/2 - ${p.bid_offset}\n` +
        `   f. NO_fill = mid_no - spread/2 - ${p.bid_offset}\n` +
        `   g. pair_cost = YES_fill + NO_fill\n`,
    };

    // For combos, concatenate sub-indicators
    const combos = {
      rsi_bollinger: () => `   [RSI]\n${calcs.rsi()}   [Bollinger Bands]\n${calcs.bollinger_bands()}`,
      macd_rsi: () => `   [MACD — primaire]\n${calcs.macd()}   [RSI — filtre]\n` +
        `   a. RSI = calcul standard sur ${p.rsi_period} bougies (seuils: ${p.rsi_os}/${p.rsi_ob})\n`,
      ema_rsi: () => `   [EMA Crossover — primaire]\n${calcs.ema_crossover()}   [RSI — filtre]\n` +
        `   a. RSI sur ${p.rsi_period} bougies (seuils: ${p.rsi_os}/${p.rsi_ob})\n`,
      stoch_rsi: () => `   [Stochastic]\n${calcs.stochastic()}   [RSI]\n` +
        `   a. RSI sur ${p.rsi_period} bougies (seuils: ${p.rsi_os}/${p.rsi_ob})\n`,
      macd_bollinger: () => `   [MACD — primaire]\n${calcs.macd()}   [Bollinger — filtre]\n${calcs.bollinger_bands()}`,
      triple_rsi_macd_bb: () => `   [RSI]\n   a. RSI(${p.rsi_period}), OS=${p.rsi_os}, OB=${p.rsi_ob}\n   [MACD]\n   a. MACD(${p.macd_fast}/${p.macd_slow}/${p.macd_signal})\n   [BB]\n   a. BB(${p.bb_period}, mult=${p.bb_mult})\n`,
      triple_ema_rsi_stoch: () => `   [EMA]\n   a. EMA(${p.ema_fast}/${p.ema_slow})\n   [RSI]\n   a. RSI(${p.rsi_period}), OS=${p.rsi_os}, OB=${p.rsi_ob}\n   [Stoch]\n   a. Stoch(${p.stoch_period}), OS=${p.stoch_os}, OB=${p.stoch_ob}\n`,
      vwap_rsi: () => `   [VWAP — primaire]\n${calcs.vwap()}   [RSI — filtre]\n   a. RSI(${p.rsi_period}), OS=${p.rsi_oversold}, OB=${p.rsi_overbought}\n`,
      obv_macd: () => `   [MACD — primaire]\n${calcs.macd()}   [OBV — volume]\n${calcs.obv()}`,
      adx_ema: () => `   [EMA Crossover — primaire]\n   a. EMA(${p.ema_fast}/${p.ema_slow})\n   [ADX — filtre]\n   a. ADX(${p.adx_period}), seuil=${p.adx_threshold}\n`,
      williams_r_stoch: () => `   [Williams %R]\n   a. %R(${p.wr_period}), OS=${p.wr_oversold}, OB=${p.wr_overbought}\n   [Stochastic]\n   a. Stoch(${p.stoch_period}), OS=${p.stoch_oversold}, OB=${p.stoch_overbought}\n`,
    };

    const fn = combos[type] || calcs[type] || (() => '   Indicateur inconnu\n');
    return fn();
  }

  function getDetailedSignalLogic(type, p, sym) {
    const logics = {
      rsi: () =>
        `   SI RSI < ${p.oversold} → ACHAT (buy YES "${sym} up")\n` +
        `   SI RSI > ${p.overbought} → VENTE (buy NO "${sym} up")\n` +
        `   SINON → HOLD (ne rien faire)\n`,
      bollinger_bands: () =>
        `   SI close < bande_basse → ACHAT (buy YES)\n` +
        `   SI close > bande_haute → VENTE (buy NO)\n` +
        `   SINON → HOLD\n`,
      macd: () =>
        `   SI histogram passe de négatif à positif → ACHAT (buy YES)\n` +
        `   SI histogram passe de positif à négatif → VENTE (buy NO)\n` +
        `   SINON → HOLD\n`,
      ema_crossover: () =>
        `   SI EMA_fast croise au-dessus de EMA_slow (golden cross) → ACHAT\n` +
        `   SI EMA_fast croise en-dessous de EMA_slow (death cross) → VENTE\n` +
        `   SINON → HOLD\n`,
      stochastic: () =>
        `   SI %K croise %D vers le haut ET %K < ${p.oversold} → ACHAT\n` +
        `   SI %K croise %D vers le bas ET %K > ${p.overbought} → VENTE\n` +
        `   SINON → HOLD\n`,
      atr_mean_reversion: () =>
        `   SI close < SMA - ${p.multiplier} × ATR → ACHAT (prix très bas)\n` +
        `   SI close > SMA + ${p.multiplier} × ATR → VENTE (prix très haut)\n` +
        `   SINON → HOLD\n`,
      vwap: () =>
        `   SI close < VWAP → ACHAT (sous-évalué)\n` +
        `   SI close > VWAP → VENTE (surévalué)\n` +
        `   SINON → HOLD\n`,
      obv: () =>
        `   SI OBV > OBV_SMA → ACHAT (volume haussier)\n` +
        `   SI OBV < OBV_SMA → VENTE (volume baissier)\n` +
        `   SINON → HOLD\n`,
      williams_r: () =>
        `   SI %R < ${p.oversold} → ACHAT (survente extrême)\n` +
        `   SI %R > ${p.overbought} → VENTE (surachat extrême)\n` +
        `   SINON → HOLD\n`,
      adx: () =>
        `   SI ADX > ${p.adx_threshold} ET +DI > -DI → ACHAT (tendance haussière forte)\n` +
        `   SI ADX > ${p.adx_threshold} ET -DI > +DI → VENTE (tendance baissière forte)\n` +
        `   SI ADX < ${p.adx_threshold} → HOLD (pas de tendance)\n`,
      rsi_bollinger: () =>
        `   SI RSI < ${p.rsi_os} ET close < bande_basse → ACHAT (double confirmation survente)\n` +
        `   SI RSI > ${p.rsi_ob} ET close > bande_haute → VENTE (double confirmation surachat)\n` +
        `   SINON → HOLD (les deux indicateurs doivent être d'accord)\n`,
      macd_rsi: () =>
        `   SI MACD histogram cross up ET RSI < ${p.rsi_ob} → ACHAT\n` +
        `   SI MACD histogram cross down ET RSI > ${p.rsi_os} → VENTE\n` +
        `   SINON → HOLD (MACD sans signal ou RSI bloque)\n`,
      ema_rsi: () =>
        `   SI golden cross EMA ET RSI < ${p.rsi_ob} → ACHAT\n` +
        `   SI death cross EMA ET RSI > ${p.rsi_os} → VENTE\n` +
        `   SINON → HOLD\n`,
      stoch_rsi: () =>
        `   SI Stoch cross up en zone < ${p.stoch_os} ET RSI < ${p.rsi_os} → ACHAT\n` +
        `   SI Stoch cross down en zone > ${p.stoch_ob} ET RSI > ${p.rsi_ob} → VENTE\n` +
        `   SINON → HOLD\n`,
      macd_bollinger: () =>
        `   SI MACD cross up ET close près de bande basse → ACHAT\n` +
        `   SI MACD cross down ET close près de bande haute → VENTE\n` +
        `   SINON → HOLD\n`,
      triple_rsi_macd_bb: () =>
        `   Compter les votes: RSI signal + MACD signal + BB signal\n` +
        `   SI >= 2 signaux ACHAT → ACHAT (majorité)\n` +
        `   SI >= 2 signaux VENTE → VENTE (majorité)\n` +
        `   SINON → HOLD (pas de consensus)\n`,
      triple_ema_rsi_stoch: () =>
        `   Compter les votes: EMA signal + RSI signal + Stoch signal\n` +
        `   SI >= 2 signaux ACHAT → ACHAT (majorité)\n` +
        `   SI >= 2 signaux VENTE → VENTE (majorité)\n` +
        `   SINON → HOLD\n`,
      vwap_rsi: () =>
        `   SI close < VWAP ET RSI < ${p.rsi_overbought} → ACHAT\n` +
        `   SI close > VWAP ET RSI > ${p.rsi_oversold} → VENTE\n` +
        `   SINON → HOLD\n`,
      obv_macd: () =>
        `   SI MACD cross up ET OBV > OBV_SMA → ACHAT (momentum + volume)\n` +
        `   SI MACD cross down ET OBV < OBV_SMA → VENTE\n` +
        `   SINON → HOLD\n`,
      adx_ema: () =>
        `   SI golden cross EMA ET ADX > ${p.adx_threshold} → ACHAT\n` +
        `   SI death cross EMA ET ADX > ${p.adx_threshold} → VENTE\n` +
        `   SI ADX < ${p.adx_threshold} → HOLD (tendance trop faible)\n`,
      williams_r_stoch: () =>
        `   SI %R < ${p.wr_oversold} ET Stoch cross up < ${p.stoch_oversold} → ACHAT\n` +
        `   SI %R > ${p.wr_overbought} ET Stoch cross down > ${p.stoch_overbought} → VENTE\n` +
        `   SINON → HOLD\n`,
      gabagool: () =>
        `   SI pair_cost < ${p.max_pair_cost} → TRADE (acheter YES + NO)\n` +
        `   SI pair_cost >= ${p.max_pair_cost} → SKIP (pas d'arbitrage)\n`,
    };
    return (logics[type] || (() => '   Logique inconnue\n'))();
  }

  function getStrategyColor(name) {
    const colors = {
      'RSI': 'text-purple-400', 'Bollinger Bands': 'text-blue-400', 'MACD': 'text-cyan-400',
      'EMA Crossover': 'text-emerald-400', 'Stochastic': 'text-yellow-400', 'ATR Mean Reversion': 'text-pink-400',
      'Gabagool': 'text-orange-400', 'VWAP': 'text-teal-400', 'OBV': 'text-lime-400',
      'Williams': 'text-rose-400', 'ADX': 'text-indigo-400',
    };
    for (const [key, color] of Object.entries(colors)) {
      if (name.includes(key) || name.includes(key.split(' ')[0])) return color;
    }
    return 'text-cyan-400';
  }

  function getRankStyle(rank) {
    if (rank === 1) return { medal: '#FFD700', label: '1er' };
    if (rank === 2) return { medal: '#C0C0C0', label: '2e' };
    if (rank === 3) return { medal: '#CD7F32', label: '3e' };
    return null;
  }

  loadStrategies();
</script>

<div class="space-y-8">
  <!-- Header -->
  <div class="flex items-center gap-3">
    <BookOpen size={28} class="text-violet-400" />
    <div>
      <h2 class="text-2xl font-bold text-white">Strategy Playbook</h2>
      <p class="text-sm text-gray-400">Top 3 stratégies par win rate — guide d'implémentation bot Polymarket</p>
    </div>
    {#if $discoveryStatus.running}
      <div class="ml-auto flex items-center gap-2 px-3 py-1 bg-cyan-900/40 border border-cyan-700/50 rounded-full">
        <div class="w-2 h-2 rounded-full bg-cyan-400 animate-pulse"></div>
        <span class="text-xs text-cyan-400 font-semibold uppercase tracking-wider">LIVE</span>
      </div>
    {/if}
  </div>

  {#if loading}
    <div class="flex justify-center py-12">
      <Loader2 class="w-8 h-8 text-violet-400 animate-spin" />
    </div>
  {:else if strategies.length === 0}
    <div class="text-center text-gray-500 py-12">
      <BookOpen class="w-12 h-12 mx-auto mb-3 opacity-50" />
      <p>Aucune stratégie dans la knowledge base.</p>
      <p class="text-sm mt-1">Lancez un Discovery Agent scan pour la remplir.</p>
    </div>
  {:else}
    {#each strategies as row, i}
      {@const rank = i + 1}
      {@const style = getRankStyle(rank)}
      {@const pnl = parseFloat(row.net_pnl)}
      {@const wr = parseFloat(row.win_rate)}
      {@const confVal = parseFloat(row.strategy_confidence || 0)}
      {@const polyParams = getPolymarketParams(row)}
      {@const botDesc = generateBotDescription(row)}
      <div class="bg-gray-800 rounded-xl border border-gray-700 overflow-hidden">
        <!-- Strategy Header with medal -->
        <div class="flex items-center gap-4 px-6 py-5 border-b border-gray-700" style="background: linear-gradient(90deg, rgba(139,92,246,0.1), transparent)">
          <div class="text-center">
            <span class="text-3xl font-black" style="color: {style.medal}">{style.label}</span>
          </div>
          <div class="flex-1">
            <div class="text-xl font-bold {getStrategyColor(row.strategy_name)}">{row.strategy_name}</div>
            <div class="text-sm text-gray-400">{row.symbol} · {row.days} jours · sizing: {row.sizing_mode}</div>
          </div>
          <div class="flex gap-6 text-right">
            <div>
              <div class="text-xs text-gray-500">Win Rate</div>
              <div class="text-2xl font-black font-mono text-yellow-400">{wr.toFixed(1)}%</div>
            </div>
            <div>
              <div class="text-xs text-gray-500">Net PnL</div>
              <div class="text-xl font-bold font-mono {pnl >= 0 ? 'text-green-400' : 'text-red-400'}">{pnl >= 0 ? '+' : ''}{pnl.toFixed(2)}</div>
            </div>
            <div>
              <div class="text-xs text-gray-500">Sharpe</div>
              <div class="text-xl font-bold font-mono text-gray-300">{parseFloat(row.sharpe_ratio).toFixed(2)}</div>
            </div>
            {#if confVal > 0}
              <div>
                <div class="text-xs text-gray-500">Confiance</div>
                <div class="text-xl font-bold font-mono {confVal >= 70 ? 'text-green-400' : confVal >= 40 ? 'text-yellow-400' : 'text-red-400'}">{confVal.toFixed(0)}%</div>
              </div>
            {/if}
          </div>
        </div>

        <!-- Polymarket Parameters Table -->
        <div class="px-6 py-4 border-b border-gray-700">
          <h4 class="text-sm font-semibold text-violet-400 uppercase tracking-wider mb-3">Paramètres essentiels Polymarket</h4>
          <div class="overflow-x-auto">
            <table class="w-full text-sm">
              <thead>
                <tr class="text-gray-500 text-xs uppercase">
                  <th class="text-left pb-2 pr-4">Paramètre</th>
                  <th class="text-left pb-2 pr-4">Valeur</th>
                  <th class="text-left pb-2">Description</th>
                </tr>
              </thead>
              <tbody>
                {#each polyParams as item}
                  <tr class="border-t border-gray-700/40">
                    <td class="py-2 pr-4 font-medium {item.param.startsWith('→') ? 'text-yellow-400' : 'text-gray-300'} whitespace-nowrap">{item.param}</td>
                    <td class="py-2 pr-4 font-mono text-white whitespace-nowrap">{item.value}</td>
                    <td class="py-2 text-xs text-gray-500">{item.desc}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </div>

        <!-- Bot Implementation Description -->
        <div class="px-6 py-4">
          <div class="flex items-center justify-between mb-3">
            <h4 class="text-sm font-semibold text-violet-400 uppercase tracking-wider">Guide d'implémentation bot</h4>
            <button
              onclick={() => copyDescription(botDesc, i)}
              class="flex items-center gap-1.5 px-3 py-1.5 rounded text-xs {copiedIdx === i ? 'bg-green-600 text-white' : 'bg-gray-700 text-gray-400 hover:bg-gray-600 hover:text-white'} transition-colors"
            >
              {#if copiedIdx === i}
                <Check size={14} /> Copié !
              {:else}
                <Copy size={14} /> Copier la description
              {/if}
            </button>
          </div>
          <pre class="text-xs text-gray-300 whitespace-pre-wrap font-mono bg-gray-900/60 rounded-lg p-5 max-h-96 overflow-y-auto leading-relaxed border border-gray-700/50">{botDesc}</pre>
        </div>
      </div>
    {/each}
  {/if}
</div>
