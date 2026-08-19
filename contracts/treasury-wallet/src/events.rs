//! Typed contract events published by the treasury wallet.
//!
//! Defined with the modern `#[contractevent]` macro so the events are included
//! in the contract's interface specification and usable by indexers, SDKs, and
//! generated clients. Static topics are kept short (≤ 10 bytes) so they remain
//! inline "short" symbols.

use soroban_sdk::{contractevent, Address};

/// Emitted when the wallet is initialized.
#[contractevent(topics = ["init"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Initialized {
    /// Address authorized to manage signers and thresholds.
    pub owner: Address,
    /// Number of approvals required to execute a transaction.
    pub required_signatures: u32,
    /// Amounts at or above this value are time-locked.
    pub high_value_threshold: i128,
    /// Delay applied to high-value transactions.
    pub timelock_seconds: u64,
    /// Proposal validity window.
    pub expiry_seconds: u64,
}

/// Emitted when a signer is added.
#[contractevent(topics = ["signer_add"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignerAdded {
    /// The signer that was added (dynamic topic).
    #[topic]
    pub signer: Address,
}

/// Emitted when a signer is removed.
#[contractevent(topics = ["signer_rm"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignerRemoved {
    /// The signer that was removed (dynamic topic).
    #[topic]
    pub signer: Address,
}

/// Emitted when the wallet configuration is updated.
#[contractevent(topics = ["cfg_upd"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigUpdated {
    /// New number of approvals required to execute a transaction.
    pub required_signatures: u32,
    /// New high-value threshold.
    pub high_value_threshold: i128,
    /// New timelock duration.
    pub timelock_seconds: u64,
    /// New proposal expiry window.
    pub expiry_seconds: u64,
}

/// Emitted when a transaction is proposed.
#[contractevent(topics = ["propose"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionProposed {
    /// The proposal id (dynamic topic).
    #[topic]
    pub proposal_id: u64,
    /// Token to transfer.
    pub token: Address,
    /// Recipient of the transfer.
    pub to: Address,
    /// Amount to transfer.
    pub amount: i128,
    /// Signer that submitted the proposal.
    pub proposer: Address,
}

/// Emitted when a transaction is approved.
#[contractevent(topics = ["approve"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionApproved {
    /// The proposal id (dynamic topic).
    #[topic]
    pub proposal_id: u64,
    /// The signer that approved (dynamic topic).
    #[topic]
    pub approver: Address,
    /// Current approval count.
    pub approval_count: u32,
}

/// Emitted when an approval is revoked.
#[contractevent(topics = ["revoke"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalRevoked {
    /// The proposal id (dynamic topic).
    #[topic]
    pub proposal_id: u64,
    /// The signer that revoked (dynamic topic).
    #[topic]
    pub revoker: Address,
    /// Current approval count after the revocation.
    pub approval_count: u32,
}

/// Emitted when an approved transaction is executed.
#[contractevent(topics = ["execute"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionExecuted {
    /// The proposal id (dynamic topic).
    #[topic]
    pub proposal_id: u64,
    /// Token that was transferred.
    pub token: Address,
    /// Recipient that received the funds.
    pub to: Address,
    /// Amount transferred.
    pub amount: i128,
}

/// Emitted when a pending transaction is cancelled.
#[contractevent(topics = ["cancel"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionCancelled {
    /// The proposal id (dynamic topic).
    #[topic]
    pub proposal_id: u64,
}
