const axios = require('axios');
const { IncidentRunbookAutomation, PagerDutyClient } = require('../src/incident-runbook');

jest.mock('axios');

describe('IncidentRunbookAutomation', () => {
  test('detects runbook incidents from service health snapshots', () => {
    const automation = new IncidentRunbookAutomation({ pagerDutyClient: { trigger: jest.fn() } });

    const incidents = automation.evaluate({
      contractPaused: true,
      remainingTtlLedgers: 99,
      oracleAgeSeconds: 301,
      p99LatencyMs: 125,
      errorRatePercent: 1.5
    });

    expect(incidents.map((incident) => incident.rule)).toEqual([
      'contract-paused',
      'ttl-low',
      'stale-oracle',
      'latency-slo-breach',
      'error-rate-high'
    ]);
    expect(incidents[0].dedupKey).toBe('utility-contracts:contract-paused');
  });

  test('triggers PagerDuty once per suppression window', async () => {
    let now = 1000;
    const trigger = jest.fn().mockResolvedValue({ status: 202, dedupKey: 'utility-contracts:ttl-low' });
    const automation = new IncidentRunbookAutomation({ pagerDutyClient: { trigger }, now: () => now, suppressMs: 60000 });

    await automation.process({ remainingTtlLedgers: 10 });
    await automation.process({ remainingTtlLedgers: 10 });
    now += 60001;
    await automation.process({ remainingTtlLedgers: 10 });

    expect(trigger).toHaveBeenCalledTimes(2);
  });
});

describe('PagerDutyClient', () => {
  test('posts Events API v2 trigger payloads', async () => {
    axios.post.mockResolvedValue({ status: 202, data: { dedup_key: 'custom-dedup' } });
    const client = new PagerDutyClient({ routingKey: 'routing-key', apiUrl: 'https://example.test/enqueue', timeoutMs: 1234 });

    const result = await client.trigger({
      dedupKey: 'service:rule',
      summary: 'summary',
      source: 'service',
      severity: 'critical',
      customDetails: { contractPaused: true }
    });

    expect(axios.post).toHaveBeenCalledWith('https://example.test/enqueue', expect.objectContaining({
      routing_key: 'routing-key',
      event_action: 'trigger',
      dedup_key: 'service:rule',
      payload: expect.objectContaining({ severity: 'critical' })
    }), { timeout: 1234 });
    expect(result).toEqual({ status: 202, dedupKey: 'custom-dedup' });
  });
});
