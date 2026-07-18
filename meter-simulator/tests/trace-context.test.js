const {
  parseTraceparent,
  formatTraceparent,
  createTraceContext,
  createBaggage,
  injectTraceContext,
  calculateLatencyMs
} = require('../src/trace-context');

describe('trace context propagation', () => {
  test('creates valid W3C traceparent values', () => {
    const context = createTraceContext();
    const traceparent = formatTraceparent(context);

    expect(parseTraceparent(traceparent)).toEqual({
      version: '00',
      traceId: context.traceId,
      spanId: context.spanId,
      traceFlags: '01'
    });
  });

  test('preserves trace id and rotates span id for child contexts', () => {
    const parent = '00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01';
    const child = createTraceContext(parent);

    expect(child.traceId).toBe('4bf92f3577b34da6a3ce929d0e0e4736');
    expect(child.parentSpanId).toBe('00f067aa0ba902b7');
    expect(child.spanId).not.toBe('00f067aa0ba902b7');
  });

  test('rejects malformed or all-zero trace identifiers', () => {
    expect(parseTraceparent('bad')).toBeNull();
    expect(parseTraceparent('00-00000000000000000000000000000000-00f067aa0ba902b7-01')).toBeNull();
    expect(parseTraceparent('00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01')).toBeNull();
  });

  test('injects trace fields and encoded baggage into meter payloads', () => {
    const payload = injectTraceContext(
      { meter_id: 42, units_consumed: 5 },
      { serviceName: 'meter-simulator', operation: 'meter.usage.publish', baggage: { region: 'test net' } }
    );

    expect(payload.traceparent).toMatch(/^00-[\da-f]{32}-[\da-f]{16}-01$/);
    expect(payload.baggage).toContain('service=meter-simulator');
    expect(payload.baggage).toContain('meter_id=42');
    expect(payload.baggage).toContain('operation=meter.usage.publish');
    expect(payload.baggage).toContain('region=test%20net');
    expect(payload.span_id).toHaveLength(16);
    expect(payload.trace_started_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  test('calculates latency with a zero floor for clock skew', () => {
    expect(calculateLatencyMs('2026-07-17T00:00:00.000Z', '2026-07-17T00:00:00.099Z')).toBe(99);
    expect(calculateLatencyMs('2026-07-17T00:00:01.000Z', '2026-07-17T00:00:00.000Z')).toBe(0);
  });
});
