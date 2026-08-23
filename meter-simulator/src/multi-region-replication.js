/**
 * Multi-Region Replication Manager
 *
 * Manages cross-region replication state tracking, health monitoring, and
 * failover coordination for the Utility Protocol stack. Emits Prometheus-
 * compatible metrics for replication lag, region health, failover events,
 * and replication throughput.
 *
 * Regions:
 *   us-east-1  — Primary
 *   eu-west-1  — Secondary (hot standby, synchronous replica)
 *   ap-southeast-1 — Tertiary (warm standby, async replica)
 *
 * Performance target: all getMetrics() and health queries must complete
 * synchronously and stay well within the 100 ms P99 critical-path budget.
 */

'use strict';

/** Region role constants. */
const REGION_ROLE = Object.freeze({
  PRIMARY: 'primary',
  SECONDARY: 'secondary',
  TERTIARY: 'tertiary',
});

/** Region health state constants. */
const HEALTH_STATE = Object.freeze({
  HEALTHY: 'HEALTHY',
  DEGRADED: 'DEGRADED',
  CRITICAL: 'CRITICAL',
  FAILOVER_IN_PROGRESS: 'FAILOVER_IN_PROGRESS',
});

/** RPO target in seconds. */
const RPO_TARGET_SECONDS = 60;

/** RTO target in seconds. */
const RTO_TARGET_SECONDS = 300;

/** Default polling interval in milliseconds. */
const DEFAULT_POLL_INTERVAL_MS = 30_000;

/** Default replication lag threshold triggering DEGRADED state (seconds). */
const LAG_DEGRADED_THRESHOLD_SECONDS = 30;

/** Default replication lag threshold triggering CRITICAL state / RPO breach (seconds). */
const LAG_CRITICAL_THRESHOLD_SECONDS = 60;

const DEFAULT_REGIONS = Object.freeze([
  Object.freeze({ id: 'us-east-1', role: REGION_ROLE.PRIMARY, priority: 1 }),
  Object.freeze({ id: 'eu-west-1', role: REGION_ROLE.SECONDARY, priority: 2 }),
  Object.freeze({ id: 'ap-southeast-1', role: REGION_ROLE.TERTIARY, priority: 3 }),
]);

/**
 * Validates the constructor configuration object.
 * @param {object} config
 */
function validateConfig(config) {
  if (config.rpoTargetSeconds !== undefined) {
    if (!Number.isFinite(config.rpoTargetSeconds) || config.rpoTargetSeconds <= 0) {
      throw new Error('rpoTargetSeconds must be a positive finite number');
    }
  }
  if (config.rtoTargetSeconds !== undefined) {
    if (!Number.isFinite(config.rtoTargetSeconds) || config.rtoTargetSeconds <= 0) {
      throw new Error('rtoTargetSeconds must be a positive finite number');
    }
  }
  if (config.regions !== undefined) {
    if (!Array.isArray(config.regions) || config.regions.length === 0) {
      throw new Error('regions must be a non-empty array');
    }
    const priorities = config.regions.map((r) => r.priority);
    if (new Set(priorities).size !== priorities.length) {
      throw new Error('each region must have a unique priority');
    }
  }
}

