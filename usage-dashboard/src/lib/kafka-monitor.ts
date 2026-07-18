/**
 * Kafka Consumer Lag Monitoring & Auto-Scaling Core Logic
 *
 * Implements a high-precision, asynchronous offset monitoring and auto-scaling
 * controller for Kafka consumer groups.
 */

export interface PartitionState {
  partitionId: number;
  logEndOffset: number;
  committedOffset: number;
  lag: number;
}

export interface TopicState {
  name: string;
  partitions: PartitionState[];
}

export interface ConsumerGroup {
  groupId: string;
  topic: string;
  activeConsumers: number;
  minConsumers: number;
  maxConsumers: number;
  targetLagPerConsumer: number; // e.g., 100 messages per consumer
  scaleUpThreshold: number;     // e.g., 1000 total lag
  scaleDownThreshold: number;   // e.g., 100 total lag
  scaleUpCooldownMs: number;    // e.g., 30000ms (30s)
  scaleDownCooldownMs: number;  // e.g., 300000ms (5 mins)
  lastScaleUpTimestamp: number;
  lastScaleDownTimestamp: number;
}

export interface ScalingEvent {
  id: string;
  timestamp: string;
  type: 'SCALE_UP' | 'SCALE_DOWN' | 'COOLDOWN_BLOCKED' | 'LIMIT_REACHED' | 'ALERT_TRIGGERED' | 'REBALANCE';
  message: string;
  previousCount: number;
  newCount: number;
  groupLag: number;
}

export interface AlertPayload {
  alertId: string;
  timestamp: string;
  severity: 'WARNING' | 'CRITICAL';
  metric: string;
  value: number;
  threshold: number;
  message: string;
}

export class KafkaMonitorAndScaler {
  private topic: TopicState;
  private group: ConsumerGroup;
  private events: ScalingEvent[] = [];
  private lastEvaluationDurationMs: number = 0;
  private alertWebhooks: ((payload: AlertPayload) => void)[] = [];
  private isRebalancing: boolean = false;
  private rebalanceEndTimestamp: number = 0;

  constructor(
    topicName: string,
    partitionCount: number,
    groupId: string,
    config?: Partial<Omit<ConsumerGroup, 'groupId' | 'topic'>>
  ) {
    // Initialize topic and partition states
    const partitions: PartitionState[] = [];
    for (let i = 0; i < partitionCount; i++) {
      partitions.push({
        partitionId: i,
        logEndOffset: 1000, // starts at arbitrary baseline
        committedOffset: 1000,
        lag: 0,
      });
    }

    this.topic = { name: topicName, partitions };

    // Initialize Consumer Group Config
    this.group = {
      groupId,
      topic: topicName,
      activeConsumers: config?.minConsumers ?? 2,
      minConsumers: config?.minConsumers ?? 1,
      maxConsumers: Math.min(config?.maxConsumers ?? 12, partitionCount),
      targetLagPerConsumer: config?.targetLagPerConsumer ?? 200,
      scaleUpThreshold: config?.scaleUpThreshold ?? 800,
      scaleDownThreshold: config?.scaleDownThreshold ?? 150,
      scaleUpCooldownMs: config?.scaleUpCooldownMs ?? 10000,   // default 10s for simulation speed
      scaleDownCooldownMs: config?.scaleDownCooldownMs ?? 20000, // default 20s for simulation speed
      lastScaleUpTimestamp: 0,
      lastScaleDownTimestamp: 0,
    };

    // Record initial state
    this.logEvent(
      'REBALANCE',
      `Cluster initialized. Monitored topic '${topicName}' with ${partitionCount} partitions.`,
      0,
      this.group.activeConsumers,
      0
    );
  }

  // --- External Alert Subscription ---
  public subscribeToAlerts(callback: (payload: AlertPayload) => void): void {
    this.alertWebhooks.push(callback);
  }

  // --- Accessors ---
  public getTopicState(): TopicState {
    return this.topic;
  }

  public getConsumerGroup(): ConsumerGroup {
    return this.group;
  }

  public getEvents(): ScalingEvent[] {
    return this.events;
  }

  public getLastEvaluationDurationMs(): number {
    return this.lastEvaluationDurationMs;
  }

  /**
   * Simulate producing new messages to partitions
   * @param count Total number of messages to distribute across partitions
   */
  public produceMessages(count: number): void {
    if (count <= 0) return;
    const partitionCount = this.topic.partitions.length;
    for (let i = 0; i < count; i++) {
      // Round-robin or random distribution
      const pIdx = i % partitionCount;
      this.topic.partitions[pIdx].logEndOffset += 1;
    }
    this.recalculateLag();
  }

