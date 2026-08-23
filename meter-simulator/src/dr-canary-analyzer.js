/**
 * DR Canary Analyzer
 *
 * Compares baseline (primary) and canary (secondary/DR) region metrics during
 * a canary rollout of DR configuration changes. Returns a PROMOTE, HOLD, or
 * ROLLBACK decision based on SLO thresholds.
 *
 * Canary stages:
 *   5%  → 25% → 50% → 100%
 *
 * Each stage requires a clean analysis window before promotion. The analyzer
 * emits Prometheus metrics for canary stage tracking and decision history.
 */

'use strict';

/** Canary decision constants. */
const CANARY_DECISION = Object.freeze({
  PROMOTE: 'PROMOTE',
  HOLD: 'HOLD',
  ROLLBACK: 'ROLLBACK',
});

/** Canary stage weights. */
const CANARY_STAGES = Object.freeze([5, 25, 50, 100]);

/** Default SLO thresholds for promotion decisions. */
const DEFAULT_THRESHOLDS = Object.freeze({
  /** Maximum allowed P99 latency in milliseconds. */
  p99LatencyMs: 100,
  /** Minimum availability fraction. */
  availabilityFraction: 0.9999,
  /** Maximum error rate fraction. */
  errorRateFraction: 0.0001,
  /** Maximum replication lag in seconds before blocking promotion. */
  replicationLagSeconds: 60,
  /** Maximum allowed relative P99 regression vs baseline (fraction). */
  p99RegressionTolerance: 0.1,
});

/**
 * Validates a metrics snapshot object.
 * @param {object} metrics
 * @param {string} label - Used in error messages.
 */
function validateMetrics(metrics, label) {
  const required = ['p99LatencyMs', 'availabilityFraction', 'errorRateFraction', 'replicationLagSeconds'];
  for (const field of required) {
    if (!Number.isFinite(metrics[field])) {
      throw new Error(`${label}.${field} must be a finite number`);
    }
  }
  if (metrics.availabilityFraction < 0 || metrics.availabilityFraction > 1) {
    throw new Error(`${label}.availabilityFraction must be between 0 and 1`);
  }
  if (metrics.errorRateFraction < 0 || metrics.errorRateFraction > 1) {
    throw new Error(`${label}.errorRateFraction must be between 0 and 1`);
  }
}

class DRCanaryAnalyzer {
  /**
   * @param {object} [options]
   * @param {object} [options.thresholds] - SLO thresholds (partial override of DEFAULT_THRESHOLDS).
   * @param {number[]} [options.stages] - Ordered canary stage weights.
   * @param {Function} [options.nowFn] - Injectable clock for testing.
   */
  constructor(options = {}) {
    this._thresholds = { ...DEFAULT_THRESHOLDS, ...(options.thresholds ?? {}) };
    this._stages = options.stages ?? [...CANARY_STAGES];
    this._nowFn = options.nowFn ?? (() => Date.now());

    /** Current stage weight (0 = not started). */
    this._currentStage = 0;

    /** Decision history: Array<{ decision, reason, stage, timestamp }>. */
    this._decisionHistory = [];

    /** Counters for each decision type. */
    this._decisionCounts = {
      [CANARY_DECISION.PROMOTE]: 0,
      [CANARY_DECISION.HOLD]: 0,
      [CANARY_DECISION.ROLLBACK]: 0,
    };
  }

  // ---------------------------------------------------------------------------
  // Internal helpers
  // ---------------------------------------------------------------------------

  /**
   * Evaluates a single metrics snapshot against absolute SLO thresholds.
   * @param {object} metrics
   * @returns {string[]} List of violation messages (empty = within SLO).
   */
  _evaluateAbsoluteThresholds(metrics) {
    const violations = [];

    if (metrics.p99LatencyMs > this._thresholds.p99LatencyMs) {
      violations.push(
        `P99 latency ${metrics.p99LatencyMs}ms exceeds threshold ${this._thresholds.p99LatencyMs}ms`
      );
    }
    if (metrics.availabilityFraction < this._thresholds.availabilityFraction) {
      const pct = (metrics.availabilityFraction * 100).toFixed(4);
      const tgt = (this._thresholds.availabilityFraction * 100).toFixed(4);
      violations.push(`Availability ${pct}% below threshold ${tgt}%`);
    }
    if (metrics.errorRateFraction > this._thresholds.errorRateFraction) {
      violations.push(
        `Error rate ${(metrics.errorRateFraction * 100).toFixed(4)}% exceeds threshold ${(this._thresholds.errorRateFraction * 100).toFixed(4)}%`
      );
    }
    if (metrics.replicationLagSeconds > this._thresholds.replicationLagSeconds) {
      violations.push(
        `Replication lag ${metrics.replicationLagSeconds}s exceeds threshold ${this._thresholds.replicationLagSeconds}s`
      );
    }

    return violations;
  }

