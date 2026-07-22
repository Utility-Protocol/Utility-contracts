import { Registry, Counter, Histogram, Gauge } from 'prom-client';

// Create a custom Prometheus registry
export const registry = new Registry();

// Define metrics
const deliveryAttempts = new Counter({
  name: 'webhook_delivery_attempts_total',
  help: 'Total number of webhook delivery attempts',
  labelNames: ['status', 'attempt', 'host'],
  registers: [registry],
});

const deliveryDuration = new Histogram({
  name: 'webhook_delivery_duration_seconds',
  help: 'Duration of webhook delivery HTTP requests in seconds',
  labelNames: ['status', 'host'],
  buckets: [0.01, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5],
  registers: [registry],
});

const queueSize = new Gauge({
  name: 'webhook_queue_size_current',
  help: 'Current size of the webhook delivery queue',
  registers: [registry],
});

const totalFailures = new Counter({
  name: 'webhook_failures_total',
  help: 'Total number of webhooks that completely failed after all retries or SSRF drops',
  registers: [registry],
});

// Cache variables for UI Dashboard querying without Prometheus integration
const statCache = {
  successCount: 0,
  failureCount: 0,
  totalAttempts: 0,
  durations: [] as number[],
  lastReset: Date.now(),
};

/**
 * Tracks a webhook delivery attempt
 */
export function trackDeliveryAttempt(
  status: number,
  durationSeconds: number,
  attempt: number,
  urlStr?: string
): void {
  let host = 'unknown';
  if (urlStr) {
    try {
      host = new URL(urlStr).host;
    } catch {
      // Ignored
    }
  }

  const statusLabel = status.toString();
  const attemptLabel = attempt.toString();

  // Update Prometheus metrics
  try {
    deliveryAttempts.inc({ status: statusLabel, attempt: attemptLabel, host });
    deliveryDuration.observe({ status: statusLabel, host }, durationSeconds);
  } catch (err) {
    // Suppress registry errors in test/mock environments
  }

  // Update UI cache
  statCache.totalAttempts++;
  if (status >= 200 && status < 300) {
    statCache.successCount++;
  } else {
    statCache.failureCount++;
  }
  statCache.durations.push(durationSeconds);
  if (statCache.durations.length > 1000) {
    statCache.durations.shift();
  }
}

/**
 * Tracks current in-memory queue size
 */
export function trackQueueSize(size: number): void {
  try {
    queueSize.set(size);
  } catch {
    // Ignored
  }
}

/**
 * Tracks absolute failure / dropped webhook
 */
export function trackFailure(): void {
  try {
    totalFailures.inc();
  } catch {
    // Ignored
  }
}

/**
 * Reset metric caches (primarily for testing)
 */
export function resetMetricCache(): void {
  statCache.successCount = 0;
  statCache.failureCount = 0;
  statCache.totalAttempts = 0;
  statCache.durations = [];
  statCache.lastReset = Date.now();
}

/**
 * Get aggregated dashboard statistics (e.g., success rates, avg latency, p99 latency)
 */
export function getStatsSummary() {
  const count = statCache.durations.length;
  const sorted = [...statCache.durations].sort((a, b) => a - b);

  const avg = count > 0
    ? statCache.durations.reduce((sum, d) => sum + d, 0) / count
    : 0;

  const p95 = count > 0 ? sorted[Math.floor(count * 0.95)] : 0;
  const p99 = count > 0 ? sorted[Math.floor(count * 0.99)] : 0;

  const successRate = statCache.totalAttempts > 0
    ? (statCache.successCount / statCache.totalAttempts) * 100
    : 100;

  return {
    successCount: statCache.successCount,
    failureCount: statCache.failureCount,
    totalAttempts: statCache.totalAttempts,
    averageLatencyMs: Math.round(avg * 1000),
    p95LatencyMs: Math.round(p95 * 1000),
    p99LatencyMs: Math.round(p99 * 1000),
    successRate: Math.round(successRate * 100) / 100,
    uptimeSeconds: Math.floor((Date.now() - statCache.lastReset) / 1000),
  };
}

/**
 * Serializes metrics to Prometheus exposition format
 */
export async function getPrometheusMetrics(): Promise<string> {
  return registry.metrics();
}
