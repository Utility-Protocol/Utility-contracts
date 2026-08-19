#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error, token,
    Address, Env, Vec,
};

mod events;
use events::*;

// ============================================================================
// Constants
// ============================================================================

/// Minimum number of signers allowed on the treasury wallet.
const MIN_SIGNERS: u32 = 2;
/// Maximum number of signers allowed on the treasury wallet.
const MAX_SIGNERS: u32 = 7;

// ============================================================================
// Errors
// ============================================================================

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TreasuryError {
    NotAuthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    InvalidSignerCount = 4,
    InvalidThreshold = 5,
    DuplicateSigner = 6,
    SignerNotFound = 7,
    ProposalNotFound = 8,
    AlreadyApproved = 9,
    NotApprovedBySigner = 10,
    InsufficientApprovals = 11,
    AlreadyExecuted = 12,
    AlreadyCancelled = 13,
    ProposalExpired = 14,
    TimelockNotElapsed = 15,
    ZeroAmount = 16,
    InsufficientBalance = 17,
    InvalidToken = 18,
    CannotRemoveSigner = 19,
}

// ============================================================================
// Data types
// ============================================================================

#[contracttype]
#[derive(Clone)]
pub struct TreasuryConfig {
    /// Address authorized to manage signers and thresholds (usually a governance DAO).
    pub owner: Address,
    /// Current set of signers (M-of-N wallet, N between MIN_SIGNERS and MAX_SIGNERS).
    pub signers: Vec<Address>,
    /// Number of approvals required to execute a transaction (M).
    pub required_signatures: u32,
    /// Amounts >= this value are treated as high-value and time-locked.
    pub high_value_threshold: i128,
    /// Seconds a high-value transaction must wait after threshold is reached.
    pub timelock_seconds: u64,
    /// Seconds a proposal stays valid before expiring.
    pub expiry_seconds: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct TreasuryProposal {
    /// Sequential proposal identifier.
    pub id: u64,
    /// Token to transfer.
    pub token: Address,
    /// Recipient of the transfer.
    pub to: Address,
    /// Amount to transfer.
    pub amount: i128,
    /// Signer that submitted the proposal (implicitly approves it).
    pub proposer: Address,
    /// Ledger timestamp when the proposal was created.
    pub created_at: u64,
    /// Ledger timestamp after which the proposal can no longer be executed.
    pub expires_at: u64,
    /// Current number of approvals (including the proposer's implicit approval).
    pub approval_count: u32,
    /// Ledger timestamp when the threshold was first reached (0 = not reached yet).
    pub threshold_reached_at: u64,
    /// Earliest timestamp at which execution is permitted (threshold_reached_at + timelock for high-value).
    pub earliest_execution_at: u64,
    pub is_executed: bool,
    pub is_cancelled: bool,
}

// ============================================================================
// Storage keys
// ============================================================================

#[contracttype]
pub enum DataKey {
    Config,
    ProposalCount,
    Proposal(u64),
    Approval(u64, Address),
}

// ============================================================================
// Contract interface
// ============================================================================

#[contractclient(name = "TreasuryWalletClient")]
pub trait TreasuryWallet {
    fn initialize(
        env: Env,
        owner: Address,
        signers: Vec<Address>,
        required_signatures: u32,
        high_value_threshold: i128,
        timelock_seconds: u64,
        expiry_seconds: u64,
    );
    fn add_signer(env: Env, caller: Address, signer: Address);
    fn remove_signer(env: Env, caller: Address, signer: Address);
    fn update_config(
        env: Env,
        caller: Address,
        new_required_signatures: u32,
        new_high_value_threshold: i128,
        new_timelock_seconds: u64,
        new_expiry_seconds: u64,
    );
    fn submit_transaction(
        env: Env,
        proposer: Address,
        token: Address,
        to: Address,
        amount: i128,
    ) -> u64;
    fn approve_transaction(env: Env, proposal_id: u64, approver: Address);
    fn revoke_approval(env: Env, proposal_id: u64, revoker: Address);
    fn execute_transaction(env: Env, proposal_id: u64);
    fn cancel_transaction(env: Env, proposal_id: u64, caller: Address);
    fn get_config(env: Env) -> TreasuryConfig;
    fn get_proposal(env: Env, proposal_id: u64) -> TreasuryProposal;
    fn get_proposal_count(env: Env) -> u64;
    fn has_approved(env: Env, proposal_id: u64, signer: Address) -> bool;
    fn get_approval_count(env: Env, proposal_id: u64) -> u32;
    fn is_signer(env: Env, signer: Address) -> bool;
}

// ============================================================================
// Implementation
// ============================================================================

#[contract]
pub struct TreasuryWalletContract;

#[contractimpl]
impl TreasuryWallet for TreasuryWalletContract {
    /// Initialize the multi-signature treasury wallet.
    ///
    /// # Arguments
    /// * `owner` - Address that can manage signers and thresholds.
    /// * `signers` - Initial set of signers (2-7).
    /// * `required_signatures` - Approvals required to execute (M-of-N).
    /// * `high_value_threshold` - Amounts at or above this are time-locked.
    /// * `timelock_seconds` - Delay applied to high-value transactions after threshold is reached.
    /// * `expiry_seconds` - Proposal validity window.
    fn initialize(
        env: Env,
        owner: Address,
        signers: Vec<Address>,
        required_signatures: u32,
        high_value_threshold: i128,
        timelock_seconds: u64,
        expiry_seconds: u64,
    ) {
        if env.storage().instance().has(&DataKey::Config) {
            panic_with_error!(&env, TreasuryError::AlreadyInitialized);
        }
        validate_signers(&env, &signers, required_signatures);
        if high_value_threshold < 0 || expiry_seconds == 0 {
            panic_with_error!(&env, TreasuryError::InvalidThreshold);
        }

        let config = TreasuryConfig {
            owner: owner.clone(),
            signers,
            required_signatures,
            high_value_threshold,
            timelock_seconds,
            expiry_seconds,
        };
        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&DataKey::ProposalCount, &0u64);

