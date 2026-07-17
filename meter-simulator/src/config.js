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

  // Per-tenant API rate limiting defaults
  rateLimit: {
    capacity: parseInt(process.env.RATE_LIMIT_CAPACITY) || 60,
    refillRatePerSecond: parseFloat(process.env.RATE_LIMIT_REFILL_PER_SECOND) || 1,
    idleTtlMs: parseInt(process.env.RATE_LIMIT_IDLE_TTL_MS) || 10 * 60 * 1000
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
