<script>
  import { onDestroy } from 'svelte';
  import { BarChart3, Play, Square, Zap, RefreshCw, Trash2 } from 'lucide-svelte';
  import {
    startObBacktest, getObBacktestStatus, cancelObBacktest,
    getObPatterns, getObStats,
    startObCollector, stopObCollector, getObCollectorStatus,
    obCleanup
  } from '../lib/api.js';

  let backtestStatus = $state(null);
  let patterns = $state([]);
  let stats = $state(null);
  let collectorStatus = $state(null);
  let selectedWindow = $state('all');
  let selectedType = $state('all');
  let loading = $state(false);

  const statusSteps = [
    { key: 'Probing', label: 'Probing data sources', color: 'text-blue-400' },
    { key: 'DiscoveringMarkets', label: 'Discovering markets', color: 'text-cyan-400' },
    { key: 'FetchingData', label: 'Fetching price data', color: 'text-yellow-400' },
    { key: 'ExtractingFeatures', label: 'Extracting features', color: 'text-purple-400' },
    { key: 'DetectingPatterns', label: 'Detecting patterns', color: 'text-emerald-400' },
    { key: 'Cleanup', label: 'Cleaning up', color: 'text-gray-400' },
    { key: 'Complete', label: 'Complete', color: 'text-green-400' },
  ];

  async function pollBacktest() {
    const result = await getObBacktestStatus();
    backtestStatus = result;
  }

  async function pollCollector() {
    const result = await getObCollectorStatus();
    collectorStatus = result;
  }

  async function loadPatterns() {
    const params = {};
    if (selectedWindow !== 'all') params.window = selectedWindow;
    params.limit = 100;
    const result = await getObPatterns(params);
    if (result.success !== false) {
      patterns = result.data || [];
    }
  }

  async function loadStats() {
    const result = await getObStats();
    if (result.success !== false) {
      stats = result;
    }
  }

  async function handleStartBacktest() {
    loading = true;
    await startObBacktest();
    // Optimistic UI: show progress immediately before next poll
    backtestStatus = { running: true, status: 'Probing', current_step: 'Starting orderbook backtest...', logs: [] };
    loading = false;
    // Immediate poll to get real status
    await pollBacktest();
  }

  async function handleCancelBacktest() {
    await cancelObBacktest();
  }

  async function handleStartCollector() {
    await startObCollector();
  }

  async function handleStopCollector() {
    await stopObCollector();
  }

  async function handleCleanup() {
    if (confirm('Purger les donnees de prix brutes et les anciens snapshots ?')) {
      const result = await obCleanup('partial');
      if (result.success) {
        loadStats();
        pollBacktest(); // refresh db_state
      }
    }
  }

  async function handleFullReset() {
    if (confirm('ATTENTION : Ceci va supprimer TOUTES les donnees orderbook (marches, prix, features, patterns, snapshots). Continuer ?')) {
      if (confirm('Derniere confirmation : reset complet de toutes les tables orderbook ?')) {
        const result = await obCleanup('full');
        if (result.success) {
          loadStats();
          loadPatterns();
          pollBacktest(); // refresh db_state
        }
      }
    }
  }

  function getAdaptiveButtonText(dbState) {
    if (!dbState || dbState.total_markets === 0) {
      return 'Analyser 1 an de marches BTC 15-min';
    }
    if (dbState.unfetched > 0) {
      return `Reprendre l'analyse (${dbState.unfetched.toLocaleString()} restants)`;
    }
    if (dbState.fetched > 0 && dbState.unfetched === 0) {
      return `Extraire features (${dbState.fetched.toLocaleString()} a traiter)`;
    }
    return 'Mettre a jour (nouveaux marches)';
  }

  // Initial loads
  pollBacktest();
  pollCollector();
  loadPatterns();
  loadStats();

  // Polling intervals
  const backtestInterval = setInterval(pollBacktest, 3000);
  const collectorInterval = setInterval(pollCollector, 5000);

  // Reload patterns when backtest completes
  let prevStatus = null;
  $effect(() => {
    if (backtestStatus?.status === 'Complete' && prevStatus !== 'Complete') {
      loadPatterns();
      loadStats();
    }
    prevStatus = backtestStatus?.status;
  });

  // Reload patterns when window filter changes
  $effect(() => {
    selectedWindow;
    loadPatterns();
  });

  // Auto-scroll log panel when new logs arrive
  $effect(() => {
    if (backtestStatus?.logs?.length) {
      // tick: wait for DOM update, then scroll
      setTimeout(() => {
        const el = document.getElementById('ob-log-panel');
        if (el) el.scrollTop = el.scrollHeight;
      }, 0);
    }
  });

  onDestroy(() => {
    clearInterval(backtestInterval);
    clearInterval(collectorInterval);
  });

  function getAccuracyColor(acc) {
    if (acc >= 0.65) return 'text-green-400';
    if (acc >= 0.55) return 'text-yellow-400';
    return 'text-orange-400';
  }

  function getAccuracyBg(acc) {
    if (acc >= 0.65) return 'bg-green-500/20 border-green-500/30';
    if (acc >= 0.55) return 'bg-yellow-500/20 border-yellow-500/30';
    return 'bg-orange-500/20 border-orange-500/30';
  }

  function getCurrentStepIndex(status) {
    return statusSteps.findIndex(s => s.key === status);
  }

  function filteredPatterns() {
    let result = patterns;
    if (selectedType !== 'all') {
      result = result.filter(p => p.pattern_type === selectedType);
    }
    return result;
  }
