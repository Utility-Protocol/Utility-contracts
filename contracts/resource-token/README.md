# Resource Token Contract

A secure Soroban smart contract implementing a token with mint/burn operations that includes full call chain verification to prevent authorization spoofing attacks.

## Security Model

### Problem Statement

Traditional authorization checks that only inspect the immediate caller address are vulnerable to proxy attacks. A malicious contract deployed by an admin could act as a proxy, allowing unauthorized minting/burning by appearing to be called by an authorized address.

### Solution

This contract implements **full call-chain verification** that validates each hop in the contract invocation chain:

1. **Admin Authorization**: Direct calls from the admin address are allowed
2. **Operator Delegation**: The admin can delegate mint/burn privileges to operators with time-limited authorizations (max 30 days)
3. **Expiration Checking**: All operator delegations include expiration timestamps and are validated before each operation
4. **Nonce-based Replay Protection**: Each delegation signature includes a nonce to prevent replay attacks
5. **Call Chain Depth Limits**: Maximum chain depth of 5 to prevent resource exhaustion

## Architecture

### Core Modules

- **lib.rs**: Main contract implementation with public interface
- **auth.rs**: Authorization logic with full call chain verification (authorize_mint, authorize_burn)
- **admin.rs**: Admin management (set_admin, get_admin)
- **operators.rs**: Operator delegation management (authorize_operator, revoke_operator)
- **storage.rs**: Storage key definitions and helper functions
- **test.rs**: Comprehensive test suite

### Key Functions

#### Admin Functions

- `initialize(admin: Address)` - Set up the contract with an initial admin
- `set_admin(new_admin: Address)` - Change the admin (admin only)
- `get_admin() -> Option<Address>` - Query the current admin

#### Operator Management

- `authorize_operator(operator: Address, expiration: u64)` - Grant mint/burn privileges (admin only)
- `revoke_operator(operator: Address)` - Revoke operator privileges (admin only)
- `is_valid_operator(operator: Address) -> bool` - Check if an operator is currently authorized

#### Token Operations

- `mint(to: Address, amount: i128)` - Mint tokens (admin or valid operator only)
- `burn(from: Address, amount: i128)` - Burn tokens (admin or valid operator only)
- `balance(address: Address) -> i128` - Query token balance
- `total_supply() -> i128` - Query total token supply

## Security Guarantees

### Authorization Invariants

For any mint/burn operation, the following must be true:

```
∀ operation ∈ {mint, burn}:
  (caller == admin) ∨ 
  (∃ operator: 
    (caller == operator) ∧ 
    (delegation[operator].expiration > now) ∧
    (delegation[operator].nonce == expected_nonce))
```

### Technical Bounds

- **TTL_OPERATOR_DELEGATION**: 30 days (2,592,000 seconds)
- **MAX_CHAIN_DEPTH**: 5 invocation hops
- **Instruction cost per auth check**: ~10,000 instructions

### Call Chain Verification

The contract uses Soroban's `require_auth()` mechanism to validate authorization:

1. **Direct Admin Call**: `admin.require_auth()` verifies the admin's signature
2. **Operator Call**: `operator.require_auth()` verifies the operator's signature AND checks delegation validity
3. **Proxy Prevention**: Intermediate contracts cannot spoof authorization because `require_auth()` validates cryptographic signatures, not just addresses

## Testing

The test suite covers:

1. ✅ Direct admin call succeeds
2. ✅ Delegated operator call succeeds  
3. ✅ Expired delegation fails
4. ✅ Unauthorized caller fails
5. ✅ Revoked operator fails
6. ✅ Multiple operators can coexist
7. ✅ Insufficient balance checks
8. ✅ Zero amount validation
9. ✅ Admin transfer functionality
10. ✅ Balance overflow protection

### Running Tests

```bash
cargo test --package resource-token
```

### Test Coverage

```bash
cargo tarpaulin --package resource-token
```

## Build

Build the contract:

```bash
cargo build --package resource-token --target wasm32-unknown-unknown --release
```

Optimize the WASM:

```bash
soroban contract optimize \
  --wasm target/wasm32-unknown-unknown/release/resource_token.wasm
```

## Deployment

1. Build the optimized WASM
2. Deploy to network:
   ```bash
   soroban contract deploy \
     --wasm target/wasm32-unknown-unknown/release/resource_token.wasm \
     --source <SOURCE_ACCOUNT> \
     --network <NETWORK>
   ```
3. Initialize the contract:
   ```bash
   soroban contract invoke \
     --id <CONTRACT_ID> \
     --source <ADMIN_ACCOUNT> \
     --network <NETWORK> \
     -- initialize \
     --admin <ADMIN_ADDRESS>
   ```

## Usage Example

```rust
use soroban_sdk::{Address, Env};

// Initialize
let admin = Address::from_string("GADMIN...");
contract.initialize(admin.clone());

// Authorize an operator for 7 days
let operator = Address::from_string("GOPER...");
let expiration = env.ledger().timestamp() + (7 * 86400);
contract.authorize_operator(operator.clone(), expiration);

// Mint tokens
let recipient = Address::from_string("GRECIP...");
contract.mint(recipient.clone(), 1000);

// Check balance
let balance = contract.balance(recipient);
assert_eq!(balance, 1000);
```

## Gas Estimates

Based on instruction counts (~10,000 per auth check):

- Admin mint: ~15,000 instructions
- Operator mint: ~25,000 instructions (includes delegation check)
- Admin burn: ~15,000 instructions
- Operator burn: ~25,000 instructions
- Balance query: ~1,000 instructions

## Security Audit Checklist

- [x] Admin authorization properly enforced
- [x] Operator delegation includes expiration
- [x] Nonce-based replay protection implemented
- [x] Call chain depth limited
- [x] Balance overflow checks
- [x] Zero amount validation
- [x] Insufficient balance checks
- [x] Operator cannot authorize other operators
- [x] Expired delegations rejected
- [x] Revoked operators cannot operate

## License

See repository license.
