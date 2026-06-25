use soroban_sdk::{
    contract, contractimpl, contracterror, panic_with_error, Address, Bytes, Env,
};

use crate::settlement_types::SettlementProposal;
use crate::settlement_lock_manager::{lock_resources, release_locked_resources};
use crate::{encode_raw_key, DataKey, NAMESPACE_SETTLEMENT};

/// Settlement window bounds: minimum 60 seconds (1 minute)
pub const MIN_SETTLEMENT_WINDOW: u64 = 60;
/// Settlement window bounds: maximum 604800 seconds (7 days)
pub const MAX_SETTLEMENT_WINDOW: u64 = 604800;

/// Encode a settlement storage key with the SETTLEMENT namespace prefix.
fn settlement_key(env: &Env, raw: &[u8]) -> Bytes {
    encode_raw_key(env, &NAMESPACE_SETTLEMENT, raw)
}

/// Settlement error codes
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SettlementError {
    /// Settlement deadline has been exceeded
    DeadlineExceeded = 1,
    /// Settlement window is outside valid bounds
    InvalidSettlementWindow = 2,
    /// Proposal not found
    ProposalNotFound = 3,
    /// Proposal already finalized
    AlreadyFinalized = 4,
    /// Unauthorized access
    Unauthorized = 5,
}

#[contract]
pub struct SettlementContract;

#[contractimpl]
impl SettlementContract {
    /// Propose a new settlement
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `proposal_id` - Unique identifier for the proposal
    /// * `payer` - Address of the payer
    /// * `payee` - Address of the payee
    /// * `amount` - Amount to be settled
    /// * `rate` - Exchange rate
    /// * `settlement_window` - Time window in seconds (must be between 60 and 604800)
    /// * `token_address` - Token contract address for locking
    pub fn propose_settlement(
        env: Env,
        proposal_id: u64,
        payer: Address,
        payee: Address,
        amount: i128,
        rate: i128,
        settlement_window: u64,
        token_address: Address,
    ) -> SettlementProposal {
        // Validate settlement_window bounds
        if settlement_window < MIN_SETTLEMENT_WINDOW || settlement_window > MAX_SETTLEMENT_WINDOW {
            panic_with_error!(env, SettlementError::InvalidSettlementWindow);
        }

        // Require authorization from payer
        payer.require_auth();

        // Get current ledger timestamp
        let submission_timestamp = env.ledger().timestamp();

        // Create the proposal
        let mut proposal = SettlementProposal::new(
            proposal_id,
            payer.clone(),
            payee,
            amount,
            rate,
            submission_timestamp,
            settlement_window,
        );

        // Lock resources
        lock_resources(&env, &mut proposal, &token_address);

        // Store the proposal
        let skey = settlement_key(&env, &proposal_id.to_be_bytes());
        env.storage().persistent().set(&skey, &proposal);

        proposal
    }

    /// Finalize a settlement
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `proposal_id` - ID of the proposal to finalize
    /// * `token_address` - Token contract address for unlocking
    /// 
    /// # Panics
    /// * If the current ledger timestamp exceeds the settlement deadline (DeadlineExceeded)
    /// * If the proposal is not found (ProposalNotFound)
    /// * If the proposal is already finalized (AlreadyFinalized)
    pub fn finalize_settlement(
        env: Env,
        proposal_id: u64,
        token_address: Address,
    ) {
        // IDEMPOTENCY GUARD — check BEFORE any state mutation
        if env.storage().instance().has(&DataKey::BatchFinalized(proposal_id)) {
            panic_with_error!(&env, SettlementError::AlreadyFinalized);
        }
        env.storage().instance().set(&DataKey::BatchFinalized(proposal_id), &true);

        // Retrieve the proposal
        let skey = settlement_key(&env, &proposal_id.to_be_bytes());
        let mut proposal: SettlementProposal = env
            .storage()
            .persistent()
            .get(&skey)
            .unwrap_or_else(|| panic_with_error!(&env, SettlementError::ProposalNotFound));

        // Deadline check — the BatchFinalized flag remains true even if we panic here
        let current_timestamp = env.ledger().timestamp();
        if current_timestamp > proposal.settlement_deadline {
            release_locked_resources(&env, &mut proposal, &token_address);
            panic_with_error!(&env, SettlementError::DeadlineExceeded);
        }

        if proposal.finalized {
            panic_with_error!(&env, SettlementError::AlreadyFinalized);
        }

        proposal.payee.require_auth();

        proposal.finalized = true;

        release_locked_resources(&env, &mut proposal, &token_address);

        // Store the updated proposal
        env.storage().persistent().set(&skey, &proposal);
    }

