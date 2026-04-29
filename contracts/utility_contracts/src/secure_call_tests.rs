#![cfg(test)]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short, Address, Env, Symbol, Vec, Status,
};
use crate::secure_call_interface::{
    SecureCallManager, SecureCallError, ContractCallConfig, CallResult,
    SecureCallDataKey, MAX_CALL_GAS, MAX_CALL_DEPTH,
};

// Mock target contract for testing
#[contract]
pub struct MockTargetContract;

#[contractimpl]
impl MockTargetContract {
    pub fn test_function(env: Env, input: u32) -> u32 {
        input * 2
    }

    pub fn test_function_with_args(env: Env, a: u32, b: u32) -> u32 {
        a + b
    }

    pub fn failing_function(env: Env) -> Result<(), u32> {
        Err(42)
    }

    pub fn gas_heavy_function(env: Env) -> u64 {
        // Simulate gas-heavy operation
        let mut result = 0u64;
        for i in 0..1000 {
            result = result.wrapping_add(i as u64);
        }
        result
    }

    pub fn recursive_call(env: Env, depth: u32) -> u32 {
        if depth > 0 {
            // This would normally cause reentrancy issues
            depth
        } else {
            0
        }
    }
}

#[test]
fn test_secure_call_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Initialize secure call manager
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Verify admin is set
    let admin_config = SecureCallManager::get_contract_config(&env, &admin);
    assert!(admin_config.is_some());
    assert!(admin_config.unwrap().requires_auth);
}

#[test]
fn test_contract_registration() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let target_contract = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Register a contract
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "test_function"));
    allowed_functions.push_back(Symbol::new(&env, "test_function_with_args"));
    
    SecureCallManager::register_contract(
        &env,
        &target_contract,
        allowed_functions.clone(),
        Some(20_000_000),
        true,
    );
    
    // Verify registration
    let config = SecureCallManager::get_contract_config(&env, &target_contract)
        .expect("Contract should be registered");
    
    assert_eq!(config.contract_address, target_contract);
    assert_eq!(config.allowed_functions.len(), 2);
    assert_eq!(config.max_gas_per_call, 20_000_000);
    assert!(config.requires_auth);
    assert!(config.enabled);
}

#[test]
fn test_function_allowed_check() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let target_contract = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Register contract with specific functions
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "test_function"));
    
    SecureCallManager::register_contract(
        &env,
        &target_contract,
        allowed_functions,
        None,
        false,
    );
    
    // Test function allowance
    assert!(SecureCallManager::is_function_allowed(
        &env,
        &target_contract,
        &Symbol::new(&env, "test_function")
    ));
    
    assert!(!SecureCallManager::is_function_allowed(
        &env,
        &target_contract,
        &Symbol::new(&env, "unauthorized_function")
    ));
}

#[test]
fn test_contract_unregistration() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let target_contract = Address::generate(&env);
    
    // Initialize and register
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "test_function"));
    
    SecureCallManager::register_contract(
        &env,
        &target_contract,
        allowed_functions,
        None,
        false,
    );
    
    // Verify registration
    assert!(SecureCallManager::get_contract_config(&env, &target_contract).is_some());
    
    // Unregister
    SecureCallManager::unregister_contract(&env, &target_contract);
    
    // Verify removal
    assert!(SecureCallManager::get_contract_config(&env, &target_contract).is_none());
}

#[test]
fn test_contract_config_update() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let target_contract = Address::generate(&env);
    
    // Initialize and register
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "test_function"));
    
    SecureCallManager::register_contract(
        &env,
        &target_contract,
        allowed_functions.clone(),
        Some(10_000_000),
        true,
    );
    
    // Update configuration
    let mut new_functions = Vec::new(&env);
    new_functions.push_back(Symbol::new(&env, "test_function"));
    new_functions.push_back(Symbol::new(&env, "test_function_with_args"));
    
    SecureCallManager::update_contract_config(
        &env,
        &target_contract,
        Some(new_functions),
        Some(25_000_000),
        Some(false),
        Some(true),
    );
    
    // Verify updates
    let config = SecureCallManager::get_contract_config(&env, &target_contract)
        .expect("Contract should still be registered");
    
    assert_eq!(config.allowed_functions.len(), 2);
    assert_eq!(config.max_gas_per_call, 25_000_000);
    assert!(!config.requires_auth);
    assert!(config.enabled);
}

#[test]
fn test_secure_call_success() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Register mock contract
    let contract_address = env.register_contract(None, MockTargetContract);
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "test_function"));
    
    SecureCallManager::register_contract(
        &env,
        &contract_address,
        allowed_functions,
        None,
        false,
    );
    
    // Make secure call
    let mut args = Vec::new(&env);
    args.push_back(21u32.into());
    
    let result = SecureCallManager::secure_call::<u32>(
        &env,
        &contract_address,
        &Symbol::new(&env, "test_function"),
        args,
        Some(10_000_000),
    );
    
    assert!(result.is_ok());
    let call_result = result.unwrap();
    assert!(call_result.success);
    assert_eq!(call_result.data, 42); // 21 * 2
}

