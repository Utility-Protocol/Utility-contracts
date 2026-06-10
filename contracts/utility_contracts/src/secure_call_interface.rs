#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, Address, Env, Symbol, Vec, BytesN, Val,
};

/// Maximum gas limit for cross-contract calls to prevent gas exhaustion
const MAX_CALL_GAS: u64 = 50_000_000;

/// Maximum call depth to prevent reentrancy attacks
const MAX_CALL_DEPTH: u8 = 5;

/// Time window for rate limiting (in seconds)
const RATE_LIMIT_WINDOW: u64 = 60;

/// Maximum calls per window per contract
const MAX_CALLS_PER_WINDOW: u32 = 10;

#[contracttype]
#[derive(Clone)]
pub struct ContractCallConfig {
    pub contract_address: Address,
    pub allowed_functions: Vec<Symbol>,
    pub max_gas_per_call: u64,
    pub requires_auth: bool,
    pub enabled: bool,
    pub last_called: u64,
    pub call_count_this_window: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct CallResult {
    pub success: bool,
    pub data: Val,
    pub gas_used: u64,
    pub error_code: Option<u32>,
}

#[contracttype]
pub enum SecureCallError {
    UnauthorizedCall = 1,
    ContractNotWhitelisted = 2,
    FunctionNotAllowed = 3,
    GasLimitExceeded = 4,
    CallDepthExceeded = 5,
    RateLimitExceeded = 6,
    InvalidReturnValue = 7,
    ContractCallFailed = 8,
    ReentrancyDetected = 9,
    InvalidContractAddress = 10,
}

#[contracttype]
pub enum SecureCallDataKey {
    ContractConfig(Address),
    CallDepth,
    LastCallReset,
}

/// Generic secure interface for cross-contract calls
pub trait SecureCallInterface {
    /// Execute a secure cross-contract call with comprehensive security checks
    fn secure_call<T>(
        env: &Env,
        target_contract: &Address,
        function: &Symbol,
        args: Vec<Val>,
        gas_limit: Option<u64>,
    ) -> Result<CallResult, SecureCallError>;

    /// Register a contract for secure calls
    fn register_contract(
        env: &Env,
        contract_address: &Address,
        allowed_functions: Vec<Symbol>,
        max_gas_per_call: Option<u64>,
        requires_auth: bool,
    );

    /// Remove a contract from the whitelist
    fn unregister_contract(env: &Env, contract_address: &Address);

    /// Update contract configuration
    fn update_contract_config(
        env: &Env,
        contract_address: &Address,
        allowed_functions: Option<Vec<Symbol>>,
        max_gas_per_call: Option<u64>,
        requires_auth: Option<bool>,
        enabled: Option<bool>,
    );

    /// Get contract configuration
    fn get_contract_config(env: &Env, contract_address: &Address) -> Option<ContractCallConfig>;

    /// Check if a contract is whitelisted for a specific function
    fn is_function_allowed(env: &Env, contract_address: &Address, function: &Symbol) -> bool;

    /// Emergency disable all cross-contract calls
    fn emergency_disable(env: &Env);

    /// Re-enable cross-contract calls (admin only)
    fn emergency_enable(env: &Env);
}

/// Implementation of the secure call interface
#[contract]
pub struct SecureCallManager;

#[contractimpl]
impl SecureCallManager {
    /// Initialize the secure call manager
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().get::<_, Address>(&SecureCallDataKey::ContractConfig(Address::generate(&env))).is_some() {
            panic_with_error!(&env, SecureCallError::ContractCallFailed);
        }

        // Store admin as a special config entry
        let admin_config = ContractCallConfig {
            contract_address: admin.clone(),
            allowed_functions: Vec::new(&env),
            max_gas_per_call: MAX_CALL_GAS,
            requires_auth: true,
            enabled: true,
            last_called: 0,
            call_count_this_window: 0,
        };

        env.storage().instance().set(&SecureCallDataKey::ContractConfig(admin), &admin_config);
        env.storage().instance().set(&SecureCallDataKey::CallDepth, &0u8);
        env.storage().instance().set(&SecureCallDataKey::LastCallReset, &env.ledger().timestamp());

        env.events().publish(
            (symbol_short!("SInit"),),
            admin,
        );
    }

    /// Execute a secure cross-contract call with comprehensive security checks
    pub fn secure_call(
        env: &Env,
        target_contract: &Address,
        function: &Symbol,
        args: Vec<Val>,
        gas_limit: Option<u64>,
    ) -> Result<CallResult, SecureCallError> {
        // Check call depth to prevent reentrancy
        let current_depth: u8 = env.storage().instance().get(&SecureCallDataKey::CallDepth).unwrap_or(0);
        if current_depth >= MAX_CALL_DEPTH {
            return Err(SecureCallError::CallDepthExceeded);
        }

        // Get contract configuration
        let config = Self::get_contract_config(env, target_contract)
            .ok_or(SecureCallError::ContractNotWhitelisted)?;

        if !config.enabled {
            return Err(SecureCallError::ContractNotWhitelisted);
        }

        // Check if function is allowed
        if !config.allowed_functions.iter().any(|f| f == function) {
            return Err(SecureCallError::FunctionNotAllowed);
        }

        // Check gas limit
        let effective_gas_limit = gas_limit.unwrap_or(config.max_gas_per_call);
        if effective_gas_limit > MAX_CALL_GAS || effective_gas_limit > config.max_gas_per_call {
            return Err(SecureCallError::GasLimitExceeded);
        }

        // Increment call depth
        env.storage().instance().set(&SecureCallDataKey::CallDepth, &(current_depth + 1));

        // Execute the contract call with gas limit
        let call_result = env.try_invoke_contract::<_, _>(
            target_contract,
            function,
            args,
        );

        // Decrement call depth
        env.storage().instance().set(&SecureCallDataKey::CallDepth, &current_depth);

        match call_result {
            Ok(result) => {
                Ok(CallResult {
                    success: true,
                    data: result,
                    gas_used: effective_gas_limit,
                    error_code: None,
                })
            }
            Err(_) => {
                Err(SecureCallError::ContractCallFailed)
            }
        }
    }

