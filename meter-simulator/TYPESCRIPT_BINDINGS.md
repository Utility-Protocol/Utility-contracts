# TypeScript Bindings Guide

## Overview

The IoT-Billing-Service smart contract now includes comprehensive TypeScript bindings that provide type-safe interfaces for the Node.js gateway. These bindings ensure perfect synchronization between the smart contract and frontend code.

## Installation

```bash
cd meter-simulator
npm install
```

This installs TypeScript and the necessary type definitions.

## File Structure

```
meter-simulator/
├── src/
│   ├── types.ts                      # Type definitions
│   ├── typed-contract-interface.ts   # Type-safe implementation
│   └── contract-interface.js         # Legacy JavaScript interface
├── tsconfig.json                     # TypeScript configuration
└── package.json
```

## Usage Examples

### 1. Basic Setup

```typescript
import TypedContractInterface from './typed-contract-interface';
import { RegisterMeterParams, BillingType } from './types';

// Initialize contract interface
const contract = new TypedContractInterface({
  network: 'testnet',
  rpcUrl: 'https://soroban-testnet.stellar.org',
  horizonUrl: 'https://horizon-testnet.stellar.org',
  contractId: 'CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS',
  friendbotUrl: 'https://friendbot.stellar.org'
});
```

### 2. Register a New Meter

```typescript
const params: RegisterMeterParams = {
  user: 'GD5DJQD7Y6KQLZBXNRCRJAY5PZQIIVMV5MW4FPX3BVUBQD2ZMJ7LFQXL',
  provider: 'GAB2JURIZ2XJ2LZ5ZQJKQWQJY5QNL7ZNVUKYB4XSV2LDEJYFGKZVQZK',
  off_peak_rate: BigInt(10), // 10 tokens per second
  token: 'XLM', // or token contract address
  device_public_key: 'base64_encoded_32_byte_public_key'
};

const result = await contract.register_meter(params);
console.log(`Meter ID: ${result.meter_id}`);
console.log(`Transaction: ${result.transaction_hash}`);
```

### 3. Register with Billing Mode

```typescript
import { RegisterMeterWithModeParams } from './types';

const params: RegisterMeterWithModeParams = {
  ...baseParams,
  billing_type: 'PostPaid' // or 'PrePaid'
};

const result = await contract.register_meter_with_mode(params);
```

### 4. Top Up Meter Balance

```typescript
await contract.top_up({
  meter_id: 1,
  amount: BigInt(1000000) // 1M tokens
});
```

### 5. Submit Signed Usage Data

```typescript
import { DeductUnitsParams, SignedUsageData } from './types';

const signedData: SignedUsageData = {
  meter_id: 1,
  timestamp: BigInt(Math.floor(Date.now() / 1000)),
  watt_hours_consumed: BigInt(250),
  units_consumed: BigInt(1),
  signature: 'base64_signature_64_bytes',
  public_key: 'base64_public_key_32_bytes'
};

await contract.deduct_units({ signed_data: signedData });
```

### 6. Claim Earnings

```typescript
await contract.claim({
  meter_id: 1
});
```

### 7. Read Meter Data

```typescript
import { Meter } from './types';

const meter: Meter | null = await contract.get_meter(1);
if (meter) {
  console.log('Balance:', meter.balance.toString());
  console.log('Is Active:', meter.is_active);
  console.log('Billing Type:', meter.billing_type);
}
```

### 8. Calculate Expected Depletion

```typescript
const depletionTime = await contract.calculate_expected_depletion(1);
if (depletionTime) {
  const date = new Date(Number(depletionTime) * 1000);
  console.log('Expected depletion:', date.toISOString());
}
```

### 9. Withdraw Earnings

```typescript
await contract.withdraw_earnings({
  meter_id: 1,
  amount_usd_cents: BigInt(5000) // $50.00 USD
});
```

### 10. Check if Meter is Offline

```typescript
const isOffline = await contract.is_meter_offline(1);
if (isOffline) {
  console.log('⚠️ Meter has not reported in over 1 hour');
}
```

## Type Definitions

### Core Types

- `MeterId` - Unique meter identifier (number)
- `StellarAddress` - Stellar public key (string)
- `TokenAddress` - Token contract address (string)
- `BillingType` - `'PrePaid' | 'PostPaid'`

### Interfaces

- `Meter` - Complete meter state and configuration
- `UsageData` - Usage statistics and tracking
- `SignedUsageData` - Signed telemetry from device
- `ProviderWithdrawalWindow` - Daily withdrawal tracking
- `PriceData` - Oracle price information

### Contract Methods

All smart contract methods are available with full type safety:

