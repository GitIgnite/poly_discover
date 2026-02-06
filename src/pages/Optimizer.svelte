<script>
  import { onDestroy } from 'svelte';
  import { startOptimization, getOptimizationStatus } from '../lib/api.js';
  import { Sparkles, Play, Loader2 } from 'lucide-svelte';

  // ============================================================================
  // Constants
  // ============================================================================
  const symbols = ['BTCUSDT', 'ETHUSDT', 'SOLUSDT', 'XRPUSDT', 'BNBUSDT', 'DOGEUSDT'];

  // ============================================================================
  // Strategy tab state
  // ============================================================================
  let activeStrategy = $state('rsi');

  // ============================================================================
  // RSI configuration
  // ============================================================================
  let rsiSymbol = $state('BTCUSDT');
  let rsiDays = $state(365);

  // ============================================================================
  // Gabagool configuration
  // ============================================================================
  let gabSymbol = $state('BTCUSDT');
  let gabDays = $state(30);

  // ============================================================================
  // Optimizer state
  // ============================================================================
  let optimizing = $state(false);
  let optimizeProgress = $state(0);
  let optimizeResults = $state([]);
  let optimizeError = $state(null);
  let optimizeStrategy = $state(null);
  let pollInterval = $state(null);

  // ============================================================================
  // Handlers
  // ============================================================================
  async function handleStartOptimize(strategy) {
    optimizeStrategy = strategy;
    optimizing = true;
    optimizeProgress = 0;
    optimizeResults = [];
    optimizeError = null;

    const sym = strategy === 'rsi' ? rsiSymbol : gabSymbol;
    const d = strategy === 'rsi' ? rsiDays : gabDays;

    const res = await startOptimization({ strategy, symbol: sym, days: d, top_n: 2 });
    if (!res.success) {
      optimizeError = res.message;
      optimizing = false;
      return;
    }

    // Poll progress every 500ms
    pollInterval = setInterval(pollOptimizeStatus, 500);
  }

  async function pollOptimizeStatus() {
    const status = await getOptimizationStatus();
    optimizeProgress = status.progress_pct || 0;

    if (status.status === 'complete') {
      optimizeResults = status.results || [];
      optimizing = false;
      clearInterval(pollInterval);
      pollInterval = null;
    } else if (status.status === 'error') {
      optimizeError = status.error || 'Optimization failed';
      optimizing = false;
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }

  function handleApply(strategy, params) {
    console.log(`[Optimizer] Apply ${strategy} params:`, params);
  }

  // ============================================================================
  // Formatters
  // ============================================================================
  function formatPnl(value) {
    const num = parseFloat(value);
    const sign = num >= 0 ? '+' : '';
    return `${sign}$${num.toFixed(2)}`;
  }

  // ============================================================================
  // Cleanup
  // ============================================================================
  onDestroy(() => {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  });
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center gap-3">
    <Sparkles size={28} class="text-amber-400" />
    <div>
      <h2 class="text-2xl font-bold text-white">Parameter Optimizer</h2>
      <p class="text-sm text-gray-400">Find optimal strategy parameters via grid search on historical data</p>
    </div>
  </div>

  <!-- Strategy Selector Tabs -->
  <div class="flex gap-1 bg-gray-800 rounded-lg p-1 w-fit">
    <button
      onclick={() => activeStrategy = 'rsi'}
      class="px-5 py-2 rounded-md text-sm font-medium transition-colors {activeStrategy === 'rsi' ? 'bg-purple-600 text-white' : 'text-gray-400 hover:text-white hover:bg-gray-700'}"
    >
      RSI Strategy
    </button>
    <button
      onclick={() => activeStrategy = 'gabagool'}
      class="px-5 py-2 rounded-md text-sm font-medium transition-colors {activeStrategy === 'gabagool' ? 'bg-orange-600 text-white' : 'text-gray-400 hover:text-white hover:bg-gray-700'}"
    >
      Gabagool Arbitrage
    </button>
  </div>

  <!-- ================================================================== -->
  <!-- RSI Configuration -->
  <!-- ================================================================== -->
  {#if activeStrategy === 'rsi'}
    <div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
      <h3 class="text-lg font-semibold text-purple-400 mb-1">RSI Optimization</h3>
      <p class="text-sm text-gray-400 mb-5">Grid search over ~200 RSI parameter combinations (period, overbought, oversold thresholds)</p>

      <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div>
          <label class="block text-sm text-gray-400 mb-1">Symbol</label>
          <select
            bind:value={rsiSymbol}
            class="w-full bg-gray-900 text-white rounded px-3 py-2 border border-gray-700 focus:border-purple-500 focus:outline-none"
          >
            {#each symbols as s}
              <option value={s}>{s}</option>
            {/each}
          </select>
        </div>
        <div>
          <label class="block text-sm text-gray-400 mb-1">Period (days)</label>
          <input
            type="number"
            bind:value={rsiDays}
            min="1"
            max="365"
            class="w-full bg-gray-900 text-white rounded px-3 py-2 border border-gray-700 focus:border-purple-500 focus:outline-none"
          />
        </div>
      </div>

      <div class="mt-6">
        <button
          onclick={() => handleStartOptimize('rsi')}
          disabled={optimizing}
          class="flex items-center gap-2 px-6 py-3 bg-amber-600 hover:bg-amber-700 disabled:bg-gray-600 disabled:cursor-not-allowed text-white font-semibold rounded-lg transition-colors"
        >
          {#if optimizing && optimizeStrategy === 'rsi'}
            <Loader2 size={20} class="animate-spin" />
            <span>Optimizing... {optimizeProgress.toFixed(0)}%</span>
          {:else}
            <Sparkles size={20} />
            <span>Find Best Parameters</span>
          {/if}
        </button>
      </div>
    </div>

    <!-- RSI Progress Bar -->
    {#if optimizing && optimizeStrategy === 'rsi'}
      <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
        <div class="flex justify-between text-sm text-gray-400 mb-2">
          <span>Grid search in progress ({rsiSymbol}, {rsiDays}d)...</span>
          <span>{optimizeProgress.toFixed(1)}%</span>
        </div>
        <div class="w-full bg-gray-700 rounded-full h-3">
          <div
            class="bg-amber-500 h-3 rounded-full transition-all duration-300"
            style="width: {optimizeProgress}%"
          ></div>
        </div>
      </div>
    {/if}

    <!-- RSI Error -->
    {#if optimizeError && optimizeStrategy === 'rsi'}
      <div class="bg-red-900/30 border border-red-700 rounded-lg p-4 text-red-300">{optimizeError}</div>
    {/if}

    <!-- RSI Results -->
    {#if optimizeResults.length > 0 && optimizeStrategy === 'rsi'}
      <div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
        <h3 class="text-lg font-semibold text-amber-400 mb-4 flex items-center gap-2">
          <Sparkles size={20} />
          Top {optimizeResults.length} RSI Configurations
        </h3>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          {#each optimizeResults as r}
            <div class="bg-gray-900/60 border border-amber-600/30 rounded-lg p-4">
              <!-- Header: Rank + Score -->
              <div class="flex items-center justify-between mb-3">
                <span class="text-amber-400 font-bold text-lg">#{r.rank}</span>
                <span class="text-xs text-gray-400">Score: {parseFloat(r.composite_score).toFixed(1)}</span>
              </div>

              <!-- Parameters -->
              <div class="grid grid-cols-3 gap-2 mb-3 text-sm">
                <div>
                  <span class="text-gray-400">Period:</span>
                  <span class="text-white font-bold ml-1">{r.params.rsi_period}</span>
                </div>
                <div>
                  <span class="text-gray-400">OB:</span>
                  <span class="text-white font-bold ml-1">{r.params.rsi_overbought}</span>
                </div>
                <div>
                  <span class="text-gray-400">OS:</span>
                  <span class="text-white font-bold ml-1">{r.params.rsi_oversold}</span>
                </div>
              </div>

              <!-- Metrics -->
              <div class="grid grid-cols-2 gap-2 text-sm">
                <div>
                  <span class="text-gray-400">Net P&L:</span>
                  <span class="{parseFloat(r.net_pnl) >= 0 ? 'text-green-400' : 'text-red-400'} ml-1">{formatPnl(r.net_pnl)}</span>
                </div>
                <div>
                  <span class="text-gray-400">Fees:</span>
                  <span class="text-orange-400 ml-1">${parseFloat(r.total_fees).toFixed(2)}</span>
                </div>
                <div>
                  <span class="text-gray-400">Win Rate:</span>
                  <span class="text-white ml-1">{parseFloat(r.win_rate).toFixed(1)}%</span>
                </div>
                <div>
                  <span class="text-gray-400">Sharpe:</span>
                  <span class="text-white ml-1">{parseFloat(r.sharpe_ratio).toFixed(2)}</span>
                </div>
                <div>
                  <span class="text-gray-400">Drawdown:</span>
                  <span class="text-red-400 ml-1">{parseFloat(r.max_drawdown_pct).toFixed(1)}%</span>
                </div>
                <div>
                  <span class="text-gray-400">Trades:</span>
                  <span class="text-white ml-1">{r.total_trades}</span>
                </div>
              </div>

              <!-- Apply Button -->
              <button
                onclick={() => handleApply('rsi', r.params)}
                class="mt-3 w-full py-2 bg-amber-600/20 hover:bg-amber-600/40 text-amber-400 rounded text-sm transition-colors"
              >
                Apply these parameters
              </button>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}

  <!-- ================================================================== -->
  <!-- Gabagool Configuration -->
  <!-- ================================================================== -->
  {#if activeStrategy === 'gabagool'}
    <div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
      <h3 class="text-lg font-semibold text-orange-400 mb-1">Gabagool Optimization</h3>
      <p class="text-sm text-gray-400 mb-5">Grid search over ~240 Gabagool parameter combinations (max_pair_cost, bid_offset, spread_multiplier)</p>

      <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div>
          <label class="block text-sm text-gray-400 mb-1">Symbol</label>
          <select
            bind:value={gabSymbol}
            class="w-full bg-gray-900 text-white rounded px-3 py-2 border border-gray-700 focus:border-orange-500 focus:outline-none"
          >
            {#each symbols as s}
              <option value={s}>{s}</option>
            {/each}
          </select>
        </div>
        <div>
          <label class="block text-sm text-gray-400 mb-1">Period (days)</label>
          <input
            type="number"
            bind:value={gabDays}
            min="1"
            max="365"
            class="w-full bg-gray-900 text-white rounded px-3 py-2 border border-gray-700 focus:border-orange-500 focus:outline-none"
          />
        </div>
      </div>

      <div class="mt-6">
        <button
          onclick={() => handleStartOptimize('gabagool')}
          disabled={optimizing}
          class="flex items-center gap-2 px-6 py-3 bg-amber-600 hover:bg-amber-700 disabled:bg-gray-600 disabled:cursor-not-allowed text-white font-semibold rounded-lg transition-colors"
        >
          {#if optimizing && optimizeStrategy === 'gabagool'}
            <Loader2 size={20} class="animate-spin" />
            <span>Optimizing... {optimizeProgress.toFixed(0)}%</span>
          {:else}
            <Sparkles size={20} />
            <span>Find Best Parameters</span>
          {/if}
        </button>
      </div>
    </div>

    <!-- Gabagool Progress Bar -->
    {#if optimizing && optimizeStrategy === 'gabagool'}
      <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
        <div class="flex justify-between text-sm text-gray-400 mb-2">
          <span>Grid search in progress ({gabSymbol}, {gabDays}d)...</span>
          <span>{optimizeProgress.toFixed(1)}%</span>
        </div>
        <div class="w-full bg-gray-700 rounded-full h-3">
          <div
            class="bg-amber-500 h-3 rounded-full transition-all duration-300"
            style="width: {optimizeProgress}%"
          ></div>
        </div>
      </div>
    {/if}

    <!-- Gabagool Error -->
    {#if optimizeError && optimizeStrategy === 'gabagool'}
      <div class="bg-red-900/30 border border-red-700 rounded-lg p-4 text-red-300">{optimizeError}</div>
    {/if}

    <!-- Gabagool Results -->
    {#if optimizeResults.length > 0 && optimizeStrategy === 'gabagool'}
      <div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
        <h3 class="text-lg font-semibold text-amber-400 mb-4 flex items-center gap-2">
          <Sparkles size={20} />
          Top {optimizeResults.length} Gabagool Configurations
        </h3>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          {#each optimizeResults as r}
            <div class="bg-gray-900/60 border border-amber-600/30 rounded-lg p-4">
              <!-- Header: Rank + Score -->
              <div class="flex items-center justify-between mb-3">
                <span class="text-amber-400 font-bold text-lg">#{r.rank}</span>
                <span class="text-xs text-gray-400">Score: {parseFloat(r.composite_score).toFixed(1)}</span>
              </div>

              <!-- Parameters -->
              <div class="grid grid-cols-3 gap-2 mb-3 text-sm">
                <div>
                  <span class="text-gray-400">Max Cost:</span>
                  <span class="text-white font-bold ml-1">{r.params.max_pair_cost}</span>
                </div>
                <div>
                  <span class="text-gray-400">Offset:</span>
                  <span class="text-white font-bold ml-1">{r.params.bid_offset}</span>
                </div>
                <div>
                  <span class="text-gray-400">Spread x:</span>
                  <span class="text-white font-bold ml-1">{r.params.spread_multiplier}</span>
                </div>
              </div>

              <!-- Metrics -->
              <div class="grid grid-cols-2 gap-2 text-sm">
                <div>
                  <span class="text-gray-400">Net P&L:</span>
                  <span class="{parseFloat(r.net_pnl) >= 0 ? 'text-green-400' : 'text-red-400'} ml-1">${parseFloat(r.net_pnl).toFixed(4)}</span>
                </div>
                <div>
                  <span class="text-gray-400">Fees:</span>
                  <span class="text-orange-400 ml-1">${parseFloat(r.total_fees).toFixed(4)}</span>
                </div>
                <div>
                  <span class="text-gray-400">Hit Rate:</span>
                  <span class="text-white ml-1">{r.hit_rate != null ? parseFloat(r.hit_rate).toFixed(1) : '-'}%</span>
                </div>
                <div>
                  <span class="text-gray-400">Avg Profit:</span>
                  <span class="text-green-400 ml-1">${r.avg_locked_profit != null ? parseFloat(r.avg_locked_profit).toFixed(4) : '-'}</span>
                </div>
                <div>
                  <span class="text-gray-400">Gross P&L:</span>
                  <span class="text-white ml-1">${parseFloat(r.gross_pnl).toFixed(4)}</span>
                </div>
                <div>
                  <span class="text-gray-400">Windows:</span>
                  <span class="text-white ml-1">{r.total_trades}</span>
                </div>
              </div>

              <!-- Apply Button -->
              <button
                onclick={() => handleApply('gabagool', r.params)}
                class="mt-3 w-full py-2 bg-amber-600/20 hover:bg-amber-600/40 text-amber-400 rounded text-sm transition-colors"
              >
                Apply these parameters
              </button>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>