  /**
   * Simulate processing messages by active consumers
   * @param elapsedSeconds Time elapsed since last process cycle
   * @param baseCapacityRate Messages consumed per consumer instance per second
   */
  public consumeMessages(elapsedSeconds: number, baseCapacityRate: number): void {
    if (elapsedSeconds <= 0 || baseCapacityRate <= 0) return;

    // If simulating active rebalance, consumption capacity drops by 90%
    let effectiveness = 1.0;
    const now = Date.now();
    if (this.isRebalancing) {
      if (now < this.rebalanceEndTimestamp) {
        effectiveness = 0.1; // extreme slowdown during partition rebalance
      } else {
        this.isRebalancing = false;
        this.logEvent('REBALANCE', 'Partition rebalancing complete. Consumers resumed full capacity.', this.group.activeConsumers, this.group.activeConsumers, this.getTotalLag());
      }
    }

    const totalCapacity = Math.floor(
      this.group.activeConsumers * baseCapacityRate * elapsedSeconds * effectiveness
    );

    if (totalCapacity <= 0) return;

    // Distribute consumption across partitions that have lag
    let messagesToConsume = totalCapacity;
    let cycles = 0;
    const maxCycles = 5; // prevent infinite loops

    while (messagesToConsume > 0 && cycles < maxCycles) {
      let activeLaggardPartitions = this.topic.partitions.filter((p) => p.logEndOffset > p.committedOffset);
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

  /**
   * Recomputes partition level lag values and total cumulative lag.
   */
  private recalculateLag(): void {
    for (const p of this.topic.partitions) {
      p.lag = Math.max(0, p.logEndOffset - p.committedOffset);
    }
  }

  /**
   * Returns the exact current total lag of the consumer group.
   */
  public getTotalLag(): number {
    return this.topic.partitions.reduce((acc, p) => acc + p.lag, 0);
  }

  /**
   * Triggers rebalancing penalty state
   */
  private triggerRebalance(): void {
    this.isRebalancing = true;
    this.rebalanceEndTimestamp = Date.now() + 3000; // 3 seconds penalty
    this.logEvent(
      'REBALANCE',
      'Triggered partition rebalance. Active consumption throughput degraded to 10% for 3s.',
      this.group.activeConsumers,
      this.group.activeConsumers,
      this.getTotalLag()
    );
  }

  /**
   * High performance (<10ms target) Scaling Controller implementation
   * Resolves scaling decisions, respects cooldown parameters, checks partition limits,
   * triggers alerts on critical levels.
   */
  public evaluateScaling(): { action: 'SCALE_UP' | 'SCALE_DOWN' | 'NONE'; targetCount: number } {
    const startTime = Date.now();
    const now = Date.now();
    const totalLag = this.getTotalLag();

    let targetCount = this.group.activeConsumers;
    let action: 'SCALE_UP' | 'SCALE_DOWN' | 'NONE' = 'NONE';

    // 1. Alerting checks
    if (totalLag >= 4000) {
      this.dispatchAlert('CRITICAL', 'Group Lag Alert', totalLag, 4000, `Critical consumer lag detected: ${totalLag} messages pending.`);
    } else if (totalLag >= 2000) {
      this.dispatchAlert('WARNING', 'Group Lag Alert', totalLag, 2000, `High consumer lag alert: ${totalLag} messages pending.`);
    }

    // 2. Compute desired consumer size mathematically
    // Desired = ceil(TotalLag / TargetLagPerConsumer)
    let computedDesired = Math.ceil(totalLag / this.group.targetLagPerConsumer);
    if (computedDesired < this.group.minConsumers) {
      computedDesired = this.group.minConsumers;
    }

    const partitionLimit = this.topic.partitions.length;
    const maxAllowed = Math.min(this.group.maxConsumers, partitionLimit);

    if (computedDesired > maxAllowed) {
      computedDesired = maxAllowed;
    }

    // 3. Scaling Decision and Cooldown Gatekeeping
    if (totalLag > this.group.scaleUpThreshold && computedDesired > this.group.activeConsumers) {
      // Wants scale up
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
      // Wants scale down
      const timeSinceLastScaleDown = now - this.group.lastScaleDownTimestamp;
      const timeSinceLastScaleUp = now - this.group.lastScaleUpTimestamp;

      // Ensure both scale down cooldown and scale up protection cooldown have passed
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

    // 4. Limits enforcement logs
    if (computedDesired > this.group.activeConsumers && action === 'NONE' && this.group.activeConsumers === maxAllowed) {
      // Blocked by maximum consumer constraints or partition caps
      this.logEvent(
        'LIMIT_REACHED',
        `Lag exceeds scale-up threshold, but scale-up blocked. Consumers at maximum constraint: ${maxAllowed}`,
        this.group.activeConsumers,
        this.group.activeConsumers,
        totalLag
      );
    }

    // 5. Actuation Execution
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

    // Calculate execution duration to verify SLA targets
    this.lastEvaluationDurationMs = Date.now() - startTime;

    return { action, targetCount };
  }

  // --- Helper logger ---
  private logEvent(
    type: ScalingEvent['type'],
    message: string,
    prev: number,
    next: number,
    lag: number
  ): void {
    const event: ScalingEvent = {
      id: Math.random().toString(36).substring(2, 9),
      timestamp: new Date().toISOString(),
      type,
      message,
      previousCount: prev,
      newCount: next,
      groupLag: lag,
    };
    // Maintain bounded size of event list (cap to 50 entries)
    this.events.unshift(event);
    if (this.events.length > 50) {
      this.events.pop();
    }
  }

  // --- Dispatch Alert Webhook ---
  private dispatchAlert(
    severity: AlertPayload['severity'],
    metric: string,
    value: number,
    threshold: number,
    message: string
  ): void {
    const payload: AlertPayload = {
      alertId: 'alert_' + Math.random().toString(36).substring(2, 9),
      timestamp: new Date().toISOString(),
      severity,
      metric,
      value,
      threshold,
      message,
    };

    // Log alert event locally if not already logged
    const lastEvent = this.events[0];
    if (!lastEvent || lastEvent.message !== message) {
      this.logEvent('ALERT_TRIGGERED', `[${severity}] ${message}`, this.group.activeConsumers, this.group.activeConsumers, value);
    }

    // Trigger registered subscribers/webhooks
    for (const callback of this.alertWebhooks) {
      try {
        callback(payload);
      } catch (err) {
        console.error('Alert callback failed:', err);
      }
    }
  }

  /**
   * Direct manual configuration overrides (secured via admin context simulation)
   */
  public overrideConfig(config: Partial<Omit<ConsumerGroup, 'groupId' | 'topic'>>): void {
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

    // Ensure active range is safe
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
