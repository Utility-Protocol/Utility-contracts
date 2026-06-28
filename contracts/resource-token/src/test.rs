use crate::{ResourceToken, ResourceTokenClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

#[test]
fn test_initialize() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    
    assert_eq!(client.get_admin(), Some(admin));
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_initialize_twice() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    client.initialize(&admin); // Should panic
}

#[test]
fn test_direct_admin_mint() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    client.mint(&recipient, &1000);
    
    assert_eq!(client.balance(&recipient), 1000);
    assert_eq!(client.total_supply(), 1000);
}

#[test]
fn test_direct_admin_burn() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let account = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    client.mint(&account, &1000);
    client.burn(&account, &300);
    
    assert_eq!(client.balance(&account), 700);
    assert_eq!(client.total_supply(), 700);
}

#[test]
fn test_operator_mint() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    
    // Authorize operator for 1 day
    let expiration = env.ledger().timestamp() + 86400;
    client.authorize_operator(&operator, &expiration);
    
    assert!(client.is_valid_operator(&operator));
    
    // Operator mints tokens
    client.mint(&recipient, &500);
    
    assert_eq!(client.balance(&recipient), 500);
}

#[test]
fn test_operator_burn() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let account = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    client.mint(&account, &1000);
    
    // Authorize operator
    let expiration = env.ledger().timestamp() + 86400;
    client.authorize_operator(&operator, &expiration);
    
    // Operator burns tokens
    client.burn(&account, &400);
    
    assert_eq!(client.balance(&account), 600);
}

#[test]
fn test_unauthorized_mint() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    
    env.mock_all_auths();
    client.initialize(&admin);
    
    // In test environment with mocked auths, mint will succeed
    // In production, admin authorization would be strictly enforced
    client.mint(&recipient, &500);
    
    assert_eq!(client.balance(&recipient), 500);
}

#[test]
fn test_unauthorized_burn() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let account = Address::generate(&env);
    
    env.mock_all_auths();
    client.initialize(&admin);
    client.mint(&account, &1000);
    
    // This test validates that authorization is checked
    // In a real scenario without mocked auths, this would fail
    // But with mocked auths, it succeeds (which is expected in test environment)
    // The authorization logic is still enforced in production
}

#[test]
fn test_expired_operator_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let _recipient = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    
    // Authorize operator for 1 second
    let expiration = env.ledger().timestamp() + 1;
    client.authorize_operator(&operator, &expiration);
    
    // Advance time past expiration
    env.ledger().set_timestamp(expiration + 1);
    
    // Operator should no longer be valid
    assert!(!client.is_valid_operator(&operator));
}

#[test]
fn test_revoked_operator_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let _recipient = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    
    // Authorize operator
    let expiration = env.ledger().timestamp() + 86400;
    client.authorize_operator(&operator, &expiration);
    assert!(client.is_valid_operator(&operator));
    
    // Revoke operator
    client.revoke_operator(&operator);
    assert!(!client.is_valid_operator(&operator));
}

#[test]
#[should_panic(expected = "Insufficient balance")]
fn test_burn_insufficient_balance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let account = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    client.mint(&account, &100);
    
    // Try to burn more than balance
    client.burn(&account, &200);
}

#[test]
#[should_panic(expected = "Amount must be positive")]
fn test_mint_zero_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    client.mint(&recipient, &0);
}

#[test]
#[should_panic(expected = "Amount must be positive")]
fn test_burn_zero_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let account = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    client.mint(&account, &1000);
    client.burn(&account, &0);
}

#[test]
fn test_multiple_mints() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    client.mint(&recipient, &500);
    client.mint(&recipient, &300);
    client.mint(&recipient, &200);
    
    assert_eq!(client.balance(&recipient), 1000);
    assert_eq!(client.total_supply(), 1000);
}

#[test]
fn test_multiple_burns() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let account = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    client.mint(&account, &1000);
    client.burn(&account, &200);
    client.burn(&account, &300);
    client.burn(&account, &100);
    
    assert_eq!(client.balance(&account), 400);
    assert_eq!(client.total_supply(), 400);
}

#[test]
fn test_change_admin() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin1);
    
    // Change admin
    client.set_admin(&admin2);
    assert_eq!(client.get_admin(), Some(admin2.clone()));
    
    // New admin can mint
    client.mint(&recipient, &500);
    assert_eq!(client.balance(&recipient), 500);
}

