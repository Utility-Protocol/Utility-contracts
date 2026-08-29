/**
 * Performance baseline comparison / regression gate.
 *
 * Pure, dependency-free module shared between the CLI gate (`node dist/perf/compare-results.js`)
 * and the Jest unit tests. It compares a fresh benchmark run against a committed baseline and
 * decides, per metric, whether the change is a PASS (within tolerance of baseline), a FAIL
 * (absolute SLO breach or regression beyond tolerance), or an informational WARN (metric
 * measured but no baseline recorded).
 */

export type MetricDirection = 'lower' | 'higher';
export type MetricStatus = 'pass' | 'warn' | 'fail';

export interface BaselineEntry {
  /** Committed reference value for this metric (same units as the current measurement). */
  value: number;
  /** Absolute upper SLO, in the same units (e.g. ms). Only enforced for `lower` metrics. */
  slaMs?: number;
  /** Which direction is "good". Defaults to `lower` (latency). Use `higher` for throughput. */
  direction?: MetricDirection;
}

export interface BaselineFile {
  schemaVersion?: number;
  generatedAt?: string;
  note?: string;
  baselines: Record<string, BaselineEntry>;
}

export interface CurrentResult {
  metrics: Record<string, number>;
}

export interface Comparison {
  metric: string;
  current: number | null;
  baseline: number | null;
  slaMs: number | null;
  direction: MetricDirection;
  deltaPercent: number | null;
  status: MetricStatus;
  message: string;
}

export interface CompareOptions {
  baselines: Record<string, BaselineEntry>;
  current: Record<string, number>;
  /** Fail when a metric drifts beyond this percentage of its baseline. Default 30. */
  regressionTolerancePercent?: number;
}

export interface CompareSummary {
  status: MetricStatus;
  failures: Comparison[];
  warnings: Comparison[];
  passes: Comparison[];
}

export const DEFAULT_REGRESSION_TOLERANCE_PERCENT = 30;

/**
 * Compare a fresh run's metrics against the baseline and classify each metric.
 */
export function compareResults(options: CompareOptions): Comparison[] {
  const tolerance = options.regressionTolerancePercent ?? DEFAULT_REGRESSION_TOLERANCE_PERCENT;
  const metrics = Array.from(
    new Set([...Object.keys(options.baselines), ...Object.keys(options.current)])
  ).sort();

  return metrics.map<Comparison>((metric) => {
    const entry: BaselineEntry | undefined = options.baselines[metric];
    const baseline = entry?.value ?? null;
    const slaMs = entry?.slaMs ?? null;
    const direction = entry?.direction ?? 'lower';
    const current = options.current[metric];

    // A metric that neither the baseline nor the run defines should never reach here, but
    // guard anyway for robustness.
    if (current === undefined) {
      return {
        metric,
        current: null,
        baseline,
        slaMs,
        direction,
        deltaPercent: null,
        status: 'fail',
        message: `${metric} was not measured in the current run.`,
      };
    }

    let deltaPercent: number | null = null;
    if (baseline !== null && baseline !== 0) {
      deltaPercent = ((current - baseline) / baseline) * 100;
    }

    // 1) Absolute SLO breach. Only meaningful when lower-is-better.
    if (slaMs !== null && direction === 'lower' && current > slaMs) {
      return {
        metric,
        current,
        baseline,
        slaMs,
        direction,
        deltaPercent,
        status: 'fail',
        message: `${metric} is ${round(current)}${unitSymbol(current, slaMs)} exceeding the ${round(
          slaMs
        )}ms SLO.`,
      };
    }

    // 2) Relative regression vs. the committed baseline.
    if (baseline !== null && deltaPercent !== null) {
      const regressed =
        direction === 'lower' ? deltaPercent > tolerance : deltaPercent < -tolerance;

      if (regressed) {
        const arrow = direction === 'lower' ? '▲ regressed ▲' : '▼ regressed ▼';
        return {
          metric,
          current,
          baseline,
          slaMs,
          direction,
          deltaPercent,
          status: 'fail',
          message: `${arrow} ${metric} moved ${signed(deltaPercent)}% from baseline ${round(
            baseline
          )} (beyond ${tolerance}% tolerance)${direction === 'lower' ? '' : ' [higher is better]'}.`,
        };
      }
    }

    // 3) No baseline recorded -> cannot detect a regression; surface as informational warning.
    if (baseline === null) {
      return {
        metric,
        current,
        baseline,
        slaMs,
        direction,
        deltaPercent,
        status: 'warn',
        message: `${metric} (${round(current)}) has no recorded baseline; add one to enable regression detection.`,
      };
    }

    return {
      metric,
      current,
      baseline,
      slaMs,
      direction,
      deltaPercent,
      status: 'pass',
      message: `${metric} = ${round(current)}, within ${tolerance}% of baseline ${round(
        baseline
      )} (${signed(deltaPercent)}%).`,
    };
  });
}

