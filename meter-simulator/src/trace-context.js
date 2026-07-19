const crypto = require('crypto');

const TRACEPARENT_VERSION = '00';
const DEFAULT_TRACE_FLAGS = '01';
const TRACEPARENT_REGEX = /^([\da-f]{2})-([\da-f]{32})-([\da-f]{16})-([\da-f]{2})$/;

function randomHex(bytes) {
  return crypto.randomBytes(bytes).toString('hex');
}

function nonZeroHex(bytes) {
  let value = randomHex(bytes);
  while (/^0+$/.test(value)) {
    value = randomHex(bytes);
  }
  return value;
}

function parseTraceparent(traceparent) {
  if (typeof traceparent !== 'string') {
    return null;
  }

  const normalized = traceparent.trim().toLowerCase();
  const match = TRACEPARENT_REGEX.exec(normalized);
  if (!match) {
    return null;
  }

  const [, version, traceId, spanId, traceFlags] = match;
  if (traceId === '00000000000000000000000000000000' || spanId === '0000000000000000') {
    return null;
  }

  return { version, traceId, spanId, traceFlags };
}

function formatTraceparent(context) {
  return `${context.version || TRACEPARENT_VERSION}-${context.traceId}-${context.spanId}-${context.traceFlags || DEFAULT_TRACE_FLAGS}`;
}

function createTraceContext(parentTraceparent) {
  const parent = parseTraceparent(parentTraceparent);
  return {
    version: TRACEPARENT_VERSION,
    traceId: parent ? parent.traceId : nonZeroHex(16),
    spanId: nonZeroHex(8),
    parentSpanId: parent ? parent.spanId : null,
    traceFlags: parent ? parent.traceFlags : DEFAULT_TRACE_FLAGS,
    startedAt: new Date().toISOString()
  };
}

function createBaggage(entries = {}) {
  return Object.entries(entries)
    .filter(([, value]) => value !== undefined && value !== null && value !== '')
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`)
    .join(',');
}

function injectTraceContext(payload, options = {}) {
  const context = createTraceContext(options.parentTraceparent);
  const baggage = createBaggage({
    service: options.serviceName || 'meter-simulator',
    meter_id: payload.meter_id,
    operation: options.operation || 'meter.publish',
    ...options.baggage
  });

  return {
    ...payload,
    traceparent: formatTraceparent(context),
    tracestate: options.tracestate || '',
    baggage,
    span_id: context.spanId,
    parent_span_id: context.parentSpanId,
    trace_started_at: context.startedAt
  };
}

function calculateLatencyMs(startedAt, endedAt = new Date()) {
  const start = new Date(startedAt).getTime();
  const end = endedAt instanceof Date ? endedAt.getTime() : new Date(endedAt).getTime();
  if (!Number.isFinite(start) || !Number.isFinite(end)) {
    return 0;
  }
  return Math.max(0, end - start);
}

module.exports = {
  TRACEPARENT_VERSION,
  DEFAULT_TRACE_FLAGS,
  parseTraceparent,
  formatTraceparent,
  createTraceContext,
  createBaggage,
  injectTraceContext,
  calculateLatencyMs
};
