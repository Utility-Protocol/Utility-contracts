use soroban_sdk::{Address, Env};

use crate::storage::{get_admin as storage_get_admin, set_admin as storage_set_admin};

/// Set the admin address
/// 
/// This function can only be called once during initialization, or by the current admin.
/// 
/// # Arguments
/// * `env` - The contract environment
/// * `new_admin` - The address to set as admin
/// 
/// # Panics
/// * If an admin already exists and the caller is not the current admin
pub fn set_admin(env: &Env, new_admin: Address) {
    // Check if admin already exists
    if let Some(current_admin) = storage_get_admin(env) {
        // Admin exists, so current admin must authorize the change
        current_admin.require_auth();
    }
    // If no admin exists, this is initialization and anyone can set it
    // (though in practice, this would be restricted to contract deployment)
    
    storage_set_admin(env, &new_admin);
    
    // Emit event
    env.events().publish(
        (soroban_sdk::symbol_short!("admin"), soroban_sdk::symbol_short!("set")),
        new_admin,
    );
}

/// Get the current admin address
/// 
/// # Arguments
/// * `env` - The contract environment
/// 
/// # Returns
/// * `Some(Address)` if an admin is set
/// * `None` if no admin has been configured
pub fn get_admin(env: &Env) -> Option<Address> {
    storage_get_admin(env)
}

/// Check if an address is the admin
/// 
/// # Arguments
/// * `env` - The contract environment
/// * `address` - The address to check
/// 
/// # Returns
/// * `true` if the address is the current admin
/// * `false` otherwise
#[allow(dead_code)]
pub fn is_admin(env: &Env, address: &Address) -> bool {
    if let Some(admin) = storage_get_admin(env) {
        &admin == address
    } else {
        false
    }
}
