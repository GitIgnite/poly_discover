<script>
  import Sidebar from './Sidebar.svelte';

  let { children } = $props();
</script>

<div class="flex min-h-screen bg-gray-900 text-white">
  <Sidebar />
  <main class="flex-1 p-6 overflow-auto">
    {@render children()}
  </main>
</div>