    /// Get a settlement proposal by ID
    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<SettlementProposal> {
        let skey = settlement_key(&env, &proposal_id.to_be_bytes());
        env.storage().persistent().get(&skey)
    }

    /// Check if a proposal deadline has passed
    pub fn is_deadline_exceeded(env: Env, proposal_id: u64) -> bool {
        if let Some(proposal) = Self::get_proposal(env.clone(), proposal_id) {
            let current_timestamp = env.ledger().timestamp();
            current_timestamp > proposal.settlement_deadline
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env,
    };

    #[test]
    fn test_settlement_window_validation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SettlementContract);
        let client = SettlementContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = Address::generate(&env);

        // Test window too small (59 seconds)
        let result = std::panic::catch_unwind(|| {
            client.propose_settlement(&1, &payer, &payee, &1000, &100, &59, &token);
        });
        assert!(result.is_err());

        // Test window too large (7 days + 1 second)
        let result = std::panic::catch_unwind(|| {
            client.propose_settlement(&2, &payer, &payee, &1000, &100, &604801, &token);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_settlement_finalized_before_deadline_succeeds() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SettlementContract);
        let client = SettlementContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = Address::generate(&env);

        // Set initial ledger timestamp
        env.ledger().with_mut(|li| {
            li.timestamp = 1000;
        });

        // Create proposal with 300 second window
        let proposal = client.propose_settlement(&1, &payer, &payee, &1000, &100, &300, &token);
        
        assert_eq!(proposal.submission_timestamp, 1000);
        assert_eq!(proposal.settlement_deadline, 1300);

        // Finalize before deadline (at timestamp 1200)
        env.ledger().with_mut(|li| {
            li.timestamp = 1200;
        });

        client.finalize_settlement(&1, &token);

        // Verify finalization
        let stored_proposal = client.get_proposal(&1).unwrap();
        assert!(stored_proposal.finalized);
    }