class MultiRegionReplicationManager {
  /**
   * @param {object} [options]
   * @param {number} [options.rpoTargetSeconds=60] - RPO target in seconds.
   * @param {number} [options.rtoTargetSeconds=300] - RTO target in seconds.
   * @param {Array<{id: string, role: string, priority: number}>} [options.regions] - Region list.
   * @param {number} [options.pollIntervalMs=30000] - Health poll interval.
   * @param {number} [options.lagDegradedThresholdSeconds=30] - Lag threshold for DEGRADED.
   * @param {number} [options.lagCriticalThresholdSeconds=60] - Lag threshold for CRITICAL.
   * @param {Function} [options.nowFn] - Injectable clock function for testing.
   */
  constructor(options = {}) {
    validateConfig(options);

    this._rpoTargetSeconds = options.rpoTargetSeconds ?? RPO_TARGET_SECONDS;
    this._rtoTargetSeconds = options.rtoTargetSeconds ?? RTO_TARGET_SECONDS;
    this._pollIntervalMs = options.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS;
    this._lagDegradedThresholdSeconds = options.lagDegradedThresholdSeconds ?? LAG_DEGRADED_THRESHOLD_SECONDS;
    this._lagCriticalThresholdSeconds = options.lagCriticalThresholdSeconds ?? LAG_CRITICAL_THRESHOLD_SECONDS;
    this._nowFn = options.nowFn ?? (() => Date.now());

    /** @type {Array<{id: string, role: string, priority: number}>} */
    this._regions = (options.regions ?? DEFAULT_REGIONS).map((r) => ({ ...r }));

    /** Per-region health state. */
    this._regionHealth = new Map();

    /** Per-region probe timestamps (last successful probe). */
    this._lastProbeTimestamp = new Map();

    /**
     * Replication lag storage: Map<`${source}->${target}`, number (seconds)>.
     */
    this._replicationLag = new Map();

    /** Per-region replication bytes (monotonically increasing counter). */
    this._replicationBytes = new Map();

    /** Failover event counter per route: Map<`${from}->${to}`, number>. */
    this._failoverCount = new Map();

    /** Active failover: null or { fromRegion, toRegion, startedAt }. */
    this._activeFailover = null;

    /** RPO violation counter. */
    this._rpoViolationCount = 0;

    /** RTO observation history: Array<{ fromRegion, toRegion, durationSeconds, timestamp }>. */
    this._rtoHistory = [];

    // Initialise health and lag for all known regions.
    for (const region of this._regions) {
      this._regionHealth.set(region.id, HEALTH_STATE.HEALTHY);
      this._lastProbeTimestamp.set(region.id, this._nowFn());
      this._replicationBytes.set(region.id, 0);
    }

    // Initialise lag for all non-primary region pairs (primary → secondary/tertiary).
    const primary = this._primaryRegion();
    for (const region of this._regions) {
      if (region.id !== primary.id) {
        this._replicationLag.set(this._lagKey(primary.id, region.id), 0);
      }
    }
  }

  // ---------------------------------------------------------------------------
  // Internal helpers
  // ---------------------------------------------------------------------------

  /** @returns {{id: string, role: string, priority: number}} */
  _primaryRegion() {
    return this._regions.find((r) => r.role === REGION_ROLE.PRIMARY) ?? this._regions[0];
  }

  /**
   * @param {string} source
   * @param {string} target
   * @returns {string}
   */
  _lagKey(source, target) {
    return `${source}->${target}`;
  }

  /**
   * Derives a health state from the current replication lag.
   * @param {number} lagSeconds
   * @returns {string}
   */
  _healthFromLag(lagSeconds) {
    if (lagSeconds >= this._lagCriticalThresholdSeconds) return HEALTH_STATE.CRITICAL;
    if (lagSeconds >= this._lagDegradedThresholdSeconds) return HEALTH_STATE.DEGRADED;
    return HEALTH_STATE.HEALTHY;
  }

  // ---------------------------------------------------------------------------
  // Public API
  // ---------------------------------------------------------------------------

  /**
   * Returns all known regions.
   * @returns {Array<{id: string, role: string, priority: number}>}
   */
  getRegions() {
    return this._regions.map((r) => ({ ...r }));
  }

  /**
   * Returns the current health state for a region.
   * @param {string} regionId
   * @returns {string} One of HEALTH_STATE values.
   */
  getRegionHealth(regionId) {
    if (!this._regionHealth.has(regionId)) {
      throw new Error(`Unknown region: ${regionId}`);
    }
    return this._regionHealth.get(regionId);
  }

  /**
   * Returns all region health states.
   * @returns {Array<{regionId: string, health: string, lastProbeTimestamp: number}>}
   */
  getAllRegionHealth() {
    return this._regions.map((r) => ({
      regionId: r.id,
      role: r.role,
      priority: r.priority,
      health: this._regionHealth.get(r.id),
      lastProbeTimestamp: this._lastProbeTimestamp.get(r.id),
    }));
  }

  /**
   * Updates the health state for a region, recording the probe timestamp.
   * @param {string} regionId
   * @param {string} healthState - One of HEALTH_STATE values.
   */
  updateRegionHealth(regionId, healthState) {
    if (!this._regionHealth.has(regionId)) {
      throw new Error(`Unknown region: ${regionId}`);
    }
    if (!Object.values(HEALTH_STATE).includes(healthState)) {
      throw new Error(`Invalid health state: ${healthState}`);
    }
    this._regionHealth.set(regionId, healthState);
    this._lastProbeTimestamp.set(regionId, this._nowFn());
  }

  /**
   * Returns the replication lag in seconds between a source and target region.
   * @param {string} sourceRegion
   * @param {string} targetRegion
   * @returns {number} Lag in seconds.
   */
  getReplicationLag(sourceRegion, targetRegion) {
    const key = this._lagKey(sourceRegion, targetRegion);
    if (!this._replicationLag.has(key)) {
      throw new Error(`No replication path from ${sourceRegion} to ${targetRegion}`);
    }
    return this._replicationLag.get(key);
  }

