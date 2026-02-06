<script>
  import { getKnowledgeBase, getKnowledgeStats, exportResults } from '../lib/api.js';
  import { Database, Download, Loader2 } from 'lucide-svelte';

  // ============================================================================
  // State
  // ============================================================================
  let kbData = $state([]);
  let kbTotal = $state(0);
  let kbStats = $state(null);
  let kbLoading = $state(false);
  let kbPage = $state(0);
  let kbPageSize = $state(20);
  let kbFilterStrategy = $state('');
  let kbFilterSymbol = $state('');
  let kbFilterMinWR = $state('');
  let kbSortBy = $state('composite_score');

  // ============================================================================
  // Data loading
  // ============================================================================
  async function loadKnowledgeBase() {
    kbLoading = true;
    const params = {
      limit: kbPageSize,
      offset: kbPage * kbPageSize,
      sort_by: kbSortBy,
    };
    if (kbFilterStrategy) params.strategy_type = kbFilterStrategy;
    if (kbFilterSymbol) params.symbol = kbFilterSymbol;
    if (kbFilterMinWR) params.min_win_rate = kbFilterMinWR;

    const [dataRes, statsRes] = await Promise.all([
      getKnowledgeBase(params),
      getKnowledgeStats(),
    ]);

    if (dataRes.success !== false) {
      kbData = dataRes.data || [];
      kbTotal = dataRes.total || 0;
    }
    if (statsRes.success !== false && statsRes.stats) {
      kbStats = statsRes.stats;
    }
    kbLoading = false;
  }

  // ============================================================================
  // Pagination
  // ============================================================================
  function kbPrevPage() {
    if (kbPage > 0) { kbPage--; loadKnowledgeBase(); }
  }
  function kbNextPage() {
    if ((kbPage + 1) * kbPageSize < kbTotal) { kbPage++; loadKnowledgeBase(); }
  }

  // ============================================================================
  // Export
  // ============================================================================
  async function handleExportJSON() {
    const result = await exportResults({
      top_n: 20,
      min_win_rate: kbFilterMinWR || undefined,
    });
    if (result.results) {
      const blob = new Blob([JSON.stringify(result, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `knowledge-base-export-${new Date().toISOString().slice(0,10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
    }
  }

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
    };
    for (const [key, color] of Object.entries(colors)) {
      if (name.includes(key) || name.includes(key.split(' ')[0])) return color;
    }
    return 'text-cyan-400';
  }

  // ============================================================================
  // Load on mount
  // ============================================================================
  loadKnowledgeBase();
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center gap-3">
    <Database size={28} class="text-emerald-400" />
    <div>
      <h2 class="text-2xl font-bold text-white">Knowledge Base</h2>
      <p class="text-sm text-gray-400">Persisted discovery backtest results with filtering and sorting</p>
    </div>
  </div>

  <!-- Stats Header -->
  {#if kbStats}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <div class="bg-gray-800 rounded-lg p-4">
        <div class="text-xs text-gray-400 uppercase">Total Backtests</div>
        <div class="text-2xl font-bold text-emerald-400">{kbStats.total_backtests.toLocaleString()}</div>
      </div>
      <div class="bg-gray-800 rounded-lg p-4">
        <div class="text-xs text-gray-400 uppercase">Unique Strategies</div>
        <div class="text-2xl font-bold text-cyan-400">{kbStats.unique_strategies}</div>
      </div>
      <div class="bg-gray-800 rounded-lg p-4">
        <div class="text-xs text-gray-400 uppercase">Best Win Rate</div>
        <div class="text-2xl font-bold text-yellow-400">{parseFloat(kbStats.best_win_rate).toFixed(1)}%</div>
        <div class="text-xs text-gray-500">{kbStats.best_strategy_name}</div>
      </div>
      <div class="bg-gray-800 rounded-lg p-4">
        <div class="text-xs text-gray-400 uppercase">Best Net PnL</div>
        <div class="text-2xl font-bold {parseFloat(kbStats.best_net_pnl) >= 0 ? 'text-green-400' : 'text-red-400'}">{parseFloat(kbStats.best_net_pnl).toFixed(2)} USDC</div>
      </div>
    </div>
  {/if}

  <!-- Filters -->
  <div class="bg-gray-800 rounded-lg p-4">
    <div class="flex flex-wrap gap-4 items-end">
      <div>
        <label class="block text-xs text-gray-400 mb-1">Strategy</label>
        <select bind:value={kbFilterStrategy} onchange={() => { kbPage = 0; loadKnowledgeBase(); }} class="bg-gray-700 text-white rounded px-3 py-1.5 text-sm border border-gray-600 focus:border-emerald-500 focus:outline-none">
          <option value="">All</option>
          <option value="rsi">RSI</option>
          <option value="bollinger_bands">Bollinger</option>
          <option value="macd">MACD</option>
          <option value="ema_crossover">EMA Cross</option>
          <option value="stochastic">Stochastic</option>
          <option value="atr_mean_reversion">ATR MeanRev</option>
          <option value="rsi_bollinger">RSI+BB</option>
          <option value="macd_rsi">MACD+RSI</option>
          <option value="ema_rsi">EMA+RSI</option>
          <option value="stoch_rsi">Stoch+RSI</option>
          <option value="macd_bollinger">MACD+BB</option>
          <option value="triple_rsi_macd_bb">Triple RSI+MACD+BB</option>
          <option value="triple_ema_rsi_stoch">Triple EMA+RSI+Stoch</option>
          <option value="gabagool">Gabagool</option>
        </select>
      </div>
      <div>
        <label class="block text-xs text-gray-400 mb-1">Symbol</label>
        <select bind:value={kbFilterSymbol} onchange={() => { kbPage = 0; loadKnowledgeBase(); }} class="bg-gray-700 text-white rounded px-3 py-1.5 text-sm border border-gray-600 focus:border-emerald-500 focus:outline-none">
          <option value="">All</option>
          <option value="BTCUSDT">BTCUSDT</option>
          <option value="ETHUSDT">ETHUSDT</option>
          <option value="SOLUSDT">SOLUSDT</option>
          <option value="XRPUSDT">XRPUSDT</option>
        </select>
      </div>
      <div>
        <label class="block text-xs text-gray-400 mb-1">Min Win Rate</label>
        <input type="number" bind:value={kbFilterMinWR} onchange={() => { kbPage = 0; loadKnowledgeBase(); }} placeholder="e.g. 60" class="bg-gray-700 text-white rounded px-3 py-1.5 text-sm w-24 border border-gray-600 focus:border-emerald-500 focus:outline-none" />
      </div>
      <div>
        <label class="block text-xs text-gray-400 mb-1">Sort By</label>
        <select bind:value={kbSortBy} onchange={() => { kbPage = 0; loadKnowledgeBase(); }} class="bg-gray-700 text-white rounded px-3 py-1.5 text-sm border border-gray-600 focus:border-emerald-500 focus:outline-none">
          <option value="composite_score">Score</option>
          <option value="net_pnl">Net PnL</option>
          <option value="win_rate">Win Rate</option>
          <option value="sharpe_ratio">Sharpe</option>
          <option value="total_trades">Trades</option>
          <option value="created_at">Date</option>
        </select>
      </div>
      <button onclick={loadKnowledgeBase} class="px-3 py-1.5 bg-emerald-600 text-white rounded text-sm hover:bg-emerald-500 transition-colors">
        Refresh
      </button>
      <button onclick={handleExportJSON} class="flex items-center gap-1 px-3 py-1.5 bg-cyan-600 hover:bg-cyan-500 text-white rounded text-sm transition-colors">
        <Download size={14} />
        Export JSON
      </button>
    </div>
  </div>

  <!-- Results Table -->
  {#if kbLoading}
    <div class="flex justify-center py-12">
      <Loader2 class="w-8 h-8 text-emerald-400 animate-spin" />
    </div>
  {:else if kbData.length === 0}
    <div class="text-center text-gray-500 py-12">
      <Database class="w-12 h-12 mx-auto mb-3 opacity-50" />
      <p>No backtests in the knowledge base yet.</p>
      <p class="text-sm mt-1">Run a Discovery Agent scan to populate it.</p>
    </div>
  {:else}
    <div class="bg-gray-800 rounded-lg overflow-hidden">
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-gray-400 text-xs uppercase border-b border-gray-700">
              <th class="px-3 py-2 text-left">#</th>
              <th class="px-3 py-2 text-left">Strategy</th>
              <th class="px-3 py-2 text-left">Symbol</th>
              <th class="px-3 py-2 text-right">Score</th>
              <th class="px-3 py-2 text-right">Net PnL</th>
              <th class="px-3 py-2 text-right">Win Rate</th>
              <th class="px-3 py-2 text-right">Sharpe</th>
              <th class="px-3 py-2 text-right">Drawdown</th>
              <th class="px-3 py-2 text-right">Trades</th>
              <th class="px-3 py-2 text-left">Params</th>
            </tr>
          </thead>
          <tbody>
            {#each kbData as row, i}
              <tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
                <td class="px-3 py-2 text-gray-500">{kbPage * kbPageSize + i + 1}</td>
                <td class="px-3 py-2 font-medium {getStrategyColor(row.strategy_name)}">{row.strategy_name}</td>
                <td class="px-3 py-2 text-gray-300">{row.symbol}</td>
                <td class="px-3 py-2 text-right text-white font-mono">{parseFloat(row.composite_score).toFixed(1)}</td>
                <td class="px-3 py-2 text-right font-mono {parseFloat(row.net_pnl) >= 0 ? 'text-green-400' : 'text-red-400'}">{parseFloat(row.net_pnl).toFixed(2)}</td>
                <td class="px-3 py-2 text-right font-mono {parseFloat(row.win_rate) >= 60 ? 'text-yellow-400' : 'text-gray-300'}">{parseFloat(row.win_rate).toFixed(1)}%</td>
                <td class="px-3 py-2 text-right font-mono text-gray-300">{parseFloat(row.sharpe_ratio).toFixed(2)}</td>
                <td class="px-3 py-2 text-right font-mono text-red-400">{parseFloat(row.max_drawdown_pct).toFixed(1)}%</td>
                <td class="px-3 py-2 text-right text-gray-300">{row.total_trades}</td>
                <td class="px-3 py-2 text-xs text-gray-500 max-w-xs truncate" title={row.strategy_params}>{row.strategy_params}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <!-- Pagination -->
      <div class="flex items-center justify-between px-4 py-3 border-t border-gray-700">
        <div class="text-xs text-gray-500">
          Showing {kbPage * kbPageSize + 1}-{Math.min((kbPage + 1) * kbPageSize, kbTotal)} of {kbTotal}
        </div>
        <div class="flex gap-2">
          <button onclick={kbPrevPage} disabled={kbPage === 0} class="px-3 py-1 rounded text-sm {kbPage === 0 ? 'text-gray-600 cursor-not-allowed' : 'text-gray-300 hover:bg-gray-700'}">
            Previous
          </button>
          <button onclick={kbNextPage} disabled={(kbPage + 1) * kbPageSize >= kbTotal} class="px-3 py-1 rounded text-sm {(kbPage + 1) * kbPageSize >= kbTotal ? 'text-gray-600 cursor-not-allowed' : 'text-gray-300 hover:bg-gray-700'}">
            Next
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
