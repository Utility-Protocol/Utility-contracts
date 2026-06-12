# Meter Simulator CLI

A Node.js CLI tool that mimics an ESP32 sending usage data to the IoT-Billing-Service smart contracts for local development and testing.

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
git clone https://github.com/IoT-Billing-Service/Billing-contracts.git
cd Billing-contracts/meter-simulator

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

The simulator integrates with the IoT-Billing-Service smart contract:

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

- 📖 [IoT-Billing-Service Documentation](../README.md)
- 🐛 [Issues](https://github.com/IoT-Billing-Service/Billing-contracts/issues)
- 💬 [Discussions](https://github.com/IoT-Billing-Service/Billing-contracts/discussions)
