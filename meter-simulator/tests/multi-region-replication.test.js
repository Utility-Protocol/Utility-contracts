'use strict';

const {
  MultiRegionReplicationManager,
  REGION_ROLE,
  HEALTH_STATE,
  RPO_TARGET_SECONDS,
  RTO_TARGET_SECONDS,
} = require('../src/multi-region-replication');

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeManager(options = {}) {
  let clock = 1_000_000;
  const nowFn = () => clock;
  const advanceClock = (ms) => {
    clock += ms;
  };
  const manager = new MultiRegionReplicationManager({ nowFn, ...options });
  return { manager, advanceClock, nowFn: () => clock };
}

// ---------------------------------------------------------------------------
// Construction and configuration
// ---------------------------------------------------------------------------

describe('MultiRegionReplicationManager — construction', () => {
  test('initialises with default regions', () => {
    const { manager } = makeManager();
    const regions = manager.getRegions();
    expect(regions).toHaveLength(3);
    const ids = regions.map((r) => r.id);
    expect(ids).toContain('us-east-1');
    expect(ids).toContain('eu-west-1');
    expect(ids).toContain('ap-southeast-1');
  });

  test('default primary region is us-east-1', () => {
    const { manager } = makeManager();
    const primary = manager.getRegions().find((r) => r.role === REGION_ROLE.PRIMARY);
    expect(primary.id).toBe('us-east-1');
  });

  test('throws when rpoTargetSeconds is invalid', () => {
    expect(() => makeManager({ rpoTargetSeconds: -1 })).toThrow('rpoTargetSeconds must be a positive finite number');
    expect(() => makeManager({ rpoTargetSeconds: 0 })).toThrow();
    expect(() => makeManager({ rpoTargetSeconds: NaN })).toThrow();
  });

  test('throws when regions have duplicate priorities', () => {
    expect(() =>
      makeManager({
        regions: [
          { id: 'a', role: 'primary', priority: 1 },
          { id: 'b', role: 'secondary', priority: 1 },
        ],
      })
    ).toThrow('unique priority');
  });

  test('accepts custom RPO and RTO targets', () => {
    const { manager } = makeManager({ rpoTargetSeconds: 30, rtoTargetSeconds: 120 });
    expect(manager.getMetrics().config.rpoTargetSeconds).toBe(30);
    expect(manager.getMetrics().config.rtoTargetSeconds).toBe(120);
  });
});

// ---------------------------------------------------------------------------
// Region health tracking
// ---------------------------------------------------------------------------

describe('MultiRegionReplicationManager — region health', () => {
  test('all regions start as HEALTHY', () => {
    const { manager } = makeManager();
    for (const r of manager.getRegions()) {
      expect(manager.getRegionHealth(r.id)).toBe(HEALTH_STATE.HEALTHY);
    }
  });

  test('updateRegionHealth persists the new state', () => {
    const { manager } = makeManager();
    manager.updateRegionHealth('eu-west-1', HEALTH_STATE.DEGRADED);
    expect(manager.getRegionHealth('eu-west-1')).toBe(HEALTH_STATE.DEGRADED);
  });

  test('getAllRegionHealth returns all regions with metadata', () => {
    const { manager } = makeManager();
    const all = manager.getAllRegionHealth();
    expect(all).toHaveLength(3);
    for (const entry of all) {
      expect(entry).toHaveProperty('regionId');
      expect(entry).toHaveProperty('health');
      expect(entry).toHaveProperty('role');
      expect(entry).toHaveProperty('priority');
      expect(entry).toHaveProperty('lastProbeTimestamp');
    }
  });

  test('throws when updating health for unknown region', () => {
    const { manager } = makeManager();
    expect(() => manager.updateRegionHealth('us-west-2', HEALTH_STATE.HEALTHY)).toThrow('Unknown region: us-west-2');
  });

  test('throws when setting invalid health state', () => {
    const { manager } = makeManager();
    expect(() => manager.updateRegionHealth('us-east-1', 'UNKNOWN_STATE')).toThrow('Invalid health state');
  });
});

// ---------------------------------------------------------------------------
// Replication lag
// ---------------------------------------------------------------------------

