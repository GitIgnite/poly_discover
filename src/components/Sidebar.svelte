<script>
  import { Search, Database, Sparkles } from 'lucide-svelte';
  import { currentPage, serverHealth } from '../lib/stores.js';

  const navItems = [
    { id: 'discovery', label: 'Discovery', icon: Search, color: 'cyan' },
    { id: 'knowledge', label: 'Knowledge Base', icon: Database, color: 'emerald' },
    { id: 'optimizer', label: 'Optimizer', icon: Sparkles, color: 'amber' },
  ];

  const colorMap = {
    cyan: { active: 'bg-cyan-600 text-white', dot: 'bg-cyan-400' },
    emerald: { active: 'bg-emerald-600 text-white', dot: 'bg-emerald-400' },
    amber: { active: 'bg-amber-600 text-white', dot: 'bg-amber-400' },
  };

  function navigate(pageId) {
    currentPage.set(pageId);
  }
</script>

<aside class="w-64 bg-gray-800 min-h-screen flex flex-col">
  <div class="p-4 border-b border-gray-700">
    <h1 class="text-xl font-bold text-cyan-400">Poly Discover</h1>
    <p class="text-xs text-gray-500">Strategy Discovery Agent</p>
  </div>

  <nav class="flex-1 p-4">
    <ul class="space-y-2">
      {#each navItems as item}
        <li>
          <button
            onclick={() => navigate(item.id)}
            class="w-full flex items-center gap-3 px-4 py-3 rounded-lg transition-colors {$currentPage === item.id ? colorMap[item.color].active : 'text-gray-300 hover:bg-gray-700 hover:text-white'}"
          >
            <svelte:component this={item.icon} size={20} />
            <span>{item.label}</span>
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <div class="p-4 border-t border-gray-700">
    <div class="flex items-center gap-2 mb-2">
      {#if $serverHealth.connected}
        <div class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></div>
        <span class="text-xs text-emerald-400">Connected</span>
      {:else}
        <div class="w-2 h-2 rounded-full bg-red-400"></div>
        <span class="text-xs text-red-400">Offline</span>
      {/if}
    </div>
    <p class="text-xs text-gray-500 text-center">v{$serverHealth.version || '...'}</p>
  </div>
</aside>
