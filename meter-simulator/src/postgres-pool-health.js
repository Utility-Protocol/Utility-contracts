class PostgresPoolHealthProbe {
  constructor(pool, options = {}) {
    if (!pool || typeof pool.query !== 'function') {
      throw new TypeError('A PostgreSQL pool with a query(sql) method is required');
    }

    this.pool = pool;
    this.options = {
      probeSql: options.probeSql || 'SELECT 1',
      timeoutMs: options.timeoutMs || 75,
      targetP99Ms: options.targetP99Ms || 100,
      minSize: options.minSize || 2,
      maxSize: options.maxSize || 20,
      scaleUpThreshold: options.scaleUpThreshold || 0.8,
      scaleDownThreshold: options.scaleDownThreshold || 0.25,
      coolDownMs: options.coolDownMs || 30000,
      now: options.now || (() => Date.now())
    };

    this.latencies = [];
    this.maxSamples = options.maxSamples || 100;
    this.currentSize = options.currentSize || this.options.minSize;
    this.lastResizeAt = 0;
  }

  async check() {
    const startedAt = this.options.now();

    try {
      await this._withTimeout(this.pool.query(this.options.probeSql));
      const latencyMs = Math.max(0, this.options.now() - startedAt);
      this._recordLatency(latencyMs);

      const metrics = this.getMetrics();
      return {
        status: latencyMs <= this.options.targetP99Ms ? 'healthy' : 'degraded',
        latencyMs,
        metrics,
        recommendation: this.recommendSize(metrics)
      };
    } catch (error) {
      return {
        status: 'unhealthy',
        latencyMs: Math.max(0, this.options.now() - startedAt),
        error: error.message,
        metrics: this.getMetrics(),
        recommendation: this.recommendSize({ utilization: 1, p99LatencyMs: this.options.targetP99Ms + 1 })
      };
    }
  }

  getMetrics() {
    return {
      totalConnections: this._poolValue('totalCount', this.currentSize),
      idleConnections: this._poolValue('idleCount', 0),
      waitingClients: this._poolValue('waitingCount', 0),
      utilization: this._utilization(),
      p99LatencyMs: this._percentile(99)
    };
  }

  recommendSize(metrics = this.getMetrics()) {
    const desired = this._desiredSize(metrics);
    return {
      currentSize: this.currentSize,
      desiredSize: desired,
      action: desired > this.currentSize ? 'scale_up' : desired < this.currentSize ? 'scale_down' : 'hold'
    };
  }

  applySizing(metrics = this.getMetrics()) {
    const now = this.options.now();
    if (now - this.lastResizeAt < this.options.coolDownMs) {
      return { resized: false, reason: 'cooldown', ...this.recommendSize(metrics) };
    }

    const recommendation = this.recommendSize(metrics);
    if (recommendation.desiredSize === this.currentSize) {
      return { resized: false, reason: 'already_optimal', ...recommendation };
    }

    this.currentSize = recommendation.desiredSize;
    this.lastResizeAt = now;
    return { resized: true, reason: 'applied', ...recommendation, currentSize: this.currentSize };
  }

  _desiredSize(metrics) {
    const highLatency = metrics.p99LatencyMs > this.options.targetP99Ms;
    const highPressure = metrics.utilization >= this.options.scaleUpThreshold || metrics.waitingClients > 0;
    const lowPressure = metrics.utilization <= this.options.scaleDownThreshold && metrics.waitingClients === 0;

    if (highLatency || highPressure) {
      return Math.min(this.options.maxSize, Math.max(this.currentSize + 1, Math.ceil(this.currentSize * 1.25)));
    }

    if (lowPressure) {
      return Math.max(this.options.minSize, this.currentSize - 1);
    }

    return this.currentSize;
  }

  _utilization() {
    const total = this._poolValue('totalCount', this.currentSize);
    if (total <= 0) return 0;
    const idle = this._poolValue('idleCount', 0);
    return Math.min(1, Math.max(0, (total - idle) / total));
  }

  _poolValue(field, fallback) {
    const value = Number(this.pool[field]);
    return Number.isFinite(value) ? value : fallback;
  }

  _recordLatency(latencyMs) {
    this.latencies.push(latencyMs);
    if (this.latencies.length > this.maxSamples) this.latencies.shift();
  }

  _percentile(percentile) {
    if (this.latencies.length === 0) return 0;
    const sorted = [...this.latencies].sort((a, b) => a - b);
    const index = Math.ceil((percentile / 100) * sorted.length) - 1;
    return sorted[Math.max(0, Math.min(sorted.length - 1, index))];
  }

  _withTimeout(promise) {
    let timer;
    const timeout = new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`PostgreSQL health probe timed out after ${this.options.timeoutMs}ms`)), this.options.timeoutMs);
    });

    return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
  }
}

module.exports = PostgresPoolHealthProbe;
