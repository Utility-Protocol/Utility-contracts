require('dotenv').config();

const config = {
  // Contract configuration
  contract: {
    network: process.env.STELLAR_NETWORK || 'testnet',
    contractId: process.env.CONTRACT_ID || 'CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS',
    rpcUrl: process.env.RPC_URL || 'https://soroban-testnet.stellar.org',
    friendbotUrl: process.env.FRIENDBOT_URL || 'https://friendbot.stellar.org',
    horizonUrl: process.env.HORIZON_URL || 'https://horizon-testnet.stellar.org'
  },

  // MQTT configuration
  mqtt: {
    host: process.env.MQTT_HOST || 'localhost',
    port: parseInt(process.env.MQTT_PORT) || 1883,
    username: process.env.MQTT_USERNAME || '',
    password: process.env.MQTT_PASSWORD || '',
    clientId: process.env.MQTT_CLIENT_ID || `meter-simulator-${Math.random().toString(16).substr(2, 8)}`,
    topic: process.env.MQTT_TOPIC || 'meters/+/usage',
    qos: parseInt(process.env.MQTT_QOS) || 1
  },

  // PostgreSQL pool health probe and adaptive sizing
  postgresql: {
    healthProbeSql: process.env.POSTGRES_HEALTH_PROBE_SQL || 'SELECT 1',
    healthTimeoutMs: parseInt(process.env.POSTGRES_HEALTH_TIMEOUT_MS) || 75,
    targetP99Ms: parseInt(process.env.POSTGRES_TARGET_P99_MS) || 100,
    minPoolSize: parseInt(process.env.POSTGRES_POOL_MIN) || 2,
    maxPoolSize: parseInt(process.env.POSTGRES_POOL_MAX) || 20,
    scaleUpThreshold: parseFloat(process.env.POSTGRES_SCALE_UP_THRESHOLD) || 0.8,
    scaleDownThreshold: parseFloat(process.env.POSTGRES_SCALE_DOWN_THRESHOLD) || 0.25,
    resizeCooldownMs: parseInt(process.env.POSTGRES_RESIZE_COOLDOWN_MS) || 30000
  },

  // Simulation defaults
  simulation: {
    defaultInterval: 30, // seconds
    baseWattHours: 100,  // base consumption per reading
    peakMultiplier: 3.0, // peak hour consumption multiplier
    variance: 0.3,        // 30% variance in consumption
    surgeProbability: 0.1 // 10% chance of surge
  },

  // Contract constants (matching the smart contract)
  constants: {
    HOUR_IN_SECONDS: 3600,
    DAY_IN_SECONDS: 86400,
    PEAK_HOUR_START: 64800,  // 18:00 UTC in seconds
    PEAK_HOUR_END: 75600,    // 21:00 UTC in seconds
    MAX_TIMESTAMP_DELAY: 300, // 5 minutes
    MIN_PRECISION_FACTOR: 1,
    MAX_USAGE_PER_UPDATE: 1000000000000 // 1 billion kWh max per update
  }
};

module.exports = config;
