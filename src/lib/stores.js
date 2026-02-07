import { writable } from 'svelte/store';

// Current page: 'discovery', 'knowledge', 'optimizer'
export const currentPage = writable('discovery');

// Health status
export const serverHealth = writable({ connected: false, version: null });

// Discovery status — persists across page changes
export const discoveryStatus = writable({
  running: false,
  continuous: false,
  phase: '',
  progress_pct: 0,
  completed: 0,
  total: 0,
  skipped: 0,
  current_cycle: 0,
  total_tested_all_cycles: 0,
  total_new_this_cycle: 0,
  current_strategy: '',
  current_symbol: '',
  best_so_far: [],
  results: [],
});
