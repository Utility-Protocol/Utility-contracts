/**
 * Disaster Recovery Health Checker
 *
 * Orchestrates cross-region health probes and produces a structured
 * readiness report that operators and automated scripts consume before
 * a failover decision. All checks are synchronous and designed to
 * complete well within the 100 ms P99 critical-path budget.
 *
 * Health states (in ascending severity):
 *   HEALTHY            — all probes pass, replication within RPO
 *   DEGRADED           — one probe slow/failing or lag approaching RPO
 *   CRITICAL           — multiple probes failing or RPO breached
 *   FAILOVER_IN_PROGRESS — a DR failover is actively executing
 */

'use strict';

const { MultiRegionReplicationManager, HEALTH_STATE, RPO_TARGET_SECONDS } = require('./multi-region-replication');

/** How many consecutive critical evaluations trigger a CRITICAL overall report. */
const CRITICAL_CONSECUTIVE_THRESHOLD = 3;

/** P99 latency budget in milliseconds for cross-region probes. */
const CROSS_REGION_LATENCY_BUDGET_MS = 100;

/** Maximum age of a region probe (ms) before it is considered stale. */
const PROBE_STALENESS_THRESHOLD_MS = 90_000;

class DRHealthChecker {
  /**
   * @param {MultiRegionReplicationManager} replicationManager
   * @param {object} [options]
   * @param {number} [options.crossRegionLatencyBudgetMs=100]
   * @param {number} [options.probeStalenessThresholdMs=90000]
   * @param {number} [options.criticalConsecutiveThreshold=3]
   * @param {Function} [options.nowFn] - Injectable clock for testing.
   */
  constructor(replicationManager, options = {}) {
    if (!(replicationManager instanceof MultiRegionReplicationManager)) {
      throw new Error('replicationManager must be a MultiRegionReplicationManager instance');
    }

    this._manager = replicationManager;
    this._latencyBudgetMs = options.crossRegionLatencyBudgetMs ?? CROSS_REGION_LATENCY_BUDGET_MS;
    this._stalenessThresholdMs = options.probeStalenessThresholdMs ?? PROBE_STALENESS_THRESHOLD_MS;
    this._criticalThreshold = options.criticalConsecutiveThreshold ?? CRITICAL_CONSECUTIVE_THRESHOLD;
    this._nowFn = options.nowFn ?? (() => Date.now());

    /** Rolling history of overall health evaluations (for consecutive-critical detection). */
    this._evaluationHistory = [];

    /** Simulated cross-region latency overrides for testing: Map<regionId, number>. */
    this._latencyOverrides = new Map();
  }

  // ---------------------------------------------------------------------------
  // Internal helpers
  // ---------------------------------------------------------------------------

  /**
   * Returns simulated (or overridden) cross-region latency for a region.
   * In production this would be replaced by actual probe timing.
   * @param {string} regionId
   * @returns {number} Latency in milliseconds.
   */
  _getCrossRegionLatencyMs(regionId) {
    return this._latencyOverrides.get(regionId) ?? 0;
  }

  /**
   * Simulates or records a cross-region latency value for testing.
   * @param {string} regionId
   * @param {number} latencyMs
   */
  setLatencyOverride(regionId, latencyMs) {
    this._latencyOverrides.set(regionId, latencyMs);
  }

  /**
   * Clears all latency overrides.
   */
  clearLatencyOverrides() {
    this._latencyOverrides.clear();
  }

  // ---------------------------------------------------------------------------
  // Check methods
  // ---------------------------------------------------------------------------

  /**
   * Checks the health of a single region.
   * @param {string} regionId
   * @returns {{ regionId: string, health: string, latencyMs: number, probeAge: number, stale: boolean, issues: string[] }}
   */
  checkRegion(regionId) {
    const allHealth = this._manager.getAllRegionHealth();
    const regionData = allHealth.find((r) => r.regionId === regionId);
    if (!regionData) {
      throw new Error(`Unknown region: ${regionId}`);
    }

    const issues = [];
    const now = this._nowFn();
    const probeAge = now - regionData.lastProbeTimestamp;
    const stale = probeAge > this._stalenessThresholdMs;
    const latencyMs = this._getCrossRegionLatencyMs(regionId);

    if (stale) {
      issues.push(`Probe data is stale (${Math.round(probeAge / 1000)}s old, threshold ${this._stalenessThresholdMs / 1000}s)`);
    }
    if (latencyMs > this._latencyBudgetMs) {
      issues.push(`Cross-region latency ${latencyMs}ms exceeds budget ${this._latencyBudgetMs}ms`);
    }
    if (regionData.health === HEALTH_STATE.CRITICAL) {
      issues.push(`Region reported CRITICAL health state`);
    }
    if (regionData.health === HEALTH_STATE.DEGRADED) {
      issues.push(`Region reported DEGRADED health state`);
    }
    if (regionData.health === HEALTH_STATE.FAILOVER_IN_PROGRESS) {
      issues.push(`Failover is in progress for this region`);
    }

    return {
      regionId,
      role: regionData.role,
      health: regionData.health,
      latencyMs,
      probeAge,
      stale,
      issues,
    };
  }

