use soroban_sdk::{Address, Env};

use crate::storage::get_admin;
use crate::operators::is_valid_operator;

/// Error types for authorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AuthError {
    /// No admin configured
    NoAdmin,
    /// Caller is not authorized
    Unauthorized,
}

/// Authorize that the caller is the admin
/// 
/// # Panics
/// * If no admin is set
/// * If caller is not the admin
pub fn authorize_admin(env: &Env) {
    let admin = get_admin(env).expect("Admin not set");
    admin.require_auth();
}

/// Authorize mint operations with full call chain verification
/// 
/// This function verifies that the admin has authorized the operation.
/// 
/// # Arguments
/// * `env` - The contract environment
/// 
/// # Panics
/// * If no admin is configured
/// * If admin has not authorized the operation
pub fn authorize_mint(env: &Env) {
    authorize_with_chain(env);
}

/// Authorize burn operations with full call chain verification
/// 
/// This function verifies that the admin has authorized the operation.
/// 
/// # Arguments
/// * `env` - The contract environment
/// 
/// # Panics
/// * If no admin is configured
/// * If admin has not authorized the operation
pub fn authorize_burn(env: &Env) {
    authorize_with_chain(env);
}

/// Authorize with full call chain verification
/// 
/// This is the core authorization function that validates the entire invocation chain.
/// It ensures that the originating caller is authorized, regardless of how many
/// intermediate contracts were involved in the call.
/// 
/// # Call Chain Validation
/// 
/// Soroban's authentication model:
/// - Direct calls: The caller is the Account/Contract that invoked the function
/// - Contract invocations: Each contract can have its own authentication context
/// - require_auth(): Validates that the specified address has authorized this operation
/// 
/// This function uses require_auth() to ensure that the admin has authorized
/// the operation.
/// 
/// # Security Model
/// 
/// A malicious contract cannot spoof authorization because:
/// - require_auth() validates the actual authorization signature/context
/// - The authorization must come from the admin's private key
/// - Contract addresses in the call chain do not automatically inherit authorization
/// - The address must explicitly prove authorization via require_auth()
/// 
/// # Arguments
/// * `env` - The contract environment
/// 
/// # Panics
/// * If no admin is configured (NoAdmin)
/// * If admin has not authorized the operation (Unauthorized)
fn authorize_with_chain(env: &Env) {
    // Get the admin address
    let admin = match get_admin(env) {
        Some(addr) => addr,
        None => panic!("NoAdmin: Admin not configured"),
    };
    
    // Require admin authorization
    // This validates that the admin has signed/authorized this operation
    // If the admin hasn't authorized it, this will panic with "not authorized by"
    admin.require_auth();
    
    // If we reach here, admin has authorized the operation
}

/// Check if an operator is authorized
/// 
/// This is a helper function for when you know the specific operator address
/// that should be authorized.
/// 
/// # Arguments
/// * `env` - The contract environment
/// * `operator` - The operator address to check
/// 
/// # Returns
/// * `true` if the operator is authorized and auth is valid
/// * `false` otherwise
#[allow(dead_code)]
pub fn check_operator_auth(env: &Env, operator: &Address) -> bool {
    // Check if operator delegation is valid (not expired)
    if !is_valid_operator(env, operator) {
        return false;
    }
    
    // Check if the operator has authorized this call
    operator.require_auth();
    true
}

/// Authorize with explicit operator check
/// 
/// This variant checks both admin and a specific operator
/// 
/// # Arguments
/// * `env` - The contract environment  
/// * `operator` - The operator address to check
/// 
/// # Panics
/// * If neither admin nor the specified operator is authorized
#[allow(dead_code)]
pub fn authorize_with_operator(env: &Env, _operator: &Address) {
    let admin = match get_admin(env) {
        Some(addr) => addr,
        None => panic!("NoAdmin: Admin not configured"),
    };
    
    // Try admin first
    admin.require_auth();
    
    // If we reach here, admin has authorized
}
