# Meter Simulator CLI

A Node.js CLI tool that mimics an ESP32 sending usage data to the Utility-Protocol smart contracts for local development and testing.

## Features

- 🔐 **Ed25519 Key Generation**: Generate cryptographic key pairs for device authentication
- 📝 **Meter Registration**: Register new meters with the smart contract
- 📊 **Realistic Usage Simulation**: Simulate energy consumption patterns with peak/off-peak pricing
- 📡 **MQTT Support**: Publish usage data via MQTT (matching ESP32 behavior)
- 🔗 **Direct Contract Integration**: Submit data directly to Soroban contracts
- ⚡ **Multiple Simulation Modes**: Realistic, surge, and low consumption patterns
- 📈 **Real-time Monitoring**: Track meter status and usage statistics

## Installation

```bash
# Clone the repository
git clone https://github.com/Utility-Protocol/Utility-contracts.git
cd Utility-contracts/meter-simulator

# Install dependencies
npm install

# Copy environment configuration
cp .env.example .env

# Make the CLI executable (Linux/Mac)
chmod +x src/index.js
```

## Configuration

Edit `.env` file with your settings:

```env
# Stellar Network
STELLAR_NETWORK=testnet
CONTRACT_ID=CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS

# MQTT Broker (optional)
MQTT_HOST=localhost
MQTT_PORT=1883
MQTT_USERNAME=
MQTT_PASSWORD=

# Simulation Settings
DEFAULT_INTERVAL=30
BASE_WATT_HOURS=100
```

## Usage

### 1. Generate Device Keys

```bash
node src/index.js generate-keys --output my-device-keys.json
```

This creates an Ed25519 key pair for device authentication:
- Private key: Keep secure!
- Public key: Used for meter registration

### 2. Register a Meter

```bash
node src/index.js register \
  --keys my-device-keys.json \
  --user GD5DJQD7Y6KQLZBXNRCRJAY5PZQIIVMV5MW4FPX3BVUBQD2ZMJ7LFQXL \
  --provider GAB2JURIZ2XJ2LZ5ZQJKQWQJY5QNL7ZNVUKYB4XSV2LDEJYFGKZVQZK \
  --rate 10
```

### 3. Start Simulation

#### Direct Contract Calls:
```bash
node src/index.js simulate --config meter-config.json --interval 30
```

#### Via MQTT:
```bash
node src/index.js simulate --config meter-config.json --mqtt --interval 30
```

### 4. Send Single Reading

```bash
node src/index.js send-reading \
  --config meter-config.json \
  --watts 250 \
  --units 1
```

### 5. Check Meter Status

```bash
node src/index.js status --config meter-config.json
```

## Simulation Modes

### Realistic Mode (default)
- Base consumption with random variance
- Peak hour multipliers (18:00-21:00 UTC)
- Random surge events

### Surge Mode
- High consumption patterns
- 3x base usage with minimal variance
- Additional peak hour multipliers

### Low Mode
- Minimal consumption (30% of base)
- Higher variance at low levels
- Reduced peak hour impact

## MQTT Integration

The simulator can publish usage data via MQTT to match real ESP32 behavior:

### MQTT Topics

- **Usage Data**: `meters/{meter_id}/usage`
- **Heartbeat**: `meters/{meter_id}/heartbeat`
- **Status**: `meters/{meter_id}/status`
- **Commands**: `meters/{meter_id}/commands`

### Payload Format

```json
{
  "meter_id": 1,
  "timestamp": 1710000000,
  "watt_hours_consumed": 250,
  "units_consumed": 1,
  "signature": "base64_encoded_64_byte_signature",
  "public_key": "base64_encoded_32_byte_public_key",
  "device_id": "ESP32-1",
  "firmware_version": "1.0.0",
  "battery_level": 85,
  "signal_strength": -70,
  "temperature": 25
}
```

## Contract Integration

The simulator integrates with the Utility-Protocol smart contract:

### Signed Usage Data

All usage data is cryptographically signed using Ed25519:
- Message includes: meter_id, timestamp, watt_hours_consumed, units_consumed
- Signature verified by smart contract
- Prevents tampering and replay attacks

### Peak/Off-Peak Pricing

- **Off-peak hours**: 21:00-18:00 UTC
- **Peak hours**: 18:00-21:00 UTC
- **Peak multiplier**: 1.5x off-peak rate
- Automatic rate calculation based on timestamp

## Development

### Project Structure

```
meter-simulator/
├── src/
│   ├── index.js          # Main CLI entry point
│   ├── config.js         # Configuration management
│   ├── meter-device.js   # Device simulation logic
│   ├── contract-interface.js # Contract interaction
│   └── mqtt-publisher.js # MQTT client
├── package.json
├── .env.example
└── README.md
```

### Testing