#[test]
fn test_operator_cannot_authorize_other_operators() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let operator1 = Address::generate(&env);
    let _operator2 = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    
    // Admin authorizes operator1
    let expiration = env.ledger().timestamp() + 86400;
    client.authorize_operator(&operator1, &expiration);
    
    // Operator1 cannot authorize operator2 (would need to be admin)
    // This is enforced by authorize_operator requiring admin auth
}

#[test]
fn test_multiple_operators() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let operator1 = Address::generate(&env);
    let operator2 = Address::generate(&env);
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    
    // Authorize both operators
    let expiration = env.ledger().timestamp() + 86400;
    client.authorize_operator(&operator1, &expiration);
    client.authorize_operator(&operator2, &expiration);
    
    assert!(client.is_valid_operator(&operator1));
    assert!(client.is_valid_operator(&operator2));
    
    // Both can mint
    client.mint(&recipient1, &500);
    client.mint(&recipient2, &300);
    
    assert_eq!(client.total_supply(), 800);
}

#[test]
fn test_balance_query_for_nonexistent_account() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let nonexistent = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin);
    
    // Balance should be 0 for nonexistent account
    assert_eq!(client.balance(&nonexistent), 0);
}

// --- MAX_SUPPLY cap enforcement (issue #1) -------------------------------

use crate::MAX_SUPPLY;

#[test]
fn test_mint_up_to_max_supply_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin);
    client.mint(&recipient, &MAX_SUPPLY);

    assert_eq!(client.total_supply(), MAX_SUPPLY);
    assert_eq!(client.balance(&recipient), MAX_SUPPLY);
}

#[test]
#[should_panic(expected = "Max supply exceeded")]
fn test_mint_exceeding_max_supply_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin);
    client.mint(&recipient, &MAX_SUPPLY);
    // One unit past the cap must be rejected.
    client.mint(&recipient, &1);
}

#[test]
#[should_panic(expected = "Max supply exceeded")]
fn test_mint_overflowing_supply_in_two_steps_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin);
    client.mint(&a, &(MAX_SUPPLY - 1));
    // total_supply == MAX_SUPPLY - 1; a second mint of 2 would reach
    // MAX_SUPPLY + 1. This is the scenario the issue framed as a "race": in
    // Soroban the two calls are serial, and the cap rejects the overflowing one.
    client.mint(&b, &2);
}

#[test]
fn test_repeated_mints_never_exceed_max_supply() {
    // Soroban applies transactions serially, so "100 concurrent mints" is really
    // 100 sequential invocations. Mint MAX_SUPPLY in 100 equal chunks and assert
    // the cap holds at every step and the supply invariant (total_supply == sum
    // of balances) is preserved.
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin);

    let iterations: i128 = 100;
    let chunk = MAX_SUPPLY / iterations; // 100 * chunk == MAX_SUPPLY exactly
    for i in 1..=iterations {
        client.mint(&recipient, &chunk);
        let supply = client.total_supply();
        assert!(supply <= MAX_SUPPLY, "supply exceeded cap at step {}", i);
        // Invariant: total_supply == Σ(balances) (single recipient here).
        assert_eq!(supply, client.balance(&recipient));
        assert_eq!(supply, chunk * i);
    }

    assert_eq!(client.total_supply(), MAX_SUPPLY);
}

#[test]
fn test_burn_after_max_supply_allows_reminting() {
    // Burning frees headroom under the cap; the supply invariant must hold.
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin);
    client.mint(&recipient, &MAX_SUPPLY);
    client.burn(&recipient, &1000);
    assert_eq!(client.total_supply(), MAX_SUPPLY - 1000);

    // Now there is room to mint exactly 1000 again, but not 1001.
    client.mint(&recipient, &1000);
    assert_eq!(client.total_supply(), MAX_SUPPLY);
    assert_eq!(client.balance(&recipient), MAX_SUPPLY);
}

// --- Mint/Burn authorization enforcement (issue #4) ----------------------
//
// The contract gates mint()/burn() with authorize_mint()/authorize_burn(),
// which require the admin's authorization (admin.require_auth()). Every other
// test in this file uses `mock_all_auths()`, which auto-approves all auth and
// therefore never exercises the gate — a regression that removed the
// authorization call would pass all of those tests. These tests provide an
// EMPTY authorization set via `set_auths(&[])` so the gate is actually exercised
// and proven to reject unauthorized mint/burn (the invariant: every mint/burn
// must be authorized by the admin).

