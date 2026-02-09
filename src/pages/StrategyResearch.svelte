<script>
  import { onMount } from 'svelte';
  import { getStrategyCatalog } from '../lib/api.js';

  let catalog = $state([]);
  let loading = $state(true);
  let filterCategory = $state('');

  const categoryColors = {
    'edge': { bg: 'bg-purple-500/20', text: 'text-purple-400', border: 'border-purple-500/30' },
    'momentum': { bg: 'bg-blue-500/20', text: 'text-blue-400', border: 'border-blue-500/30' },
    'value': { bg: 'bg-green-500/20', text: 'text-green-400', border: 'border-green-500/30' },
    'market-making': { bg: 'bg-yellow-500/20', text: 'text-yellow-400', border: 'border-yellow-500/30' },
    'mean-reversion': { bg: 'bg-cyan-500/20', text: 'text-cyan-400', border: 'border-cyan-500/30' },
    'arbitrage': { bg: 'bg-orange-500/20', text: 'text-orange-400', border: 'border-orange-500/30' },
  };

  const riskColors = {
    'low': 'text-green-400 bg-green-500/20',
    'medium': 'text-yellow-400 bg-yellow-500/20',
    'high': 'text-red-400 bg-red-500/20',
  };

  function getCategoryStyle(cat) {
    return categoryColors[cat] || { bg: 'bg-gray-500/20', text: 'text-gray-400', border: 'border-gray-500/30' };
  }

  onMount(async () => {
    const result = await getStrategyCatalog();
    if (result.success) {
      catalog = result.data;
    }
    loading = false;
  });

  let backtestable = $derived(catalog.filter(s => s.backtestable));
  let displayOnly = $derived(catalog.filter(s => !s.backtestable));
  let categories = $derived([...new Set(catalog.map(s => s.category))]);

  let filteredBacktestable = $derived(
    filterCategory ? backtestable.filter(s => s.category === filterCategory) : backtestable
  );
  let filteredDisplayOnly = $derived(
    filterCategory ? displayOnly.filter(s => s.category === filterCategory) : displayOnly
  );
</script>

<div class="p-4 md:p-6 max-w-6xl mx-auto">
  <!-- Header -->
  <div class="mb-6">
    <h1 class="text-2xl font-bold text-white mb-1">Strategies Web-Researched</h1>
    <p class="text-sm text-gray-400">Catalogue de strategies specifiques aux marches de prediction Polymarket, decouvertes via recherche internet.</p>
  </div>

  {#if loading}
    <div class="text-center py-12 text-gray-400">Chargement du catalogue...</div>
  {:else}
    <!-- Filter bar -->
    <div class="mb-4 flex items-center gap-3 flex-wrap">
      <span class="text-xs text-gray-400">Filtrer :</span>
      <button onclick={() => filterCategory = ''} class="px-3 py-1 rounded-full text-xs transition-colors {filterCategory === '' ? 'bg-cyan-600 text-white' : 'bg-gray-700 text-gray-300 hover:bg-gray-600'}">
        Toutes ({catalog.length})
      </button>
      {#each categories as cat}
        {@const style = getCategoryStyle(cat)}
        <button onclick={() => filterCategory = filterCategory === cat ? '' : cat} class="px-3 py-1 rounded-full text-xs transition-colors {filterCategory === cat ? 'bg-cyan-600 text-white' : style.bg + ' ' + style.text + ' hover:opacity-80'}">
          {cat}
        </button>
      {/each}
    </div>

    <!-- Stats -->
    <div class="grid grid-cols-3 gap-3 mb-6">
      <div class="bg-gray-800 rounded-lg p-3 border border-gray-700">
        <div class="text-2xl font-bold text-cyan-400">{catalog.length}</div>
        <div class="text-xs text-gray-400">Strategies totales</div>
      </div>
      <div class="bg-gray-800 rounded-lg p-3 border border-green-600/30">
        <div class="text-2xl font-bold text-green-400">{backtestable.length}</div>
        <div class="text-xs text-gray-400">Backtestables</div>
      </div>
      <div class="bg-gray-800 rounded-lg p-3 border border-gray-600/30">
        <div class="text-2xl font-bold text-gray-400">{displayOnly.length}</div>
        <div class="text-xs text-gray-400">Display-only</div>
      </div>
    </div>

    <!-- Backtestable Strategies -->
    {#if filteredBacktestable.length > 0}
      <div class="mb-8">
        <div class="flex items-center gap-2 mb-3">
          <h2 class="text-lg font-semibold text-green-400">Strategies Backtestables</h2>
          <span class="px-2 py-0.5 rounded text-[10px] bg-green-500/20 text-green-400">Incluses dans Discovery</span>
        </div>
        <div class="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
          {#each filteredBacktestable as strategy}
            {@const catStyle = getCategoryStyle(strategy.category)}
            <div class="bg-gray-800 rounded-lg p-4 border {catStyle.border} hover:border-opacity-60 transition-colors">
              <div class="flex items-start justify-between mb-2">
                <h3 class="font-bold text-white text-sm">{strategy.name}</h3>
                <div class="flex gap-1.5">
                  <span class="px-1.5 py-0.5 rounded text-[10px] {catStyle.bg} {catStyle.text}">{strategy.category}</span>
                  <span class="px-1.5 py-0.5 rounded text-[10px] {riskColors[strategy.risk_level]}">{strategy.risk_level}</span>
                </div>
              </div>
              <p class="text-xs text-gray-300 mb-3 leading-relaxed">{strategy.description}</p>
              <div class="bg-gray-900/50 rounded p-2 mb-2">
                <p class="text-[10px] text-gray-400 italic">{strategy.rationale}</p>
              </div>
              <div class="flex items-center justify-between">
                <span class="text-[10px] text-green-400">Backtestable</span>
                <a href={strategy.source_url} target="_blank" rel="noopener" class="text-[10px] text-cyan-400 hover:underline">Source</a>
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Display-Only Strategies -->
    {#if filteredDisplayOnly.length > 0}
      <div>
        <div class="flex items-center gap-2 mb-3">
          <h2 class="text-lg font-semibold text-gray-400">Strategies Non-Backtestables</h2>
          <span class="px-2 py-0.5 rounded text-[10px] bg-gray-500/20 text-gray-500">Information uniquement</span>
        </div>
        <div class="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
          {#each filteredDisplayOnly as strategy}
            {@const catStyle = getCategoryStyle(strategy.category)}
            <div class="bg-gray-800/60 rounded-lg p-4 border border-gray-700/50">
              <div class="flex items-start justify-between mb-2">
                <h3 class="font-bold text-gray-300 text-sm">{strategy.name}</h3>
                <div class="flex gap-1.5">
                  <span class="px-1.5 py-0.5 rounded text-[10px] {catStyle.bg} {catStyle.text}">{strategy.category}</span>
                  <span class="px-1.5 py-0.5 rounded text-[10px] {riskColors[strategy.risk_level]}">{strategy.risk_level}</span>
                </div>
              </div>
              <p class="text-xs text-gray-400 mb-3 leading-relaxed">{strategy.description}</p>
              <div class="bg-gray-900/30 rounded p-2">
                <p class="text-[10px] text-gray-500 italic">{strategy.rationale}</p>
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>