**Read Methods:**
- `get_minimum_balance_to_flow(): Promise<bigint>`
- `get_meter(meter_id: MeterId): Promise<Meter | null>`
- `get_usage_data(meter_id: MeterId): Promise<UsageData | null>`
- `calculate_expected_depletion(meter_id: MeterId): Promise<bigint | null>`
- `is_meter_offline(meter_id: MeterId): Promise<boolean>`

**Write Methods:**
- `register_meter(params: RegisterMeterParams): Promise<RegisterMeterResult>`
- `top_up(params: TopUpParams): Promise<void>`
- `deduct_units(params: DeductUnitsParams): Promise<void>`
- `claim(params: ClaimParams): Promise<void>`
- `withdraw_earnings(params: WithdrawEarningsParams): Promise<void>`

## Error Handling

```typescript
import { ContractError, ContractErrorCode } from './types';

try {
  await contract.deduct_units(params);
} catch (error) {
  if (error instanceof ContractError) {
    switch (error.code) {
      case ContractErrorCode.InvalidSignature:
        console.error('❌ Signature verification failed');
        break;
      case ContractErrorCode.MeterNotFound:
        console.error('❌ Meter not found');
        break;
      case ContractErrorCode.TimestampTooOld:
        console.error('❌ Timestamp is too old (replay attack prevention)');
        break;
      default:
        console.error('Contract error:', error.message);
    }
  } else {
    console.error('Network error:', error);
  }
}
```

## Constants

Access contract constants directly:

```typescript
import { CONTRACT_CONSTANTS } from './types';

console.log('Minimum balance:', CONTRACT_CONSTANTS.MINIMUM_BALANCE_TO_FLOW.toString());
console.log('Peak hour start:', new Date(CONTRACT_CONSTANTS.PEAK_HOUR_START * 1000).toISOString());
console.log('Max usage per update:', CONTRACT_CONSTANTS.MAX_USAGE_PER_UPDATE.toString());
```

## Event Monitoring

```typescript
import { ContractEvent, UsageReportedEvent } from './types';

// Listen for contract events (implementation depends on your event listener)
function handleEvent(event: ContractEvent) {
  switch (event.event_type) {
    case 'UsageReported':
      const usageEvent = event as UsageReportedEvent;
      console.log(`Meter ${usageEvent.meter_id}: ${usageEvent.units_consumed} units, cost: ${usageEvent.cost}`);
      break;
    case 'Active':
      console.log(`Meter ${event.meter_id} activated`);
      break;
    case 'Inactive':
      console.log(`Meter ${event.meter_id} deactivated`);
      break;
  }
}
```

## Migration from JavaScript

If you're using the legacy JavaScript interface (`contract-interface.js`), migration is straightforward:

```javascript
// Old JavaScript way
const contract = new ContractInterface(config);
await contract.topUp(1, 1000000);

// New TypeScript way
const contract = new TypedContractInterface(config);
await contract.top_up({ meter_id: 1, amount: BigInt(1000000) });
```

Benefits of TypeScript bindings:
- ✅ Compile-time type checking
- ✅ IntelliSense autocomplete
- ✅ Automatic documentation
- ✅ Catch errors before runtime
- ✅ Better refactoring support

## Building for Production

```bash
# Compile TypeScript to JavaScript
npm run build

# Output will be in ./dist directory
```

## Testing

```typescript
import { describe, it, expect } from '@jest/globals';

describe('TypedContractInterface', () => {
  it('should register a meter', async () => {
    const contract = new TypedContractInterface(testConfig);
    const result = await contract.register_meter(testParams);
    expect(result.meter_id).toBeDefined();
    expect(result.transaction_hash).toBeDefined();
  });
  
  it('should get meter data', async () => {
    const contract = new TypedContractInterface(testConfig);
    const meter = await contract.get_meter(1);
    expect(meter).toBeDefined();
    expect(meter?.billing_type).toBe('PrePaid');
  });
});
```

## Best Practices

1. **Always use BigInt for large numbers** - Token amounts can exceed JavaScript's safe integer limit
2. **Validate addresses** - Use `isStellarAddress()` type guard before making calls
3. **Handle errors gracefully** - Contract operations can fail for various reasons
4. **Check meter status** - Verify `is_active` before expecting flow
5. **Monitor timestamps** - Ensure device signatures are recent (< 5 minutes)

## API Reference

For complete API reference, see:
- `types.ts` - All type definitions and interfaces
- `typed-contract-interface.ts` - Implementation details

## Support

For issues or questions about TypeScript bindings:
1. Check the type definitions in `types.ts`
2. Review examples in this guide
3. Consult the main contract documentation

---

**Generated**: March 26, 2026  
**Version**: 1.0.0  
**Contract**: CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS
