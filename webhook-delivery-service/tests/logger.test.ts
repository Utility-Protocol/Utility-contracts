import { logger } from '../src/logger';

describe('Structured Logger', () => {
  afterEach(() => {
    jest.restoreAllMocks();
  });

  test('info logs emit a structured JSON record to stdout', () => {
    const spy = jest.spyOn(console, 'log').mockImplementation(() => {});

    logger.info('webhook delivered successfully', { 'webhook.id': 'abc123' });

    expect(spy).toHaveBeenCalledTimes(1);
    const record = JSON.parse(spy.mock.calls[0][0] as string);

    expect(record.SeverityText).toBe('INFO');
    expect(record.SeverityNumber).toBe(9);
    expect(record.Body).toBe('webhook delivered successfully');
    expect(record.Resource['service.name']).toBe('webhook-delivery-service');
    expect(record.Attributes['webhook.id']).toBe('abc123');
    expect(typeof record.Timestamp).toBe('string');
  });

  test('error logs are written to stderr with the correct severity', () => {
    const spy = jest.spyOn(console, 'error').mockImplementation(() => {});

    logger.error('webhook delivery failed permanently', { 'error.message': 'timeout' });

    expect(spy).toHaveBeenCalledTimes(1);
    const record = JSON.parse(spy.mock.calls[0][0] as string);

    expect(record.SeverityText).toBe('ERROR');
    expect(record.SeverityNumber).toBe(17);
    expect(record.Attributes['error.message']).toBe('timeout');
  });
});
