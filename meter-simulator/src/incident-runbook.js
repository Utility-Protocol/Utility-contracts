#!/usr/bin/env node

/**
 * Incident response runbook automation with PagerDuty Events API v2 integration.
 *
 * The automation intentionally keeps critical-path evaluation local and synchronous;
 * PagerDuty network I/O only occurs after a rule has matched. This keeps health
 * checks suitable for sub-100ms polling loops while still escalating actionable
 * incidents to responders.
 */

const axios = require('axios');

const DEFAULT_EVENTS_API_URL = 'https://events.pagerduty.com/v2/enqueue';

const Severity = Object.freeze({
  CRITICAL: 'critical',
  ERROR: 'error',
  WARNING: 'warning',
  INFO: 'info'
});

class PagerDutyClient {
  constructor({ routingKey = process.env.PAGERDUTY_ROUTING_KEY, apiUrl = process.env.PAGERDUTY_EVENTS_API_URL || DEFAULT_EVENTS_API_URL, timeoutMs = 5000 } = {}) {
    if (!routingKey) {
      throw new Error('PAGERDUTY_ROUTING_KEY environment variable is required');
    }
    this.routingKey = routingKey;
    this.apiUrl = apiUrl;
    this.timeoutMs = timeoutMs;
  }

  async trigger({ dedupKey, summary, source, severity, customDetails = {} }) {
    const payload = {
      routing_key: this.routingKey,
      event_action: 'trigger',
      dedup_key: dedupKey,
      payload: {
        summary,
        source,
        severity,
        timestamp: new Date().toISOString(),
        custom_details: customDetails
      }
    };

    const response = await axios.post(this.apiUrl, payload, { timeout: this.timeoutMs });
    return { status: response.status, dedupKey: response.data && response.data.dedup_key ? response.data.dedup_key : dedupKey };
  }
}

class IncidentRunbookAutomation {
  constructor({ pagerDutyClient, serviceName = 'utility-contracts', now = () => Date.now(), suppressMs = 15 * 60 * 1000 } = {}) {
    this.pagerDutyClient = pagerDutyClient;
    this.serviceName = serviceName;
    this.now = now;
    this.suppressMs = suppressMs;
    this.lastTriggeredAt = new Map();
  }

  evaluate(snapshot) {
    const incidents = [];
    const add = (rule, severity, summary, details = {}) => {
      incidents.push({
        rule,
        severity,
        summary,
        source: this.serviceName,
        dedupKey: `${this.serviceName}:${rule}`,
        customDetails: { ...snapshot, ...details }
      });
    };

    if (snapshot.contractPaused === true) {
      add('contract-paused', Severity.CRITICAL, 'Utility contract is paused');
    }

    if (typeof snapshot.remainingTtlLedgers === 'number' && snapshot.remainingTtlLedgers < 1000) {
      add('ttl-low', Severity.ERROR, 'Contract TTL is below the safe operating threshold', { threshold: 1000 });
    }

    if (typeof snapshot.oracleAgeSeconds === 'number' && snapshot.oracleAgeSeconds > 300) {
      add('stale-oracle', Severity.ERROR, 'Oracle price feed is stale', { thresholdSeconds: 300 });
    }

    if (typeof snapshot.p99LatencyMs === 'number' && snapshot.p99LatencyMs >= 100) {
      add('latency-slo-breach', Severity.WARNING, 'Critical path P99 latency exceeds 100ms target', { targetMs: 100 });
    }

    if (typeof snapshot.errorRatePercent === 'number' && snapshot.errorRatePercent >= 1) {
      add('error-rate-high', Severity.ERROR, 'Service error rate exceeds incident threshold', { thresholdPercent: 1 });
    }

    return incidents;
  }

  async process(snapshot) {
    const incidents = this.evaluate(snapshot);
    const triggered = [];

    for (const incident of incidents) {
      if (this.shouldSuppress(incident.dedupKey)) {
        continue;
      }
      const result = await this.pagerDutyClient.trigger(incident);
      this.lastTriggeredAt.set(incident.dedupKey, this.now());
      triggered.push({ ...incident, pagerDuty: result });
    }

    return { incidents, triggered };
  }

  shouldSuppress(dedupKey) {
    const previous = this.lastTriggeredAt.get(dedupKey);
    return typeof previous === 'number' && this.now() - previous < this.suppressMs;
  }
}

if (require.main === module) {
  const snapshot = JSON.parse(process.env.INCIDENT_SNAPSHOT || '{}');
  const automation = new IncidentRunbookAutomation({ pagerDutyClient: new PagerDutyClient() });
  automation.process(snapshot)
    .then((result) => {
      console.log(JSON.stringify(result, null, 2));
    })
    .catch((error) => {
      console.error(`Incident automation failed: ${error.message}`);
      process.exit(1);
    });
}

module.exports = { IncidentRunbookAutomation, PagerDutyClient, Severity };
