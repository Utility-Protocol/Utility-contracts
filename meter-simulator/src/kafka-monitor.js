/**
 * Kafka Consumer Lag Monitoring & Auto-Scaling Core Logic
 *
 * Implements a high-precision, asynchronous offset monitoring and auto-scaling
 * controller for Kafka consumer groups.
 */

class KafkaMonitorAndScaler {
  constructor(topicName, partitionCount, groupId, config = {}) {
    const partitions = [];
    for (let i = 0; i < partitionCount; i++) {
      partitions.push({
        partitionId: i,
        logEndOffset: 1000,
        committedOffset: 1000,
        lag: 0,
      });
    }

    this.topic = { name: topicName, partitions };
    this.group = {
      groupId,
      topic: topicName,
      activeConsumers: config.minConsumers ?? 2,
      minConsumers: config.minConsumers ?? 1,
      maxConsumers: Math.min(config.maxConsumers ?? 12, partitionCount),
      targetLagPerConsumer: config.targetLagPerConsumer ?? 200,
      scaleUpThreshold: config.scaleUpThreshold ?? 800,
      scaleDownThreshold: config.scaleDownThreshold ?? 150,
      scaleUpCooldownMs: config.scaleUpCooldownMs ?? 10000,
      scaleDownCooldownMs: config.scaleDownCooldownMs ?? 20000,
      lastScaleUpTimestamp: 0,
      lastScaleDownTimestamp: 0,
    };

    this.events = [];
    this.lastEvaluationDurationMs = 0;
    this.alertWebhooks = [];
    this.isRebalancing = false;
    this.rebalanceEndTimestamp = 0;

    this.logEvent(
      'REBALANCE',
      `Cluster initialized. Monitored topic '${topicName}' with ${partitionCount} partitions.`,
      0,
      this.group.activeConsumers,
      0
    );
  }

  subscribeToAlerts(callback) {
    this.alertWebhooks.push(callback);
  }

  getTopicState() {
    return this.topic;
  }

  getConsumerGroup() {
    return this.group;
  }

  getEvents() {
    return this.events;
  }

  getLastEvaluationDurationMs() {
    return this.lastEvaluationDurationMs;
  }

  produceMessages(count) {
    if (count <= 0) return;
    const partitionCount = this.topic.partitions.length;
    for (let i = 0; i < count; i++) {
      const pIdx = i % partitionCount;
      this.topic.partitions[pIdx].logEndOffset += 1;
    }
    this.recalculateLag();
  }

  consumeMessages(elapsedSeconds, baseCapacityRate) {
    if (elapsedSeconds <= 0 || baseCapacityRate <= 0) return;

    let effectiveness = 1.0;
    const now = Date.now();
    if (this.isRebalancing) {
      if (now < this.rebalanceEndTimestamp) {
        effectiveness = 0.1;
      } else {
        this.isRebalancing = false;
        this.logEvent('REBALANCE', 'Partition rebalancing complete. Consumers resumed full capacity.', this.group.activeConsumers, this.group.activeConsumers, this.getTotalLag());
      }
    }

    const totalCapacity = Math.floor(
      this.group.activeConsumers * baseCapacityRate * elapsedSeconds * effectiveness
    );

    if (totalCapacity <= 0) return;

    let messagesToConsume = totalCapacity;
    let cycles = 0;
    const maxCycles = 5;

    while (messagesToConsume > 0 && cycles < maxCycles) {
      const activeLaggardPartitions = this.topic.partitions.filter((p) => p.logEndOffset > p.committedOffset);
      if (activeLaggardPartitions.length === 0) break;

      const consumptionPerPartition = Math.ceil(messagesToConsume / activeLaggardPartitions.length);

      for (const p of activeLaggardPartitions) {
        const available = p.logEndOffset - p.committedOffset;
        const consumeAmount = Math.min(consumptionPerPartition, available, messagesToConsume);
        p.committedOffset += consumeAmount;
        messagesToConsume -= consumeAmount;
        if (messagesToConsume <= 0) break;
      }
      cycles++;
    }

    this.recalculateLag();
  }

  recalculateLag() {
    for (const p of this.topic.partitions) {
      p.lag = Math.max(0, p.logEndOffset - p.committedOffset);
    }
  }

  getTotalLag() {
    return this.topic.partitions.reduce((acc, p) => acc + p.lag, 0);
  }

  triggerRebalance() {
    this.isRebalancing = true;
    this.rebalanceEndTimestamp = Date.now() + 3000;
    this.logEvent(
      'REBALANCE',
      'Triggered partition rebalance. Active consumption throughput degraded to 10% for 3s.',
      this.group.activeConsumers,
      this.group.activeConsumers,
      this.getTotalLag()
    );
  }