  /**
   * Checks replication lag across all known replication paths.
   * @returns {{ valid: boolean, paths: Array<{path: string, lagSeconds: number, withinRPO: boolean}> }}
   */
  checkReplicationLag() {
    const metrics = this._manager.getMetrics();
    const paths = Object.entries(metrics.replicationLag).map(([path, lagSeconds]) => ({
      path,
      lagSeconds,
      withinRPO: lagSeconds <= this._manager._rpoTargetSeconds,
    }));
    const valid = paths.every((p) => p.withinRPO);
    return { valid, paths };
  }

  /**
   * Checks whether a failover to the best available region is feasible.
   * Returns the recommended promotion target if the primary is unhealthy.
   * @returns {{ ready: boolean, primaryHealthy: boolean, recommendedTarget: string | null, reason: string }}
   */
  checkFailoverReadiness() {
    const allHealth = this._manager.getAllRegionHealth();
    const primary = allHealth.find((r) => r.role === 'primary');

    if (!primary) {
      return {
        ready: false,
        primaryHealthy: false,
        recommendedTarget: null,
        reason: 'No primary region found',
      };
    }

    const primaryHealthy = primary.health === HEALTH_STATE.HEALTHY;
    if (primaryHealthy) {
      return {
        ready: true,
        primaryHealthy: true,
        recommendedTarget: null,
        reason: 'Primary region is healthy; no failover needed',
      };
    }

    // Find the best healthy non-primary region by priority.
    const candidates = allHealth
      .filter((r) => r.regionId !== primary.regionId && r.health === HEALTH_STATE.HEALTHY)
      .sort((a, b) => a.priority - b.priority);

    if (candidates.length === 0) {
      return {
        ready: false,
        primaryHealthy: false,
        recommendedTarget: null,
        reason: 'No healthy secondary region available for failover',
      };
    }

    const target = candidates[0];

    // Verify the candidate is within RPO.
    const lagCheck = this.checkReplicationLag();
    const targetPath = lagCheck.paths.find((p) => p.path.includes(target.regionId));
    if (targetPath && !targetPath.withinRPO) {
      return {
        ready: false,
        primaryHealthy: false,
        recommendedTarget: target.regionId,
        reason: `Target region ${target.regionId} replication lag ${targetPath.lagSeconds}s exceeds RPO ${this._manager._rpoTargetSeconds}s`,
      };
    }

    return {
      ready: true,
      primaryHealthy: false,
      recommendedTarget: target.regionId,
      reason: `Ready to failover from ${primary.regionId} to ${target.regionId}`,
    };
  }

  /**
   * Runs all checks and returns a comprehensive report.
   * @returns {object}
   */
  checkAll() {
    const now = this._nowFn();
    const allHealth = this._manager.getAllRegionHealth();
    const regionChecks = allHealth.map((r) => this.checkRegion(r.regionId));
    const lagCheck = this.checkReplicationLag();
    const failoverReadiness = this.checkFailoverReadiness();
    const metrics = this._manager.getMetrics();

    const criticalRegions = regionChecks.filter((r) =>
      r.health === HEALTH_STATE.CRITICAL || r.health === HEALTH_STATE.FAILOVER_IN_PROGRESS
    );
    const degradedRegions = regionChecks.filter((r) => r.health === HEALTH_STATE.DEGRADED);
    const staleRegions = regionChecks.filter((r) => r.stale);

    let overallHealth;
    if (criticalRegions.length > 0 || !lagCheck.valid) {
      overallHealth = HEALTH_STATE.CRITICAL;
    } else if (degradedRegions.length > 0 || staleRegions.length > 0) {
      overallHealth = HEALTH_STATE.DEGRADED;
    } else {
      overallHealth = HEALTH_STATE.HEALTHY;
    }

    this._evaluationHistory.push({ health: overallHealth, timestamp: now });
    // Retain only the last 20 evaluations to bound memory.
    if (this._evaluationHistory.length > 20) {
      this._evaluationHistory.shift();
    }

    const consecutiveCritical = this._countConsecutive(HEALTH_STATE.CRITICAL);

    return {
      timestamp: now,
      overallHealth,
      consecutiveCritical,
      regionChecks,
      lagCheck,
      failoverReadiness,
      activeFailover: metrics.activeFailover,
      rpoViolationCount: metrics.rpoViolationCount,
      rtoHistory: metrics.rtoHistory,
    };
  }

