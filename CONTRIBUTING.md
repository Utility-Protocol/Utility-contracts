# Contributing to Billing-contracts

Welcome to the Billing-contracts project! This guide will help you contribute effectively, whether you're working on hardware (C++/Arduino) or smart contracts (Soroban/Rust).

## Project Overview

Billing-contracts is a utility billing system built on Stellar that allows:
- Individual meter billing and management
- Group billing for property managers
- Real-time balance monitoring
- Automated payment processing

## Development Areas

### 🔌 Hardware Development (C++/Arduino)
Hardware components handle the physical meter readings and communicate with the blockchain.

### ⚡ Smart Contract Development (Rust/Soroban)
Smart contracts handle billing logic, payment processing, and account management.

---

## Hardware Development Guidelines

### 🛠️ Development Environment

**Required Tools:**
- Arduino IDE 2.0+ or PlatformIO
- C++17 compatible compiler
- ESP32 or Arduino-compatible hardware
- Stellar SDK for embedded systems (if available)

**Recommended Setup:**
```bash
# For PlatformIO users
pio project init --board esp32dev
pio lib install "Stellar SDK"
```

### 📋 Hardware Standards

**Meter Reading Specifications:**
- Sample rate: Minimum 1 reading per second
- Accuracy: ±1% for power measurements
- Data format: JSON over MQTT/HTTP
- Power consumption: < 100mA during operation

**Communication Protocol:**
```json
{
  "meter_id": 12345,
  "timestamp": 1640995200,
  "reading": 1250,
  "unit": "watt_hours",
  "signature": "0x..."
}
```

### 🔧 Code Standards

**C++ Guidelines:**
- Use `camelCase` for variables
- Use `PascalCase` for classes
- Use `UPPER_SNAKE_CASE` for constants
- Include comprehensive error handling
- Memory management: prefer RAII patterns

**Example Structure:**
```cpp
class UtilityMeter {
private:
    uint32_t meterId;
    float currentReading;
    StellarClient* stellarClient;
    
public:
    UtilityMeter(uint32_t id, StellarClient* client);
    bool takeReading();
    bool submitToBlockchain();
    float getCurrentReading() const;
};
```

### 🧪 Testing Hardware

**Unit Testing:**
- Use ArduinoUnit or GoogleTest framework
- Test meter accuracy with known loads
- Validate communication protocols
- Test error recovery mechanisms

**Integration Testing:**
- Test against testnet blockchain
- Validate contract interactions
- Test network connectivity issues
- Power consumption validation

### 📦 Hardware Deployment

**Pre-deployment Checklist:**
- [ ] Meter calibration completed
- [ ] Network connectivity verified
- [ ] Testnet transactions successful
- [ ] Power consumption within limits
- [ ] Error handling tested
- [ ] Firmware version documented

---

## Smart Contract Development Guidelines

### 🛠️ Development Environment

**Required Tools:**
- Rust 1.70+
- Soroban CLI
- Stellar Testnet access

**Setup:**
```bash
# Install Soroban CLI
cargo install soroban-cli

# Build contracts
make build

# Run tests
make test
```

### 📋 Contract Standards

**Gas Optimization:**
- Minimize storage operations
- Use efficient data structures
- Batch operations when possible
- Consider gas costs in design

**Security Guidelines:**
- Validate all inputs
- Use proper access controls
- Implement reentrancy protection
- Audit critical functions

### 🧪 Testing Contracts

**Test Coverage:**
- Unit tests for all functions
- Integration tests for workflows
- Edge case testing
- Gas usage analysis

---

## 🚀 Contribution Workflow

### 1. Fork and Clone
```bash
git clone https://github.com/your-username/Billing-contracts.git
cd Billing-contracts
```

### 2. Create Feature Branch
```bash
git checkout -b feature/hardware-meter-optimization
```

### 3. Development

**For Hardware Changes:**
- Modify C++/Arduino code in `hardware/` directory
- Update documentation
- Add tests
- Verify against testnet

**For Contract Changes:**
- Modify Rust code in `contracts/` directory
- Update tests
- Run gas analysis
- Document changes

### 4. Testing
```bash
# Hardware tests
cd hardware && pio test

# Contract tests
cd contracts && cargo test

# Integration tests
make integration-test
```

### 5. Documentation
- Update README.md if needed
- Add inline code comments
- Update API documentation
- Include hardware specifications

### 6. Pull Request
- Create descriptive PR title
- Describe changes in detail
- Include test results
- Tag relevant reviewers

## 🏷️ Label Guidelines

**Hardware PRs:**
- `hardware`: For hardware-related changes
- `arduino`: For Arduino-specific code
- `embedded`: For embedded systems work

**Contract PRs:**
- `contracts`: For smart contract changes
- `soroban`: For Soroban-specific features
- `backend`: For backend logic

**General:**
- `bugfix`: For bug fixes
- `feature`: For new features
- `documentation`: For documentation updates
- `testing`: For test improvements

## 🐛 Bug Reports

**Hardware Bugs:**
Include:
- Hardware model and firmware version
- Environmental conditions
- Error logs
- Reproduction steps
- Expected vs actual behavior

**Contract Bugs:**
Include:
- Contract version
- Transaction hash
- Input parameters
- Error message
- Expected vs actual behavior

## 💡 Feature Requests

**Hardware Features:**
- Describe the hardware capability
- Explain the user benefit
- Consider power/processing constraints
- Include implementation suggestions

**Contract Features:**
- Describe the functionality
- Explain the use case
- Consider gas implications
- Include API design suggestions

## 🤝 Community Guidelines

- Be respectful and inclusive
- Provide constructive feedback
- Help others learn
- Follow the code of conduct
- Focus on what's best for the community

## 📞 Get Help

- **Discord**: [Utility-Drip Community](https://discord.gg/utilitydrip)
- **GitHub Issues**: For bug reports and feature requests
- **Documentation**: Check the `/docs` directory
- **Examples**: See `/examples` directory

## 📜 License

By contributing, you agree that your contributions will be licensed under the same license as the project.

---

Thank you for contributing to Billing-contracts! 🎉