</script>

<div class="p-4 md:p-6 space-y-6">
  <!-- Header -->
  <div class="flex items-center gap-3">
    <BarChart3 size={28} class="text-orange-400" />
    <div>
      <h1 class="text-2xl font-bold text-white">Orderbook Backtest</h1>
      <p class="text-sm text-gray-400">Analyse des marches BTC 15-min Polymarket — detection de patterns predictifs</p>
    </div>
  </div>

  <!-- Section 1: Backtest Historique -->
  <div class="bg-gray-800 rounded-xl border border-gray-700 p-5">
    <div class="flex items-center justify-between mb-4">
      <h2 class="text-lg font-semibold text-white">Backtest Historique</h2>
      <div class="flex gap-2">
        {#if backtestStatus?.running}
          <button onclick={handleCancelBacktest}
            class="px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg text-sm flex items-center gap-2">
            <Square size={14} /> Annuler
          </button>
        {:else}
          <button onclick={handleStartBacktest} disabled={loading}
            class="px-4 py-2 bg-orange-600 hover:bg-orange-700 text-white rounded-lg text-sm flex items-center gap-2 disabled:opacity-50">
            <Play size={14} /> {getAdaptiveButtonText(backtestStatus?.db_state)}
          </button>
        {/if}
      </div>
    </div>

    <!-- Progress -->
    {#if backtestStatus?.running || backtestStatus?.status === 'Complete' || backtestStatus?.status === 'Error'}
      <div class="space-y-3">
        <!-- Status steps -->
        <div class="flex flex-wrap gap-2">
          {#each statusSteps as step, i}
            {@const currentIdx = getCurrentStepIndex(backtestStatus?.status)}
            <div class="flex items-center gap-1 text-xs px-2 py-1 rounded-full border
              {i < currentIdx ? 'bg-green-500/20 border-green-500/30 text-green-400' :
               i === currentIdx ? 'bg-orange-500/20 border-orange-500/30 text-orange-400 animate-pulse' :
               'bg-gray-700/50 border-gray-600 text-gray-500'}">
              {#if i < currentIdx}
                ✓
              {:else if i === currentIdx}
                ●
              {:else}
                ○
              {/if}
              {step.label}
            </div>
          {/each}
        </div>

        <!-- Current step detail -->
        {#if backtestStatus?.current_step}
          <p class="text-sm text-gray-300">{backtestStatus.current_step}</p>
        {/if}

        <!-- Counters -->
        <div class="grid grid-cols-2 md:grid-cols-5 gap-3">
          <div class="bg-gray-700/50 rounded-lg p-3 text-center">
            <div class="text-lg font-bold text-cyan-400">{backtestStatus?.markets_discovered?.toLocaleString() || 0}</div>
            <div class="text-[10px] text-gray-400">Marches decouverts</div>
          </div>
          <div class="bg-gray-700/50 rounded-lg p-3 text-center">
            <div class="text-lg font-bold text-yellow-400">{backtestStatus?.markets_fetched?.toLocaleString() || 0}</div>
            <div class="text-[10px] text-gray-400">Donnees fetched</div>
          </div>
          <div class="bg-gray-700/50 rounded-lg p-3 text-center">
            <div class="text-lg font-bold text-purple-400">{backtestStatus?.features_extracted?.toLocaleString() || 0}</div>
            <div class="text-[10px] text-gray-400">Features extraites</div>
          </div>
          <div class="bg-gray-700/50 rounded-lg p-3 text-center">
            <div class="text-lg font-bold text-emerald-400">{backtestStatus?.patterns_found || 0}</div>
            <div class="text-[10px] text-gray-400">Patterns detectes</div>
          </div>
          <div class="bg-gray-700/50 rounded-lg p-3 text-center">
            <div class="text-lg font-bold text-white">{backtestStatus?.data_source || 'N/A'}</div>
            <div class="text-[10px] text-gray-400">Source de donnees</div>
          </div>
        </div>

        <!-- Error -->
        {#if backtestStatus?.error}
          <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-3 text-sm text-red-400">
            {backtestStatus.error}
          </div>
        {/if}

        <!-- Logs -->
        {#if backtestStatus?.logs?.length > 0}
          <div class="bg-gray-900 border border-gray-700 rounded-lg p-3 max-h-[200px] overflow-y-auto font-mono text-xs text-gray-300 space-y-0.5"
               id="ob-log-panel">
            {#each backtestStatus.logs as log}
              <div class="text-gray-400">{log}</div>
            {/each}
          </div>
        {/if}
      </div>
    {:else}
      {#if backtestStatus?.db_state && backtestStatus.db_state.total_markets > 0}
        {@const db = backtestStatus.db_state}
        {@const total = db.total_markets || 1}
        {@const extractedPct = (db.features_extracted / total) * 100}
        {@const actualFetchedOnly = db.fetched > db.features_extracted ? db.fetched - db.features_extracted : 0}
        {@const fetchedOnlyPct = (actualFetchedOnly / total) * 100}
        <!-- DB state counters when idle -->
        <div class="space-y-3">
          <p class="text-sm text-gray-400">
            Base de donnees existante — {db.total_markets.toLocaleString()} marches en DB.
            {#if db.last_step}
              Derniere etape : <span class="text-white">{db.last_step}</span>.
            {/if}
          </p>

          <!-- Progress bar: 3 segments -->
          <div class="w-full bg-gray-700 rounded-full h-3 overflow-hidden flex">
            {#if extractedPct > 0}
              <div class="bg-green-500 h-full transition-all" style="width: {extractedPct}%"
                   title="Analyses : {db.features_extracted.toLocaleString()}"></div>
            {/if}
            {#if fetchedOnlyPct > 0}
              <div class="bg-yellow-500 h-full transition-all" style="width: {fetchedOnlyPct}%"
                   title="Recuperes : {actualFetchedOnly.toLocaleString()}"></div>
            {/if}
          </div>
          <div class="flex gap-4 text-[10px] text-gray-400">
            <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-green-500 inline-block"></span> Analyses ({db.features_extracted.toLocaleString()})</span>
            <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-yellow-500 inline-block"></span> Recuperes ({actualFetchedOnly.toLocaleString()})</span>
            <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-gray-600 inline-block"></span> Non recuperes ({db.unfetched.toLocaleString()})</span>
            <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-emerald-500 inline-block"></span> Patterns ({db.patterns.toLocaleString()})</span>
          </div>

          <!-- Counters grid -->
          <div class="grid grid-cols-2 md:grid-cols-5 gap-3">
            <div class="bg-gray-700/50 rounded-lg p-3 text-center">
              <div class="text-lg font-bold text-cyan-400">{db.total_markets.toLocaleString()}</div>
              <div class="text-[10px] text-gray-400">Total marches</div>
            </div>
            <div class="bg-gray-700/50 rounded-lg p-3 text-center">
              <div class="text-lg font-bold text-gray-400">{db.unfetched.toLocaleString()}</div>
              <div class="text-[10px] text-gray-400">Non recuperes</div>
            </div>
            <div class="bg-gray-700/50 rounded-lg p-3 text-center">
              <div class="text-lg font-bold text-yellow-400">{db.fetched.toLocaleString()}</div>
              <div class="text-[10px] text-gray-400">Recuperes</div>
            </div>
            <div class="bg-gray-700/50 rounded-lg p-3 text-center">
              <div class="text-lg font-bold text-purple-400">{db.features_extracted.toLocaleString()}</div>
              <div class="text-[10px] text-gray-400">Features extraites</div>
            </div>
            <div class="bg-gray-700/50 rounded-lg p-3 text-center">
              <div class="text-lg font-bold text-emerald-400">{db.patterns.toLocaleString()}</div>
              <div class="text-[10px] text-gray-400">Patterns</div>
            </div>
          </div>

          {#if db.data_source}
            <p class="text-[10px] text-gray-500">Source : {db.data_source}</p>
          {/if}
        </div>
      {:else}
        <p class="text-sm text-gray-400">
          Lance l'analyse de ~35 000 marches BTC 15 minutes Polymarket sur 1 an.
          Detecte des patterns dans les premieres 90-120 secondes qui predisent le resultat UP/DOWN.
        </p>
      {/if}
    {/if}
  </div>

  <!-- Global Stats -->
  {#if stats?.market_stats}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
      <div class="bg-gray-800 rounded-xl border border-gray-700 p-4">
        <div class="text-xl font-bold text-white">{stats.market_stats.total_markets?.toLocaleString() || 0}</div>
        <div class="text-xs text-gray-400">Total marches en DB</div>
      </div>
      <div class="bg-gray-800 rounded-xl border border-gray-700 p-4">
        <div class="text-xl font-bold text-white">{stats.market_stats.fetched_markets?.toLocaleString() || 0}</div>
        <div class="text-xs text-gray-400">Marches avec donnees</div>
      </div>
      <div class="bg-gray-800 rounded-xl border border-gray-700 p-4">
        <div class="text-xl font-bold text-white">{stats.market_stats.total_features?.toLocaleString() || 0}</div>
        <div class="text-xs text-gray-400">Features en DB</div>
      </div>
      <div class="bg-gray-800 rounded-xl border border-gray-700 p-4">
        <div class="text-xl font-bold text-white">{stats.market_stats.total_patterns?.toLocaleString() || 0}</div>
        <div class="text-xs text-gray-400">Patterns detectes</div>
      </div>
    </div>
  {/if}

  <!-- Section 2: Patterns detectes -->
  <div class="bg-gray-800 rounded-xl border border-gray-700 p-5">
    <div class="flex items-center justify-between mb-4">
      <h2 class="text-lg font-semibold text-white">Patterns Detectes</h2>
      <div class="flex gap-2">
        <select bind:value={selectedWindow}
          class="px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-white">
          <option value="all">Toutes les fenetres</option>
          <option value="30">30s</option>
          <option value="60">60s</option>
          <option value="90">90s</option>
          <option value="120">120s</option>
          <option value="180">180s</option>
          <option value="300">300s</option>
        </select>
        <select bind:value={selectedType}
          class="px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-white">
          <option value="all">Tous les types</option>
          <option value="univariate">Univariate</option>
          <option value="multivariate">Multivariate</option>
          <option value="sequence">Sequence</option>
        </select>
        <button onclick={loadPatterns}
          class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 rounded-lg text-sm text-white">
          <RefreshCw size={14} />
        </button>
      </div>
    </div>

    {#if filteredPatterns().length > 0}
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-gray-400 border-b border-gray-700">
              <th class="text-left py-2 px-2">Pattern</th>
              <th class="text-left py-2 px-2">Type</th>
              <th class="text-center py-2 px-2">Fenetre</th>
              <th class="text-center py-2 px-2">Direction</th>
              <th class="text-center py-2 px-2">Accuracy</th>
              <th class="text-center py-2 px-2">Precision</th>
              <th class="text-center py-2 px-2">Recall</th>
              <th class="text-center py-2 px-2">F1</th>
              <th class="text-center py-2 px-2">Echantillon</th>
              <th class="text-center py-2 px-2">IC 95%</th>
              <th class="text-center py-2 px-2">Stabilite</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredPatterns() as pattern}
              <tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
                <td class="py-2 px-2">
                  <div class="font-mono text-xs text-white">{pattern.pattern_name}</div>
                  {#if pattern.threshold_json}
                    {@const desc = (() => { try { return JSON.parse(pattern.threshold_json)?.description } catch { return null } })()}
                    {#if desc}
                      <div class="text-[10px] text-gray-500 mt-0.5">{desc}</div>
                    {/if}
                  {/if}
                </td>
                <td class="py-2 px-2">
                  <span class="px-2 py-0.5 rounded text-[10px] font-medium
                    {pattern.pattern_type === 'univariate' ? 'bg-blue-500/20 text-blue-400' :
                     pattern.pattern_type === 'multivariate' ? 'bg-purple-500/20 text-purple-400' :
                     'bg-emerald-500/20 text-emerald-400'}">
                    {pattern.pattern_type}
                  </span>
                </td>
                <td class="py-2 px-2 text-center text-gray-300">{pattern.time_window}s</td>
                <td class="py-2 px-2 text-center">
                  <span class="px-2 py-0.5 rounded text-[10px] font-bold
                    {pattern.direction === 'UP' ? 'bg-green-500/20 text-green-400' : 'bg-red-500/20 text-red-400'}">
                    {pattern.direction}
                  </span>
                </td>
                <td class="py-2 px-2 text-center font-bold {getAccuracyColor(pattern.accuracy)}">
                  {(pattern.accuracy * 100).toFixed(1)}%
                </td>
                <td class="py-2 px-2 text-center text-gray-300">
                  {pattern.precision_pct != null ? (pattern.precision_pct * 100).toFixed(1) + '%' : '-'}
                </td>
                <td class="py-2 px-2 text-center text-gray-300">
                  {pattern.recall_pct != null ? (pattern.recall_pct * 100).toFixed(1) + '%' : '-'}
                </td>
                <td class="py-2 px-2 text-center text-gray-300">
                  {pattern.f1_score != null ? (pattern.f1_score * 100).toFixed(1) + '%' : '-'}
                </td>
                <td class="py-2 px-2 text-center text-gray-300">
                  {pattern.sample_size?.toLocaleString() || '-'}
                </td>
                <td class="py-2 px-2 text-center text-[10px] text-gray-400">
                  {#if pattern.confidence_95_low != null}
                    [{(pattern.confidence_95_low * 100).toFixed(1)}-{(pattern.confidence_95_high * 100).toFixed(1)}%]
                  {:else}
                    -
                  {/if}
                </td>
                <td class="py-2 px-2 text-center">
                  {#if pattern.stability_score != null}
                    <span class="{pattern.stability_score >= 0.85 ? 'text-green-400' : pattern.stability_score >= 0.7 ? 'text-yellow-400' : 'text-red-400'}">
                      {pattern.stability_score.toFixed(2)}
                    </span>
                  {:else}
                    -
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <p class="text-sm text-gray-400 text-center py-8">
        Aucun pattern detecte. Lancez le backtest historique pour analyser les marches.
      </p>
    {/if}
  </div>

  <!-- Section 3: Collecteur Live -->
  <div class="bg-gray-800 rounded-xl border border-gray-700 p-5">
    <div class="flex items-center justify-between mb-4">
      <div class="flex items-center gap-2">
        <h2 class="text-lg font-semibold text-white">Collecteur Live</h2>
        {#if collectorStatus?.running}
          <span class="px-2 py-0.5 rounded-full bg-green-500/20 text-green-400 text-[10px] font-bold animate-pulse">
            LIVE
          </span>
        {/if}
      </div>
      <div class="flex gap-2">
        {#if collectorStatus?.running}
          <button onclick={handleStopCollector}
            class="px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg text-sm flex items-center gap-2">
            <Square size={14} /> Stop
          </button>
        {:else}
          <button onclick={handleStartCollector}
            class="px-4 py-2 bg-emerald-600 hover:bg-emerald-700 text-white rounded-lg text-sm flex items-center gap-2">
            <Zap size={14} /> Start Collector
          </button>
        {/if}
      </div>
    </div>

    {#if collectorStatus?.running || collectorStatus?.snapshots_recorded > 0}
      <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
        <div class="bg-gray-700/50 rounded-lg p-3 text-center">
          <div class="text-lg font-bold text-emerald-400">{collectorStatus?.markets_watched || 0}</div>
          <div class="text-[10px] text-gray-400">Marches surveilles</div>
        </div>
        <div class="bg-gray-700/50 rounded-lg p-3 text-center">
          <div class="text-lg font-bold text-cyan-400">{collectorStatus?.snapshots_recorded?.toLocaleString() || 0}</div>
          <div class="text-[10px] text-gray-400">Snapshots enregistres</div>
        </div>
        <div class="bg-gray-700/50 rounded-lg p-3 text-center">
          <div class="text-sm font-medium text-white truncate">{collectorStatus?.current_market || '-'}</div>
          <div class="text-[10px] text-gray-400">Marche actuel</div>
        </div>
        <div class="bg-gray-700/50 rounded-lg p-3 text-center">
          <div class="text-sm font-medium text-white">{collectorStatus?.last_snapshot_time || '-'}</div>
          <div class="text-[10px] text-gray-400">Dernier snapshot</div>
        </div>
      </div>
      {#if collectorStatus?.error}
        <div class="mt-3 bg-red-500/10 border border-red-500/30 rounded-lg p-3 text-sm text-red-400">
          {collectorStatus.error}
        </div>
      {/if}
    {:else}
      <p class="text-sm text-gray-400">
        Le collecteur se connecte en WebSocket au marche BTC 15-min actif et enregistre les snapshots d'orderbook en temps reel.
      </p>
    {/if}
  </div>

  <!-- DB Size & Cleanup -->
  {#if stats?.db_size}
    <div class="bg-gray-800 rounded-xl border border-gray-700 p-5">
      <div class="flex items-center justify-between mb-3">
        <h2 class="text-lg font-semibold text-white">Base de donnees</h2>
        <div class="flex gap-2">
          <button onclick={handleCleanup}
            class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 rounded-lg text-sm text-white flex items-center gap-2"
            title="Supprime les prix bruts et les anciens snapshots (>30j)">
            <Trash2 size={14} /> Purge prix/snapshots
          </button>
          <button onclick={handleFullReset}
            class="px-3 py-1.5 bg-red-700 hover:bg-red-600 rounded-lg text-sm text-white flex items-center gap-2"
            title="Supprime TOUTES les donnees orderbook (marches, prix, features, patterns, snapshots)">
            <Trash2 size={14} /> Reset complet
          </button>
        </div>
      </div>
      <div class="grid grid-cols-2 md:grid-cols-5 gap-3">
        <div class="bg-gray-700/50 rounded-lg p-3 text-center">
          <div class="text-sm font-bold text-white">{stats.db_size.ob_markets?.toLocaleString() || 0}</div>
          <div class="text-[10px] text-gray-400">ob_markets</div>
        </div>
        <div class="bg-gray-700/50 rounded-lg p-3 text-center">
          <div class="text-sm font-bold text-white">{stats.db_size.ob_market_prices?.toLocaleString() || 0}</div>
          <div class="text-[10px] text-gray-400">ob_market_prices</div>
        </div>
        <div class="bg-gray-700/50 rounded-lg p-3 text-center">
          <div class="text-sm font-bold text-white">{stats.db_size.ob_market_features?.toLocaleString() || 0}</div>
          <div class="text-[10px] text-gray-400">ob_market_features</div>
        </div>
        <div class="bg-gray-700/50 rounded-lg p-3 text-center">
          <div class="text-sm font-bold text-white">{stats.db_size.ob_snapshots?.toLocaleString() || 0}</div>
          <div class="text-[10px] text-gray-400">ob_snapshots</div>
        </div>
        <div class="bg-gray-700/50 rounded-lg p-3 text-center">
          <div class="text-sm font-bold text-white">{stats.db_size.ob_patterns?.toLocaleString() || 0}</div>
          <div class="text-[10px] text-gray-400">ob_patterns</div>
        </div>
      </div>
    </div>
  {/if}
</div>