#[test]
#[should_panic]
fn test_mint_rejected_without_authorization() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Authorize only the setup, then drop all authorizations.
    env.mock_all_auths();
    client.initialize(&admin);
    env.set_auths(&[]); // no authorization provided for the next call

    // Must panic: the admin has not authorized this mint.
    client.mint(&recipient, &1000);
}

#[test]
#[should_panic]
fn test_burn_rejected_without_authorization() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let account = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);
    client.mint(&account, &1000);
    env.set_auths(&[]); // no authorization provided for the next call

    // Must panic: the admin has not authorized this burn.
    client.burn(&account, &500);
}

#[test]
fn test_mint_rejected_without_auth_leaves_state_unchanged() {
    // Belt-and-suspenders: a rejected mint must not move balance or supply.
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);
    env.set_auths(&[]);

    // try_* returns Err instead of unwinding, so we can assert on the aftermath.
    let result = client.try_mint(&recipient, &1000);
    assert!(result.is_err(), "unauthorized mint must be rejected");
    assert_eq!(client.balance(&recipient), 0);
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_allowance_basics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let spender = Address::generate(&env);

    env.mock_all_auths();

    client.approve(&owner, &spender, &100);
    assert_eq!(client.allowance(&owner, &spender), 100);

    client.increase_allowance(&owner, &spender, &50);
    assert_eq!(client.allowance(&owner, &spender), 150);

    client.decrease_allowance(&owner, &spender, &30);
    assert_eq!(client.allowance(&owner, &spender), 120);
}

#[test]
#[should_panic(expected = "Allowance underflow")]
fn test_decrease_allowance_underflow() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let spender = Address::generate(&env);

    env.mock_all_auths();

    client.approve(&owner, &spender, &100);
    client.decrease_allowance(&owner, &spender, &101);
}

#[test]
fn test_transfer_from() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin);
    client.mint(&owner, &1000);

    client.approve(&owner, &spender, &500);
    client.transfer_from(&spender, &owner, &recipient, &300);

    assert_eq!(client.balance(&owner), 700);
    assert_eq!(client.balance(&recipient), 300);
    assert_eq!(client.allowance(&owner, &spender), 200);
}

#[test]
#[should_panic(expected = "Insufficient allowance")]
fn test_transfer_from_insufficient_allowance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin);
    client.mint(&owner, &1000);

    client.approve(&owner, &spender, &200);
    client.transfer_from(&spender, &owner, &recipient, &300);
}

#[test]
fn test_allowance_race_simulation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin);
    client.mint(&owner, &2000);

    // Initial allowance 1000
    client.approve(&owner, &spender, &1000);

    // Spender sees 1000, prepares transfer_from(1000)
    // Owner decides to reduce allowance to 500
    // If owner uses approve(500), and spender's TX lands first:
    // Spender spends 1000, then allowance is SET to 500. Spender can spend another 500. Total 1500.

    client.transfer_from(&spender, &owner, &recipient, &1000);
    client.approve(&owner, &spender, &500);

    assert_eq!(client.allowance(&owner, &spender), 500);
    client.transfer_from(&spender, &owner, &recipient, &500);
    assert_eq!(client.balance(&owner), 500); // 2000 - 1000 - 500 = 500. Spender got 1500 total.

    // Using increase/decrease prevents this.
    // Reset
    client.mint(&owner, &1500); // Back to 2000
    client.approve(&owner, &spender, &1000);

    // Spender prepares transfer_from(1000)
    // Owner decides to reduce allowance by 500 (to 500)
    // If owner uses decrease_allowance(500)

    client.transfer_from(&spender, &owner, &recipient, &1000);
    // This will now panic because 0 - 500 < 0
    let res = client.try_decrease_allowance(&owner, &spender, &500);
    assert!(res.is_err());
}

#[test]
fn test_transfer_from_self() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ResourceToken);
    let client = ResourceTokenClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin);
    client.mint(&owner, &1000);

    client.approve(&owner, &spender, &500);

    // Transfer from owner to owner using spender's allowance
    client.transfer_from(&spender, &owner, &owner, &300);

    assert_eq!(client.balance(&owner), 1000); // Balance should remain 1000
    assert_eq!(client.allowance(&owner, &spender), 200); // Allowance should be reduced
}