  evaluateScaling() {
    const startTime = Date.now();
    const now = Date.now();
    const totalLag = this.getTotalLag();

    let targetCount = this.group.activeConsumers;
    let action = 'NONE';

    if (totalLag >= 4000) {
      this.dispatchAlert('CRITICAL', 'Group Lag Alert', totalLag, 4000, `Critical consumer lag detected: ${totalLag} messages pending.`);
    } else if (totalLag >= 2000) {
      this.dispatchAlert('WARNING', 'Group Lag Alert', totalLag, 2000, `High consumer lag alert: ${totalLag} messages pending.`);
    }

    let computedDesired = Math.ceil(totalLag / this.group.targetLagPerConsumer);
    if (computedDesired < this.group.minConsumers) {
      computedDesired = this.group.minConsumers;
    }

    const partitionLimit = this.topic.partitions.length;
    const maxAllowed = Math.min(this.group.maxConsumers, partitionLimit);

    if (computedDesired > maxAllowed) {
      computedDesired = maxAllowed;
    }

    if (totalLag > this.group.scaleUpThreshold && computedDesired > this.group.activeConsumers) {
      const timeSinceLastScaleUp = now - this.group.lastScaleUpTimestamp;
      if (timeSinceLastScaleUp >= this.group.scaleUpCooldownMs) {
        action = 'SCALE_UP';
        targetCount = computedDesired;
      } else {
        this.logEvent(
          'COOLDOWN_BLOCKED',
          `Scale-up request blocked by cooldown. Remaining lock: ${Math.ceil((this.group.scaleUpCooldownMs - timeSinceLastScaleUp) / 1000)}s`,
          this.group.activeConsumers,
          this.group.activeConsumers,
          totalLag
        );
      }
    } else if (totalLag < this.group.scaleDownThreshold && computedDesired < this.group.activeConsumers) {
      const timeSinceLastScaleDown = now - this.group.lastScaleDownTimestamp;
      const timeSinceLastScaleUp = now - this.group.lastScaleUpTimestamp;

      if (timeSinceLastScaleDown >= this.group.scaleDownCooldownMs && timeSinceLastScaleUp >= this.group.scaleUpCooldownMs) {
        action = 'SCALE_DOWN';
        targetCount = computedDesired;
      } else {
        const remainingLock = Math.max(
          this.group.scaleDownCooldownMs - timeSinceLastScaleDown,
          this.group.scaleUpCooldownMs - timeSinceLastScaleUp
        );
        this.logEvent(
          'COOLDOWN_BLOCKED',
          `Scale-down request blocked by cooldown lock. Remaining: ${Math.ceil(remainingLock / 1000)}s`,
          this.group.activeConsumers,
          this.group.activeConsumers,
          totalLag
        );
      }
    }

    if (computedDesired > this.group.activeConsumers && action === 'NONE' && this.group.activeConsumers === maxAllowed) {
      this.logEvent(
        'LIMIT_REACHED',
        `Lag exceeds scale-up threshold, but scale-up blocked. Consumers at maximum constraint: ${maxAllowed}`,
        this.group.activeConsumers,
        this.group.activeConsumers,
        totalLag
      );
    }

    if (action === 'SCALE_UP') {
      const prev = this.group.activeConsumers;
      this.group.activeConsumers = targetCount;
      this.group.lastScaleUpTimestamp = now;
      this.logEvent(
        'SCALE_UP',
        `Scaled up consumer group dynamically from ${prev} to ${targetCount} due to lag spike (${totalLag} messages).`,
        prev,
        targetCount,
        totalLag
      );
      this.triggerRebalance();
    } else if (action === 'SCALE_DOWN') {
      const prev = this.group.activeConsumers;
      this.group.activeConsumers = targetCount;
      this.group.lastScaleDownTimestamp = now;
      this.logEvent(
        'SCALE_DOWN',
        `Scaled down consumer group gracefully from ${prev} to ${targetCount} as queue cleared (${totalLag} messages).`,
        prev,
        targetCount,
        totalLag
      );
      this.triggerRebalance();
    }

    this.lastEvaluationDurationMs = Date.now() - startTime;

    return { action, targetCount };
  }

  logEvent(type, message, prev, next, lag) {
    const event = {
      id: Math.random().toString(36).substring(2, 9),
      timestamp: new Date().toISOString(),
      type,
      message,
      previousCount: prev,
      newCount: next,
      groupLag: lag,
    };
    this.events.unshift(event);
    if (this.events.length > 50) {
      this.events.pop();
    }
  }

  dispatchAlert(severity, metric, value, threshold, message) {
    const payload = {
      alertId: 'alert_' + Math.random().toString(36).substring(2, 9),
      timestamp: new Date().toISOString(),
      severity,
      metric,
      value,
      threshold,
      message,
    };

    const lastEvent = this.events[0];
    if (!lastEvent || lastEvent.message !== message) {
      this.logEvent('ALERT_TRIGGERED', `[${severity}] ${message}`, this.group.activeConsumers, this.group.activeConsumers, value);
    }

    for (const callback of this.alertWebhooks) {
      try {
        callback(payload);
      } catch (err) {
        console.error('Alert callback failed:', err);
      }
    }
  }

  overrideConfig(config) {
    if (config.minConsumers !== undefined) {
      this.group.minConsumers = Math.max(1, config.minConsumers);
    }
    if (config.maxConsumers !== undefined) {
      this.group.maxConsumers = Math.min(config.maxConsumers, this.topic.partitions.length);
    }
    if (config.targetLagPerConsumer !== undefined) {
      this.group.targetLagPerConsumer = Math.max(1, config.targetLagPerConsumer);
    }
    if (config.scaleUpThreshold !== undefined) {
      this.group.scaleUpThreshold = config.scaleUpThreshold;
    }
    if (config.scaleDownThreshold !== undefined) {
      this.group.scaleDownThreshold = config.scaleDownThreshold;
    }

    if (this.group.activeConsumers < this.group.minConsumers) {
      this.group.activeConsumers = this.group.minConsumers;
    }
    if (this.group.activeConsumers > this.group.maxConsumers) {
      this.group.activeConsumers = this.group.maxConsumers;
    }

    this.logEvent(
      'REBALANCE',
      'Consumer group scaling parameters updated by Grid Administrator.',
      this.group.activeConsumers,
      this.group.activeConsumers,
      this.getTotalLag()
    );
  }
}

module.exports = KafkaMonitorAndScaler;
