import express, { Request, Response } from 'express';
import { enqueueWebhook, getDeliveryLogs, getQueueSize } from './delivery';
import { getPrometheusMetrics, getStatsSummary } from './metrics';

const app = express();
const port = process.env.PORT || 3001;

// Middlewares
app.use(express.json());

// Enable CORS for dashboard queries
app.use((req, res, next) => {
  res.header('Access-Control-Allow-Origin', '*');
  res.header('Access-Control-Allow-Headers', 'Origin, X-Requested-With, Content-Type, Accept');
  res.header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  if (req.method === 'OPTIONS') {
    return res.sendStatus(200);
  }
  next();
});

/**
 * POST /webhooks
 * Decoupled High-Performance Webhook Ingestion API (<100ms SLA, typically <10ms)
 */
app.post('/webhooks', (req: Request, res: Response) => {
  const { payload, url, secret, privateKey, maxAttempts } = req.body;

  // Input validation
  if (!payload || !url || !secret) {
    return res.status(400).json({
      error: 'Missing required parameters. "payload", "url", and "secret" are mandatory.',
    });
  }

  if (typeof payload !== 'object' || typeof url !== 'string' || typeof secret !== 'string') {
    return res.status(400).json({
      error: 'Invalid parameter types.',
    });
  }

  // Enqueue job asynchronously (executes in <1ms)
  const jobId = enqueueWebhook(
    payload,
    url,
    secret,
    privateKey,
    maxAttempts ? parseInt(maxAttempts, 10) : undefined
  );

  // Return immediately with 202 Accepted
  return res.status(202).json({
    status: 'ACCEPTED',
    message: 'Webhook enqueued for asynchronous delivery.',
    jobId,
  });
});

/**
 * GET /metrics
 * Expose standard Prometheus exposition formats
 */
app.get('/metrics', async (req: Request, res: Response) => {
  try {
    const metrics = await getPrometheusMetrics();
    res.set('Content-Type', 'text/plain; version=0.0.4');
    return res.send(metrics);
  } catch (err: any) {
    return res.status(500).send(err.message);
  }
});

/**
 * GET /stats
 * Expose real-time metrics for direct dashboard rendering
 */
app.get('/stats', (req: Request, res: Response) => {
  const stats = getStatsSummary();
  return res.json({
    ...stats,
    queueSize: getQueueSize(),
  });
});

/**
 * GET /logs
 * Expose historical logs of delivery attempts
 */
app.get('/logs', (req: Request, res: Response) => {
  const logs = getDeliveryLogs();
  return res.json(logs);
});

/**
 * GET /health
 * Basic system health indicator
 */
app.get('/health', (req: Request, res: Response) => {
  return res.json({
    status: 'UP',
    timestamp: Date.now(),
    queueSize: getQueueSize(),
  });
});

// Start the server
if (process.env.NODE_ENV !== 'test') {
  app.listen(port, () => {
    console.log(`🚀 Webhook Delivery Service listening on port ${port}`);
  });
}

export default app;
