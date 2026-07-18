const crypto = require('crypto');

const DEFAULT_CRITICAL_PATH_BUDGET_MS = 100;

function canonicalize(value) {
  if (Array.isArray(value)) {
    return value.map(canonicalize);
  }
  if (value && typeof value === 'object') {
    return Object.keys(value)
      .sort()
      .reduce((acc, key) => {
        acc[key] = canonicalize(value[key]);
        return acc;
      }, {});
  }
  return value;
}

function stableStringify(value) {
  return JSON.stringify(canonicalize(value));
}

function hashConfig(config) {
  return crypto.createHash('sha256').update(stableStringify(config)).digest('hex');
}

function redactValue(key, value) {
  if (/password|private|secret|token|key/i.test(key)) {
    return '[REDACTED]';
  }
  return value;
}

function flattenConfig(config, prefix = '') {
  if (!config || typeof config !== 'object' || Array.isArray(config)) {
    return { [prefix || 'value']: config };
  }

  return Object.keys(config).reduce((acc, key) => {
    const path = prefix ? `${prefix}.${key}` : key;
    const value = config[key];

    if (value && typeof value === 'object' && !Array.isArray(value)) {
      Object.assign(acc, flattenConfig(value, path));
    } else {
      acc[path] = redactValue(path, value);
    }

    return acc;
  }, {});
}

class RuntimeConfigAuditor {
  constructor(options = {}) {
    this.serviceName = options.serviceName || 'unknown-service';
    this.criticalPathBudgetMs = options.criticalPathBudgetMs || DEFAULT_CRITICAL_PATH_BUDGET_MS;
    this.baseline = null;
  }

  createSnapshot(config, observedAt = new Date()) {
    const started = process.hrtime.bigint();
    const redacted = flattenConfig(config);
    const snapshot = {
      service: this.serviceName,
      observedAt: observedAt.toISOString(),
      hash: hashConfig(redacted),
      values: redacted
    };
    const durationMs = Number(process.hrtime.bigint() - started) / 1_000_000;

    return {
      ...snapshot,
      durationMs,
      withinBudget: durationMs < this.criticalPathBudgetMs
    };
  }

  setBaseline(config, observedAt = new Date()) {
    this.baseline = this.createSnapshot(config, observedAt);
    return this.baseline;
  }

  audit(config, observedAt = new Date()) {
    if (!this.baseline) {
      throw new Error('Runtime configuration baseline has not been set');
    }

    const current = this.createSnapshot(config, observedAt);
    const changes = diffValues(this.baseline.values, current.values);

    return {
      service: this.serviceName,
      baselineHash: this.baseline.hash,
      currentHash: current.hash,
      driftDetected: changes.length > 0,
      changes,
      durationMs: current.durationMs,
      withinBudget: current.withinBudget,
      observedAt: current.observedAt
    };
  }
}

function diffValues(expected, actual) {
  const keys = new Set([...Object.keys(expected), ...Object.keys(actual)]);
  return Array.from(keys)
    .sort()
    .filter((key) => expected[key] !== actual[key])
    .map((key) => ({
      path: key,
      expected: expected[key],
      actual: actual[key]
    }));
}

module.exports = {
  RuntimeConfigAuditor,
  DEFAULT_CRITICAL_PATH_BUDGET_MS,
  flattenConfig,
  hashConfig,
  stableStringify,
  diffValues
};