  /**
   * Evaluates canary metrics relative to a baseline.
   * @param {object} baseline
   * @param {object} canary
   * @returns {string[]} List of regression messages (empty = no regression).
   */
  _evaluateRelativeRegressions(baseline, canary) {
    const regressions = [];

    if (baseline.p99LatencyMs > 0) {
      const regression = (canary.p99LatencyMs - baseline.p99LatencyMs) / baseline.p99LatencyMs;
      if (regression > this._thresholds.p99RegressionTolerance) {
        regressions.push(
          `Canary P99 latency regression ${(regression * 100).toFixed(1)}% relative to baseline (tolerance ${(this._thresholds.p99RegressionTolerance * 100).toFixed(0)}%)`
        );
      }
    }

    return regressions;
  }

  /**
   * Records a decision and increments its counter.
   * @param {string} decision
   * @param {string} reason
   * @param {number} stage
   */
  _recordDecision(decision, reason, stage) {
    this._decisionHistory.push({
      decision,
      reason,
      stage,
      timestamp: this._nowFn(),
    });
    this._decisionCounts[decision] = (this._decisionCounts[decision] ?? 0) + 1;
    // Retain only the last 100 decisions.
    if (this._decisionHistory.length > 100) {
      this._decisionHistory.shift();
    }
  }

  // ---------------------------------------------------------------------------
  // Public API
  // ---------------------------------------------------------------------------

  /**
   * Returns the current canary stage weight.
   * @returns {number}
   */
  getCurrentStage() {
    return this._currentStage;
  }

  /**
   * Returns the next stage weight, or null if at 100%.
   * @returns {number | null}
   */
  getNextStage() {
    const idx = this._stages.indexOf(this._currentStage);
    if (idx === -1 || idx === this._stages.length - 1) return null;
    return this._stages[idx + 1];
  }

  /**
   * Manually sets the current stage (used by promotion scripts).
   * @param {number} stage
   */
  setCurrentStage(stage) {
    if (!this._stages.includes(stage) && stage !== 0) {
      throw new Error(`Invalid stage: ${stage}. Valid stages: 0, ${this._stages.join(', ')}`);
    }
    this._currentStage = stage;
  }

  /**
   * Compares baseline and canary region metrics.
   * @param {string} baselineRegion - Region ID acting as baseline.
   * @param {string} canaryRegion - Region ID under canary evaluation.
   * @param {object} baselineMetrics - { p99LatencyMs, availabilityFraction, errorRateFraction, replicationLagSeconds }
   * @param {object} canaryMetrics - Same shape as baselineMetrics.
   * @returns {{ decision: string, violations: string[], regressions: string[], summary: string }}
   */
  compareRegionMetrics(baselineRegion, canaryRegion, baselineMetrics, canaryMetrics) {
    validateMetrics(baselineMetrics, 'baselineMetrics');
    validateMetrics(canaryMetrics, 'canaryMetrics');

    const absoluteViolations = this._evaluateAbsoluteThresholds(canaryMetrics);
    const regressions = this._evaluateRelativeRegressions(baselineMetrics, canaryMetrics);

    let decision;
    let summary;

    if (absoluteViolations.length > 0 || regressions.length > 0) {
      decision = CANARY_DECISION.ROLLBACK;
      summary = `Canary (${canaryRegion}) vs baseline (${baselineRegion}): ${absoluteViolations.length} SLO violations, ${regressions.length} regressions — ROLLBACK`;
    } else {
      decision = CANARY_DECISION.PROMOTE;
      summary = `Canary (${canaryRegion}) vs baseline (${baselineRegion}): all SLOs satisfied — PROMOTE`;
    }

    this._recordDecision(decision, summary, this._currentStage);

    return {
      decision,
      violations: absoluteViolations,
      regressions,
      summary,
      baselineRegion,
      canaryRegion,
      stage: this._currentStage,
    };
  }

