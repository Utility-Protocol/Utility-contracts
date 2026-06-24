use soroban_sdk::{contracttype, Address, Env};

/// Maximum TTL for operator delegation (30 days in seconds)
pub const TTL_OPERATOR_DELEGATION: u32 = 30 * 86400;

/// Maximum call chain depth allowed
pub const MAX_CHAIN_DEPTH: u32 = 5;

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
}

/// Get the admin address from storage
pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&DataKey::Admin)
}

/// Set the admin address in storage
pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().persistent().set(&DataKey::Admin, admin);
}

/// Get operator delegation expiration timestamp
pub fn get_operator_expiration(env: &Env, operator: &Address) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::Operator(operator.clone()))
}

/// Set operator delegation with expiration
pub fn set_operator(env: &Env, operator: &Address, expiration: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::Operator(operator.clone()), &expiration);
    
    // Extend TTL for the operator entry
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::Operator(operator.clone()), TTL_OPERATOR_DELEGATION, TTL_OPERATOR_DELEGATION);
}

/// Remove operator delegation
pub fn remove_operator(env: &Env, operator: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::Operator(operator.clone()));
}

/// Get nonce for an address
pub fn get_nonce(env: &Env, address: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::Nonce(address.clone()))
        .unwrap_or(0)
}

/// Increment and return the new nonce for an address
pub fn increment_nonce(env: &Env, address: &Address) -> u64 {
    let current = get_nonce(env, address);
    let new_nonce = current + 1;
    env.storage()
        .persistent()
        .set(&DataKey::Nonce(address.clone()), &new_nonce);
    new_nonce
}

/// Get total supply
pub fn get_total_supply(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::TotalSupply)
        .unwrap_or(0)
}

/// Set total supply
pub fn set_total_supply(env: &Env, supply: i128) {
    env.storage().persistent().set(&DataKey::TotalSupply, &supply);
}

/// Get balance for an address
pub fn get_balance(env: &Env, address: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(address.clone()))
        .unwrap_or(0)
}

/// Set balance for an address
pub fn set_balance(env: &Env, address: &Address, balance: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Balance(address.clone()), &balance);
}