        Initialized {
            owner,
            required_signatures,
            high_value_threshold,
            timelock_seconds,
            expiry_seconds,
        }
        .publish(&env);
    }

    /// Add a new signer to the wallet (owner only).
    fn add_signer(env: Env, caller: Address, signer: Address) {
        let mut config = get_config(&env);
        caller.require_auth();
        if caller != config.owner {
            panic_with_error!(&env, TreasuryError::NotAuthorized);
        }

        if config.signers.len() >= MAX_SIGNERS {
            panic_with_error!(&env, TreasuryError::InvalidSignerCount);
        }
        if config.signers.contains(&signer) {
            panic_with_error!(&env, TreasuryError::DuplicateSigner);
        }

        config.signers.push_back(signer.clone());
        env.storage().instance().set(&DataKey::Config, &config);
        SignerAdded { signer }.publish(&env);
    }

    /// Remove a signer from the wallet (owner only).
    ///
    /// The removal is rejected if it would leave the wallet with fewer than
    /// `MIN_SIGNERS` signers or if the configured threshold could no longer be
    /// met by the remaining signers.
    fn remove_signer(env: Env, caller: Address, signer: Address) {
        let mut config = get_config(&env);
        caller.require_auth();
        if caller != config.owner {
            panic_with_error!(&env, TreasuryError::NotAuthorized);
        }

        if !config.signers.contains(&signer) {
            panic_with_error!(&env, TreasuryError::SignerNotFound);
        }
        let remaining = config.signers.len() - 1;
        if remaining < MIN_SIGNERS || remaining < config.required_signatures {
            panic_with_error!(&env, TreasuryError::CannotRemoveSigner);
        }

        let mut new_signers: Vec<Address> = Vec::new(&env);
        for s in config.signers.iter() {
            if s != signer {
                new_signers.push_back(s);
            }
        }
        config.signers = new_signers;
        env.storage().instance().set(&DataKey::Config, &config);
        SignerRemoved { signer }.publish(&env);
    }

    /// Update threshold, high-value bound, timelock, and expiry (owner only).
    fn update_config(
        env: Env,
        caller: Address,
        new_required_signatures: u32,
        new_high_value_threshold: i128,
        new_timelock_seconds: u64,
        new_expiry_seconds: u64,
    ) {
        let mut config = get_config(&env);
        caller.require_auth();
        if caller != config.owner {
            panic_with_error!(&env, TreasuryError::NotAuthorized);
        }

        validate_signers(&env, &config.signers, new_required_signatures);
        if new_high_value_threshold < 0 || new_expiry_seconds == 0 {
            panic_with_error!(&env, TreasuryError::InvalidThreshold);
        }

        config.required_signatures = new_required_signatures;
        config.high_value_threshold = new_high_value_threshold;
        config.timelock_seconds = new_timelock_seconds;
        config.expiry_seconds = new_expiry_seconds;
        env.storage().instance().set(&DataKey::Config, &config);

        ConfigUpdated {
            required_signatures: new_required_signatures,
            high_value_threshold: new_high_value_threshold,
            timelock_seconds: new_timelock_seconds,
            expiry_seconds: new_expiry_seconds,
        }
        .publish(&env);
    }

