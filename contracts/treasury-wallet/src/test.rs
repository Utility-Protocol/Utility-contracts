#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env, Vec};

const T0: u64 = 1_000_000;
const TIMELOCK: u64 = 3600; // 1 hour
const EXPIRY: u64 = 7 * 24 * 3600; // 7 days
const HIGH_VALUE: i128 = 1_000_000; // 100 XLM worth of token units (example)

// --- Helpers ---------------------------------------------------------------

fn signers(env: &Env, n: u32) -> Vec<Address> {
    let mut v = Vec::new(env);
    for _ in 0..n {
        v.push_back(Address::generate(env));
    }
    v
}

fn create_token(env: &Env) -> (Address, Address) {
    let admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    (admin, token)
}

/// Deploy an initialized 3-of-5 wallet and mint funds into it.
fn setup() -> (
    Env,
    TreasuryWalletContractClient<'static>,
    Address,
    Vec<Address>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);

    let contract_id = env.register(TreasuryWalletContract, ());
    let client = TreasuryWalletContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let s = signers(&env, 5);
    let (token_admin, token) = create_token(&env);

    client.initialize(&owner, &s, &3, &HIGH_VALUE, &TIMELOCK, &EXPIRY);

    // Mint treasury funds to the wallet contract.
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&contract_id, &10_000_000i128);

    (env, client, owner, s, token, token_admin, contract_id)
}

fn recipient(env: &Env) -> Address {
    Address::generate(env)
}

// ============================================================================
// initialize
// ============================================================================

#[test]
fn test_initialize_sets_config() {
    let (env, client, owner, s, _token, _, _contract_id) = setup();
    let cfg = client.get_config();
    assert_eq!(cfg.owner, owner);
    assert_eq!(cfg.signers.len(), 5);
    assert_eq!(cfg.required_signatures, 3);
    assert_eq!(cfg.high_value_threshold, HIGH_VALUE);
    assert_eq!(cfg.timelock_seconds, TIMELOCK);
    assert_eq!(cfg.expiry_seconds, EXPIRY);
    assert_eq!(client.get_proposal_count(), 0);
    // Signer lookup
    assert!(client.is_signer(&s.get(0).unwrap()));
    let outsider = Address::generate(&env);
    assert!(!client.is_signer(&outsider));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_initialize_twice_fails() {
    let (env, client, owner, s, _token, _, _contract_id) = setup();
    let (_token2, _) = create_token(&env);
    client.initialize(&owner, &s, &3, &HIGH_VALUE, &TIMELOCK, &EXPIRY);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_initialize_too_few_signers() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);
    let contract_id = env.register(TreasuryWalletContract, ());
    let client = TreasuryWalletContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let s = signers(&env, 1);
    client.initialize(&owner, &s, &1, &HIGH_VALUE, &TIMELOCK, &EXPIRY);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_initialize_too_many_signers() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);
    let contract_id = env.register(TreasuryWalletContract, ());
    let client = TreasuryWalletContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let s = signers(&env, 8);
    client.initialize(&owner, &s, &4, &HIGH_VALUE, &TIMELOCK, &EXPIRY);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_initialize_threshold_too_low() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);
    let contract_id = env.register(TreasuryWalletContract, ());
    let client = TreasuryWalletContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let s = signers(&env, 5);
    // 1-of-5 is below the strict majority minimum of 3.
    client.initialize(&owner, &s, &1, &HIGH_VALUE, &TIMELOCK, &EXPIRY);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_initialize_threshold_too_high() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);
    let contract_id = env.register(TreasuryWalletContract, ());
    let client = TreasuryWalletContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let s = signers(&env, 5);
    client.initialize(&owner, &s, &6, &HIGH_VALUE, &TIMELOCK, &EXPIRY);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_initialize_duplicate_signers() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);
    let contract_id = env.register(TreasuryWalletContract, ());
    let client = TreasuryWalletContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let dup = Address::generate(&env);
    let s = Vec::from_array(&env, [dup.clone(), dup.clone(), dup.clone()]);
    client.initialize(&owner, &s, &2, &HIGH_VALUE, &TIMELOCK, &EXPIRY);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_get_config_before_init_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TreasuryWalletContract, ());
    let client = TreasuryWalletContractClient::new(&env, &contract_id);
    client.get_config();
}

// ============================================================================
// add_signer / remove_signer
// ============================================================================

