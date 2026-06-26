#![no_std]
extern crate alloc;

// Resource Token Contract
// 
// This contract implements a secure token with mint/burn operations that include
// full call chain verification to prevent authorization spoofing attacks.
// 
// # Security Model
// 
// The contract authorizes mint/burn operations based on:
// 1. Direct admin authorization, OR
// 2. Delegated operator authorization with expiration
// 
// The authorization check validates the full invocation chain to ensure that
// a malicious intermediate contract cannot spoof authorization.

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

mod admin;
mod allowance;
mod auth;
mod operators;
mod storage;

pub use admin::{get_admin, set_admin};
pub use auth::{authorize_burn, authorize_mint};
pub use operators::{authorize_operator, is_valid_operator, revoke_operator};
use storage as storage_mod;
pub use storage::{
    get_balance, get_total_supply, set_balance, set_total_supply, MAX_CHAIN_DEPTH, MAX_SUPPLY,
    NAMESPACE_PREFIX, TTL_OPERATOR_DELEGATION,
};

#[contract]
pub struct ResourceToken;

#[contractimpl]
impl ResourceToken {
    /// Initialize the contract with an admin
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The address to set as admin
    pub fn initialize(env: Env, admin: Address) {
        // Ensure not already initialized
        if get_admin(&env).is_some() {
            panic!("Already initialized");
        }
        
        admin.require_auth();
        set_admin(&env, admin);
    }

    /// Set a new admin (only callable by current admin)
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `new_admin` - The address to set as the new admin
    pub fn set_admin(env: Env, new_admin: Address) {
        set_admin(&env, new_admin);
    }

    /// Get the current admin address
    /// 
    /// # Returns
    /// * `Some(Address)` if admin is set, `None` otherwise
    pub fn get_admin(env: Env) -> Option<Address> {
        get_admin(&env)
    }

    /// Authorize an operator to perform mint/burn operations
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `operator` - The address to authorize
    /// * `expiration` - Unix timestamp when authorization expires (max 30 days)
    /// 
    /// # Panics
    /// * If caller is not admin
    /// * If expiration is in the past or too far in the future
    pub fn authorize_operator(env: Env, operator: Address, expiration: u64) {
        authorize_operator(&env, operator, expiration);
    }

    /// Revoke operator authorization
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `operator` - The address to revoke
    /// 
    /// # Panics
    /// * If caller is not admin
    pub fn revoke_operator(env: Env, operator: Address) {
        revoke_operator(&env, operator);
    }

    /// Check if an address is a valid (non-expired) operator
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `operator` - The address to check
    /// 
    /// # Returns
    /// * `true` if operator is authorized and not expired
    pub fn is_valid_operator(env: Env, operator: Address) -> bool {
        is_valid_operator(&env, &operator)
    }

    /// Mint tokens to an address
    /// 
    /// This operation requires full authorization via call chain verification.
    /// Only the admin or a valid operator can mint tokens.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `to` - The address to mint tokens to
    /// * `amount` - The amount of tokens to mint
    /// 
    /// # Panics
    /// * If caller is not authorized (not admin or valid operator)
    /// * If amount is negative or zero
    /// * If the mint would push `total_supply` above `MAX_SUPPLY`
    /// * If call chain depth is exceeded
    pub fn mint(env: Env, to: Address, amount: i128) {
        // Authorize with full call chain verification
        authorize_mint(&env);

        // Validate amount
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        // Compute the new total supply first and enforce the supply cap BEFORE
        // any state is written. Each token is backed 1:1 by a real resource
        // deposit, so total_supply must never exceed MAX_SUPPLY. Soroban applies
        // transactions serially and each sees committed state, so this
        // check-then-write is atomic with respect to other transactions — there
        // is no in-ledger concurrency to guard against; the real invariant to
        // enforce is the cap itself.
        let current_supply = get_total_supply(&env);
        let new_supply = current_supply
            .checked_add(amount)
            .expect("Supply overflow");
        if new_supply > MAX_SUPPLY {
            panic!("Max supply exceeded");
        }

        // Update balance (overflow-checked; the workspace build does not enable
        // overflow-checks, so the explicit check is load-bearing).
        let current_balance = get_balance(&env, &to);
        let new_balance = current_balance
            .checked_add(amount)
            .expect("Balance overflow");
        set_balance(&env, &to, new_balance);

        // Commit the new total supply.
        set_total_supply(&env, new_supply);

        // Emit event
        env.events().publish(
            (soroban_sdk::symbol_short!("mint"),),
            (to, amount),
        );
    }

    /// Burn tokens from an address
    /// 
    /// This operation requires full authorization via call chain verification.
    /// Only the admin or a valid operator can burn tokens.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `from` - The address to burn tokens from
    /// * `amount` - The amount of tokens to burn
    /// 
    /// # Panics
    /// * If caller is not authorized (not admin or valid operator)
    /// * If amount is negative or zero
    /// * If insufficient balance
    /// * If call chain depth is exceeded
    pub fn burn(env: Env, from: Address, amount: i128) {
        // Authorize with full call chain verification
        authorize_burn(&env);
        
        // Validate amount
        if amount <= 0 {
            panic!("Amount must be positive");
        }
        
        // Update balance (overflow-checked subtraction; the workspace build does
        // not enable overflow-checks, so use checked_sub rather than `-`).
        let current_balance = get_balance(&env, &from);
        if current_balance < amount {
            panic!("Insufficient balance");
        }
        let new_balance = current_balance
            .checked_sub(amount)
            .expect("Balance underflow");
        set_balance(&env, &from, new_balance);

        // Update total supply
        let current_supply = get_total_supply(&env);
        let new_supply = current_supply
            .checked_sub(amount)
            .expect("Supply underflow");
        set_total_supply(&env, new_supply);
        
        // Emit event
        env.events().publish(
            (soroban_sdk::symbol_short!("burn"),),
            (from, amount),
        );
    }

    /// Get the balance of an address
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The address to query
    /// 
    /// # Returns
    /// * The token balance of the address
    pub fn balance(env: Env, address: Address) -> i128 {
        get_balance(&env, &address)
    }

    /// Get the total supply of tokens
    /// 
    /// # Returns
    /// * The total supply of tokens
    pub fn total_supply(env: Env) -> i128 {
        get_total_supply(&env)
    }

    /// Migrate all storage entries from legacy (non-prefixed) keys to new namespaced keys.
    /// Must be called by the admin after a contract upgrade.
    pub fn migrate_namespace(env: Env, addresses: Vec<Address>) {
        storage_mod::migrate_namespace(&env, &addresses);
    }

    /// Set allowance for a spender
    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
        allowance::approve(env, owner, spender, amount);
    }

    /// Increase allowance for a spender
    pub fn increase_allowance(env: Env, owner: Address, spender: Address, delta: i128) {
        allowance::increase_allowance(env, owner, spender, delta);
    }

    /// Decrease allowance for a spender
    pub fn decrease_allowance(env: Env, owner: Address, spender: Address, delta: i128) {
        allowance::decrease_allowance(env, owner, spender, delta);
    }

    /// Get allowance for a spender
    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        allowance::get_allowance(env, owner, spender)
    }

    /// Transfer tokens using an allowance
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        allowance::transfer_from(env, spender, from, to, amount);
    }
}

#[cfg(test)]
mod test;