#[test]
fn test_secure_call_with_multiple_args() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Register mock contract
    let contract_address = env.register_contract(None, MockTargetContract);
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "test_function_with_args"));
    
    SecureCallManager::register_contract(
        &env,
        &contract_address,
        allowed_functions,
        None,
        false,
    );
    
    // Make secure call with multiple arguments
    let mut args = Vec::new(&env);
    args.push_back(15u32.into());
    args.push_back(27u32.into());
    
    let result = SecureCallManager::secure_call::<u32>(
        &env,
        &contract_address,
        &Symbol::new(&env, "test_function_with_args"),
        args,
        Some(10_000_000),
    );
    
    assert!(result.is_ok());
    let call_result = result.unwrap();
    assert!(call_result.success);
    assert_eq!(call_result.data, 42); // 15 + 27
}

#[test]
fn test_secure_call_unauthorized_contract() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Try to call unregistered contract
    let contract_address = Address::generate(&env);
    let mut args = Vec::new(&env);
    args.push_back(42u32.into());
    
    let result = SecureCallManager::secure_call::<u32>(
        &env,
        &contract_address,
        &Symbol::new(&env, "test_function"),
        args,
        Some(10_000_000),
    );
    
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), SecureCallError::ContractNotWhitelisted);
}

#[test]
fn test_secure_call_unauthorized_function() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Register contract but not the specific function
    let contract_address = env.register_contract(None, MockTargetContract);
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "other_function"));
    
    SecureCallManager::register_contract(
        &env,
        &contract_address,
        allowed_functions,
        None,
        false,
    );
    
    // Try to call unauthorized function
    let mut args = Vec::new(&env);
    args.push_back(42u32.into());
    
    let result = SecureCallManager::secure_call::<u32>(
        &env,
        &contract_address,
        &Symbol::new(&env, "test_function"),
        args,
        Some(10_000_000),
    );
    
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), SecureCallError::FunctionNotAllowed);
}

#[test]
fn test_secure_call_gas_limit_exceeded() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Register contract with low gas limit
    let contract_address = env.register_contract(None, MockTargetContract);
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "test_function"));
    
    SecureCallManager::register_contract(
        &env,
        &contract_address,
        allowed_functions,
        Some(1_000_000), // Very low gas limit
        false,
    );
    
    // Try to call with higher gas limit than allowed
    let mut args = Vec::new(&env);
    args.push_back(42u32.into());
    
    let result = SecureCallManager::secure_call::<u32>(
        &env,
        &contract_address,
        &Symbol::new(&env, "test_function"),
        args,
        Some(50_000_000), // Higher than contract's limit
    );
    
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), SecureCallError::GasLimitExceeded);
}

#[test]
fn test_secure_call_disabled_contract() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Register contract but disable it
    let contract_address = env.register_contract(None, MockTargetContract);
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "test_function"));
    
    SecureCallManager::register_contract(
        &env,
        &contract_address,
        allowed_functions,
        None,
        false,
    );
    
    // Disable the contract
    SecureCallManager::update_contract_config(
        &env,
        &contract_address,
        None,
        None,
        None,
        Some(false), // Disable
    );
    
    // Try to call disabled contract
    let mut args = Vec::new(&env);
    args.push_back(42u32.into());
    
    let result = SecureCallManager::secure_call::<u32>(
        &env,
        &contract_address,
        &Symbol::new(&env, "test_function"),
        args,
        Some(10_000_000),
    );
    
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), SecureCallError::ContractNotWhitelisted);
}

#[test]
fn test_emergency_disable_enable() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Register a contract
    let contract_address = env.register_contract(None, MockTargetContract);
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "test_function"));
    
    SecureCallManager::register_contract(
        &env,
        &contract_address,
        allowed_functions,
        None,
        false,
    );
    
    // Emergency disable
    SecureCallManager::emergency_disable(&env);
    
    // Emergency enable
    SecureCallManager::emergency_enable(&env);
    
    // Verify contract is still registered
    let config = SecureCallManager::get_contract_config(&env, &contract_address);
    assert!(config.is_some());
}

#[test]
fn test_call_depth_protection() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Manually set call depth to maximum to test protection
    env.storage().instance().set(&SecureCallDataKey::CallDepth, &MAX_CALL_DEPTH);
    
    // Register contract
    let contract_address = env.register_contract(None, MockTargetContract);
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "test_function"));
    
    SecureCallManager::register_contract(
        &env,
        &contract_address,
        allowed_functions,
        None,
        false,
    );
    
    // Try to call - should fail due to depth limit
    let mut args = Vec::new(&env);
    args.push_back(42u32.into());
    
    let result = SecureCallManager::secure_call::<u32>(
        &env,
        &contract_address,
        &Symbol::new(&env, "test_function"),
        args,
        Some(10_000_000),
    );
    
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), SecureCallError::CallDepthExceeded);
}

#[test]
fn test_secure_call_error_handling() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Initialize
    SecureCallManager::initialize(env.clone(), admin.clone());
    
    // Register mock contract
    let contract_address = env.register_contract(None, MockTargetContract);
    let mut allowed_functions = Vec::new(&env);
    allowed_functions.push_back(Symbol::new(&env, "failing_function"));
    
    SecureCallManager::register_contract(
        &env,
        &contract_address,
        allowed_functions,
        None,
        false,
    );
    
    // Call failing function
    let result = SecureCallManager::secure_call::<()>(
        &env,
        &contract_address,
        &Symbol::new(&env, "failing_function"),
        Vec::new(&env),
        Some(10_000_000),
    );
    
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), SecureCallError::ContractCallFailed);
}