describe('MultiRegionReplicationManager — replication lag', () => {
  test('initial replication lag is 0 for all paths', () => {
    const { manager } = makeManager();
    expect(manager.getReplicationLag('us-east-1', 'eu-west-1')).toBe(0);
    expect(manager.getReplicationLag('us-east-1', 'ap-southeast-1')).toBe(0);
  });

  test('updateReplicationLag persists lag and derives DEGRADED health', () => {
    const { manager } = makeManager();
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 35);
    expect(manager.getReplicationLag('us-east-1', 'eu-west-1')).toBe(35);
    expect(manager.getRegionHealth('eu-west-1')).toBe(HEALTH_STATE.DEGRADED);
  });

  test('lag >= lagCriticalThresholdSeconds derives CRITICAL health', () => {
    const { manager } = makeManager();
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 60);
    expect(manager.getRegionHealth('eu-west-1')).toBe(HEALTH_STATE.CRITICAL);
  });

  test('lag within thresholds keeps health HEALTHY', () => {
    const { manager } = makeManager();
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 10);
    expect(manager.getRegionHealth('eu-west-1')).toBe(HEALTH_STATE.HEALTHY);
  });

  test('RPO violations are counted when lag exceeds rpoTargetSeconds', () => {
    const { manager } = makeManager({ rpoTargetSeconds: 30 });
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 31);
    expect(manager.getMetrics().rpoViolationCount).toBe(1);
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 29);
    // Lag within RPO — count stays at 1.
    expect(manager.getMetrics().rpoViolationCount).toBe(1);
  });

  test('throws for invalid lag value', () => {
    const { manager } = makeManager();
    expect(() => manager.updateReplicationLag('us-east-1', 'eu-west-1', -1)).toThrow();
    expect(() => manager.updateReplicationLag('us-east-1', 'eu-west-1', NaN)).toThrow();
  });

  test('lag does not override FAILOVER_IN_PROGRESS state', () => {
    const { manager } = makeManager();
    manager.updateRegionHealth('eu-west-1', HEALTH_STATE.FAILOVER_IN_PROGRESS);
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 5);
    expect(manager.getRegionHealth('eu-west-1')).toBe(HEALTH_STATE.FAILOVER_IN_PROGRESS);
  });
});

// ---------------------------------------------------------------------------
// Failover
// ---------------------------------------------------------------------------

