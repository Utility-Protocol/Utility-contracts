#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, Vec};

fn create_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn admin(env: &Env) -> Address {
    Address::generate(env)
}

fn alice(env: &Env) -> Address {
    Address::generate(env)
}

fn bob(env: &Env) -> Address {
    Address::generate(env)
}

fn charlie(env: &Env) -> Address {
    Address::generate(env)
}

fn deploy<'a>(
    env: &'a Env,
    admin_addr: &Address,
) -> (FeeDistributorContractClient<'a>, SplitConfig) {
    let split = SplitConfig {
        recipients: Vec::from_array(
            env,
            [
                (admin_addr.clone(), 5000u32),
                (alice(env), 3000u32),
                (bob(env), 2000u32),
            ],
        ),
    };
    let contract_id = env.register(FeeDistributorContract, ());
    let client = FeeDistributorContractClient::new(env, &contract_id);
    client.initialize(admin_addr, &split);
    (client, split)
}

// ============================================================================
// initialize
// ============================================================================

#[test]
fn test_initialize() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (_, _) = deploy(&env, &admin_addr);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_initialize_twice() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    let split = client.get_split();
    let other = admin(&env);
    client.initialize(&other, &split);
}

// ============================================================================
// set_split
// ============================================================================

#[test]
fn test_set_split() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    let new_split = SplitConfig {
        recipients: Vec::from_array(
            &env,
            [(admin_addr.clone(), 4000u32), (alice(&env), 6000u32)],
        ),
    };
    client.set_split(&new_split);
    let stored = client.get_split();
    assert_eq!(stored.recipients.len(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_set_split_invalid_total() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    let bad = SplitConfig {
        recipients: Vec::from_array(
            &env,
            [(admin_addr.clone(), 3000u32), (alice(&env), 3000u32)],
        ),
    };
    client.set_split(&bad);
}

// ============================================================================
// add_collector / remove_collector
// ============================================================================

#[test]
fn test_add_remove_collector() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    let collector = alice(&env);
    client.add_collector(&collector);
    client.remove_collector(&collector);
}

// ============================================================================
// deposit_fee
// ============================================================================

#[test]
fn test_deposit_fee() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    client.add_collector(&admin_addr);
    client.deposit_fee(&admin_addr, &1000i128);
    assert_eq!(client.get_pending_fees(), 1000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_deposit_fee_unauthorized() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    let stranger = charlie(&env);
    client.deposit_fee(&stranger, &1000i128);
}

// ============================================================================
// close_period / get_period
// ============================================================================

#[test]
fn test_close_period() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    client.add_collector(&admin_addr);
    client.deposit_fee(&admin_addr, &5000i128);
    let root = BytesN::from_array(&env, &[0u8; 32]);
    client.close_period(&root);
    let period = client.get_period(&0u64);
    assert_eq!(period.total_fees, 5000i128);
    assert_eq!(period.period_id, 0u64);
}

// ============================================================================
// claim
// ============================================================================

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_claim_no_period() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    let proof = Vec::new(&env);
    client.claim(&0u64, &admin_addr, &1000i128, &proof);
}

// ============================================================================
// sweep
// ============================================================================

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_sweep_not_due() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    client.add_collector(&admin_addr);
    client.deposit_fee(&admin_addr, &5000i128);
    let root = BytesN::from_array(&env, &[1u8; 32]);
    client.close_period(&root);
    client.sweep(&0u64, &admin_addr);
}

// ============================================================================
// getters
// ============================================================================

#[test]
fn test_get_current_period_id() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    assert_eq!(client.get_current_period_id(), 0u64);
    client.add_collector(&admin_addr);
    client.deposit_fee(&admin_addr, &1000i128);
    client.close_period(&BytesN::from_array(&env, &[2u8; 32]));
    assert_eq!(client.get_current_period_id(), 0u64);
    client.deposit_fee(&admin_addr, &2000i128);
    client.close_period(&BytesN::from_array(&env, &[3u8; 32]));
    assert_eq!(client.get_current_period_id(), 1u64);
}

#[test]
fn test_get_pending_fees() {
    let env = create_env();
    let admin_addr = admin(&env);
    let (client, _) = deploy(&env, &admin_addr);
    assert_eq!(client.get_pending_fees(), 0i128);
}
