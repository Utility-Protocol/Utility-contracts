import axios from 'axios';
import { generateSignatures, validateUrlForSsrf } from './security';
import { trackDeliveryAttempt, trackQueueSize, trackFailure } from './metrics';

export interface WebhookPayload {
  event: string;
  timestamp: number;
  data: any;
}

export interface WebhookJob {
  id: string;
  payload: WebhookPayload;
  url: string;
  secret: string; // shared secret for HMAC
  privateKey?: string; // hex-encoded private key for Ed25519 (optional)
  attempts: number;
  maxAttempts: number;
  nextAttemptTime: number;
}

export interface WebhookDeliveryLog {
  id: string;
  url: string;
  event: string;
  timestamp: number;
  attempts: number;
  status: 'SUCCESS' | 'FAILED' | 'RETRYING';
  statusCode?: number;
  errorMessage?: string;
  lastAttemptTime: number;
}

// In-memory job queue and log storage
const queue: WebhookJob[] = [];
const deliveryLogs: WebhookDeliveryLog[] = [];
const MAX_LOGS = 100;

let isProcessing = false;

/**
 * Enqueue a new webhook delivery job
 */
export function enqueueWebhook(
  payload: WebhookPayload,
  url: string,
  secret: string,
  privateKey?: string,
  maxAttempts: number = 5
): string {
  const id = Math.random().toString(36).substring(2, 15);

  const job: WebhookJob = {
    id,
    payload,
    url,
    secret,
    privateKey,
    attempts: 0,
    maxAttempts,
    nextAttemptTime: Date.now(),
  };

  queue.push(job);
  trackQueueSize(queue.length);

  // Initialize delivery log
  addLog({
    id,
    url,
    event: payload.event,
    timestamp: payload.timestamp,
    attempts: 0,
    status: 'RETRYING',
    lastAttemptTime: Date.now(),
  });

  // Process queue asynchronously
  setImmediate(() => {
    processQueue();
  });

  return id;
}

/**
 * Retrieve recent delivery logs
 */
export function getDeliveryLogs(): WebhookDeliveryLog[] {
  return [...deliveryLogs];
}

/**
 * Retrieve queue size
 */
export function getQueueSize(): number {
  return queue.length;
}

/**
 * Clear queue and logs (primarily for testing)
 */
export function clearQueueAndLogs(): void {
  queue.length = 0;
  deliveryLogs.length = 0;
  trackQueueSize(0);
}

/**
 * Add or update delivery logs
 */
function addLog(log: WebhookDeliveryLog) {
  const index = deliveryLogs.findIndex((l) => l.id === log.id);
  if (index >= 0) {
    deliveryLogs[index] = log;
  } else {
    deliveryLogs.unshift(log);
    if (deliveryLogs.length > MAX_LOGS) {
      deliveryLogs.pop();
    }
  }
}

/**
 * Calculate retry delay with exponential backoff and full randomized jitter
 */
export function calculateRetryDelay(attempt: number, baseDelay = 1000, maxDelay = 30000): number {
  if (process.env.NODE_ENV === 'test') {
    return 1; // 1ms for tests to execute retries instantly
  }
  const temp = Math.min(maxDelay, baseDelay * Math.pow(2, attempt));
  // Full jitter: randomized between 0 and temp
  return Math.random() * temp;
}

/**
 * Background queue processor
 */
async function processQueue() {
  if (isProcessing) return;
  isProcessing = true;

  try {
    while (queue.length > 0) {
      // Find jobs ready for processing (nextAttemptTime <= now)
      const now = Date.now();
      const jobIndex = queue.findIndex((job) => job.nextAttemptTime <= now);

      if (jobIndex === -1) {
        // No jobs are ready right now, wait or break
        break;
      }

      // Extract the job
      const [job] = queue.splice(jobIndex, 1);
      trackQueueSize(queue.length);

      // Process the job
      await deliverWebhook(job);
    }
  } finally {
    isProcessing = false;

    // If there are still items in the queue, schedule the next check
    if (queue.length > 0) {
      setTimeout(() => {
        processQueue();
      }, 200); // Check every 200ms
    }
  }
}

/**
 * Deliver a single webhook job
 */
async function deliverWebhook(job: WebhookJob) {
  job.attempts++;
  const startTime = Date.now();

  // 1. SSRF URL validation
  const ssrfCheck = validateUrlForSsrf(job.url);
  if (!ssrfCheck.valid) {
    const errorMsg = `SSRF Prevention: ${ssrfCheck.reason}`;
    trackFailure();
    addLog({
      id: job.id,
      url: job.url,
      event: job.payload.event,
      timestamp: job.payload.timestamp,
      attempts: job.attempts,
      status: 'FAILED',
      errorMessage: errorMsg,
      lastAttemptTime: Date.now(),
    });
    return; // Drop immediately
  }

  // 2. Generate signatures
  const timestamp = Math.floor(Date.now() / 1000);
  const bodyString = JSON.stringify(job.payload);
  const signatures = generateSignatures(bodyString, timestamp, job.secret, job.privateKey);

  try {
    // 3. Make HTTP POST call with timeout
    const response = await axios.post(job.url, bodyString, {
      headers: {
        'Content-Type': 'application/json',
        'X-Webhook-Timestamp': timestamp.toString(),
        'X-Webhook-Signature-256': `t=${timestamp},v1=${signatures.hmacSignature}`,
        ...(signatures.ed25519Signature
          ? { 'X-Webhook-Signature-Ed25519': `t=${timestamp},v1=${signatures.ed25519Signature}` }
          : {}),
      },
      timeout: 5000, // 5-second timeout
    });

    const duration = (Date.now() - startTime) / 1000;
    trackDeliveryAttempt(response.status, duration, job.attempts);

    addLog({
      id: job.id,
      url: job.url,
      event: job.payload.event,
      timestamp: job.payload.timestamp,
      attempts: job.attempts,
      status: 'SUCCESS',
      statusCode: response.status,
      lastAttemptTime: Date.now(),
    });

  } catch (error: any) {
    const duration = (Date.now() - startTime) / 1000;
    const statusCode = error.response?.status;
    const errorMessage = error.message || 'Unknown network error';

    trackDeliveryAttempt(statusCode || 0, duration, job.attempts);

    if (job.attempts < job.maxAttempts) {
      // Re-queue for retry
      const delay = calculateRetryDelay(job.attempts);
      job.nextAttemptTime = Date.now() + delay;
      queue.push(job);
      trackQueueSize(queue.length);

      addLog({
        id: job.id,
        url: job.url,
        event: job.payload.event,
        timestamp: job.payload.timestamp,
        attempts: job.attempts,
        status: 'RETRYING',
        statusCode,
        errorMessage: `${errorMessage}. Retrying in ${Math.round(delay)}ms...`,
        lastAttemptTime: Date.now(),
      });
    } else {
      // Max attempts exhausted
      trackFailure();
      addLog({
        id: job.id,
        url: job.url,
        event: job.payload.event,
        timestamp: job.payload.timestamp,
        attempts: job.attempts,
        status: 'FAILED',
        statusCode,
        errorMessage: `Max attempts (${job.maxAttempts}) exhausted. Last error: ${errorMessage}`,
        lastAttemptTime: Date.now(),
      });
    }
  }
}
