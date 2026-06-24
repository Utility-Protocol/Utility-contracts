use soroban_sdk::{Address, Env};

use crate::storage::{get_operator_expiration, remove_operator, set_operator, increment_nonce};
use crate::auth::authorize_admin;

/// Authorize an operator to perform mint/burn operations until the expiration timestamp
/// 
/// # Arguments
/// * `env` - The contract environment
/// * `operator` - The address to authorize as an operator
/// * `expiration` - Unix timestamp when the delegation expires
/// 
/// # Panics
/// * If caller is not the admin
/// * If expiration is in the past or zero
pub fn authorize_operator(env: &Env, operator: Address, expiration: u64) {
    // Only admin can authorize operators
    authorize_admin(env);
    
    let current_time = env.ledger().timestamp();
    
    // Validate expiration is in the future
    if expiration <= current_time {
        panic!("Expiration must be in the future");
    }
    
    // Increment nonce for replay protection
    increment_nonce(env, &operator);
    
    // Store the operator with expiration
    set_operator(env, &operator, expiration);
    
    // Emit event
    env.events().publish(
        (soroban_sdk::symbol_short!("operator"), soroban_sdk::symbol_short!("auth")),
        (operator.clone(), expiration),
    );
}

/// Revoke operator authorization
/// 
/// # Arguments
/// * `env` - The contract environment
/// * `operator` - The address to revoke authorization from
/// 
/// # Panics
/// * If caller is not the admin
pub fn revoke_operator(env: &Env, operator: Address) {
    // Only admin can revoke operators
    authorize_admin(env);
    
    // Remove the operator
    remove_operator(env, &operator);
    
    // Increment nonce to invalidate any pending operations
    increment_nonce(env, &operator);
    
    // Emit event
    env.events().publish(
        (soroban_sdk::symbol_short!("operator"), soroban_sdk::symbol_short!("revoke")),
        operator,
    );
}

/// Check if an address is a valid operator (not expired)
/// 
/// # Arguments
/// * `env` - The contract environment
/// * `operator` - The address to check
/// 
/// # Returns
/// * `true` if the operator is authorized and not expired, `false` otherwise
pub fn is_valid_operator(env: &Env, operator: &Address) -> bool {
    if let Some(expiration) = get_operator_expiration(env, operator) {
        let current_time = env.ledger().timestamp();
        expiration > current_time
    } else {
        false
    }
}
