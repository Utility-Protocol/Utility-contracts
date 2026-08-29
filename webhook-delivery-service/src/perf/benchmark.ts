/**
 * Webhook ingestion performance benchmark.
 *
 * Measures the end-to-end latency of the critical ingestion path (POST /webhooks -> HTTP 202)
 * against the request-decoupled SLA documented in WEBHOOK_ARCHITECTURE.md (<100ms P99,
 * typically <10ms). The service responds to ingestion immediately and delivers asynchronously,
 * so ingestion latency is the correct critical-path signal for regression detection.
 *
 * The suite runs three phases:
 *   1. warmup  - discard the first batch to let the JIT settle
 *   2. measure - record per-request latency for the sampled batch
 *   3. summarise - compute p50/p95/p99/mean/max and request throughput
 *
 * Run directly:  node dist/perf/benchmark.js  (or `npm run perf`)
 * The JSON output feeds scripts/perf-regression-gate.sh.
 */

import { AddressInfo } from 'net';
import fs from 'fs';
import path from 'path';

export interface PercentileSummary {
  p50: number;
  p95: number;
  p99: number;
  mean: number;
  min: number;
  max: number;
}

export interface BenchmarkMeta {
  suite: string;
  samples: number;
  warmup: number;
  /** Full percentile breakdown, reported for transparency but not used by the gate. */
  percentiles: PercentileSummary;
  env: {
    node: string;
    arch: string;
    platform: string;
  };
}

export interface BenchmarkResult {
  schemaVersion: number;
  generatedAt: string;
  meta: BenchmarkMeta;
  metrics: Record<string, number>;
}

const SUITE = 'webhook-ingestion';

export function percentile(sortedMs: number[], p: number): number {
  if (sortedMs.length === 0) return 0;
  const idx = Math.min(
    sortedMs.length - 1,
    Math.max(0, Math.ceil(p * sortedMs.length) - 1)
  );
  return sortedMs[idx];
}

export function summarizePercentiles(durationsMs: number[]): PercentileSummary {
  const sorted = [...durationsMs].sort((a, b) => a - b);
  const mean = sorted.length > 0 ? sorted.reduce((sum, d) => sum + d, 0) / sorted.length : 0;
  return {
    p50: percentile(sorted, 0.5),
    p95: percentile(sorted, 0.95),
    p99: percentile(sorted, 0.99),
    mean,
    min: sorted.length > 0 ? sorted[0] : 0,
    max: sorted.length > 0 ? sorted[sorted.length - 1] : 0,
  };
}

async function postWebhook(port: number): Promise<void> {
  const res = await fetch(`http://127.0.0.1:${port}/webhooks`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      payload: {
        event: 'low_balance',
        timestamp: Date.now(),
        data: { meter_id: 123456, balance: 42 },
      },
      url: 'https://webhook.receiver.invalid/hook',
      secret: 'perf_secret',
      maxAttempts: 1,
    }),
  });
  // Drain the body to capture the full round trip.
  await res.text();
  if (res.status !== 202) {
    throw new Error(`Unexpected status ${res.status} from /webhooks`);
  }
}

export interface BenchmarkOptions {
  samples?: number;
  warmup?: number;
}

