use soroban_sdk::{contracttype, Address};

/// Settlement proposal structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct SettlementProposal {
    /// Unique identifier for the settlement proposal
    pub proposal_id: u64,
    /// Address of the payer
    pub payer: Address,
    /// Address of the payee
    pub payee: Address,
    /// Amount to be settled
    pub amount: i128,
    /// Exchange rate at the time of proposal
    pub rate: i128,
    /// Timestamp when the proposal was submitted (epoch seconds)
    pub submission_timestamp: u64,
    /// Deadline by which the settlement must be finalized (epoch seconds)
    pub settlement_deadline: u64,
    /// Whether the proposal has been finalized
    pub finalized: bool,
    /// Whether resources are locked
    pub resources_locked: bool,
}

impl SettlementProposal {
    pub fn new(
        proposal_id: u64,
        payer: Address,
        payee: Address,
        amount: i128,
        rate: i128,
        submission_timestamp: u64,
        settlement_window: u64,
    ) -> Self {
        Self {
            proposal_id,
            payer,
            payee,
            amount,
            rate,
            submission_timestamp,
            settlement_deadline: submission_timestamp.saturating_add(settlement_window),
            finalized: false,
            resources_locked: false,
        }
    }
}
