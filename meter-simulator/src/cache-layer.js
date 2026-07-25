const net = require('net');
const { URL } = require('url');

/**
 * Two-tier cache with an in-process hot path and optional Redis backing.
 *
 * The memory tier keeps critical status reads under the 100ms target when data is
 * warm. Redis can be enabled for cross-process coherence; when disabled, the
 * same API operates entirely in memory for local development and tests.
 */

class MinimalRedisClient {
  constructor(options) {
    const url = new URL(options.url || 'redis://localhost:6379');
    this.host = url.hostname;
    this.port = Number.parseInt(url.port || '6379', 10);
    this.password = url.password ? decodeURIComponent(url.password) : '';
    this.connectTimeoutMs = options.connectTimeoutMs;
    this.socket = null;
    this.buffer = '';
  }

  connect() {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection({ host: this.host, port: this.port });
      const timeout = setTimeout(() => {
        socket.destroy();
        reject(new Error('Redis connection timed out'));
      }, this.connectTimeoutMs);

      socket.once('connect', async () => {
        clearTimeout(timeout);
        this.socket = socket;
        try {
          if (this.password) {
            await this.command('AUTH', this.password);
          }
          resolve();
        } catch (error) {
          reject(error);
        }
      });
      socket.once('error', reject);
      socket.on('data', (chunk) => {
        this.buffer += chunk.toString('utf8');
      });
    });
  }

  async get(key) {
    return await this.command('GET', key);
  }

  async set(key, value, ttlSeconds) {
    return await this.command('SET', key, value, 'EX', String(ttlSeconds));
  }

  async del(keys) {
    const normalized = Array.isArray(keys) ? keys : [keys];
    if (normalized.length === 0) {
      return 0;
    }
    return await this.command('DEL', ...normalized);
  }

  async keys(pattern) {
    return await this.command('KEYS', pattern);
  }

  close() {
    if (this.socket) {
      this.socket.end();
      this.socket = null;
    }
  }

  async command(...parts) {
    this.socket.write(this.encode(parts));
    return await this.readResponse();
  }

  encode(parts) {
    const bulk = parts.map((part) => {
      const value = String(part);
      return `$${Buffer.byteLength(value)}\r\n${value}\r\n`;
    }).join('');
    return `*${parts.length}\r\n${bulk}`;
  }

  readResponse() {
    return new Promise((resolve, reject) => {
      const poll = () => {
        const parsed = this.parse(this.buffer);
        if (parsed) {
          this.buffer = this.buffer.slice(parsed.offset);
          if (parsed.error) {
            reject(parsed.error);
          } else {
            resolve(parsed.value);
          }
          return;
        }
        setTimeout(poll, 5);
      };
      poll();
    });
  }

  parse(input) {
    if (!input) return null;
    const type = input[0];
    const lineEnd = input.indexOf('\r\n');
    if (lineEnd === -1) return null;
    const line = input.slice(1, lineEnd);
    if (type === '+') return { value: line, offset: lineEnd + 2 };
    if (type === '-') return { error: new Error(line), offset: lineEnd + 2 };
    if (type === ':') return { value: Number.parseInt(line, 10), offset: lineEnd + 2 };
    if (type === '$') {
      const length = Number.parseInt(line, 10);
      if (length === -1) return { value: null, offset: lineEnd + 2 };
      const start = lineEnd + 2;
      const end = start + length;
      if (input.length < end + 2) return null;
      return { value: input.slice(start, end), offset: end + 2 };
    }
    if (type === '*') {
      const count = Number.parseInt(line, 10);
      const values = [];
      let offset = lineEnd + 2;
      for (let index = 0; index < count; index += 1) {
        const parsed = this.parse(input.slice(offset));
        if (!parsed) return null;
        values.push(parsed.value);
        offset += parsed.offset;
      }
      return { value: values, offset };
    }
    return null;
  }
}

class CacheLayer {
  constructor(options = {}) {
    this.enabled = options.enabled !== false;
    this.redisEnabled = Boolean(options.redis?.enabled);
    this.defaultTtlSeconds = this._positiveInteger(options.defaultTtlSeconds, 60);
    this.keyPrefix = options.keyPrefix || 'utility:cache';
    this.memory = new Map();
    this.metrics = {
      hits: 0,
      misses: 0,
      sets: 0,
      deletes: 0,
      errors: 0
    };

    this.redis = null;
    this.redisReady = false;
    this.redisOptions = options.redis || {};
  }

  async connect() {
    if (!this.enabled || !this.redisEnabled || this.redisReady) {
      return;
    }

    this.redis = new MinimalRedisClient({
      url: this.redisOptions.url,
      connectTimeoutMs: this._positiveInteger(this.redisOptions.connectTimeoutMs, 500)
    });

    try {
      await this.redis.connect();
      this.redisReady = true;
    } catch (error) {
      this.redisReady = false;
      this.metrics.errors += 1;
    }
  }

  async disconnect() {
    if (this.redis) {
      this.redis.close();
      this.redis = null;
      this.redisReady = false;
    }
  }

  async get(key) {
    if (!this.enabled) {
      this.metrics.misses += 1;
      return null;
    }

    const memoryEntry = this.memory.get(key);
    if (memoryEntry && memoryEntry.expiresAt > Date.now()) {
      this.metrics.hits += 1;
      return memoryEntry.value;
    }
    this.memory.delete(key);

    if (this.redisReady) {
      try {
        const raw = await this.redis.get(this._redisKey(key));
        if (raw) {
          const value = JSON.parse(raw);
          this.memory.set(key, {
            value,
            expiresAt: Date.now() + this.defaultTtlSeconds * 1000
          });
          this.metrics.hits += 1;
          return value;
        }
      } catch (error) {
        this.redisReady = false;
        this.metrics.errors += 1;
      }
    }

    this.metrics.misses += 1;
    return null;
  }

  async set(key, value, ttlSeconds = this.defaultTtlSeconds) {
    if (!this.enabled) {
      return value;
    }

    const ttl = this._positiveInteger(ttlSeconds, this.defaultTtlSeconds);
    this.memory.set(key, {
      value,
      expiresAt: Date.now() + ttl * 1000
    });

    if (this.redisReady) {
      try {
        await this.redis.set(this._redisKey(key), JSON.stringify(value), ttl);
      } catch (error) {
        this.redisReady = false;
        this.metrics.errors += 1;
      }
    }

    this.metrics.sets += 1;
    return value;
  }

  async remember(key, ttlSeconds, loader) {
    const cached = await this.get(key);
    if (cached !== null) {
      return cached;
    }

    const value = await loader();
    await this.set(key, value, ttlSeconds);
    return value;
  }

  async delete(key) {
    this.memory.delete(key);
    if (this.redisReady) {
      try {
        await this.redis.del(this._redisKey(key));
      } catch (error) {
        this.redisReady = false;
        this.metrics.errors += 1;
      }
    }
    this.metrics.deletes += 1;
  }

  async clear() {
    this.memory.clear();
    if (this.redisReady) {
      const keys = await this.redis.keys(`${this.keyPrefix}:*`);
      if (keys.length > 0) {
        await this.redis.del(keys);
      }
    }
  }

  getMetrics() {
    return { ...this.metrics, memory_entries: this.memory.size, redis_ready: this.redisReady };
  }

  _redisKey(key) {
    return `${this.keyPrefix}:${key}`;
  }

  _positiveInteger(value, fallback) {
    const parsed = Number.parseInt(value, 10);
    return Number.isInteger(parsed) && parsed > 0 ? parsed : fallback;
  }
}

module.exports = CacheLayer;
