<script>
  import { onDestroy } from 'svelte';
  import {
    startLeaderboardAnalysis, getLeaderboardStatus, getLeaderboardTraders,
    startWatcher, stopWatcher, getWatcherStatus
  } from '../lib/api.js';

  let status = $state('Idle');
  let progressPct = $state(0);
  let totalTraders = $state(0);
  let analyzed = $state(0);
  let currentTrader = $state('');
  let results = $state([]);
  let error = $state(null);
  let expandedCards = $state({});
  let polling = $state(null);

  // Trade Watcher state
  let watcherStatus = $state('Idle');
  let watcherAlerts = $state([]);
  let watcherWatchedCount = $state(0);
  let watcherError = $state(null);
  let watcherPolling = $state(null);

  const strategyColors = {
    Momentum: { bg: 'bg-blue-900/40', border: 'border-blue-500', text: 'text-blue-400', bar: 'bg-blue-500' },
    Contrarian: { bg: 'bg-purple-900/40', border: 'border-purple-500', text: 'text-purple-400', bar: 'bg-purple-500' },
    Scalper: { bg: 'bg-yellow-900/40', border: 'border-yellow-500', text: 'text-yellow-400', bar: 'bg-yellow-500' },
    'Market Maker': { bg: 'bg-teal-900/40', border: 'border-teal-500', text: 'text-teal-400', bar: 'bg-teal-500' },
    Arbitrage: { bg: 'bg-orange-900/40', border: 'border-orange-500', text: 'text-orange-400', bar: 'bg-orange-500' },
    'Event-Driven': { bg: 'bg-pink-900/40', border: 'border-pink-500', text: 'text-pink-400', bar: 'bg-pink-500' },
    'High Conviction': { bg: 'bg-red-900/40', border: 'border-red-500', text: 'text-red-400', bar: 'bg-red-500' },
    Diversified: { bg: 'bg-green-900/40', border: 'border-green-500', text: 'text-green-400', bar: 'bg-green-500' },
    Mixed: { bg: 'bg-gray-900/40', border: 'border-gray-500', text: 'text-gray-400', bar: 'bg-gray-500' },
  };

  function getColors(label) {
    return strategyColors[label] || strategyColors.Mixed;
  }

  async function startAnalysis() {
    error = null;
    const res = await startLeaderboardAnalysis();
    if (!res.success) {
      error = res.message;
      return;
    }
    startPolling();
  }

  function startPolling() {
    stopPolling();
    pollStatus();
    polling = setInterval(pollStatus, 2000);
  }

  function stopPolling() {
    if (polling) {
      clearInterval(polling);
      polling = null;
    }
  }

  async function pollStatus() {
    const res = await getLeaderboardStatus();
    status = res.status || 'Idle';
    progressPct = res.progress_pct || 0;
    totalTraders = res.total_traders || 0;
    analyzed = res.analyzed || 0;
    currentTrader = res.current_trader || '';
    results = res.results || [];
    error = res.error || null;

    if (status === 'Complete' || status === 'Error') {
      stopPolling();
    }
  }

  // --- Trade Watcher ---
  async function handleStartWatcher() {
    watcherError = null;
    const res = await startWatcher();
    if (!res.success) {
      watcherError = res.message;
      return;
    }
    startWatcherPolling();
  }

  async function handleStopWatcher() {
    await stopWatcher();
  }

  function startWatcherPolling() {
    stopWatcherPolling();
    pollWatcherStatus();
    watcherPolling = setInterval(pollWatcherStatus, 5000);
  }

  function stopWatcherPolling() {
    if (watcherPolling) {
      clearInterval(watcherPolling);
      watcherPolling = null;
    }
  }

  async function pollWatcherStatus() {
    const res = await getWatcherStatus();
    watcherStatus = res.status || 'Idle';
    watcherAlerts = res.alerts || [];
    watcherWatchedCount = res.watched_count || 0;
    watcherError = res.error || null;

    if (watcherStatus === 'Idle' || watcherStatus === 'Error') {
      stopWatcherPolling();
    }
  }

  function toggleCard(index) {
    expandedCards = { ...expandedCards, [index]: !expandedCards[index] };
  }

  function fmt(val, decimals = 2) {
    if (val == null) return '-';
    return Number(val).toFixed(decimals);
  }

  function fmtMoney(val) {
    if (val == null) return '-';
    const n = Number(val);
    if (Math.abs(n) >= 1_000_000) return `$${(n / 1_000_000).toFixed(1)}M`;
    if (Math.abs(n) >= 1_000) return `$${(n / 1_000).toFixed(1)}K`;
    return `$${n.toFixed(0)}`;
  }

  function rankMedal(rank) {
    if (rank === 1) return '1st';
    if (rank === 2) return '2nd';
    if (rank === 3) return '3rd';
    return `#${rank}`;
  }

  function fmtTimestamp(ts) {
    if (!ts) return '-';
    const d = new Date(ts * 1000);
    return d.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  // On mount: load persisted traders from DB, check analysis and watcher status
  async function init() {
    // Check if analysis is running
    await pollStatus();
    if (status === 'FetchingLeaderboard' || status === 'AnalyzingTrader') {
      startPolling();
    }

    // If no results from live analysis, try loading from DB
    if (results.length === 0 && status === 'Idle') {
      const dbRes = await getLeaderboardTraders();
      if (dbRes.success && dbRes.data?.length > 0) {
        // Convert DB records to the display format the template expects
        results = dbRes.data.map(t => ({
          entry: {
            rank: t.rank,
            proxyWallet: t.proxy_wallet,
            userName: t.user_name,
            pnl: t.pnl,
            vol: t.volume,
          },
          metrics: t.metrics_json ? JSON.parse(t.metrics_json) : {},
          strategies: t.strategies_json ? JSON.parse(t.strategies_json) : [],
          portfolioValue: t.portfolio_value,
          topPositions: t.top_positions_json ? JSON.parse(t.top_positions_json) : [],
          recentTrades: [],
        }));
        status = 'Complete';
      }
    }

    // Check watcher status
    await pollWatcherStatus();
    if (watcherStatus === 'Watching') {
      startWatcherPolling();
    }
  }

  init();

  onDestroy(() => {
    stopPolling();
    stopWatcherPolling();
  });
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
    <div>
      <h2 class="text-2xl font-bold text-white">Leaderboard Analyzer</h2>
      <p class="text-sm text-gray-400 mt-1">Analyze top Polymarket traders and infer their strategies</p>
    </div>
    <button
      onclick={startAnalysis}
      disabled={status === 'FetchingLeaderboard' || status === 'AnalyzingTrader'}
      class="px-5 py-2.5 rounded-lg font-semibold text-sm transition-colors
        {status === 'FetchingLeaderboard' || status === 'AnalyzingTrader'
          ? 'bg-gray-600 text-gray-400 cursor-not-allowed'
          : 'bg-rose-600 hover:bg-rose-500 text-white'}"
    >
      {#if status === 'FetchingLeaderboard' || status === 'AnalyzingTrader'}
        Analyzing...
      {:else}
        Analyze Top 10
      {/if}
    </button>
  </div>

  <!-- Progress bar -->
  {#if status === 'FetchingLeaderboard' || status === 'AnalyzingTrader'}
    <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
      <div class="flex items-center justify-between mb-2">
        <span class="text-sm text-gray-300">
          {#if status === 'FetchingLeaderboard'}
            Fetching leaderboard...
          {:else}
            Analyzing: <span class="text-rose-400 font-medium">{currentTrader}</span>
          {/if}
        </span>
        <span class="text-sm text-gray-400">{analyzed}/{totalTraders} traders ({progressPct}%)</span>
      </div>
      <div class="w-full h-2 bg-gray-700 rounded-full overflow-hidden">
        <div
          class="h-full bg-rose-500 rounded-full transition-all duration-500"
          style="width: {progressPct}%"
        ></div>
      </div>
    </div>
  {/if}

  <!-- Error -->
  {#if error}
    <div class="bg-red-900/30 border border-red-700 rounded-lg p-4">
      <p class="text-red-400 text-sm">{error}</p>
    </div>
  {/if}

  <!-- Results -->
  {#if results.length > 0}
    <div class="space-y-4">
      {#each results as trader, i}
        {@const primary = trader.strategies?.[0]}
        {@const colors = primary ? getColors(primary.strategy?.label || primary.strategy) : getColors('Mixed')}
        {@const stratLabel = primary?.strategy?.label || primary?.strategy || 'Mixed'}
        {@const rank = trader.entry?.rank || (i + 1)}

        <div class="bg-gray-800 rounded-lg border border-gray-700 overflow-hidden">
          <!-- Card Header (always visible) -->
          <button
            onclick={() => toggleCard(i)}
            class="w-full p-4 flex items-center gap-4 hover:bg-gray-750 transition-colors text-left"
          >
            <!-- Rank -->
            <div class="flex-shrink-0 w-12 h-12 rounded-full flex items-center justify-center font-bold text-lg
              {rank <= 3 ? 'bg-yellow-900/50 text-yellow-400 border border-yellow-600' : 'bg-gray-700 text-gray-300 border border-gray-600'}">
              {rankMedal(rank)}
            </div>

            <!-- Name & strategy -->
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <span class="text-white font-semibold truncate">{trader.entry?.userName || `Trader #${i+1}`}</span>
                {#if trader.entry?.verifiedBadge}
                  <span class="text-blue-400 text-xs" title="Verified">&#10003;</span>
                {/if}
                {#if trader.entry?.xUsername}
                  <span class="text-gray-500 text-xs">@{trader.entry.xUsername}</span>
                {/if}
              </div>
              <div class="flex items-center gap-3 mt-1">
                <span class="text-xs px-2 py-0.5 rounded-full {colors.bg} {colors.text} border {colors.border}">
                  {stratLabel}
                </span>
                {#if primary?.confidence}
                  <span class="text-xs text-gray-500">{(primary.confidence * 100).toFixed(0)}% confidence</span>
                {/if}
              </div>
            </div>

            <!-- PnL & Volume -->
            <div class="flex-shrink-0 text-right">
              <div class="text-sm font-semibold {(trader.entry?.pnl || 0) >= 0 ? 'text-emerald-400' : 'text-red-400'}">
                PnL: {fmtMoney(trader.entry?.pnl)}
              </div>
              <div class="text-xs text-gray-400">
                Vol: {fmtMoney(trader.entry?.vol)}
              </div>
              {#if trader.portfolioValue != null}
                <div class="text-xs text-gray-500">
                  Portfolio: {fmtMoney(trader.portfolioValue)}
                </div>
              {/if}
            </div>

            <!-- Expand arrow -->
            <div class="flex-shrink-0 text-gray-500 ml-2">
              <svg class="w-5 h-5 transition-transform {expandedCards[i] ? 'rotate-180' : ''}" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
              </svg>
            </div>
          </button>

          <!-- Expanded content -->
          {#if expandedCards[i]}
            <div class="border-t border-gray-700 p-4 space-y-5">
              <!-- Strategy signals -->
              <div>
                <h4 class="text-sm font-semibold text-gray-300 mb-3">Inferred Strategies</h4>
                <div class="space-y-2">
                  {#each trader.strategies || [] as sig}
                    {@const sc = getColors(sig.strategy?.label || sig.strategy)}
                    <div class="flex items-center gap-3">
                      <span class="text-xs w-28 {sc.text} font-medium">{sig.strategy?.label || sig.strategy}</span>
                      <div class="flex-1 h-2 bg-gray-700 rounded-full overflow-hidden">
                        <div class="h-full {sc.bar} rounded-full" style="width: {(sig.confidence * 100).toFixed(0)}%"></div>
                      </div>
                      <span class="text-xs text-gray-400 w-10 text-right">{(sig.confidence * 100).toFixed(0)}%</span>
                    </div>
                    <p class="text-xs text-gray-500 ml-[7.5rem]">{sig.evidence}</p>
                  {/each}
                </div>
              </div>

              <!-- Metrics grid -->
              <div>
                <h4 class="text-sm font-semibold text-gray-300 mb-3">Trading Metrics</h4>
                <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-3">
                  {#each [
                    { label: 'Trades', value: trader.metrics?.tradeCount || trader.metrics?.trade_count || 0 },
                    { label: 'Trades/Day', value: fmt(trader.metrics?.tradeFrequencyPerDay || trader.metrics?.trade_frequency_per_day, 1) },
                    { label: 'Markets', value: trader.metrics?.uniqueMarkets || trader.metrics?.unique_markets || 0 },
                    { label: 'Win Rate', value: `${fmt(trader.metrics?.winRate || trader.metrics?.win_rate, 1)}%` },
                    { label: 'Avg Position', value: fmtMoney(trader.metrics?.avgPositionSize || trader.metrics?.avg_position_size) },
                    { label: 'Avg Entry', value: fmt(trader.metrics?.avgEntryPrice || trader.metrics?.avg_entry_price, 3) },
                    { label: 'Buy/Sell Ratio', value: fmt(trader.metrics?.buySellRatio || trader.metrics?.buy_sell_ratio, 2) },
                    { label: 'Top 3 Conc.', value: `${fmt(trader.metrics?.concentrationTop3 || trader.metrics?.concentration_top3, 1)}%` },
                  ] as m}
                    <div class="bg-gray-900/50 rounded-lg p-2.5">
                      <div class="text-xs text-gray-500">{m.label}</div>
                      <div class="text-sm font-semibold text-white mt-0.5">{m.value}</div>
                    </div>
                  {/each}
                </div>
              </div>

              <!-- Top Positions -->
              {#if trader.topPositions?.length || trader.top_positions?.length}
                {@const positions = trader.topPositions || trader.top_positions || []}
                <div>
                  <h4 class="text-sm font-semibold text-gray-300 mb-3">Top Positions</h4>
                  <div class="overflow-x-auto">
                    <table class="w-full text-xs">
                      <thead>
                        <tr class="text-gray-500 border-b border-gray-700">
                          <th class="text-left py-1.5 px-2">Market</th>
                          <th class="text-left py-1.5 px-2">Outcome</th>
                          <th class="text-right py-1.5 px-2">Size</th>
                          <th class="text-right py-1.5 px-2">Avg Price</th>
                          <th class="text-right py-1.5 px-2">Cash PnL</th>
                        </tr>
                      </thead>
                      <tbody>
                        {#each positions as pos}
                          <tr class="border-b border-gray-700/50">
                            <td class="py-1.5 px-2 text-gray-300 max-w-[200px] truncate">{pos.title || '-'}</td>
                            <td class="py-1.5 px-2 text-gray-400">{pos.outcome || '-'}</td>
                            <td class="py-1.5 px-2 text-right text-gray-300">{fmtMoney(pos.size)}</td>
                            <td class="py-1.5 px-2 text-right text-gray-300">{fmt(pos.avgPrice || pos.avg_price, 3)}</td>
                            <td class="py-1.5 px-2 text-right font-medium
                              {(pos.cashPnl || pos.cash_pnl || 0) >= 0 ? 'text-emerald-400' : 'text-red-400'}">
                              {fmtMoney(pos.cashPnl || pos.cash_pnl)}
                            </td>
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                  </div>
                </div>
              {/if}

              <!-- Recent Trades -->
              {#if trader.recentTrades?.length || trader.recent_trades?.length}
                {@const trades = trader.recentTrades || trader.recent_trades || []}
                <div>
                  <h4 class="text-sm font-semibold text-gray-300 mb-3">Recent Trades</h4>
                  <div class="overflow-x-auto">
                    <table class="w-full text-xs">
                      <thead>
                        <tr class="text-gray-500 border-b border-gray-700">
                          <th class="text-left py-1.5 px-2">Side</th>
                          <th class="text-left py-1.5 px-2">Market</th>
                          <th class="text-left py-1.5 px-2">Outcome</th>
                          <th class="text-right py-1.5 px-2">Size</th>
                          <th class="text-right py-1.5 px-2">Price</th>
                        </tr>
                      </thead>
                      <tbody>
                        {#each trades as trade}
                          <tr class="border-b border-gray-700/50">
                            <td class="py-1.5 px-2 font-medium {trade.side === 'BUY' ? 'text-emerald-400' : 'text-red-400'}">{trade.side || '-'}</td>
                            <td class="py-1.5 px-2 text-gray-300 max-w-[200px] truncate">{trade.title || '-'}</td>
                            <td class="py-1.5 px-2 text-gray-400">{trade.outcome || '-'}</td>
                            <td class="py-1.5 px-2 text-right text-gray-300">{fmtMoney(trade.size)}</td>
                            <td class="py-1.5 px-2 text-right text-gray-300">{fmt(trade.price, 3)}</td>
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                  </div>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {:else if status === 'Idle'}
    <div class="bg-gray-800 rounded-lg p-12 text-center border border-gray-700">
      <p class="text-gray-400 text-lg mb-2">No analysis yet</p>
      <p class="text-gray-500 text-sm">Click "Analyze Top 10" to fetch and analyze the top Polymarket traders</p>
    </div>
  {/if}

  <!-- ================================================================== -->
  <!-- Trade Watcher Section -->
  <!-- ================================================================== -->
  <div class="border-t border-gray-700 pt-6 mt-6">
    <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 mb-4">
      <div class="flex items-center gap-3">
        <h3 class="text-xl font-bold text-white">Trade Watcher</h3>
        {#if watcherStatus === 'Watching'}
          <span class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-emerald-900/40 text-emerald-400 text-xs font-medium border border-emerald-700">
            <span class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
            LIVE
          </span>
        {/if}
      </div>
      <div class="flex items-center gap-3">
        {#if watcherStatus === 'Watching'}
          <span class="text-xs text-gray-400">Watching {watcherWatchedCount} wallets</span>
          <button
            onclick={handleStopWatcher}
            class="px-4 py-2 rounded-lg font-semibold text-sm bg-red-600 hover:bg-red-500 text-white transition-colors"
          >
            Stop Watcher
          </button>
        {:else}
          <button
            onclick={handleStartWatcher}
            disabled={results.length === 0}
            class="px-4 py-2 rounded-lg font-semibold text-sm transition-colors
              {results.length === 0
                ? 'bg-gray-600 text-gray-400 cursor-not-allowed'
                : 'bg-emerald-600 hover:bg-emerald-500 text-white'}"
            title={results.length === 0 ? 'Run leaderboard analysis first to populate wallets' : 'Start monitoring top trader trades'}
          >
            Start Watcher
          </button>
        {/if}
      </div>
    </div>

    <p class="text-xs text-gray-500 mb-4">
      Polls top trader wallets every 15s for new trades. Requires leaderboard analysis first.
    </p>

    {#if watcherError}
      <div class="bg-red-900/30 border border-red-700 rounded-lg p-3 mb-4">
        <p class="text-red-400 text-sm">{watcherError}</p>
      </div>
    {/if}

    <!-- Alert feed -->
    {#if watcherAlerts.length > 0}
      <div class="space-y-2 max-h-96 overflow-y-auto">
        {#each watcherAlerts as alert, i}
          <div class="bg-gray-800 rounded-lg p-3 border border-gray-700 flex items-center gap-3">
            <span class="flex-shrink-0 text-xs font-bold px-2 py-0.5 rounded
              {alert.side === 'BUY' ? 'bg-emerald-900/50 text-emerald-400' : 'bg-red-900/50 text-red-400'}">
              {alert.side}
            </span>
            <div class="flex-1 min-w-0">
              <span class="text-sm text-white font-medium">[{alert.user_name}]</span>
              <span class="text-sm text-gray-300 ml-1">{fmtMoney(alert.size)} @ {fmt(alert.price, 3)}</span>
              <span class="text-sm text-gray-500 ml-1">— "{alert.title || '?'}"</span>
              {#if alert.outcome}
                <span class="text-xs text-gray-500 ml-1">({alert.outcome})</span>
              {/if}
            </div>
            <span class="flex-shrink-0 text-xs text-gray-500">{fmtTimestamp(alert.timestamp)}</span>
          </div>
        {/each}
      </div>
    {:else if watcherStatus === 'Watching'}
      <div class="bg-gray-800 rounded-lg p-6 text-center border border-gray-700">
        <p class="text-gray-400 text-sm">Waiting for new trades...</p>
        <p class="text-gray-500 text-xs mt-1">Polling every 15 seconds</p>
      </div>
    {:else}
      <div class="bg-gray-800 rounded-lg p-6 text-center border border-gray-700">
        <p class="text-gray-500 text-sm">Watcher is stopped. Start it to monitor top traders' trades in real-time.</p>
      </div>
    {/if}
  </div>
</div>
