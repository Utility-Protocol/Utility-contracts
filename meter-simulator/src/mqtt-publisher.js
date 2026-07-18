const mqtt = require('mqtt');
const chalk = require('chalk');
const config = require('./config');
const { PerTenantRateLimiter } = require('./rate-limiter');

class MQTTPublisher {
  constructor(mqttConfig) {
    this.config = mqttConfig;
    this.client = null;
    this.connected = false;
    this.rateLimiter = new PerTenantRateLimiter(config.rateLimit);
  }

  /**
   * Connect to MQTT broker
   */
  async connect() {
    return new Promise((resolve, reject) => {
      const options = {
        clientId: this.config.clientId,
        clean: true,
        connectTimeout: 30000,
        reconnectPeriod: 1000,
        qos: this.config.qos
      };

      // Add authentication if provided
      if (this.config.username && this.config.password) {
        options.username = this.config.username;
        options.password = this.config.password;
      }

      this.client = mqtt.connect(`mqtt://${this.config.host}:${this.config.port}`, options);

      this.client.on('connect', () => {
        this.connected = true;
        console.log(chalk.green('📡 Connected to MQTT broker'));
        resolve();
      });

      this.client.on('error', (error) => {
        console.error(chalk.red('❌ MQTT connection error:'), error);
        reject(error);
      });

      this.client.on('offline', () => {
        this.connected = false;
        console.log(chalk.yellow('📡 MQTT client offline'));
      });

      this.client.on('reconnect', () => {
        console.log(chalk.blue('📡 Reconnecting to MQTT broker...'));
      });

      this.client.on('message', (topic, message) => {
        console.log(chalk.cyan(`📨 Received message on ${topic}:`), message.toString());
      });
    });
  }

  /**
   * Disconnect from MQTT broker
   */
  async disconnect() {
    if (this.client && this.connected) {
      return new Promise((resolve) => {
        this.client.end(false, {}, () => {
          this.connected = false;
          console.log(chalk.green('📡 Disconnected from MQTT broker'));
          resolve();
        });
      });
    }
  }

  /**
   * Publish usage data to MQTT topic
   */
  async publishUsageData(usageData) {
    if (!this.connected) {
      throw new Error('Not connected to MQTT broker');
    }

    this.rateLimiter.assertAllowed(`meter:${usageData.meter_id}`);

    const topic = this.config.topic.replace('+', usageData.meter_id.toString());
    const tracePayload = injectTraceContext({
      meter_id: usageData.meter_id,
      timestamp: usageData.timestamp,
      watt_hours_consumed: usageData.display_watt_hours, // Human-readable value
      units_consumed: usageData.units_consumed,
      signature: usageData.signature,
      public_key: usageData.public_key,
      // Additional metadata for backend processing
      is_peak_hour: usageData.is_peak_hour,
      effective_rate: usageData.effective_rate,
      device_id: `ESP32-${usageData.meter_id}`,
      firmware_version: '1.0.0',
      battery_level: Math.floor(Math.random() * 30) + 70, // 70-100%
      signal_strength: Math.floor(Math.random() * 20) + -80, // -80 to -60 dBm
      temperature: Math.floor(Math.random() * 15) + 20 // 20-35°C
    }, {
      parentTraceparent: usageData.traceparent,
      serviceName: this.serviceName,
      operation: 'meter.usage.publish',
      baggage: { device_id: `ESP32-${usageData.meter_id}` }
    });
    const payload = JSON.stringify(tracePayload);

    return new Promise((resolve, reject) => {
      this.client.publish(topic, payload, { qos: this.config.qos }, (error) => {
        if (error) {
          console.error(chalk.red('❌ Failed to publish MQTT message:'), error);
          reject(error);
        } else {
          const latencyMs = calculateLatencyMs(tracePayload.trace_started_at);
          const budgetStatus = latencyMs <= 100 ? chalk.green(`${latencyMs}ms`) : chalk.yellow(`${latencyMs}ms`);
          console.log(chalk.green(`📤 Published to ${topic}`));
          console.log(chalk.cyan(`   Trace: ${tracePayload.traceparent} (${budgetStatus} publish path)`));
          console.log(chalk.cyan(`   Payload: ${payload}`));
          resolve();
        }
      });
    });
  }

