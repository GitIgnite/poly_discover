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

// Orderbook backtest status — persists across page changes
export const orderbookStatus = writable({
  running: false,
  status: 'Idle',
  current_step: '',
  markets_fetched: 0,
  features_extracted: 0,
  patterns_found: 0,
});
