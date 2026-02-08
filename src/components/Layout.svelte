<script>
  import Sidebar from './Sidebar.svelte';

  let { children } = $props();
</script>

<div class="flex min-h-screen bg-gray-900 text-white">
  <Sidebar />
  <main class="flex-1 p-4 md:p-6 overflow-auto max-md:pt-14">
    {@render children()}
  </main>
</div>