  /**
   * Counts consecutive occurrences of a health state at the end of history.
   * @param {string} state
   * @returns {number}
   */
  _countConsecutive(state) {
    let count = 0;
    for (let i = this._evaluationHistory.length - 1; i >= 0; i--) {
      if (this._evaluationHistory[i].health === state) {
        count += 1;
      } else {
        break;
      }
    }
    return count;
  }

  /**
   * Generates a human-readable report string.
   * @returns {string}
   */
  generateReport() {
    const report = this.checkAll();
    const lines = [
      `=== DR Health Report — ${new Date(report.timestamp).toISOString()} ===`,
      `Overall: ${report.overallHealth}`,
      `Consecutive critical evaluations: ${report.consecutiveCritical}`,
      '',
      '--- Region Health ---',
    ];

    for (const r of report.regionChecks) {
      lines.push(`  ${r.regionId} (${r.role}): ${r.health}`);
      if (r.issues.length > 0) {
        for (const issue of r.issues) {
          lines.push(`    ⚠ ${issue}`);
        }
      }
    }

    lines.push('', '--- Replication Lag ---');
    for (const p of report.lagCheck.paths) {
      const status = p.withinRPO ? '✓' : '✗ RPO VIOLATION';
      lines.push(`  ${p.path}: ${p.lagSeconds.toFixed(1)}s ${status}`);
    }

    lines.push('', '--- Failover Readiness ---');
    lines.push(`  ${report.failoverReadiness.reason}`);
    if (report.failoverReadiness.recommendedTarget) {
      lines.push(`  Recommended target: ${report.failoverReadiness.recommendedTarget}`);
    }

    if (report.activeFailover) {
      lines.push('', '--- Active Failover ---');
      lines.push(`  ${report.activeFailover.fromRegion} → ${report.activeFailover.toRegion}`);
      lines.push(`  Started: ${new Date(report.activeFailover.startedAt).toISOString()}`);
    }

    lines.push('', '--- RPO / RTO Summary ---');
    lines.push(`  RPO violations: ${report.rpoViolationCount}`);
    if (report.rtoHistory.length > 0) {
      const lastRTO = report.rtoHistory[report.rtoHistory.length - 1];
      lines.push(
        `  Last RTO: ${lastRTO.durationSeconds.toFixed(1)}s (${lastRTO.fromRegion} → ${lastRTO.toRegion}, success=${lastRTO.success})`
      );
    }

    return lines.join('\n');
  }

  /**
   * Returns Prometheus-compatible metric lines (textfile format).
   * @returns {string}
   */
  getPrometheusMetrics() {
    const report = this.checkAll();
    const lines = [];

    lines.push('# HELP utility_dr_overall_health Overall DR health (1 = HEALTHY, 0 = degraded/critical).');
    lines.push('# TYPE utility_dr_overall_health gauge');
    lines.push(
      `utility_dr_overall_health ${report.overallHealth === HEALTH_STATE.HEALTHY ? 1 : 0}`
    );

    lines.push('# HELP utility_dr_consecutive_critical_evaluations Consecutive CRITICAL health evaluations.');
    lines.push('# TYPE utility_dr_consecutive_critical_evaluations gauge');
    lines.push(`utility_dr_consecutive_critical_evaluations ${report.consecutiveCritical}`);

    lines.push('# HELP utility_dr_failover_ready Failover readiness (1 = ready, 0 = not ready).');
    lines.push('# TYPE utility_dr_failover_ready gauge');
    lines.push(`utility_dr_failover_ready ${report.failoverReadiness.ready ? 1 : 0}`);

    // Include manager-level metrics as well.
    lines.push(this._manager.getPrometheusMetrics());

    return lines.join('\n') + '\n';
  }
}

module.exports = {
  DRHealthChecker,
  HEALTH_STATE,
  CROSS_REGION_LATENCY_BUDGET_MS,
  PROBE_STALENESS_THRESHOLD_MS,
};
