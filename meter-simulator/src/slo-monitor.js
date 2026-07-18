/**
 * Service Level Objective (SLO) monitor with multi-window burn-rate alerts.
 *
 * The monitor is intentionally dependency-free so it can run in the meter
 * simulator, CI smoke checks, and dashboard ingestion jobs without adding
 * critical-path latency. It evaluates availability and latency objectives from
 * pre-aggregated rolling-window measurements.
 */

const DEFAULT_CONFIG = Object.freeze({
  availabilityTarget: 0.9999,
  latencyP99TargetMs: 100,
  objectiveWindowMinutes: 30 * 24 * 60,
  burnRateWindows: Object.freeze([
    Object.freeze({ name: 'fast', minutes: 5, threshold: 14.4, severity: 'page' }),
    Object.freeze({ name: 'slow', minutes: 60, threshold: 6, severity: 'ticket' })
  ])
});

function validateConfig(config) {
  if (config.availabilityTarget <= 0 || config.availabilityTarget >= 1) {
    throw new Error('availabilityTarget must be between 0 and 1');
  }
  if (!Number.isFinite(config.latencyP99TargetMs) || config.latencyP99TargetMs <= 0) {
    throw new Error('latencyP99TargetMs must be greater than zero');
  }
  if (!Number.isFinite(config.objectiveWindowMinutes) || config.objectiveWindowMinutes <= 0) {
    throw new Error('objectiveWindowMinutes must be greater than zero');
  }
  if (!Array.isArray(config.burnRateWindows) || config.burnRateWindows.length === 0) {
    throw new Error('at least one burn-rate window is required');
  }
}

function normalizeWindow(window) {
  return {
    name: window.name,
    minutes: window.minutes,
    threshold: window.threshold,
    severity: window.severity || 'ticket'
  };
}

class SLOMonitor {
  constructor(options = {}) {
    this.config = {
      ...DEFAULT_CONFIG,
      ...options,
      burnRateWindows: (options.burnRateWindows || DEFAULT_CONFIG.burnRateWindows).map(normalizeWindow)
    };
    validateConfig(this.config);
  }

  get errorBudgetRatio() {
    return 1 - this.config.availabilityTarget;
  }

  evaluateWindow(measurement) {
    const totalRequests = measurement.totalRequests || 0;
    const failedRequests = measurement.failedRequests || 0;
    const latencyViolations = measurement.latencyViolations || 0;
    const p99LatencyMs = measurement.p99LatencyMs || 0;
    const badEvents = failedRequests + latencyViolations;
    const observedErrorRatio = totalRequests === 0 ? 0 : badEvents / totalRequests;
    const burnRate = this.errorBudgetRatio === 0 ? 0 : observedErrorRatio / this.errorBudgetRatio;

    return {
      windowName: measurement.windowName,
      minutes: measurement.minutes,
      totalRequests,
      failedRequests,
      latencyViolations,
      badEvents,
      observedErrorRatio,
      burnRate,
      availability: totalRequests === 0 ? 1 : 1 - failedRequests / totalRequests,
      p99LatencyMs,
      availabilityHealthy: totalRequests === 0 || failedRequests / totalRequests <= this.errorBudgetRatio,
      latencyHealthy: p99LatencyMs <= this.config.latencyP99TargetMs
    };
  }

  evaluate(measurements) {
    const byName = new Map(measurements.map((measurement) => [measurement.windowName, measurement]));

    return this.config.burnRateWindows.map((window) => {
      const measurement = byName.get(window.name) || {
        windowName: window.name,
        minutes: window.minutes,
        totalRequests: 0,
        failedRequests: 0,
        latencyViolations: 0,
        p99LatencyMs: 0
      };
      const result = this.evaluateWindow({ ...measurement, windowName: window.name, minutes: window.minutes });
      const alerting = result.burnRate >= window.threshold || !result.latencyHealthy;

      return {
        ...result,
        threshold: window.threshold,
        severity: alerting ? window.severity : 'none',
        alerting,
        dashboardLabels: {
          slo: 'utility-protocol-availability-latency',
          window: window.name,
          severity: alerting ? window.severity : 'none'
        }
      };
    });
  }

  buildAlert(measurements) {
    const evaluations = this.evaluate(measurements);
    const active = evaluations.filter((evaluation) => evaluation.alerting);

    return {
      active: active.length > 0,
      severity: active.some((evaluation) => evaluation.severity === 'page') ? 'page' : active[0]?.severity || 'none',
      objective: {
        availabilityTarget: this.config.availabilityTarget,
        latencyP99TargetMs: this.config.latencyP99TargetMs,
        objectiveWindowMinutes: this.config.objectiveWindowMinutes
      },
      evaluations: active.length > 0 ? active : evaluations
    };
  }
}

module.exports = {
  DEFAULT_CONFIG,
  SLOMonitor
};
