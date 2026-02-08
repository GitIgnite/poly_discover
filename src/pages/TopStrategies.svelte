<script>
  import { onDestroy } from 'svelte';
  import { getTopStrategies } from '../lib/api.js';
  import { discoveryStatus } from '../lib/stores.js';
  import { Trophy, Loader2 } from 'lucide-svelte';

  // ============================================================================
  // State
  // ============================================================================
  let strategies = $state([]);
  let loading = $state(false);
  let autoRefreshInterval = $state(null);

  // ============================================================================
  // Data loading
  // ============================================================================
  async function loadTopStrategies() {
    loading = true;
    const res = await getTopStrategies(20);
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
      autoRefreshInterval = setInterval(loadTopStrategies, 60000);
    } else if (!status.running && autoRefreshInterval) {
      clearInterval(autoRefreshInterval);
      autoRefreshInterval = null;
      loadTopStrategies();
    }
  });

  onDestroy(() => {
    unsubscribe();
    if (autoRefreshInterval) {
      clearInterval(autoRefreshInterval);
    }
  });

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

  function getRankStyle(rank) {
    if (rank === 1) return { medal: '#FFD700', bg: 'bg-yellow-900/30 border-yellow-600/50', label: '1st' };
    if (rank === 2) return { medal: '#C0C0C0', bg: 'bg-gray-600/20 border-gray-500/50', label: '2nd' };
    if (rank === 3) return { medal: '#CD7F32', bg: 'bg-orange-900/20 border-orange-700/50', label: '3rd' };
    return null;
  }

  // ============================================================================
  // Load on mount
  // ============================================================================
  loadTopStrategies();
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center gap-3">
    <Trophy size={28} class="text-yellow-400" />
    <div>
      <h2 class="text-2xl font-bold text-white">Top 20 Strategies</h2>
      <p class="text-sm text-gray-400">Best unique strategy by win rate — one entry per strategy type</p>
    </div>
    {#if $discoveryStatus.running}
      <div class="ml-auto flex items-center gap-2 px-3 py-1 bg-cyan-900/40 border border-cyan-700/50 rounded-full">
        <div class="w-2 h-2 rounded-full bg-cyan-400 animate-pulse"></div>
        <span class="text-xs text-cyan-400 font-semibold uppercase tracking-wider">LIVE</span>
      </div>
    {/if}
  </div>

  <!-- Podium for Top 3 -->
  {#if strategies.length >= 3}
    <div class="grid grid-cols-3 gap-4">
      {#each [1, 0, 2] as podiumIdx}
        {@const row = strategies[podiumIdx]}
        {@const rank = podiumIdx + 1}
        {@const style = getRankStyle(rank)}
        {@const wr = parseFloat(row.win_rate)}
        {@const pnl = parseFloat(row.net_pnl)}
        {@const confVal = parseFloat(row.strategy_confidence || 0)}
        <div class="rounded-lg p-4 border {style.bg} {rank === 1 ? 'order-2 scale-105' : rank === 2 ? 'order-1' : 'order-3'}">
          <div class="flex items-center gap-2 mb-2">
            <span class="text-2xl font-black" style="color: {style.medal}">{style.label}</span>
            <Trophy size={rank === 1 ? 24 : 18} style="color: {style.medal}" />
          </div>
          <div class="text-lg font-bold {getStrategyColor(row.strategy_name)}">{row.strategy_name}</div>
          <div class="text-xs text-gray-400 mb-3">{row.symbol}</div>
          <div class="grid grid-cols-2 gap-2 text-sm">
            <div>
              <span class="text-gray-500 text-xs">Win Rate</span>
              <div class="font-mono font-bold {wr >= 60 ? 'text-yellow-400' : 'text-gray-300'}">{wr.toFixed(1)}%</div>
            </div>
            <div>
              <span class="text-gray-500 text-xs">Net PnL</span>
              <div class="font-mono font-bold {pnl >= 0 ? 'text-green-400' : 'text-red-400'}">{pnl.toFixed(2)}</div>
            </div>
            <div>
              <span class="text-gray-500 text-xs">Sharpe</span>
              <div class="font-mono text-gray-300">{parseFloat(row.sharpe_ratio).toFixed(2)}</div>
            </div>
            <div>
              <span class="text-gray-500 text-xs">Confidence</span>
              <div class="font-mono {confVal >= 70 ? 'text-green-400' : confVal >= 40 ? 'text-yellow-400' : confVal > 0 ? 'text-red-400' : 'text-gray-600'}">{confVal > 0 ? confVal.toFixed(0) + '%' : '-'}</div>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Results Table -->
  {#if loading}
    <div class="flex justify-center py-12">
      <Loader2 class="w-8 h-8 text-yellow-400 animate-spin" />
    </div>
  {:else if strategies.length === 0}
    <div class="text-center text-gray-500 py-12">
      <Trophy class="w-12 h-12 mx-auto mb-3 opacity-50" />
      <p>No strategies in the knowledge base yet.</p>
      <p class="text-sm mt-1">Run a Discovery Agent scan to populate it.</p>
    </div>
  {:else}
    <div class="bg-gray-800 rounded-lg overflow-hidden">
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-gray-400 text-xs uppercase border-b border-gray-700">
              <th class="px-3 py-2 text-left">Rank</th>
              <th class="px-3 py-2 text-left">Strategy</th>
              <th class="px-3 py-2 text-left">Symbol</th>
              <th class="px-3 py-2 text-right">Win Rate</th>
              <th class="px-3 py-2 text-right">Net PnL</th>
              <th class="px-3 py-2 text-right">Conf.</th>
              <th class="px-3 py-2 text-right">Ann. Ret.</th>
              <th class="px-3 py-2 text-right">Sortino</th>
              <th class="px-3 py-2 text-right">Sharpe</th>
              <th class="px-3 py-2 text-right">Drawdown</th>
              <th class="px-3 py-2 text-right">Trades</th>
              <th class="px-3 py-2 text-left">Params</th>
            </tr>
          </thead>
          <tbody>
            {#each strategies as row, i}
              {@const rank = i + 1}
              {@const style = getRankStyle(rank)}
              {@const confVal = parseFloat(row.strategy_confidence || 0)}
              {@const confColor = confVal >= 70 ? 'text-green-400' : confVal >= 40 ? 'text-yellow-400' : confVal > 0 ? 'text-red-400' : 'text-gray-600'}
              {@const confBg = confVal >= 70 ? 'bg-green-500' : confVal >= 40 ? 'bg-yellow-500' : 'bg-red-500'}
              <tr class="border-b border-gray-700/50 hover:bg-gray-700/30 {style ? style.bg : ''}">
                <td class="px-3 py-2">
                  {#if style}
                    <span class="font-bold" style="color: {style.medal}">{rank}</span>
                  {:else}
                    <span class="text-gray-500">{rank}</span>
                  {/if}
                </td>
                <td class="px-3 py-2 font-medium {getStrategyColor(row.strategy_name)}">{row.strategy_name}</td>
                <td class="px-3 py-2 text-gray-300">{row.symbol}</td>
                <td class="px-3 py-2 text-right font-mono font-bold {parseFloat(row.win_rate) >= 60 ? 'text-yellow-400' : 'text-gray-300'}">{parseFloat(row.win_rate).toFixed(1)}%</td>
                <td class="px-3 py-2 text-right font-mono {parseFloat(row.net_pnl) >= 0 ? 'text-green-400' : 'text-red-400'}">{parseFloat(row.net_pnl).toFixed(2)}</td>
                <td class="px-3 py-2 text-right">
                  {#if confVal > 0}
                    <div class="flex items-center justify-end gap-1">
                      <div class="w-8 bg-gray-700 rounded-full h-1.5">
                        <div class="{confBg} h-1.5 rounded-full" style="width: {confVal}%"></div>
                      </div>
                      <span class="font-mono text-xs {confColor}">{confVal.toFixed(0)}%</span>
                    </div>
                  {:else}
                    <span class="text-gray-600 text-xs">-</span>
                  {/if}
                </td>
                <td class="px-3 py-2 text-right font-mono {parseFloat(row.annualized_return_pct || 0) >= 0 ? 'text-green-400' : 'text-red-400'}">{parseFloat(row.annualized_return_pct || 0).toFixed(1)}%</td>
                <td class="px-3 py-2 text-right font-mono text-gray-300">{parseFloat(row.sortino_ratio || 0).toFixed(2)}</td>
                <td class="px-3 py-2 text-right font-mono text-gray-300">{parseFloat(row.sharpe_ratio).toFixed(2)}</td>
                <td class="px-3 py-2 text-right font-mono text-red-400">{parseFloat(row.max_drawdown_pct).toFixed(1)}%</td>
                <td class="px-3 py-2 text-right text-gray-300">{row.total_trades}</td>
                <td class="px-3 py-2 text-xs text-gray-500 max-w-xs truncate" title={row.strategy_params}>{row.strategy_params}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}
</div>