/**
 * Roll up a list of per-metric comparisons into a single summary.
 */
export function summarize(comparisons: Comparison[]): CompareSummary {
  const failures = comparisons.filter((c) => c.status === 'fail');
  const warnings = comparisons.filter((c) => c.status === 'warn');
  const passes = comparisons.filter((c) => c.status === 'pass');
  const status: MetricStatus =
    failures.length > 0 ? 'fail' : warnings.length > 0 ? 'warn' : 'pass';
  return { status, failures, warnings, passes };
}

function round(n: number): number {
  return Math.round(n * 100) / 100;
}

function signed(n: number | null): string {
  if (n === null) return 'n/a';
  return n >= 0 ? `+${round(n)}` : `${round(n)}`;
}

function unitSymbol(current: number, slaMs: number): string {
  return slaMs >= 1000 ? 's' : 'ms';
}

// ---- CLI entry point -------------------------------------------------------

interface CliArgs {
  currentPath: string;
  baselinePath: string;
  tolerance: number;
  updateBaseline: boolean;
  strictWarn: boolean;
}

function parseArgs(argv: string[]): CliArgs {
  const args: CliArgs = {
    currentPath: '',
    baselinePath: '',
    tolerance: DEFAULT_REGRESSION_TOLERANCE_PERCENT,
    updateBaseline: false,
    strictWarn: false,
  };

  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const value = (): string => {
      i++;
      const v = argv[i];
      if (v === undefined) throw new Error(`Missing value for ${flag}`);
      return v;
    };
    switch (flag) {
      case '--current':
        args.currentPath = value();
        break;
      case '--baseline':
        args.baselinePath = value();
        break;
      case '--tolerance':
        args.tolerance = parseFloat(value());
        break;
      case '--update-baseline':
        // Rewrites the --baseline file with the current run's values. Takes no argument.
        args.updateBaseline = true;
        break;
      case '--strict-warn':
        args.strictWarn = true;
        break;
      case '--help':
      case '-h':
        // eslint-disable-next-line no-console
        console.log(USAGE);
        process.exit(0);
        break;
      default:
        throw new Error(`Unknown argument: ${flag}`);
    }
  }
  return args;
}

const USAGE = `Usage: node dist/perf/compare-results.js --current <json> --baseline <json> [flags]

Flags:
  --current <json>   Fresh benchmark results ({ "metrics": { "<name>": <number>, ... } })
  --baseline <json>  Committed baseline file ({ "baselines": { "<name>": {value,slaMs?,direction?} } })
  --tolerance <n>    Regression tolerance percent (default 30)
  --update-baseline  Rewrite the baseline file with the current run's values (must pass)
  --strict-warn      Treat warnings as a non-zero exit code (2)
  --help             Show this help

Exit codes:
  0  all metrics pass
  1  one or more metrics failed (SLO breach or regression)
  2  warnings only and --strict-warn was set
  3  usage / file error
`;