export async function runBenchmarkSuite(
  options: BenchmarkOptions = {}
): Promise<BenchmarkResult> {
  const samples = options.samples ?? 600;
  const warmup = options.warmup ?? 150;

  // Disable the auto-listen and structured-logging side effects that the app enables for
  // non-test environments. We bring up our own server on an ephemeral port below so the
  // benchmark is hermetic and parallel-safe.
  process.env.NODE_ENV = 'test';
  process.env.PORT = '0';

  const { default: app } = await import('../index');
  const server = app.listen(0);
  await new Promise<void>((resolve) => server.once('listening', () => resolve()));
  const { port } = server.address() as AddressInfo;

  try {
    // Phase 1: warmup (discarded).
    for (let i = 0; i < warmup; i++) {
      await postWebhook(port);
    }

    // Phase 2: measure.
    const durations: number[] = [];
    const startWindow = Date.now();
    for (let i = 0; i < samples; i++) {
      const t0 = process.hrtime.bigint();
      await postWebhook(port);
      const t1 = process.hrtime.bigint();
      durations.push(Number(t1 - t0) / 1e6); // ns -> ms
    }
    const windowMs = Date.now() - startWindow;

    const stats = summarizePercentiles(durations);
    const throughputRps = windowMs > 0 ? Math.round((samples / windowMs) * 1000) : 0;

    // Only latency metrics that are stable across environments are exposed for the gate.
    // min/max are intentionally reported (meta.percentiles) but not gated: they are dominated
    // by scheduling noise and a single outlier would otherwise cause spurious failures.
    return {
      schemaVersion: 1,
      generatedAt: new Date().toISOString(),
      meta: {
        suite: SUITE,
        samples,
        warmup,
        percentiles: stats,
        env: {
          node: process.version,
          arch: process.arch,
          platform: process.platform,
        },
      },
      metrics: {
        ingestion_p50_ms: stats.p50,
        ingestion_p95_ms: stats.p95,
        ingestion_p99_ms: stats.p99,
        ingestion_mean_ms: stats.mean,
        ingestion_throughput_rps: throughputRps,
      },
    };
  } finally {
    server.close();
  }
}

function renderTable(result: BenchmarkResult): string {
  const p = result.meta.percentiles;
  const rows: string[] = [
    '# Webhook Ingestion Benchmark',
    '',
    `Suite: ${result.meta.suite}  |  Samples: ${result.meta.samples}  |  Warmup: ${result.meta.warmup}`,
    `Environment: ${result.meta.env.node} / ${result.meta.env.platform}-${result.meta.env.arch}`,
    '',
    '| Metric | Value |',
    '| --- | ---: |',
    `| p50 latency | ${round2(result.metrics.ingestion_p50_ms)} ms |`,
    `| p95 latency | ${round2(result.metrics.ingestion_p95_ms)} ms |`,
    `| p99 latency | ${round2(result.metrics.ingestion_p99_ms)} ms |`,
    `| mean latency | ${round2(result.metrics.ingestion_mean_ms)} ms |`,
    `| max latency | ${round2(p.max)} ms |`,
    `| throughput | ${result.metrics.ingestion_throughput_rps} req/s |`,
    '',
  ];
  return rows.join('\n');
}

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

// ---- CLI entry -------------------------------------------------------------
// eslint-disable-next-line @typescript-eslint/no-var-requires
if (typeof require !== 'undefined' && require.main === module) {
  (async () => {
    const argv = process.argv.slice(2);
    let samples = 600;
    let warmup = 150;
    let outFile = '';
    for (let i = 0; i < argv.length; i++) {
      const flag = argv[i];
      const value = (): string => {
        i++;
        const v = argv[i];
        if (v === undefined) throw new Error(`Missing value for ${flag}`);
        return v;
      };
      if (flag === '--samples') samples = parseInt(value(), 10);
      else if (flag === '--warmup') warmup = parseInt(value(), 10);
      else if (flag === '--out') outFile = value();
      else if (flag === '--help' || flag === '-h') {
        // eslint-disable-next-line no-console
        console.log(
          'Usage: node dist/perf/benchmark.js [--samples N] [--warmup N] [--out <json>]'
        );
        process.exit(0);
      } else {
        throw new Error(`Unknown argument: ${flag}`);
      }
    }

    const result = await runBenchmarkSuite({ samples, warmup });

    // eslint-disable-next-line no-console
    console.log(renderTable(result));
    if (outFile) {
      const absolute = path.resolve(outFile);
      fs.mkdirSync(path.dirname(absolute), { recursive: true });
      fs.writeFileSync(absolute, JSON.stringify(result, null, 2) + '\n');
      // eslint-disable-next-line no-console
      console.log(`\nResults written to ${absolute}`);
    }
    process.exit(0);
  })().catch((err) => {
    // eslint-disable-next-line no-console
    console.error('Benchmark failed:', err);
    process.exit(1);
  });
}