<script>
  import { Search, Database, Sparkles, Trophy, BookOpen, PanelLeftClose, PanelLeftOpen, Menu, X } from 'lucide-svelte';
  import { currentPage, serverHealth, discoveryStatus } from '../lib/stores.js';

  let collapsed = $state(false);
  let mobileOpen = $state(false);

  const navItems = [
    { id: 'discovery', label: 'Discovery', icon: Search, color: 'cyan' },
    { id: 'top-strategies', label: 'Top 20', icon: Trophy, color: 'yellow' },
    { id: 'playbook', label: 'Playbook', icon: BookOpen, color: 'violet' },
    { id: 'knowledge', label: 'Knowledge Base', icon: Database, color: 'emerald' },
    { id: 'optimizer', label: 'Optimizer', icon: Sparkles, color: 'amber' },
  ];

  const colorMap = {
    cyan: { active: 'bg-cyan-600 text-white', dot: 'bg-cyan-400' },
    yellow: { active: 'bg-yellow-600 text-white', dot: 'bg-yellow-400' },
    violet: { active: 'bg-violet-600 text-white', dot: 'bg-violet-400' },
    emerald: { active: 'bg-emerald-600 text-white', dot: 'bg-emerald-400' },
    amber: { active: 'bg-amber-600 text-white', dot: 'bg-amber-400' },
  };

  function navigate(pageId) {
    currentPage.set(pageId);
    mobileOpen = false;
  }
</script>

<!-- Mobile hamburger button (visible only on small screens when sidebar is closed) -->
{#if !mobileOpen}
  <button
    onclick={() => mobileOpen = true}
    class="md:hidden fixed top-3 left-3 z-50 p-2 bg-gray-800 rounded-lg text-gray-300 hover:text-white border border-gray-700"
  >
    <Menu size={20} />
  </button>
{/if}

<!-- Mobile overlay backdrop -->
{#if mobileOpen}
  <button
    onclick={() => mobileOpen = false}
    class="md:hidden fixed inset-0 bg-black/50 z-40"
    aria-label="Close menu"
  ></button>
{/if}

<!-- Sidebar -->
<aside
  class="sidebar bg-gray-800 min-h-screen flex flex-col transition-all duration-200 flex-shrink-0"
  class:collapsed
  class:mobile-open={mobileOpen}
>
  <!-- Header -->
  <div class="p-3 border-b border-gray-700 flex items-center" class:justify-center={collapsed} class:justify-between={!collapsed}>
    {#if !collapsed}
      <div class="min-w-0">
        <h1 class="text-lg font-bold text-cyan-400 truncate">Poly Discover</h1>
        <p class="text-[10px] text-gray-500">ML Discovery Agent</p>
      </div>
    {:else}
      <span class="text-lg font-bold text-cyan-400">P</span>
    {/if}

    <!-- Close button on mobile -->
    <button
      onclick={() => mobileOpen = false}
      class="md:hidden text-gray-400 hover:text-white ml-1"
    >
      <X size={18} />
    </button>

    <!-- Collapse toggle on desktop -->
    <button
      onclick={() => collapsed = !collapsed}
      class="hidden md:block text-gray-400 hover:text-white ml-1"
      title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
    >
      {#if collapsed}
        <PanelLeftOpen size={16} />
      {:else}
        <PanelLeftClose size={16} />
      {/if}
    </button>
  </div>

  <nav class="flex-1 p-2">
    <ul class="space-y-1">
      {#each navItems as item}
        <li>
          <button
            onclick={() => navigate(item.id)}
            class="w-full flex items-center gap-2 px-3 py-2 rounded-lg transition-colors text-sm relative {$currentPage === item.id ? colorMap[item.color].active : 'text-gray-300 hover:bg-gray-700 hover:text-white'}"
            class:justify-center={collapsed}
            title={collapsed ? item.label : ''}
          >
            <item.icon size={18} />
            {#if !collapsed}
              <span class="truncate">{item.label}</span>
            {/if}
            {#if item.id === 'discovery' && $discoveryStatus.running}
              {#if collapsed}
                <div class="absolute top-0.5 right-0.5 w-2 h-2 rounded-full bg-cyan-400 animate-pulse"></div>
              {:else}
                <div class="ml-auto w-2 h-2 rounded-full bg-cyan-400 animate-pulse"></div>
              {/if}
            {/if}
          </button>
        </li>
      {/each}
    </ul>

    <!-- Discovery status counter -->
    {#if $discoveryStatus.running}
      <div class="mt-3 px-2 py-2 bg-gray-700/50 rounded-lg">
        {#if collapsed}
          <div class="text-[10px] text-cyan-400 font-bold text-center">{$discoveryStatus.total_tested_all_cycles.toLocaleString()}</div>
        {:else}
          <div class="text-[10px] text-gray-400">Discovery running...</div>
          <div class="text-xs text-cyan-400 font-bold">{$discoveryStatus.total_tested_all_cycles.toLocaleString()} tested</div>
          <div class="text-[10px] text-gray-500">Cycle {$discoveryStatus.current_cycle}</div>
        {/if}
      </div>
    {/if}
  </nav>

  <div class="p-3 border-t border-gray-700">
    <div class="flex items-center gap-2" class:justify-center={collapsed}>
      {#if $serverHealth.connected}
        <div class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></div>
        {#if !collapsed}
          <span class="text-[10px] text-emerald-400">Connected</span>
        {/if}
      {:else}
        <div class="w-2 h-2 rounded-full bg-red-400"></div>
        {#if !collapsed}
          <span class="text-[10px] text-red-400">Offline</span>
        {/if}
      {/if}
    </div>
    {#if !collapsed}
      <p class="text-[10px] text-gray-500 text-center mt-1">v{$serverHealth.version || '...'}</p>
    {/if}
  </div>
</aside>

<style>
  /* Desktop sidebar widths */
  .sidebar {
    width: 14rem; /* w-56 */
  }
  .sidebar.collapsed {
    width: 4rem; /* w-16 */
  }

  /* Mobile: fixed overlay sidebar */
  @media (max-width: 767px) {
    .sidebar {
      position: fixed;
      inset: 0;
      right: auto;
      width: 14rem;
      z-index: 50;
      transform: translateX(-100%);
    }
    .sidebar.mobile-open {
      transform: translateX(0);
    }
    /* On mobile, never show collapsed state */
    .sidebar.collapsed {
      width: 14rem;
    }
  }
</style>
