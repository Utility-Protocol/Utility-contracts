const KafkaMonitorAndScaler = require('../src/kafka-monitor');

describe('KafkaMonitorAndScaler Core Engine', () => {
  let monitor;

  beforeEach(() => {
    // 8 partitions, group ID 'test-billing-consumers', min consumers 2, max 8
    monitor = new KafkaMonitorAndScaler('billing-events', 8, 'test-billing-consumers', {
      minConsumers: 2,
      maxConsumers: 8,
      targetLagPerConsumer: 100,
      scaleUpThreshold: 500,
      scaleDownThreshold: 100,
      scaleUpCooldownMs: 1000,
      scaleDownCooldownMs: 2000,
    });
  });

  test('should initialize correctly with partitions and configs', () => {
    const topic = monitor.getTopicState();
    const group = monitor.getConsumerGroup();

    expect(topic.name).toBe('billing-events');
    expect(topic.partitions).toHaveLength(8);
    expect(group.activeConsumers).toBe(2);
    expect(group.minConsumers).toBe(2);
    expect(group.maxConsumers).toBe(8);
    expect(group.targetLagPerConsumer).toBe(100);
    expect(monitor.getTotalLag()).toBe(0);
  });

  test('should compute correct lag upon message production', () => {
    monitor.produceMessages(400); // dist across 8 partitions
    expect(monitor.getTotalLag()).toBe(400);

    const partitions = monitor.getTopicState().partitions;
    for (let i = 0; i < 8; i++) {
      expect(partitions[i].lag).toBe(50); // 400 / 8
    }
  });

  test('should compute correct lag reduction upon consumption', () => {
    monitor.produceMessages(400);
    // Consuming 1 second at capacity rate of 50 per consumer
    // Active consumers = 2, total capacity = 2 * 50 * 1 = 100
    monitor.consumeMessages(1, 50);

    expect(monitor.getTotalLag()).toBe(300);
  });

  test('should trigger SCALE_UP correctly when lag exceeds threshold', () => {
    monitor.produceMessages(600); // 600 > scaleUpThreshold (500)
    // Desired = ceil(600 / 100) = 6. Limit is maxConsumers (8).
    const decision = monitor.evaluateScaling();

    expect(decision.action).toBe('SCALE_UP');
    expect(decision.targetCount).toBe(6);
    expect(monitor.getConsumerGroup().activeConsumers).toBe(6);

    const events = monitor.getEvents();
    expect(events.some(e => e.type === 'SCALE_UP')).toBe(true);
    expect(events.some(e => e.type === 'REBALANCE')).toBe(true);
  });

  test('should respect Scale-Up Cooldown locks', () => {
    monitor.produceMessages(600);
    const decision1 = monitor.evaluateScaling();
    expect(decision1.action).toBe('SCALE_UP');
    expect(monitor.getConsumerGroup().activeConsumers).toBe(6);

    // Immediately try to scale up further with more messages, cooldown lock should block it
    monitor.produceMessages(200); // lag = 800. Desired = ceil(800 / 100) = 8 > 6.
    const decision2 = monitor.evaluateScaling();
    expect(decision2.action).toBe('NONE');

    const events = monitor.getEvents();
    expect(events.some(e => e.type === 'COOLDOWN_BLOCKED')).toBe(true);
  });

  test('should trigger SCALE_DOWN correctly when lag drops', () => {
    // Override cooldown timestamp so we can downscale
    const group = monitor.getConsumerGroup();
    group.lastScaleDownTimestamp = Date.now() - 5000;
    group.lastScaleUpTimestamp = Date.now() - 5000;

    // Manually set consumers to 6 and produce light load
    group.activeConsumers = 6;
    monitor.produceMessages(50); // lag = 50 < scaleDownThreshold (100)
    // Desired = ceil(50 / 100) = 1. Min is 2.
    const decision = monitor.evaluateScaling();

    expect(decision.action).toBe('SCALE_DOWN');
    expect(decision.targetCount).toBe(2);
    expect(group.activeConsumers).toBe(2);
  });

  test('should dispatch alerts on high lag values', () => {
    const alertReceived = [];
    monitor.subscribeToAlerts((alert) => {
      alertReceived.push(alert);
    });

    // Produce 3000 messages (should trigger WARNING)
    monitor.produceMessages(3000);
    monitor.evaluateScaling();

    expect(alertReceived).toHaveLength(1);
    expect(alertReceived[0].severity).toBe('WARNING');
    expect(alertReceived[0].value).toBe(3000);

    // Produce another 1500 (total 4500, should trigger CRITICAL)
    monitor.produceMessages(1500);
    monitor.evaluateScaling();

    expect(alertReceived).toHaveLength(2);
    expect(alertReceived[1].severity).toBe('CRITICAL');
    expect(alertReceived[1].value).toBe(4500);
  });

  test('should enforce config overrides securely', () => {
    monitor.overrideConfig({
      minConsumers: 3,
      maxConsumers: 5,
      targetLagPerConsumer: 150,
    });

    const group = monitor.getConsumerGroup();
    expect(group.minConsumers).toBe(3);
    expect(group.maxConsumers).toBe(5);
    expect(group.targetLagPerConsumer).toBe(150);
    expect(group.activeConsumers).toBe(3); // auto-adjusted to new minimum
  });
});