// The side-effectful CLI lives below inside a `require.main === module` guard so importing
// this module for unit tests stays side-effect free. Run via `node dist/perf/compare-results.js`.
// eslint-disable-next-line @typescript-eslint/no-var-requires
if (typeof require !== 'undefined' && require.main === module) {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const fs = require('fs') as typeof import('fs');
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const path = require('path') as typeof import('path');

  function failCli(message: string): never {
    // eslint-disable-next-line no-console
    console.error(`error: ${message}`);
    process.exit(3);
  }

  let args: CliArgs;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (err: any) {
    failCli(err?.message ?? String(err));
  }

  if (!args.currentPath || !args.baselinePath) {
    failCli('Both --current and --baseline are required.\n' + USAGE);
  }

  let currentFile: CurrentResult;
  let baselineFile: BaselineFile;
  try {
    currentFile = JSON.parse(
      fs.readFileSync(path.resolve(args.currentPath), 'utf-8')
    ) as CurrentResult;
    baselineFile = JSON.parse(
      fs.readFileSync(path.resolve(args.baselinePath), 'utf-8')
    ) as BaselineFile;
  } catch (err: any) {
    failCli(`Failed to read input files: ${err?.message ?? String(err)}`);
  }

  const comparisons = compareResults({
    baselines: baselineFile.baselines ?? {},
    current: currentFile.metrics ?? {},
    regressionTolerancePercent: args.tolerance,
  });
  const summary = summarize(comparisons);

  // ---- Rendering -------------------------------------------------------------
  const table: Array<Array<string>> = [
    ['Metric', 'Current', 'Baseline', 'Δ%', 'Status'],
  ];
  for (const c of comparisons) {
    table.push([
      c.metric,
      c.current === null ? 'n/a' : String(round(c.current)),
      c.baseline === null ? 'n/a' : String(round(c.baseline)),
      c.deltaPercent === null ? 'n/a' : `${signed(c.deltaPercent)}`,
      c.status.toUpperCase(),
    ]);
  }

  const widths: number[] = [];
  for (let col = 0; col < table[0].length; col++) {
    widths[col] = Math.max(...table.map((row) => row[col].length));
  }
  // eslint-disable-next-line no-console
  console.log(
    table
      .map((row) => row.map((cell, col) => cell.padEnd(widths[col])).join('  |  '))
      .join('\n')
  );

  if (summary.failures.length + summary.warnings.length > 0) {
    // eslint-disable-next-line no-console
    console.log('\nDetails:');
    for (const c of [...summary.failures, ...summary.warnings]) {
      // eslint-disable-next-line no-console
      console.log(`  [${c.status.toUpperCase()}] ${c.message}`);
    }
  }

  // eslint-disable-next-line no-console
  console.log(
    `\nSummary: ${summary.passes.length} pass, ${summary.warnings.length} warn, ${summary.failures.length} fail (tolerance ${args.tolerance}%).`
  );

  // ---- Baseline refresh (workflow_dispatch) ----------------------------------
  // Rewrites the committed baseline file with the current run's values, preserving each
  // metric's SLO and direction metadata. Only runs when the run passes so a regressing
  // baseline is never silently raised.
  if (args.updateBaseline && summary.status !== 'fail') {
    const clean: Record<string, BaselineEntry> = {};
    for (const [metric, value] of Object.entries(currentFile.metrics)) {
      const previous = baselineFile.baselines?.[metric];
      clean[metric] = {
        value,
        ...(previous?.slaMs !== undefined ? { slaMs: previous.slaMs } : {}),
        ...(previous?.direction !== undefined ? { direction: previous.direction } : {}),
      };
    }
    const updated: BaselineFile = {
      schemaVersion: baselineFile.schemaVersion ?? 1,
      generatedAt: new Date().toISOString(),
      note: baselineFile.note,
      baselines: clean,
    };
    fs.writeFileSync(path.resolve(args.baselinePath), JSON.stringify(updated, null, 2) + '\n');
    // eslint-disable-next-line no-console
    console.log(`\nBaseline updated: ${path.resolve(args.baselinePath)}`);
  }

  if (summary.failures.length > 0) {
    process.exit(1);
  }
  if (summary.warnings.length > 0 && args.strictWarn) {
    process.exit(2);
  }
  process.exit(0);
}