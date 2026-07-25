const { SLOMonitor } = require('../src/slo-monitor');

describe('SLOMonitor', () => {
  test('keeps healthy windows below alert thresholds', () => {
    const monitor = new SLOMonitor();
    const alert = monitor.buildAlert([
      { windowName: 'fast', totalRequests: 100000, failedRequests: 1, latencyViolations: 2, p99LatencyMs: 82 },
      { windowName: 'slow', totalRequests: 1000000, failedRequests: 10, latencyViolations: 20, p99LatencyMs: 91 }
    ]);

    expect(alert.active).toBe(false);
    expect(alert.severity).toBe('none');
    expect(alert.evaluations).toHaveLength(2);
    expect(alert.evaluations[0].availabilityHealthy).toBe(true);
    expect(alert.evaluations[0].latencyHealthy).toBe(true);
  });

  test('pages when the fast burn-rate window exhausts error budget too quickly', () => {
    const monitor = new SLOMonitor();
    const alert = monitor.buildAlert([
      { windowName: 'fast', totalRequests: 100000, failedRequests: 100, latencyViolations: 100, p99LatencyMs: 88 },
      { windowName: 'slow', totalRequests: 1000000, failedRequests: 20, latencyViolations: 20, p99LatencyMs: 90 }
    ]);

    expect(alert.active).toBe(true);
    expect(alert.severity).toBe('page');
    expect(alert.evaluations[0].windowName).toBe('fast');
    expect(alert.evaluations[0].burnRate).toBeCloseTo(20, 5);
    expect(alert.evaluations[0].dashboardLabels).toEqual({
      slo: 'utility-protocol-availability-latency',
      window: 'fast',
      severity: 'page'
    });
  });

  test('alerts on latency SLO violations even when availability is healthy', () => {
    const monitor = new SLOMonitor();
    const alert = monitor.buildAlert([
      { windowName: 'fast', totalRequests: 50000, failedRequests: 0, latencyViolations: 0, p99LatencyMs: 125 }
    ]);

    expect(alert.active).toBe(true);
    expect(alert.severity).toBe('page');
    expect(alert.evaluations[0].latencyHealthy).toBe(false);
  });

  test('supports custom burn-rate windows for canary analysis', () => {
    const monitor = new SLOMonitor({
      burnRateWindows: [{ name: 'canary', minutes: 10, threshold: 2, severity: 'page' }]
    });
    const alert = monitor.buildAlert([
      { windowName: 'canary', totalRequests: 10000, failedRequests: 3, latencyViolations: 0, p99LatencyMs: 70 }
    ]);

    expect(alert.active).toBe(true);
    expect(alert.evaluations[0].minutes).toBe(10);
    expect(alert.evaluations[0].burnRate).toBeCloseTo(3, 5);
  });
});
