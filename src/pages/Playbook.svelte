<script>
  import { onDestroy } from 'svelte';
  import { getTopStrategies } from '../lib/api.js';
  import { discoveryStatus } from '../lib/stores.js';
  import { BookOpen, Loader2, Copy, Check } from 'lucide-svelte';

  // ============================================================================
  // State
  // ============================================================================
  let strategies = $state([]);
  let loading = $state(false);
  let autoRefreshInterval = $state(null);
  let copiedIdx = $state(-1);

  // ============================================================================
  // Data loading
  // ============================================================================
  async function loadStrategies() {
    loading = true;
    const res = await getTopStrategies(10, 'net_pnl');
    if (res.success !== false) {
      strategies = res.data || [];
    }
    loading = false;
  }

  // ============================================================================
  // Auto-refresh when discovery is running
  // ============================================================================
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

  // ============================================================================
  // Copy to clipboard
  // ============================================================================
  async function copyDescription(text, idx) {
    await navigator.clipboard.writeText(text);
    copiedIdx = idx;
    setTimeout(() => { copiedIdx = -1; }, 2000);
  }

  // ============================================================================
  // Strategy descriptions — AI-readable
  // ============================================================================

  const paramDescriptions = {
    period: 'Lookback period (number of candles)',
    rsi_period: 'RSI lookback period (number of candles)',
    overbought: 'Overbought threshold — sell when indicator exceeds this level',
    oversold: 'Oversold threshold — buy when indicator drops below this level',
    rsi_ob: 'RSI overbought threshold',
    rsi_os: 'RSI oversold threshold',
    rsi_overbought: 'RSI overbought threshold',
    rsi_oversold: 'RSI oversold threshold',
    multiplier: 'Band width multiplier (standard deviations from mean)',
    bb_mult: 'Bollinger Bands multiplier (standard deviations)',
    bb_period: 'Bollinger Bands SMA period',
    fast: 'Fast EMA period for MACD line',
    slow: 'Slow EMA period for MACD line',
    signal: 'Signal line EMA period',
    macd_fast: 'MACD fast EMA period',
    macd_slow: 'MACD slow EMA period',
    macd_signal: 'MACD signal line period',
    fast_period: 'Fast EMA period',
    slow_period: 'Slow EMA period',
    ema_fast: 'Fast EMA period',
    ema_slow: 'Slow EMA period',
    stoch_period: 'Stochastic oscillator lookback period',
    stoch_ob: 'Stochastic overbought threshold',
    stoch_os: 'Stochastic oversold threshold',
    stoch_overbought: 'Stochastic overbought threshold',
    stoch_oversold: 'Stochastic oversold threshold',
    atr_period: 'ATR (Average True Range) lookback period',
    sma_period: 'Simple Moving Average period',
    obv_sma_period: 'OBV Simple Moving Average period',
    vwap_period: 'VWAP lookback period',
    adx_period: 'ADX lookback period',
    adx_threshold: 'ADX minimum strength threshold — only trade when trend is strong',
    wr_period: 'Williams %R lookback period',
    wr_overbought: 'Williams %R overbought level (near 0)',
    wr_oversold: 'Williams %R oversold level (near -100)',
    max_pair_cost: 'Maximum combined cost of YES+NO pair to enter arbitrage',
    bid_offset: 'Offset below mid-price for maker order placement',
    spread_multiplier: 'Multiplier for volatility-based spread calculation',
  };

  function getIndicatorSetup(type, params) {
    const setups = {
      rsi: () => `Configure RSI indicator with period=${params.period || params.rsi_period}. Overbought level at ${params.overbought || params.rsi_ob}, oversold level at ${params.oversold || params.rsi_os}.`,
      bollinger_bands: () => `Configure Bollinger Bands with SMA period=${params.period || params.bb_period} and multiplier=${params.multiplier || params.bb_mult} standard deviations.`,
      macd: () => `Configure MACD with fast EMA=${params.fast || params.macd_fast}, slow EMA=${params.slow || params.macd_slow}, signal line=${params.signal || params.macd_signal}.`,
      ema_crossover: () => `Configure dual EMA crossover with fast period=${params.fast_period || params.ema_fast} and slow period=${params.slow_period || params.ema_slow}.`,
      stochastic: () => `Configure Stochastic Oscillator with period=${params.period || params.stoch_period}. Overbought at ${params.overbought || params.stoch_ob}, oversold at ${params.oversold || params.stoch_os}.`,
      atr_mean_reversion: () => `Configure ATR Mean Reversion with ATR period=${params.atr_period}, SMA period=${params.sma_period}, multiplier=${params.multiplier}. Measures distance from mean in ATR units.`,
      vwap: () => `Configure VWAP (Volume Weighted Average Price) with period=${params.period || params.vwap_period}.`,
      obv: () => `Configure OBV (On-Balance Volume) with SMA smoothing period=${params.sma_period || params.obv_sma_period}.`,
      williams_r: () => `Configure Williams %R with period=${params.period || params.wr_period}. Overbought at ${params.overbought || params.wr_overbought} (near 0), oversold at ${params.oversold || params.wr_oversold} (near -100).`,
      adx: () => `Configure ADX (Average Directional Index) with period=${params.period || params.adx_period}. Trend strength threshold=${params.adx_threshold}. Only trade when ADX > threshold.`,
      rsi_bollinger: () => `Combine RSI (period=${params.rsi_period}, OB=${params.rsi_ob}, OS=${params.rsi_os}) with Bollinger Bands (period=${params.bb_period}, mult=${params.bb_mult}). Mode: UNANIMOUS — both must agree.`,
      macd_rsi: () => `Combine MACD (fast=${params.macd_fast}, slow=${params.macd_slow}, signal=${params.macd_signal}) as PRIMARY with RSI (period=${params.rsi_period}, OB=${params.rsi_ob}, OS=${params.rsi_os}) as CONFIRMER.`,
      ema_rsi: () => `Combine EMA Crossover (fast=${params.ema_fast}, slow=${params.ema_slow}) as PRIMARY with RSI (period=${params.rsi_period}, OB=${params.rsi_ob}, OS=${params.rsi_os}) as CONFIRMER.`,
      stoch_rsi: () => `Combine Stochastic (period=${params.stoch_period}, OB=${params.stoch_ob}, OS=${params.stoch_os}) with RSI (period=${params.rsi_period}, OB=${params.rsi_ob}, OS=${params.rsi_os}). Mode: UNANIMOUS.`,
      macd_bollinger: () => `Combine MACD (fast=${params.macd_fast}, slow=${params.macd_slow}, signal=${params.macd_signal}) as PRIMARY with Bollinger Bands (period=${params.bb_period}, mult=${params.bb_mult}) as CONFIRMER.`,
      triple_rsi_macd_bb: () => `Triple indicator: RSI (period=${params.rsi_period}, OB=${params.rsi_ob}, OS=${params.rsi_os}) + MACD (fast=${params.macd_fast}, slow=${params.macd_slow}, signal=${params.macd_signal}) + Bollinger Bands (period=${params.bb_period}, mult=${params.bb_mult}). Mode: MAJORITY (2 out of 3 must agree).`,
      triple_ema_rsi_stoch: () => `Triple indicator: EMA Crossover (fast=${params.ema_fast}, slow=${params.ema_slow}) + RSI (period=${params.rsi_period}, OB=${params.rsi_ob}, OS=${params.rsi_os}) + Stochastic (period=${params.stoch_period}, OB=${params.stoch_ob}, OS=${params.stoch_os}). Mode: MAJORITY (2 out of 3 must agree).`,
      vwap_rsi: () => `Combine VWAP (period=${params.vwap_period}) as PRIMARY with RSI (period=${params.rsi_period}, OB=${params.rsi_overbought}, OS=${params.rsi_oversold}) as CONFIRMER.`,
      obv_macd: () => `Combine MACD (fast=${params.macd_fast}, slow=${params.macd_slow}, signal=${params.macd_signal}) as PRIMARY with OBV (SMA period=${params.obv_sma_period}) as volume CONFIRMER.`,
      adx_ema: () => `Combine EMA Crossover (fast=${params.ema_fast}, slow=${params.ema_slow}) as PRIMARY with ADX (period=${params.adx_period}, threshold=${params.adx_threshold}) as trend strength FILTER.`,
      williams_r_stoch: () => `Combine Williams %R (period=${params.wr_period}, OB=${params.wr_overbought}, OS=${params.wr_oversold}) with Stochastic (period=${params.stoch_period}, OB=${params.stoch_overbought}, OS=${params.stoch_oversold}). Mode: UNANIMOUS.`,
      gabagool: () => `Binary arbitrage engine. Each 15-min candle generates a synthetic Polymarket binary market. YES price = 0.50 + clamp(price_change × 5, -0.40, 0.40). NO price = 1.00 - YES. Spread = volatility × ${params.spread_multiplier}. Fill prices use bid_offset=${params.bid_offset}. Trade when YES_fill + NO_fill < ${params.max_pair_cost}.`,
    };
    return (setups[type] || (() => 'Unknown strategy type.'))();
  }

  function getTradingRules(type, symbol) {
    const sym = symbol.replace('USDT', '');
    const rules = {
      rsi: `BUY (Polymarket: Buy YES on "${sym} up in next 15min"): When RSI drops below oversold threshold.\nSELL (Polymarket: Buy NO on "${sym} up in next 15min"): When RSI rises above overbought threshold.\nHOLD: When RSI is between oversold and overbought levels.`,
      bollinger_bands: `BUY (Polymarket: Buy YES on "${sym} up"): When price closes below the lower Bollinger Band — oversold, expect mean reversion up.\nSELL (Polymarket: Buy NO on "${sym} up"): When price closes above the upper Bollinger Band — overbought, expect mean reversion down.\nHOLD: When price is within the bands.`,
      macd: `BUY (Polymarket: Buy YES on "${sym} up"): When MACD histogram crosses from negative to positive — bullish momentum shift.\nSELL (Polymarket: Buy NO on "${sym} up"): When MACD histogram crosses from positive to negative — bearish momentum shift.\nHOLD: When no histogram crossover occurs.`,
      ema_crossover: `BUY (Polymarket: Buy YES on "${sym} up"): When fast EMA crosses above slow EMA (Golden Cross) — uptrend starting.\nSELL (Polymarket: Buy NO on "${sym} up"): When fast EMA crosses below slow EMA (Death Cross) — downtrend starting.\nHOLD: When no crossover occurs.`,
      stochastic: `BUY (Polymarket: Buy YES on "${sym} up"): When %K crosses above %D in the oversold zone — reversal up expected.\nSELL (Polymarket: Buy NO on "${sym} up"): When %K crosses below %D in the overbought zone — reversal down expected.\nHOLD: When no crossover in extreme zones.`,
      atr_mean_reversion: `BUY (Polymarket: Buy YES on "${sym} up"): When price < SMA - (ATR × multiplier) — extremely far below mean, expect reversion up.\nSELL (Polymarket: Buy NO on "${sym} up"): When price > SMA + (ATR × multiplier) — extremely far above mean, expect reversion down.\nHOLD: When price is within ATR bands of the mean.`,
      vwap: `BUY (Polymarket: Buy YES on "${sym} up"): When price is below VWAP — undervalued relative to volume-weighted fair value.\nSELL (Polymarket: Buy NO on "${sym} up"): When price is above VWAP — overvalued relative to fair value.\nHOLD: When price is near VWAP.`,
      obv: `BUY (Polymarket: Buy YES on "${sym} up"): When OBV > SMA(OBV) — volume flow supports bullish move.\nSELL (Polymarket: Buy NO on "${sym} up"): When OBV < SMA(OBV) — volume flow supports bearish move.\nHOLD: When OBV is near its SMA.`,
      williams_r: `BUY (Polymarket: Buy YES on "${sym} up"): When Williams %R drops below oversold level (near -100) — extreme oversold.\nSELL (Polymarket: Buy NO on "${sym} up"): When Williams %R rises above overbought level (near 0) — extreme overbought.\nHOLD: When %R is between thresholds.`,
      adx: `BUY (Polymarket: Buy YES on "${sym} up"): When ADX > threshold AND +DI > -DI — strong bullish trend confirmed.\nSELL (Polymarket: Buy NO on "${sym} up"): When ADX > threshold AND -DI > +DI — strong bearish trend confirmed.\nHOLD: When ADX < threshold (weak/no trend).`,
      rsi_bollinger: `BUY (Polymarket: Buy YES on "${sym} up"): When BOTH RSI is oversold AND price is below lower Bollinger Band — double confirmation of oversold condition.\nSELL (Polymarket: Buy NO on "${sym} up"): When BOTH RSI is overbought AND price is above upper Bollinger Band.\nHOLD: When indicators disagree or neither triggers.`,
      macd_rsi: `BUY (Polymarket: Buy YES on "${sym} up"): When MACD histogram crosses bullish (primary) AND RSI is not in overbought zone (confirms room to rise).\nSELL (Polymarket: Buy NO on "${sym} up"): When MACD histogram crosses bearish (primary) AND RSI is not oversold (confirms room to fall).\nHOLD: When MACD has no signal or RSI blocks confirmation.`,
      ema_rsi: `BUY (Polymarket: Buy YES on "${sym} up"): When EMA golden cross occurs (primary) AND RSI confirms not overbought.\nSELL (Polymarket: Buy NO on "${sym} up"): When EMA death cross occurs (primary) AND RSI confirms not oversold.\nHOLD: When no EMA crossover or RSI blocks.`,
      stoch_rsi: `BUY (Polymarket: Buy YES on "${sym} up"): When BOTH Stochastic crosses up in oversold zone AND RSI is oversold — double oscillator confirmation.\nSELL (Polymarket: Buy NO on "${sym} up"): When BOTH Stochastic crosses down in overbought zone AND RSI is overbought.\nHOLD: When oscillators disagree.`,
      macd_bollinger: `BUY (Polymarket: Buy YES on "${sym} up"): When MACD crosses bullish (primary) AND price is near lower Bollinger Band (confirms oversold).\nSELL (Polymarket: Buy NO on "${sym} up"): When MACD crosses bearish (primary) AND price is near upper Bollinger Band.\nHOLD: When MACD has no signal or BB doesn't confirm.`,
      triple_rsi_macd_bb: `BUY (Polymarket: Buy YES on "${sym} up"): When at least 2 out of 3 indicators (RSI, MACD, Bollinger) signal BUY — majority vote.\nSELL (Polymarket: Buy NO on "${sym} up"): When at least 2 out of 3 signal SELL.\nHOLD: When no majority consensus (e.g., 1 buy, 1 sell, 1 hold).`,
      triple_ema_rsi_stoch: `BUY (Polymarket: Buy YES on "${sym} up"): When at least 2 out of 3 indicators (EMA Cross, RSI, Stochastic) signal BUY.\nSELL (Polymarket: Buy NO on "${sym} up"): When at least 2 out of 3 signal SELL.\nHOLD: When no majority consensus.`,
      vwap_rsi: `BUY (Polymarket: Buy YES on "${sym} up"): When price < VWAP (primary) AND RSI confirms not overbought.\nSELL (Polymarket: Buy NO on "${sym} up"): When price > VWAP (primary) AND RSI confirms not oversold.\nHOLD: When VWAP has no signal or RSI blocks.`,
      obv_macd: `BUY (Polymarket: Buy YES on "${sym} up"): When MACD crosses bullish (primary) AND OBV is above its SMA (volume confirms momentum).\nSELL (Polymarket: Buy NO on "${sym} up"): When MACD crosses bearish (primary) AND OBV is below its SMA.\nHOLD: When MACD has no signal or volume doesn't confirm.`,
      adx_ema: `BUY (Polymarket: Buy YES on "${sym} up"): When EMA golden cross occurs (primary) AND ADX > threshold (strong trend confirmed).\nSELL (Polymarket: Buy NO on "${sym} up"): When EMA death cross occurs (primary) AND ADX > threshold.\nHOLD: When no EMA crossover or ADX indicates weak trend.`,
      williams_r_stoch: `BUY (Polymarket: Buy YES on "${sym} up"): When BOTH Williams %R is oversold AND Stochastic crosses up in oversold zone — double confirmation.\nSELL (Polymarket: Buy NO on "${sym} up"): When BOTH Williams %R is overbought AND Stochastic crosses down in overbought zone.\nHOLD: When oscillators disagree.`,
      gabagool: `This is a NON-DIRECTIONAL arbitrage strategy. It does NOT predict price direction.\nTRADE: When the combined cost of buying YES + NO < max_pair_cost — buy BOTH sides to lock guaranteed profit = 1.00 - pair_cost.\nSKIP: When pair cost is too high (no arbitrage opportunity).\nProfit is guaranteed regardless of outcome since YES + NO always pays 1.00.`,
    };
    return (rules[type] || 'Unknown strategy type.')();
  }

  function generateFullDescription(row) {
    const parsed = parseParams(row.strategy_params);
    const params = parsed.values;
    const type = row.strategy_type;
    const pnl = parseFloat(row.net_pnl);
    const wr = parseFloat(row.win_rate);
    const sharpe = parseFloat(row.sharpe_ratio);
    const sortino = parseFloat(row.sortino_ratio || 0);
    const dd = parseFloat(row.max_drawdown_pct);
    const conf = parseFloat(row.strategy_confidence || 0);
    const annRet = parseFloat(row.annualized_return_pct || 0);

    let text = `## ${row.strategy_name} on ${row.symbol}\n`;
    text += `Timeframe: 15-minute candles (Binance klines) | Backtest period: ${row.days} days\n\n`;

    text += `### Indicator Setup\n`;
    text += getIndicatorSetup(type, params) + '\n\n';

    text += `### Parameters\n`;
    for (const [key, val] of Object.entries(params)) {
      const desc = paramDescriptions[key] || '';
      text += `- ${key} = ${val}${desc ? ' — ' + desc : ''}\n`;
    }
    text += '\n';

    text += `### Trading Rules\n`;
    text += getTradingRules(type, row.symbol) + '\n\n';

    text += `### Position Sizing\n`;
    text += `Mode: ${row.sizing_mode} | `;
    if (row.sizing_mode === 'fixed') text += 'Fixed position size of $10 per trade.\n';
    else if (row.sizing_mode === 'kelly') text += 'Kelly criterion — position size proportional to edge/odds ratio.\n';
    else text += 'Confidence-weighted — position size scales with signal confidence (0.3-1.0).\n';
    text += '\n';

    text += `### Polymarket Execution\n`;
    if (type === 'gabagool') {
      text += `Market type: Binary crypto prediction (e.g., "Will BTC go up in the next 15 minutes?")\n`;
      text += `Execution: Place maker orders on BOTH YES and NO sides simultaneously.\n`;
      text += `Profit mechanism: Guaranteed profit when YES_fill + NO_fill < 1.00 (after fees).\n`;
    } else {
      text += `Market type: Binary crypto prediction (e.g., "Will ${row.symbol.replace('USDT', '')} go up in the next 15 minutes?")\n`;
      text += `On BUY signal: Purchase YES tokens (betting price will rise).\n`;
      text += `On SELL signal: Purchase NO tokens (betting price will fall).\n`;
      text += `Fee model: Polymarket taker fee = C × feeRate × (p × (1-p))^2 where p = estimated probability.\n`;
    }
    text += '\n';

    text += `### Backtest Results\n`;
    text += `Net PnL: ${pnl.toFixed(2)} USDC | Win Rate: ${wr.toFixed(1)}% | Sharpe: ${sharpe.toFixed(2)} | Sortino: ${sortino.toFixed(2)}\n`;
    text += `Max Drawdown: ${dd.toFixed(1)}% | Total Trades: ${row.total_trades} | Annualized Return: ${annRet.toFixed(1)}%\n`;
    if (conf > 0) text += `Strategy Confidence (quartile analysis): ${conf.toFixed(0)}%\n`;

    return text;
  }

  function parseParams(jsonStr) {
    try {
      const obj = JSON.parse(jsonStr);
      // Strategy params are wrapped: { "StrategyType": { ...params } }
      const keys = Object.keys(obj);
      if (keys.length === 1 && typeof obj[keys[0]] === 'object') {
        return { type: keys[0], values: obj[keys[0]] };
      }
      return { type: '', values: obj };
    } catch {
      return { type: '', values: {} };
    }
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

  // ============================================================================
  // Load on mount
  // ============================================================================
  loadStrategies();
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center gap-3">
    <BookOpen size={28} class="text-violet-400" />
    <div>
      <h2 class="text-2xl font-bold text-white">Strategy Playbook</h2>
      <p class="text-sm text-gray-400">Top 10 strategies by Net PnL — AI-readable descriptions for Polymarket execution</p>
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
      <p>No strategies in the knowledge base yet.</p>
      <p class="text-sm mt-1">Run a Discovery Agent scan to populate it.</p>
    </div>
  {:else}
    {#each strategies as row, i}
      {@const pnl = parseFloat(row.net_pnl)}
      {@const wr = parseFloat(row.win_rate)}
      {@const confVal = parseFloat(row.strategy_confidence || 0)}
      {@const parsed = parseParams(row.strategy_params)}
      {@const description = generateFullDescription(row)}
      <div class="bg-gray-800 rounded-lg border border-gray-700 overflow-hidden">
        <!-- Strategy Header -->
        <div class="flex items-center gap-4 px-5 py-4 border-b border-gray-700 bg-gray-800/80">
          <span class="text-2xl font-black text-violet-400">#{i + 1}</span>
          <div class="flex-1">
            <div class="text-lg font-bold {getStrategyColor(row.strategy_name)}">{row.strategy_name}</div>
            <div class="text-xs text-gray-400">{row.symbol} · {row.days} days · {row.sizing_mode}</div>
          </div>
          <div class="flex gap-4 text-right">
            <div>
              <div class="text-xs text-gray-500">Net PnL</div>
              <div class="text-lg font-bold font-mono {pnl >= 0 ? 'text-green-400' : 'text-red-400'}">{pnl >= 0 ? '+' : ''}{pnl.toFixed(2)}</div>
            </div>
            <div>
              <div class="text-xs text-gray-500">Win Rate</div>
              <div class="text-lg font-bold font-mono {wr >= 60 ? 'text-yellow-400' : 'text-gray-300'}">{wr.toFixed(1)}%</div>
            </div>
            <div>
              <div class="text-xs text-gray-500">Sharpe</div>
              <div class="text-lg font-bold font-mono text-gray-300">{parseFloat(row.sharpe_ratio).toFixed(2)}</div>
            </div>
            {#if confVal > 0}
              <div>
                <div class="text-xs text-gray-500">Confidence</div>
                <div class="text-lg font-bold font-mono {confVal >= 70 ? 'text-green-400' : confVal >= 40 ? 'text-yellow-400' : 'text-red-400'}">{confVal.toFixed(0)}%</div>
              </div>
            {/if}
          </div>
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-0">
          <!-- Parameters Table -->
          <div class="p-5 border-b lg:border-b-0 lg:border-r border-gray-700">
            <h4 class="text-xs text-gray-400 uppercase tracking-wider mb-3">Parameters</h4>
            <table class="w-full text-sm">
              <thead>
                <tr class="text-gray-500 text-xs">
                  <th class="text-left pb-1">Parameter</th>
                  <th class="text-right pb-1">Value</th>
                  <th class="text-left pb-1 pl-3">Description</th>
                </tr>
              </thead>
              <tbody>
                {#each Object.entries(parsed.values) as [key, val]}
                  <tr class="border-t border-gray-700/50">
                    <td class="py-1.5 font-mono text-violet-300 text-xs">{key}</td>
                    <td class="py-1.5 text-right font-mono text-white">{val}</td>
                    <td class="py-1.5 pl-3 text-xs text-gray-500">{paramDescriptions[key] || ''}</td>
                  </tr>
                {/each}
              </tbody>
            </table>

            <!-- Extra metrics -->
            <h4 class="text-xs text-gray-400 uppercase tracking-wider mt-4 mb-2">Performance Metrics</h4>
            <div class="grid grid-cols-2 gap-2 text-xs">
              <div class="flex justify-between"><span class="text-gray-500">Sortino</span><span class="font-mono text-gray-300">{parseFloat(row.sortino_ratio || 0).toFixed(2)}</span></div>
              <div class="flex justify-between"><span class="text-gray-500">Max Drawdown</span><span class="font-mono text-red-400">{parseFloat(row.max_drawdown_pct).toFixed(1)}%</span></div>
              <div class="flex justify-between"><span class="text-gray-500">Ann. Return</span><span class="font-mono {parseFloat(row.annualized_return_pct || 0) >= 0 ? 'text-green-400' : 'text-red-400'}">{parseFloat(row.annualized_return_pct || 0).toFixed(1)}%</span></div>
              <div class="flex justify-between"><span class="text-gray-500">Total Trades</span><span class="font-mono text-gray-300">{row.total_trades}</span></div>
              <div class="flex justify-between"><span class="text-gray-500">Profit Factor</span><span class="font-mono text-gray-300">{parseFloat(row.profit_factor).toFixed(2)}</span></div>
              <div class="flex justify-between"><span class="text-gray-500">Avg Trade PnL</span><span class="font-mono {parseFloat(row.avg_trade_pnl) >= 0 ? 'text-green-400' : 'text-red-400'}">{parseFloat(row.avg_trade_pnl).toFixed(4)}</span></div>
            </div>
          </div>

          <!-- AI Description -->
          <div class="p-5 relative">
            <div class="flex items-center justify-between mb-3">
              <h4 class="text-xs text-gray-400 uppercase tracking-wider">AI-Readable Strategy Description</h4>
              <button
                onclick={() => copyDescription(description, i)}
                class="flex items-center gap-1 px-2 py-1 rounded text-xs {copiedIdx === i ? 'bg-green-600 text-white' : 'bg-gray-700 text-gray-400 hover:bg-gray-600 hover:text-white'} transition-colors"
              >
                {#if copiedIdx === i}
                  <Check size={12} /> Copied
                {:else}
                  <Copy size={12} /> Copy
                {/if}
              </button>
            </div>
            <pre class="text-xs text-gray-300 whitespace-pre-wrap font-mono bg-gray-900/50 rounded-lg p-4 max-h-80 overflow-y-auto leading-relaxed">{description}</pre>
          </div>
        </div>
      </div>
    {/each}
  {/if}
</div>