describe('MultiRegionReplicationManager — failover', () => {
  test('triggerFailover marks both regions FAILOVER_IN_PROGRESS', () => {
    const { manager } = makeManager();
    const result = manager.triggerFailover('us-east-1', 'eu-west-1');
    expect(result.success).toBe(true);
    expect(manager.getRegionHealth('us-east-1')).toBe(HEALTH_STATE.FAILOVER_IN_PROGRESS);
    expect(manager.getRegionHealth('eu-west-1')).toBe(HEALTH_STATE.FAILOVER_IN_PROGRESS);
  });

  test('triggerFailover increments failover counter', () => {
    const { manager } = makeManager();
    manager.triggerFailover('us-east-1', 'eu-west-1');
    manager.completeFailover();
    manager.triggerFailover('us-east-1', 'eu-west-1');
    manager.completeFailover();
    const metrics = manager.getMetrics();
    expect(metrics.failoverCount['us-east-1->eu-west-1']).toBe(2);
  });

  test('concurrent failover returns failure', () => {
    const { manager } = makeManager();
    manager.triggerFailover('us-east-1', 'eu-west-1');
    const second = manager.triggerFailover('us-east-1', 'ap-southeast-1');
    expect(second.success).toBe(false);
    expect(second.message).toMatch(/already in progress/);
  });

  test('getActiveFailover returns active failover descriptor', () => {
    const { manager, advanceClock } = makeManager();
    advanceClock(1000);
    manager.triggerFailover('us-east-1', 'eu-west-1');
    const active = manager.getActiveFailover();
    expect(active).not.toBeNull();
    expect(active.fromRegion).toBe('us-east-1');
    expect(active.toRegion).toBe('eu-west-1');
  });

  test('completeFailover promotes target and demotes source on success', () => {
    const { manager } = makeManager();
    manager.triggerFailover('us-east-1', 'eu-west-1');
    manager.completeFailover(true);
    expect(manager.getRegionHealth('eu-west-1')).toBe(HEALTH_STATE.HEALTHY);
    expect(manager.getRegionHealth('us-east-1')).toBe(HEALTH_STATE.CRITICAL);
  });

  test('completeFailover swaps region roles on success', () => {
    const { manager } = makeManager();
    manager.triggerFailover('us-east-1', 'eu-west-1');
    manager.completeFailover(true);
    const regions = manager.getRegions();
    const newPrimary = regions.find((r) => r.role === REGION_ROLE.PRIMARY);
    expect(newPrimary.id).toBe('eu-west-1');
  });

  test('completeFailover records RTO in history', () => {
    const { manager, advanceClock } = makeManager();
    manager.triggerFailover('us-east-1', 'eu-west-1');
    advanceClock(120_000); // 120 seconds
    manager.completeFailover(true);
    const metrics = manager.getMetrics();
    expect(metrics.rtoHistory).toHaveLength(1);
    expect(metrics.rtoHistory[0].durationSeconds).toBeCloseTo(120, 0);
    expect(metrics.rtoHistory[0].success).toBe(true);
  });

  test('completeFailover throws when no active failover', () => {
    const { manager } = makeManager();
    expect(() => manager.completeFailover()).toThrow('No active failover');
  });

  test('getActiveFailover returns null after completion', () => {
    const { manager } = makeManager();
    manager.triggerFailover('us-east-1', 'eu-west-1');
    manager.completeFailover();
    expect(manager.getActiveFailover()).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Replication validation
// ---------------------------------------------------------------------------

describe('MultiRegionReplicationManager — validateReplication', () => {
  test('returns valid when all paths are within RPO', () => {
    const { manager } = makeManager();
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 10);
    manager.updateReplicationLag('us-east-1', 'ap-southeast-1', 20);
    const result = manager.validateReplication();
    expect(result.valid).toBe(true);
    expect(result.violations).toHaveLength(0);
  });

  test('returns violations when lag exceeds RPO', () => {
    const { manager } = makeManager({ rpoTargetSeconds: 30 });
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 31);
    const result = manager.validateReplication();
    expect(result.valid).toBe(false);
    expect(result.violations).toHaveLength(1);
    expect(result.violations[0].lagSeconds).toBe(31);
  });
});

// ---------------------------------------------------------------------------
// Replication bytes
// ---------------------------------------------------------------------------

describe('MultiRegionReplicationManager — replication bytes', () => {
  test('addReplicationBytes accumulates correctly', () => {
    const { manager } = makeManager();
    manager.addReplicationBytes('eu-west-1', 1024);
    manager.addReplicationBytes('eu-west-1', 512);
    const metrics = manager.getMetrics();
    expect(metrics.regions['eu-west-1'].replicationBytesTotal).toBe(1536);
  });

  test('throws for negative bytes', () => {
    const { manager } = makeManager();
    expect(() => manager.addReplicationBytes('eu-west-1', -1)).toThrow();
  });

  test('throws for unknown region', () => {
    const { manager } = makeManager();
    expect(() => manager.addReplicationBytes('us-west-2', 100)).toThrow('Unknown region: us-west-2');
  });
});

// ---------------------------------------------------------------------------
// Metrics output
// ---------------------------------------------------------------------------

describe('MultiRegionReplicationManager — getMetrics and getPrometheusMetrics', () => {
  test('getMetrics returns structured snapshot', () => {
    const { manager } = makeManager();
    const metrics = manager.getMetrics();
    expect(metrics).toHaveProperty('regions');
    expect(metrics).toHaveProperty('replicationLag');
    expect(metrics).toHaveProperty('failoverCount');
    expect(metrics).toHaveProperty('rpoViolationCount');
    expect(metrics).toHaveProperty('rtoHistory');
    expect(metrics).toHaveProperty('replicationValid');
    expect(metrics).toHaveProperty('activeFailover');
    expect(metrics).toHaveProperty('config');
  });

  test('getPrometheusMetrics returns valid Prometheus textfile lines', () => {
    const { manager } = makeManager();
    manager.updateReplicationLag('us-east-1', 'eu-west-1', 5);
    const text = manager.getPrometheusMetrics();
    expect(text).toContain('utility_region_health_status');
    expect(text).toContain('utility_replication_lag_seconds');
    expect(text).toContain('utility_failover_total');
    expect(text).toContain('utility_replication_bytes_total');
    expect(text).toContain('utility_dr_rpo_violation_total');
    // All metric lines should either be comments or valid metric entries.
    for (const line of text.trim().split('\n')) {
      if (line.trim() === '') continue;
      if (!line.startsWith('#')) {
        expect(line).toMatch(/^utility_\w+/);
      }
    }
  });

  test('getPrometheusMetrics reflects failover count', () => {
    const { manager } = makeManager();
    manager.triggerFailover('us-east-1', 'eu-west-1');
    manager.completeFailover(true);
    const text = manager.getPrometheusMetrics();
    expect(text).toContain('utility_failover_total{from_region="us-east-1",to_region="eu-west-1"} 1');
  });
});