  /**
   * Updates the replication lag between two regions.
   * Also derives and updates the target region health based on lag.
   * Increments the RPO violation counter when lag exceeds the RPO target.
   * @param {string} sourceRegion
   * @param {string} targetRegion
   * @param {number} lagSeconds - Non-negative lag value.
   */
  updateReplicationLag(sourceRegion, targetRegion, lagSeconds) {
    if (!Number.isFinite(lagSeconds) || lagSeconds < 0) {
      throw new Error('lagSeconds must be a non-negative finite number');
    }
    const key = this._lagKey(sourceRegion, targetRegion);
    if (!this._replicationLag.has(key)) {
      // Auto-register new replication paths.
      this._replicationLag.set(key, lagSeconds);
    } else {
      this._replicationLag.set(key, lagSeconds);
    }

    // Update health of the target region based on lag.
    if (this._regionHealth.has(targetRegion)) {
      const currentHealth = this._regionHealth.get(targetRegion);
      // Do not override FAILOVER_IN_PROGRESS from a lag update.
      if (currentHealth !== HEALTH_STATE.FAILOVER_IN_PROGRESS) {
        this._regionHealth.set(targetRegion, this._healthFromLag(lagSeconds));
      }
    }

    // Track RPO violations.
    if (lagSeconds > this._rpoTargetSeconds) {
      this._rpoViolationCount += 1;
    }
  }

  /**
   * Increments the replication bytes counter for a region.
   * @param {string} regionId
   * @param {number} bytes
   */
  addReplicationBytes(regionId, bytes) {
    if (!Number.isFinite(bytes) || bytes < 0) {
      throw new Error('bytes must be a non-negative finite number');
    }
    if (!this._replicationBytes.has(regionId)) {
      throw new Error(`Unknown region: ${regionId}`);
    }
    this._replicationBytes.set(regionId, this._replicationBytes.get(regionId) + bytes);
  }

  /**
   * Triggers a failover from one region to another.
   * Records the failover event and marks both regions with appropriate states.
   * @param {string} fromRegion - The failing region.
   * @param {string} toRegion - The target region to promote.
   * @returns {{ success: boolean, message: string, failoverKey: string }}
   */
  triggerFailover(fromRegion, toRegion) {
    if (!this._regionHealth.has(fromRegion)) {
      throw new Error(`Unknown source region: ${fromRegion}`);
    }
    if (!this._regionHealth.has(toRegion)) {
      throw new Error(`Unknown target region: ${toRegion}`);
    }
    if (fromRegion === toRegion) {
      throw new Error('fromRegion and toRegion must be different');
    }
    if (this._activeFailover !== null) {
      return {
        success: false,
        message: `Failover already in progress: ${this._activeFailover.fromRegion} → ${this._activeFailover.toRegion}`,
        failoverKey: this._lagKey(fromRegion, toRegion),
      };
    }

    const startedAt = this._nowFn();
    this._activeFailover = { fromRegion, toRegion, startedAt };

    // Mark both regions with FAILOVER_IN_PROGRESS.
    this._regionHealth.set(fromRegion, HEALTH_STATE.FAILOVER_IN_PROGRESS);
    this._regionHealth.set(toRegion, HEALTH_STATE.FAILOVER_IN_PROGRESS);

    const failoverKey = this._lagKey(fromRegion, toRegion);
    const current = this._failoverCount.get(failoverKey) ?? 0;
    this._failoverCount.set(failoverKey, current + 1);

    return {
      success: true,
      message: `Failover initiated from ${fromRegion} to ${toRegion}`,
      failoverKey,
    };
  }

  /**
   * Completes the active failover, recording the observed RTO.
   * @param {boolean} [success=true] - Whether the failover succeeded.
   * @returns {{ rtoSeconds: number, rpoViolated: boolean }}
   */
  completeFailover(success = true) {
    if (this._activeFailover === null) {
      throw new Error('No active failover to complete');
    }

    const { fromRegion, toRegion, startedAt } = this._activeFailover;
    const completedAt = this._nowFn();
    const rtoSeconds = (completedAt - startedAt) / 1000;
    const rpoViolated = rtoSeconds > this._rpoTargetSeconds;

    this._rtoHistory.push({
      fromRegion,
      toRegion,
      durationSeconds: rtoSeconds,
      success,
      timestamp: completedAt,
    });

    if (success) {
      // Demote source, promote target.
      this._regionHealth.set(fromRegion, HEALTH_STATE.CRITICAL);
      this._regionHealth.set(toRegion, HEALTH_STATE.HEALTHY);

      // Flip roles in the region list.
      for (const region of this._regions) {
        if (region.id === fromRegion) region.role = REGION_ROLE.SECONDARY;
        if (region.id === toRegion) region.role = REGION_ROLE.PRIMARY;
      }
    } else {
      this._regionHealth.set(fromRegion, HEALTH_STATE.CRITICAL);
      this._regionHealth.set(toRegion, HEALTH_STATE.DEGRADED);
    }

    this._activeFailover = null;
    return { rtoSeconds, rpoViolated };
  }

