# Gasless Transaction Relay - Issue #131

## Overview

The Gasless Transaction Relay is an EIP-2771 compatible system that enables the Utility Protocol to sponsor gas costs for approved operations during user onboarding. This removes friction for new users who would otherwise need to acquire XLM to pay gas fees.

## Architecture

### Core Components

1. **Gasless Relay (`gasless_relay.rs`)**
   - Main contract implementing EIP-2771 meta-transaction forwarding
   - Manages nonce counters for replay protection
   - Tracks sponsorship pool balance
   - Forwards authenticated meta-transactions

2. **Signature Verification (`gasless_relay_sig_verify.rs`)**
   - Ed25519 signature verification for meta-transactions
   - Timestamp validation (6-hour window)
   - Approved forwarder validation
   - Signer address recovery
   - Nonce-based replay attack prevention

3. **Sponsorship Policy Engine (`gasless_relay_policy.rs`)**
   - Fine-grained control over which operations are sponsored
   - Multiple sponsorship statuses (FullySponsored, PartiallySponsored, Suspended)
   - Per-operation gas limits
   - Daily transaction rate limiting
   - Operation statistics tracking
   - Policy suspension/resumption for operational control

## Key Features

### 1. EIP-2771 Compatibility

The relay implements the EIP-2771 standard for meta-transactions:

```
User → signs meta-transaction → Forwarder → Relay contract → Target contract
```

Meta-transactions include:
- `from` - Originating user address
- `to` - Target contract address
- `value` - XLM amount in stroops
- `data` - Encoded function call
- `nonce` - For replay protection
- `gas_price` - Gas price offered
- `gas_limit` - Maximum gas
- `deadline` - Transaction deadline

### 2. Replay Attack Prevention

- **Nonce Management**: Each user has a sequential nonce that must increment
- **Forwarder Nonce Tracking**: Forwarders have independent nonce sequences
- **Signature Aging**: Signatures are only valid for 6 hours

### 3. Rate Limiting

- **Per-User Limits**: Configurable maximum transactions per time period
- **Per-Operation Limits**: Daily limits on specific operations
- **Dynamic Period Tracking**: Automatic reset at period boundaries

### 4. Sponsorship Pool Management

- **Balance Tracking**: Real-time pool balance monitoring
- **Deduction on Use**: Immediate deduction from pool when transaction is sponsored
- **Top-Up Functionality**: Admin can add funds to pool
- **Low Balance Warnings**: Can trigger alerts when balance is low

### 5. Flexible Sponsorship Policies

```rust
DetailedSponsorshipPolicy {
    operation_id: Symbol,           // Operation identifier
    status: SponsorshipStatus,      // Full, Partial, Suspended, NotSponsored
    max_gas: u64,                   // Maximum gas per transaction
    sponsorship_percentage: u32,    // 0-100% sponsorship
    daily_tx_limit: u32,            // Transactions per day
    cost_per_tx: i128,              // Cost in stroops
    created_at: u64,                // Timestamp
    updated_at: u64,                // Last update
    admin: Address,                 // Policy administrator
    description: String,            // Human-readable description
}
```

## Usage Flow

### 1. Initialize the Relay

```rust
relay.initialize(env, admin_address, initial_pool_balance)?;
```

### 2. Register Trusted Forwarder

```rust
relay.register_forwarder(env, forwarder_address, public_key)?;
```

### 3. Create Sponsorship Policies

```rust
relay.register_sponsorship_policy(env, operation_id, policy)?;
```

### 4. Set Rate Limits

```rust
relay.set_rate_limit(env, user_address, max_txs_per_period, period_seconds)?;
```

### 5. Forward Meta-Transaction

```rust
relay.forward_meta_transaction(env, meta_tx_request, signature)?;
```

## Error Handling

The relay defines comprehensive error codes:

| Error | Code | Meaning |
|-------|------|---------|
| InvalidSignature | 1 | Signature verification failed |
| NonceMismatch | 2 | Nonce is invalid or replay attempted |
| RateLimitExceeded | 3 | User exceeded rate limit |
| OperationNotSponsored | 4 | Operation not in approved list |
| UntrustedForwarder | 5 | Forwarder not in whitelist |
| DeadlineExpired | 6 | Meta-transaction deadline passed |
| InsufficientSponsorshipBalance | 7 | Pool doesn't have enough funds |
| PolicyRejected | 8 | Operation failed policy check |

