#![no_std]

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Bytes, Env,
};

pub const NAMESPACE_PREFIX: [u8; 4] = [0x43, 0x4f, 0x4d, 0x4d]; // "COMM"

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    pub price: i128,       // Price in smallest units (e.g., cents for USD)
    pub decimals: u32,     // Number of decimal places
    pub last_updated: u64, // Timestamp of last update
}

#[contracttype]
#[derive(Copy, Clone)]
pub enum DataKey {
    Price,
    Admin,
    Updater,
}

impl DataKey {
    pub fn encode(&self, env: &Env) -> Bytes {
        let mut key = Bytes::new(env);
        key.append(&Bytes::from_array(env, &NAMESPACE_PREFIX));
        key.append(&self.clone().to_xdr(env));
        key
    }
}

#[contracterror]
#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    NotAuthorized = 1,
    InvalidPrice = 2,
    StalePrice = 3,
    NotInitialized = 4,
}

const MAX_PRICE_AGE_SECONDS: u64 = 300; // 5 minutes

/// Migrate storage entries from legacy (non-prefixed) keys to new namespaced keys.
/// Idempotent — safe to call multiple times.
pub fn migrate_namespace(env: &Env) {
    let legacy_admin: Option<Address> = env.storage().instance().get(&DataKey::Admin);
    if let Some(admin) = legacy_admin {
        let new_key = DataKey::Admin.encode(env);
        env.storage().instance().set(&new_key, &admin);
        env.storage().instance().remove(&DataKey::Admin);
    }

    let legacy_updater: Option<Address> = env.storage().instance().get(&DataKey::Updater);
    if let Some(updater) = legacy_updater {
        let new_key = DataKey::Updater.encode(env);
        env.storage().instance().set(&new_key, &updater);
        env.storage().instance().remove(&DataKey::Updater);
    }

    let legacy_price: Option<PriceData> = env.storage().instance().get(&DataKey::Price);
    if let Some(price) = legacy_price {
        let new_key = DataKey::Price.encode(env);
        env.storage().instance().set(&new_key, &price);
        env.storage().instance().remove(&DataKey::Price);
    }
}

#[contract]
pub struct PriceOracle;

#[contractimpl]
impl PriceOracle {
    /// Initialize the oracle with admin and updater addresses
    pub fn initialize(
        env: Env,
        admin: Address,
        updater: Address,
        initial_price: i128,
        decimals: u32,
    ) {
        let admin_key = DataKey::Admin.encode(&env);
        if env
            .storage()
            .instance()
            .get::<Bytes, Address>(&admin_key)
            .is_some()
        {
            panic!("already initialized");
        }

        if initial_price <= 0 {
            panic_with_error!(env, ContractError::InvalidPrice);
        }

        let admin_key = DataKey::Admin.encode(&env);
        let updater_key = DataKey::Updater.encode(&env);
        let price_key = DataKey::Price.encode(&env);

        env.storage().instance().set(&admin_key, &admin);
        env.storage().instance().set(&updater_key, &updater);

        let price_data = PriceData {
            price: initial_price,
            decimals,
            last_updated: env.ledger().timestamp(),
        };
        env.storage().instance().set(&price_key, &price_data);
    }

    /// Update the price (only callable by updater)
    pub fn update_price(env: Env, new_price: i128) {
        let updater_key = DataKey::Updater.encode(&env);
        let updater = env
            .storage()
            .instance()
            .get::<Bytes, Address>(&updater_key)
            .unwrap_or_else(|| panic_with_error!(env, ContractError::NotInitialized));

        updater.require_auth();

        if new_price <= 0 {
            panic_with_error!(env, ContractError::InvalidPrice);
        }

        let price_data = PriceData {
            price: new_price,
            decimals: Self::get_decimals(env.clone()),
            last_updated: env.ledger().timestamp(),
        };
        let price_key = DataKey::Price.encode(&env);
        env.storage().instance().set(&price_key, &price_data);
    }

    /// Get current price data
    pub fn get_price(env: Env) -> PriceData {
        let price_key = DataKey::Price.encode(&env);
        env.storage()
            .instance()
            .get::<Bytes, PriceData>(&price_key)
            .unwrap_or_else(|| panic_with_error!(env, ContractError::NotInitialized))
    }

    /// Get price with staleness check
    pub fn get_fresh_price(env: Env) -> PriceData {
        let price_data = Self::get_price(env.clone());
        let now = env.ledger().timestamp();

        if now.saturating_sub(price_data.last_updated) > MAX_PRICE_AGE_SECONDS {
            panic_with_error!(env, ContractError::StalePrice);
        }

        price_data
    }

    /// Get just the price value
    pub fn get_price_value(env: Env) -> i128 {
        Self::get_price(env).price
    }

    /// Get number of decimals
    pub fn get_decimals(env: Env) -> u32 {
        Self::get_price(env).decimals
    }

    /// Convert XLM amount to USD cents
    pub fn xlm_to_usd_cents(env: Env, xlm_amount: i128) -> i128 {
        let price_data = Self::get_fresh_price(env);

        // price is in cents per XLM, so multiply
        xlm_amount.saturating_mul(price_data.price)
    }

    /// Convert USD cents to XLM amount
    pub fn usd_cents_to_xlm(env: Env, usd_cents: i128) -> i128 {
        let price_data = Self::get_fresh_price(env);

        // price is in cents per XLM, so divide
        usd_cents / price_data.price
    }

    /// Check if price is fresh
    pub fn is_price_fresh(env: Env) -> bool {
        let price_data = Self::get_price(env.clone());
        let now = env.ledger().timestamp();
        now.saturating_sub(price_data.last_updated) <= MAX_PRICE_AGE_SECONDS
    }

    /// Admin functions
    pub fn set_admin(env: Env, new_admin: Address) {
        let admin_key = DataKey::Admin.encode(&env);
        let admin = env
            .storage()
            .instance()
            .get::<Bytes, Address>(&admin_key)
            .unwrap_or_else(|| panic_with_error!(env, ContractError::NotInitialized));

        admin.require_auth();
        let new_key = DataKey::Admin.encode(&env);
        env.storage().instance().set(&new_key, &new_admin);
    }

    pub fn set_updater(env: Env, new_updater: Address) {
        let admin_key = DataKey::Admin.encode(&env);
        let admin = env
            .storage()
            .instance()
            .get::<Bytes, Address>(&admin_key)
            .unwrap_or_else(|| panic_with_error!(env, ContractError::NotInitialized));

        admin.require_auth();
        let updater_key = DataKey::Updater.encode(&env);
        env.storage()
            .instance()
            .set(&updater_key, &new_updater);
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Address {
        let admin_key = DataKey::Admin.encode(&env);
        env.storage()
            .instance()
            .get::<Bytes, Address>(&admin_key)
            .unwrap_or_else(|| panic_with_error!(env, ContractError::NotInitialized))
    }

    /// Get updater address
    pub fn get_updater(env: Env) -> Address {
        let updater_key = DataKey::Updater.encode(&env);
        env.storage()
            .instance()
            .get::<Bytes, Address>(&updater_key)
            .unwrap_or_else(|| panic_with_error!(env, ContractError::NotInitialized))
    }

    /// Migrate all storage entries from legacy (non-prefixed) keys to new namespaced keys.
    /// Must be called by admin after a contract upgrade.
    pub fn migrate_namespace(env: Env) {
        migrate_namespace(&env);
    }
}

#[cfg(test)]
mod test;