  /**
   * Evaluates whether the current canary window should be promoted.
   * Returns PROMOTE if recent decisions are all PROMOTE, ROLLBACK if any
   * recent decision is ROLLBACK, HOLD otherwise.
   * @param {number} [requiredConsecutivePromotes=3] - Clean promotes needed.
   * @returns {{ decision: string, reason: string, stage: number, nextStage: number|null }}
   */
  evaluatePromotionCriteria(requiredConsecutivePromotes = 3) {
    const recent = this._decisionHistory.slice(-requiredConsecutivePromotes);

    if (recent.length === 0) {
      const reason = 'No evaluation history; holding';
      this._recordDecision(CANARY_DECISION.HOLD, reason, this._currentStage);
      return { decision: CANARY_DECISION.HOLD, reason, stage: this._currentStage, nextStage: this.getNextStage() };
    }

    if (recent.some((d) => d.decision === CANARY_DECISION.ROLLBACK)) {
      const reason = `Recent ROLLBACK decision detected — promoting is blocked`;
      this._recordDecision(CANARY_DECISION.ROLLBACK, reason, this._currentStage);
      return { decision: CANARY_DECISION.ROLLBACK, reason, stage: this._currentStage, nextStage: null };
    }

    const allPromote = recent.every((d) => d.decision === CANARY_DECISION.PROMOTE);
    if (!allPromote || recent.length < requiredConsecutivePromotes) {
      const reason = `${recent.filter((d) => d.decision === CANARY_DECISION.PROMOTE).length}/${requiredConsecutivePromotes} consecutive PROMOTE decisions; holding`;
      this._recordDecision(CANARY_DECISION.HOLD, reason, this._currentStage);
      return { decision: CANARY_DECISION.HOLD, reason, stage: this._currentStage, nextStage: this.getNextStage() };
    }

    const nextStage = this.getNextStage();
    if (nextStage !== null) {
      this._currentStage = nextStage;
    }
    const reason = `${requiredConsecutivePromotes} consecutive PROMOTE decisions — advancing to stage ${this._currentStage}%`;
    this._recordDecision(CANARY_DECISION.PROMOTE, reason, this._currentStage);
    return { decision: CANARY_DECISION.PROMOTE, reason, stage: this._currentStage, nextStage };
  }

  /**
   * Analyzes a canary window given a set of time-series metric samples.
   * Evaluates each sample and returns an aggregated decision.
   * @param {Array<{ baselineMetrics: object, canaryMetrics: object }>} samples
   * @param {string} [baselineRegion='baseline']
   * @param {string} [canaryRegion='canary']
   * @returns {{ decision: string, sampleCount: number, rollbackCount: number, promoteCount: number, summary: string }}
   */
  analyzeCanaryWindow(samples, baselineRegion = 'baseline', canaryRegion = 'canary') {
    if (!Array.isArray(samples) || samples.length === 0) {
      throw new Error('samples must be a non-empty array');
    }

    let rollbackCount = 0;
    let promoteCount = 0;

    for (const sample of samples) {
      const result = this.compareRegionMetrics(
        baselineRegion,
        canaryRegion,
        sample.baselineMetrics,
        sample.canaryMetrics
      );
      if (result.decision === CANARY_DECISION.ROLLBACK) {
        rollbackCount += 1;
      } else if (result.decision === CANARY_DECISION.PROMOTE) {
        promoteCount += 1;
      }
    }

    const decision =
      rollbackCount > 0 ? CANARY_DECISION.ROLLBACK : CANARY_DECISION.PROMOTE;

    const summary =
      `Window analysis: ${samples.length} samples, ${promoteCount} PROMOTE, ${rollbackCount} ROLLBACK → ${decision}`;

    return {
      decision,
      sampleCount: samples.length,
      rollbackCount,
      promoteCount,
      summary,
      stage: this._currentStage,
    };
  }

  /**
   * Generates a canary report with history and current state.
   * @returns {object}
   */
  generateCanaryReport() {
    return {
      timestamp: this._nowFn(),
      currentStage: this._currentStage,
      nextStage: this.getNextStage(),
      decisionCounts: { ...this._decisionCounts },
      recentDecisions: this._decisionHistory.slice(-10),
      thresholds: { ...this._thresholds },
      stages: [...this._stages],
    };
  }

  /**
   * Returns Prometheus-compatible metric lines for canary state.
   * @returns {string}
   */
  getPrometheusMetrics() {
    const lines = [];

    lines.push('# HELP utility_dr_canary_stage Current canary stage percentage (0 = not started).');
    lines.push('# TYPE utility_dr_canary_stage gauge');
    lines.push(`utility_dr_canary_stage ${this._currentStage}`);

    lines.push('# HELP utility_dr_canary_decision_total Total canary decisions by type.');
    lines.push('# TYPE utility_dr_canary_decision_total counter');
    for (const [decision, count] of Object.entries(this._decisionCounts)) {
      lines.push(`utility_dr_canary_decision_total{decision="${decision.toLowerCase()}"} ${count}`);
    }

    return lines.join('\n') + '\n';
  }
}

module.exports = {
  DRCanaryAnalyzer,
  CANARY_DECISION,
  CANARY_STAGES,
  DEFAULT_THRESHOLDS,
};