## Security Considerations

### 1. Signature Verification

- Ed25519 signatures verified before processing
- Timestamps checked to prevent replay via old signatures
- Public key validation against approved forwarders

### 2. Nonce Management

- Sequential nonce enforcement prevents replay attacks
- Nonce increments required for each new transaction
- Independent nonce sequences for different forwarders

### 3. Rate Limiting

- Prevents denial-of-service through sponsorship abuse
- Per-user and per-operation limits
- Automatic reset at period boundaries

### 4. Access Control

- Admin-only operations for configuration
- Policy management restricted to admins
- Forwarder registration requires admin approval

### 5. Pool Protection

- Balance checks before each transaction
- Immediate deduction to prevent double-spending
- Emergency suspension capability for policies

## Operational Procedures

### Monitoring Pool Balance

```rust
let balance = relay.get_sponsorship_pool_balance(env);
if balance < CRITICAL_THRESHOLD {
    // Alert admin
}
```

### Checking User Nonce

```rust
let current_nonce = relay.get_nonce(env, user_address);
```

### Suspending an Operation

```rust
engine.suspend_policy(env, operation_id)?;
```

### Resuming an Operation

```rust
engine.resume_policy(env, operation_id, SponsorshipStatus::FullySponsored)?;
```

### Topping Up Sponsorship Pool

```rust
relay.top_up_sponsorship_pool(env, amount)?;
```

## Performance Characteristics

- **Nonce Lookup**: O(1) per user
- **Policy Lookup**: O(n) where n = number of policies (typically small)
- **Rate Limit Check**: O(1) per user
- **Signature Verification**: Native Soroban crypto (optimized)

## Testing

The implementation includes three levels of tests:

1. **Unit Tests** (`gasless_relay_tests.rs`)
   - 15 focused test cases
   - Tests individual component functionality
   - Validates error conditions

2. **Integration Tests** (`gasless_relay_integration_tests.rs`)
   - 16 comprehensive scenarios
   - Tests cross-component interactions
   - Validates complete workflows
   - Covers edge cases and recovery scenarios

3. **Property Tests**
   - Can be added using quickcheck for fuzzing
   - Test invariants across random inputs

## Future Enhancements

1. **Batch Meta-Transactions**: Support multiple operations per relay call
2. **Conditional Sponsorship**: Sponsor only if certain conditions are met
3. **Dynamic Pricing**: Adjust sponsorship costs based on pool balance
4. **Multi-Sig Approval**: Require multiple admins to top up pool
5. **Statistics Export**: Detailed reporting on sponsorship usage
6. **Cross-Contract Integration**: Relay calls to other contracts
7. **Emergency Mode**: Pause all sponsorship without suspending policies

## Governance

- **Admin Role**: Controls initialization, policy creation, forwarder registration
- **Policy Admin**: Can suspend/resume individual policies
- **Monitor Role**: Can check pool balance and statistics (future)

## Cost Analysis

Each sponsored transaction costs:
- Storage: ~100 bytes per rate limit entry
- Computation: ~5,000 stroops for relay overhead
- Gas: Depends on underlying operation

Total cost to protocol: `sponsorship_cost + relay_overhead + gas_cost`

## Migration Guide

### Deploying to Production

1. Deploy relay contract
2. Register trusted forwarders
3. Create initial sponsorship policies
4. Set conservative rate limits
5. Fund sponsorship pool
6. Monitor usage patterns
7. Adjust policies based on metrics

### Rollback Plan

- Suspend all policies: `suspend_policy()` for each
- Redirect traffic: Update forwarder configuration
- Drain pool: `get_sponsorship_pool_balance()` and manual transfer
- Disable relay: Empty pool, remove all policies

## References

- **EIP-2771**: https://eips.ethereum.org/EIPS/eip-2771
- **Soroban Documentation**: https://developers.stellar.org/soroban
- **Project Issue #131**: Gasless Transaction Relay for User Onboarding
