<script>
  import { onDestroy } from 'svelte';
  import Layout from './components/Layout.svelte';
  import Discovery from './pages/Discovery.svelte';
  import KnowledgeBase from './pages/KnowledgeBase.svelte';
  import Optimizer from './pages/Optimizer.svelte';
  import TopStrategies from './pages/TopStrategies.svelte';
  import Playbook from './pages/Playbook.svelte';
  import Leaderboard from './pages/Leaderboard.svelte';
  import StrategyResearch from './pages/StrategyResearch.svelte';
  import ProfileAnalysis from './pages/ProfileAnalysis.svelte';
  import OrderbookAnalysis from './pages/OrderbookAnalysis.svelte';
  import { currentPage, serverHealth, discoveryStatus, orderbookStatus } from './lib/stores.js';
  import { checkHealth, getDiscoveryStatus, getObBacktestStatus } from './lib/api.js';

  // Health check on mount and periodic
  async function updateHealth() {
    const result = await checkHealth();
    serverHealth.set(result);
  }
  updateHealth();
  const healthInterval = setInterval(updateHealth, 15000);

  // Global discovery status polling — persists across page changes
  async function pollDiscovery() {
    const status = await getDiscoveryStatus();
    const isRunning = status.status !== 'idle' && status.status !== 'complete' && status.status !== 'error';

    discoveryStatus.set({
      running: isRunning,
      continuous: status.is_continuous || false,
      phase: status.phase || '',
      progress_pct: status.progress_pct || 0,
      completed: status.completed || 0,
      total: status.total || 0,
      skipped: status.skipped || 0,
      current_cycle: status.current_cycle || 0,
      total_tested_all_cycles: status.total_tested_all_cycles || 0,
      total_new_this_cycle: status.total_new_this_cycle || 0,
      current_strategy: status.current_strategy || '',
      current_symbol: status.current_symbol || '',
      best_so_far: status.best_so_far || [],
      results: status.results || [],
    });
  }

  // Global orderbook backtest status polling
  async function pollOrderbook() {
    const status = await getObBacktestStatus();
    orderbookStatus.set({
      running: status.running || false,
      status: status.status || 'Idle',
      current_step: status.current_step || '',
      markets_fetched: status.markets_fetched || 0,
      features_extracted: status.features_extracted || 0,
      patterns_found: status.patterns_found || 0,
    });
  }

  // Poll immediately on startup, then every 30s
  pollDiscovery();
  pollOrderbook();
  const discoveryInterval = setInterval(pollDiscovery, 30000);
  const orderbookInterval = setInterval(pollOrderbook, 30000);

  onDestroy(() => {
    clearInterval(healthInterval);
    clearInterval(discoveryInterval);
    clearInterval(orderbookInterval);
  });
</script>

<Layout>
  {#if $currentPage === 'discovery'}
    <Discovery />
  {:else if $currentPage === 'knowledge'}
    <KnowledgeBase />
  {:else if $currentPage === 'top-strategies'}
    <TopStrategies />
  {:else if $currentPage === 'playbook'}
    <Playbook />
  {:else if $currentPage === 'optimizer'}
    <Optimizer />
  {:else if $currentPage === 'leaderboard'}
    <Leaderboard />
  {:else if $currentPage === 'strategy-research'}
    <StrategyResearch />
  {:else if $currentPage === 'profile'}
    <ProfileAnalysis />
  {:else if $currentPage === 'orderbook'}
    <OrderbookAnalysis />
  {/if}
</Layout>