    /// Submit a transfer proposal. The caller must be a signer and becomes the
    /// implicit first approver. Returns the new proposal id.
    fn submit_transaction(
        env: Env,
        proposer: Address,
        token: Address,
        to: Address,
        amount: i128,
    ) -> u64 {
        let config = get_config(&env);
        if amount <= 0 {
            panic_with_error!(&env, TreasuryError::ZeroAmount);
        }
        if token == to {
            panic_with_error!(&env, TreasuryError::InvalidToken);
        }
        proposer.require_auth();
        if !config.signers.contains(&proposer) {
            panic_with_error!(&env, TreasuryError::NotAuthorized);
        }

        let proposal_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0);
        let now = env.ledger().timestamp();

        let proposal = TreasuryProposal {
            id: proposal_id,
            token: token.clone(),
            to: to.clone(),
            amount,
            proposer: proposer.clone(),
            created_at: now,
            expires_at: now + config.expiry_seconds,
            approval_count: 1, // proposer implicitly approves
            threshold_reached_at: 0,
            earliest_execution_at: 0,
            is_executed: false,
            is_cancelled: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage()
            .persistent()
            .set(&DataKey::Approval(proposal_id, proposer.clone()), &true);
        env.storage()
            .instance()
            .set(&DataKey::ProposalCount, &(proposal_id + 1));

        TransactionProposed {
            proposal_id,
            token,
            to,
            amount,
            proposer,
        }
        .publish(&env);
        proposal_id
    }

    /// Approve a pending transaction. Only signers may approve, once each.
    fn approve_transaction(env: Env, proposal_id: u64, approver: Address) {
        let config = get_config(&env);
        let mut proposal = get_proposal(&env, proposal_id);

        if proposal.is_executed {
            panic_with_error!(&env, TreasuryError::AlreadyExecuted);
        }
        if proposal.is_cancelled {
            panic_with_error!(&env, TreasuryError::AlreadyCancelled);
        }
        if env.ledger().timestamp() > proposal.expires_at {
            panic_with_error!(&env, TreasuryError::ProposalExpired);
        }

        approver.require_auth();
        if !config.signers.contains(&approver) {
            panic_with_error!(&env, TreasuryError::NotAuthorized);
        }

        let approval_key = DataKey::Approval(proposal_id, approver.clone());
        if env.storage().persistent().has(&approval_key) {
            panic_with_error!(&env, TreasuryError::AlreadyApproved);
        }
        env.storage().persistent().set(&approval_key, &true);
        proposal.approval_count += 1;

        // Arm the timelock the first time the threshold is reached.
        if proposal.threshold_reached_at == 0
            && proposal.approval_count >= config.required_signatures
        {
            let now = env.ledger().timestamp();
            proposal.threshold_reached_at = now;
            proposal.earliest_execution_at = if proposal.amount >= config.high_value_threshold {
                now + config.timelock_seconds
            } else {
                now
            };
        }

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        TransactionApproved {
            proposal_id,
            approver,
            approval_count: proposal.approval_count,
        }
        .publish(&env);
    }

