/**
 * Structured JSON logger following OpenTelemetry log data model / semantic conventions.
 * https://opentelemetry.io/docs/specs/otel/logs/data-model/
 */

export type LogSeverity = 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';

const SEVERITY_NUMBER: Record<LogSeverity, number> = {
  DEBUG: 5,
  INFO: 9,
  WARN: 13,
  ERROR: 17,
};

const SERVICE_NAME = process.env.OTEL_SERVICE_NAME || 'webhook-delivery-service';

export interface LogAttributes {
  [key: string]: string | number | boolean | undefined;
}

function emit(severity: LogSeverity, body: string, attributes?: LogAttributes): void {
  const record = {
    Timestamp: new Date().toISOString(),
    SeverityText: severity,
    SeverityNumber: SEVERITY_NUMBER[severity],
    Body: body,
    Resource: { 'service.name': SERVICE_NAME },
    Attributes: attributes ?? {},
  };

  const line = JSON.stringify(record);
  if (severity === 'ERROR') {
    // eslint-disable-next-line no-console
    console.error(line);
  } else {
    // eslint-disable-next-line no-console
    console.log(line);
  }
}

export const logger = {
  debug: (body: string, attributes?: LogAttributes) => emit('DEBUG', body, attributes),
  info: (body: string, attributes?: LogAttributes) => emit('INFO', body, attributes),
  warn: (body: string, attributes?: LogAttributes) => emit('WARN', body, attributes),
  error: (body: string, attributes?: LogAttributes) => emit('ERROR', body, attributes),
};
