const {
  RuntimeConfigAuditor,
  diffValues,
  flattenConfig,
  hashConfig
} = require('../src/runtime-config-auditor');

const baseConfig = {
  contract: {
    network: 'testnet',
    rpcUrl: 'https://soroban-testnet.stellar.org'
  },
  mqtt: {
    host: 'localhost',
    password: 'super-secret',
    qos: 1
  }
};

describe('RuntimeConfigAuditor', () => {
  test('creates deterministic redacted hashes independent of key order', () => {
    const reordered = {
      mqtt: { qos: 1, password: 'super-secret', host: 'localhost' },
      contract: { rpcUrl: 'https://soroban-testnet.stellar.org', network: 'testnet' }
    };

    expect(hashConfig(flattenConfig(baseConfig))).toBe(hashConfig(flattenConfig(reordered)));
    expect(flattenConfig(baseConfig)['mqtt.password']).toBe('[REDACTED]');
  });

  test('detects runtime configuration drift with path-level changes', () => {
    const auditor = new RuntimeConfigAuditor({ serviceName: 'meter-simulator' });
    auditor.setBaseline(baseConfig, new Date('2026-07-17T00:00:00.000Z'));

    const report = auditor.audit({
      ...baseConfig,
      mqtt: { ...baseConfig.mqtt, qos: 2 }
    }, new Date('2026-07-17T00:01:00.000Z'));

    expect(report.driftDetected).toBe(true);
    expect(report.changes).toEqual([
      { path: 'mqtt.qos', expected: 1, actual: 2 }
    ]);
    expect(report.withinBudget).toBe(true);
  });

  test('reports no drift for unchanged effective configuration', () => {
    const auditor = new RuntimeConfigAuditor({ serviceName: 'meter-simulator' });
    auditor.setBaseline(baseConfig);

    const report = auditor.audit({ ...baseConfig });

    expect(report.driftDetected).toBe(false);
    expect(report.changes).toEqual([]);
  });

  test('diffValues reports additions and removals', () => {
    expect(diffValues({ a: 1, b: 2 }, { b: 2, c: 3 })).toEqual([
      { path: 'a', expected: 1, actual: undefined },
      { path: 'c', expected: undefined, actual: 3 }
    ]);
  });
});