  /**
   * Publish heartbeat message
   */
  async publishHeartbeat(meterId) {
    if (!this.connected) {
      throw new Error('Not connected to MQTT broker');
    }

    this.rateLimiter.assertAllowed(`meter:${meterId}:heartbeat`, 0.2);

    const topic = `meters/${meterId}/heartbeat`;
    const tracePayload = injectTraceContext({
      meter_id: meterId,
      timestamp: Math.floor(Date.now() / 1000),
      device_id: `ESP32-${meterId}`,
      firmware_version: '1.0.0',
      battery_level: Math.floor(Math.random() * 30) + 70,
      signal_strength: Math.floor(Math.random() * 20) + -80,
      temperature: Math.floor(Math.random() * 15) + 20,
      uptime: Math.floor(Math.random() * 86400), // Random uptime in seconds
      memory_usage: Math.floor(Math.random() * 50) + 30 // 30-80%
    }, {
      serviceName: this.serviceName,
      operation: 'meter.heartbeat.publish',
      baggage: { device_id: `ESP32-${meterId}` }
    });
    const payload = JSON.stringify(tracePayload);

    return new Promise((resolve, reject) => {
      this.client.publish(topic, payload, { qos: 0 }, (error) => {
        if (error) {
          reject(error);
        } else {
          console.log(chalk.blue(`💓 Heartbeat sent to ${topic}`));
          resolve();
        }
      });
    });
  }

  /**
   * Subscribe to topics for receiving commands
   */
  async subscribeToCommands(meterId) {
    if (!this.connected) {
      throw new Error('Not connected to MQTT broker');
    }

    const topic = `meters/${meterId}/commands`;
    
    return new Promise((resolve, reject) => {
      this.client.subscribe(topic, { qos: 1 }, (error) => {
        if (error) {
          reject(error);
        } else {
          console.log(chalk.green(`👂 Subscribed to commands on ${topic}`));
          resolve();
        }
      });
    });
  }

  /**
   * Publish device status
   */
  async publishStatus(meterId, status) {
    if (!this.connected) {
      throw new Error('Not connected to MQTT broker');
    }

    this.rateLimiter.assertAllowed(`meter:${meterId}:status`, 0.5);

    const topic = `meters/${meterId}/status`;
    const tracePayload = injectTraceContext({
      meter_id: meterId,
      timestamp: Math.floor(Date.now() / 1000),
      status: status, // 'online', 'offline', 'error', 'maintenance'
      device_id: `ESP32-${meterId}`,
      firmware_version: '1.0.0',
      last_reboot: Math.floor(Date.now() / 1000) - Math.floor(Math.random() * 86400),
      error_count: Math.floor(Math.random() * 5),
      last_error: status === 'error' ? 'Sensor malfunction' : null
    }, {
      serviceName: this.serviceName,
      operation: 'meter.status.publish',
      baggage: { device_id: `ESP32-${meterId}`, status }
    });
    const payload = JSON.stringify(tracePayload);

    return new Promise((resolve, reject) => {
      this.client.publish(topic, payload, { qos: 1 }, (error) => {
        if (error) {
          reject(error);
        } else {
          console.log(chalk.blue(`📊 Status published to ${topic}: ${status}`));
          resolve();
        }
      });
    });
  }

  /**
   * Test MQTT connection
   */
  async testConnection() {
    if (!this.connected) {
      throw new Error('Not connected to MQTT broker');
    }

    const testTopic = 'test/connection';
    const testPayload = JSON.stringify({
      timestamp: Math.floor(Date.now() / 1000),
      message: 'Connection test',
      client_id: this.config.clientId
    });

    return new Promise((resolve, reject) => {
      this.client.publish(testTopic, testPayload, { qos: 0 }, (error) => {
        if (error) {
          reject(error);
        } else {
          console.log(chalk.green('✅ MQTT connection test successful'));
          resolve();
        }
      });
    });
  }

  /**
   * Get connection status
   */
  isConnected() {
    return this.connected;
  }

  /**
   * Get MQTT client info
   */
  getClientInfo() {
    return {
      connected: this.connected,
      clientId: this.config.clientId,
      host: this.config.host,
      port: this.config.port,
      reconnecting: this.client ? this.client.reconnecting : false
    };
  }
}

module.exports = MQTTPublisher;
