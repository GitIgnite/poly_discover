<script>
  import { onDestroy } from 'svelte';
  import { startProfileAnalysis, getProfileStatus, cancelProfileAnalysis, getProfileHistory } from '../lib/api.js';

  // State
  let username = $state('');
  let status = $state(null);
  let result = $state(null);
  let history = $state([]);
  let error = $state('');
  let polling = $state(null);
  let activeTab = $state('overview');
  let marketFilter = $state('all');
  let marketStrategyFilter = $state('all');
  let marketSort = $state('volume');

  // Load history on mount
  loadHistory();

  async function loadHistory() {
    const res = await getProfileHistory();
    if (res.success && res.data) {
      history = res.data;
    }
  }

  async function analyze() {
    if (!username.trim()) return;
    error = '';
    result = null;
    const res = await startProfileAnalysis(username.trim());
    if (!res.success) {
      error = res.message || res.error || 'Failed to start analysis';
      return;
    }
    startPolling();
  }

  function startPolling() {
    stopPolling();
    poll();
    polling = setInterval(poll, 2000);
  }

  function stopPolling() {
    if (polling) {
      clearInterval(polling);
      polling = null;
    }
  }

  async function poll() {
    const res = await getProfileStatus();
    status = res;
    if (res.status === 'Complete' && res.result) {
      result = res.result;
      stopPolling();
      loadHistory();
    } else if (res.status === 'Error') {
      error = res.error || 'Analysis failed';
      stopPolling();
    }
  }

  async function cancel() {
    await cancelProfileAnalysis();
    stopPolling();
    status = null;
  }

  function loadFromHistory(item) {
    // Parse JSON fields from DB record
    result = {
      wallet: item.wallet,
      username: item.username || item.wallet,
      portfolio_value: item.portfolio_value || 0,
      total_pnl: item.total_pnl || 0,
      total_volume: item.total_volume || 0,
      total_trades: item.total_trades || 0,
      unique_markets: item.unique_markets || 0,
      win_rate: item.win_rate || 0,
      primary_strategy: item.primary_strategy || 'Unknown',
      strategy_confidence: item.strategy_confidence || 0,
      open_positions: item.open_positions_json ? JSON.parse(item.open_positions_json) : [],
      closed_positions: item.closed_positions_json ? JSON.parse(item.closed_positions_json) : [],
      markets: item.markets_json ? JSON.parse(item.markets_json) : [],
      category_breakdown: item.category_breakdown_json ? JSON.parse(item.category_breakdown_json) : [],
      activity_timeline: item.activity_timeline_json ? JSON.parse(item.activity_timeline_json) : [],
      strategy_signals: item.strategy_signals_json ? JSON.parse(item.strategy_signals_json) : [],
      avg_hold_duration_days: item.avg_hold_duration || 0,
      best_trade_pnl: item.best_trade_pnl || 0,
      worst_trade_pnl: item.worst_trade_pnl || 0,
      max_drawdown: item.max_drawdown || 0,
      active_days: item.active_days || 0,
      avg_position_size: item.avg_position_size || 0,
    };
    username = item.username || item.wallet;
    status = null;
    activeTab = 'overview';
  }

  function fmt(n, decimals = 2) {
    if (n == null) return '—';
    return Number(n).toLocaleString('en-US', { minimumFractionDigits: decimals, maximumFractionDigits: decimals });
  }

  function fmtUsd(n) {
    if (n == null) return '—';
    return '$' + fmt(n);
  }

  function pnlClass(n) {
    if (n > 0) return 'text-emerald-400';
    if (n < 0) return 'text-red-400';
    return 'text-gray-400';
  }

  function strategyColor(strategy) {
    const colors = {
      'Scalping': 'bg-yellow-500/20 text-yellow-300',
      'Momentum': 'bg-blue-500/20 text-blue-300',
      'Contrarian': 'bg-purple-500/20 text-purple-300',
      'Market Making': 'bg-teal-500/20 text-teal-300',
      'Event-Driven': 'bg-pink-500/20 text-pink-300',
      'Hold to Resolution': 'bg-green-500/20 text-green-300',
      'Swing Trading': 'bg-orange-500/20 text-orange-300',
      'Accumulation': 'bg-indigo-500/20 text-indigo-300',
      'DCA': 'bg-cyan-500/20 text-cyan-300',
    };
    return colors[strategy] || 'bg-gray-500/20 text-gray-300';
  }

  function filteredMarkets() {
    if (!result?.markets) return [];
    let markets = [...result.markets];

    if (marketFilter !== 'all') {
      markets = markets.filter(m => m.category === marketFilter);
    }
    if (marketStrategyFilter !== 'all') {
      markets = markets.filter(m => m.inferred_strategy === marketStrategyFilter);
    }

    if (marketSort === 'volume') markets.sort((a, b) => b.volume - a.volume);
    else if (marketSort === 'pnl') markets.sort((a, b) => b.realized_pnl - a.realized_pnl);
    else if (marketSort === 'trades') markets.sort((a, b) => b.trade_count - a.trade_count);
    else if (marketSort === 'recent') markets.sort((a, b) => b.last_trade_ts - a.last_trade_ts);

    return markets;
  }

  function uniqueCategories() {
    if (!result?.markets) return [];
    const cats = [...new Set(result.markets.map(m => m.category))];
    return cats.sort();
  }

  function uniqueStrategies() {
    if (!result?.markets) return [];
    const strats = [...new Set(result.markets.map(m => m.inferred_strategy))];
    return strats.sort();
  }

  function formatDate(ts) {
    if (!ts) return '—';
    return new Date(ts * 1000).toLocaleDateString('fr-FR', { day: '2-digit', month: '2-digit', year: 'numeric' });
  }

  function formatDateTime(ts) {
    if (!ts) return '—';
    return new Date(ts * 1000).toLocaleString('fr-FR');
  }

  let expandedMarket = $state(null);

  function toggleMarket(cid) {
    expandedMarket = expandedMarket === cid ? null : cid;
  }

  onDestroy(() => stopPolling());
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center gap-3 flex-wrap">
    <h2 class="text-2xl font-bold text-white">Profile Analysis</h2>
    {#if status?.running}
      <span class="px-2 py-0.5 bg-purple-500/20 text-purple-300 rounded text-xs animate-pulse">ANALYZING</span>
    {/if}
  </div>

  <!-- Search bar -->
  <div class="bg-gray-800 rounded-xl p-4 border border-gray-700">
    <div class="flex gap-3 items-end flex-wrap">
      <div class="flex-1 min-w-[200px]">
        <label for="username" class="block text-xs text-gray-400 mb-1">Nom d'utilisateur Polymarket</label>
        <input
          id="username"
          type="text"
          bind:value={username}
          placeholder="Entrez un username Polymarket..."
          class="w-full bg-gray-700 text-white rounded-lg px-3 py-2 text-sm border border-gray-600 focus:border-purple-500 focus:outline-none"
          onkeydown={(e) => e.key === 'Enter' && analyze()}
        />
      </div>
      {#if status?.running}
        <button onclick={cancel} class="px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg text-sm font-medium">
          Cancel
        </button>
      {:else}
        <button onclick={analyze} class="px-4 py-2 bg-purple-600 hover:bg-purple-700 text-white rounded-lg text-sm font-medium" disabled={!username.trim()}>
          Analyze
        </button>
      {/if}
    </div>

    {#if error}
      <div class="mt-2 text-red-400 text-sm">{error}</div>
    {/if}

    <!-- Progress -->
    {#if status?.running}
      <div class="mt-3">
        <div class="flex justify-between text-xs text-gray-400 mb-1">
          <span>{status.current_step || 'Starting...'}</span>
          <span>{status.completed_steps}/{status.total_steps}</span>
        </div>
        <div class="w-full bg-gray-700 rounded-full h-2">
          <div
            class="bg-purple-500 h-2 rounded-full transition-all duration-300"
            style="width: {status.total_steps > 0 ? (status.completed_steps / status.total_steps * 100) : 0}%"
          ></div>
        </div>
      </div>
    {/if}

    <!-- History chips -->
    {#if history.length > 0 && !result}
      <div class="mt-3 flex gap-2 flex-wrap">
        <span class="text-xs text-gray-500">Historique :</span>
        {#each history.slice(0, 8) as item}
          <button
            onclick={() => loadFromHistory(item)}
            class="px-2 py-0.5 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded text-xs"
          >
            {item.username || item.wallet?.slice(0, 10)}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Results -->
  {#if result}
    <!-- Tab navigation -->
    <div class="flex gap-1 bg-gray-800 rounded-lg p-1 border border-gray-700">
      {#each [
        { id: 'overview', label: 'Vue d\'ensemble' },
        { id: 'markets', label: `Marchés (${result.unique_markets})` },
        { id: 'positions', label: 'Positions' },
        { id: 'timeline', label: 'Timeline' },
      ] as tab}
        <button
          onclick={() => activeTab = tab.id}
          class="px-3 py-1.5 rounded-md text-sm transition-colors {activeTab === tab.id ? 'bg-purple-600 text-white' : 'text-gray-400 hover:text-white hover:bg-gray-700'}"
        >
          {tab.label}
        </button>
      {/each}
    </div>

    <!-- Overview Tab -->
    {#if activeTab === 'overview'}
      <div class="space-y-4">
        <!-- Username header -->
        <div class="bg-gray-800 rounded-xl p-4 border border-gray-700">
          <div class="flex items-center gap-3 mb-3">
            <h3 class="text-lg font-bold text-white">{result.username}</h3>
            <span class="px-2 py-0.5 rounded text-xs {strategyColor(result.primary_strategy)}">{result.primary_strategy}</span>
          </div>
          <p class="text-xs text-gray-500 font-mono">{result.wallet}</p>
        </div>

        <!-- Metric cards -->
        <div class="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3">
          <div class="bg-gray-800 rounded-lg p-3 border border-gray-700">
            <div class="text-xs text-gray-400">Portfolio Value</div>
            <div class="text-lg font-bold text-white">{fmtUsd(result.portfolio_value)}</div>
          </div>
          <div class="bg-gray-800 rounded-lg p-3 border border-gray-700">
            <div class="text-xs text-gray-400">Total PnL</div>
            <div class="text-lg font-bold {pnlClass(result.total_pnl)}">{fmtUsd(result.total_pnl)}</div>
          </div>
          <div class="bg-gray-800 rounded-lg p-3 border border-gray-700">
            <div class="text-xs text-gray-400">Volume</div>
            <div class="text-lg font-bold text-white">{fmtUsd(result.total_volume)}</div>
          </div>
          <div class="bg-gray-800 rounded-lg p-3 border border-gray-700">
            <div class="text-xs text-gray-400">Win Rate</div>
            <div class="text-lg font-bold {result.win_rate >= 50 ? 'text-emerald-400' : 'text-red-400'}">{fmt(result.win_rate, 1)}%</div>
          </div>
          <div class="bg-gray-800 rounded-lg p-3 border border-gray-700">
            <div class="text-xs text-gray-400">Trades</div>
            <div class="text-lg font-bold text-white">{result.total_trades?.toLocaleString()}</div>
          </div>
          <div class="bg-gray-800 rounded-lg p-3 border border-gray-700">
            <div class="text-xs text-gray-400">Markets</div>
            <div class="text-lg font-bold text-white">{result.unique_markets}</div>
          </div>
        </div>

        <!-- Strategy signals -->
        {#if result.strategy_signals?.length > 0}
          <div class="bg-gray-800 rounded-xl p-4 border border-gray-700">
            <h4 class="text-sm font-semibold text-gray-300 mb-3">Strategies detectees</h4>
            <div class="space-y-2">
              {#each result.strategy_signals as sig}
                <div class="flex items-center gap-3">
                  <span class="px-2 py-0.5 rounded text-xs font-medium min-w-[120px] text-center {strategyColor(sig.strategy)}">{sig.strategy}</span>
                  <div class="flex-1 bg-gray-700 rounded-full h-2">
                    <div class="bg-purple-500 h-2 rounded-full" style="width: {sig.confidence * 100}%"></div>
                  </div>
                  <span class="text-xs text-gray-400 min-w-[40px] text-right">{fmt(sig.confidence * 100, 0)}%</span>
                  <span class="text-xs text-gray-500">{sig.market_count} marchés</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <!-- Category breakdown -->
        {#if result.category_breakdown?.length > 0}
          <div class="bg-gray-800 rounded-xl p-4 border border-gray-700">
            <h4 class="text-sm font-semibold text-gray-300 mb-3">Repartition par categorie</h4>
            <div class="overflow-x-auto">
              <table class="w-full text-sm">
                <thead>
                  <tr class="text-xs text-gray-500 border-b border-gray-700">
                    <th class="text-left py-2 px-2">Categorie</th>
                    <th class="text-right py-2 px-2">Marchés</th>
                    <th class="text-right py-2 px-2">Trades</th>
                    <th class="text-right py-2 px-2">Volume</th>
                    <th class="text-right py-2 px-2">PnL</th>
                    <th class="text-right py-2 px-2">Win Rate</th>
                  </tr>
                </thead>
                <tbody>
                  {#each result.category_breakdown as cat}
                    <tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
                      <td class="py-2 px-2 text-white font-medium">{cat.category}</td>
                      <td class="py-2 px-2 text-right text-gray-300">{cat.market_count}</td>
                      <td class="py-2 px-2 text-right text-gray-300">{cat.trade_count}</td>
                      <td class="py-2 px-2 text-right text-gray-300">{fmtUsd(cat.volume)}</td>
                      <td class="py-2 px-2 text-right {pnlClass(cat.pnl)}">{fmtUsd(cat.pnl)}</td>
                      <td class="py-2 px-2 text-right {cat.win_rate >= 50 ? 'text-emerald-400' : 'text-red-400'}">{fmt(cat.win_rate, 1)}%</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </div>
        {/if}

        <!-- Advanced metrics -->
        <div class="bg-gray-800 rounded-xl p-4 border border-gray-700">
          <h4 class="text-sm font-semibold text-gray-300 mb-3">Metriques avancees</h4>
          <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
            <div>
              <div class="text-xs text-gray-500">Jours actifs</div>
              <div class="text-sm text-white font-medium">{result.active_days}</div>
            </div>
            <div>
              <div class="text-xs text-gray-500">Taille moy. position</div>
              <div class="text-sm text-white font-medium">{fmtUsd(result.avg_position_size)}</div>
            </div>
            <div>
              <div class="text-xs text-gray-500">Meilleur marché</div>
              <div class="text-sm text-emerald-400 font-medium">{fmtUsd(result.best_trade_pnl)}</div>
            </div>
            <div>
              <div class="text-xs text-gray-500">Pire marché</div>
              <div class="text-sm text-red-400 font-medium">{fmtUsd(result.worst_trade_pnl)}</div>
            </div>
            <div>
              <div class="text-xs text-gray-500">Max Drawdown</div>
              <div class="text-sm text-red-400 font-medium">{fmtUsd(result.max_drawdown)}</div>
            </div>
            <div>
              <div class="text-xs text-gray-500">Durée moy. position</div>
              <div class="text-sm text-white font-medium">{fmt(result.avg_hold_duration_days, 1)} jours</div>
            </div>
          </div>
        </div>
      </div>
    {/if}

    <!-- Markets Tab -->
    {#if activeTab === 'markets'}
      <div class="space-y-3">
        <!-- Filters -->
        <div class="flex gap-3 flex-wrap items-center">
          <select bind:value={marketFilter} class="bg-gray-700 text-white text-xs rounded-lg px-3 py-1.5 border border-gray-600">
            <option value="all">Toutes categories</option>
            {#each uniqueCategories() as cat}
              <option value={cat}>{cat}</option>
            {/each}
          </select>
          <select bind:value={marketStrategyFilter} class="bg-gray-700 text-white text-xs rounded-lg px-3 py-1.5 border border-gray-600">
            <option value="all">Toutes strategies</option>
            {#each uniqueStrategies() as strat}
              <option value={strat}>{strat}</option>
            {/each}
          </select>
          <select bind:value={marketSort} class="bg-gray-700 text-white text-xs rounded-lg px-3 py-1.5 border border-gray-600">
            <option value="volume">Tri: Volume</option>
            <option value="pnl">Tri: PnL</option>
            <option value="trades">Tri: Nb trades</option>
            <option value="recent">Tri: Plus recent</option>
          </select>
          <span class="text-xs text-gray-500">{filteredMarkets().length} marchés</span>
        </div>

        <!-- Market cards -->
        {#each filteredMarkets() as market}
          <div class="bg-gray-800 rounded-lg border border-gray-700 overflow-hidden">
            <button
              onclick={() => toggleMarket(market.condition_id)}
              class="w-full text-left p-3 hover:bg-gray-700/30 transition-colors"
            >
              <div class="flex items-start justify-between gap-2">
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-2 flex-wrap">
                    <span class="text-sm text-white font-medium truncate">{market.title}</span>
                    <span class="px-1.5 py-0.5 rounded text-[10px] {strategyColor(market.inferred_strategy)}">{market.inferred_strategy}</span>
                    {#if market.category && market.category !== 'Unknown'}
                      <span class="px-1.5 py-0.5 bg-gray-600/50 text-gray-400 rounded text-[10px]">{market.category}</span>
                    {/if}
                  </div>
                  <div class="flex gap-4 mt-1 text-xs text-gray-400">
                    <span>{market.trade_count} trades</span>
                    <span>B:{market.buy_count} / S:{market.sell_count}</span>
                    <span>Vol: {fmtUsd(market.volume)}</span>
                  </div>
                </div>
                <div class="text-right flex-shrink-0">
                  <div class="text-sm font-medium {pnlClass(market.realized_pnl)}">{fmtUsd(market.realized_pnl)}</div>
                  <div class="text-xs text-gray-500">PnL</div>
                </div>
              </div>
            </button>

            <!-- Expanded detail -->
            {#if expandedMarket === market.condition_id}
              <div class="border-t border-gray-700 p-3 bg-gray-750">
                <div class="grid grid-cols-2 md:grid-cols-4 gap-3 mb-3 text-xs">
                  <div>
                    <span class="text-gray-500">Avg Buy Price</span>
                    <div class="text-white">{fmt(market.avg_buy_price, 4)}</div>
                  </div>
                  <div>
                    <span class="text-gray-500">Avg Sell Price</span>
                    <div class="text-white">{fmt(market.avg_sell_price, 4)}</div>
                  </div>
                  <div>
                    <span class="text-gray-500">Net Position</span>
                    <div class="text-white">{fmt(market.net_position, 2)}</div>
                  </div>
                  <div>
                    <span class="text-gray-500">Periode</span>
                    <div class="text-white">{formatDate(market.first_trade_ts)} — {formatDate(market.last_trade_ts)}</div>
                  </div>
                </div>

                <!-- Trades table -->
                {#if market.trades?.length > 0}
                  <table class="w-full text-xs">
                    <thead>
                      <tr class="text-gray-500 border-b border-gray-700">
                        <th class="text-left py-1 px-1">Side</th>
                        <th class="text-right py-1 px-1">Size</th>
                        <th class="text-right py-1 px-1">Price</th>
                        <th class="text-right py-1 px-1">Date</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each market.trades.slice(0, 20) as trade}
                        <tr class="border-b border-gray-700/30">
                          <td class="py-1 px-1 {trade.side === 'BUY' ? 'text-emerald-400' : 'text-red-400'}">{trade.side}</td>
                          <td class="py-1 px-1 text-right text-gray-300">{fmt(trade.size, 2)}</td>
                          <td class="py-1 px-1 text-right text-gray-300">{fmt(trade.price, 4)}</td>
                          <td class="py-1 px-1 text-right text-gray-500">{formatDateTime(trade.timestamp)}</td>
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                  {#if market.trades.length > 20}
                    <div class="text-xs text-gray-500 mt-1 text-center">... et {market.trades.length - 20} trades de plus</div>
                  {/if}
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <!-- Positions Tab -->
    {#if activeTab === 'positions'}
      <div class="space-y-4">
        <!-- Open positions -->
        <div class="bg-gray-800 rounded-xl p-4 border border-gray-700">
          <h4 class="text-sm font-semibold text-gray-300 mb-3">Positions ouvertes ({result.open_positions?.length || 0})</h4>
          {#if result.open_positions?.length > 0}
            <div class="overflow-x-auto">
              <table class="w-full text-xs">
                <thead>
                  <tr class="text-gray-500 border-b border-gray-700">
                    <th class="text-left py-2 px-2">Marché</th>
                    <th class="text-left py-2 px-2">Outcome</th>
                    <th class="text-right py-2 px-2">Size</th>
                    <th class="text-right py-2 px-2">Avg Price</th>
                    <th class="text-right py-2 px-2">Cur Price</th>
                    <th class="text-right py-2 px-2">PnL</th>
                    <th class="text-right py-2 px-2">PnL %</th>
                  </tr>
                </thead>
                <tbody>
                  {#each result.open_positions as pos}
                    <tr class="border-b border-gray-700/30 hover:bg-gray-700/20">
                      <td class="py-2 px-2 text-white max-w-[200px] truncate">{pos.title || pos.conditionId || '—'}</td>
                      <td class="py-2 px-2 text-gray-400">{pos.outcome || '—'}</td>
                      <td class="py-2 px-2 text-right text-gray-300">{fmt(pos.size, 2)}</td>
                      <td class="py-2 px-2 text-right text-gray-300">{fmt(pos.avgPrice, 4)}</td>
                      <td class="py-2 px-2 text-right text-gray-300">{fmt(pos.curPrice, 4)}</td>
                      <td class="py-2 px-2 text-right {pnlClass(pos.cashPnl)}">{fmtUsd(pos.cashPnl)}</td>
                      <td class="py-2 px-2 text-right {pnlClass(pos.percentPnl)}">{fmt(pos.percentPnl, 1)}%</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {:else}
            <p class="text-gray-500 text-sm">Aucune position ouverte</p>
          {/if}
        </div>

        <!-- Closed positions -->
        <div class="bg-gray-800 rounded-xl p-4 border border-gray-700">
          <h4 class="text-sm font-semibold text-gray-300 mb-3">Positions fermees ({result.closed_positions?.length || 0})</h4>
          {#if result.closed_positions?.length > 0}
            <div class="overflow-x-auto">
              <table class="w-full text-xs">
                <thead>
                  <tr class="text-gray-500 border-b border-gray-700">
                    <th class="text-left py-2 px-2">Marché</th>
                    <th class="text-left py-2 px-2">Outcome</th>
                    <th class="text-right py-2 px-2">Avg Price</th>
                    <th class="text-right py-2 px-2">Total Bought</th>
                    <th class="text-right py-2 px-2">Realized PnL</th>
                    <th class="text-right py-2 px-2">Date</th>
                  </tr>
                </thead>
                <tbody>
                  {#each result.closed_positions as pos}
                    <tr class="border-b border-gray-700/30 hover:bg-gray-700/20">
                      <td class="py-2 px-2 text-white max-w-[200px] truncate">{pos.title || pos.conditionId || '—'}</td>
                      <td class="py-2 px-2 text-gray-400">{pos.outcome || '—'}</td>
                      <td class="py-2 px-2 text-right text-gray-300">{fmt(pos.avgPrice, 4)}</td>
                      <td class="py-2 px-2 text-right text-gray-300">{fmt(pos.totalBought, 2)}</td>
                      <td class="py-2 px-2 text-right {pnlClass(pos.realizedPnl)}">{fmtUsd(pos.realizedPnl)}</td>
                      <td class="py-2 px-2 text-right text-gray-500">{formatDate(pos.timestamp)}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {:else}
            <p class="text-gray-500 text-sm">Aucune position fermee</p>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Timeline Tab -->
    {#if activeTab === 'timeline'}
      <div class="bg-gray-800 rounded-xl p-4 border border-gray-700">
        <h4 class="text-sm font-semibold text-gray-300 mb-3">Activite quotidienne ({result.activity_timeline?.length || 0} jours)</h4>
        {#if result.activity_timeline?.length > 0}
          <div class="space-y-1 max-h-[500px] overflow-y-auto">
            {#each result.activity_timeline as day}
              {@const maxVol = Math.max(...result.activity_timeline.map(d => d.volume))}
              {@const barWidth = maxVol > 0 ? (day.volume / maxVol * 100) : 0}
              <div class="flex items-center gap-2 text-xs">
                <span class="text-gray-500 w-20 flex-shrink-0">{day.date}</span>
                <div class="flex-1 bg-gray-700 rounded h-4 relative">
                  <div
                    class="bg-purple-500/50 h-4 rounded"
                    style="width: {barWidth}%"
                  ></div>
                </div>
                <span class="text-gray-400 w-16 text-right flex-shrink-0">{day.trade_count} tr.</span>
                <span class="text-gray-300 w-24 text-right flex-shrink-0">{fmtUsd(day.volume)}</span>
              </div>
            {/each}
          </div>
        {:else}
          <p class="text-gray-500 text-sm">Aucune activite</p>
        {/if}
      </div>
    {/if}
  {/if}
</div>
