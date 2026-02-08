<script>
  import { onDestroy } from 'svelte';
  import { getKnowledgeBase, getKnowledgeStats, exportResults } from '../lib/api.js';
  import { discoveryStatus } from '../lib/stores.js';
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

  // Auto-refresh interval
  let autoRefreshInterval = $state(null);

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
  // Auto-refresh when discovery is running
  // ============================================================================
  const unsubscribe = discoveryStatus.subscribe(status => {
    if (status.running && !autoRefreshInterval) {
      autoRefreshInterval = setInterval(loadKnowledgeBase, 60000);
    } else if (!status.running && autoRefreshInterval) {
      clearInterval(autoRefreshInterval);
      autoRefreshInterval = null;
      // One final refresh when discovery stops
      loadKnowledgeBase();
    }
  });

  onDestroy(() => {
    unsubscribe();
    if (autoRefreshInterval) {
      clearInterval(autoRefreshInterval);
    }
  });

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
    {#if $discoveryStatus.running}
      <div class="ml-auto flex items-center gap-2 px-3 py-1 bg-cyan-900/40 border border-cyan-700/50 rounded-full">
        <div class="w-2 h-2 rounded-full bg-cyan-400 animate-pulse"></div>
        <span class="text-xs text-cyan-400 font-semibold uppercase tracking-wider">LIVE</span>
      </div>
    {/if}
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
          <option value="vwap">VWAP</option>
          <option value="obv">OBV</option>
          <option value="williams_r">Williams %R</option>
          <option value="adx">ADX</option>
          <option value="rsi_bollinger">RSI+BB</option>
          <option value="macd_rsi">MACD+RSI</option>
          <option value="ema_rsi">EMA+RSI</option>
          <option value="stoch_rsi">Stoch+RSI</option>
          <option value="macd_bollinger">MACD+BB</option>
          <option value="triple_rsi_macd_bb">Triple RSI+MACD+BB</option>
          <option value="triple_ema_rsi_stoch">Triple EMA+RSI+Stoch</option>
          <option value="vwap_rsi">VWAP+RSI</option>
          <option value="obv_macd">OBV+MACD</option>
          <option value="adx_ema">ADX+EMA</option>
          <option value="williams_r_stoch">Williams%R+Stoch</option>
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
          <option value="strategy_confidence">Confidence</option>
          <option value="annualized_return_pct">Ann. Return</option>
          <option value="sortino_ratio">Sortino</option>
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
              <th class="px-3 py-2 text-left" title="Numéro de ligne dans la page courante">#</th>
              <th class="px-3 py-2 text-left" title="Nom et type de stratégie (indicateur seul, combo ou arbitrage)">Strategy</th>
              <th class="px-3 py-2 text-left" title="Paire de trading utilisée pour le backtest (symbole Binance)">Symbol</th>
              <th class="px-3 py-2 text-right" title="Confiance de la stratégie (0-100%) — mesure la consistance sur 4 quartiles temporels. Vert >=70%, Jaune >=40%, Rouge <40%">Conf.</th>
              <th class="px-3 py-2 text-right" title="Score composite combinant PnL net, win rate, Sharpe, drawdown, profit factor, bonus confiance et bonus Sortino">Score</th>
              <th class="px-3 py-2 text-right" title="Profit/perte net en USDC après frais taker Polymarket">Net PnL</th>
              <th class="px-3 py-2 text-right" title="Pourcentage de trades gagnants sur le total des trades">Win Rate</th>
              <th class="px-3 py-2 text-right" title="Rendement annualisé : (1 + rendement_total) ^ (365 / jours) - 1">Ann. Ret.</th>
              <th class="px-3 py-2 text-right" title="Ratio de Sharpe — rendement ajusté au risque (rendement excédentaire / écart-type). Plus c'est haut mieux c'est, >1 = bon, >2 = excellent">Sharpe</th>
              <th class="px-3 py-2 text-right" title="Ratio de Sortino — comme le Sharpe mais ne pénalise que la volatilité à la baisse. Plus c'est haut mieux c'est">Sortino</th>
              <th class="px-3 py-2 text-right" title="Drawdown maximum — plus grande baisse entre un pic et un creux pendant la période de backtest">Drawdown</th>
              <th class="px-3 py-2 text-right" title="Nombre total de trades exécutés pendant le backtest">Trades</th>
              <th class="px-3 py-2 text-left" title="Paramètres de la stratégie en JSON (réglages des indicateurs, seuils, périodes)">Params</th>
            </tr>
          </thead>
          <tbody>
            {#each kbData as row, i}
              {@const confVal = parseFloat(row.strategy_confidence || 0)}
              {@const confColor = confVal >= 70 ? 'text-green-400' : confVal >= 40 ? 'text-yellow-400' : confVal > 0 ? 'text-red-400' : 'text-gray-600'}
              {@const confBg = confVal >= 70 ? 'bg-green-500' : confVal >= 40 ? 'bg-yellow-500' : 'bg-red-500'}
              <tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
                <td class="px-3 py-2 text-gray-500">{kbPage * kbPageSize + i + 1}</td>
                <td class="px-3 py-2 font-medium {getStrategyColor(row.strategy_name)}">{row.strategy_name}</td>
                <td class="px-3 py-2 text-gray-300">{row.symbol}</td>
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
                <td class="px-3 py-2 text-right text-white font-mono">{parseFloat(row.composite_score).toFixed(1)}</td>
                <td class="px-3 py-2 text-right font-mono {parseFloat(row.net_pnl) >= 0 ? 'text-green-400' : 'text-red-400'}">{parseFloat(row.net_pnl).toFixed(2)}</td>
                <td class="px-3 py-2 text-right font-mono {parseFloat(row.win_rate) >= 60 ? 'text-yellow-400' : 'text-gray-300'}">{parseFloat(row.win_rate).toFixed(1)}%</td>
                <td class="px-3 py-2 text-right font-mono {parseFloat(row.annualized_return_pct || 0) >= 0 ? 'text-green-400' : 'text-red-400'}">{parseFloat(row.annualized_return_pct || 0).toFixed(1)}%</td>
                <td class="px-3 py-2 text-right font-mono text-gray-300">{parseFloat(row.sharpe_ratio).toFixed(2)}</td>
                <td class="px-3 py-2 text-right font-mono text-gray-300">{parseFloat(row.sortino_ratio || 0).toFixed(2)}</td>
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
