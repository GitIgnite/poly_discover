<script>
  import { startDiscovery, cancelDiscovery } from '../lib/api.js';
  import { serverHealth, discoveryStatus } from '../lib/stores.js';
  import { Search, Sparkles, Loader2, Zap, Trophy, Square } from 'lucide-svelte';

  // ============================================================================
  // Configuration state
  // ============================================================================
  let discoverSymbols = $state({ BTCUSDT: true, ETHUSDT: true, SOLUSDT: true, XRPUSDT: true });
  let discoverDays = $state(90);
  let discoverError = $state(null);

  // ============================================================================
  // Helpers
  // ============================================================================
  function getStrategyColor(name) {
    const colors = {
      'RSI': 'text-purple-400',
      'Bollinger Bands': 'text-blue-400',
      'MACD': 'text-cyan-400',
      'EMA Crossover': 'text-emerald-400',
      'Stochastic': 'text-yellow-400',
      'ATR Mean Reversion': 'text-pink-400',
      'Gabagool': 'text-orange-400',
      'VWAP': 'text-teal-400',
      'OBV': 'text-lime-400',
      'Williams': 'text-rose-400',
      'ADX': 'text-indigo-400',
    };
    for (const [key, color] of Object.entries(colors)) {
      if (name.includes(key) || name.includes(key.split(' ')[0])) return color;
    }
    return 'text-cyan-400';
  }

  function formatDiscoveryParams(result) {
    const st = result.strategy_type;
    if (!st) return '';
    switch (st.type) {
      case 'rsi': return `Period=${st.period} OB=${st.overbought} OS=${st.oversold}`;
      case 'bollinger_bands': return `Period=${st.period} Mult=${st.multiplier}`;
      case 'macd': return `Fast=${st.fast} Slow=${st.slow} Sig=${st.signal}`;
      case 'ema_crossover': return `Fast=${st.fast_period} Slow=${st.slow_period}`;
      case 'stochastic': return `Period=${st.period} OB=${st.overbought} OS=${st.oversold}`;
      case 'atr_mean_reversion': return `ATR=${st.atr_period} SMA=${st.sma_period} Mult=${st.multiplier}`;
      case 'gabagool': return `MaxCost=${st.max_pair_cost} Offset=${st.bid_offset} SpreadX=${st.spread_multiplier}`;
      case 'rsi_bollinger': return `RSI(${st.rsi_period},${st.rsi_ob},${st.rsi_os}) BB(${st.bb_period},${st.bb_mult})`;
      case 'macd_rsi': return `MACD(${st.macd_fast},${st.macd_slow}) RSI(${st.rsi_period})`;
      case 'ema_rsi': return `EMA(${st.ema_fast},${st.ema_slow}) RSI(${st.rsi_period})`;
      case 'stoch_rsi': return `Stoch(${st.stoch_period}) RSI(${st.rsi_period})`;
      case 'macd_bollinger': return `MACD(${st.macd_fast},${st.macd_slow}) BB(${st.bb_period})`;
      case 'triple_rsi_macd_bb': return `RSI(${st.rsi_period}) MACD(${st.macd_slow}) BB(${st.bb_period})`;
      case 'triple_ema_rsi_stoch': return `EMA(${st.ema_fast}) RSI(${st.rsi_period}) Stoch(${st.stoch_period})`;
      case 'vwap': return `Period=${st.period}`;
      case 'obv': return `SMA=${st.sma_period}`;
      case 'williams_r': return `Period=${st.period} OB=${st.overbought} OS=${st.oversold}`;
      case 'adx': return `Period=${st.period} Threshold=${st.adx_threshold}`;
      case 'vwap_rsi': return `VWAP(${st.vwap_period}) RSI(${st.rsi_period})`;
      case 'obv_macd': return `OBV(${st.obv_sma_period}) MACD(${st.macd_fast},${st.macd_slow})`;
      case 'adx_ema': return `ADX(${st.adx_period},${st.adx_threshold}) EMA(${st.ema_fast},${st.ema_slow})`;
      case 'williams_r_stoch': return `WR(${st.wr_period}) Stoch(${st.stoch_period})`;
      default: return JSON.stringify(st);
    }
  }

  function formatPnl(value) {
    const num = parseFloat(value);
    const sign = num >= 0 ? '+' : '';
    return `${sign}$${num.toFixed(2)}`;
  }

  // ============================================================================
  // Discovery actions
  // ============================================================================
  async function handleStart() {
    const selectedSymbols = Object.entries(discoverSymbols)
      .filter(([_, v]) => v)
      .map(([k]) => k);

    if (selectedSymbols.length === 0) {
      discoverError = 'Select at least one symbol';
      return;
    }

    discoverError = null;

    const res = await startDiscovery({
      symbols: selectedSymbols,
      days: discoverDays,
    });

    if (!res.success) {
      discoverError = res.message;
    }
  }

  async function handleStop() {
    discoverError = null;
    await cancelDiscovery();
  }
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center gap-3">
    <Search size={28} class="text-cyan-400" />
    <div>
      <h2 class="text-2xl font-bold text-white">Discovery Agent</h2>
      <p class="text-sm text-gray-400">ML-guided autonomous agent — tests thousands of strategy combinations and learns from results</p>
    </div>
  </div>

  <!-- Configuration + Start/Stop -->
  <div class="bg-gray-800 rounded-lg p-6">
    <div class="flex items-center justify-between mb-5">
      <h3 class="text-lg font-semibold text-white">Configuration</h3>
      {#if $discoveryStatus.running}
        <div class="flex items-center gap-2 text-sm text-cyan-400">
          <div class="w-2 h-2 rounded-full bg-cyan-400 animate-pulse"></div>
          Running continuously
        </div>
      {/if}
    </div>

    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
      <!-- Symbols selection -->
      <div>
        <label class="block text-sm text-gray-400 mb-2">Symbols</label>
        <div class="flex flex-wrap gap-3">
          {#each ['BTCUSDT', 'ETHUSDT', 'SOLUSDT', 'XRPUSDT'] as sym}
            <label class="flex items-center gap-2 bg-gray-700 px-3 py-2 rounded-lg cursor-pointer hover:bg-gray-600 transition-colors {$discoveryStatus.running ? 'opacity-50 pointer-events-none' : ''}">
              <input type="checkbox" bind:checked={discoverSymbols[sym]} class="accent-cyan-500 w-4 h-4" disabled={$discoveryStatus.running} />
              <span class="text-white text-sm font-medium">{sym.replace('USDT', '')}</span>
            </label>
          {/each}
        </div>
      </div>

      <!-- Days parameter -->
      <div>
        <label class="block text-sm text-gray-400 mb-1">Days of data</label>
        <input type="number" bind:value={discoverDays} min="7" max="365" disabled={$discoveryStatus.running} class="w-32 bg-gray-700 text-white rounded px-3 py-2 border border-gray-600 focus:border-cyan-500 focus:outline-none disabled:opacity-50" />
      </div>
    </div>

    <div class="mt-6">
      {#if $discoveryStatus.running}
        <button
          onclick={handleStop}
          class="flex items-center gap-2 px-6 py-3 bg-red-600 hover:bg-red-700 text-white font-semibold rounded-lg transition-colors"
        >
          <Square size={20} />
          <span>Stop Discovery</span>
        </button>
      {:else}
        <button
          onclick={handleStart}
          disabled={!$serverHealth.connected}
          class="flex items-center gap-2 px-6 py-3 bg-emerald-600 hover:bg-emerald-700 disabled:bg-gray-600 disabled:cursor-not-allowed text-white font-semibold rounded-lg transition-colors"
        >
          <Sparkles size={20} />
          <span>Start Discovery</span>
        </button>
      {/if}
    </div>
  </div>

  <!-- Progress Panel -->
  {#if $discoveryStatus.running}
    <div class="bg-gray-800 rounded-lg p-5">
      <!-- Cycle + Phase info -->
      <div class="flex justify-between items-center mb-2">
        <div>
          <span class="text-sm text-cyan-400 font-medium">{$discoveryStatus.phase}</span>
          {#if $discoveryStatus.current_strategy}
            <span class="text-sm text-gray-400 ml-2">
              {$discoveryStatus.current_strategy} / {$discoveryStatus.current_symbol}
            </span>
          {/if}
        </div>
        <div class="text-right">
          <span class="text-sm text-white font-medium">{$discoveryStatus.progress_pct.toFixed(1)}%</span>
          <span class="text-xs text-gray-500 ml-2">{$discoveryStatus.completed}/{$discoveryStatus.total}</span>
          {#if $discoveryStatus.skipped > 0}
            <span class="text-xs text-emerald-400 ml-2">({$discoveryStatus.skipped} cached)</span>
          {/if}
        </div>
      </div>

      <!-- Progress bar -->
      <div class="w-full bg-gray-700 rounded-full h-3">
        <div
          class="bg-gradient-to-r from-cyan-600 to-cyan-400 h-3 rounded-full transition-all duration-300"
          style="width: {Math.min($discoveryStatus.progress_pct, 100)}%"
        ></div>
      </div>

      <!-- Global stats -->
      <div class="mt-3 flex items-center gap-6 text-sm">
        <span class="text-gray-400">Cycle <span class="text-white font-bold">{$discoveryStatus.current_cycle}</span></span>
        <span class="text-gray-400">Total: <span class="text-white font-bold">{$discoveryStatus.total_tested_all_cycles.toLocaleString()}</span> tested</span>
        <span class="text-gray-400">New this cycle: <span class="text-cyan-400 font-bold">{$discoveryStatus.total_new_this_cycle.toLocaleString()}</span></span>
      </div>

      <!-- Live Top 3 -->
      {#if $discoveryStatus.best_so_far.length > 0}
        <div class="mt-4">
          <h4 class="text-sm text-gray-400 mb-2 flex items-center gap-1">
            <Zap size={14} class="text-yellow-400" />
            Live Top {Math.min(3, $discoveryStatus.best_so_far.length)}
          </h4>
          <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
            {#each $discoveryStatus.best_so_far.slice(0, 3) as r, i}
              <div class="bg-gray-700/50 border border-gray-600 rounded-lg p-3">
                <div class="flex items-center justify-between mb-1">
                  <span class="text-xs font-bold {getStrategyColor(r.strategy_name)}">{r.strategy_name}</span>
                  <span class="text-xs text-gray-500">{r.symbol.replace('USDT', '')}</span>
                </div>
                <div class="flex items-center gap-3 text-sm">
                  <span class="{parseFloat(r.net_pnl) >= 0 ? 'text-green-400' : 'text-red-400'} font-bold">{formatPnl(r.net_pnl)}</span>
                  <span class="text-gray-400">{parseFloat(r.win_rate).toFixed(0)}% WR</span>
                  <span class="text-gray-500">{r.total_trades}T</span>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <!-- Error -->
  {#if discoverError}
    <div class="bg-red-900/30 border border-red-700 rounded-lg p-4 text-red-300">{discoverError}</div>
  {/if}

  <!-- Results when idle (show last results) -->
  {#if !$discoveryStatus.running && $discoveryStatus.results.length > 0}
    <div class="bg-gray-800 rounded-lg p-6">
      <h3 class="text-lg font-semibold text-cyan-400 mb-4 flex items-center gap-2">
        <Trophy size={20} />
        Best Strategies — Top {Math.min(10, $discoveryStatus.results.length)}
      </h3>

      <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
        {#each $discoveryStatus.results.slice(0, 10) as r}
          <div class="bg-gray-700/50 border rounded-lg p-4 {r.rank <= 3 ? 'border-cyan-600/50' : 'border-gray-600/30'}">
            <!-- Header -->
            <div class="flex items-center justify-between mb-3">
              <div class="flex items-center gap-2">
                <span class="text-lg font-bold {r.rank === 1 ? 'text-yellow-400' : r.rank === 2 ? 'text-gray-300' : r.rank === 3 ? 'text-amber-600' : 'text-gray-500'}">#{r.rank}</span>
                <span class="font-semibold {getStrategyColor(r.strategy_name)}">{r.strategy_name}</span>
              </div>
              <div class="flex items-center gap-2">
                <span class="text-xs bg-gray-600 text-gray-300 px-2 py-0.5 rounded">{r.symbol.replace('USDT', '')}</span>
                <span class="text-xs text-gray-500">Score: {parseFloat(r.composite_score).toFixed(0)}</span>
              </div>
            </div>

            <!-- Parameters -->
            <div class="text-xs text-gray-400 bg-gray-800/50 px-3 py-1.5 rounded mb-3 font-mono">
              {formatDiscoveryParams(r)}
            </div>

            <!-- Metrics grid -->
            <div class="grid grid-cols-3 gap-x-4 gap-y-1 text-sm">
              <div><span class="text-gray-400">Net P&L:</span> <span class="font-bold {parseFloat(r.net_pnl) >= 0 ? 'text-green-400' : 'text-red-400'}">{formatPnl(r.net_pnl)}</span></div>
              <div><span class="text-gray-400">Win Rate:</span> <span class="font-bold text-white">{parseFloat(r.win_rate).toFixed(1)}%</span></div>
              <div><span class="text-gray-400">Trades:</span> <span class="text-white">{r.total_trades}</span></div>
              <div><span class="text-gray-400">Sharpe:</span> <span class="text-white">{parseFloat(r.sharpe_ratio).toFixed(2)}</span></div>
              <div><span class="text-gray-400">Drawdown:</span> <span class="text-red-400">{parseFloat(r.max_drawdown_pct).toFixed(1)}%</span></div>
              <div><span class="text-gray-400">PF:</span> <span class="{parseFloat(r.profit_factor) >= 1 ? 'text-green-400' : 'text-red-400'}">{parseFloat(r.profit_factor).toFixed(2)}</span></div>
            </div>

            {#if r.hit_rate != null}
              <div class="mt-2 pt-2 border-t border-gray-600/30 grid grid-cols-2 gap-2 text-sm">
                <div><span class="text-gray-400">Hit Rate:</span> <span class="text-orange-400">{parseFloat(r.hit_rate).toFixed(1)}%</span></div>
                <div><span class="text-gray-400">Avg Locked:</span> <span class="text-green-400">${r.avg_locked_profit != null ? parseFloat(r.avg_locked_profit).toFixed(4) : '-'}</span></div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