    /// Revoke a previously given approval. If the approval count drops below the
    /// threshold, the timelock is disarmed and must be re-armed by reaching the
    /// threshold again.
    fn revoke_approval(env: Env, proposal_id: u64, revoker: Address) {
        let config = get_config(&env);
        let mut proposal = get_proposal(&env, proposal_id);

        if proposal.is_executed {
            panic_with_error!(&env, TreasuryError::AlreadyExecuted);
        }
        if proposal.is_cancelled {
            panic_with_error!(&env, TreasuryError::AlreadyCancelled);
        }

        revoker.require_auth();
        if !config.signers.contains(&revoker) {
            panic_with_error!(&env, TreasuryError::NotAuthorized);
        }

        let approval_key = DataKey::Approval(proposal_id, revoker.clone());
        if !env.storage().persistent().has(&approval_key) {
            panic_with_error!(&env, TreasuryError::NotApprovedBySigner);
        }
        env.storage().persistent().remove(&approval_key);
        proposal.approval_count = proposal.approval_count.saturating_sub(1);

        if proposal.approval_count < config.required_signatures {
            proposal.threshold_reached_at = 0;
            proposal.earliest_execution_at = 0;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        ApprovalRevoked {
            proposal_id,
            revoker,
            approval_count: proposal.approval_count,
        }
        .publish(&env);
    }

    /// Execute an approved transaction. Requires the signature threshold to be
    /// met and, for high-value amounts, the timelock to have elapsed. Anyone may
    /// trigger execution once the conditions are satisfied.
    fn execute_transaction(env: Env, proposal_id: u64) {
        let config = get_config(&env);
        let mut proposal = get_proposal(&env, proposal_id);

        if proposal.is_executed {
            panic_with_error!(&env, TreasuryError::AlreadyExecuted);
        }
        if proposal.is_cancelled {
            panic_with_error!(&env, TreasuryError::AlreadyCancelled);
        }
        if env.ledger().timestamp() > proposal.expires_at {
            panic_with_error!(&env, TreasuryError::ProposalExpired);
        }
        if proposal.approval_count < config.required_signatures {
            panic_with_error!(&env, TreasuryError::InsufficientApprovals);
        }
        if proposal.threshold_reached_at == 0
            || env.ledger().timestamp() < proposal.earliest_execution_at
        {
            panic_with_error!(&env, TreasuryError::TimelockNotElapsed);
        }

        let client = token::Client::new(&env, &proposal.token);
        let balance = client.balance(&env.current_contract_address());
        if balance < proposal.amount {
            panic_with_error!(&env, TreasuryError::InsufficientBalance);
        }
        client.transfer(
            &env.current_contract_address(),
            &proposal.to,
            &proposal.amount,
        );

        proposal.is_executed = true;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        TransactionExecuted {
            proposal_id,
            token: proposal.token,
            to: proposal.to,
            amount: proposal.amount,
        }
        .publish(&env);
    }

    /// Cancel a pending proposal. Only the owner or the proposer may cancel.
    fn cancel_transaction(env: Env, proposal_id: u64, caller: Address) {
        let config = get_config(&env);
        let mut proposal = get_proposal(&env, proposal_id);

        if proposal.is_executed {
            panic_with_error!(&env, TreasuryError::AlreadyExecuted);
        }
        if proposal.is_cancelled {
            panic_with_error!(&env, TreasuryError::AlreadyCancelled);
        }

        caller.require_auth();
        if caller != config.owner && caller != proposal.proposer {
            panic_with_error!(&env, TreasuryError::NotAuthorized);
        }

        proposal.is_cancelled = true;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        TransactionCancelled { proposal_id }.publish(&env);
    }

    fn get_config(env: Env) -> TreasuryConfig {
        get_config(&env)
    }

    fn get_proposal(env: Env, proposal_id: u64) -> TreasuryProposal {
        get_proposal(&env, proposal_id)
    }

    fn get_proposal_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0)
    }

    fn has_approved(env: Env, proposal_id: u64, signer: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Approval(proposal_id, signer))
    }

    fn get_approval_count(env: Env, proposal_id: u64) -> u32 {
        get_proposal(&env, proposal_id).approval_count
    }

    fn is_signer(env: Env, signer: Address) -> bool {
        get_config(&env).signers.contains(&signer)
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn get_config(env: &Env) -> TreasuryConfig {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .unwrap_or_else(|| panic_with_error!(env, TreasuryError::NotInitialized))
}

fn get_proposal(env: &Env, proposal_id: u64) -> TreasuryProposal {
    env.storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .unwrap_or_else(|| panic_with_error!(env, TreasuryError::ProposalNotFound))
}

/// Minimum required signatures for a wallet with `n` signers: a strict majority,
/// but never less than 2 (so a single key can never move treasury funds).
fn minimum_required(n: u32) -> u32 {
    let majority = n / 2 + 1;
    if majority > 2 {
        majority
    } else {
        2
    }
}

fn validate_signers(env: &Env, signers: &Vec<Address>, required: u32) {
    let n = signers.len();
    if !(MIN_SIGNERS..=MAX_SIGNERS).contains(&n) {
        panic_with_error!(env, TreasuryError::InvalidSignerCount);
    }
    let mut seen: Vec<Address> = Vec::new(env);
    for s in signers.iter() {
        if seen.contains(&s) {
            panic_with_error!(env, TreasuryError::DuplicateSigner);
        }
        seen.push_back(s);
    }
    if required < minimum_required(n) || required > n {
        panic_with_error!(env, TreasuryError::InvalidThreshold);
    }
}

#[cfg(test)]
mod test;
