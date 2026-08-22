#![cfg(test)]

use crate::batch_executor::{BatchOperation, estimate_batch_gas};
use crate::{UtilityContract, UtilityContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, Symbol, Vec, symbol_short};

// A dummy contract to test the batch executor.
#[soroban_sdk::contract]
pub struct DummyContract;

#[soroban_sdk::contractimpl]
impl DummyContract {
    pub fn add(env: Env, a: u32, b: u32) -> u32 {
        a + b
    }
    
    pub fn fail(_env: Env) {
        panic!("Intended failure");
    }
}

#[test]
fn test_batch_execution_success() {
    let env = Env::default();
    
    let utility_id = env.register_contract(None, UtilityContract);
    let utility_client = UtilityContractClient::new(&env, &utility_id);
    
    let dummy_id = env.register_contract(None, DummyContract);
    
    let mut ops = Vec::new(&env);
    
    ops.push_back(BatchOperation {
        contract: dummy_id.clone(),
        function: symbol_short!("add"),
        args: (2u32, 3u32).into_val(&env),
    });
    
    ops.push_back(BatchOperation {
        contract: dummy_id.clone(),
        function: symbol_short!("add"),
        args: (10u32, 20u32).into_val(&env),
    });
    
    let results = utility_client.execute_batch(&ops);
    
    assert_eq!(results.len(), 2);
    let res1: u32 = results.get(0).unwrap().into_val(&env);
    let res2: u32 = results.get(1).unwrap().into_val(&env);
    
    assert_eq!(res1, 5);
    assert_eq!(res2, 30);
}

#[test]
#[should_panic(expected = "Intended failure")]
fn test_batch_execution_partial_failure() {
    let env = Env::default();
    let utility_id = env.register_contract(None, UtilityContract);
    let utility_client = UtilityContractClient::new(&env, &utility_id);
    let dummy_id = env.register_contract(None, DummyContract);
    
    let mut ops = Vec::new(&env);
    
    ops.push_back(BatchOperation {
        contract: dummy_id.clone(),
        function: symbol_short!("add"),
        args: (2u32, 3u32).into_val(&env),
    });
    
    ops.push_back(BatchOperation {
        contract: dummy_id.clone(),
        function: symbol_short!("fail"),
        args: ().into_val(&env),
    });
    
    utility_client.execute_batch(&ops);
}

#[test]
#[should_panic(expected = "Batch exceeds maximum of 20 operations")]
fn test_batch_execution_limit() {
    let env = Env::default();
    let utility_id = env.register_contract(None, UtilityContract);
    let utility_client = UtilityContractClient::new(&env, &utility_id);
    let dummy_id = env.register_contract(None, DummyContract);
    
    let mut ops = Vec::new(&env);
    
    for _ in 0..21 {
        ops.push_back(BatchOperation {
            contract: dummy_id.clone(),
            function: symbol_short!("add"),
            args: (1u32, 1u32).into_val(&env),
        });
    }
    
    utility_client.execute_batch(&ops);
}

#[test]
fn test_gas_estimation() {
    let env = Env::default();
    let dummy_id = Address::generate(&env);
    
    let mut ops = Vec::new(&env);
    for _ in 0..5 {
        ops.push_back(BatchOperation {
            contract: dummy_id.clone(),
            function: symbol_short!("add"),
            args: (1u32, 1u32).into_val(&env),
        });
    }
    
    let utility_id = env.register_contract(None, UtilityContract);
    let utility_client = UtilityContractClient::new(&env, &utility_id);
    
    let estimated_gas = utility_client.estimate_batch_gas(&ops);
    
    // Base 10000 + 5 * 5000 = 35000
    assert_eq!(estimated_gas, 35000);
}
