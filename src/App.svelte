<script>
  import { onDestroy } from 'svelte';
  import Layout from './components/Layout.svelte';
  import Discovery from './pages/Discovery.svelte';
  import KnowledgeBase from './pages/KnowledgeBase.svelte';
  import Optimizer from './pages/Optimizer.svelte';
  import { currentPage, serverHealth } from './lib/stores.js';
  import { checkHealth } from './lib/api.js';

  // Health check on mount and periodic
  async function updateHealth() {
    const result = await checkHealth();
    serverHealth.set(result);
  }
  updateHealth();
  const healthInterval = setInterval(updateHealth, 15000);

  onDestroy(() => {
    clearInterval(healthInterval);
  });
</script>

<Layout>
  {#if $currentPage === 'discovery'}
    <Discovery />
  {:else if $currentPage === 'knowledge'}
    <KnowledgeBase />
  {:else if $currentPage === 'optimizer'}
    <Optimizer />
  {/if}
</Layout>
