import { writable } from 'svelte/store';

// Current page: 'discovery', 'knowledge', 'optimizer'
export const currentPage = writable('discovery');

// Health status
export const serverHealth = writable({ connected: false, version: null });