```bash
# Run tests
npm test

# Lint code
npm run lint
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `STELLAR_NETWORK` | Stellar network (testnet/mainnet) | testnet |
| `CONTRACT_ID` | Smart contract ID | - |
| `MQTT_HOST` | MQTT broker host | localhost |
| `MQTT_PORT` | MQTT broker port | 1883 |
| `DEFAULT_INTERVAL` | Simulation interval (seconds) | 30 |

## Security Considerations

- 🔐 Private keys are stored locally and never transmitted
- ✅ All usage data is cryptographically signed
- 🕐 Timestamp validation prevents replay attacks
- 🚫 Maximum usage limits prevent abuse
- 🔑 Device authentication via public key verification

## Troubleshooting

### Common Issues

1. **"Meter not found" error**
   - Ensure meter is registered with the contract
   - Check meter-config.json contains correct meter_id

2. **"Invalid signature" error**
   - Verify keys match between registration and simulation
   - Check device public key is correctly registered

3. **MQTT connection failed**
   - Verify MQTT broker is running
   - Check host/port configuration
   - Validate credentials if authentication required

4. **"Timestamp too old" error**
   - Ensure system clock is synchronized
   - Check network connectivity

### Debug Mode

Enable verbose logging:
```bash
DEBUG=* node src/index.js simulate
```

## Contributing

1. Fork the repository
2. Create feature branch
3. Make changes
4. Add tests
5. Submit pull request

## License

MIT License - see LICENSE file for details.

## Support

- 📖 [Utility-Protocol Documentation](../README.md)
- 🐛 [Issues](https://github.com/Utility-Protocol/Utility-contracts/issues)
- 💬 [Discussions](https://github.com/Utility-Protocol/Utility-contracts/discussions)

## PostgreSQL Pool Health Probe and Adaptive Sizing

The simulator now includes a reusable PostgreSQL pool health probe for services that depend on database-backed ingestion, telemetry, or settlement workers. The probe is intentionally dependency-light: pass any `pg.Pool`-compatible object with a `query(sql)` method and the standard pool counters (`totalCount`, `idleCount`, and `waitingCount`).

### Architecture

1. **Health probe** runs `SELECT 1` (or `POSTGRES_HEALTH_PROBE_SQL`) under a strict timeout.
2. **Metrics snapshot** captures total, idle, waiting, utilization, and rolling P99 latency.
3. **Adaptive sizing** recommends scale-up when P99 exceeds the 100ms target, utilization is high, or clients are waiting; it recommends scale-down only when utilization is low and no clients are queued.
4. **Cooldown guard** prevents pool-size oscillation during short-lived traffic spikes.
5. **Monitoring contract** exposes status values of `healthy`, `degraded`, and `unhealthy` for dashboards and alert rules.

### Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `POSTGRES_HEALTH_PROBE_SQL` | SQL used for liveness checks | `SELECT 1` |
| `POSTGRES_HEALTH_TIMEOUT_MS` | Probe timeout | `75` |
| `POSTGRES_TARGET_P99_MS` | Critical-path latency target | `100` |
| `POSTGRES_POOL_MIN` | Minimum recommended pool size | `2` |
| `POSTGRES_POOL_MAX` | Maximum recommended pool size | `20` |
| `POSTGRES_SCALE_UP_THRESHOLD` | Utilization threshold for scale up | `0.8` |
| `POSTGRES_SCALE_DOWN_THRESHOLD` | Utilization threshold for scale down | `0.25` |
| `POSTGRES_RESIZE_COOLDOWN_MS` | Minimum time between size changes | `30000` |

### Example

```js
const { Pool } = require('pg');
const PostgresPoolHealthProbe = require('./src/postgres-pool-health');
const config = require('./src/config');

const pool = new Pool({ max: config.postgresql.maxPoolSize });
const probe = new PostgresPoolHealthProbe(pool, {
  timeoutMs: config.postgresql.healthTimeoutMs,
  targetP99Ms: config.postgresql.targetP99Ms,
  minSize: config.postgresql.minPoolSize,
  maxSize: config.postgresql.maxPoolSize
});

setInterval(async () => {
  const health = await probe.check();
  console.log(JSON.stringify(health));
}, 10000);
```

### Monitoring and Alerting

Recommended production alerts:

- Page when `status="unhealthy"` for 2 consecutive probes.
- Warn when rolling P99 probe latency exceeds `POSTGRES_TARGET_P99_MS` for 5 minutes.
- Warn when `waitingClients > 0` for 3 minutes.
- Warn when the adaptive recommendation stays at `maxPoolSize` for 10 minutes, which indicates database capacity or query tuning work is needed.

### Runbook

1. Confirm the database endpoint, credentials, and network path are healthy.
2. Check pool metrics: utilization, waiting clients, and rolling P99 latency.
3. If waiting clients are non-zero and database CPU/IO have headroom, apply the scale-up recommendation using a blue-green or canary rollout.
4. If the pool is at maximum size, inspect slow queries and database saturation before increasing limits.
5. During recovery, keep canaries below 10% traffic until `healthy` status and P99 below 100ms are stable for at least 15 minutes.
