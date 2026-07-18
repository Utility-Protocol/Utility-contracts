import request from 'supertest';
// supertest allows testing express apps without spawning a live TCP server, making tests fast and deterministic.
import express from 'express';
import app from '../src/index';
import * as delivery from '../src/delivery';
import { validateUrlForSsrf, generateSignatures, verifyHmacSignature, verifyEd25519Signature } from '../src/security';
import { resetMetricCache, getStatsSummary } from '../src/metrics';
import axios from 'axios';
import nacl from 'tweetnacl';
import bs58 from 'bs58';

// Mock axios
jest.mock('axios');
const mockedAxios = axios as jest.Mocked<typeof axios>;

describe('Webhook Delivery Service Suite', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    delivery.clearQueueAndLogs();
    resetMetricCache();
  });

  describe('1. SSRF URL Validation', () => {
    test('should allow valid public URLs', () => {
      expect(validateUrlForSsrf('https://api.github.com/webhooks').valid).toBe(true);
      expect(validateUrlForSsrf('http://example.com/callback').valid).toBe(true);
    });

    test('should block invalid protocols', () => {
      const res = validateUrlForSsrf('ftp://example.com/callback');
      expect(res.valid).toBe(false);
      expect(res.reason).toContain('protocol');
    });

    test('should block localhost and loopback targets', () => {
      expect(validateUrlForSsrf('http://localhost/webhooks').valid).toBe(false);
      expect(validateUrlForSsrf('http://127.0.0.1:3000/callback').valid).toBe(false);
      expect(validateUrlForSsrf('http://[::1]/callback').valid).toBe(false);
    });

    test('should block AWS/Metadata instance addresses', () => {
      const res = validateUrlForSsrf('http://169.254.169.254/latest/meta-data');
      expect(res.valid).toBe(false);
      expect(res.reason).toContain('Metadata');
    });

    test('should block RFC1918 Private Subnets', () => {
      expect(validateUrlForSsrf('http://10.0.1.5/hooks').valid).toBe(false);
      expect(validateUrlForSsrf('http://172.16.42.1/hooks').valid).toBe(false);
      expect(validateUrlForSsrf('http://192.168.1.100/hooks').valid).toBe(false);
    });
  });

  describe('2. Cryptographic Signatures', () => {
    const secret = 'super_secret_signing_key_123';
    const body = JSON.stringify({ event: 'low_balance', amount: 50 });
    const timestamp = Math.floor(Date.now() / 1000);

    test('should generate correct HMAC-SHA256 signatures', () => {
      const sigs = generateSignatures(body, timestamp, secret);
      expect(sigs.hmacSignature).toBeDefined();
      expect(sigs.hmacSignature).toHaveLength(64); // hex encoded sha256 is 64 characters

      const isValid = verifyHmacSignature(body, timestamp, sigs.hmacSignature, secret);
      expect(isValid).toBe(true);
    });

    test('should reject expired or replayed HMAC-SHA256 signatures', () => {
      const sigs = generateSignatures(body, timestamp, secret);
      // Verify with an expired timestamp (more than 5 mins difference)
      const isStillValid = verifyHmacSignature(body, timestamp - 400, sigs.hmacSignature, secret);
      expect(isStillValid).toBe(false);
    });

    test('should generate and verify Ed25519 signatures correctly', () => {
      // Generate standard ed25519 keypair
      const keyPair = nacl.sign.keyPair();
      const privateKeyBase58 = bs58.encode(keyPair.secretKey.slice(0, 32));
      const publicKeyBase58 = bs58.encode(keyPair.publicKey);

      const sigs = generateSignatures(body, timestamp, secret, privateKeyBase58);
      expect(sigs.ed25519Signature).toBeDefined();

      const isEdValid = verifyEd25519Signature(
        body,
        timestamp,
        sigs.ed25519Signature!,
        publicKeyBase58
      );
      expect(isEdValid).toBe(true);
    });
  });

  describe('3. Async Ingestion and Queue Performance', () => {
    test('POST /webhooks should ingest and return HTTP 202 immediately (<100ms)', async () => {
      const startTime = Date.now();
      const response = await request(app)
        .post('/webhooks')
        .send({
          payload: { event: 'low_balance', timestamp: Date.now(), data: { meter_id: 123 } },
          url: 'https://webhook.receiver.com/hook',
          secret: 'shh_secret',
        });

      const duration = Date.now() - startTime;
      expect(response.status).toBe(202);
      expect(response.body).toHaveProperty('status', 'ACCEPTED');
      expect(response.body).toHaveProperty('jobId');
      expect(duration).toBeLessThan(100); // Guarantees critical ingestion path is < 100ms
    });

    test('POST /webhooks should reject requests with missing parameters', async () => {
      const response = await request(app)
        .post('/webhooks')
        .send({
          url: 'https://webhook.receiver.com/hook',
        });

      expect(response.status).toBe(400);
      expect(response.body).toHaveProperty('error');
    });
  });

  describe('4. Exponential Backoff with Random Jitter', () => {
    test('calculateRetryDelay should calculate exponential backoff limits', () => {
      const baseDelay = 1000;
      const maxDelay = 30000;

      // Without randomized jitter, base * 2^attempt would be:
      // Attempt 1: 2000
      // Attempt 2: 4000
      // Attempt 5: 32000 (capped at 30000)

      const delay1 = delivery.calculateRetryDelay(1, baseDelay, maxDelay);
      expect(delay1).toBeGreaterThanOrEqual(0);
      expect(delay1).toBeLessThanOrEqual(2000);

      const delay5 = delivery.calculateRetryDelay(5, baseDelay, maxDelay);
      expect(delay5).toBeGreaterThanOrEqual(0);
      expect(delay5).toBeLessThanOrEqual(30000);
    });
  });

  describe('5. Delivery execution, retries, and failure logging', () => {
    test('successful delivery should log SUCCESS', async () => {
      mockedAxios.post.mockResolvedValueOnce({ status: 200, data: {} });

      const jobId = delivery.enqueueWebhook(
        { event: 'test', timestamp: Date.now(), data: {} },
        'https://webhook.receiver.com/hook',
        'secret'
      );

      // Wait briefly for setImmediate / background processing to fire
      await new Promise((resolve) => setTimeout(resolve, 100));

      const logs = delivery.getDeliveryLogs();
      const log = logs.find((l) => l.id === jobId);
      expect(log).toBeDefined();
      expect(log!.status).toBe('SUCCESS');
      expect(log!.attempts).toBe(1);
    });

    test('failed delivery should trigger retries and eventually mark as FAILED', async () => {
      // Mock retry delay to return 1ms so retries execute instantly
      jest.spyOn(delivery, 'calculateRetryDelay').mockReturnValue(1);

      // Mock network error for axios
      mockedAxios.post.mockRejectedValue({
        message: 'Network Error',
        response: { status: 500 },
      });

      const jobId = delivery.enqueueWebhook(
        { event: 'test', timestamp: Date.now(), data: {} },
        'https://webhook.receiver.com/hook',
        'secret',
        undefined,
        2 // Max attempts 2
      );

      // Wait for attempts to execute (should retry in 1ms)
      await new Promise((resolve) => setTimeout(resolve, 500));

      const logs = delivery.getDeliveryLogs();
      const log = logs.find((l) => l.id === jobId);
      expect(log).toBeDefined();
      expect(log!.status).toBe('FAILED');
      expect(log!.attempts).toBe(2); // Attempted exactly twice
    });

    test('should drop webhooks targeting SSRF destinations immediately', async () => {
      const jobId = delivery.enqueueWebhook(
        { event: 'test', timestamp: Date.now(), data: {} },
        'http://127.0.0.1:9000/hooks', // Restricted SSRF
        'secret'
      );

      await new Promise((resolve) => setTimeout(resolve, 100));

      const logs = delivery.getDeliveryLogs();
      const log = logs.find((l) => l.id === jobId);
      expect(log).toBeDefined();
      expect(log!.status).toBe('FAILED');
      expect(log!.errorMessage).toContain('SSRF Prevention');
      expect(mockedAxios.post).not.toHaveBeenCalled(); // Axios call never made
    });
  });

  describe('6. Monitoring endpoints', () => {
    test('GET /health should return system status UP', async () => {
      const response = await request(app).get('/health');
      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('status', 'UP');
    });

    test('GET /stats should return correct summary statistics', async () => {
      const response = await request(app).get('/stats');
      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('successCount');
      expect(response.body).toHaveProperty('p99LatencyMs');
    });

    test('GET /metrics should return Prometheus metrics exposition', async () => {
      const response = await request(app).get('/metrics');
      expect(response.status).toBe(200);
      expect(response.text).toContain('webhook_delivery_attempts_total');
    });
  });
});