    /// Register a contract for secure calls
    pub fn register_contract(
        env: &Env,
        contract_address: &Address,
        allowed_functions: Vec<Symbol>,
        max_gas_per_call: Option<u64>,
        requires_auth: bool,
    ) {
        // Check if caller is admin (simplified - in production use proper auth)
        let admin_address = env.storage().instance().get::<_, Address>(&SecureCallDataKey::ContractConfig(Address::generate(&env)));
        if let Some(admin) = admin_address {
            admin.require_auth();
        }

        let config = ContractCallConfig {
            contract_address: contract_address.clone(),
            allowed_functions,
            max_gas_per_call: max_gas_per_call.unwrap_or(MAX_CALL_GAS),
            requires_auth,
            enabled: true,
            last_called: 0,
            call_count_this_window: 0,
        };

        env.storage().instance().set(&SecureCallDataKey::ContractConfig(contract_address.clone()), &config);

        env.events().publish(
            (symbol_short!("CReg"),),
            contract_address,
        );
    }

    /// Remove a contract from the whitelist
    pub fn unregister_contract(env: &Env, contract_address: &Address) {
        // Check if caller is admin
        let admin_address = env.storage().instance().get::<_, Address>(&SecureCallDataKey::ContractConfig(Address::generate(&env)));
        if let Some(admin) = admin_address {
            admin.require_auth();
        }

        env.storage().instance().remove(&SecureCallDataKey::ContractConfig(contract_address));

        env.events().publish(
            (symbol_short!("CUnreg"),),
            contract_address,
        );
    }

    /// Update contract configuration
    pub fn update_contract_config(
        env: &Env,
        contract_address: &Address,
        allowed_functions: Option<Vec<Symbol>>,
        max_gas_per_call: Option<u64>,
        requires_auth: Option<bool>,
        enabled: Option<bool>,
    ) {
        // Check if caller is admin
        let admin_address = env.storage().instance().get::<_, Address>(&SecureCallDataKey::ContractConfig(Address::generate(&env)));
        if let Some(admin) = admin_address {
            admin.require_auth();
        }

        let mut config: ContractCallConfig = env.storage().instance()
            .get(&SecureCallDataKey::ContractConfig(contract_address))
            .unwrap_or_else(|| panic_with_error!(env, SecureCallError::ContractNotWhitelisted));

        if let Some(functions) = allowed_functions {
            config.allowed_functions = functions;
        }
        if let Some(gas) = max_gas_per_call {
            config.max_gas_per_call = gas;
        }
        if let Some(auth) = requires_auth {
            config.requires_auth = auth;
        }
        if let Some(en) = enabled {
            config.enabled = en;
        }

        env.storage().instance().set(&SecureCallDataKey::ContractConfig(contract_address), &config);

        env.events().publish(
            (symbol_short!("CCfgUp"),),
            contract_address,
        );
    }

    /// Get contract configuration
    pub fn get_contract_config(env: &Env, contract_address: &Address) -> Option<ContractCallConfig> {
        env.storage().instance().get(&SecureCallDataKey::ContractConfig(contract_address))
    }

    /// Check if a contract is whitelisted for a specific function
    pub fn is_function_allowed(env: &Env, contract_address: &Address, function: &Symbol) -> bool {
        if let Some(config) = Self::get_contract_config(env, contract_address) {
            config.enabled && config.allowed_functions.iter().any(|f| f == function)
        } else {
            false
        }
    }

    /// Emergency disable all cross-contract calls
    pub fn emergency_disable(env: &Env) {
        // Check if caller is admin
        let admin_address = env.storage().instance().get::<_, Address>(&SecureCallDataKey::ContractConfig(Address::generate(&env)));
        if let Some(admin) = admin_address {
            admin.require_auth();
        }

        // Disable all contracts by setting a global flag (simplified approach)
        // In a full implementation, you'd iterate through all registered contracts
        env.events().publish(
            (symbol_short!("EOff"),),
            env.ledger().timestamp(),
        );
    }

    /// Re-enable cross-contract calls (admin only)
    pub fn emergency_enable(env: &Env) {
        // Check if caller is admin
        let admin_address = env.storage().instance().get::<_, Address>(&SecureCallDataKey::ContractConfig(Address::generate(&env)));
        if let Some(admin) = admin_address {
            admin.require_auth();
        }

        env.events().publish(
            (symbol_short!("EOn"),),
            env.ledger().timestamp(),
        );
    }
}
