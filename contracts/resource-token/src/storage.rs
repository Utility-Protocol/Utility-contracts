use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{contracttype, Address, Bytes, Env, Vec as SdkVec};

pub const NAMESPACE_PREFIX: [u8; 4] = [0x52, 0x45, 0x53, 0x4f]; // "RESO"

/// Maximum TTL for operator delegation (30 days in seconds)
pub const TTL_OPERATOR_DELEGATION: u32 = 30 * 86400;

/// Maximum call chain depth allowed
pub const MAX_CHAIN_DEPTH: u32 = 5;

/// Maximum total supply of resource-backed tokens (10^15 base units).
///
/// Each token is backed 1:1 by a real-world resource deposit, so the total
/// supply must never exceed the maximum backable amount. `mint` enforces
/// `total_supply <= MAX_SUPPLY`; combined with overflow-checked accounting this
/// keeps the invariant `total_supply == Σ(balances) <= MAX_SUPPLY`.
pub const MAX_SUPPLY: i128 = 1_000_000_000_000_000;

/// Maximum allowance allowed (1M tokens with 7 decimals).
pub const MAX_ALLOWANCE: i128 = 1_000_000 * 10_000_000;

/// Storage keys for the contract
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// The admin address with full privileges
    Admin,
    /// Operator delegation: maps operator address to expiration timestamp
    Operator(Address),
    /// Nonce for replay protection: maps address to current nonce
    Nonce(Address),
    /// Total supply of tokens
    TotalSupply,
    /// Balance: maps address to balance
    Balance(Address),
    /// Allowance: maps (owner, spender) to allowance amount
    Allowance(Address, Address),
}

impl DataKey {
    pub fn encode(&self, env: &Env) -> Bytes {
        let mut key = Bytes::new(env);
        key.append(&Bytes::from_array(env, &NAMESPACE_PREFIX));
        key.append(&self.clone().to_xdr(env));
        key
    }
}

/// Get the admin address from storage
pub fn get_admin(env: &Env) -> Option<Address> {
    let key = DataKey::Admin.encode(env);
    env.storage().persistent().get(&key)
}

/// Set the admin address in storage
pub fn set_admin(env: &Env, admin: &Address) {
    let key = DataKey::Admin.encode(env);
    env.storage().persistent().set(&key, admin);
}

/// Get operator delegation expiration timestamp
pub fn get_operator_expiration(env: &Env, operator: &Address) -> Option<u64> {
    let key = DataKey::Operator(operator.clone()).encode(env);
    env.storage().persistent().get(&key)
}

/// Set operator delegation with expiration
pub fn set_operator(env: &Env, operator: &Address, expiration: u64) {
    let key = DataKey::Operator(operator.clone()).encode(env);
    env.storage().persistent().set(&key, &expiration);

    // Extend TTL for the operator entry
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_OPERATOR_DELEGATION, TTL_OPERATOR_DELEGATION);
}

/// Remove operator delegation
pub fn remove_operator(env: &Env, operator: &Address) {
    let key = DataKey::Operator(operator.clone()).encode(env);
    env.storage().persistent().remove(&key);
}

/// Get nonce for an address
pub fn get_nonce(env: &Env, address: &Address) -> u64 {
    let key = DataKey::Nonce(address.clone()).encode(env);
    env.storage().persistent().get(&key).unwrap_or(0)
}

/// Increment and return the new nonce for an address
pub fn increment_nonce(env: &Env, address: &Address) -> u64 {
    let current = get_nonce(env, address);
    let new_nonce = current + 1;
    let key = DataKey::Nonce(address.clone()).encode(env);
    env.storage().persistent().set(&key, &new_nonce);
    new_nonce
}

/// Get total supply
pub fn get_total_supply(env: &Env) -> i128 {
    let key = DataKey::TotalSupply.encode(env);
    env.storage().persistent().get(&key).unwrap_or(0)
}

/// Set total supply
pub fn set_total_supply(env: &Env, supply: i128) {
    let key = DataKey::TotalSupply.encode(env);
    env.storage().persistent().set(&key, &supply);
}

/// Get balance for an address
pub fn get_balance(env: &Env, address: &Address) -> i128 {
    let key = DataKey::Balance(address.clone()).encode(env);
    env.storage().persistent().get(&key).unwrap_or(0)
}

/// Set balance for an address
pub fn set_balance(env: &Env, address: &Address, balance: i128) {
    let key = DataKey::Balance(address.clone()).encode(env);
    env.storage().persistent().set(&key, &balance);
}