    #[test]
    fn test_settlement_finalized_exactly_at_deadline_succeeds() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SettlementContract);
        let client = SettlementContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = Address::generate(&env);

        // Set initial ledger timestamp
        env.ledger().with_mut(|li| {
            li.timestamp = 1000;
        });

        // Create proposal with 300 second window
        let proposal = client.propose_settlement(&2, &payer, &payee, &1000, &100, &300, &token);
        
        assert_eq!(proposal.settlement_deadline, 1300);

        // Finalize exactly at deadline (at timestamp 1300)
        env.ledger().with_mut(|li| {
            li.timestamp = 1300;
        });

        client.finalize_settlement(&2, &token);

        // Verify finalization
        let stored_proposal = client.get_proposal(&2).unwrap();
        assert!(stored_proposal.finalized);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_settlement_finalized_after_deadline_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SettlementContract);
        let client = SettlementContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = Address::generate(&env);

        // Set initial ledger timestamp
        env.ledger().with_mut(|li| {
            li.timestamp = 1000;
        });

        // Create proposal with 300 second window
        client.propose_settlement(&3, &payer, &payee, &1000, &100, &300, &token);

        // Try to finalize 1 second after deadline (at timestamp 1301)
        env.ledger().with_mut(|li| {
            li.timestamp = 1301;
        });

        // This should panic with DeadlineExceeded error (code 1)
        client.finalize_settlement(&3, &token);
    }

    #[test]
    fn test_settlement_window_bounds() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SettlementContract);
        let client = SettlementContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = Address::generate(&env);

        // Test minimum valid window (60 seconds)
        let proposal = client.propose_settlement(&4, &payer, &payee, &1000, &100, &60, &token);
        assert_eq!(proposal.settlement_deadline - proposal.submission_timestamp, 60);

        // Test maximum valid window (604800 seconds = 7 days)
        let proposal = client.propose_settlement(&5, &payer, &payee, &1000, &100, &604800, &token);
        assert_eq!(proposal.settlement_deadline - proposal.submission_timestamp, 604800);
    }

    #[test]
    fn test_is_deadline_exceeded() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SettlementContract);
        let client = SettlementContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = Address::generate(&env);

        // Set initial timestamp
        env.ledger().with_mut(|li| {
            li.timestamp = 1000;
        });

        // Create proposal with 300 second window
        client.propose_settlement(&6, &payer, &payee, &1000, &100, &300, &token);

        // Check before deadline
        assert!(!client.is_deadline_exceeded(&6));

        // Move time past deadline
        env.ledger().with_mut(|li| {
            li.timestamp = 1301;
        });

        // Check after deadline
        assert!(client.is_deadline_exceeded(&6));
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_double_finalization_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SettlementContract);
        let client = SettlementContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = Address::generate(&env);

        // Set initial timestamp
        env.ledger().with_mut(|li| {
            li.timestamp = 1000;
        });

        // Create and finalize proposal
        client.propose_settlement(&7, &payer, &payee, &1000, &100, &300, &token);
        client.finalize_settlement(&7, &token);

        // Try to finalize again - should panic with AlreadyFinalized error (code 4)
        client.finalize_settlement(&7, &token);
    }

    #[test]
    fn test_double_finalization_via_independent_flag() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SettlementContract);
        let client = SettlementContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = Address::generate(&env);

        env.ledger().with_mut(|li| {
            li.timestamp = 1000;
        });

        // Create and finalize proposal (all good)
        client.propose_settlement(&8, &payer, &payee, &1000, &100, &300, &token);
        client.finalize_settlement(&8, &token);

        // Manually reset proposal.finalized to false in storage (simulating corruption)
        // to prove the independent BatchFinalized flag blocks re-finalization
        let proposal_key = settlement_key(&env, &8u64.to_be_bytes());
        env.as_contract(&contract_id, || {
            let mut p: SettlementProposal = env.storage().persistent().get(&proposal_key).unwrap();
            p.finalized = false;
            env.storage().persistent().set(&proposal_key, &p);
        });

        // Should still fail because the BatchFinalized flag is independently set
        let result = std::panic::catch_unwind(|| {
            client.finalize_settlement(&8, &token);
        });
        assert!(
            result.is_err(),
            "Re-finalization should be blocked by independent BatchFinalized flag"
        );
    }

    #[test]
    fn test_partial_execution_after_deadline_blocks_retry() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SettlementContract);
        let client = SettlementContractClient::new(&env, &contract_id);

        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = Address::generate(&env);

        env.ledger().with_mut(|li| {
            li.timestamp = 1000;
        });

        client.propose_settlement(&9, &payer, &payee, &1000, &100, &300, &token);

        // Move past deadline (deadline = 1300)
        env.ledger().with_mut(|li| {
            li.timestamp = 1400;
        });

        // First attempt: BatchFinalized flag is set, then DeadlineExceeded panic
        let result = std::panic::catch_unwind(|| {
            client.finalize_settlement(&9, &token);
        });
        assert!(result.is_err(), "Finalization past deadline should fail");

        // Move back within deadline
        env.ledger().with_mut(|li| {
            li.timestamp = 1200;
        });

        // Second attempt: should fail with AlreadyFinalized (flag from first attempt)
        let result = std::panic::catch_unwind(|| {
            client.finalize_settlement(&9, &token);
        });
        assert!(
            result.is_err(),
            "Retry after partial-execution deadline failure should be blocked"
        );
    }
}