  /**
   * Returns the active failover descriptor, or null if none is in progress.
   * @returns {{ fromRegion: string, toRegion: string, startedAt: number } | null}
   */
  getActiveFailover() {
    return this._activeFailover ? { ...this._activeFailover } : null;
  }

  /**
   * Validates that all replication paths are within RPO.
   * @returns {{ valid: boolean, violations: Array<{path: string, lagSeconds: number}> }}
   */
  validateReplication() {
    const violations = [];
    for (const [path, lagSeconds] of this._replicationLag.entries()) {
      if (lagSeconds > this._rpoTargetSeconds) {
        violations.push({ path, lagSeconds });
      }
    }
    return {
      valid: violations.length === 0,
      violations,
    };
  }

  /**
   * Returns Prometheus-compatible metric lines (textfile format).
   * @returns {string}
   */
  getPrometheusMetrics() {
    const lines = [];

    lines.push('# HELP utility_region_health_status Region health status (1 = HEALTHY, 0 = unhealthy).');
    lines.push('# TYPE utility_region_health_status gauge');
    for (const region of this._regions) {
      const health = this._regionHealth.get(region.id);
      const value = health === HEALTH_STATE.HEALTHY ? 1 : 0;
      lines.push(
        `utility_region_health_status{region="${region.id}",role="${region.role}"} ${value}`
      );
    }

    lines.push('# HELP utility_replication_lag_seconds Current replication lag in seconds.');
    lines.push('# TYPE utility_replication_lag_seconds gauge');
    for (const [path, lagSeconds] of this._replicationLag.entries()) {
      const [source, target] = path.split('->');
      lines.push(
        `utility_replication_lag_seconds{source_region="${source}",target_region="${target}"} ${lagSeconds}`
      );
    }

    lines.push('# HELP utility_failover_total Total failover events by route.');
    lines.push('# TYPE utility_failover_total counter');
    for (const [path, count] of this._failoverCount.entries()) {
      const [from, to] = path.split('->');
      lines.push(
        `utility_failover_total{from_region="${from}",to_region="${to}"} ${count}`
      );
    }

    lines.push('# HELP utility_replication_bytes_total Total bytes replicated per region.');
    lines.push('# TYPE utility_replication_bytes_total counter');
    for (const [regionId, bytes] of this._replicationBytes.entries()) {
      lines.push(`utility_replication_bytes_total{region="${regionId}"} ${bytes}`);
    }

    lines.push('# HELP utility_dr_rpo_violation_total Total RPO violations detected.');
    lines.push('# TYPE utility_dr_rpo_violation_total counter');
    lines.push(`utility_dr_rpo_violation_total ${this._rpoViolationCount}`);

    return lines.join('\n') + '\n';
  }

  /**
   * Returns a structured metrics snapshot for programmatic consumption.
   * @returns {object}
   */
  getMetrics() {
    const regionHealthMap = {};
    for (const region of this._regions) {
      regionHealthMap[region.id] = {
        health: this._regionHealth.get(region.id),
        role: region.role,
        priority: region.priority,
        lastProbeTimestamp: this._lastProbeTimestamp.get(region.id),
        replicationBytesTotal: this._replicationBytes.get(region.id),
      };
    }

    const replicationLagMap = {};
    for (const [path, lag] of this._replicationLag.entries()) {
      replicationLagMap[path] = lag;
    }

    const failoverCountMap = {};
    for (const [path, count] of this._failoverCount.entries()) {
      failoverCountMap[path] = count;
    }

    const { valid, violations } = this.validateReplication();

    return {
      regions: regionHealthMap,
      replicationLag: replicationLagMap,
      failoverCount: failoverCountMap,
      rpoViolationCount: this._rpoViolationCount,
      rtoHistory: [...this._rtoHistory],
      replicationValid: valid,
      replicationViolations: violations,
      activeFailover: this.getActiveFailover(),
      config: {
        rpoTargetSeconds: this._rpoTargetSeconds,
        rtoTargetSeconds: this._rtoTargetSeconds,
      },
    };
  }
}

module.exports = {
  MultiRegionReplicationManager,
  REGION_ROLE,
  HEALTH_STATE,
  RPO_TARGET_SECONDS,
  RTO_TARGET_SECONDS,
};