#[test]
fn test_add_signer_grows_wallet() {
    let (env, client, owner, _s, _, _, _) = setup();
    let newcomer = Address::generate(&env);
    client.add_signer(&owner, &newcomer);
    let cfg = client.get_config();
    assert_eq!(cfg.signers.len(), 6);
    assert!(client.is_signer(&newcomer));
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_add_signer_non_owner_rejected() {
    let (env, client, _owner, _s, _, _, _) = setup();
    let newcomer = Address::generate(&env);
    let attacker = Address::generate(&env);
    client.add_signer(&attacker, &newcomer);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_add_signer_duplicate_rejected() {
    let (_env, client, owner, s, _, _, _) = setup();
    client.add_signer(&owner, &s.get(0).unwrap());
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_add_signer_over_cap_rejected() {
    let (env, client, owner, _s, _, _, _) = setup();
    // 5 + 1 = 6
    client.add_signer(&owner, &Address::generate(&env));
    // 6 + 1 = 7
    client.add_signer(&owner, &Address::generate(&env));
    // 7 + 1 = 8 -> cap exceeded
    client.add_signer(&owner, &Address::generate(&env));
}

#[test]
fn test_remove_signer_shrinks_wallet() {
    let (_env, client, owner, s, _, _, _) = setup();
    client.remove_signer(&owner, &s.get(4).unwrap());
    let cfg = client.get_config();
    assert_eq!(cfg.signers.len(), 4);
    assert!(!client.is_signer(&s.get(4).unwrap()));
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_remove_signer_non_owner_rejected() {
    let (env, client, _owner, s, _, _, _) = setup();
    let attacker = Address::generate(&env);
    client.remove_signer(&attacker, &s.get(0).unwrap());
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_remove_signer_not_found() {
    let (env, client, owner, _s, _, _, _) = setup();
    let outsider = Address::generate(&env);
    client.remove_signer(&owner, &outsider);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_remove_signer_below_minimum_rejected() {
    let (_env, client, owner, s, _, _, _) = setup();
    // 5 -> 4 -> 3 -> 2 is fine, but 2 -> 1 violates MIN_SIGNERS.
    client.remove_signer(&owner, &s.get(4).unwrap());
    client.remove_signer(&owner, &s.get(3).unwrap());
    client.remove_signer(&owner, &s.get(2).unwrap());
    client.remove_signer(&owner, &s.get(1).unwrap());
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_remove_signer_breaks_threshold_rejected() {
    // 3-of-3 wallet: removing any signer makes the threshold unsatisfiable.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);
    let contract_id = env.register(TreasuryWalletContract, ());
    let client = TreasuryWalletContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let s = signers(&env, 3);
    client.initialize(&owner, &s, &3, &HIGH_VALUE, &TIMELOCK, &EXPIRY);
    client.remove_signer(&owner, &s.get(0).unwrap());
}

// ============================================================================
// update_config
// ============================================================================

#[test]
fn test_update_config_changes_parameters() {
    let (_env, client, owner, _s, _, _, _) = setup();
    client.update_config(&owner, &4, &2_000_000i128, &7200u64, &(14 * 24 * 3600u64));
    let cfg = client.get_config();
    assert_eq!(cfg.required_signatures, 4);
    assert_eq!(cfg.high_value_threshold, 2_000_000);
    assert_eq!(cfg.timelock_seconds, 7200);
    assert_eq!(cfg.expiry_seconds, 14 * 24 * 3600);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_update_config_non_owner_rejected() {
    let (env, client, _owner, _s, _, _, _) = setup();
    let attacker = Address::generate(&env);
    client.update_config(&attacker, &4, &2_000_000i128, &7200u64, &EXPIRY);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_update_config_bad_threshold_rejected() {
    let (_env, client, owner, _s, _, _, _) = setup();
    client.update_config(&owner, &6, &2_000_000i128, &7200u64, &EXPIRY);
}

// ============================================================================
// submit_transaction
// ============================================================================

#[test]
fn test_submit_transaction_creates_proposal() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let proposer = s.get(0).unwrap();
    let id = client.submit_transaction(&proposer, &token, &to, &50_000i128);
    assert_eq!(id, 0);
    let proposal = client.get_proposal(&id);
    assert_eq!(proposal.proposer, proposer);
    assert_eq!(proposal.to, to);
    assert_eq!(proposal.amount, 50_000);
    assert_eq!(proposal.approval_count, 1); // proposer auto-approves
    assert_eq!(client.get_proposal_count(), 1);
    assert!(client.has_approved(&id, &proposer));
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_submit_transaction_non_signer_rejected() {
    let (env, client, _owner, _s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let outsider = Address::generate(&env);
    client.submit_transaction(&outsider, &token, &to, &50_000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_submit_transaction_zero_amount_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    client.submit_transaction(&s.get(0).unwrap(), &token, &to, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_submit_transaction_token_equals_recipient_rejected() {
    let (_env, client, _owner, s, token, _, _contract_id) = setup();
    client.submit_transaction(&s.get(0).unwrap(), &token, &token, &50_000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_get_proposal_not_found() {
    let (_env, client, _owner, _s, _token, _, _contract_id) = setup();
    client.get_proposal(&99);
}

// ============================================================================
// approve_transaction
// ============================================================================

#[test]
fn test_approve_reaches_threshold() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    // Below high-value threshold: timelock is skipped, execution allowed at once.
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.approve_transaction(&id, &s.get(2).unwrap());
    let proposal = client.get_proposal(&id);
    assert_eq!(proposal.approval_count, 3);
    assert!(proposal.threshold_reached_at > 0);
    assert_eq!(
        proposal.earliest_execution_at,
        proposal.threshold_reached_at
    );
    assert!(client.has_approved(&id, &s.get(2).unwrap()));
    assert_eq!(client.get_approval_count(&id), 3);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_approve_non_signer_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let id = client.submit_transaction(&s.get(0).unwrap(), &token, &to, &50_000i128);
    let outsider = Address::generate(&env);
    client.approve_transaction(&id, &outsider);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_approve_duplicate_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    client.approve_transaction(&id, &p0);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_approve_after_executed_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.approve_transaction(&id, &s.get(2).unwrap());
    client.execute_transaction(&id);
    client.approve_transaction(&id, &s.get(3).unwrap());
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_approve_after_expiry_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    // Jump past the expiry window.
    env.ledger().set_timestamp(T0 + EXPIRY + 1);
    client.approve_transaction(&id, &s.get(1).unwrap());
}

// ============================================================================
// revoke_approval
// ============================================================================

#[test]
fn test_revoke_drops_below_threshold_disarms_timelock() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    let a1 = s.get(1).unwrap();
    let a2 = s.get(2).unwrap();
    client.approve_transaction(&id, &a1);
    client.approve_transaction(&id, &a2);
    assert!(client.get_proposal(&id).threshold_reached_at > 0);

    client.revoke_approval(&id, &a2);
    let proposal = client.get_proposal(&id);
    assert_eq!(proposal.approval_count, 2);
    assert_eq!(proposal.threshold_reached_at, 0);
    assert!(!client.has_approved(&id, &a2));
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_revoke_without_approval_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let id = client.submit_transaction(&s.get(0).unwrap(), &token, &to, &50_000i128);
    let a1 = s.get(1).unwrap();
    client.revoke_approval(&id, &a1);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_revoke_non_signer_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let id = client.submit_transaction(&s.get(0).unwrap(), &token, &to, &50_000i128);
    let outsider = Address::generate(&env);
    client.revoke_approval(&id, &outsider);
}

// ============================================================================
// execute_transaction (non-high-value: immediate once threshold met)
// ============================================================================

#[test]
fn test_execute_small_transfer_success() {
    let (env, client, _owner, s, token, _, contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.approve_transaction(&id, &s.get(2).unwrap());
    client.execute_transaction(&id);

    let proposal = client.get_proposal(&id);
    assert!(proposal.is_executed);
    // Funds moved from wallet to recipient.
    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&to), 50_000);
    assert_eq!(token_client.balance(&contract_id), 10_000_000 - 50_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_execute_insufficient_approvals_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let id = client.submit_transaction(&s.get(0).unwrap(), &token, &to, &50_000i128);
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.execute_transaction(&id);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_execute_twice_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.approve_transaction(&id, &s.get(2).unwrap());
    client.execute_transaction(&id);
    client.execute_transaction(&id);
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_execute_expired_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.approve_transaction(&id, &s.get(2).unwrap());
    env.ledger().set_timestamp(T0 + EXPIRY + 1);
    client.execute_transaction(&id);
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_execute_insufficient_balance_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    // Above the high-value threshold, so the timelock must elapse before the
    // balance check is even reached.
    let id = client.submit_transaction(&p0, &token, &to, &99_000_000i128);
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.approve_transaction(&id, &s.get(2).unwrap());
    let reached = client.get_proposal(&id).threshold_reached_at;
    env.ledger().set_timestamp(reached + TIMELOCK);
    client.execute_transaction(&id);
}

// ============================================================================
// Time-locked execution for high-value transactions
// ============================================================================

#[test]
fn test_high_value_tx_enforces_timelock() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    // Amount at or above high_value_threshold => timelock applies.
    let id = client.submit_transaction(&p0, &token, &to, &HIGH_VALUE);
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.approve_transaction(&id, &s.get(2).unwrap());

    let proposal = client.get_proposal(&id);
    assert!(proposal.threshold_reached_at > 0);
    assert_eq!(
        proposal.earliest_execution_at,
        proposal.threshold_reached_at + TIMELOCK
    );

    // Executing before the timelock elapses must fail.
    env.ledger()
        .set_timestamp(proposal.threshold_reached_at + TIMELOCK - 1);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_transaction(&id);
    }));
    assert!(res.is_err());

    // Executing once the timelock has elapsed succeeds.
    env.ledger()
        .set_timestamp(proposal.threshold_reached_at + TIMELOCK);
    client.execute_transaction(&id);
    assert!(client.get_proposal(&id).is_executed);
    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&to), HIGH_VALUE);
}

#[test]
fn test_high_value_threshold_boundary_immediate() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    // Just below the high-value threshold: no timelock.
    let id = client.submit_transaction(&p0, &token, &to, &(HIGH_VALUE - 1));
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.approve_transaction(&id, &s.get(2).unwrap());
    let proposal = client.get_proposal(&id);
    assert_eq!(
        proposal.earliest_execution_at,
        proposal.threshold_reached_at
    );
    client.execute_transaction(&id);
    assert!(client.get_proposal(&id).is_executed);
}

#[test]
fn test_revoke_then_reapprove_rearms_timelock() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &HIGH_VALUE);
    let a1 = s.get(1).unwrap();
    let a2 = s.get(2).unwrap();
    client.approve_transaction(&id, &a1);
    client.approve_transaction(&id, &a2);
    let first_reach = client.get_proposal(&id).threshold_reached_at;

    // A signer changes their mind: threshold drops, timelock disarms.
    client.revoke_approval(&id, &a2);
    assert_eq!(client.get_proposal(&id).threshold_reached_at, 0);

    // Re-approval re-arms the timelock from the new threshold-reached time.
    env.ledger().set_timestamp(first_reach + TIMELOCK + 1);
    client.approve_transaction(&id, &a2);
    let proposal = client.get_proposal(&id);
    assert_eq!(
        proposal.earliest_execution_at,
        proposal.threshold_reached_at + TIMELOCK
    );

    // Even though plenty of time passed since the first reach, the clock
    // restarted on re-approval, so execution must still wait.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_transaction(&id);
    }));
    assert!(res.is_err());

    env.ledger()
        .set_timestamp(proposal.threshold_reached_at + TIMELOCK);
    client.execute_transaction(&id);
    assert!(client.get_proposal(&id).is_executed);
}

// ============================================================================
// cancel_transaction
// ============================================================================

#[test]
fn test_cancel_by_owner() {
    let (env, client, owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let id = client.submit_transaction(&s.get(0).unwrap(), &token, &to, &50_000i128);
    client.cancel_transaction(&id, &owner);
    let proposal = client.get_proposal(&id);
    assert!(proposal.is_cancelled);
    assert!(!proposal.is_executed);
}

#[test]
fn test_cancel_by_proposer() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    client.cancel_transaction(&id, &p0);
    assert!(client.get_proposal(&id).is_cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_cancel_by_stranger_rejected() {
    let (env, client, _owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let id = client.submit_transaction(&s.get(0).unwrap(), &token, &to, &50_000i128);
    // A signer who did not propose is not allowed to cancel.
    client.cancel_transaction(&id, &s.get(1).unwrap());
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_cancel_twice_rejected() {
    let (env, client, owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let id = client.submit_transaction(&s.get(0).unwrap(), &token, &to, &50_000i128);
    client.cancel_transaction(&id, &owner);
    client.cancel_transaction(&id, &owner);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_cancel_after_executed_rejected() {
    let (env, client, owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.approve_transaction(&id, &s.get(2).unwrap());
    client.execute_transaction(&id);
    client.cancel_transaction(&id, &owner);
}

#[test]
fn test_cancelled_proposal_cannot_execute() {
    let (env, client, owner, s, token, _, _contract_id) = setup();
    let to = recipient(&env);
    let p0 = s.get(0).unwrap();
    let id = client.submit_transaction(&p0, &token, &to, &50_000i128);
    client.approve_transaction(&id, &s.get(1).unwrap());
    client.approve_transaction(&id, &s.get(2).unwrap());
    client.cancel_transaction(&id, &owner);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_transaction(&id);
    }));
    assert!(res.is_err());
    // Funds untouched.
    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&to), 0);
}
