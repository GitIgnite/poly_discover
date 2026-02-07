// Poly Discover — API client
// All endpoints are served from the same origin (Axum serves both API and static files)

function getApiBase() {
  if (typeof window !== 'undefined') {
    const host = window.location.host;
    // In dev mode (Vite port 5174), proxy to backend on port 3001
    if (host.includes('5174')) {
      const hostname = window.location.hostname || 'localhost';
      return `http://${hostname}:3001`;
    }
    // In production, same origin
    return window.location.origin;
  }
  return 'http://localhost:3001';
}

async function apiCall(endpoint, options = {}) {
  const url = `${getApiBase()}${endpoint}`;
  const response = await fetch(url, {
    headers: {
      'Content-Type': 'application/json',
      ...options.headers,
    },
    ...options,
  });
  if (!response.ok) {
    throw new Error(`API error: ${response.status}`);
  }
  return response.json();
}

// ============================================================================
// Health
// ============================================================================

export async function checkHealth() {
  try {
    const result = await apiCall('/api/health');
    return { connected: true, version: result.version };
  } catch (_e) {
    return { connected: false, version: null };
  }
}

// ============================================================================
// Discovery Agent
// ============================================================================

export async function startDiscovery(config) {
  try {
    return await apiCall('/api/discover', {
      method: 'POST',
      body: JSON.stringify({ ...config, continuous: true }),
    });
  } catch (e) {
    return { success: false, message: String(e) };
  }
}

export async function cancelDiscovery() {
  try {
    return await apiCall('/api/discover/cancel', { method: 'POST' });
  } catch (e) {
    return { success: false, message: String(e) };
  }
}

export async function getDiscoveryStatus() {
  try {
    return await apiCall('/api/discover/status');
  } catch (e) {
    return { status: 'error', progress_pct: 0, results: [], best_so_far: [] };
  }
}

// ============================================================================
// Knowledge Base
// ============================================================================

export async function getKnowledgeBase(params = {}) {
  try {
    const query = new URLSearchParams();
    if (params.limit) query.set('limit', params.limit);
    if (params.offset) query.set('offset', params.offset);
    if (params.strategy_type) query.set('strategy_type', params.strategy_type);
    if (params.symbol) query.set('symbol', params.symbol);
    if (params.min_win_rate) query.set('min_win_rate', params.min_win_rate);
    if (params.sort_by) query.set('sort_by', params.sort_by);
    const qs = query.toString();
    return await apiCall(`/api/knowledge${qs ? '?' + qs : ''}`);
  } catch (e) {
    return { success: false, data: [], total: 0, error: String(e) };
  }
}

export async function getKnowledgeStats() {
  try {
    return await apiCall('/api/knowledge/stats');
  } catch (e) {
    return { success: false, stats: null, error: String(e) };
  }
}

export async function exportResults(params = {}) {
  try {
    const query = new URLSearchParams();
    if (params.top_n) query.set('top_n', params.top_n);
    if (params.min_win_rate) query.set('min_win_rate', params.min_win_rate);
    const qs = query.toString();
    return await apiCall(`/api/export${qs ? '?' + qs : ''}`);
  } catch (e) {
    return { success: false, error: String(e) };
  }
}

// ============================================================================
// Optimizer
// ============================================================================

export async function startOptimization(config) {
  try {
    return await apiCall('/api/optimize', {
      method: 'POST',
      body: JSON.stringify(config),
    });
  } catch (e) {
    return { success: false, message: String(e) };
  }
}

export async function getOptimizationStatus() {
  try {
    return await apiCall('/api/optimize/status');
  } catch (e) {
    return { status: 'error', progress_pct: 0, results: [] };
  }
}

// ============================================================================
// Binance Klines
// ============================================================================

export async function getBinanceKlines(symbol, interval, startTime, endTime) {
  try {
    let url = `/api/binance/klines?symbol=${symbol}&interval=${interval}`;
    if (startTime) url += `&start_time=${startTime}`;
    if (endTime) url += `&end_time=${endTime}`;
    return await apiCall(url);
  } catch (e) {
    return { success: false, message: String(e) };
  }
}