/// Get allowance for a spender from an owner
pub fn get_allowance(env: &Env, owner: &Address, spender: &Address) -> i128 {
    let key = DataKey::Allowance(owner.clone(), spender.clone()).encode(env);
    env.storage().persistent().get(&key).unwrap_or(0)
}

/// Set allowance for a spender from an owner
pub fn set_allowance(env: &Env, owner: &Address, spender: &Address, amount: i128) {
    let key = DataKey::Allowance(owner.clone(), spender.clone()).encode(env);
    env.storage().persistent().set(&key, &amount);
}

/// Migrate all storage entries from legacy (non-prefixed) keys to new namespaced keys.
/// Idempotent — safe to call multiple times.
pub fn migrate_namespace(env: &Env, addresses: &SdkVec<Address>) {
    // Migrate singleton keys
    let legacy_admin: Option<Address> = env.storage().persistent().get(&DataKey::Admin);
    if let Some(admin) = legacy_admin {
        let new_key = DataKey::Admin.encode(env);
        env.storage().persistent().set(&new_key, &admin);
        env.storage().persistent().remove(&DataKey::Admin);
    }

    let legacy_supply: i128 = env.storage().persistent().get(&DataKey::TotalSupply).unwrap_or(0);
    if legacy_supply != 0 {
        let new_key = DataKey::TotalSupply.encode(env);
        env.storage().persistent().set(&new_key, &legacy_supply);
        env.storage().persistent().remove(&DataKey::TotalSupply);
    }

    // Migrate per-address keys
    for addr in addresses.iter() {
        let legacy_bal: i128 = env.storage().persistent().get(&DataKey::Balance(addr.clone())).unwrap_or(0);
        if legacy_bal != 0 {
            let new_key = DataKey::Balance(addr.clone()).encode(env);
            env.storage().persistent().set(&new_key, &legacy_bal);
            env.storage().persistent().remove(&DataKey::Balance(addr.clone()));
        }

        let legacy_nonce: u64 = env.storage().persistent().get(&DataKey::Nonce(addr.clone())).unwrap_or(0);
        if legacy_nonce != 0 {
            let new_key = DataKey::Nonce(addr.clone()).encode(env);
            env.storage().persistent().set(&new_key, &legacy_nonce);
            env.storage().persistent().remove(&DataKey::Nonce(addr.clone()));
        }

        let legacy_op: Option<u64> = env.storage().persistent().get(&DataKey::Operator(addr.clone()));
        if let Some(exp) = legacy_op {
            let new_key = DataKey::Operator(addr.clone()).encode(env);
            env.storage().persistent().set(&new_key, &exp);
            env.storage().persistent().remove(&DataKey::Operator(addr.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use alloc::vec::Vec;

    #[test]
    fn test_key_uniqueness() {
        let env = Env::default();
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        let mut keys = Vec::new();
        keys.push(DataKey::Admin.encode(&env));
        keys.push(DataKey::TotalSupply.encode(&env));
        keys.push(DataKey::Operator(addr1.clone()).encode(&env));
        keys.push(DataKey::Operator(addr2.clone()).encode(&env));
        keys.push(DataKey::Nonce(addr1.clone()).encode(&env));
        keys.push(DataKey::Nonce(addr2.clone()).encode(&env));
        keys.push(DataKey::Balance(addr1.clone()).encode(&env));
        keys.push(DataKey::Balance(addr2.clone()).encode(&env));
        keys.push(DataKey::Allowance(addr1.clone(), addr2.clone()).encode(&env));
        keys.push(DataKey::Allowance(addr2.clone(), addr1.clone()).encode(&env));

        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(keys[i], keys[j], "Key collision: {:?} vs {:?}", keys[i], keys[j]);
            }
        }

        // Verify all keys start with namespace prefix
        for k in &keys {
            let slice: soroban_sdk::Bytes = k.clone();
            assert_eq!(slice.get(0).unwrap(), NAMESPACE_PREFIX[0], "Key missing namespace prefix");
            assert_eq!(slice.get(1).unwrap(), NAMESPACE_PREFIX[1], "Key missing namespace prefix");
            assert_eq!(slice.get(2).unwrap(), NAMESPACE_PREFIX[2], "Key missing namespace prefix");
            assert_eq!(slice.get(3).unwrap(), NAMESPACE_PREFIX[3], "Key missing namespace prefix");
        }
    }
}
