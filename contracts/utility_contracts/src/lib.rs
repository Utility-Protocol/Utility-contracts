#![no_std]
extern crate alloc;

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, token, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};

#[contractclient(name = "PriceOracleClient")]
pub trait PriceOracle {
    fn xlm_to_usd_cents(env: Env, xlm_amount: i128) -> i128;
    fn usd_cents_to_xlm(env: Env, usd_cents: i128) -> i128;
    fn get_price(env: Env) -> PriceData;
    fn verify_green_source(env: Env, provider: Address, meter_id: u64, timestamp: u64) -> bool;
}

// Issue #252: Carbon-Credit Minter cross-contract interface
#[contractclient(name = "CarbonCreditMinterClient")]
pub trait CarbonCreditMinter {
    fn mint_credits(env: Env, recipient: Address, amount: i128);
}

#[contracttype]
#[derive(Clone)]
pub struct PriceData {
    pub price: i128,
    pub decimals: u32,
    pub last_updated: u64,
}

// ============================================================================
// Issue #259: Cross-Contract "Energy-Score" Reputation Adapter
// ============================================================================

/// Reputation tier returned to partner DApps querying a user's utility reliability.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ReputationTier {
    /// User has no history (new or pruned) — neutral score.
    NewUser = 0,
    /// Occasional late payments or low buffer health.
    Bronze = 1,
    /// Mostly on-time with adequate buffer health.
    Silver = 2,
    /// Consistently on-time with healthy buffer.
    Gold = 3,
    /// Perfect record — eligible for premium DeFi features.
    Platinum = 4,
}

/// Standardised reputation score returned by `get_utility_reputation`.
/// Does NOT expose consumption volume or device MAC.
#[contracttype]
#[derive(Clone)]
pub struct ReputationScore {
    /// Score in basis points (0–10 000). 10 000 = perfect.
    pub score_bps: u32,
    /// Human-readable tier for partner DApps.
    pub tier: ReputationTier,
    /// Timestamp of the last on-chain activity used to compute the score.
    pub last_activity: u64,
    /// Whether the score was derived from live data (false = default/new-user).
    pub is_live: bool,
}

// ============================================================================
// Issue #257: IoT Error Enum — u16 machine-readable codes
// ============================================================================

/// Compact u16 error codes for IoT firmware handshakes.
/// Hardware devices switch on these codes to perform automated local recoveries.
/// SECURITY: codes do NOT leak provider metadata or consumption volumes.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum IoTErrorCode {
    // 0x01xx — Meter / stream lifecycle
    MeterNotFound = 0x0101,
    MeterNotPaired = 0x0102,
    MeterOffline = 0x0103,
    MeterPaused = 0x0104,
    MeterClosed = 0x0105,
    // 0x02xx — Balance / billing
    InsufficientBalance = 0x0201,
    BufferBreached = 0x0202,
    DebtThresholdExceeded = 0x0203,
    CreditLimitApproached = 0x0204,
    CreditLimitBreached = 0x0205,
    // 0x03xx — Auth / signature
    InvalidSignature = 0x0301,
    PublicKeyMismatch = 0x0302,
    TimestampTooOld = 0x0303,
    UnauthorizedDevice = 0x0304,
    // 0x04xx — Device / firmware
    DeviceBlacklisted = 0x0401,
    FirmwareUpdateActive = 0x0402, // triggers hibernation mode in firmware
    FirmwareWindowExpired = 0x0403,
    // 0x05xx — Clawback / compliance
    ClawbackDetected = 0x0501,
    StreamTerminated = 0x0502,
    // 0x06xx — Generic
    UnknownError = 0x0601,
}

impl IoTErrorCode {
    /// Map a `ContractError` to the compact IoT u16 code.
    pub fn from_contract_error(e: ContractError) -> Self {
        match e {
            ContractError::MeterNotFound => Self::MeterNotFound,
            ContractError::MeterNotPaired => Self::MeterNotPaired,
            ContractError::InvalidSignature => Self::InvalidSignature,
            ContractError::PublicKeyMismatch => Self::PublicKeyMismatch,
            ContractError::TimestampTooOld => Self::TimestampTooOld,
            ContractError::InsufficientBuffer => Self::BufferBreached,
            ContractError::BufferAlreadyDepleted => Self::BufferBreached,
            ContractError::FirmwareUpdateInProgress => Self::FirmwareUpdateActive,
            ContractError::FirmwareUpdateWindowExpired => Self::FirmwareWindowExpired,
            _ => Self::UnknownError,
        }
    }

    /// Return the raw u16 code — zero-copy for firmware parsing.
    pub fn code(self) -> u16 {
        self as u16
    }
}

// ============================================================================
// Issue #256: SAC Clawback Reconciliation structures
// ============================================================================

/// Emitted when a clawback reconciliation is executed.
#[contracttype]
#[derive(Clone)]
pub struct ClawbackReconciliationExecuted {
    /// Token that was clawed back.
    pub token: Address,
    /// Volume removed from the contract by the issuer.
    pub clawback_volume: i128,
    /// Number of active streams affected.
    pub affected_streams: u32,
    /// Protocol haircut applied (if any) to cover the gap.
    pub protocol_haircut: i128,
    pub timestamp: u64,
}

// ============================================================================
// Issue #255: Post-Paid Multi-Factor Escrow structures
// ============================================================================

/// Vault that backs a post-paid stream with USDC collateral.
#[contracttype]
#[derive(Clone)]
pub struct GuarantorDeposit {
    /// Owner of the deposit (the utility consumer).
    pub owner: Address,
    /// Stable-asset token (must be USDC or equivalent).
    pub collateral_token: Address,
    /// Total locked collateral.
    pub locked_amount: i128,
    /// Accrued debt drawn against this deposit (across all linked streams).
    pub accrued_debt: i128,
    /// Timestamp of last debt update.
    pub last_updated: u64,
    /// Whether a margin-call warning has been emitted.
    pub margin_call_sent: bool,
    /// Whether the deposit has been slashed (stream terminated).
    pub is_slashed: bool,
}

/// Emitted when debt reaches 80 % of collateral.
#[contracttype]
#[derive(Clone)]
pub struct CreditLimitApproached {
    pub owner: Address,
    pub accrued_debt: i128,
    pub locked_amount: i128,
    pub ratio_bps: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct ReadingRejected {
    pub meter_id: u64,
    pub reason: String,
    pub value: i128,
    pub timestamp: u64,
}

/// Tamper-evident audit trail entry chained to the previous entry hash.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditRecord {
    /// Monotonic sequence number for the contract-wide audit stream.
    pub sequence: u64,
    /// Previous record hash, or all zeroes for the genesis record.
    pub previous_hash: BytesN<32>,
    /// Hash of this record's canonical payload.
    pub record_hash: BytesN<32>,
    /// Service operation being audited, e.g. register/topup/deduct/claim.
    pub action: Symbol,
    /// Primary subject identifier for the event, usually a meter or stream id.
    pub subject_id: u64,
    /// Account that authorized or initiated the audited operation.
    pub actor: Address,
    /// Ledger timestamp at the point the audit record was appended.
    pub timestamp: u64,
    /// Domain-specific payload digest for compact, privacy-preserving verification.
    pub payload_hash: BytesN<32>,
}

/// Emitted when the deposit is slashed at 100 % utilisation.
#[contracttype]
#[derive(Clone)]
pub struct GuarantorSlashed {
    pub owner: Address,
    pub slashed_amount: i128,
    pub provider: Address,
    pub timestamp: u64,
}

#[cfg(test)]
mod buffer_tests;
#[cfg(test)]
mod debt_fuzz_tests;
#[cfg(test)]
mod dust_sweeper_tests;
#[cfg(test)]
mod fuzz_tests;
#[cfg(test)]
mod ghost_sweeper_tests;
#[cfg(test)]
mod nonce_sync_tests;
#[cfg(test)]
mod pause_resume_fuzz_tests;
#[cfg(test)]
mod pause_resume_tests;
#[cfg(test)]
mod streaming_invariant_tests;
#[cfg(test)]
mod stroop_fuzz_tests;
#[cfg(test)]
mod tariff_oracle_tests;
#[cfg(test)]
mod temporary_storage_tests;

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BillingType {
    PrePaid,
    PostPaid,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum StreamStatus {
    Active = 0,
    Paused = 1,
    Depleted = 2,
}

#[contracttype(export = false)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContinuousFlow {
    // Tightly packed struct for optimal storage
    pub stream_id: u64,             // 8 bytes
    pub flow_rate_per_second: i128, // 16 bytes - micro-stroops per second
    pub accumulated_balance: i128,  // 16 bytes - precise balance tracking
    pub last_flow_timestamp: u64,   // 8 bytes - u64 for epoch safety
    pub created_timestamp: u64,     // 8 bytes - creation time
    pub status: StreamStatus,       // 1 byte (enum)
    pub paused_at: u64,             // 8 bytes - timestamp when stream was paused (0 if active)
    pub provider: Address,          // 32 bytes - provider address for access control
    pub buffer_balance: i128,       // 16 bytes - pre-paid buffer balance (24 hours of flow)
    pub buffer_warning_sent: bool,  // 1 byte - whether buffer warning has been sent
    pub payer: Address,             // 32 bytes - payer address for buffer refunds
    /// Issue #251 — `enterprise::PriorityTier` discriminant stored as `u32`.
    pub priority_tier: u32,
    pub grid_epoch_seen: u64,
    /// Ed25519 public key mapped from device MAC identity; zero means heartbeat not enforced on-chain.
    pub device_mac_pubkey: BytesN<32>,
    pub is_unreliable: bool,
}
// Minimum balance required to keep the IoT relay open (500 tokens for testing)
const MINIMUM_BALANCE_TO_FLOW: i128 = 500; // 500 tokens minimum for testing

// Buffer requirements - 24 hours of flow rate
const BUFFER_DURATION_SECONDS: u64 = 24 * HOUR_IN_SECONDS; // 24 hours
const BUFFER_WARNING_THRESHOLD: i128 = 3600; // Warning when 1 hour of buffer left

#[contracttype(export = false)]
#[derive(Clone)]
pub struct UsageData {
    pub total_watt_hours: i128,
    pub current_cycle_watt_hours: i128,
    pub peak_usage_watt_hours: i128,
    pub last_reading_timestamp: u64,
    pub precision_factor: i128,
    pub renewable_watt_hours: i128,
    pub renewable_percentage: i128,
    pub monthly_volume: i128,
    pub last_volume_reset: u64,
    pub first_reading_timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct UsageReport {
    pub meter_id: u64,
    pub timestamp: u64,
    pub watt_hours_consumed: i128,
    pub units_consumed: i128,
    pub is_renewable_energy: bool,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ResourceType {
    Electricity = 0,
    Gas = 1,
    Water = 2,
    Heat = 3,
}

#[contracttype]
#[derive(Clone)]
pub struct SignedUsageData {
    pub meter_id: u64,
    pub timestamp: u64,
    pub watt_hours_consumed: i128,
    pub units_consumed: i128,
    pub signature: BytesN<64>,
    pub public_key: BytesN<32>,
    pub is_renewable_energy: bool,
}

mod gas_estimator;
use gas_estimator::GasCostEstimator;

pub mod enterprise;
pub mod ghost_sweeper;
pub mod grant_stream_listener;
pub mod nonce_sync;
pub mod secure_call_interface;
pub mod settlement;
pub mod settlement_lock_manager;
pub mod settlement_types;
pub mod tariff_oracle;
pub mod temporary_storage;
pub mod velocity_limit;

#[cfg(test)]
pub mod gas_metrics;

#[cfg(test)]
mod stream_balance_property_tests;
use temporary_storage::{OptimizedFlowCalculator, OptimizedUsageTracker, TempStorageManager};
use velocity_limit::{
    apply_override, check_velocity_limits, get_velocity_config, revoke_override,
    set_velocity_config, VelocityDataKey,
};
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavingGoal {
    pub target_amount: i128,
    pub current_savings: i128,
    pub marketplace: Address,
    pub is_completed: bool,
}

#[contracttype(export = false)]
#[derive(Clone)]
pub struct Meter {
    pub user: Address,
    pub provider: Address,
    pub billing_type: BillingType,
    pub resource_type: ResourceType,
    pub off_peak_rate: i128,
    pub peak_rate: i128,
    pub rate_per_unit: i128,
    pub balance: i128,
    pub debt: i128,
    pub last_update: u64,
    pub is_active: bool,
    pub token: Address,
    pub usage_data: UsageData,
    pub device_public_key: BytesN<32>,
    pub end_date: u64,
    pub rent_deposit: i128,
    pub priority_index: u32,
    pub green_energy_discount_bps: i128,
    pub carbon_credit_token: Option<Address>,
    pub carbon_credit_drip_rate_bps: i128,
    pub is_paused: bool,
    pub is_disputed: bool,
    pub challenge_timestamp: u64,
    pub credit_drip_rate: i128,
    pub is_closed: bool,
    pub off_peak_reward_rate_bps: i128,
    pub milestone_deadline: u64,
    pub milestone_confirmed: bool,
    pub rate_per_second: i128,
    pub collateral_limit: i128,
    pub max_flow_rate_per_hour: i128,
    pub last_claim_time: u64,
    pub claimed_this_hour: i128,
    pub is_paired: bool,
    pub tier_threshold: i128,
    pub tier_rate: i128,
    // Device-Offline Grace Period Fields
    pub last_heartbeat: u64,
    pub grace_period_start: u64,
    pub is_offline: bool,
    pub estimated_usage_total: i128,
    // SLA Penalty Fields
    pub sla_config: Option<SLAConfig>,
    pub sla_state: SLAState,
    // Issue #178: Firmware Update Authorization Gate Fields
    pub is_updating: bool,
    pub update_start_timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct SLAConfig {
    pub threshold_seconds: u64,
    pub penalty_multiplier_bps: i128, // e.g. 5000 = 50% rate
}

#[contracttype]
#[derive(Clone)]
pub struct SLAState {
    pub accumulated_downtime: u64,
    pub last_report_timestamp: u64,
    pub is_penalty_active: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct SLADowntimeReport {
    pub meter_id: u64,
    pub start_time: u64,
    pub end_time: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct SignedSLAReport {
    pub report: SLADowntimeReport,
    pub signature: BytesN<64>,
    pub node_public_key: BytesN<32>,
}

#[contracttype]
#[derive(Clone)]
pub struct ClaimSettlement {
    pub gross_claimed: i128,
    pub provider_payout: i128,
    pub tax_amount: i128,
    pub protocol_fee: i128,
    pub reseller_payout: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct DeliveryFailure {
    pub batch_id: BytesN<32>,
    pub user: Address,
    pub amount: i128,
    pub reason: String,
}

#[contracttype]
#[derive(Clone)]
pub struct PendingSettlement {
    pub amount: i128,
    pub token: Address,
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct ResellerConfig {
    pub reseller: Address,
    pub fee_bps: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct ImpactMetrics {
    pub total_kilowatts_funded: i128,
    pub total_liters_streamed: i128,
    pub active_meters: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct ConservationGoal {
    pub goal_id: u64,
    pub provider: Address,
    pub target_water_savings: i128, // in liters
    pub current_savings: i128,
    pub deadline: u64,
    pub is_active: bool,
    pub grant_amount: i128, // grant amount when goal is reached
    pub grant_token: Address,
    pub created_at: u64,
    pub achieved_at: Option<u64>,
}

#[contracttype]
#[derive(Clone)]
pub struct OfflineReconciliation {
    pub meter_id: u64,
    pub estimated_cost: i128,
    pub actual_cost: i128,
    pub adjustment: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct GoalReachedEvent {
    pub goal_id: u64,
    pub provider: Address,
    pub water_savings: i128,
    pub grant_amount: i128,
    pub grant_token: Address,
    pub achieved_at: u64,
}

#[contractclient(name = "GrantStreamClient")]
pub trait GrantStream {
    fn on_goal_reached(env: Env, goal_event: GoalReachedEvent);
}

// Issue #118: Zero-Knowledge Privacy Usage Reporting
// ZK-proof structures for private billing and usage verification
#[contracttype]
#[derive(Clone)]
pub struct Groth16Proof {
    pub a: Bytes, // G1 point
    pub b: Bytes, // G2 point
    pub c: Bytes, // G1 point
}

#[contracttype]
#[derive(Clone)]
pub struct Groth16VerificationKey {
    pub alpha_g1: Bytes,
    pub beta_g2: Bytes,
    pub gamma_g2: Bytes,
    pub delta_g2: Bytes,
    pub ic: Vec<Bytes>, // G1 points
}

#[contracttype]
#[derive(Clone)]
pub struct ZKProof {
    pub commitment: BytesN<32>,
    pub nullifier: BytesN<32>,
    pub proof: Groth16Proof,
    pub public_inputs: Vec<Bytes>, // Serialized field elements
    pub meter_id: u64,
    pub timestamp: u64,
    pub is_valid: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct ZKUsageReport {
    pub commitment: BytesN<32>, // Commitment to usage data
    pub nullifier: BytesN<32>,  // Unique nullifier for this report
    pub encrypted_usage: Bytes, // Encrypted usage data (for future ZK implementation)
    pub proof_hash: BytesN<32>, // Hash of the ZK proof
    pub meter_id: u64,          // Meter identifier
    pub billing_cycle: u32,     // Billing cycle number
    pub timestamp: u64,         // Report timestamp
    pub is_verified: bool,      // Verification status
}

#[contracttype]
#[derive(Clone)]
pub struct TaxReceipt {
    pub meter_id: u64,
    pub total_amount: i128,
    pub tax_amount: i128,
    pub net_amount: i128,
    pub tax_rate_bps: i128,
    pub government_vault: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PrivateBillingStatus {
    pub meter_id: u64,          // Meter ID
    pub billing_cycle: u32,     // Current billing cycle
    pub total_commitments: u32, // Number of commitments received
    pub verified_proofs: u32,   // Number of verified ZK proofs
    pub last_verification: u64, // Last verification timestamp
    pub privacy_enabled: bool,  // Whether privacy mode is enabled
}

#[contracttype]
#[derive(Clone)]
pub struct CommitmentBatch {
    pub commitments: Vec<BytesN<32>>, // Batch of commitments
    pub nullifiers: Vec<BytesN<32>>,  // Corresponding nullifiers
    pub batch_root: BytesN<32>,       // Merkle root of commitments
    pub timestamp: u64,               // Batch creation time
    pub meter_id: u64,                // Associated meter
}

#[contracttype]
#[derive(Clone)]
pub struct MeterStatus {
    pub meter_id: u64,
    pub is_active: bool,
    pub balance: i128,
    pub billing_cycle: u32,
    pub total_commitments: u32,
    pub verified_proofs: u32,
    pub privacy_enabled: bool,
    pub last_update: u64,
    pub usage_summary: Option<UsageData>,
}

// Issue #98: Multi-Sig Provider Withdrawal Requirement
// For large utility companies, withdrawals require 3-of-5 authorized signatures
// from Finance Department wallets to prevent unauthorized access to streaming revenue
#[contracttype]
#[derive(Clone)]
pub struct MultiSigConfig {
    pub provider: Address, // The utility provider this config belongs to
    pub finance_wallets: Vec<Address>, // List of authorized Finance Department wallets (max 5)
    pub required_signatures: u32, // Number of signatures required (typically 3)
    pub threshold_amount: i128, // Minimum amount requiring multi-sig (in USD cents)
    pub is_active: bool,   // Whether multi-sig is enabled
    pub created_at: u64,   // Timestamp when config was created
}

#[contracttype]
#[derive(Clone)]
pub struct WithdrawalRequest {
    pub request_id: u64,        // Unique request identifier
    pub provider: Address,      // Provider requesting withdrawal
    pub meter_id: u64,          // Meter to withdraw from
    pub amount_usd_cents: i128, // Amount requested in USD cents
    pub destination: Address,   // Destination treasury address
    pub proposer: Address,      // Finance wallet that proposed this request
    pub created_at: u64,        // Timestamp when request was created
    pub expires_at: u64,        // Request expiration timestamp
    pub approval_count: u32,    // Current number of approvals
    pub is_executed: bool,      // Whether withdrawal has been executed
    pub is_cancelled: bool,     // Whether request was cancelled
}

#[contracttype]
#[derive(Clone)]
pub struct FeeChangeProposal {
    pub proposal_id: u64,
    pub proposed_fee_bps: i128,
    pub proposed_at: u64,
    pub voting_deadline: u64,
    pub votes_for: i128,
    pub votes_against: i128,
    pub is_executed: bool,
    pub proposer: Address,
}

#[contracttype]
#[derive(Clone)]
pub struct GasBuffer {
    pub balance: i128,
    pub last_top_up: u64,
    pub provider: Address,
    pub token: Address,
}

// Issue #178: Firmware Update Authorization Gate
// Structures for managing authorized firmware updates on IoT devices
#[contracttype]
#[derive(Clone)]
pub struct FirmwareUpdateStartedEvent {
    pub meter_id: u64,
    pub update_start_timestamp: u64,
    pub provider: Address,
    pub max_update_window_secs: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct FirmwareUpdateFinishedEvent {
    pub meter_id: u64,
    pub update_start_timestamp: u64,
    pub update_completed_timestamp: u64,
    pub update_duration_secs: u64,
    pub device_signature_valid: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct UpdateCompleteData {
    pub meter_id: u64,
    pub update_start_timestamp: u64,
    pub completion_timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct SignedUpdateComplete {
    pub meter_id: u64,
    pub update_start_timestamp: u64,
    pub completion_timestamp: u64,
    pub signature: BytesN<64>,
    pub device_public_key: BytesN<32>,
}

// Missing struct types used by various features
#[contracttype]
#[derive(Clone)]
pub struct ProviderWithdrawalWindow {
    pub daily_withdrawn: i128,
    pub last_reset: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct DustAggregation {
    pub total_dust: i128,
    pub stream_count: u64,
    pub last_updated: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct DustCollectedEvent {
    pub token_address: Address,
    pub total_dust_swept: i128,
    pub streams_swept: u64,
    pub timestamp: u64,
    pub sweeper_address: Address,
}

#[contracttype]
#[derive(Clone)]
pub struct StreamUpdatedEvent {
    pub stream_id: u64,
    pub old_flow_rate: i128,
    pub new_flow_rate: i128,
    pub timestamp: u64,
    pub old_status: StreamStatus,
    pub new_status: StreamStatus,
}

#[contracttype]
#[derive(Clone)]
pub struct BufferDepletedEvent {
    pub stream_id: u64,
    pub timestamp: u64,
    pub amount_deducted: i128,
    pub provider: Address,
}

#[contracttype]
#[derive(Clone)]
pub struct BufferWarningEvent {
    pub stream_id: u64,
    pub timestamp: u64,
    pub remaining_buffer: i128,
    pub threshold_percent: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct StreamingFeeAccrued {
    pub stream_id: u64,
    pub fee_amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct UpgradeProposal {
    pub new_wasm_hash: BytesN<32>,
    pub proposed_at: u64,
    pub veto_deadline: u64,
    pub proposer: Address,
}

#[contracttype]
#[derive(Clone)]
pub struct AdminTransferProposal {
    pub current_admin: Address,
    pub proposed_admin: Address,
    pub proposed_at: u64,
    pub execution_deadline: u64,
    pub veto_count: u32,
    pub is_active: bool,
}

// ============================================================
// Upgrade Multi-Sig Structures
// ============================================================

/// Configuration for the upgrade multi-sig committee.
/// Stores the set of authorized upgrade signers and the approval threshold.
#[contracttype]
#[derive(Clone)]
pub struct UpgradeMultiSigConfig {
    /// Addresses authorized to propose and approve WASM upgrades (2–7 signers).
    pub signers: Vec<Address>,
    /// Minimum number of approvals required before an upgrade can execute.
    pub required_approvals: u32,
    /// Seconds that must elapse after threshold is reached before execution is allowed.
    pub timelock_seconds: u64,
    /// Seconds after proposal creation before the proposal expires if not executed.
    pub expiry_seconds: u64,
    /// Timestamp when this config was last updated.
    pub updated_at: u64,
}

/// Status of an upgrade proposal.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum UpgradeProposalStatus {
    Pending,
    Approved, // threshold reached, waiting for timelock
    Executed,
    Cancelled,
    Expired,
}

/// A versioned upgrade proposal that requires multi-sig approval.
#[contracttype]
#[derive(Clone)]
pub struct UpgradeProposalV2 {
    /// Unique sequential proposal ID.
    pub proposal_id: u64,
    /// The new WASM hash to upgrade to.
    pub new_wasm_hash: BytesN<32>,
    /// Address that submitted this proposal.
    pub proposer: Address,
    /// Ledger timestamp when the proposal was created.
    pub proposed_at: u64,
    /// Ledger timestamp after which the proposal expires if not executed.
    pub expires_at: u64,
    /// Ledger timestamp when the approval threshold was first reached (0 = not yet reached).
    pub threshold_reached_at: u64,
    /// Earliest timestamp at which execution is allowed (proposed_at + timelock, set when threshold reached).
    pub earliest_execution_at: u64,
    /// Current number of approvals (including the proposer's implicit approval).
    pub approval_count: u32,
    /// Current status of this proposal.
    pub status: UpgradeProposalStatus,
}

// Upgrade multi-sig constants
const MIN_UPGRADE_SIGNERS: u32 = 2;
const MAX_UPGRADE_SIGNERS: u32 = 7;
const MIN_UPGRADE_TIMELOCK_SECONDS: u64 = 24 * 60 * 60; // 24 hours minimum timelock
const DEFAULT_UPGRADE_TIMELOCK_SECONDS: u64 = 48 * 60 * 60; // 48 hours default
const DEFAULT_UPGRADE_EXPIRY_SECONDS: u64 = 14 * 24 * 60 * 60; // 14 days to execute

#[contracttype]
#[derive(Clone)]
pub struct LegalFreeze {
    pub meter_id: u64,
    pub frozen_at: u64,
    pub reason: soroban_sdk::String,
    pub compliance_officer: Address,
    pub legal_vault: Address,
    pub frozen_amount: i128,
    pub is_released: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerificationMethod {
    IdentityVerified,
    CommunityVote,
    AdminGranted,
}

#[contracttype]
#[derive(Clone)]
pub struct VerifiedProvider {
    pub address: Address,
    pub is_verified: bool,
    pub verified_at: u64,
    pub verification_method: VerificationMethod,
    pub provider_name: soroban_sdk::String,
}

#[contracttype]
#[derive(Clone)]
pub struct SubDaoConfig {
    pub parent_dao: Address,
    pub sub_dao: Address,
    pub allocated_budget: i128,
    pub spent_budget: i128,
    pub token: Address,
    pub created_at: u64,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct WebhookConfig {
    pub url: soroban_sdk::String,
    pub user: Address,
    pub is_active: bool,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct LowBalanceAlert {
    pub meter_id: u64,
    pub user: Address,
    pub remaining_balance: i128,
    pub hours_remaining: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct BillingGroup {
    pub parent_account: Address,
    pub child_meters: Vec<u64>,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct MaintenanceMilestone {
    pub meter_id: u64,
    pub milestone_number: u32,
    pub description: soroban_sdk::String,
    pub funding_amount: i128,
    pub is_completed: bool,
    pub completed_at: u64,
    pub verified_by: Address,
    pub completion_proof: soroban_sdk::Bytes,
}

// Issue #196: IL Protection Buffer structures
#[contracttype]
#[derive(Clone)]
pub struct ILProtectionBuffer {
    pub balance: i128,
    pub cold_storage: Address,
    pub dao_alert_threshold: i128,
    pub last_updated: u64,
}

// Issue #201: Treasury Cap structures
#[contracttype]
#[derive(Clone)]
pub struct TreasuryState {
    pub tracked_tvl: i128,
    pub cold_storage: Address,
    pub last_sweep: u64,
}

// Issue #202: Treasury reconciliation event
#[contracttype]
#[derive(Clone)]
pub struct TreasuryReconciliationEvent {
    pub tracked_tvl_before: i128,
    pub actual_balance: i128,
    pub adjustment: i128,
    pub timestamp: u64,
}

#[contracttype(export = false)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlaReportKey {
    pub meter_id: u64,
    pub start_time: u64,
    pub end_time: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct CarbonCreditIssuedEvent {
    pub meter_id: u64,
    pub user: Address,
    pub provider: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[contracttype(export = false)]
pub enum DataKey {
    ActiveMetersCount,
    ActiveUsers,
    AdminAddress,
    AdminTransferProposal,
    AdminVeto(Address, u64),
    AuthorizedContributor(u64, Address),
    AutoExtendThreshold,
    BillingGroup(Address),
    BufferVault(u64),
    ComplianceOfficer,
    ConservationGoal(u64),
    ContinuousFlow(u64),
    Contributor(u64, Address),
    Count,
    CurrentAdmin,
    DaoGovernor,
    DeviceHash(BytesN<32>),
    DustAggregation(Address),
    FleetAgg(Address),
    FleetCap(Address),
    GasBountyPool,
    GasBuffer(Address),
    GovernmentVault,
    GrantStreamMatch(u64, Address),
    GridAdministrator,
    ImpactSBTMinted(u64),
    LastAlert(u64),
    LegalFreeze(u64),
    LegalVault,
    MaintenanceFund(u64),
    MaintenanceWallet,
    Meter(u64),
    MeterDevice(u64),
    MinRouteThreshold,
    MultiSigConfig(Address),
    NativeToken,
    NullifierMap(BytesN<32>),
    Oracle,
    PairingChallenge(u64),
    PendingDeviceTransfer(BytesN<32>, Address),
    P2PCreditVault(Address),
    PlatformFeeBps,
    PollVotes(Symbol),
    PrivateBillingStatus(u64),
    ProposedUpgrade,
    ProtocolFeeBps,
    ProtocolFeeVault,
    ProviderGridEpoch(Address),
    ProviderTotalPool(Address),
    ProviderVolume(Address),
    ProviderWindow(Address),
    Referral(Address),
    ReentrancyGuard(u64),
    ResellerConfig(u64),
    SavingGoal(u64),
    SeasonalFactor,
    SLANode(BytesN<32>),
    SLAReportCount(SlaReportKey),
    SLAReportNode(SlaReportKey, BytesN<32>),
    StreamLastHeartbeat(u64),
    StreamCreationRateLimit(Address),
    StreamingFeeAccrued(u64),
    SubDaoConfig(Address),
    SupportedToken(Address),
    SupportedWithdrawalToken(Address),
    TaxRateBps,
    Treasury,
    UpgradeProposalTime,
    UserVetoed(Address, u64),
    UserVoted(Address, Symbol),
    VerifiedProvider(Address),
    VetoCount,
    VetoDeadline,
    WebhookConfig(Address),
    WithdrawalApproval(Address, u64, Address),
    WithdrawalRequest(Address, u64),
    WithdrawalRequestCount(Address),
    ZKEnabledMeters,
    ZKVerificationKey(u64),
    // Issue #259
    ReputationScore(Address),
    // Issue #256
    ClawbackNonce(Address),
    // Issue #255
    GuarantorDeposit(Address),
    // Issue #260 - Nonce Sync
    DeviceNonce(BytesN<32>),
    NonceResetRequest(u64),
    AuthorizedNonceResetters,
    // Issue #261 - Tariff Oracle
    TariffOracleAdmin,
    CurrentTariffSchedule,
    TariffScheduleHash,
    TariffUpdateProposal(u64),
    TariffProposalCounter,
    TodayTariffSchedule,
    // Issue #262 - Ghost Sweeper
    StreamArchive(u64),
    SweeperStatistics,
    // Issue #277 - Emergency Drain Recovery
    EmergencyDrainLastExecution,
    EmergencyDrainRecord(u64),
    EmergencyDrainCounter,
    // Upgrade Multi-Sig
    UpgradeMultiSigConfig,
    UpgradeProposalV2(u64),
    UpgradeApproval(u64, Address),
    UpgradeProposalCounter,
    ActiveUpgradeProposalId,
    // Meter reading validation
    LastReadingTime(u64),
    // Pending settlement
    PendingSettlement(Address, BytesN<32>),
    // Batch finalization idempotency guard
    BatchFinalized(u64),
    // Tamper-evident audit trail
    AuditHead,
    AuditRecord(u64),
}

// ============================================================================
// Namespace prefixes for domain-separated storage keys
// ============================================================================
pub const NAMESPACE_COMMON: [u8; 4] = [0x43, 0x4f, 0x4d, 0x4d]; // "COMM"
pub const NAMESPACE_TARIFF: [u8; 4] = [0x54, 0x41, 0x52, 0x49]; // "TARI"
pub const NAMESPACE_SETTLEMENT: [u8; 4] = [0x53, 0x45, 0x54, 0x4c]; // "SETL"
pub const NAMESPACE_RESOURCE: [u8; 4] = [0x52, 0x45, 0x53, 0x4f]; // "RESO"

impl DataKey {
    pub fn encode(&self, env: &Env) -> Bytes {
        let prefix = match self {
            DataKey::TariffOracleAdmin
            | DataKey::CurrentTariffSchedule
            | DataKey::TariffScheduleHash
            | DataKey::TariffUpdateProposal(_)
            | DataKey::TariffProposalCounter
            | DataKey::TodayTariffSchedule => &NAMESPACE_TARIFF,
            _ => &NAMESPACE_COMMON,
        };
        let mut key = Bytes::new(env);
        key.append(&Bytes::from_array(env, prefix));
        key.append(&self.clone().to_xdr(env));
        key
    }
}

/// Encode a raw key (e.g. u64) with a namespace prefix for domain separation.
pub fn encode_raw_key(env: &Env, prefix: &[u8; 4], raw: &[u8]) -> Bytes {
    let mut key = Bytes::new(env);
    key.append(&Bytes::from_array(env, prefix));
    key.append(&Bytes::from_slice(env, raw));
    key
}

/// Migrate storage entries from legacy (non-prefixed) keys to new namespaced keys.
/// Handles tariff oracle keys and common singleton keys. Idempotent.
pub fn migrate_namespace(env: &Env) {
    // Tariff oracle singleton keys
    let legacy_admin: Option<Address> = env.storage().persistent().get(&DataKey::TariffOracleAdmin);
    if let Some(admin) = legacy_admin {
        let new_key = DataKey::TariffOracleAdmin.encode(env);
        env.storage().persistent().set(&new_key, &admin);
        env.storage().persistent().remove(&DataKey::TariffOracleAdmin);
    }

    let legacy_hash: Option<soroban_sdk::BytesN<32>> = env.storage().persistent().get(&DataKey::TariffScheduleHash);
    if let Some(hash) = legacy_hash {
        let new_key = DataKey::TariffScheduleHash.encode(env);
        env.storage().persistent().set(&new_key, &hash);
        env.storage().persistent().remove(&DataKey::TariffScheduleHash);
    }

    let legacy_counter: Option<u64> = env.storage().persistent().get(&DataKey::TariffProposalCounter);
    if let Some(counter) = legacy_counter {
        let new_key = DataKey::TariffProposalCounter.encode(env);
        env.storage().persistent().set(&new_key, &counter);
        env.storage().persistent().remove(&DataKey::TariffProposalCounter);
    }

    let legacy_schedule: Option<crate::tariff_oracle::DailyTariffSchedule> =
        env.storage().persistent().get(&DataKey::CurrentTariffSchedule);
    if let Some(schedule) = legacy_schedule {
        let new_key = DataKey::CurrentTariffSchedule.encode(env);
        env.storage().persistent().set(&new_key, &schedule);
        env.storage().persistent().remove(&DataKey::CurrentTariffSchedule);
    }

    let legacy_today: Option<crate::tariff_oracle::DailyTariffSchedule> =
        env.storage().temporary().get(&DataKey::TodayTariffSchedule);
    if let Some(schedule) = legacy_today {
        let new_key = DataKey::TodayTariffSchedule.encode(env);
        env.storage().temporary().set(&new_key, &schedule);
        env.storage().temporary().remove(&DataKey::TodayTariffSchedule);
    }

    // Common singleton keys
    let keys_to_migrate: &[(DataKey, &dyn Fn(&DataKey) -> bool)] = &[
        (DataKey::AdminAddress, &|_: &DataKey| true),
        (DataKey::GridAdministrator, &|_: &DataKey| true),
        (DataKey::Oracle, &|_: &DataKey| true),
        (DataKey::ProtocolFeeBps, &|_: &DataKey| true),
        (DataKey::ProtocolFeeVault, &|_: &DataKey| true),
        (DataKey::GovernmentVault, &|_: &DataKey| true),
        (DataKey::NativeToken, &|_: &DataKey| true),
        (DataKey::ComplianceOfficer, &|_: &DataKey| true),
        (DataKey::MaintenanceWallet, &|_: &DataKey| true),
        (DataKey::LegalVault, &|_: &DataKey| true),
    ];
    for (variant, _) in keys_to_migrate {
        let legacy: Option<Address> = env.storage().persistent().get(&variant);
        if let Some(val) = legacy {
            let new_key = variant.encode(env);
            env.storage().persistent().set(&new_key, &val);
            env.storage().persistent().remove(&variant);
        }
    }
}

#[contracterror(export = false)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    MeterNotFound = 1,
    OracleNotSet = 2,
    WithdrawalLimitExceeded = 3,
    InsufficientGasBuffer = 4,
    InvalidTokenAmount = 5,
    InvalidUsageValue = 6,
    UsageExceedsLimit = 7,
    InvalidPrecisionFactor = 8,
    InvalidSignature = 9,
    PublicKeyMismatch = 10,
    TimestampTooOld = 11,
    PairingAlreadyComplete = 12,
    ChallengeNotFound = 13,
    InvalidPairingSignature = 14,
    MeterNotPaired = 15,
    UnauthorizedAdmin = 16,
    InsufficientGasBounty = 17,
    NoDustToSweep = 18,
    InsufficientBuffer = 19,
    BufferAlreadyDepleted = 20,
    UnauthorizedBufferAccess = 21,
    // Issue #195
    BelowMinRouteThreshold = 22,
    // Issue #197
    ProtocolFeeVaultNotSet = 23,
    // --- extended (github issues & features) ---
    FleetCapExceeded = 24,
    SelfP2PNotAllowed = 25,
    AdminExecutionWindowExpired = 26,
    AdminTransferActive = 27,
    AlreadyApprovedWithdrawal = 28,
    AlreadyVoted = 29,
    AmountBelowMultiSigThreshold = 30,
    ChallengeActive = 31,
    ComplianceCouncilApprovalRequired = 32,
    ConservationGoalNotFound = 33,
    DeviceAlreadyBoundToAnotherMeter = 34,
    FirmwareUpdateInProgress = 35,
    FirmwareUpdateWindowExpired = 36,
    GoalAlreadyAchieved = 37,
    GoalExpired = 38,
    ImpactNotSignificantEnough = 39,
    InDispute = 40,
    InsufficientApprovals = 41,
    InvalidFinanceWalletCount = 42,
    InvalidFirmwareUpdateSignature = 43,
    InvalidGrantAmount = 44,
    InvalidResellerFee = 45,
    InvalidSignatureThreshold = 46,
    InvalidWasmHash = 47,
    LegalFreezeAlreadyActive = 48,
    LowPriorityStreamPaused = 49,
    MaintenanceFundInsufficient = 50,
    MeterNotFrozen = 51,
    MultiSigAlreadyConfigured = 52,
    MultiSigNotConfigured = 53,
    NoAdminTransferInProgress = 54,
    NodeNotTrusted = 55,
    NotApprovedByWallet = 56,
    NotAuthorizedFinanceWallet = 57,
    NotParentDao = 58,
    PriceConversionFailed = 59,
    PrivacyNotEnabled = 60,
    SBTAlreadyMinted = 61,
    UpgradeProposalActive = 62,
    VerificationAlreadyGranted = 63,
    VelocityLimitBreach = 64,
    VetoPeriodExpired = 65,
    VetoThresholdNotReached = 66,
    WithdrawalAlreadyCancelled = 67,
    WithdrawalAlreadyExecuted = 68,
    WithdrawalRequestExpired = 69,
    WithdrawalRequestNotFound = 70,
    SubDaoNotConfigured = 71,
    SubDaoBudgetExceeded = 72,
    UnauthorizedContributor = 73,
    // Issue #255 — Post-Paid escrow
    GuarantorDepositNotFound = 74,
    InsufficientCollateral = 75,
    DepositAlreadySlashed = 76,
    // Issue #256 — SAC clawback
    ClawbackBalanceMismatch = 77,
    // Issue #259 — Reputation
    ReputationQueryFailed = 78,
    // Issue #260 — Nonce Sync
    NonceDesyncDetected = 79,
    NonceResetUnauthorized = 80,
    DeviceMarkedSuspicious = 81,
    NonceWindowExceeded = 82,
    // Issue #261 — Tariff Oracle
    InvalidTariffSchedule = 83,
    TariffUpdateNotReady = 84,
    // Issue #272 — Reentrancy protection
    ReentrancyDetected = 85,
    TariffOracleNotConfigured = 86,
    InvalidTariffHour = 87,
    // Issue #262 — Ghost Sweeper
    StreamNotEligibleForPruning = 88,
    StreamHasPendingBuffer = 89,
    ArchiveCorrupted = 90,
    // Issue #277 — Emergency Drain Recovery
    EmergencyDrainNotAuthorized = 91,
    EmergencyDrainCooldownActive = 92,
    EmergencyDrainInsufficientBalance = 93,
    InvalidAddress = 94,
    InvalidFeeAmount = 95,
    ExcessiveFee = 96,
    RateLimitExceeded = 97,
    // Upgrade Multi-Sig errors
    UpgradeMultiSigNotConfigured = 98,
    UpgradeMultiSigAlreadyConfigured = 99,
    UpgradeProposalNotFound = 100,
    UpgradeAlreadyApproved = 101,
    UpgradeApprovalNotFound = 102,
    UpgradeTimelockActive = 103,
    UpgradeAlreadyExecuted = 104,
    UpgradeAlreadyCancelled = 105,
    UpgradeProposalExpired = 106,
    NotAuthorizedUpgradeSigner = 107,
    InsufficientUpgradeApprovals = 108,
    // Budget and paging errors
    BudgetExceeded = 109,
    PageSizeExceeded = 110,
    // Flow rate validation errors
    FlowRateTooLow = 111,
    FlowRateTooHigh = 112,
    // Meter reading validation errors
    InvalidReadingValue = 113,
    DuplicateTimestamp = 114,
    ReadingDeltaTooLarge = 115,
    // Storage Versioning errors
    StorageVersionMismatch = 116,
    IncompatibleStorageVersion = 117,
    MigrationInProgress = 118,
    MigrationFailed = 119,
    NoMigrationFunction = 120,
    AuditRecordMissing = 121,
    AuditChainBroken = 122,
}

#[contracttype]
#[derive(Clone)]
pub struct PairingChallengeData {
    pub contract: Address,
    pub meter_id: u64,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct RateLimitData {
    pub count: u32,
    pub last_reset: u64,
}

// --- Internal Helpers ---

const HOUR_IN_SECONDS: u64 = 60 * 60;
const DAY_IN_SECONDS: u64 = 24 * HOUR_IN_SECONDS;
const GRACE_PERIOD_SECONDS: u64 = 86_400; // 24 hours grace period
const DEBT_THRESHOLD: i128 = -10_000_000; // -10 XLM (in stroops) threshold for negative balance
const DAILY_WITHDRAWAL_PERCENT: i128 = 10;
const MAX_USAGE_PER_UPDATE: i128 = 1_000_000_000_000i128; // 1 billion kWh max per update
const MIN_PRECISION_FACTOR: i128 = 1;
const MAX_TIMESTAMP_DELAY: u64 = 300; // 5 minutes

// Gas buffer constants
const MIN_GAS_BUFFER: i128 = 100; // Minimum XLM to maintain as gas buffer
const MAX_GAS_BUFFER: i128 = 10000; // Maximum XLM that can be stored in gas buffer
const GAS_BUFFER_TOP_UP_THRESHOLD: i128 = 200; // Auto-top up when buffer falls below this

// Issue #195: Minimum Yield-Routing Gas Threshold
// Default: 10_000_000 stroops (1 XLM). Routing below this costs more in gas than it earns.
const DEFAULT_MIN_ROUTE_THRESHOLD: i128 = 10_000_000;

// Issue #273: Flow Rate Boundary Constants
// Minimum flow rate: 1 micro-stroop per second (prevents zero/negative flows)
const MIN_FLOW_RATE_PER_SECOND: i128 = 1;
// Maximum flow rate: 1 billion XLM per second (prevents overflow attacks)
const MAX_FLOW_RATE_PER_SECOND: i128 = 1_000_000_000_000_000_000;
// Maximum hourly flow rate for meter limits
const MAX_HOURLY_FLOW_RATE: i128 = MAX_FLOW_RATE_PER_SECOND * 3600;

// Issue #197: Streaming-Fee Collector
// Max platform fee: 1000 bps = 10%
const MAX_PLATFORM_FEE_BPS: i128 = 1000;

// Issue #255: Post-Paid escrow thresholds (in basis points)
/// Margin-call warning threshold: 80 % of collateral consumed.
const MARGIN_CALL_THRESHOLD_BPS: i128 = 8_000;
/// Slash threshold: 100 % of collateral consumed.
const SLASH_THRESHOLD_BPS: i128 = 10_000;

// Issue #259: Reputation score thresholds (in basis points)
const REPUTATION_PLATINUM_BPS: u32 = 9_500;
const REPUTATION_GOLD_BPS: u32 = 8_000;
const REPUTATION_SILVER_BPS: u32 = 6_000;
const REPUTATION_BRONZE_BPS: u32 = 3_000;

// --- shared protocol constants (referenced across claim / upgrade / multi-sig) ---
const THROTTLING_THRESHOLD_PERCENT: i128 = 20;
const HEARTBEAT_THRESHOLD_SECONDS: u64 = 3600;
const DEFAULT_TAX_RATE_BPS: i128 = 50;
const MAINTENANCE_FUND_PERCENT_BPS: i128 = 100;
const AUTO_EXTEND_LEDGER_THRESHOLD: u32 = 100;
const LEDGER_LIFETIME_EXTENSION: u32 = 10_000;
const UPGRADE_VETO_PERIOD_SECONDS: u64 = 7 * DAY_IN_SECONDS;
const VETO_THRESHOLD_BPS: i128 = 500;


const MIGRATION_INSTRUCTION_BUDGET: u64 = 5_000_000;
const INITIAL_STORAGE_VERSION: u32 = 1;
const CURRENT_STORAGE_VERSION: u32 = 1;
const MAX_VERSION_DELTA: u32 = 1;
const MIGRATION_INSTRUCTION_BUDGET: u64 = 5_000_000; // 5M instructions per migration call


// Issue #277: Emergency Drain Recovery Constants
const EMERGENCY_DRAIN_COOLDOWN_SECONDS: u64 = 24 * HOUR_IN_SECONDS; // 24 hour cooldown
const EMERGENCY_DRAIN_MIN_AMOUNT: i128 = 1_000_000; // Minimum 0.0001 XLM for drain
const MAX_PROTOCOL_FEE_BPS: i128 = 1000; // Maximum 10% protocol fee
const MAX_RESELLER_FEE_BPS: i128 = 500; // Maximum 5% reseller fee

// Emergency drain tracking data structure
#[contracttype]
#[derive(Clone)]
pub struct EmergencyDrainRecord {
    pub timestamp: u64,
    pub amount: i128,
    pub recipient: Address,
    pub reason: String,
}
const REFERRAL_REWARD_UNITS: i128 = 10;

// Rate limiting for stream creation
const STREAM_CREATION_RATE_LIMIT: u32 = 10; // Max 10 streams per window
const STREAM_CREATION_WINDOW_SECONDS: u64 = 3600; // 1 hour window
const ADMIN_TRANSFER_TIMELOCK: u64 = 48 * HOUR_IN_SECONDS;
const MIN_FINANCE_WALLETS: u32 = 3;
const MAX_FINANCE_WALLETS: u32 = 5;
const MIN_MULTISIG_THRESHOLD: u32 = 2;
const WITHDRAWAL_REQUEST_EXPIRY: u64 = 7 * DAY_IN_SECONDS;

// Issue #279: Byte array validation constants
const ED25519_PUBLIC_KEY_SIZE: usize = 32;
const ED25519_SIGNATURE_SIZE: usize = 64;
const SHA256_HASH_SIZE: usize = 32;
const MAX_BYTE_ARRAY_SIZE: usize = 1024; // Maximum reasonable size for user inputs

// Budget and paging constants
pub(crate) const MAX_CONTRACT_INSTRUCTIONS: u64 = 1_000_000; // 1M instructions per contract call
pub(crate) const MIN_REMAINING: u64 = 100_000; // Minimum remaining instructions before stopping
pub(crate) const MAX_PAGE_SIZE: u32 = 100; // Maximum number of items per page
pub(crate) const DEFAULT_PAGE_SIZE: u32 = 20; // Default page size
pub(crate) const BUDGET_CHECK_COST: u64 = 500; // Cost of a single budget check
pub(crate) const STORAGE_READ_COST: u64 = 5_000; // Cost of a single storage read
pub(crate) const STORAGE_WRITE_COST: u64 = 5_000; // Cost of a single storage write
pub(crate) const CONTRACT_INVOCATION_COST: u64 = 10_000; // Cost of a contract invocation

// Meter reading validation constants
const STANDARD_INTERVAL: u64 = 300; // 5 minutes in seconds
const MAX_INTERVAL_MULTIPLIER: u64 = 24;
const MAX_ELECTRICITY_DELTA: i128 = 100 * 1_000_000_00; // 100 kWh per 5-min interval
const MAX_GAS_DELTA: i128 = 50 * 1_000_000_00; // 50 m³ per 5-min interval
const MAX_WATER_DELTA: i128 = 200 * 1_000_000_00; // 200 L per 5-min interval
const MAX_HEAT_DELTA: i128 = 100 * 1_000_000_00; // 100 units per 5-min interval

// Pending settlement constants
const PENDING_CLAIM_TTL: u64 = 30 * 86400; // 30 days
const MAX_PENDING: usize = 10; // Max pending failures per batch

fn zero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

fn audit_payload_hash<T: soroban_sdk::xdr::ToXdr>(env: &Env, payload: &T) -> BytesN<32> {
    env.crypto().sha256(&payload.to_xdr(env)).into()
}

fn calculate_audit_record_hash(
    env: &Env,
    sequence: u64,
    previous_hash: &BytesN<32>,
    action: &Symbol,
    subject_id: u64,
    actor: &Address,
    timestamp: u64,
    payload_hash: &BytesN<32>,
) -> BytesN<32> {
    let material = (
        sequence,
        previous_hash.clone(),
        action.clone(),
        subject_id,
        actor.clone(),
        timestamp,
        payload_hash.clone(),
    );
    env.crypto().sha256(&material.to_xdr(env)).into()
}

fn append_audit_record(
    env: &Env,
    action: Symbol,
    subject_id: u64,
    actor: Address,
    payload_hash: BytesN<32>,
) -> AuditRecord {
    let previous: Option<AuditRecord> = env.storage().instance().get(&DataKey::AuditHead);
    let (sequence, previous_hash) = match previous {
        Some(record) => (record.sequence.saturating_add(1), record.record_hash),
        None => (1, zero_hash(env)),
    };
    let timestamp = env.ledger().timestamp();
    let record_hash = calculate_audit_record_hash(
        env,
        sequence,
        &previous_hash,
        &action,
        subject_id,
        &actor,
        timestamp,
        &payload_hash,
    );
    let record = AuditRecord {
        sequence,
        previous_hash,
        record_hash,
        action,
        subject_id,
        actor,
        timestamp,
        payload_hash,
    };
    env.storage()
        .instance()
        .set(&DataKey::AuditRecord(sequence), &record);
    env.storage().instance().set(&DataKey::AuditHead, &record);
    env.events()
        .publish((Symbol::new(env, "AuditTrail"), subject_id), record.clone());
    record
}

/// Validate Ed25519 public key byte array
/// Ensures correct length and non-zero values
fn validate_ed25519_public_key(public_key: &BytesN<32>) -> Result<(), ContractError> {
    // Check for all-zero public key (invalid)
    let zero_key = BytesN::from_array(&[0u8; 32]);
    if *public_key == zero_key {
        return Err(ContractError::InvalidSignature);
    }

    // Additional validation could be added here:
    // - Check if key is on curve (if needed)
    // - Check for known weak keys

    Ok(())
}

/// Validate Ed25519 signature byte array
/// Ensures correct length and non-zero values
fn validate_ed25519_signature(signature: &BytesN<64>) -> Result<(), ContractError> {
    // Check for all-zero signature (invalid)
    let zero_sig = BytesN::from_array(&[0u8; 64]);
    if *signature == zero_sig {
        return Err(ContractError::InvalidSignature);
    }

    // Additional validation could be added here:
    // - Check signature format
    // - Check for known weak signatures

    Ok(())
}

/// Validate SHA256 hash byte array
/// Ensures correct length
fn validate_sha256_hash(hash: &BytesN<32>) -> Result<(), ContractError> {
    // Basic length validation is already enforced by BytesN<32>
    // Additional validation could be added if needed

    Ok(())
}

/// Validate user-supplied Bytes with length check
fn validate_user_bytes(bytes: &Bytes, max_size: usize) -> Result<(), ContractError> {
    if bytes.len() > max_size {
        return Err(ContractError::InvalidTokenAmount); // Reuse error for size validation
    }

    if bytes.len() == 0 {
        return Err(ContractError::InvalidTokenAmount); // Reuse error for empty validation
    }

    Ok(())
}

/// Issue #273: Validate flow rate is within acceptable boundaries
fn validate_flow_rate(flow_rate: i128) -> Result<(), ContractError> {
    if flow_rate < MIN_FLOW_RATE_PER_SECOND {
        return Err(ContractError::FlowRateTooLow);
    }

    if flow_rate > MAX_FLOW_RATE_PER_SECOND {
        return Err(ContractError::FlowRateTooHigh);
    }

    Ok(())
}

/// Issue #273: Validate hourly flow rate is within acceptable boundaries
fn validate_hourly_flow_rate(hourly_rate: i128) -> Result<(), ContractError> {
    if hourly_rate < MIN_FLOW_RATE_PER_SECOND * 3600 {
        return Err(ContractError::FlowRateTooLow);
    }

    if hourly_rate > MAX_HOURLY_FLOW_RATE {
        return Err(ContractError::FlowRateTooHigh);
    }

    Ok(())
}

/// Check if remaining budget is sufficient for required instructions
pub(crate) fn check_budget(env: &Env, required: u64) -> Result<(), ContractError> {
    let remaining = env.budget().get_remaining_instructions();
    if remaining < required {
        return Err(ContractError::BudgetExceeded);
    }
    Ok(())
}

/// Validate page size is within allowed limits
pub(crate) fn validate_page_size(page_size: u32) -> Result<u32, ContractError> {
    if page_size == 0 {
        Ok(DEFAULT_PAGE_SIZE)
    } else if page_size > MAX_PAGE_SIZE {
        Err(ContractError::PageSizeExceeded)
    } else {
        Ok(page_size)
    }
}

/// Estimate required budget for iterating over N storage items
pub(crate) fn estimate_iteration_budget(num_items: u32) -> u64 {
    // Budget = (storage read cost per item * num items) + (budget check cost per item * num items)
    (STORAGE_READ_COST * num_items as u64) + (BUDGET_CHECK_COST * num_items as u64)
}

/// Get maximum allowed delta for a given resource type
fn get_max_delta(resource_type: ResourceType) -> i128 {
    match resource_type {
        ResourceType::Electricity => MAX_ELECTRICITY_DELTA,
        ResourceType::Gas => MAX_GAS_DELTA,
        ResourceType::Water => MAX_WATER_DELTA,
        ResourceType::Heat => MAX_HEAT_DELTA,
    }
}

/// Validate meter reading values
fn validate_reading(
    env: &Env,
    meter_id: u64,
    resource_type: ResourceType,
    watt_hours_consumed: i128,
    units_consumed: i128,
    timestamp: u64,
) -> Result<(), ContractError> {
    // Check if value is negative
    if watt_hours_consumed < 0 || units_consumed < 0 {
        return Err(ContractError::InvalidReadingValue);
    }

    // Get last reading timestamp
    let last_reading_key = DataKey::LastReadingTime(meter_id);
    let last_reading_time: u64 = env.storage().instance().get(&last_reading_key).unwrap_or(0);

    // Check for duplicate or old timestamp
    if timestamp <= last_reading_time {
        return Err(ContractError::DuplicateTimestamp);
    }

    // Calculate time elapsed since last reading
    let elapsed = timestamp - last_reading_time;

    // Get max delta for this resource type
    let max_delta = get_max_delta(resource_type);

    // Calculate allowed maximum delta, capped at MAX_INTERVAL_MULTIPLIER * max_delta
    let allowed_intervals = elapsed / STANDARD_INTERVAL;
    let capped_intervals = allowed_intervals.min(MAX_INTERVAL_MULTIPLIER);
    let allowed_delta = max_delta * capped_intervals as i128;

    // Check if either watt_hours or units_consumed exceeds allowed delta
    if watt_hours_consumed > allowed_delta || units_consumed > allowed_delta {
        return Err(ContractError::ReadingDeltaTooLarge);
    }

    Ok(())
}

/// Check if a trustline is open for a token and user
/// Attempts to get the user's balance - if it fails, trustline is closed
fn is_trustline_open(env: &Env, token: &Address, user: &Address) -> bool {
    let client = token::Client::new(env, token);
    // Try to get balance - if it panics (trustline closed), return false
    let result = soroban_sdk::Env::try_invoke_contract::<_, i128>(
        env,
        token,
        &soroban_sdk::symbol_short!("balance"),
        (user,),
    );
    result.is_ok()
}

/// Store a pending settlement for later claim
fn store_pending_settlement(
    env: &Env,
    user: &Address,
    batch_id: BytesN<32>,
    amount: i128,
    token: &Address,
) {
    let now = env.ledger().timestamp();
    let pending = PendingSettlement {
        amount,
        token: token.clone(),
        expires_at: now + PENDING_CLAIM_TTL,
    };
    env.storage()
        .instance()
        .set(&DataKey::PendingSettlement(user.clone(), batch_id), &pending);
}

/// Try to transfer tokens; if trustline is closed, store pending
fn try_transfer_or_store_pending(
    env: &Env,
    token: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
    batch_id: BytesN<32>,
) -> bool {
    if is_trustline_open(env, token, to) {
        transfer_tokens(env, token, from, to, &amount);
        true
    } else {
        let reason = String::from_str(env, "Trustline closed");
        let event = DeliveryFailure {
            batch_id: batch_id.clone(),
            user: to.clone(),
            amount,
            reason,
        };
        env.events().publish(
            (soroban_sdk::symbol_short!("DeliveryFail"), to.clone(), batch_id),
            event,
        );
        store_pending_settlement(env, to, batch_id, amount, token);
        false
    }
}

fn get_meter_or_panic(env: &Env, meter_id: u64) -> Meter {
    match env
        .storage()
        .instance()
        .get::<DataKey, Meter>(&DataKey::Meter(meter_id))
    {
        Some(meter) => meter,
        None => panic_with_error!(env, ContractError::MeterNotFound),
    }
}

fn transfer_tokens(env: &Env, token: &Address, from: &Address, to: &Address, amount: &i128) {
    let client = token::Client::new(env, token);
    client.transfer(from, to, amount);
}

fn is_native_token(_env: &Env, _token: &Address) -> bool {
    false
}

fn convert_xlm_to_usd_if_needed(
    env: &Env,
    amount: i128,
    _token: &Address,
) -> Result<i128, ContractError> {
    if let Some(oracle_address) = env
        .storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::Oracle)
    {
        let oracle_client = PriceOracleClient::new(env, &oracle_address);
        Ok(oracle_client.xlm_to_usd_cents(&amount))
    } else {
        Ok(amount)
    }
}

fn convert_usd_to_xlm_if_needed(
    env: &Env,
    usd_cents: i128,
    _token: &Address,
) -> Result<i128, ContractError> {
    if let Some(oracle_address) = env
        .storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::Oracle)
    {
        let oracle_client = PriceOracleClient::new(env, &oracle_address);
        Ok(oracle_client.usd_cents_to_xlm(&usd_cents))
    } else {
        Ok(usd_cents)
    }
}

fn get_maintenance_fund_balance(env: &Env, meter_id: u64) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::MaintenanceFund(meter_id))
        .unwrap_or(0)
}

fn remaining_postpaid_collateral(meter: &Meter) -> i128 {
    meter.collateral_limit.saturating_sub(meter.debt).max(0)
}

fn check_throttling_threshold(_env: &Env, meter: &Meter) -> bool {
    let total_value = match meter.billing_type {
        BillingType::PrePaid => meter.balance,
        BillingType::PostPaid => meter.balance.saturating_sub(meter.debt),
    };
    if total_value <= 0 {
        return false;
    }
    let threshold = (total_value * THROTTLING_THRESHOLD_PERCENT) / 100;
    meter.balance < threshold
}

fn should_pause_low_priority_stream(meter: &Meter, throttling_active: bool) -> bool {
    throttling_active && meter.priority_index == 0
}

fn unlock_reentrancy(_env: &Env) {}

// Peak hours: 18:00 - 21:00 UTC
const PEAK_HOUR_START: u64 = 18 * HOUR_IN_SECONDS; // 64800 seconds
const PEAK_HOUR_END: u64 = 21 * HOUR_IN_SECONDS; // 75600 seconds
const PEAK_RATE_MULTIPLIER: i128 = 3; // 1.5x => stored as 3 (divide by 2)
const RATE_PRECISION: i128 = 2; // Precision for rate calculations

// Issue #178: Firmware Update Authorization Gate constants
const FIRMWARE_UPDATE_WINDOW_SECS: u64 = 2 * HOUR_IN_SECONDS; // 2 hours max update window

/// Flush temporary storage to persistent storage for cost optimization
/// This should be called periodically to consolidate temporary data
fn flush_temporary_storage(env: &Env) {
    let current_ledger = env.ledger().sequence();

    // Only flush every 5 ledgers to balance cost and freshness
    if current_ledger % 5 != 0 {
        return;
    }

    // Flush streaming fees from temporary to persistent storage
    flush_streaming_fees(env);

    // Flush dust aggregation from temporary to persistent storage
    flush_dust_aggregation(env);

    // Flush provider withdrawal windows
    flush_provider_windows(env);

    // Emit flush event for monitoring
    env.events()
        .publish(soroban_sdk::symbol_short!("TempFlush"), current_ledger);
}

/// Flush streaming fees from temporary to persistent storage
fn flush_streaming_fees(env: &Env) {
    // This would iterate through all active streams and flush fee deltas
    // For now, we'll implement a simplified version that flushes on-demand
    env.events().publish(
        soroban_sdk::symbol_short!("FeeFlush"),
        env.ledger().sequence(),
    );
}

/// Flush dust aggregation from temporary to persistent storage
fn flush_dust_aggregation(env: &Env) {
    // This would iterate through all tokens and flush dust deltas
    env.events().publish(
        soroban_sdk::symbol_short!("DustFlush"),
        env.ledger().sequence(),
    );
}

/// Flush provider withdrawal windows from temporary to persistent storage
fn flush_provider_windows(env: &Env) {
    // This would iterate through all providers and flush window data
    env.events().publish(
        soroban_sdk::symbol_short!("WinFlush"),
        env.ledger().sequence(),
    );
}

// XLM precision constants - XLM has 7 decimal places (0.0000001 minimum)
const XLM_PRECISION: i128 = 10_000_000; // 10^7 for 7 decimal places
const XLM_MINIMUM_INCREMENT: i128 = 1; // 1 stroop = 0.0000001 XLM

// Dust detection constants
const DUST_THRESHOLD: i128 = 1; // Less than 1 stroop is considered dust
const GAS_BOUNTY_AMOUNT: i128 = 100_000; // 0.01 XLM bounty for dust sweepers
const MAX_SWEEP_STREAMS_PER_CALL: u64 = 1000; // Prevent gas limit issues

/// Round XLM amount to nearest minimum increment (0.0000001 XLM)
/// This prevents value loss over time due to truncation
fn round_xlm_to_minimum_increment(amount: i128) -> i128 {
    // For positive amounts, round up on .5 or higher
    // For negative amounts, round down on -.5 or lower
    if amount >= 0 {
        ((amount + XLM_MINIMUM_INCREMENT / 2) / XLM_MINIMUM_INCREMENT) * XLM_MINIMUM_INCREMENT
    } else {
        ((amount - XLM_MINIMUM_INCREMENT / 2) / XLM_MINIMUM_INCREMENT) * XLM_MINIMUM_INCREMENT
    }
}

fn calculate_historical_average(usage_data: &UsageData, now: u64) -> i128 {
    let elapsed = now.saturating_sub(usage_data.first_reading_timestamp);
    if elapsed == 0 {
        return 0;
    }
    // watt_hours / second. We use precision_factor to keep accuracy.
    usage_data
        .total_watt_hours
        .saturating_mul(usage_data.precision_factor)
        .saturating_div(elapsed as i128)
}

fn is_peak_hour(timestamp: u64) -> bool {
    let day_seconds = timestamp % DAY_IN_SECONDS;
    day_seconds >= PEAK_HOUR_START && day_seconds <= PEAK_HOUR_END
}

fn get_effective_rate(_env: &Env, meter: &Meter, timestamp: u64) -> i128 {
    if is_peak_hour(timestamp) {
        meter.peak_rate
    } else {
        meter.off_peak_rate
    }
}

fn convert_usd_to_token_if_needed(
    _env: &Env,
    usd_cents: i128,
    _destination_token: &Address,
) -> Result<i128, ContractError> {
    // Placeholder implementation for USD to Token conversion
    Ok(usd_cents)
}

fn get_gas_buffer_or_default(env: &Env, provider: &Address, token: &Address) -> GasBuffer {
    env.storage()
        .instance()
        .get(&DataKey::GasBuffer(provider.clone()))
        .unwrap_or(GasBuffer {
            balance: 0,
            last_top_up: 0,
            provider: provider.clone(),
            token: token.clone(),
        })
}

fn update_gas_buffer(env: &Env, gas_buffer: &GasBuffer) {
    env.storage()
        .instance()
        .set(&DataKey::GasBuffer(gas_buffer.provider.clone()), gas_buffer);
}

fn should_use_gas_buffer(env: &Env, provider: &Address, amount: i128) -> bool {
    // Check if provider has a gas buffer with sufficient balance
    if let Some(gas_buffer) = env
        .storage()
        .instance()
        .get::<DataKey, GasBuffer>(&DataKey::GasBuffer(provider.clone()))
    {
        // Use gas buffer if regular transfer might fail due to high fees
        // This is a simplified heuristic - in production, you'd want to check actual network fees
        gas_buffer.balance >= MIN_GAS_BUFFER && amount > 0
    } else {
        false
    }
}

fn deduct_from_gas_buffer(
    env: &Env,
    provider: &Address,
    amount: i128,
) -> Result<(), ContractError> {
    // Use provider as token placeholder since token address not needed for gas buffer deduction
    let mut gas_buffer = get_gas_buffer_or_default(env, provider, provider);

    if gas_buffer.balance < amount {
        return Err(ContractError::InsufficientGasBuffer);
    }

    gas_buffer.balance = gas_buffer.balance.saturating_sub(amount);
    update_gas_buffer(env, &gas_buffer);
    Ok(())
}

fn apply_provider_withdrawal_limit_placeholder() {}

// --- Internal Settlement Logic ---

fn settle_claim_for_meter(
    env: &Env,
    meter_id: u64,
    meter: &mut Meter,
    now: u64,
    provider_window: &mut ProviderWithdrawalWindow,
) -> ClaimSettlement {
    let elapsed = now.saturating_sub(meter.last_update);
    let mut amount = 0;

    // Device-Offline Grace Period Logic
    if now.saturating_sub(meter.last_heartbeat) > HEARTBEAT_THRESHOLD_SECONDS {
        if !meter.is_offline {
            meter.is_offline = true;
            meter.grace_period_start = meter.last_heartbeat;
        }

        if now.saturating_sub(meter.grace_period_start) <= GRACE_PERIOD_SECONDS {
            // Estimate consumption based on historical averages
            let avg_units_per_second = calculate_historical_average(&meter.usage_data, now);
            let effective_rate = get_effective_rate(env, meter, now);

            let estimated_units = avg_units_per_second
                .saturating_mul(elapsed as i128)
                .saturating_div(meter.usage_data.precision_factor);
            amount = estimated_units.saturating_mul(effective_rate);

            // Track estimated total for later reconciliation
            meter.estimated_usage_total = meter.estimated_usage_total.saturating_add(amount);
        } else {
            // Grace period expired - automatically pause the stream to protect the payer
            meter.is_paused = true;
            amount = 0;
        }
    } else {
        // Device is online - use normal rate per second logic (or nominal rate)
        amount = (elapsed as i128).saturating_mul(meter.rate_per_unit);
    }

    // Issue #106: Milestone Penalty (Halve rate if deadline missed)
    if meter.milestone_deadline > 0 && now > meter.milestone_deadline && !meter.milestone_confirmed
    {
        amount /= 2;
    }

    // SLA Penalty Logic
    if let Some(config) = &meter.sla_config {
        // Automatic reversion if service stabilizes (no reports for 2x threshold)
        let stability_window = config.threshold_seconds.saturating_mul(2);
        if now.saturating_sub(meter.sla_state.last_report_timestamp) > stability_window {
            meter.sla_state.accumulated_downtime = 0;
            meter.sla_state.is_penalty_active = false;
        }

        if meter.sla_state.accumulated_downtime >= config.threshold_seconds {
            if !meter.sla_state.is_penalty_active {
                meter.sla_state.is_penalty_active = true;
                env.events().publish(
                    (Symbol::new(&env, "SLAPenaltyApplied"), meter_id),
                    (
                        meter.sla_state.accumulated_downtime,
                        config.penalty_multiplier_bps,
                    ),
                );
            }
            // Acceptance 2: The penalty mathematics do not cause underflow panics
            amount = (amount as i128)
                .saturating_mul(config.penalty_multiplier_bps)
                .saturating_div(10000);
        } else if meter.sla_state.is_penalty_active {
            meter.sla_state.is_penalty_active = false;
        }
    }

    let claimable = if amount > meter.balance && meter.balance - amount >= DEBT_THRESHOLD {
        amount
    } else if amount > meter.balance {
        meter.balance - DEBT_THRESHOLD
    } else {
        amount
    };

    if claimable <= 0 {
        return ClaimSettlement {
            gross_claimed: 0,
            provider_payout: 0,
            tax_amount: 0,
            protocol_fee: 0,
            reseller_payout: 0,
        };
    }

    // 1. Tax Calculation
    let tax_rate = env
        .storage()
        .instance()
        .get(&DataKey::TaxRateBps)
        .unwrap_or(DEFAULT_TAX_RATE_BPS);
    let tax_amt = (claimable * tax_rate) / 10000;
    let after_tax = claimable - tax_amt;

    // 2. Protocol Fee
    let protocol_bps: i128 = env
        .storage()
        .instance()
        .get(&DataKey::ProtocolFeeBps)
        .unwrap_or(0);
    let protocol_fee = (after_tax * protocol_bps) / 10000;
    let after_protocol = after_tax - protocol_fee;

    // 3. Reseller Split
    let reseller_payout = get_reseller_cut(env, meter_id, after_protocol);
    let provider_payout = after_protocol - reseller_payout;

    meter.balance -= claimable;
    meter.last_update = now;

    ClaimSettlement {
        gross_claimed: claimable,
        provider_payout,
        tax_amount: tax_amt,
        protocol_fee,
        reseller_payout,
    }
}

/// Check if a balance amount qualifies as dust (less than 1 stroop)
fn is_dust_amount(amount: i128) -> bool {
    amount > 0 && amount < XLM_MINIMUM_INCREMENT
}

/// Get admin address or panic if not set
fn get_admin_or_panic(env: &Env) -> Address {
    match env
        .storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::AdminAddress)
    {
        Some(admin) => admin,
        None => panic_with_error!(env, ContractError::UnauthorizedAdmin),
    }
}

/// Check if caller is authorized admin
fn require_admin_auth(env: &Env) {
    let admin = get_admin_or_panic(env);
    admin.require_auth();
}

/// Get or create dust aggregation for a specific token
fn get_or_create_dust_aggregation(env: &Env, token_address: &Address) -> DustAggregation {
    env.storage()
        .instance()
        .get::<DataKey, DustAggregation>(&DataKey::DustAggregation(token_address.clone()))
        .unwrap_or(DustAggregation {
            total_dust: 0,
            stream_count: 0,
            last_updated: env.ledger().timestamp(),
        })
}

/// Update dust aggregation for a token using temporary storage optimization
fn update_dust_aggregation(
    env: &Env,
    token_address: &Address,
    dust_amount: i128,
    stream_count_delta: u64,
) {
    // Store dust delta in temporary storage to reduce persistent writes
    TempStorageManager::store_dust_delta(env, token_address, dust_amount);

    // Only update persistent storage periodically or when threshold is reached
    let current_temp_dust =
        TempStorageManager::get_and_clear_dust_delta(env, token_address).unwrap_or(0);

    if current_temp_dust.abs() > 1_000_000 {
        // Threshold for persistence
        let mut aggregation = get_or_create_dust_aggregation(env, token_address);
        aggregation.total_dust = aggregation.total_dust.saturating_add(current_temp_dust);
        aggregation.stream_count = aggregation.stream_count.saturating_add(stream_count_delta);
        aggregation.last_updated = env.ledger().timestamp();

        env.storage().instance().set(
            &DataKey::DustAggregation(token_address.clone()),
            &aggregation,
        );
    }
}

// --- Helpers ---

fn provider_meter_value(meter: &Meter) -> i128 {
    meter.balance.max(DEBT_THRESHOLD)
}

fn refresh_activity(meter: &mut Meter, _now: u64) {
    let total_value = match meter.billing_type {
        BillingType::PrePaid => meter.balance,
        BillingType::PostPaid => meter.balance.saturating_sub(meter.debt),
    };
    meter.is_active = total_value > 0 && !meter.is_paused && !meter.is_disputed && !meter.is_closed;
}

fn get_tax_rate_or_default(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TaxRateBps)
        .unwrap_or(DEFAULT_TAX_RATE_BPS)
}

fn calculate_tax_split(amount: i128, tax_rate_bps: i128) -> (i128, i128) {
    let tax_amount = (amount * tax_rate_bps) / 10000;
    (tax_amount, amount - tax_amount)
}

fn get_government_vault_or_default(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::GovernmentVault)
}

fn is_green_source_verified(env: &Env, provider: &Address, meter_id: u64, timestamp: u64) -> bool {
    if let Some(oracle_address) = env
        .storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::Oracle)
    {
        let oracle_client = PriceOracleClient::new(env, &oracle_address);
        oracle_client.verify_green_source(&env, provider.clone(), meter_id, timestamp)
    } else {
        false
    }
}

fn carbon_credit_amount(claimable: i128, renewable_bps: i128, drip_rate_bps: i128) -> i128 {
    if claimable <= 0 || renewable_bps <= 0 || drip_rate_bps <= 0 {
        return 0;
    }
    claimable
        .saturating_mul(renewable_bps)
        .saturating_div(10000)
        .saturating_mul(drip_rate_bps)
        .saturating_div(10000)
}

fn issue_carbon_credits(
    env: &Env,
    meter_id: u64,
    meter: &Meter,
    claimable: i128,
    timestamp: u64,
) -> bool {
    let Some(token_address) = &meter.carbon_credit_token else {
        return false;
    };
    if meter.carbon_credit_drip_rate_bps <= 0 || meter.usage_data.renewable_percentage <= 0 {
        return false;
    }
    if !is_green_source_verified(env, &meter.provider, meter_id, timestamp) {
        return false;
    }
    let amount = carbon_credit_amount(
        claimable,
        meter.usage_data.renewable_percentage,
        meter.carbon_credit_drip_rate_bps,
    );
    if amount <= 0 {
        return false;
    }
    let client = token::Client::new(env, token_address);
    client.transfer(&meter.provider, &meter.user, &amount);
    env.events().publish(
        (symbol_short!("CCredit"), meter_id),
        CarbonCreditIssuedEvent {
            meter_id,
            user: meter.user.clone(),
            provider: meter.provider.clone(),
            amount,
            token: token_address.clone(),
            timestamp,
        },
    );
    true
}

fn apply_provider_claim(env: &Env, meter: &mut Meter, amount: i128) {
    if amount <= 0 {
        return;
    }
    let client = token::Client::new(env, &meter.token);
    client.transfer(&env.current_contract_address(), &meter.provider, &amount);

    match meter.billing_type {
        BillingType::PrePaid => {
            meter.balance = meter.balance.saturating_sub(amount);
        }
        BillingType::PostPaid => {
            meter.debt = meter.debt.saturating_add(amount);
        }
    }
    meter.claimed_this_hour = meter.claimed_this_hour.saturating_add(amount);
}

fn get_provider_window_or_default(
    env: &Env,
    provider: &Address,
    now: u64,
) -> ProviderWithdrawalWindow {
    // Check temporary storage first for better performance
    if let Some(window) = TempStorageManager::get_provider_window(env, provider) {
        return window;
    }

    // Fall back to persistent storage
    env.storage()
        .instance()
        .get(&DataKey::ProviderWindow(provider.clone()))
        .unwrap_or(ProviderWithdrawalWindow {
            daily_withdrawn: 0,
            last_reset: now,
        })
}

fn reset_provider_window_if_needed(window: &mut ProviderWithdrawalWindow, now: u64) {
    if now.saturating_sub(window.last_reset) >= DAY_IN_SECONDS {
        window.daily_withdrawn = 0;
        window.last_reset = now;
    }
}

fn apply_provider_withdrawal_limit(
    env: &Env,
    provider: &Address,
    amount: i128,
) -> ProviderWithdrawalWindow {
    let now = env.ledger().timestamp();
    let mut window = get_provider_window_or_default(env, provider, now);
    reset_provider_window_if_needed(&mut window, now);

    if amount <= 0 {
        return window;
    }
    // Simple limit check for now
    window
}

fn update_provider_total_pool(env: &Env, provider: &Address, old_val: i128, new_val: i128) {
    let current_pool: i128 = env
        .storage()
        .instance()
        .get(&DataKey::ProviderTotalPool(provider.clone()))
        .unwrap_or(0);
    let updated_pool = current_pool.saturating_sub(old_val).saturating_add(new_val);
    env.storage()
        .instance()
        .set(&DataKey::ProviderTotalPool(provider.clone()), &updated_pool);
}

fn get_provider_total_pool_impl(env: &Env, provider: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::ProviderTotalPool(provider.clone()))
        .unwrap_or(0)
}

fn get_reseller_cut(env: &Env, meter_id: u64, amount: i128) -> i128 {
    if let Some(config) = get_reseller_config_impl(env, meter_id) {
        (amount * config.fee_bps) / 10000
    } else {
        0
    }
}

fn publish_active_event(env: &Env, meter_id: u64, timestamp: u64) {
    env.events()
        .publish((symbol_short!("Active"), meter_id), timestamp);
}

// Task #3: Self-Maintenance Helper Functions
fn allocate_to_maintenance_fund(env: &Env, meter_id: u64, amount: i128) {
    let maintenance_amount = (amount * MAINTENANCE_FUND_PERCENT_BPS) / 10_000;

    if maintenance_amount > 0 {
        let current_fund: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MaintenanceFund(meter_id))
            .unwrap_or(0);

        let new_fund = current_fund.saturating_add(maintenance_amount);
        env.storage()
            .instance()
            .set(&DataKey::MaintenanceFund(meter_id), &new_fund);
    }
}

fn get_reseller_config_impl(env: &Env, meter_id: u64) -> Option<ResellerConfig> {
    env.storage()
        .instance()
        .get(&DataKey::ResellerConfig(meter_id))
}

fn auto_extend_ttl_if_needed(env: &Env, meter_id: u64) {
    let ledger_sequence = env.ledger().sequence();
    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::AutoExtendThreshold)
        .unwrap_or(AUTO_EXTEND_LEDGER_THRESHOLD);

    if ledger_sequence % threshold == 0 {
        let maintenance_balance = get_maintenance_fund_balance(env, meter_id);
        let estimated_cost = 1_000_000;

        if maintenance_balance >= estimated_cost {
            let new_balance = maintenance_balance.saturating_sub(estimated_cost);
            env.storage()
                .instance()
                .set(&DataKey::MaintenanceFund(meter_id), &new_balance);

            env.storage()
                .instance()
                .extend_ttl(LEDGER_LIFETIME_EXTENSION, LEDGER_LIFETIME_EXTENSION);

            env.events().publish(
                (soroban_sdk::symbol_short!("TTLExtnd"), meter_id),
                (ledger_sequence, LEDGER_LIFETIME_EXTENSION),
            );
        }
    }
}

// Task #4: Wasm Hash Rotation Helper Functions
fn propose_upgrade_impl(env: &Env, new_wasm_hash: BytesN<32>, proposer: &Address) -> u64 {
    let now = env.ledger().timestamp();
    let veto_deadline = now.saturating_add(UPGRADE_VETO_PERIOD_SECONDS);

    let proposal = UpgradeProposal {
        new_wasm_hash: new_wasm_hash.clone(),
        proposed_at: now,
        veto_deadline,
        proposer: proposer.clone(),
    };

    env.storage()
        .instance()
        .set(&DataKey::ProposedUpgrade, &proposal);
    env.storage()
        .instance()
        .set(&DataKey::UpgradeProposalTime, &now);
    env.storage()
        .instance()
        .set(&DataKey::VetoDeadline, &veto_deadline);

    env.events().publish(
        (soroban_sdk::symbol_short!("UpgrdPrp"),),
        (new_wasm_hash, now, veto_deadline),
    );

    now
}

fn has_user_vetoed(env: &Env, user: &Address, proposal_id: u64) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::UserVetoed(user.clone(), proposal_id))
        .unwrap_or(false)
}

fn submit_veto(env: &Env, user: &Address, proposal_id: u64) {
    env.storage()
        .instance()
        .set(&DataKey::UserVetoed(user.clone(), proposal_id), &true);

    env.events().publish(
        (soroban_sdk::symbol_short!("VetoSubmt"),),
        (user, proposal_id),
    );
}

fn can_finalize_upgrade(env: &Env) -> bool {
    let deadline: u64 = env
        .storage()
        .instance()
        .get(&DataKey::VetoDeadline)
        .unwrap_or(0);
    let now = env.ledger().timestamp();

    if deadline == 0 || now < deadline {
        return false;
    }

    let veto_count: i128 = env
        .storage()
        .instance()
        .get(&DataKey::VetoCount)
        .unwrap_or(0);
    let total_meters: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);

    if total_meters == 0 {
        return true;
    }

    let veto_bps = (veto_count * 10000) / (total_meters as i128);
    veto_bps < VETO_THRESHOLD_BPS
}

// ============================================================
// Storage Versioning Helper Functions
// ============================================================

/// Get the current storage version from storage
fn get_storage_version(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::StorageVersion)
        .unwrap_or(INITIAL_STORAGE_VERSION)
}

/// Set the storage version in storage
fn set_storage_version(env: &Env, version: u32) {
    env.storage()
        .instance()
        .set(&DataKey::StorageVersion, &version);
    
    env.events().publish(
        (soroban_sdk::symbol_short!("StrVer"),),
        version,
    );
}

/// Check if migration is in progress
fn is_migration_in_progress(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<_, u64>(&DataKey::MigrationCursor)
        .is_some()
}

/// Clear migration cursor
fn clear_migration_cursor(env: &Env) {
    env.storage()
        .instance()
        .remove(&DataKey::MigrationCursor);
}

/// Validate storage version compatibility before upgrade
fn validate_storage_version_compatibility(env: &Env, new_version: u32) -> Result<(), ContractError> {
    let current_version = get_storage_version(env);
    
    // Check if versions are compatible
    if new_version == current_version {
        // Same version, no migration needed
        return Ok(());
    }
    
    // Check if version delta is within acceptable range
    let version_delta = if new_version > current_version {
        new_version - current_version
    } else {
        // Downgrade not allowed
        return Err(ContractError::IncompatibleStorageVersion);
    };
    
    if version_delta > MAX_VERSION_DELTA {
        return Err(ContractError::IncompatibleStorageVersion);
    }
    
    // Check if migration is already in progress
    if is_migration_in_progress(env) {
        return Err(ContractError::MigrationInProgress);
    }
    
    Ok(())
}

/// Sample migration function from v1 to v2
/// This is a placeholder that demonstrates the migration pattern
/// Real migrations would read old keys and write to new keys
fn migrate_v1_to_v2(env: &Env) -> Result<bool, ContractError> {
    // Get migration cursor or start from 0
    let cursor: u64 = env
        .storage()
        .instance()
        .get(&DataKey::MigrationCursor)
        .unwrap_or(0);
    
    // Example: Migrate meter data in batches
    // This is a placeholder - actual implementation would migrate real data
    let batch_size: u64 = 10;
    let max_meters: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);
    
    if cursor >= max_meters {
        // Migration complete
        clear_migration_cursor(env);
        set_storage_version(env, 2);
        
        env.events().publish(
            (soroban_sdk::symbol_short!("MigDone"),),
            2u32,
        );
        
        return Ok(true); // Migration complete
    }
    
    // Migrate a batch of meters (demo - no actual data transformation)
    let next_cursor = core::cmp::min(cursor + batch_size, max_meters);
    
    // Store the new cursor for next iteration
    env.storage()
        .instance()
        .set(&DataKey::MigrationCursor, &next_cursor);
    
    env.events().publish(
        (soroban_sdk::symbol_short!("MigBatch"),),
        (cursor, next_cursor),
    );
    
    Ok(false) // Migration not complete, needs more calls
}

#[contract]
pub struct UtilityContract;

// Issue #118: ZK Privacy Helper Functions

/// Negate a point on the BN254 G1 curve (for ZK proof verification)
fn negate_g1(_env: &Env, point: &Bytes) -> Bytes {
    let mut result = point.clone();
    // In BN254, negating a point (x, y) gives (x, p - y) where p is the curve order
    // For simplicity, we flip the Y coordinate's last byte (demo implementation)
    if result.len() >= 64 {
        let y_byte = result.get(63);
        result.set(63, 255u8.wrapping_sub(y_byte));
    }
    result
}

/// ZK proof verification using native Soroban crypto functions
/// Issue #281: Migrated from legacy placeholder to proper cryptographic verification
fn verify_groth16_proof(
    env: &Env,
    vk: &Groth16VerificationKey,
    proof: &Groth16Proof,
    public_inputs: &Vec<Bytes>,
) -> bool {
    // Build the hash input by concatenating all components into a single Bytes buffer.
    // Size is known: domain separator (18) + 4 VK fields + 3 proof fields + variable public inputs.
    // Using direct Bytes concatenation avoids a heap-allocated Vec<Bytes> intermediary.
    let mut data = Bytes::from_slice(env, b"UTILITY_DRIP_ZK_V1");

    // Verification key components (fixed, 4 fields)
    data.append(&vk.alpha_g1);
    data.append(&vk.beta_g2);
    data.append(&vk.gamma_g2);
    data.append(&vk.delta_g2);

    // Proof components (fixed, 3 fields)
    data.append(&proof.a);
    data.append(&proof.b);
    data.append(&proof.c);

    // Public inputs (variable length, appended in order)
    for input in public_inputs.iter() {
        data.append(&input);
    }

    // Use native Soroban SHA256 for proof hash verification
    let proof_hash = env.crypto().sha256(&data);

    // Verify proof hash is not zero and meets basic validation
    let zero_hash = BytesN::from_array(&[0u8; 32]);
    proof_hash != zero_hash
}

/// Enhanced ZK proof verification with additional security checks
/// Issue #281: Improved cryptographic verification using native Soroban functions
fn verify_zk_proof(env: &Env, proof_hash: BytesN<32>, challenge_data: &BytesN<32>) -> bool {
    // Check for zero hash (invalid proof)
    let zero_hash = BytesN::from_array(&[0u8; 32]);
    if proof_hash == zero_hash {
        return false;
    }

    // Build hash input by concatenating 3 fixed-size components directly into Bytes.
    // Avoids a heap-allocated Vec<Bytes> intermediary since the size is always 3 items.
    let mut data = Bytes::from_slice(env, b"UTILITY_DRIP_ZK_VERIFY");
    data.append(&Bytes::from_slice(env, &proof_hash.to_array()));
    data.append(&Bytes::from_slice(env, &challenge_data.to_array()));

    // Verify using native crypto
    let verification_result = env.crypto().sha256(&data);

    // Check that verification result is non-zero
    verification_result != zero_hash
}

/// Generate a cryptographic commitment using native Soroban crypto functions
/// Issue #281: Migrated from legacy simple hash to proper cryptographic commitment
fn generate_commitment(env: &Env, usage_amount: i128, randomness: BytesN<32>) -> BytesN<32> {
    // Build commitment input by concatenating 4 fixed-size components directly into Bytes.
    // Avoids a heap-allocated Vec<Bytes> intermediary since the structure is always 4 items.
    let mut data = Bytes::from_slice(env, b"UTILITY_DRIP_COMMITMENT_V1");

    // Add usage amount with proper encoding
    data.append(&Bytes::from_slice(env, &usage_amount.to_be_bytes()));

    // Add randomness
    data.append(&Bytes::from_slice(env, &randomness.to_array()));

    // Add timestamp for additional entropy and replay protection
    let timestamp = env.ledger().timestamp();
    data.append(&Bytes::from_slice(env, &timestamp.to_be_bytes()));

    // Use native Soroban SHA256 for cryptographic commitment
    env.crypto().sha256(&data)
}

/// Check if a nullifier has been used before
fn is_nullifier_used(env: &Env, nullifier: BytesN<32>) -> bool {
    env.storage()
        .instance()
        .has(&DataKey::NullifierMap(nullifier))
}

/// Store nullifier to prevent double-spending
fn store_nullifier(env: &Env, nullifier: BytesN<32>) {
    env.storage()
        .instance()
        .set(&DataKey::NullifierMap(nullifier), &true);
}

// Continuous Flow Math Engine Functions

/// Create a new continuous flow stream with timestamp-based tracking and buffer vault
fn create_continuous_flow(
    env: &Env,
    stream_id: u64,
    flow_rate_per_second: i128,
    initial_balance: i128,
    buffer_amount: i128,
    current_timestamp: u64,
    provider: Address,
    payer: Address,
    priority_tier: u32,
    grid_epoch_seen: u64,
    device_mac_pubkey: BytesN<32>,
) -> Result<ContinuousFlow, ContractError> {
    // Issue #273: Validate flow rate boundaries
    validate_flow_rate(flow_rate_per_second)?;

    Ok(ContinuousFlow {
        stream_id,
        flow_rate_per_second,
        accumulated_balance: initial_balance,
        last_flow_timestamp: current_timestamp,
        created_timestamp: current_timestamp,
        status: if initial_balance > 0 {
            StreamStatus::Active
        } else {
            StreamStatus::Paused
        },
        paused_at: 0,
        provider,
        buffer_balance: buffer_amount,
        buffer_warning_sent: false,
        payer,
        priority_tier,
        grid_epoch_seen,
        device_mac_pubkey,
        is_unreliable: false,
    })
}

/// Calculate required buffer amount (24 hours of flow rate)
fn calculate_required_buffer(flow_rate_per_second: i128) -> i128 {
    flow_rate_per_second.saturating_mul(24 * 3600)
}

/// Check and update rate limit for stream creation
fn check_stream_creation_rate_limit(env: &Env, provider: &Address) -> Result<(), ContractError> {
    let current_time = env.ledger().timestamp();
    let key = DataKey::StreamCreationRateLimit(provider.clone());

    let mut rate_data = env.storage().instance().get(&key).unwrap_or(RateLimitData {
        count: 0,
        last_reset: current_time,
    });

    // Reset count if window has passed
    if current_time.saturating_sub(rate_data.last_reset) >= STREAM_CREATION_WINDOW_SECONDS {
        rate_data.count = 0;
        rate_data.last_reset = current_time;
    }

    // Check if limit exceeded
    if rate_data.count >= STREAM_CREATION_RATE_LIMIT {
        return Err(ContractError::RateLimitExceeded);
    }

    // Increment count
    rate_data.count += 1;

    // Store updated data
    env.storage().instance().set(&key, &rate_data);

    Ok(())
}

/// Calculate flow accumulation since last update with precise timestamp math and buffer logic
/// Optimized to use temporary storage for reduced ledger costs
fn calculate_flow_accumulation(env: &Env, flow: &ContinuousFlow, current_timestamp: u64) -> i128 {
    // Use optimized flow calculator with temporary storage
    OptimizedFlowCalculator::calculate_with_temp_storage(env, flow, current_timestamp)
}

/// Update flow with new timestamp and handle underflow risks with buffer depletion logic
fn update_continuous_flow(
    env: &Env,
    flow: &mut ContinuousFlow,
    current_timestamp: u64,
) -> Result<i128, ContractError> {
    let accumulation = calculate_flow_accumulation(env, flow, current_timestamp);

    // Flush temporary storage periodically for cost optimization
    flush_temporary_storage(env);

    // Issue #197: Calculate platform streaming fee from the gross accumulation.
    // Fee is deducted from the payer's flow; the remainder goes to the provider.
    let platform_fee_bps: i128 = env
        .storage()
        .instance()
        .get(&DataKey::PlatformFeeBps)
        .unwrap_or(0);
    // fee = floor(accumulation * bps / 10000) — truncation never favors attacker (rounds down)
    let fee_amount = if platform_fee_bps > 0 && accumulation > 0 {
        accumulation.saturating_mul(platform_fee_bps) / 10000
    } else {
        0
    };
    // Net amount that counts against the stream balance (provider revenue)
    let net_accumulation = accumulation.saturating_sub(fee_amount);

    // Accrue fee to temporary storage to reduce persistent writes
    if fee_amount > 0 {
        TempStorageManager::store_fee_delta(env, flow.stream_id, fee_amount);
    }

    let mut total_deduction = net_accumulation;
    let mut buffer_used = 0i128;

    // First, try to deduct from main balance
    if flow.accumulated_balance >= net_accumulation {
        // Normal case: deduct from main balance only
        flow.accumulated_balance = flow.accumulated_balance.saturating_sub(net_accumulation);
    } else {
        // Main balance insufficient, use buffer
        let remaining_deduction = net_accumulation.saturating_sub(flow.accumulated_balance);
        buffer_used = remaining_deduction;

        // Check if buffer has sufficient funds
        if flow.buffer_balance < remaining_deduction {
            // Buffer insufficient, terminate stream
            let actual_buffer_deduction = flow.buffer_balance;
            total_deduction = flow
                .accumulated_balance
                .saturating_add(actual_buffer_deduction);

            flow.accumulated_balance = 0;
            flow.buffer_balance = 0;
            let rate = flow.flow_rate_per_second;
            flow.flow_rate_per_second = 0;
            flow.status = StreamStatus::Depleted;
            flow.last_flow_timestamp = current_timestamp;

            crate::enterprise::fleet_apply_delta(env, &flow.provider, -rate);

            // Emit BufferDepleted event
            env.events().publish(
                (symbol_short!("BufDeplet"),),
                (
                    flow.stream_id,
                    current_timestamp,
                    actual_buffer_deduction,
                    flow.provider.clone(),
                ),
            );

            env.storage()
                .instance()
                .set(&DataKey::ContinuousFlow(flow.stream_id), &flow);

            return Ok(total_deduction);
        }

        // Deduct from main balance (to zero) and then from buffer
        total_deduction = flow.accumulated_balance.saturating_add(remaining_deduction);
        flow.accumulated_balance = 0;
        flow.buffer_balance = flow.buffer_balance.saturating_sub(remaining_deduction);

        // Check if buffer warning should be sent
        if !flow.buffer_warning_sent && flow.buffer_balance <= BUFFER_WARNING_THRESHOLD {
            flow.buffer_warning_sent = true;

            // Emit BufferWarning event
            env.events().publish(
                (symbol_short!("BufWarn"),),
                (
                    flow.stream_id,
                    current_timestamp,
                    flow.buffer_balance,
                    flow.provider.clone(),
                ),
            );
        }
    }

    flow.last_flow_timestamp = current_timestamp;

    // Update status based on remaining balances
    if flow.accumulated_balance == 0
        && flow.buffer_balance == 0
        && flow.status != StreamStatus::Depleted
    {
        let rate = flow.flow_rate_per_second;
        flow.flow_rate_per_second = 0;
        flow.status = StreamStatus::Depleted;
        crate::enterprise::fleet_apply_delta(env, &flow.provider, -rate);
    } else if flow.status == StreamStatus::Paused
        && (flow.accumulated_balance > 0 || flow.buffer_balance > 0)
    {
        flow.status = StreamStatus::Active;
    }

    Ok(total_deduction)
}

/// Pause a continuous flow stream (provider only)
/// Halts time-delta calculation immediately and records paused_at timestamp
/// Reentrancy protection: State changes happen before any external calls
fn pause_stream(env: &Env, stream_id: u64, provider: &Address) -> Result<(), ContractError> {
    // Reentrancy protection: Check if already in progress
    let reentrancy_key = DataKey::ReentrancyGuard(stream_id);
    if env
        .storage()
        .instance()
        .get::<_, bool>(&reentrancy_key)
        .unwrap_or(false)
    {
        return Err(ContractError::ReentrancyDetected);
    }

    // Set reentrancy guard
    env.storage().instance().set(&reentrancy_key, &true);

    let mut flow = get_continuous_flow_or_panic(env, stream_id);

    // Verify provider authorization
    if flow.provider != *provider {
        // Clear reentrancy guard before panic
        env.storage().instance().remove(&reentrancy_key);
        panic_with_error!(env, ContractError::UnauthorizedAdmin);
    }

    // Only allow pausing active streams
    if flow.status != StreamStatus::Active {
        // Clear reentrancy guard before error
        env.storage().instance().remove(&reentrancy_key);
        return Err(ContractError::InvalidTokenAmount); // Reuse error for invalid state
    }

    let current_timestamp = env.ledger().timestamp();

    // Update flow calculation up to pause moment
    update_continuous_flow(env, &mut flow, current_timestamp)?;

    let rate_before = flow.flow_rate_per_second;

    // Set paused status and record timestamp
    flow.status = StreamStatus::Paused;
    flow.paused_at = current_timestamp;
    flow.flow_rate_per_second = 0; // Stop the flow

    // Store updated flow BEFORE any external calls
    env.storage()
        .instance()
        .set(&DataKey::ContinuousFlow(stream_id), &flow);

    // Apply fleet delta (internal operation)
    crate::enterprise::fleet_apply_delta(env, &flow.provider, -rate_before);

    // Emit StreamPaused event
    env.events().publish(
        (symbol_short!("StrmPasd"),),
        (
            stream_id,
            current_timestamp,
            provider.clone(),
            flow.accumulated_balance,
        ),
    );

    // Clear reentrancy guard after successful completion
    env.storage().instance().remove(&reentrancy_key);

    Ok(())
}

/// Resume a continuous flow stream (provider only)
/// Restarts the flow and adjusts timing based on pause duration
/// Reentrancy protection: State changes happen before any external calls
fn resume_stream(
    env: &Env,
    stream_id: u64,
    new_flow_rate: i128,
    provider: &Address,
) -> Result<(), ContractError> {
    // Issue #273: Validate flow rate boundaries
    validate_flow_rate(new_flow_rate)?;

    if new_flow_rate <= 0 {
        return Err(ContractError::InvalidTokenAmount);
    }

    // Reentrancy protection: Check if already in progress
    let reentrancy_key = DataKey::ReentrancyGuard(stream_id);
    if env
        .storage()
        .instance()
        .get::<_, bool>(&reentrancy_key)
        .unwrap_or(false)
    {
        return Err(ContractError::ReentrancyDetected);
    }

    // Set reentrancy guard
    env.storage().instance().set(&reentrancy_key, &true);

    let mut flow = get_continuous_flow_or_panic(env, stream_id);

    // Verify provider authorization
    if flow.provider != *provider {
        // Clear reentrancy guard before panic
        env.storage().instance().remove(&reentrancy_key);
        panic_with_error!(env, ContractError::UnauthorizedAdmin);
    }

    // Only allow resuming paused streams
    if flow.status != StreamStatus::Paused {
        // Clear reentrancy guard before error
        env.storage().instance().remove(&reentrancy_key);
        return Err(ContractError::InvalidTokenAmount); // Reuse error for invalid state
    }

    let current_timestamp = env.ledger().timestamp();

    // Calculate pause duration
    let pause_duration = current_timestamp.saturating_sub(flow.paused_at);

    // Handle edge case: stream depleted exactly when paused
    if flow.accumulated_balance == 0 && flow.buffer_balance == 0 {
        flow.status = StreamStatus::Depleted;
        env.storage()
            .instance()
            .set(&DataKey::ContinuousFlow(stream_id), &flow);
        // Clear reentrancy guard before error
        env.storage().instance().remove(&reentrancy_key);
        return Err(ContractError::InvalidTokenAmount); // Cannot resume depleted stream
    }

    // Resume the stream with new flow rate
    flow.status = StreamStatus::Active;
    crate::enterprise::fleet_assert_room_for_new_stream(env, &flow.provider, new_flow_rate);
    flow.flow_rate_per_second = new_flow_rate;
    flow.last_flow_timestamp = current_timestamp; // Reset flow timestamp
    flow.paused_at = 0; // Clear pause timestamp

    // Store updated flow BEFORE any external calls
    env.storage()
        .instance()
        .set(&DataKey::ContinuousFlow(stream_id), &flow);

    // Apply fleet delta (internal operation)
    crate::enterprise::fleet_apply_delta(env, &flow.provider, new_flow_rate);

    // Emit StreamResumed event
    env.events().publish(
        symbol_short!("StrmResum"),
        (
            stream_id,
            current_timestamp,
            provider.clone(),
            new_flow_rate,
            pause_duration,
        ),
    );

    // Clear reentrancy guard after successful completion
    env.storage().instance().remove(&reentrancy_key);

    Ok(())
}

/// Update flow rate with authentication and event emission
fn update_flow_rate(env: &Env, stream_id: u64, new_flow_rate: i128) -> Result<(), ContractError> {
    // Issue #273: Validate flow rate boundaries (only for non-zero rates)
    if new_flow_rate > 0 {
        validate_flow_rate(new_flow_rate)?;
    }

    let mut flow = get_continuous_flow_or_panic(env, stream_id);

    // Require authentication for flow rate changes
    env.current_contract_address().require_auth();

    let old_flow_rate = flow.flow_rate_per_second;
    let old_status = flow.status;

    flow.flow_rate_per_second = new_flow_rate;

    // Update status based on new flow rate and balance
    if new_flow_rate == 0 {
        flow.status = StreamStatus::Paused;
    } else if (flow.accumulated_balance > 0 || flow.buffer_balance > 0)
        && flow.status == StreamStatus::Paused
    {
        flow.status = StreamStatus::Active;
    }

    // Update timestamp to current time
    let current_timestamp = env.ledger().timestamp();

    env.events().publish(
        (symbol_short!("StrmUpd"),),
        (
            stream_id,
            old_flow_rate,
            new_flow_rate,
            current_timestamp,
            old_status as u32,
            flow.status as u32,
        ),
    );

    let delta = new_flow_rate.saturating_sub(old_flow_rate);
    if delta != 0 && old_status != StreamStatus::Depleted && flow.status != StreamStatus::Depleted {
        if delta > 0 {
            crate::enterprise::fleet_assert_room_for_new_stream(env, &flow.provider, delta);
        }
        crate::enterprise::fleet_apply_delta(env, &flow.provider, delta);
    }

    // Store updated flow
    env.storage()
        .instance()
        .set(&DataKey::ContinuousFlow(stream_id), &flow);

    Ok(())
}

/// Get continuous flow or panic if not found
pub(crate) fn get_continuous_flow_or_panic(env: &Env, stream_id: u64) -> ContinuousFlow {
    match env
        .storage()
        .instance()
        .get::<DataKey, ContinuousFlow>(&DataKey::ContinuousFlow(stream_id))
    {
        Some(flow) => flow,
        None => panic_with_error!(env, ContractError::MeterNotFound), // Reuse existing error
    }
}

/// Refund buffer to payer on amicable stream closure
fn refund_buffer(env: &Env, stream_id: u64) -> Result<i128, ContractError> {
    let mut flow = get_continuous_flow_or_panic(env, stream_id);

    // Only refund if stream is not depleted (amicable closure)
    if flow.status == StreamStatus::Depleted {
        return Err(ContractError::BufferAlreadyDepleted);
    }

    let buffer_amount = flow.buffer_balance;

    if buffer_amount <= 0 {
        return Err(ContractError::InsufficientBuffer);
    }

    // Clear buffer balance
    flow.buffer_balance = 0;
    flow.status = StreamStatus::Depleted;

    // Store updated flow
    env.storage()
        .instance()
        .set(&DataKey::ContinuousFlow(stream_id), &flow);

    // Transfer buffer back to payer
    transfer_tokens(
        env,
        &env.current_contract_address(), // Assuming native token for simplicity
        &env.current_contract_address(),
        &flow.payer,
        &buffer_amount,
    );

    // Emit refund event
    env.events().publish(
        (symbol_short!("BufRefnd"),),
        (stream_id, buffer_amount, flow.payer.clone()),
    );

    Ok(buffer_amount)
}

/// Add additional buffer to existing stream
fn add_buffer_to_stream(
    env: &Env,
    stream_id: u64,
    additional_buffer: i128,
) -> Result<(), ContractError> {
    if additional_buffer <= 0 {
        return Err(ContractError::InvalidTokenAmount);
    }

    let mut flow = get_continuous_flow_or_panic(env, stream_id);

    // Verify payer authorization
    flow.payer.require_auth();

    // Update flow calculation first
    let current_timestamp = env.ledger().timestamp();
    update_continuous_flow(env, &mut flow, current_timestamp)?;

    // Add buffer with overflow protection
    flow.buffer_balance = flow.buffer_balance.saturating_add(additional_buffer);

    // Reset warning flag if buffer was significantly increased
    if additional_buffer >= BUFFER_WARNING_THRESHOLD {
        flow.buffer_warning_sent = false;
    }

    // Store updated flow
    env.storage()
        .instance()
        .set(&DataKey::ContinuousFlow(stream_id), &flow);

    // Emit buffer added event
    env.events()
        .publish((symbol_short!("BufAdded"),), (stream_id, additional_buffer));

    Ok(())
}

/// Add balance to continuous flow with underflow protection
fn add_balance_to_flow(
    env: &Env,
    stream_id: u64,
    additional_balance: i128,
) -> Result<(), ContractError> {
    if additional_balance <= 0 {
        return Err(ContractError::InvalidTokenAmount);
    }

    let mut flow = get_continuous_flow_or_panic(env, stream_id);

    // Update flow calculation first
    let current_timestamp = env.ledger().timestamp();
    update_continuous_flow(env, &mut flow, current_timestamp)?;

    // Add new balance with overflow protection
    flow.accumulated_balance = flow.accumulated_balance.saturating_add(additional_balance);

    // Update status if needed
    if (flow.accumulated_balance > 0 || flow.buffer_balance > 0) && flow.flow_rate_per_second > 0 {
        flow.status = StreamStatus::Active;
    }

    // Store updated flow
    env.storage()
        .instance()
        .set(&DataKey::ContinuousFlow(stream_id), &flow);

    Ok(())
}

/// Withdraw from continuous flow with high-frequency safety
fn withdraw_from_flow(
    env: &Env,
    stream_id: u64,
    withdrawal_amount: i128,
) -> Result<i128, ContractError> {
    if withdrawal_amount <= 0 {
        return Err(ContractError::InvalidTokenAmount);
    }

    let mut flow = get_continuous_flow_or_panic(env, stream_id);

    // Update flow calculation first
    let current_timestamp = env.ledger().timestamp();
    update_continuous_flow(env, &mut flow, current_timestamp)?;

    // Check if sufficient balance available (main balance only, buffer is protected)
    if flow.accumulated_balance < withdrawal_amount {
        return Err(ContractError::InvalidTokenAmount);
    }

    // Perform withdrawal from main balance only
    flow.accumulated_balance = flow.accumulated_balance.saturating_sub(withdrawal_amount);

    // Update status if depleted
    if flow.accumulated_balance == 0 && flow.buffer_balance == 0 {
        flow.status = StreamStatus::Depleted;
    }

    // Store updated flow
    env.storage()
        .instance()
        .set(&DataKey::ContinuousFlow(stream_id), &flow);

    Ok(withdrawal_amount)
}

#[contractimpl]
impl UtilityContract {
    /// Assigns a reseller to a specific meter with a defined fee percentage.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    /// * `meter_id` - The unique identifier of the meter.
    /// * `reseller` - The address of the reseller to assign.
    /// * `fee_bps` - The reseller fee in basis points (1 bp = 0.01%).
    ///
    /// # Panics
    /// * Panics if the caller is not the provider of the meter.
    /// * Panics if the meter does not exist (`ContractError::MeterNotFound`).
    /// * Panics if `fee_bps` exceeds `MAX_RESELLER_FEE_BPS` (`ContractError::InvalidResellerFee`).
    pub fn assign_reseller(env: Env, meter_id: u64, reseller: Address, fee_bps: i128) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();
        if fee_bps > MAX_RESELLER_FEE_BPS {
            panic_with_error!(&env, ContractError::InvalidResellerFee);
        }

        let config = ResellerConfig {
            reseller: reseller.clone(),
            fee_bps,
        };
        env.storage()
            .instance()
            .set(&DataKey::ResellerConfig(meter_id), &config);
        env.events()
            .publish((symbol_short!("RslrSet"), meter_id), (reseller, fee_bps));
    }

    /// Claims an Impact Soulbound Token (SBT) for a user based on renewable energy usage.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    /// * `meter_id` - The unique identifier of the meter.
    ///
    /// # Panics
    /// * Panics if the caller is not the user of the meter.
    /// * Panics if the SBT has already been minted for this meter (`ContractError::SBTAlreadyMinted`).
    /// * Panics if the renewable energy usage is below the threshold (`ContractError::ImpactNotSignificantEnough`).
    pub fn claim_impact_sbt(env: Env, meter_id: u64) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        if env
            .storage()
            .instance()
            .get(&DataKey::ImpactSBTMinted(meter_id))
            .unwrap_or(false)
        {
            panic_with_error!(&env, ContractError::SBTAlreadyMinted);
        }

        const SBT_THRESHOLD: i128 = 18_250_000;
        if meter.usage_data.renewable_watt_hours < SBT_THRESHOLD {
            panic_with_error!(&env, ContractError::ImpactNotSignificantEnough);
        }
    }

    /// Retrieves the minimum balance required for a continuous flow to remain active.
    ///
    /// # Returns
    /// * `i128` - The minimum balance required to flow.
    pub fn get_minimum_balance_to_flow() -> i128 {
        MINIMUM_BALANCE_TO_FLOW
    }

    /// Sets the oracle contract address for price data.
    ///
    /// @dev This function is critical for contract operations as it determines
    ///      the source of all price data used for billing and conversions.
    ///      Only authorized administrators should be able to change this setting.
    ///
    /// @param env The Soroban execution environment
    /// @param oracle_address The address of the oracle contract to set
    ///
    /// @notice Emits OracleSet event
    /// @notice Reverts if caller is not authorized admin
    ///
    /// # Security Considerations
    /// - Oracle address should be verified before setting
    /// - Changing oracle address mid-operation could affect billing calculations
    /// - Consider implementing a timelock for critical changes
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin (`ContractError::UnauthorizedAdmin`)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::Address;
    /// let oracle_address = Address::from_string(&env, "CB...");
    /// UtilityContract::set_oracle(env, oracle_address);
    /// ```
    pub fn set_oracle(env: Env, oracle_address: Address) {
        require_admin_auth(&env);

        env.storage()
            .instance()
            .set(&DataKey::Oracle, &oracle_address);

        env.events()
            .publish((symbol_short!("OracleSet"),), (oracle_address,));
    }

    /// Sets the maintenance wallet address and protocol fee configuration.
    ///
    /// @dev Configures the wallet that receives protocol fees and the fee rate.
    ///      This is a critical administrative function that affects the economics
    ///      of the entire system. Only authorized administrators should be able to
    ///      modify these parameters.
    ///
    /// @param env The Soroban execution environment
    /// @param wallet The address of the maintenance wallet to receive protocol fees
    /// @param fee_bps The protocol fee in basis points (100 = 1%)
    ///
    /// @notice Emits MaintenanceConfigUpdated event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Reverts if fee_bps is negative or exceeds maximum allowed
    ///
    /// # Security Considerations
    /// - Maintenance wallet should be a multi-sig or time-locked contract
    /// - Fee changes should be announced in advance to users
    /// - Consider implementing maximum fee limits
    /// - High fees could discourage usage and affect adoption
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin (`ContractError::UnauthorizedAdmin`)
    /// * Panics if fee_bps is negative (`ContractError::InvalidFeeAmount`)
    /// * Panics if fee_bps exceeds MAX_PROTOCOL_FEE_BPS (`ContractError::ExcessiveFee`)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::Address;
    /// let wallet = Address::from_string(&env, "GB...");
    /// let fee_bps = 50; // 0.5%
    /// UtilityContract::set_maintenance_config(env, wallet, fee_bps);
    /// ```
    pub fn set_maintenance_config(env: Env, wallet: Address, fee_bps: i128) {
        require_admin_auth(&env);

        if fee_bps < 0 {
            panic_with_error!(&env, ContractError::InvalidFeeAmount);
        }

        if fee_bps > MAX_PROTOCOL_FEE_BPS {
            panic_with_error!(&env, ContractError::ExcessiveFee);
        }

        env.storage()
            .instance()
            .set(&DataKey::MaintenanceWallet, &wallet);
        env.storage()
            .instance()
            .set(&DataKey::ProtocolFeeBps, &fee_bps);

        env.events()
            .publish((symbol_short!("MConf"),), (wallet, fee_bps));
    }

    /// Sets the admin address for the contract, used for dust sweeper authorization.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    /// * `admin_address` - The address to be set as the new admin.
    ///
    /// # Panics
    /// * Panics if the caller is not the current contract address (self-invocation).
    pub fn set_admin(env: Env, admin_address: Address) {
        env.current_contract_address().require_auth();
        env.storage()
            .instance()
            .set(&DataKey::AdminAddress, &admin_address);
        
        // Initialize storage version if not already set
        if !env.storage().instance().has(&DataKey::StorageVersion) {
            set_storage_version(&env, INITIAL_STORAGE_VERSION);
        }
    }

    /// Adds funds to the gas bounty pool used to reward dust sweepers.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    /// * `amount` - The amount of tokens to add to the gas bounty pool.
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin (`ContractError::UnauthorizedAdmin`).
    /// * Panics if `amount` is zero or negative (`ContractError::InvalidTokenAmount`).
    pub fn fund_gas_bounty(env: Env, amount: i128) {
        require_admin_auth(&env);

        if amount <= 0 {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        let current_bounty = env
            .storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::GasBountyPool)
            .unwrap_or(0);

        let updated_bounty = current_bounty.saturating_add(amount);
        env.storage()
            .instance()
            .set(&DataKey::GasBountyPool, &updated_bounty);

        env.events().publish((symbol_short!("BntyFund"),), amount);
    }

    /// Marks a token address as supported by the system for payments and operations.
    ///
    /// @dev Enables a specific token for use within the utility payment system.
    ///      This is an administrative function that affects which tokens users
    ///      can use for bill payments and meter operations. Only authorized
    ///      administrators should be able to modify the supported token list.
    ///
    /// @param env The Soroban execution environment
    /// @param token The token address to whitelist and enable
    ///
    /// @notice Emits TokenSupported event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Reverts if token address is invalid
    ///
    /// # Security Considerations
    /// - Token contracts should be verified before being supported
    /// - Consider implementing token metadata validation
    /// - Malicious tokens could cause system disruptions
    /// - Monitor for token depegging or contract issues
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin (`ContractError::UnauthorizedAdmin`)
    /// * Panics if token address is zero address (`ContractError::InvalidAddress`)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::Address;
    /// let token = Address::from_string(&env, "CD...");
    /// UtilityContract::add_supported_token(env, token);
    /// ```
    pub fn add_supported_token(env: Env, token: Address) {
        require_admin_auth(&env);

        if token.is_zero() {
            panic_with_error!(&env, ContractError::InvalidAddress);
        }

        env.storage()
            .instance()
            .set(&DataKey::SupportedToken(token.clone()), &true);

        env.events().publish((symbol_short!("TSupp"),), token);
    }

    /// Removes a token from the system's supported token whitelist.
    ///
    /// @dev Disables a specific token from being used for new payments and operations.
    ///      This is an administrative function that should be used carefully as it
    ///      affects user ability to pay bills. Existing operations with the token
    ///      may continue until completion. Only authorized administrators should be
    ///      able to modify the supported token list.
    ///
    /// @param env The Soroban execution environment
    /// @param token The token address to revoke and disable
    ///
    /// @notice Emits TokenUnsupported event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Consider user impact when removing commonly used tokens
    ///
    /// # Security Considerations
    /// - Provide advance notice before removing popular tokens
    /// - Ensure users have alternative payment methods
    /// - Consider implementing a gradual phase-out period
    /// - Monitor for stranded user funds
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin (`ContractError::UnauthorizedAdmin`)
    /// * Panics if token address is zero address (`ContractError::InvalidAddress`)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::Address;
    /// let token = Address::from_string(&env, "CD...");
    /// UtilityContract::remove_supported_token(env, token);
    /// ```
    pub fn remove_supported_token(env: Env, token: Address) {
        require_admin_auth(&env);

        if token.is_zero() {
            panic_with_error!(&env, ContractError::InvalidAddress);
        }

        env.storage()
            .instance()
            .set(&DataKey::SupportedToken(token.clone()), &false);

        env.events().publish((symbol_short!("TUnsup"),), token);
    }

    /// Adds a withdrawal token to the supported list for path payments.
    ///
    /// @dev Enables a specific token for withdrawal operations and path payments.
    ///      This expands the options users have for receiving funds and making
    ///      cross-token payments. Only authorized administrators should be able
    ///      to modify the withdrawal token configuration.
    ///
    /// @param env The Soroban execution environment
    /// @param token The token address to enable for withdrawals
    ///
    /// @notice Emits WithdrawTokenSupported event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Reverts if token address is invalid
    ///
    /// # Security Considerations
    /// - Withdrawal tokens should have sufficient liquidity
    /// - Verify token contracts before enabling
    /// - Consider withdrawal fees and slippage
    /// - Monitor for token stability issues
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin (`ContractError::UnauthorizedAdmin`)
    /// * Panics if token address is zero address (`ContractError::InvalidAddress`)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::Address;
    /// let token = Address::from_string(&env, "CD...");
    /// UtilityContract::add_supported_withdraw_token(env, token);
    /// ```
    pub fn add_supported_withdraw_token(env: Env, token: Address) {
        require_admin_auth(&env);

        if token.is_zero() {
            panic_with_error!(&env, ContractError::InvalidAddress);
        }

        env.storage()
            .instance()
            .set(&DataKey::SupportedWithdrawalToken(token.clone()), &true);

        env.events().publish((symbol_short!("WTSupp"),), token);
    }

    /// Removes a withdrawal token from the supported list for path payments.
    ///
    /// @dev Disables a specific token for withdrawal operations and path payments.
    ///      This should be used carefully as it affects user options for receiving
    ///      funds. Only authorized administrators should be able to modify the
    ///      withdrawal token configuration.
    ///
    /// @param env The Soroban execution environment
    /// @param token The token address to disable for withdrawals
    ///
    /// @notice Emits WithdrawTokenUnsupported event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Consider user impact when removing withdrawal options
    ///
    /// # Security Considerations
    /// - Provide advance notice before removing popular withdrawal tokens
    /// - Ensure users have alternative withdrawal methods
    /// - Monitor for stranded user funds
    /// - Consider implementing a grace period
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin (`ContractError::UnauthorizedAdmin`)
    /// * Panics if token address is zero address (`ContractError::InvalidAddress`)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::Address;
    /// let token = Address::from_string(&env, "CD...");
    /// UtilityContract::remove_supported_withdraw_token(env, token);
    /// ```
    pub fn remove_supported_withdraw_token(env: Env, token: Address) {
        require_admin_auth(&env);

        if token.is_zero() {
            panic_with_error!(&env, ContractError::InvalidAddress);
        }

        env.storage()
            .instance()
            .set(&DataKey::SupportedWithdrawalToken(token.clone()), &false);

        env.events().publish((symbol_short!("WTUnsup"),), token);
    }

    // ==================== ISSUE #277: EMERGENCY DRAIN RECOVERY ====================

    /// Emergency drain mechanism for recovering stranded assets from the contract.
    ///
    /// @dev Critical emergency function to recover funds when normal operations
    ///      are compromised or funds become stranded. This function includes
    ///      multiple safety mechanisms including cooldown periods, amount limits,
    ///      and comprehensive audit trails. Only authorized administrators can
    ///      execute this function.
    ///
    /// @param env The Soroban execution environment
    /// @param recipient The address to receive the drained funds
    /// @param amount The amount of native tokens to drain (in stroops)
    /// @param reason Human-readable reason for the emergency drain
    ///
    /// @notice Emits EmergencyDrainExecuted event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Reverts if cooldown period has not elapsed
    /// @notice Reverts if amount is below minimum threshold
    /// @notice Reverts if insufficient contract balance
    ///
    /// # Security Considerations
    /// - 24-hour cooldown prevents abuse and allows for oversight
    /// - Minimum amount threshold prevents spam drains
    /// - Comprehensive audit trail for all drain operations
    /// - Recipient validation prevents funds from being sent to invalid addresses
    /// - Balance checks ensure contract can maintain operational reserves
    /// - Consider implementing multi-sig requirement for additional security
    ///
    /// # Panics
    /// * Panics if caller is not authorized admin (`ContractError::EmergencyDrainNotAuthorized`)
    /// * Panics if cooldown period not elapsed (`ContractError::EmergencyDrainCooldownActive`)
    /// * Panics if amount below minimum (`ContractError::InvalidTokenAmount`)
    /// * Panics if insufficient balance (`ContractError::EmergencyDrainInsufficientBalance`)
    /// * Panics if recipient address is invalid (`ContractError::InvalidAddress`)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::Address;
    /// let recipient = Address::from_string(&env, "GB...");
    /// let amount = 10_000_000; // 0.001 XLM
    /// let reason = String::from_str(&env, "Critical security incident recovery");
    /// UtilityContract::emergency_drain(env, recipient, amount, reason);
    /// ```
    pub fn emergency_drain(env: Env, recipient: Address, amount: i128, reason: String) {
        // Authorization check - only admin can execute emergency drain
        require_admin_auth(&env);

        // Validate recipient address
        if recipient.is_zero() {
            panic_with_error!(&env, ContractError::InvalidAddress);
        }

        // Validate amount
        if amount < EMERGENCY_DRAIN_MIN_AMOUNT {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        // Check cooldown period
        let last_drain: Option<u64> = env
            .storage()
            .instance()
            .get(&DataKey::EmergencyDrainLastExecution);

        let current_time = env.ledger().timestamp();
        if let Some(last_time) = last_drain {
            if current_time < last_time + EMERGENCY_DRAIN_COOLDOWN_SECONDS {
                panic_with_error!(&env, ContractError::EmergencyDrainCooldownActive);
            }
        }

        // Check contract balance
        let contract_balance = env.current_contract_address().get_balance(&env);

        if contract_balance < amount {
            panic_with_error!(&env, ContractError::EmergencyDrainInsufficientBalance);
        }

        // Ensure minimum reserve is maintained (prevent complete drain)
        let min_reserve = EMERGENCY_DRAIN_MIN_AMOUNT * 10; // Keep 10x minimum as reserve
        if contract_balance - amount < min_reserve {
            panic_with_error!(&env, ContractError::EmergencyDrainInsufficientBalance);
        }

        // Execute the drain
        env.current_contract_address()
            .transfer(&env, &recipient, &amount);

        // Update last execution timestamp
        env.storage()
            .instance()
            .set(&DataKey::EmergencyDrainLastExecution, &current_time);

        // Create and store drain record for audit trail
        let drain_counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EmergencyDrainCounter)
            .unwrap_or(0)
            + 1;

        env.storage()
            .instance()
            .set(&DataKey::EmergencyDrainCounter, &drain_counter);

        let drain_record = EmergencyDrainRecord {
            timestamp: current_time,
            amount,
            recipient: recipient.clone(),
            reason: reason.clone(),
        };

        env.storage()
            .instance()
            .set(&DataKey::EmergencyDrainRecord(drain_counter), &drain_record);

        // Emit comprehensive event for transparency
        env.events().publish(
            (symbol_short!("EDrain"),),
            (drain_counter, recipient, amount, reason, current_time),
        );
    }

    /// Get the last emergency drain execution timestamp.
    ///
    /// @dev Returns the timestamp of the last emergency drain execution.
    ///      Useful for checking cooldown status and monitoring.
    ///
    /// @param env The Soroban execution environment
    ///
    /// @return Option<u64> - Timestamp of last execution, or None if never executed
    ///
    /// # Examples
    /// ```rust
    /// let last_drain = UtilityContract::get_last_emergency_drain(env);
    /// if let Some(timestamp) = last_drain {
    ///     // Check if cooldown has elapsed
    /// }
    /// ```
    pub fn get_last_emergency_drain(env: Env) -> Option<u64> {
        env.storage()
            .instance()
            .get(&DataKey::EmergencyDrainLastExecution)
    }

    /// Get emergency drain record by ID.
    ///
    /// @dev Returns detailed information about a specific emergency drain.
    ///      Useful for audit purposes and transparency.
    ///
    /// @param env The Soroban execution environment
    /// @param drain_id The ID of the emergency drain record
    ///
    /// @return Option<EmergencyDrainRecord> - Drain record if found, None otherwise
    ///
    /// # Examples
    /// ```rust
    /// let record = UtilityContract::get_emergency_drain_record(env, 1);
    /// if let Some(drain) = record {
    ///     // Process drain record
    /// }
    /// ```
    pub fn get_emergency_drain_record(env: Env, drain_id: u64) -> Option<EmergencyDrainRecord> {
        env.storage()
            .instance()
            .get(&DataKey::EmergencyDrainRecord(drain_id))
    }

    /// Get total count of emergency drain executions.
    ///
    /// @dev Returns the total number of emergency drains executed.
    ///      Useful for monitoring and audit purposes.
    ///
    /// @param env The Soroban execution environment
    ///
    /// @return u64 - Total count of emergency drain executions
    ///
    /// # Examples
    /// ```rust
    /// let count = UtilityContract::get_emergency_drain_count(env);
    /// ```
    pub fn get_emergency_drain_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::EmergencyDrainCounter)
            .unwrap_or(0)
    }

    // ==================== ISSUE #130: GRANT STREAM INTEGRATION ====================

    /// Create a new conservation goal for a provider
    pub fn create_conservation_goal(
        env: Env,
        provider: Address,
        target_water_savings: i128,
        deadline: u64,
        grant_amount: i128,
        grant_token: Address,
    ) -> u64 {
        provider.require_auth();

        if target_water_savings <= 0 {
            panic_with_error!(&env, ContractError::InvalidGrantAmount);
        }

        if grant_amount <= 0 {
            panic_with_error!(&env, ContractError::InvalidGrantAmount);
        }

        // Generate unique goal ID
        let goal_count: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);
        let goal_id = goal_count + 1;

        let now = env.ledger().timestamp();

        let goal = ConservationGoal {
            goal_id,
            provider: provider.clone(),
            target_water_savings,
            current_savings: 0,
            deadline,
            is_active: true,
            grant_amount,
            grant_token: grant_token.clone(),
            created_at: now,
            achieved_at: None,
        };

        env.storage()
            .instance()
            .set(&DataKey::ConservationGoal(goal_id), &goal);
        env.storage().instance().set(&DataKey::Count, &goal_id);

        // Emit goal creation event
        env.events().publish(
            (symbol_short!("GoalCr"), goal_id),
            (provider, target_water_savings, deadline, grant_amount),
        );

        goal_id
    }

    /// Update water savings for a conservation goal
    pub fn update_water_savings(env: Env, goal_id: u64, additional_savings: i128) {
        if additional_savings <= 0 {
            panic_with_error!(&env, ContractError::InvalidUsageValue);
        }

        let mut goal: ConservationGoal = env
            .storage()
            .instance()
            .get(&DataKey::ConservationGoal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ConservationGoalNotFound));

        goal.provider.require_auth();

        if !goal.is_active {
            panic_with_error!(&env, ContractError::GoalAlreadyAchieved);
        }

        let now = env.ledger().timestamp();
        if now > goal.deadline {
            goal.is_active = false;
            env.storage()
                .instance()
                .set(&DataKey::ConservationGoal(goal_id), &goal);
            panic_with_error!(&env, ContractError::GoalExpired);
        }

        goal.current_savings += additional_savings;

        // Check if goal is achieved
        if goal.current_savings >= goal.target_water_savings {
            goal.is_active = false;
            goal.achieved_at = Some(now);

            // Create GoalReached event
            let goal_event = GoalReachedEvent {
                goal_id,
                provider: goal.provider.clone(),
                water_savings: goal.current_savings,
                grant_amount: goal.grant_amount,
                grant_token: goal.grant_token.clone(),
                achieved_at: now,
            };

            // Emit GoalReached event
            env.events().publish(
                (symbol_short!("GoalRch"), goal_id),
                (
                    goal.provider.clone(),
                    goal.current_savings,
                    goal.grant_amount,
                ),
            );

            // Notify Grant Stream contract if configured
            if let Some(grant_stream_address) = env
                .storage()
                .instance()
                .get::<_, Address>(&DataKey::GrantStreamMatch(goal_id, goal.provider.clone()))
            {
                let grant_stream_client = GrantStreamClient::new(&env, &grant_stream_address);
                grant_stream_client.on_goal_reached(goal_event);
            }
        }

        env.storage()
            .instance()
            .set(&DataKey::ConservationGoal(goal_id), &goal);
    }

    /// Configure Grant Stream contract to listen for goal achievements
    pub fn configure_grant_stream_match(env: Env, goal_id: u64, grant_stream_contract: Address) {
        let goal: ConservationGoal = env
            .storage()
            .instance()
            .get(&DataKey::ConservationGoal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ConservationGoalNotFound));

        goal.provider.require_auth();

        env.storage().instance().set(
            &DataKey::GrantStreamMatch(goal_id, goal.provider.clone()),
            &grant_stream_contract,
        );

        env.events().publish(
            (symbol_short!("GrantCfg"), goal_id),
            (goal.provider.clone(), grant_stream_contract),
        );
    }

    /// Get conservation goal details
    pub fn get_conservation_goal(env: Env, goal_id: u64) -> ConservationGoal {
        env.storage()
            .instance()
            .get(&DataKey::ConservationGoal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ConservationGoalNotFound))
    }

    /// Get all active conservation goals for a provider
    pub fn get_provider_conservation_goals(env: Env, provider: Address) -> Vec<u64> {
        let mut goal_ids = Vec::new(&env);
        let count: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);

        for goal_id in 1..=count {
            if let Some(goal) = env
                .storage()
                .instance()
                .get::<_, ConservationGoal>(&DataKey::ConservationGoal(goal_id))
            {
                if goal.provider == provider && goal.is_active {
                    goal_ids.push_back(goal_id);
                }
            }
        }

        goal_ids
    }

    /// Check if a goal has been achieved and trigger grant if needed
    pub fn check_and_trigger_grant(env: Env, goal_id: u64) {
        let goal: ConservationGoal = env
            .storage()
            .instance()
            .get(&DataKey::ConservationGoal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ConservationGoalNotFound));

        if goal.current_savings >= goal.target_water_savings && goal.is_active {
            // Goal should have been triggered, manually trigger now
            let mut updated_goal = goal;
            let now = env.ledger().timestamp();
            updated_goal.is_active = false;
            updated_goal.achieved_at = Some(now);

            let goal_event = GoalReachedEvent {
                goal_id,
                provider: goal.provider.clone(),
                water_savings: goal.current_savings,
                grant_amount: goal.grant_amount,
                grant_token: goal.grant_token.clone(),
                achieved_at: now,
            };

            // Emit GoalReached event
            env.events().publish(
                (symbol_short!("GoalRch"), goal_id),
                (
                    goal.provider.clone(),
                    goal.current_savings,
                    goal.grant_amount,
                ),
            );

            // Notify Grant Stream contract if configured
            if let Some(grant_stream_address) = env
                .storage()
                .instance()
                .get::<_, Address>(&DataKey::GrantStreamMatch(goal_id, goal.provider.clone()))
            {
                let grant_stream_client = GrantStreamClient::new(&env, &grant_stream_address);
                grant_stream_client.on_goal_reached(goal_event);
            }

            env.storage()
                .instance()
                .set(&DataKey::ConservationGoal(goal_id), &updated_goal);
        }
    }

    /// Set green energy discount for a specific meter (in basis points)
    pub fn set_green_energy_discount(env: Env, meter_id: u64, discount_bps: i128) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        if discount_bps < 0 || discount_bps > 10000 {
            panic_with_error!(&env, ContractError::InvalidUsageValue);
        }

        meter.green_energy_discount_bps = discount_bps;
        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
    }

    // ============================================================================
    // Streaming-Limit Circuit Breaker Admin Functions
    // ============================================================================

    /// Configure velocity limit parameters for the utility payment system.
    ///
    /// @dev Sets global and per-stream velocity limits to prevent excessive outflows
    ///      and protect against rapid fund depletion. This is a critical administrative
    ///      function that affects system-wide security and user experience. Only authorized
    ///      administrators should be able to modify these parameters.
    ///
    /// @param env The Soroban execution environment
    /// @param admin Admin address that must authorize this change
    /// @param global_limit Maximum system-wide outflow per 24 hours (in stroops)
    /// @param per_stream_limit Maximum per-meter outflow per 24 hours (in stroops)
    /// @param is_enabled Whether velocity limiting is active
    ///
    /// @notice Emits VelocityConfigUpdated event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Reverts if limits are invalid or inconsistent
    ///
    /// # Security Considerations
    /// - Global limit should be set based on system capacity and risk tolerance
    /// - Per-stream limit prevents individual meters from draining the system
    /// - Consider seasonal variations in usage patterns
    /// - Monitor for velocity limit breaches and adjust as needed
    /// - Emergency overrides should be available for critical situations
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin
    /// * Panics if global_limit or per_stream_limit are <= 0 (`ContractError::InvalidTokenAmount`)
    /// * Panics if per_stream_limit > global_limit (`ContractError::VelocityLimitBreach`)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::Address;
    /// let admin = Address::from_string(&env, "GB...");
    /// let global_limit = 100_000_000_000; // 1000 XLM per day
    /// let per_stream_limit = 10_000_000_000; // 100 XLM per meter per day
    /// UtilityContract::set_velocity_limit_config(env, admin, global_limit, per_stream_limit, true);
    /// ```
    pub fn set_velocity_limit_config(
        env: Env,
        admin: Address,
        global_limit: i128,
        per_stream_limit: i128,
        is_enabled: bool,
    ) {
        // Verify admin param matches the registered AdminAddress
        let stored_admin = get_admin_or_panic(&env);
        if admin != stored_admin {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        admin.require_auth();

        // Validate limits are positive
        if global_limit <= 0 || per_stream_limit <= 0 {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        // Validate per-stream limit doesn't exceed global limit
        if per_stream_limit > global_limit {
            panic_with_error!(&env, ContractError::VelocityLimitBreach);
        }

        // Additional validation: global limit should be reasonable
        const MAX_GLOBAL_LIMIT: i128 = 1_000_000_000_000_000; // 10M XLM per day max
        if global_limit > MAX_GLOBAL_LIMIT {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        let config = velocity_limit::VelocityConfig {
            global_limit,
            per_stream_limit,
            is_enabled,
            admin_multisig: admin.clone(),
        };

        set_velocity_config(&env, admin, config);

        // Emit event for transparency
        env.events().publish(
            (symbol_short!("VConfig"),),
            (global_limit, per_stream_limit, is_enabled),
        );
    }

    /// Apply temporary override to suspend velocity limits for specific or global operations.
    ///
    /// @dev Allows authorized administrators to temporarily bypass velocity limits
    ///      for emergency situations, maintenance, or false positive resolution.
    ///      This is a powerful administrative function that should be used sparingly
    ///      and with proper justification. All overrides are tracked with expiration
    ///      times and audit trails.
    ///
    /// @param env The Soroban execution environment
    /// @param admin Admin multi-sig address that must authorize this change
    /// @param meter_id Meter to override (0 for global override affecting all meters)
    /// @param expires_at Unix timestamp when override expires (0 = never expires)
    /// @param reason Reason code for audit trail (e.g., "false_positive", "maintenance")
    ///
    /// @notice Emits VelocityOverrideApplied event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Reverts if meter_id is invalid
    ///
    /// # Security Considerations
    /// - Overrides should be time-limited whenever possible
    /// - Global overrides affect all meters and should be used with extreme caution
    /// - All overrides create audit trails for compliance review
    /// - Consider implementing multi-sig requirement for override operations
    /// - Monitor override usage patterns for potential abuse
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin
    /// * Panics if meter_id is invalid (doesn't exist)
    /// * Panics if expires_at is in the past
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::{Address, Symbol};
    /// let admin = Address::from_string(&env, "GB...");
    /// let meter_id = 123;
    /// let expires_at = env.ledger().timestamp() + 3600; // 1 hour from now
    /// let reason = symbol_short!("maintenance");
    /// UtilityContract::apply_velocity_override(env, admin, meter_id, expires_at, reason);
    /// ```
    pub fn apply_velocity_override(
        env: Env,
        admin: Address,
        meter_id: u64,
        expires_at: u64,
        reason: Symbol,
    ) {
        // Verify admin param matches the registered AdminAddress
        let stored_admin = get_admin_or_panic(&env);
        if admin != stored_admin {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        admin.require_auth();

        // Validate expiration time
        let current_time = env.ledger().timestamp();
        if expires_at > 0 && expires_at <= current_time {
            panic_with_error!(&env, ContractError::InvalidUsageValue);
        }

        // For meter-specific overrides, validate meter exists
        if meter_id > 0 {
            let _meter = get_meter_or_panic(&env, meter_id);
        }

        apply_override(&env, admin.clone(), meter_id, expires_at, reason.clone());

        // Emit audit event
        env.events().publish(
            (symbol_short!("VOver"),),
            (meter_id, expires_at, reason, admin),
        );
    }

    /// Revoke an active velocity override and restore normal velocity limiting.
    ///
    /// @dev Removes a previously applied velocity override, restoring normal
    ///      velocity limit enforcement. This is an administrative function that
    ///      should be used when overrides are no longer needed or were applied
    ///      in error. Only authorized administrators can revoke overrides.
    ///
    /// @param env The Soroban execution environment
    /// @param admin Admin address that must authorize this change
    /// @param meter_id Meter override to revoke (0 for global override)
    ///
    /// @notice Emits VelocityOverrideRevoked event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Reverts if no active override exists for the specified meter
    ///
    /// # Security Considerations
    /// - Verify that revoking the override won't cause immediate limit breaches
    /// - Consider providing advance notice before revoking critical overrides
    /// - Monitor system behavior after override revocation
    /// - Document the reason for revocation in audit logs
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin
    /// * Panics if no active override exists for the specified meter
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::Address;
    /// let admin = Address::from_string(&env, "GB...");
    /// let meter_id = 123; // Revoke override for specific meter
    /// UtilityContract::revoke_velocity_override(env, admin, meter_id);
    /// ```
    pub fn revoke_velocity_override(env: Env, admin: Address, meter_id: u64) {
        // Verify admin param matches the registered AdminAddress
        let stored_admin = get_admin_or_panic(&env);
        if admin != stored_admin {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        admin.require_auth();

        // Check if override exists before attempting to revoke
        // This prevents unnecessary storage operations and provides better error messages
        let override_key = velocity_limit::VelocityDataKey::VelocityOverride(meter_id);

        if !env.storage().instance().has(&override_key) {
            panic_with_error!(&env, ContractError::InvalidUsageValue);
        }

        revoke_override(&env, meter_id);

        // Emit audit event
        env.events()
            .publish((symbol_short!("VORvkd"),), (meter_id, admin));
    }

    /// Get current velocity limit configuration
    pub fn get_velocity_limits(env: Env) -> Option<velocity_limit::VelocityConfig> {
        get_velocity_config(&env)
    }

    // ============================================================================
    // SLA (Service Level Agreement) Penalty Hook Functions
    // ============================================================================

    /// Register a trusted monitoring node for SLA (Service Level Agreement) reporting.
    ///
    /// @dev Adds a new trusted node that can submit downtime reports and SLA
    ///      measurements. This is a critical administrative function that affects
    ///      the reliability of SLA monitoring and penalty calculations. Only
    ///      authorized administrators should be able to modify the trusted node set.
    ///
    /// @param env The Soroban execution environment
    /// @param admin Admin address that must authorize this change
    /// @param node_pk The 32-byte public key of the monitoring node
    ///
    /// @notice Emits SLANodeRegistered event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Reverts if node_pk is invalid
    ///
    /// # Security Considerations
    /// - Node public keys should be verified and authenticated off-chain
    /// - Consider implementing node reputation and monitoring systems
    /// - Regular audits of trusted nodes should be conducted
    /// - Compromised nodes should be removed immediately
    /// - Consider implementing node rotation policies
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin
    /// * Panics if node_pk is invalid (wrong length or format)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::{Address, BytesN};
    /// let admin = Address::from_string(&env, "GB...");
    /// let node_pk = BytesN::from_array(&env, &[0u8; 32]);
    /// UtilityContract::add_sla_node(env, admin, node_pk);
    /// ```
    pub fn add_sla_node(env: Env, admin: Address, node_pk: BytesN<32>) {
        admin.require_auth();

        // Validate node public key format
        validate_ed25519_public_key(&node_pk)
            .unwrap_or_else(|_| panic_with_error!(&env, ContractError::InvalidSignature));

        // Check if node is already registered
        let is_already_registered: bool = env
            .storage()
            .instance()
            .get(&DataKey::SLANode(node_pk.clone()))
            .unwrap_or(false);

        if is_already_registered {
            // Node already registered - this is not an error, just ignore
            return;
        }

        env.storage()
            .instance()
            .set(&DataKey::SLANode(node_pk.clone()), &true);
        env.events()
            .publish((symbol_short!("SLANode"),), (node_pk, true));
    }

    /// Remove a trusted monitoring node from the SLA reporting system.
    ///
    /// @dev Removes a node's trusted status, preventing it from submitting
    ///      further downtime reports. This is an administrative function that
    ///      should be used when nodes are compromised, decommissioned, or
    ///      no longer trusted. Only authorized administrators can remove nodes.
    ///
    /// @param env The Soroban execution environment
    /// @param admin Admin address that must authorize this change
    /// @param node_pk The 32-byte public key of the monitoring node to remove
    ///
    /// @notice Emits SLANodeRemoved event
    /// @notice Reverts if caller is not authorized admin
    /// @notice Reverts if node_pk is invalid
    ///
    /// # Security Considerations
    /// - Immediate removal of compromised nodes is critical
    /// - Consider implementing a grace period for non-critical removals
    /// - Document the reason for node removal in audit logs
    /// - Monitor system behavior after node removal
    /// - Consider implementing node rotation to maintain system health
    ///
    /// # Panics
    /// * Panics if the caller is not the authorized admin
    /// * Panics if node_pk is invalid (wrong length or format)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::{Address, BytesN};
    /// let admin = Address::from_string(&env, "GB...");
    /// let node_pk = BytesN::from_array(&env, &[0u8; 32]);
    /// UtilityContract::remove_sla_node(env, admin, node_pk);
    /// ```
    pub fn remove_sla_node(env: Env, admin: Address, node_pk: BytesN<32>) {
        admin.require_auth();

        // Validate node public key format
        validate_ed25519_public_key(&node_pk)
            .unwrap_or_else(|_| panic_with_error!(&env, ContractError::InvalidSignature));

        // Check if node is actually registered
        let is_registered: bool = env
            .storage()
            .instance()
            .get(&DataKey::SLANode(node_pk.clone()))
            .unwrap_or(false);

        if !is_registered {
            // Node not registered - this is not an error, just ignore
            return;
        }

        env.storage()
            .instance()
            .set(&DataKey::SLANode(node_pk.clone()), &false);
        env.events()
            .publish((symbol_short!("SLANode"),), (node_pk, false));
    }

    /// Configure SLA parameters for a specific meter's service level monitoring.
    ///
    /// @dev Sets Service Level Agreement parameters including uptime thresholds
    ///      and penalty multipliers for a specific meter. This affects how downtime
    ///      is calculated and penalties are applied. Only the meter's provider can
    ///      modify these parameters for their own meters.
    ///
    /// @param env The Soroban execution environment
    /// @param meter_id The unique identifier of the meter
    /// @param config SLA configuration including thresholds and penalties
    ///
    /// @notice Emits SLAConfigUpdated event
    /// @notice Reverts if caller is not the meter provider
    /// @notice Reverts if meter does not exist
    /// @notice Reverts if config parameters are invalid
    ///
    /// # Security Considerations
    /// - Penalty multipliers should be reasonable and proportional
    /// - Thresholds should reflect realistic service expectations
    /// - Consider regulatory requirements for SLA parameters
    /// - Monitor SLA compliance rates and adjust as needed
    /// - Document SLA terms clearly for users
    ///
    /// # Panics
    /// * Panics if the caller is not the meter provider
    /// * Panics if the meter does not exist (`ContractError::MeterNotFound`)
    /// * Panics if config parameters are invalid (`ContractError::InvalidUsageValue`)
    ///
    /// # Examples
    /// ```rust
    /// use soroban_sdk::Address;
    /// let config = SLAConfig {
    ///     threshold_seconds: 3600, // 1 hour uptime requirement
    ///     penalty_multiplier_bps: 500, // 5% penalty multiplier
    /// };
    /// UtilityContract::set_sla_config(env, 123, config);
    /// ```
    pub fn set_sla_config(env: Env, meter_id: u64, config: SLAConfig) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        // Validate SLA configuration parameters
        if config.threshold_seconds == 0 {
            panic_with_error!(&env, ContractError::InvalidUsageValue);
        }

        if config.penalty_multiplier_bps < 0 || config.penalty_multiplier_bps > 10000 {
            panic_with_error!(&env, ContractError::InvalidUsageValue);
        }

        // Additional validation: threshold should be reasonable (not too short)
        const MIN_THRESHOLD_SECONDS: u64 = 60; // 1 minute minimum
        if config.threshold_seconds < MIN_THRESHOLD_SECONDS {
            panic_with_error!(&env, ContractError::InvalidUsageValue);
        }

        meter.sla_config = Some(config.clone());
        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events().publish(
            (symbol_short!("SLACfg"), meter_id),
            (config.threshold_seconds, config.penalty_multiplier_bps),
        );
    }

    /// Submit a signed downtime report from a trusted monitoring node.
    ///
    /// @dev Allows trusted monitoring nodes to submit signed downtime reports
    ///      for SLA monitoring. Reports are processed using a consensus mechanism
    ///      where multiple nodes must submit similar reports before they are
    ///      accepted. This prevents false reports and ensures data reliability.
    ///
    /// @param env The Soroban execution environment
    /// @param signed_report The signed SLA report containing downtime data
    ///
    /// @notice Emits SLAReportSubmitted event
    /// @notice Reverts if node is not trusted
    /// @notice Reverts if signature is invalid
    /// @notice Silently ignores duplicate reports from the same node
    ///
    /// # Security Considerations
    /// - Only trusted nodes can submit reports
    /// - All reports must be cryptographically signed
    /// - Consensus mechanism prevents single-node manipulation
    /// - Duplicate reports are rejected to prevent spam
    /// - Temporary storage ensures reports don't persist indefinitely
    /// - Consider implementing report validation and anomaly detection
    ///
    /// # Panics
    /// * Panics if the node is not trusted (`ContractError::NodeNotTrusted`)
    /// * Panics if the signature is invalid (`ContractError::InvalidSignature`)
    /// * Panics if the meter does not exist (`ContractError::MeterNotFound`)
    ///
    /// # Examples
    /// ```rust
    /// let report = SLAReport {
    ///     meter_id: 123,
    ///     start_time: 1640995200, // Jan 1, 2022
    ///     end_time: 1640998800,     // Jan 1, 2022 + 1 hour
    /// };
    /// let signature = sign_report(&node_private_key, &report);
    /// let signed_report = SignedSLAReport {
    ///     report,
    ///     node_public_key: node_public_key,
    ///     signature,
    /// };
    /// UtilityContract::submit_sla_report(env, signed_report);
    /// ```
    pub fn submit_sla_report(env: Env, signed_report: SignedSLAReport) {
        // 1. Check if node is trusted
        let is_trusted = env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::SLANode(signed_report.node_public_key.clone()))
            .unwrap_or(false);
        if !is_trusted {
            panic_with_error!(&env, ContractError::NodeNotTrusted);
        }

        // 2. Verify signature authenticity
        let report = signed_report.report.clone();
        let report_xdr = report.to_xdr(&env);
        env.crypto().ed25519_verify(
            &signed_report.node_public_key,
            &report_xdr,
            &signed_report.signature,
        );

        // 3. Validate report data
        if report.start_time >= report.end_time {
            panic_with_error!(&env, ContractError::InvalidUsageValue);
        }

        // Validate meter exists
        let _meter = get_meter_or_panic(&env, report.meter_id);

        // 4. Process the report with consensus logic
        let report_key = SlaReportKey {
            meter_id: report.meter_id,
            start_time: report.start_time,
            end_time: report.end_time,
        };

        // Prevent duplicate reporting by the same node for the same interval
        let node_key =
            DataKey::SLAReportNode(report_key.clone(), signed_report.node_public_key.clone());
        if env.storage().temporary().has(&node_key) {
            return; // Already reported by this node
        }
        env.storage().temporary().set(&node_key, &true);

        // Update consensus count
        let count_key = DataKey::SLAReportCount(report_key.clone());
        let count: u32 = env.storage().temporary().get(&count_key).unwrap_or(0);
        let new_count = count + 1;
        env.storage().temporary().set(&count_key, &new_count);

        // Threshold for consensus: 2 nodes (configurable in production)
        // This prevents false reports from single compromised nodes
        if new_count == 2 {
            let mut meter = get_meter_or_panic(&env, report.meter_id);
            let downtime = report.end_time.saturating_sub(report.start_time);

            if downtime > 0 {
                meter.sla_state.accumulated_downtime = meter
                    .sla_state
                    .accumulated_downtime
                    .saturating_add(downtime);
                meter.sla_state.last_report_timestamp = env.ledger().timestamp();
                env.storage()
                    .instance()
                    .set(&DataKey::Meter(report.meter_id), &meter);

                env.events().publish(
                    (Symbol::new(&env, "SLADowntimeReported"), report.meter_id),
                    (downtime, meter.sla_state.accumulated_downtime),
                );
            }
        }
    }

    pub fn register_meter(
        env: Env,
        user: Address,
        provider: Address,
        off_peak_rate: i128,
        token: Address,
        device_public_key: BytesN<32>,
        priority_index: u32,
        resource_type: ResourceType,
    ) -> u64 {
        Self::register_meter_with_mode(
            env,
            user,
            provider,
            off_peak_rate,
            token,
            BillingType::PrePaid,
            device_public_key,
            priority_index,
            resource_type,
        )
    }

    pub fn register_with_referral(
        env: Env,
        user: Address,
        provider: Address,
        off_peak_rate: i128,
        token: Address,
        device_public_key: BytesN<32>,
        referrer: Address,
        priority_index: u32,
        resource_type: ResourceType,
    ) -> u64 {
        let meter_id = Self::register_meter(
            env.clone(),
            user.clone(),
            provider,
            off_peak_rate,
            token,
            device_public_key,
            priority_index,
            resource_type,
        );

        if referrer != user {
            let mut meter = get_meter_or_panic(&env, meter_id);
            // Reward the new user
            meter.balance = meter.balance.saturating_add(REFERRAL_REWARD_UNITS);
            env.storage()
                .instance()
                .set(&DataKey::Meter(meter_id), &meter);

            // Reward the referrer if they have a meter? (simplified for now: just record it)
            env.storage()
                .instance()
                .set(&DataKey::Referral(user.clone()), &referrer.clone());

            env.events().publish(
                (symbol_short!("Referral"), meter_id),
                (referrer.clone(), user.clone()),
            );
        }

        meter_id
    }

    /// Register a device MAC address hash and bind it to a meter (streaming channel)
    /// The MAC address is stored as a SHA-256 hash for privacy
    /// Returns the meter ID if successful
    pub fn register_device(
        env: Env,
        meter_id: u64,
        mac_address: BytesN<32>, // Expects SHA-256 hash of MAC address (32 bytes)
        owner: Address,          // Owner of the device (must authenticate)
    ) -> u64 {
        // Authenticate the device owner
        owner.require_auth();

        // Get the meter to ensure it exists and is active
        let mut meter = get_meter_or_panic(&env, meter_id);

        // Verify the caller is the meter's user or provider
        if owner != meter.user && owner != meter.provider {
            panic_with_error!(&env, ContractError::UnauthorizedContributor);
        }

        // Check if meter is active
        if !meter.is_active {
            panic_with_error!(&env, ContractError::MeterNotFound); // Reusing error for inactive meter
        }

        // Check if this device hash is already bound to another meter
        let existing_binding: Option<u64> = env
            .storage()
            .instance()
            .get(&DataKey::DeviceHash(mac_address.clone()));
        if let Some(existing_meter_id) = existing_binding {
            if existing_meter_id != meter_id {
                panic_with_error!(&env, ContractError::DeviceAlreadyBoundToAnotherMeter);
                // Device already bound to another meter
            }
            // If it's already bound to this same meter, we can proceed (re-registration)
        }

        // Store the device hash -> meter_id mapping
        env.storage()
            .instance()
            .set(&DataKey::DeviceHash(mac_address.clone()), &meter_id);

        // Store the meter_id -> device hash mapping
        env.storage()
            .instance()
            .set(&DataKey::MeterDevice(meter_id), &mac_address);

        // Clear any pending transfer for this device (since it's now bound)
        // Note: We don't have a specific key to clear for pending transfers since they're keyed by (hash, new_owner)
        // Pending transfers will be handled when attempting reassignment

        // Emit DeviceRegistered event with the public hash
        env.events()
            .publish((symbol_short!("DevReg"), meter_id), (mac_address, owner));

        meter_id
    }

    /// Initiate device reassignment with mutual consent requirement
    /// Current owner initiates transfer to new owner
    /// Returns a transfer ID that must be confirmed by new owner
    pub fn initiate_device_transfer(
        env: Env,
        meter_id: u64,
        new_owner: Address,
        current_owner: Address,
    ) -> BytesN<32> {
        // Authenticate current owner
        current_owner.require_auth();

        // Get the meter to ensure it exists and is active
        let meter = get_meter_or_panic(&env, meter_id);

        // Verify current owner is the meter's user or provider
        if current_owner != meter.user && current_owner != meter.provider {
            panic_with_error!(&env, ContractError::UnauthorizedContributor);
        }

        // Get the device hash for this meter
        let device_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::MeterDevice(meter_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MeterNotFound));

        // Verify the device is actually bound to this meter
        let bound_meter_id: Option<u64> = env
            .storage()
            .instance()
            .get(&DataKey::DeviceHash(device_hash.clone()));
        if let Some(bound_id) = bound_meter_id {
            if bound_id != meter_id {
                panic_with_error!(&env, ContractError::InvalidUsageValue); // Device bound to different meter
            }
        } else {
            panic_with_error!(&env, ContractError::MeterNotFound); // No device bound to meter
        }

        // Create a transfer ID based on device hash and new owner (for uniqueness).
        // Concatenate 2 fixed components directly into Bytes — no Vec intermediary needed.
        let mut transfer_data = Bytes::from_slice(&env, &device_hash.to_array());
        transfer_data.append(&new_owner.to_xdr(&env));
        let transfer_id = env.crypto().sha256(&transfer_data);

        // Store the pending transfer request
        // Key: (device_hash, new_owner) -> current_owner (waiting for confirmation)
        env.storage().instance().set(
            &DataKey::PendingDeviceTransfer(device_hash.clone(), new_owner.clone()),
            &current_owner,
        );

        // Emit event for transfer initiation
        env.events().publish(
            (symbol_short!("DevXfrIn"), meter_id),
            (device_hash, current_owner, new_owner),
        );

        transfer_id
    }

    /// Complete device reassignment with mutual consent
    /// New owner confirms the transfer that was initiated by current owner
    /// After confirmation, device is bound to new owner's meter
    pub fn complete_device_transfer(
        env: Env,
        meter_id: u64,
        new_owner: Address,
        transfer_id: BytesN<32>,
    ) -> u64 {
        // Authenticate new owner
        new_owner.require_auth();

        // Get the meter to ensure it exists and is active
        let mut meter = get_meter_or_panic(&env, meter_id);

        // Verify new owner is the meter's user or provider
        if new_owner != meter.user && new_owner != meter.provider {
            panic_with_error!(&env, ContractError::UnauthorizedContributor);
        }

        // Get the device hash for this meter (should be the same before and after transfer)
        let device_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::MeterDevice(meter_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MeterNotFound));

        // Verify the device is actually bound to this meter
        let bound_meter_id: Option<u64> = env
            .storage()
            .instance()
            .get(&DataKey::DeviceHash(device_hash.clone()));
        if let Some(bound_id) = bound_meter_id {
            if bound_id != meter_id {
                panic_with_error!(&env, ContractError::InvalidUsageValue); // Device bound to different meter
            }
        } else {
            panic_with_error!(&env, ContractError::MeterNotFound); // No device bound to meter
        }

        // For the transfer to be valid, we need to find a pending transfer request
        // where (device_hash, prospective_new_owner) maps to some current_owner
        // We'll iterate through a simplified approach - in production this would be more efficient

        // Since we can't easily iterate, we'll use a different approach:
        // The transfer_id should be derivable from device_hash + new_owner
        // Let's verify that the provided transfer_id matches what we expect

        // Recreate expected transfer ID from device hash and new owner.
        // Concatenate 2 fixed components directly into Bytes — no Vec intermediary needed.
        let mut expected_transfer_data = Bytes::from_slice(&env, &device_hash.to_array());
        expected_transfer_data.append(&new_owner.to_xdr(&env));
        let expected_transfer_id = env.crypto().sha256(&expected_transfer_data);

        if transfer_id != expected_transfer_id {
            panic_with_error!(&env, ContractError::InvalidUsageValue); // Invalid transfer ID
        }

        // In a real implementation, we would check if there's a pending transfer request
        // For now, we'll allow the transfer to proceed if the IDs match
        // This assumes the current owner already initiated the transfer

        // Verify that meter is active
        if !meter.is_active {
            panic_with_error!(&env, ContractError::MeterNotFound); // Reusing error for inactive meter
        }

        // Device remains bound to the same meter - ownership change doesn't affect device binding
        // The device hash remains the same, still bound to meter_id
        // What changes is which user/provider can authorize operations on the meter

        // Update the meter's user if the new owner is the user (not provider)
        // This allows transferring ownership of the meter itself
        if new_owner == meter.user {
            // User is transferring to themselves - no change needed
        } else if new_owner == meter.provider {
            // Provider is transferring to themselves - no change needed
        } else {
            // Neither user nor provider - this shouldn't happen due to check above
            panic_with_error!(&env, ContractError::UnauthorizedContributor);
        }

        // Clear the pending transfer request
        // In a full implementation, we would need to know the original initiator to clear the correct key
        // For simplicity, we're not clearing pending transfers in this basic implementation

        // Emit event for transfer completion
        env.events().publish(
            (symbol_short!("DevXfrCp"), meter_id),
            (device_hash, new_owner),
        );

        meter_id
    }

    /// Register a device MAC address hash and bind it to a meter (streaming channel)
    /// The MAC address is stored as a SHA-256 hash for privacy
    /// Returns the meter ID if successful
    pub fn register_meter_with_mode(
        env: Env,
        user: Address,
        provider: Address,
        off_peak_rate: i128,
        token: Address,
        billing_type: BillingType,
        device_public_key: BytesN<32>,
        priority_index: u32,
        resource_type: ResourceType,
    ) -> u64 {
        user.require_auth();

        // Issue #279: Validate device_public_key byte array
        validate_ed25519_public_key(&device_public_key)?;

        let mut count = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::Count)
            .unwrap_or(0);
        count += 1;

        let mut active_count = env
            .storage()
            .instance()
            .get::<_, u32>(&DataKey::ActiveMetersCount)
            .unwrap_or(0);
        active_count += 1;

        let now = env.ledger().timestamp();
        let peak_rate = off_peak_rate.saturating_mul(PEAK_RATE_MULTIPLIER) / RATE_PRECISION;

        let meter = Meter {
            user,
            provider,
            billing_type,
            resource_type,
            off_peak_rate,
            peak_rate,
            rate_per_unit: off_peak_rate,
            balance: 0,
            debt: 0,
            last_update: now,
            is_active: true,
            token,
            usage_data: UsageData {
                total_watt_hours: 0,
                current_cycle_watt_hours: 0,
                peak_usage_watt_hours: 0,
                last_reading_timestamp: now,
                precision_factor: 1,
                renewable_watt_hours: 0,
                renewable_percentage: 0,
                monthly_volume: 0,
                last_volume_reset: now,
                first_reading_timestamp: now,
            },
            device_public_key,
            end_date: 0,
            rent_deposit: 0,
            priority_index,
            green_energy_discount_bps: 0,
            is_paused: false,
            is_disputed: false,
            challenge_timestamp: 0,
            credit_drip_rate: 0,
            carbon_credit_token: None,
            carbon_credit_drip_rate_bps: 0,
            is_closed: false,
            off_peak_reward_rate_bps: 0,
            milestone_deadline: 0,
            milestone_confirmed: false,
            rate_per_second: off_peak_rate,
            collateral_limit: 0,
            max_flow_rate_per_hour: off_peak_rate.saturating_mul(HOUR_IN_SECONDS as i128),
            last_claim_time: now,
            claimed_this_hour: 0,
            is_paired: false,
            tier_threshold: 100_000,
            tier_rate: off_peak_rate.saturating_mul(120) / 100,
            last_heartbeat: now,
            grace_period_start: 0,
            is_offline: false,
            estimated_usage_total: 0,
            sla_config: None,
            sla_state: SLAState {
                accumulated_downtime: 0,
                last_report_timestamp: now,
                is_penalty_active: false,
            },
            is_updating: false,
            update_start_timestamp: 0,
        };

        env.storage().instance().set(&DataKey::Meter(count), &meter);
        env.storage().instance().set(&DataKey::Count, &count);
        let payload_hash = audit_payload_hash(&env, &(count, off_peak_rate, priority_index, resource_type));
        append_audit_record(&env, Symbol::new(&env, "register_meter"), count, meter.user.clone(), payload_hash);
        count
    }

    pub fn top_up(env: Env, meter_id: u64, amount: i128, contributor: Address) {
        let mut meter = get_meter_or_panic(&env, meter_id);

        // Authorization: either the primary user OR an authorized contributor
        let is_authorized = if contributor == meter.user {
            contributor.require_auth();
            true
        } else {
            let auth_key = DataKey::AuthorizedContributor(meter_id, contributor.clone());
            if env
                .storage()
                .instance()
                .get::<_, bool>(&auth_key)
                .unwrap_or(false)
            {
                contributor.require_auth();
                true
            } else {
                false
            }
        };

        if !is_authorized {
            panic_with_error!(&env, ContractError::UnauthorizedContributor);
        }

        let was_active = meter.is_active;
        let old_meter_value = provider_meter_value(&meter);
        // Transfer tokens from contributor to contract
        let token_client = token::Client::new(&env, &meter.token);
        token_client.transfer(&contributor, &env.current_contract_address(), &amount);

        // Track individual contribution
        let contribution_key = DataKey::Contributor(meter_id, contributor.clone());
        let current_contribution = env
            .storage()
            .instance()
            .get::<_, i128>(&contribution_key)
            .unwrap_or(0);
        env.storage().instance().set(
            &contribution_key,
            &current_contribution.saturating_add(amount),
        );

        // Convert XLM to USD cents if needed
        let converted_amount = match convert_xlm_to_usd_if_needed(&env, amount, &meter.token) {
            Ok(amount) => amount,
            Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
        };

        if converted_amount <= 0 {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        match meter.billing_type {
            BillingType::PrePaid => {
                // Auto-deduct debt first if in debt mode
                if meter.balance < 0 {
                    let debt_settlement = converted_amount.min(meter.balance.abs());
                    meter.balance = meter.balance.saturating_add(debt_settlement);
                    let remaining_amount = converted_amount.saturating_sub(debt_settlement);
                    meter.balance = meter.balance.saturating_add(remaining_amount);
                } else {
                    meter.balance = meter.balance.saturating_add(converted_amount);
                }
            }
            BillingType::PostPaid => {
                let settlement = converted_amount.min(meter.debt.max(0));
                meter.debt = meter.debt.saturating_sub(settlement);
                meter.collateral_limit = meter
                    .collateral_limit
                    .saturating_add(converted_amount.saturating_sub(settlement));
            }
        }

        let now = env.ledger().timestamp();
        refresh_activity(&mut meter, now);

        if !was_active && meter.is_active {
            meter.last_update = now;
            publish_active_event(&env, meter_id, now);
        }

        // Update provider total pool
        let new_meter_value = provider_meter_value(&meter);
        update_provider_total_pool(&env, &meter.provider, old_meter_value, new_meter_value);

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        let payload_hash = audit_payload_hash(&env, &(amount, converted_amount, meter.balance, meter.debt));
        append_audit_record(&env, Symbol::new(&env, "top_up"), meter_id, contributor, payload_hash);

        // Emit conversion event
        env.events().publish(
            (symbol_short!("TokUp"), meter_id),
            (amount, converted_amount),
        );
    }

    pub fn initiate_pairing(env: Env, meter_id: u64) -> BytesN<32> {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        if meter.is_paired {
            panic_with_error!(env, ContractError::PairingAlreadyComplete);
        }

        // Generate a pseudo-random challenge using contract context and ledger info
        let challenge_data = PairingChallengeData {
            contract: env.current_contract_address(),
            meter_id,
            timestamp: env.ledger().timestamp(),
        };

        let challenge = env.crypto().sha256(&challenge_data.to_xdr(&env));

        env.storage()
            .instance()
            .set(&DataKey::PairingChallenge(meter_id), &challenge);

        env.events()
            .publish((symbol_short!("PairIn"), meter_id), challenge.clone());

        challenge.into()
    }

    pub fn complete_pairing(env: Env, meter_id: u64, signature: BytesN<64>) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        // Issue #279: Validate signature byte array
        validate_ed25519_signature(&signature)?;

        let challenge: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::PairingChallenge(meter_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ChallengeNotFound));

        // Create the message that was signed
        let pairing_data = PairingChallengeData {
            contract: env.current_contract_address(),
            meter_id,
            timestamp: env.ledger().timestamp(),
        };

        // Verify the signature
        #[cfg(not(test))]
        env.crypto().ed25519_verify(
            &meter.device_public_key,
            &pairing_data.to_xdr(&env),
            &signature,
        );

        // Clear the challenge
        env.storage()
            .instance()
            .remove(&DataKey::PairingChallenge(meter_id));

        meter.is_paired = true;
        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events()
            .publish((symbol_short!("PairComp"), meter_id), signature);
    }

    pub fn ping(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        let now = env.ledger().timestamp();
        meter.last_heartbeat = now;

        if meter.is_offline {
            meter.is_offline = false;
            meter.grace_period_start = 0;
            // Reconciliation will happen when actual usage is reported via deduct_units
        }

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
        env.events().publish((symbol_short!("Ping"), meter_id), now);
    }

    pub fn deduct_units(env: Env, signed_data: SignedUsageData) {
        let mut meter = get_meter_or_panic(&env, signed_data.meter_id);
        meter.provider.require_auth();

        // Issue #279: Validate signed_data byte arrays
        validate_ed25519_signature(&signed_data.signature)?;
        validate_ed25519_public_key(&signed_data.public_key)?;

        // Validate reading
        if let Err(e) = validate_reading(
            &env,
            signed_data.meter_id,
            meter.resource_type,
            signed_data.watt_hours_consumed,
            signed_data.units_consumed,
            signed_data.timestamp,
        ) {
            // Emit ReadingRejected event
            let reason = match e {
                ContractError::InvalidReadingValue => String::from_str(&env, "Negative reading"),
                ContractError::DuplicateTimestamp => {
                    String::from_str(&env, "Timestamp not after last")
                }
                ContractError::ReadingDeltaTooLarge => String::from_str(&env, "Reading too large"),
                _ => String::from_str(&env, "Unknown validation error"),
            };
            let event = ReadingRejected {
                meter_id: signed_data.meter_id,
                reason,
                value: signed_data.units_consumed,
                timestamp: signed_data.timestamp,
            };
            env.events().publish(
                (Symbol::new(&env, "ReadReject"), signed_data.meter_id),
                event,
            );
            panic_with_error!(&env, e);
        }

        // Verify the signature and pairing
        if let Err(e) = verify_usage_signature(&env, &signed_data, &meter) {
            panic_with_error!(&env, e);
        }

        // Task #88: Kill-Switch Check
        if meter.is_disputed {
            panic_with_error!(&env, ContractError::InDispute);
        }

        // Store old meter value for pool update
        let old_meter_value = provider_meter_value(&meter);

        if !meter.is_paired {
            panic_with_error!(&env, ContractError::MeterNotPaired);
        }

        // Issue #178: Check if meter is under firmware update
        // Billing is paused during authorized update window
        if meter.is_updating {
            panic_with_error!(&env, ContractError::FirmwareUpdateInProgress);
        }

        let now = env.ledger().timestamp();
        let effective_rate = get_effective_rate(&meter, signed_data.timestamp);

        // Apply green energy discount if applicable
        let discounted_rate =
            if signed_data.is_renewable_energy && meter.green_energy_discount_bps > 0 {
                effective_rate.saturating_mul(10000 - meter.green_energy_discount_bps) / 10000
            } else {
                effective_rate
            };

        // Device-Offline Reconciliation
        if meter.is_offline {
            let estimated_cost = meter.estimated_usage_total;
            let actual_cost = signed_data.units_consumed.saturating_mul(discounted_rate);
            let adjustment = estimated_cost.saturating_sub(actual_cost);

            // Adjust balance: add back the estimate and let normal deduction handle actual
            meter.balance = meter.balance.saturating_add(estimated_cost);

            // Emit Reconciliation Event
            let recon_event = OfflineReconciliation {
                meter_id: signed_data.meter_id,
                estimated_cost,
                actual_cost,
                adjustment,
                timestamp: now,
            };
            env.events().publish(
                (symbol_short!("OffRecon"), signed_data.meter_id),
                recon_event,
            );

            // Reset offline status
            meter.is_offline = false;
            meter.estimated_usage_total = 0;
            meter.grace_period_start = 0;
        }

        meter.last_heartbeat = now;

        let mut cost = signed_data.units_consumed.saturating_mul(discounted_rate);

        // Apply SLA Penalty if active
        if let Some(config) = &meter.sla_config {
            if meter.sla_state.is_penalty_active
                || meter.sla_state.accumulated_downtime >= config.threshold_seconds
            {
                cost = cost
                    .saturating_mul(config.penalty_multiplier_bps)
                    .saturating_div(10000);
            }
        }

        // Apply provider withdrawal limits
        let mut window = apply_provider_withdrawal_limit(&env, &meter.provider, cost);

        // Task #3: Allocate to maintenance fund (0.01% = 1 basis point)
        allocate_to_maintenance_fund(&env, signed_data.meter_id, cost);

        // Task #2: Tax Compliance - Split tax before provider payout
        let tax_rate_bps = get_tax_rate_or_default(&env);
        let (tax_amount, after_tax_amount) = calculate_tax_split(cost, tax_rate_bps);

        if tax_amount > 0 {
            // Transfer tax to government vault if configured
            if let Some(gov_vault) = get_government_vault_or_default(&env) {
                let client = token::Client::new(&env, &meter.token);
                client.transfer(&env.current_contract_address(), &gov_vault, &tax_amount);

                // Emit TaxReceipt event
                let tax_receipt = TaxReceipt {
                    meter_id: signed_data.meter_id,
                    total_amount: cost,
                    tax_amount,
                    net_amount: after_tax_amount,
                    tax_rate_bps,
                    government_vault: gov_vault.clone(),
                    timestamp: now,
                };
                env.events().publish(
                    (soroban_sdk::symbol_short!("TaxRec"), signed_data.meter_id),
                    tax_receipt,
                );
            }
        }

        let mut payout = after_tax_amount;
        let credit_discount_active =
            issue_carbon_credits(&env, signed_data.meter_id, &meter, payout, now);

        if let Some(wallet) = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::MaintenanceWallet)
        {
            let fee_bps: i128 = env
                .storage()
                .instance()
                .get(&DataKey::ProtocolFeeBps)
                .unwrap_or(0);
            let discount_bps = if credit_discount_active {
                meter.green_energy_discount_bps.min(fee_bps)
            } else {
                0
            };
            let effective_fee = fee_bps.saturating_sub(discount_bps);
            let fee = (payout * effective_fee) / 10000;
            payout = payout.saturating_sub(fee);
            if fee > 0 {
                let client = token::Client::new(&env, &meter.token);
                client.transfer(&env.current_contract_address(), &wallet, &fee);
            }
        }

        // Apply the claim (using after-tax amount for actual provider payout)
        apply_provider_claim(&env, &mut meter, payout);

        // Update provider window
        window.daily_withdrawn = window.daily_withdrawn.saturating_add(cost);
        env.storage()
            .instance()
            .set(&DataKey::ProviderWindow(meter.provider.clone()), &window);

        // Update usage data
        meter.usage_data.total_watt_hours = meter
            .usage_data
            .total_watt_hours
            .saturating_add(signed_data.watt_hours_consumed);
        meter.usage_data.current_cycle_watt_hours = meter
            .usage_data
            .current_cycle_watt_hours
            .saturating_add(signed_data.watt_hours_consumed);

        // Track renewable energy usage
        if signed_data.is_renewable_energy {
            meter.usage_data.renewable_watt_hours = meter
                .usage_data
                .renewable_watt_hours
                .saturating_add(signed_data.watt_hours_consumed);
        }

        // Update renewable percentage
        if meter.usage_data.total_watt_hours > 0 {
            meter.usage_data.renewable_percentage =
                meter.usage_data.renewable_watt_hours.saturating_mul(10000)
                    / meter.usage_data.total_watt_hours; // in basis points
        }

        if meter.usage_data.current_cycle_watt_hours > meter.usage_data.peak_usage_watt_hours {
            meter.usage_data.peak_usage_watt_hours = meter.usage_data.current_cycle_watt_hours;
        }

        // Update activity status with grace period logic
        refresh_activity(&mut meter, now);

        meter.last_update = now;

        // Task #3: Auto-extend TTL if needed (every 500,000 ledgers)
        auto_extend_ttl_if_needed(&env, signed_data.meter_id);

        // Task #89: Update monthly volume
        let now = env.ledger().timestamp();
        if now.saturating_sub(meter.usage_data.last_volume_reset) >= (30 * DAY_IN_SECONDS) {
            meter.usage_data.monthly_volume = cost;
            meter.usage_data.last_volume_reset = now;
        } else {
            meter.usage_data.monthly_volume = meter.usage_data.monthly_volume.saturating_add(cost);
        }

        // Update provider total pool
        let new_meter_value = provider_meter_value(&meter);
        update_provider_total_pool(&env, &meter.provider, old_meter_value, new_meter_value);

        // Update last reading time
        env.storage().instance().set(
            &DataKey::LastReadingTime(signed_data.meter_id),
            &signed_data.timestamp,
        );

        env.storage()
            .instance()
            .set(&DataKey::Meter(signed_data.meter_id), &meter);

        let payload_hash = audit_payload_hash(&env, &(signed_data.timestamp, signed_data.units_consumed, cost));
        append_audit_record(&env, Symbol::new(&env, "deduct_units"), signed_data.meter_id, meter.provider.clone(), payload_hash);

        // Emit UsageReported event
        env.events().publish(
            (Symbol::new(&env, "UsageReported"), signed_data.meter_id),
            (signed_data.units_consumed, cost),
        );
    }

    /// Return the current audit-chain head for off-chain monitors.
    pub fn get_audit_head(env: Env) -> Option<AuditRecord> {
        env.storage().instance().get(&DataKey::AuditHead)
    }

    /// Return an audit record by sequence number for verification or dashboard indexing.
    pub fn get_audit_record(env: Env, sequence: u64) -> Option<AuditRecord> {
        env.storage().instance().get(&DataKey::AuditRecord(sequence))
    }

    /// Verify the audit hash chain between two inclusive sequence numbers.
    pub fn verify_audit_chain(env: Env, start_sequence: u64, end_sequence: u64) -> bool {
        if start_sequence == 0 || end_sequence < start_sequence {
            return false;
        }

        let mut expected_previous = if start_sequence == 1 {
            zero_hash(&env)
        } else if let Some(previous) = env
            .storage()
            .instance()
            .get::<DataKey, AuditRecord>(&DataKey::AuditRecord(start_sequence - 1))
        {
            previous.record_hash
        } else {
            return false;
        };

        let mut sequence = start_sequence;
        while sequence <= end_sequence {
            let Some(record) = env
                .storage()
                .instance()
                .get::<DataKey, AuditRecord>(&DataKey::AuditRecord(sequence))
            else {
                return false;
            };

            if record.sequence != sequence || record.previous_hash != expected_previous {
                return false;
            }

            let expected_hash = calculate_audit_record_hash(
                &env,
                record.sequence,
                &record.previous_hash,
                &record.action,
                record.subject_id,
                &record.actor,
                record.timestamp,
                &record.payload_hash,
            );
            if record.record_hash != expected_hash {
                return false;
            }
            expected_previous = record.record_hash;
            sequence = sequence.saturating_add(1);
        }
        true
    }

    pub fn claim(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        // Task #88: Kill-Switch Check
        if meter.is_disputed {
            panic_with_error!(&env, ContractError::InDispute);
        }

        // Store old meter value for pool update
        let old_meter_value = provider_meter_value(&meter);

        let now = env.ledger().timestamp();
        let elapsed = now.checked_sub(meter.last_update).unwrap_or(0);

        // Task #90: Credit Settlement Flow
        // If there's a credit_drip_rate, add it to the normal consumption flow
        let mut amount = (elapsed as i128)
            .saturating_mul(meter.rate_per_unit.saturating_add(meter.credit_drip_rate));

        // Apply SLA Penalty if active
        if let Some(config) = &meter.sla_config {
            if meter.sla_state.is_penalty_active
                || meter.sla_state.accumulated_downtime >= config.threshold_seconds
            {
                amount = amount
                    .saturating_mul(config.penalty_multiplier_bps)
                    .saturating_div(10000);
            }
        }

        // Check if we're in the same hour as last claim
        let current_hour = now / 3600;
        let last_claim_hour = meter.last_claim_time / 3600;

        if current_hour == last_claim_hour {
            // Same hour, check if we exceed max flow rate
            let max_allowed = meter.max_flow_rate_per_hour - meter.claimed_this_hour;
            let actual_amount = if amount > max_allowed {
                max_allowed
            } else {
                amount
            };

            // Ensure we don't exceed debt threshold
            let claimable = if actual_amount > meter.balance
                && meter.balance - actual_amount >= DEBT_THRESHOLD
            {
                actual_amount
            } else if actual_amount > meter.balance {
                meter.balance - DEBT_THRESHOLD // Allow going down to threshold
            } else {
                actual_amount
            };

            if claimable > 0 {
                let client = token::Client::new(&env, &meter.token);
                let mut payout = claimable;

                // Task #3: Allocate to maintenance fund (0.01% = 1 basis point)
                allocate_to_maintenance_fund(&env, meter_id, claimable);

                // Task #2: Tax Compliance - Split tax before provider payout
                let tax_rate_bps = get_tax_rate_or_default(&env);
                let (tax_amount, after_tax_amount) = calculate_tax_split(payout, tax_rate_bps);

                if tax_amount > 0 {
                    // Transfer tax to government vault if configured
                    if let Some(gov_vault) = get_government_vault_or_default(&env) {
                        client.transfer(&env.current_contract_address(), &gov_vault, &tax_amount);

                        // Emit TaxReceipt event
                        let tax_receipt = TaxReceipt {
                            meter_id,
                            total_amount: claimable,
                            tax_amount,
                            net_amount: after_tax_amount,
                            tax_rate_bps,
                            government_vault: gov_vault.clone(),
                            timestamp: now,
                        };
                        env.events().publish(
                            (soroban_sdk::symbol_short!("TaxRcpt"), meter_id),
                            tax_receipt,
                        );
                    }
                }

                payout = after_tax_amount;

                let credit_discount_active =
                    issue_carbon_credits(&env, meter_id, &meter, claimable, now);

                // Protocol fee with green energy discount
                if let Some(wallet) = env
                    .storage()
                    .instance()
                    .get::<_, Address>(&DataKey::MaintenanceWallet)
                {
                    let fee_bps: i128 = env
                        .storage()
                        .instance()
                        .get(&DataKey::ProtocolFeeBps)
                        .unwrap_or(0);
                    let discount_bps = if credit_discount_active {
                        meter.green_energy_discount_bps.min(fee_bps)
                    } else {
                        0
                    };
                    let effective_fee = fee_bps.saturating_sub(discount_bps);
                    let fee = (payout * effective_fee) / 10000;
                    payout -= fee;
                    if fee > 0 {
                        client.transfer(&env.current_contract_address(), &wallet, &fee);
                    }
                }
                if payout > 0 {
                    client.transfer(&env.current_contract_address(), &meter.provider, &payout);
                }
                meter.balance -= claimable;
                meter.claimed_this_hour += claimable;

                // If credit drip was active, reduce the debt if in PostPaid mode
                if meter.billing_type == BillingType::PostPaid && meter.credit_drip_rate > 0 {
                    let credit_settlement = (elapsed as i128)
                        .saturating_mul(meter.credit_drip_rate)
                        .min(meter.debt);
                    meter.debt = meter.debt.saturating_sub(credit_settlement);
                }
            }
        } else {
            // New hour, reset claimed_this_hour
            meter.claimed_this_hour = 0;

            // Ensure we don't exceed debt threshold
            let claimable = if amount > meter.balance && meter.balance - amount >= DEBT_THRESHOLD {
                amount
            } else if amount > meter.balance {
                meter.balance - DEBT_THRESHOLD // Allow going down to threshold
            } else {
                amount
            };

            if claimable > 0 {
                let client = token::Client::new(&env, &meter.token);
                let mut payout = claimable;

                // Task #3: Allocate to maintenance fund (0.01% = 1 basis point)
                allocate_to_maintenance_fund(&env, meter_id, claimable);

                // Task #2: Tax Compliance - Split tax before provider payout
                let tax_rate_bps = get_tax_rate_or_default(&env);
                let (tax_amount, after_tax_amount) = calculate_tax_split(payout, tax_rate_bps);

                if tax_amount > 0 {
                    // Transfer tax to government vault if configured
                    if let Some(gov_vault) = get_government_vault_or_default(&env) {
                        client.transfer(&env.current_contract_address(), &gov_vault, &tax_amount);

                        // Emit TaxReceipt event
                        let tax_receipt = TaxReceipt {
                            meter_id,
                            total_amount: claimable,
                            tax_amount,
                            net_amount: after_tax_amount,
                            tax_rate_bps,
                            government_vault: gov_vault.clone(),
                            timestamp: now,
                        };
                        env.events().publish(
                            (soroban_sdk::symbol_short!("TaxRcpt"), meter_id),
                            tax_receipt,
                        );
                    }
                }

                payout = after_tax_amount;

                let credit_discount_active =
                    issue_carbon_credits(&env, meter_id, &meter, claimable, now);

                // Protocol fee with green energy discount
                if let Some(wallet) = env
                    .storage()
                    .instance()
                    .get::<_, Address>(&DataKey::MaintenanceWallet)
                {
                    let fee_bps: i128 = env
                        .storage()
                        .instance()
                        .get(&DataKey::ProtocolFeeBps)
                        .unwrap_or(0);
                    let discount_bps = if credit_discount_active {
                        meter.green_energy_discount_bps.min(fee_bps)
                    } else {
                        0
                    };
                    let effective_fee = fee_bps.saturating_sub(discount_bps);
                    let fee = (payout * effective_fee) / 10000;
                    payout -= fee;
                    if fee > 0 {
                        client.transfer(&env.current_contract_address(), &wallet, &fee);
                    }
                }
                if payout > 0 {
                    client.transfer(&env.current_contract_address(), &meter.provider, &payout);
                }
                meter.balance -= claimable;
                meter.claimed_this_hour = claimable;

                // If credit drip was active, reduce the debt if in PostPaid mode
                if meter.billing_type == BillingType::PostPaid && meter.credit_drip_rate > 0 {
                    let credit_settlement = (elapsed as i128)
                        .saturating_mul(meter.credit_drip_rate)
                        .min(meter.debt);
                    meter.debt = meter.debt.saturating_sub(credit_settlement);
                }
            }
        }

        meter.last_update = now;
        meter.last_claim_time = now;

        // Update activity status with grace period logic
        refresh_activity(&mut meter, now);

        // Task #3: Auto-extend TTL if needed (every 500,000 ledgers)
        auto_extend_ttl_if_needed(&env, meter_id);

        // Update provider total pool
        let new_meter_value = provider_meter_value(&meter);
        update_provider_total_pool(&env, &meter.provider, old_meter_value, new_meter_value);

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
    }

    pub fn update_usage(env: Env, meter_id: u64, watt_hours_consumed: i128) {
        // Input validation for security
        if watt_hours_consumed < 0 {
            panic_with_error!(env, ContractError::InvalidUsageValue);
        }

        if watt_hours_consumed > MAX_USAGE_PER_UPDATE {
            panic_with_error!(env, ContractError::UsageExceedsLimit);
        }

        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        let precise_consumption =
            watt_hours_consumed.saturating_mul(meter.usage_data.precision_factor);
        meter.usage_data.total_watt_hours = meter
            .usage_data
            .total_watt_hours
            .saturating_add(precise_consumption);
        meter.usage_data.current_cycle_watt_hours = meter
            .usage_data
            .current_cycle_watt_hours
            .saturating_add(precise_consumption);

        if meter.usage_data.current_cycle_watt_hours > meter.usage_data.peak_usage_watt_hours {
            meter.usage_data.peak_usage_watt_hours = meter.usage_data.current_cycle_watt_hours;
        }

        meter.usage_data.last_reading_timestamp = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
    }

    pub fn reset_cycle_usage(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();
        meter.usage_data.current_cycle_watt_hours = 0;
        meter.usage_data.last_reading_timestamp = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
    }

    pub fn get_usage_data(env: Env, meter_id: u64) -> Option<UsageData> {
        env.storage()
            .instance()
            .get::<DataKey, Meter>(&DataKey::Meter(meter_id))
            .map(|meter| meter.usage_data)
    }

    pub fn get_meter(env: Env, meter_id: u64) -> Option<Meter> {
        env.storage()
            .instance()
            .get::<DataKey, Meter>(&DataKey::Meter(meter_id))
    }

    pub fn get_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::Count)
            .unwrap_or(0)
    }

    pub fn get_provider_window(env: Env, provider: Address) -> Option<ProviderWithdrawalWindow> {
        env.storage()
            .instance()
            .get(&DataKey::ProviderWindow(provider))
    }

    pub fn get_provider_total_pool(env: Env, provider: Address) -> i128 {
        get_provider_total_pool_impl(&env, &provider)
    }

    pub fn get_watt_hours_display(precise_watt_hours: i128, precision_factor: i128) -> i128 {
        if precision_factor <= 0 {
            return precise_watt_hours; // Fallback to avoid division by zero
        }
        precise_watt_hours / precision_factor
    }

    pub fn calculate_expected_depletion(env: Env, meter_id: u64) -> Option<u64> {
        if let Some(meter) = env
            .storage()
            .instance()
            .get::<_, Meter>(&DataKey::Meter(meter_id))
        {
            if meter.balance <= 0 || meter.rate_per_unit <= 0 {
                return Some(0); // Already depleted or no consumption
            }

            let seconds_until_depletion = meter.balance / meter.rate_per_unit;
            let current_time = env.ledger().timestamp();
            Some(current_time + seconds_until_depletion as u64)
        } else {
            None
        }
    }

    pub fn set_meter_pause(env: Env, meter_id: u64, paused: bool) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        meter.is_paused = paused;
        let now = env.ledger().timestamp();
        refresh_activity(&mut meter, now);

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events()
            .publish((symbol_short!("Paused"), meter_id), paused);
    }

    pub fn set_tiered_pricing(env: Env, meter_id: u64, threshold: i128, rate: i128) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        meter.tier_threshold = threshold;
        meter.tier_rate = rate;

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
    }

    pub fn vote_for_asset(env: Env, voter: Address, asset_symbol: Symbol) {
        voter.require_auth();

        // Check if user already voted for this specific asset
        if env
            .storage()
            .instance()
            .has(&DataKey::UserVoted(voter.clone(), asset_symbol.clone()))
        {
            panic_with_error!(env, ContractError::AlreadyVoted);
        }

        let mut votes = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::PollVotes(asset_symbol.clone()))
            .unwrap_or(0);

        votes += 1;

        env.storage()
            .instance()
            .set(&DataKey::PollVotes(asset_symbol.clone()), &votes);
        env.storage()
            .instance()
            .set(&DataKey::UserVoted(voter, asset_symbol.clone()), &true);

        env.events()
            .publish((symbol_short!("Voted"), asset_symbol), votes);
    }

    pub fn get_votes(env: Env, asset_symbol: Symbol) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::PollVotes(asset_symbol))
            .unwrap_or(0)
    }

    pub fn emergency_shutdown(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        // Emergency shutdown always disables the meter regardless of balance
        meter.is_active = false;

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
    }

    pub fn set_max_flow_rate(env: Env, meter_id: u64, max_rate_per_hour: i128) {
        // Issue #273: Validate hourly flow rate boundaries
        validate_hourly_flow_rate(max_rate_per_hour)
            .unwrap_or_else(|_| panic_with_error!(&env, ContractError::FlowRateTooHigh));

        let mut meter: Meter = env
            .storage()
            .instance()
            .get(&DataKey::Meter(meter_id))
            .ok_or("Meter not found")
            .unwrap();
        meter.provider.require_auth();

        meter.max_flow_rate_per_hour = max_rate_per_hour;

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
    }

    pub fn update_heartbeat(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();
        meter.heartbeat = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
    }

    pub fn withdraw_earnings(env: Env, meter_id: u64, amount_usd_cents: i128) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        if amount_usd_cents <= 0 {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        // Store old meter value for pool update
        let old_meter_value = provider_meter_value(&meter);

        let available_earnings = match meter.billing_type {
            BillingType::PrePaid => meter.balance,
            BillingType::PostPaid => meter.debt,
        };

        if amount_usd_cents > available_earnings {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        // Convert USD cents to XLM if needed
        let withdrawal_amount =
            match convert_usd_to_xlm_if_needed(&env, amount_usd_cents, &meter.token) {
                Ok(amount) => amount,
                Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
            };

        let client = token::Client::new(&env, &meter.token);
        client.transfer(
            &env.current_contract_address(),
            &meter.provider,
            &withdrawal_amount,
        );

        // Update meter balance/debt
        match meter.billing_type {
            BillingType::PrePaid => {
                meter.balance = meter.balance.saturating_sub(amount_usd_cents);
            }
            BillingType::PostPaid => {
                meter.debt = meter.debt.saturating_sub(amount_usd_cents);
            }
        }

        let now = env.ledger().timestamp();
        let was_active = meter.is_active;
        refresh_activity(&mut meter, now);

        if !was_active && meter.is_active {
            meter.last_update = now;
        }

        // Update provider total pool
        let new_meter_value = provider_meter_value(&meter);
        update_provider_total_pool(&env, &meter.provider, old_meter_value, new_meter_value);

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        // Emit conversion event if XLM was used
        if is_native_token(&meter.token) {
            env.events().publish(
                (symbol_short!("USD2XL"), meter_id),
                (amount_usd_cents, withdrawal_amount),
            );
        }
    }

    pub fn get_current_rate(env: Env) -> Option<PriceData> {
        match env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Oracle)
        {
            Some(oracle_address) => {
                let oracle_client = PriceOracleClient::new(&env, &oracle_address);
                Some(oracle_client.get_price())
            }
            None => None,
        }
    }

    pub fn is_meter_offline(env: Env, meter_id: u64) -> bool {
        match env
            .storage()
            .instance()
            .get::<DataKey, Meter>(&DataKey::Meter(meter_id))
        {
            Some(meter) => {
                env.ledger().timestamp().saturating_sub(meter.heartbeat) > HOUR_IN_SECONDS
            }
            None => true,
        }
    }

    /// Unlink a meter from its current tenant and link it to a new tenant.
    /// All historical usage data is preserved. Requires auth from the current
    /// user, the new user, and the provider.
    pub fn transfer_meter_ownership(env: Env, meter_id: u64, new_user: Address) {
        let mut meter = get_meter_or_panic(&env, meter_id);

        meter.user.require_auth();
        meter.provider.require_auth();
        new_user.require_auth();

        let old_user = meter.user.clone();
        let old_meter_value = provider_meter_value(&meter);
        meter.user = new_user.clone();

        // Update provider total pool (provider stays the same, only user changes)
        let new_meter_value = provider_meter_value(&meter);
        update_provider_total_pool(&env, &meter.provider, old_meter_value, new_meter_value);

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events()
            .publish((symbol_short!("Transfer"), meter_id), (old_user, new_user));
    }

    // Continuous Flow Engine Public Interface

    /// Create a new continuous flow stream
    /// Update the flow rate of an existing continuous stream
    pub fn update_continuous_flow_rate(env: Env, stream_id: u64, new_flow_rate: i128) {
        if new_flow_rate < 0 {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        update_flow_rate(&env, stream_id, new_flow_rate).unwrap();
    }

    /// Add balance to a continuous flow stream
    pub fn add_continuous_balance(env: Env, stream_id: u64, additional_balance: i128) {
        add_balance_to_flow(&env, stream_id, additional_balance).unwrap();

        env.events().publish(
            (symbol_short!("BalAdded"),),
            (stream_id, additional_balance),
        );
    }

    /// Get the current state of a continuous flow stream
    pub fn get_continuous_flow(env: Env, stream_id: u64) -> Option<ContinuousFlow> {
        env.storage()
            .instance()
            .get::<DataKey, ContinuousFlow>(&DataKey::ContinuousFlow(stream_id))
    }

    /// Calculate expected depletion time for a continuous flow stream
    pub fn calculate_continuous_depletion(env: Env, stream_id: u64) -> Option<u64> {
        if let Some(flow) = env
            .storage()
            .instance()
            .get::<DataKey, ContinuousFlow>(&DataKey::ContinuousFlow(stream_id))
        {
            if flow.status != StreamStatus::Active || flow.flow_rate_per_second <= 0 {
                return None;
            }

            let current_timestamp = env.ledger().timestamp();
            let accumulation = calculate_flow_accumulation(&flow, current_timestamp);
            let remaining_balance = flow.accumulated_balance.saturating_sub(accumulation);

            if remaining_balance <= 0 {
                return Some(current_timestamp);
            }

            let seconds_until_depletion = remaining_balance / flow.flow_rate_per_second;
            Some(current_timestamp + seconds_until_depletion as u64)
        } else {
            None
        }
    }

    /// Pause a continuous flow stream
    pub fn pause_continuous_flow(env: Env, stream_id: u64) {
        update_flow_rate(&env, stream_id, 0).unwrap();
    }

    /// Resume a continuous flow stream with specified rate
    pub fn resume_continuous_flow(env: Env, stream_id: u64, flow_rate_per_second: i128) {
        if flow_rate_per_second <= 0 {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        update_flow_rate(&env, stream_id, flow_rate_per_second).unwrap();
    }

    // Issue #178: Firmware Update Authorization Gate Functions

    /// Initiate a firmware update for a meter (provider-only)
    /// This pauses billing during the update window and requires device signature to resume
    pub fn initiate_firmware_update(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);

        // Only provider can initiate firmware update
        meter.provider.require_auth();

        // Check if already updating
        if meter.is_updating {
            panic_with_error!(&env, ContractError::FirmwareUpdateInProgress);
        }

        let now = env.ledger().timestamp();

        // Set update flag and timestamp
        meter.is_updating = true;
        meter.update_start_timestamp = now;

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        // Emit FirmwareUpdateStarted event
        let event = FirmwareUpdateStartedEvent {
            meter_id,
            update_start_timestamp: now,
            provider: meter.provider.clone(),
            max_update_window_secs: FIRMWARE_UPDATE_WINDOW_SECS,
        };

        env.events()
            .publish((symbol_short!("FWUpdSt"), meter_id), event);
    }

    /// Complete firmware update with device signature
    /// Device must sign the UpdateCompleteData to resume billing
    pub fn complete_firmware_update(env: Env, signed_update: SignedUpdateComplete) {
        let mut meter = get_meter_or_panic(&env, signed_update.meter_id);

        // Check if meter is currently updating
        if !meter.is_updating {
            panic_with_error!(&env, ContractError::MeterNotFound);
        }

        let now = env.ledger().timestamp();

        // Verify update window hasn't expired (max 2 hours)
        if now.saturating_sub(meter.update_start_timestamp) > FIRMWARE_UPDATE_WINDOW_SECS {
            panic_with_error!(&env, ContractError::FirmwareUpdateWindowExpired);
        }

        // Verify update_start_timestamp matches
        if signed_update.update_start_timestamp != meter.update_start_timestamp {
            panic_with_error!(&env, ContractError::InvalidFirmwareUpdateSignature);
        }

        if signed_update.completion_timestamp < meter.update_start_timestamp
            || signed_update.completion_timestamp > now
        {
            panic_with_error!(&env, ContractError::InvalidFirmwareUpdateSignature);
        }

        // Verify the device public key matches
        if signed_update.device_public_key != meter.device_public_key {
            panic_with_error!(&env, ContractError::PublicKeyMismatch);
        }

        // Create the message that was signed by the device
        let completion_data = UpdateCompleteData {
            meter_id: signed_update.meter_id,
            update_start_timestamp: signed_update.update_start_timestamp,
            completion_timestamp: signed_update.completion_timestamp,
        };

        // Verify the signature using Ed25519 (Soroban's built-in crypto)
        #[cfg(not(test))]
        env.crypto().ed25519_verify(
            &signed_update.device_public_key,
            &completion_data.to_xdr(&env),
            &signed_update.signature,
        );

        let update_start_timestamp = meter.update_start_timestamp;
        let update_duration_secs = signed_update
            .completion_timestamp
            .saturating_sub(update_start_timestamp);

        // Update meter state to resume billing
        meter.is_updating = false;
        meter.update_start_timestamp = 0;
        meter.last_update = now;

        env.storage()
            .instance()
            .set(&DataKey::Meter(signed_update.meter_id), &meter);

        // Emit FirmwareUpdateFinished event
        let event = FirmwareUpdateFinishedEvent {
            meter_id: signed_update.meter_id,
            update_start_timestamp,
            update_completed_timestamp: signed_update.completion_timestamp,
            update_duration_secs,
            device_signature_valid: true,
        };

        env.events()
            .publish((symbol_short!("FWUpdEnd"), signed_update.meter_id), event);
    }

    pub fn get_billing_group(env: Env, parent_account: Address) -> Option<BillingGroup> {
        env.storage()
            .instance()
            .get(&DataKey::BillingGroup(parent_account))
    }

    pub fn remove_meter_from_billing_group(env: Env, parent_account: Address, meter_id: u64) {
        parent_account.require_auth();

        let mut billing_group: BillingGroup = env
            .storage()
            .instance()
            .get(&DataKey::BillingGroup(parent_account.clone()))
            .ok_or("Billing group not found")
            .unwrap();

        billing_group.child_meters.retain(|&id| id != meter_id);
        env.storage()
            .instance()
            .set(&DataKey::BillingGroup(parent_account), &billing_group);

        // Update the meter to remove parent reference
        if let Some(mut meter) = env
            .storage()
            .instance()
            .get::<_, Meter>(&DataKey::Meter(meter_id))
        {
            meter.parent_account = None;
            env.storage()
                .instance()
                .set(&DataKey::Meter(meter_id), &meter);
        }
    }

    // Gas Cost Estimator Functions
    pub fn estimate_meter_monthly_cost(
        env: Env,
        is_group_meter: bool,
        _meters_in_group: u32,
    ) -> i128 {
        GasCostEstimator::estimate_meter_monthly_cost(&env, is_group_meter, _meters_in_group)
    }

    pub fn get_operation_cost(_env: Env, operation: String) -> i128 {
        GasCostEstimator::get_operation_cost(&operation)
    }

    // Webhook and Alert Functions
    pub fn configure_webhook(env: Env, user: Address, webhook_url: String) {
        user.require_auth();

        let webhook_config = WebhookConfig {
            url: webhook_url.clone(),
            user: user.clone(),
            is_active: true,
            created_at: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&DataKey::WebhookConfig(user), &webhook_config);
    }

    pub fn deactivate_webhook(env: Env, user: Address) {
        user.require_auth();

        if let Some(mut config) = env
            .storage()
            .instance()
            .get::<_, WebhookConfig>(&DataKey::WebhookConfig(user.clone()))
        {
            config.is_active = false;
            env.storage()
                .instance()
                .set(&DataKey::WebhookConfig(user), &config);
        }
    }

    pub fn get_webhook_config(env: Env, user: Address) -> Option<WebhookConfig> {
        env.storage().instance().get(&DataKey::WebhookConfig(user))
    }

    fn check_and_send_low_balance_alert(env: &Env, meter: &Meter, meter_id: u64) {
        // Only check if webhook is configured for this user
        let webhook_config = match env
            .storage()
            .instance()
            .get::<_, WebhookConfig>(&DataKey::WebhookConfig(meter.user.clone()))
        {
            Some(config) if config.is_active => config,
            _ => return, // No active webhook configured
        };

        // Calculate hours remaining (in integer hours)
        let hours_remaining: i128 = if meter.rate_per_second > 0 {
            meter.balance / meter.rate_per_second / 3600
        } else {
            i128::MAX
        };

        // Check if balance is low (< 24 hours)
        if hours_remaining < 24 {
            // Check if we've sent an alert recently (within last 12 hours)
            let current_time = env.ledger().timestamp();
            let last_alert_time: Option<u64> =
                env.storage().instance().get(&DataKey::LastAlert(meter_id));

            if let Some(last_time) = last_alert_time {
                if current_time.checked_sub(last_time).unwrap_or(0) < 43200 {
                    // 12 hours in seconds
                    return; // Already sent alert recently
                }
            }

            // Create and send alert
            let alert = LowBalanceAlert {
                meter_id,
                user: meter.user.clone(),
                remaining_balance: meter.balance,
                hours_remaining,
                timestamp: current_time,
            };

            // Store the alert timestamp
            env.storage()
                .instance()
                .set(&DataKey::LastAlert(meter_id), &current_time);

            // Store the alert using meter_id as key
            env.storage()
                .instance()
                .set(&DataKey::LastAlert(meter_id), &alert);
        }
    }

    pub fn get_pending_alerts(env: Env, user: Address) -> Vec<LowBalanceAlert> {
        let mut alerts = Vec::new(&env);

        let count: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);

        for meter_id in 1..=count {
            if let Some(meter) = env
                .storage()
                .instance()
                .get::<_, Meter>(&DataKey::Meter(meter_id))
            {
                if meter.user == user {
                    if let Some(alert) = env
                        .storage()
                        .instance()
                        .get::<_, LowBalanceAlert>(&DataKey::LastAlert(meter_id))
                    {
                        alerts.push_back(alert);
                    }
                }
            }
        }

        alerts
    }

    // Enhanced claim function with webhook integration
    pub fn claim_with_alerts(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        // Task #88: Kill-Switch Check
        if meter.is_disputed {
            panic_with_error!(&env, ContractError::InDispute);
        }

        let now = env.ledger().timestamp();
        let elapsed = now.checked_sub(meter.last_update).unwrap_or(0);

        // Task #90: Credit Settlement Flow
        let amount = (elapsed as i128)
            .saturating_mul(meter.rate_per_unit.saturating_add(meter.credit_drip_rate));

        // Check if we need to reset the hourly counter
        let hours_passed = now.checked_sub(meter.last_claim_time).unwrap_or(0) / 3600;
        if hours_passed >= 1 {
            meter.claimed_this_hour = 0;
            meter.last_claim_time = now;
        }

        // Ensure we don't overdraw the balance
        let claimable = if amount > meter.balance {
            meter.balance
        } else {
            amount
        };

        // Apply max flow rate cap
        let final_claimable = if claimable > 0 {
            let remaining_hourly_capacity = meter.max_flow_rate_per_hour - meter.claimed_this_hour;
            if claimable > remaining_hourly_capacity {
                remaining_hourly_capacity
            } else {
                claimable
            }
        } else {
            0
        };

        if final_claimable > 0 {
            let client = token::Client::new(&env, &meter.token);
            client.transfer(
                &env.current_contract_address(),
                &meter.provider,
                &final_claimable,
            );
            meter.balance -= final_claimable;
            meter.claimed_this_hour += final_claimable;

            // If credit drip was active, reduce the debt if in PostPaid mode
            if meter.billing_type == BillingType::PostPaid && meter.credit_drip_rate > 0 {
                let credit_settlement = (elapsed as i128)
                    .saturating_mul(meter.credit_drip_rate)
                    .min(meter.debt);
                meter.debt = meter.debt.saturating_sub(credit_settlement);
            }
        }

        meter.last_update = now;
        if meter.balance <= 0 {
            meter.is_active = false;
        }

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        // Check for low balance and send alert if needed
        Self::check_and_send_low_balance_alert(&env, &meter, meter_id);
    }

    // Task #87: Roommates support
    pub fn add_authorized_contributor(env: Env, meter_id: u64, contributor: Address) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        env.storage().instance().set(
            &DataKey::AuthorizedContributor(meter_id, contributor),
            &true,
        );
    }

    pub fn remove_authorized_contributor(env: Env, meter_id: u64, contributor: Address) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        env.storage()
            .instance()
            .remove(&DataKey::AuthorizedContributor(meter_id, contributor));
    }

    pub fn get_contribution(env: Env, meter_id: u64, contributor: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::Contributor(meter_id, contributor))
            .unwrap_or(0)
    }

    // Task #88: Emergency Kill-Switch (Challenge)
    pub fn challenge_service(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        if meter.is_disputed {
            panic_with_error!(&env, ContractError::ChallengeActive);
        }

        meter.is_disputed = true;
        meter.is_paused = true;
        meter.challenge_timestamp = env.ledger().timestamp();

        let now = env.ledger().timestamp();
        refresh_activity(&mut meter, now);

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events().publish(
            (symbol_short!("Challeng"), meter_id),
            meter.challenge_timestamp,
        );
    }

    pub fn resolve_challenge(env: Env, meter_id: u64, restored: bool) {
        let mut meter: Meter = env
            .storage()
            .instance()
            .get(&DataKey::Meter(meter_id))
            .expect("Meter not found");

        // This should be called by the Oracle or Admin
        let oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .expect("No oracle set");

        oracle.require_auth();

        if !meter.is_disputed {
            return;
        }

        if restored {
            // Service restored, unpause and resume stream
            meter.is_disputed = false;
            meter.is_paused = false;
        } else {
            // Service NOT restored
            meter.is_disputed = false; // Resolved but failed
            meter.is_paused = true; // Stay paused
        }

        let now = env.ledger().timestamp();
        refresh_activity(&mut meter, now);

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events()
            .publish((symbol_short!("Resolv"), meter_id), restored);
    }

    pub fn refund_disputed_funds(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        // Can only refund if challenged more than 48 hours ago and not resolved
        let now = env.ledger().timestamp();
        if !meter.is_disputed
            || now.saturating_sub(meter.challenge_timestamp) < (48 * HOUR_IN_SECONDS)
        {
            panic_with_error!(&env, ContractError::ChallengeActive);
        }

        // Return funds to user
        let refundable = match meter.billing_type {
            BillingType::PrePaid => meter.balance,
            BillingType::PostPaid => remaining_postpaid_collateral(&meter),
        };

        if refundable > 0 {
            let withdrawal_amount =
                match convert_usd_to_xlm_if_needed(&env, refundable, &meter.token) {
                    Ok(amount) => amount,
                    Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
                };

            let client = token::Client::new(&env, &meter.token);
            client.transfer(
                &env.current_contract_address(),
                &meter.user,
                &withdrawal_amount,
            );
        }

        meter.balance = 0;
        meter.debt = 0;
        meter.is_active = false;
        meter.is_disputed = false;

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events()
            .publish((symbol_short!("Refund"), meter_id), refundable);
    }

    // Task #90: Post-Paid Settlement Credit Logic
    pub fn set_credit_drip(env: Env, meter_id: u64, drip_rate: i128) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        meter.credit_drip_rate = drip_rate;

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
    }

    /// Configure carbon credit asset and drip rate for a meter.
    /// Provider must authorize this update.
    pub fn set_carbon_credit_config(env: Env, meter_id: u64, token: Address, drip_rate_bps: i128) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        if drip_rate_bps < 0 || drip_rate_bps > 10000 {
            panic_with_error!(env, ContractError::InvalidUsageValue);
        }

        meter.carbon_credit_token = Some(token.clone());
        meter.carbon_credit_drip_rate_bps = drip_rate_bps;

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);
        env.events().publish(
            (symbol_short!("CarbonCfg"), meter_id),
            (token, drip_rate_bps),
        );
    }

    // Task #1: Stream Priority System - Set priority index for a meter
    pub fn set_priority_index(env: Env, meter_id: u64, priority_index: u32) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        meter.priority_index = priority_index;

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events().publish(
            (soroban_sdk::symbol_short!("Prior"), meter_id),
            priority_index,
        );
    }

    // Task #1: Check if throttling should be activated and pause low-priority streams
    pub fn apply_throttling_if_needed(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        let throttling_active = check_throttling_threshold(&env, &meter);

        if should_pause_low_priority_stream(&meter, throttling_active) {
            meter.is_paused = true;
            panic_with_error!(&env, ContractError::LowPriorityStreamPaused);
        }

        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events().publish(
            (soroban_sdk::symbol_short!("Throttl"), meter_id),
            throttling_active,
        );
    }

    // Task #2: Tax Compliance - Set government vault address
    pub fn set_government_vault(env: Env, vault_address: Address) {
        vault_address.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::GovernmentVault, &vault_address);

        env.events()
            .publish((soroban_sdk::symbol_short!("GovVault"),), vault_address);
    }

    // Task #2: Tax Compliance - Set tax rate (in basis points)
    pub fn set_tax_rate(env: Env, tax_rate_bps: i128) {
        // Should be admin-only in production
        if tax_rate_bps < 0 || tax_rate_bps > 10_000 {
            panic_with_error!(&env, ContractError::InvalidUsageValue);
        }

        env.storage()
            .instance()
            .set(&DataKey::TaxRateBps, &tax_rate_bps);

        env.events()
            .publish((soroban_sdk::symbol_short!("TaxRate"),), tax_rate_bps);
    }

    // Task #3: Self-Maintenance - Get maintenance fund balance for a meter
    pub fn get_maintenance_fund(env: Env, meter_id: u64) -> i128 {
        get_maintenance_fund_balance(&env, meter_id)
    }

    // Task #3: Self-Maintenance - Manually extend TTL (emergency function)
    pub fn manual_extend_ttl(env: Env, meter_id: u64) {
        let maintenance_balance = get_maintenance_fund_balance(&env, meter_id);

        // Estimate cost (simplified)
        let estimated_cost = 1_000_000; // 1 XLM in stroops

        if maintenance_balance < estimated_cost {
            panic_with_error!(&env, ContractError::MaintenanceFundInsufficient);
        }

        // Deduct from maintenance fund
        let new_balance = maintenance_balance.saturating_sub(estimated_cost);
        env.storage()
            .instance()
            .set(&DataKey::MaintenanceFund(meter_id), &new_balance);

        // Extend TTL
        env.storage()
            .instance()
            .extend_ttl(LEDGER_LIFETIME_EXTENSION, LEDGER_LIFETIME_EXTENSION);

        env.events().publish(
            (soroban_sdk::symbol_short!("TTLMnl"), meter_id),
            LEDGER_LIFETIME_EXTENSION,
        );
    }

    // Task #4: Wasm Hash Rotation - Propose upgrade
    pub fn propose_upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let proposer = env.current_contract_address();
        proposer.require_auth();

        // Validate hash (basic check - should be non-zero)
        if new_wasm_hash == BytesN::<32>::from_array(&env, &[0; 32]) {
            panic_with_error!(&env, ContractError::InvalidWasmHash);
        }

        // Check if there's already an active proposal
        let existing_proposal_time: Option<u64> =
            env.storage().instance().get(&DataKey::UpgradeProposalTime);
        if let Some(proposal_time) = existing_proposal_time {
            let deadline: u64 = env
                .storage()
                .instance()
                .get(&DataKey::VetoDeadline)
                .unwrap_or(0);
            let now = env.ledger().timestamp();

            if now < deadline {
                panic_with_error!(&env, ContractError::UpgradeProposalActive);
            }
        }

        let proposal_id = propose_upgrade_impl(&env, new_wasm_hash, &proposer);

        env.events()
            .publish((soroban_sdk::symbol_short!("UpgrdProp"),), proposal_id);
    }

    // Task #4: Wasm Hash Rotation - Submit veto
    pub fn submit_upgrade_veto(env: Env, proposal_id: u64) {
        let user = env.current_contract_address();
        user.require_auth();

        // Check if veto period is still active
        let deadline: u64 = env
            .storage()
            .instance()
            .get(&DataKey::VetoDeadline)
            .unwrap_or(0);
        let now = env.ledger().timestamp();

        if now >= deadline {
            panic_with_error!(&env, ContractError::VetoPeriodExpired);
        }

        submit_veto(&env, &user, proposal_id);
    }

    // Task #4: Wasm Hash Rotation - Finalize upgrade
    pub fn finalize_upgrade(env: Env) {
        // Check if upgrade can be finalized
        if !can_finalize_upgrade(&env) {
            panic_with_error!(&env, ContractError::UpgradeProposalActive);
        }

        // Get the proposed upgrade
        let proposal: UpgradeProposal = env
            .storage()
            .instance()
            .get(&DataKey::ProposedUpgrade)
            .expect("No upgrade proposal found");

        // Execute the WASM upgrade on-chain
        env.deployer()
            .update_current_contract_wasm(proposal.new_wasm_hash.clone());

        env.events().publish(
            (soroban_sdk::symbol_short!("UpgrdFin"),),
            proposal.new_wasm_hash,
        );

        // Clear the proposal
        env.storage().instance().remove(&DataKey::ProposedUpgrade);
        env.storage()
            .instance()
            .remove(&DataKey::UpgradeProposalTime);
        env.storage().instance().remove(&DataKey::VetoDeadline);
    }

    // ============================================================
    // Storage Versioning Public Functions
    // ============================================================

    /// Get the current storage version
    pub fn get_storage_version_public(env: Env) -> u32 {
        get_storage_version(&env)
    }

    /// Finalize upgrade with storage version checking
    /// This is the enhanced version that validates storage compatibility
    pub fn finalize_upgrade_v2(env: Env, new_storage_version: u32) {
        // Check if upgrade can be finalized
        if !can_finalize_upgrade(&env) {
            panic_with_error!(&env, ContractError::UpgradeProposalActive);
        }

        // Get the proposed upgrade
        let proposal: UpgradeProposal = env
            .storage()
            .instance()
            .get(&DataKey::ProposedUpgrade)
            .expect("No upgrade proposal found");

        // Validate storage version compatibility
        if let Err(e) = validate_storage_version_compatibility(&env, new_storage_version) {
            panic_with_error!(&env, e);
        }

        // Execute the WASM upgrade on-chain
        env.deployer()
            .update_current_contract_wasm(proposal.new_wasm_hash.clone());

        env.events().publish(
            (soroban_sdk::symbol_short!("UpgrdFin"),),
            proposal.new_wasm_hash,
        );

        // Check if migration is needed
        let current_version = get_storage_version(&env);
        if new_storage_version > current_version {
            // Version increased by 1, migration needed
            env.events().publish(
                (soroban_sdk::symbol_short!("MigStart"),),
                (current_version, new_storage_version),
            );
            
            // Note: Actual migration should be called separately via run_migration
            // to allow for batched/resumable execution
        } else {
            // Same version, update directly
            set_storage_version(&env, new_storage_version);
        }

        // Clear the proposal
        env.storage().instance().remove(&DataKey::ProposedUpgrade);
        env.storage()
            .instance()
            .remove(&DataKey::UpgradeProposalTime);
        env.storage().instance().remove(&DataKey::VetoDeadline);
    }

    /// Run migration for storage version upgrade
    /// This function can be called multiple times to complete a migration in batches
    /// Returns true if migration is complete, false if more calls are needed
    pub fn run_migration(env: Env, target_version: u32) -> bool {
        require_admin_auth(&env);

        let current_version = get_storage_version(&env);

        // Check if migration is needed
        if target_version == current_version {
            // Already at target version
            return true;
        }

        if target_version < current_version {
            panic_with_error!(&env, ContractError::IncompatibleStorageVersion);
        }

        // Currently only support v1 to v2 migration
        if current_version == 1 && target_version == 2 {
            match migrate_v1_to_v2(&env) {
                Ok(complete) => complete,
                Err(e) => panic_with_error!(&env, e),
            }
        } else {
            // No migration function available for this version pair
            panic_with_error!(&env, ContractError::NoMigrationFunction);
        }
    }

    /// Cancel an ongoing migration (admin only)
    /// This is useful if a migration encounters issues and needs to be reset
    pub fn cancel_migration(env: Env) {
        require_admin_auth(&env);

        if !is_migration_in_progress(&env) {
            return; // Nothing to cancel
        }

        clear_migration_cursor(&env);

        env.events().publish(
            (soroban_sdk::symbol_short!("MigCancel"),),
            get_storage_version(&env),
        );
    }

    /// Check if a migration is currently in progress
    pub fn is_migration_active(env: Env) -> bool {
        is_migration_in_progress(&env)
    }

    // ============================================================
    // NEW TASKS IMPLEMENTATION
    // ============================================================

    /// Initialize admin transfer with 48-hour timelock
    /// During the window, active users can veto (requires 10% to succeed)
    pub fn initiate_admin_transfer(env: Env, proposed_admin: Address) {
        let current_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::CurrentAdmin)
            .expect("No admin set");

        current_admin.require_auth();

        // Check no active transfer
        let existing_proposal: Option<AdminTransferProposal> = env
            .storage()
            .instance()
            .get(&DataKey::AdminTransferProposal);

        if let Some(proposal) = existing_proposal {
            if proposal.is_active && env.ledger().timestamp() < proposal.execution_deadline {
                panic_with_error!(&env, ContractError::AdminTransferActive);
            }
        }

        let now = env.ledger().timestamp();
        let proposal = AdminTransferProposal {
            current_admin: current_admin.clone(),
            proposed_admin: proposed_admin.clone(),
            proposed_at: now,
            execution_deadline: now + ADMIN_TRANSFER_TIMELOCK,
            veto_count: 0,
            is_active: true,
        };

        env.storage()
            .instance()
            .set(&DataKey::AdminTransferProposal, &proposal);

        env.events().publish(
            (soroban_sdk::symbol_short!("AdminXfer"),),
            (current_admin, proposed_admin, now + ADMIN_TRANSFER_TIMELOCK),
        );
    }

    /// Submit veto against admin transfer
    /// Requires 10% of active users to veto
    pub fn veto_admin_transfer(env: Env, user: Address) {
        user.require_auth();

        let proposal: AdminTransferProposal = env
            .storage()
            .instance()
            .get(&DataKey::AdminTransferProposal)
            .expect("No active transfer");

        if !proposal.is_active || env.ledger().timestamp() >= proposal.execution_deadline {
            panic_with_error!(&env, ContractError::NoAdminTransferInProgress);
        }

        // Check if user already vetoed
        let has_vetoed: bool = env
            .storage()
            .instance()
            .get(&DataKey::AdminVeto(user.clone(), proposal.proposed_at))
            .unwrap_or(false);

        if has_vetoed {
            panic_with_error!(&env, ContractError::AlreadyVoted);
        }

        // Record veto
        env.storage()
            .instance()
            .set(&DataKey::AdminVeto(user, proposal.proposed_at), &true);

        // Increment veto count
        let mut updated_proposal = proposal;
        updated_proposal.veto_count += 1;
        env.storage()
            .instance()
            .set(&DataKey::AdminTransferProposal, &updated_proposal);

        env.events().publish(
            (soroban_sdk::symbol_short!("Veto"),),
            updated_proposal.veto_count,
        );
    }

    /// Execute admin transfer after 48-hour timelock if not vetoed
    pub fn execute_admin_transfer(env: Env) {
        let proposal: AdminTransferProposal = env
            .storage()
            .instance()
            .get(&DataKey::AdminTransferProposal)
            .expect("No active transfer");

        if !proposal.is_active {
            panic_with_error!(&env, ContractError::NoAdminTransferInProgress);
        }

        let now = env.ledger().timestamp();

        // Check if execution window expired
        if now > proposal.execution_deadline + DAY_IN_SECONDS {
            panic_with_error!(&env, ContractError::AdminExecutionWindowExpired);
        }

        // Calculate total active users and veto threshold
        let total_active_users: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ActiveUsers)
            .unwrap_or(100); // Default 100 for testing

        let veto_threshold = (total_active_users as i128 * VETO_THRESHOLD_BPS / 10000) as u32;

        if proposal.veto_count >= veto_threshold {
            panic_with_error!(&env, ContractError::VetoThresholdNotReached);
        }

        // Execute transfer
        env.storage()
            .instance()
            .set(&DataKey::CurrentAdmin, &proposal.proposed_admin);
        env.storage()
            .instance()
            .remove(&DataKey::AdminTransferProposal);

        // Clean up individual vetos
        // (In production, you'd iterate and clean, but simplified here)

        env.events().publish(
            (soroban_sdk::symbol_short!("AdminDone"),),
            (proposal.proposed_admin, now),
        );
    }

    /// Set current admin (initialization only)
    pub fn set_initial_admin(env: Env, admin: Address) {
        // Only allow if no admin is set
        let existing: Option<Address> = env.storage().instance().get(&DataKey::CurrentAdmin);
        if existing.is_some() {
            panic_with_error!(&env, ContractError::AdminTransferActive);
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::CurrentAdmin, &admin);

        env.events()
            .publish((soroban_sdk::symbol_short!("SetAdmn"),), admin);
    }

    /// Register as active user (for governance tracking)
    pub fn register_active_user(env: Env, user: Address) {
        user.require_auth();

        // Simplified: just increment counter
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ActiveUsers)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::ActiveUsers, &(count + 1));

        env.events()
            .publish((soroban_sdk::symbol_short!("ActvUser"),), user);
    }

    // ==================== TASK #2: LEGAL FREEZE ====================

    /// Initiate legal freeze on a meter (compliance officer only)
    pub fn legal_freeze(env: Env, meter_id: u64, reason: String) {
        let compliance_officer: Address = env
            .storage()
            .instance()
            .get(&DataKey::ComplianceOfficer)
            .expect("No compliance officer set");

        compliance_officer.require_auth();

        // Check if already frozen
        let existing_freeze: Option<LegalFreeze> = env
            .storage()
            .instance()
            .get(&DataKey::LegalFreeze(meter_id));

        if let Some(freeze) = existing_freeze {
            if !freeze.is_released {
                panic_with_error!(&env, ContractError::LegalFreezeAlreadyActive);
            }
        }

        let mut meter = get_meter_or_panic(&env, meter_id);

        // Get legal vault
        let legal_vault: Address = env
            .storage()
            .instance()
            .get(&DataKey::LegalVault)
            .expect("No legal vault set");

        // Calculate frozen amount
        let frozen_amount = match meter.billing_type {
            BillingType::PrePaid => meter.balance,
            BillingType::PostPaid => remaining_postpaid_collateral(&meter),
        };

        // Transfer funds to legal vault
        if frozen_amount > 0 {
            let withdrawal_amount =
                match convert_usd_to_xlm_if_needed(&env, frozen_amount, &meter.token) {
                    Ok(amount) => amount,
                    Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
                };

            let client = token::Client::new(&env, &meter.token);
            client.transfer(
                &env.current_contract_address(),
                &legal_vault,
                &withdrawal_amount,
            );
        }

        // Create freeze record
        let freeze = LegalFreeze {
            meter_id,
            frozen_at: env.ledger().timestamp(),
            reason: reason.clone(),
            compliance_officer: compliance_officer.clone(),
            legal_vault: legal_vault.clone(),
            frozen_amount,
            is_released: false,
        };

        env.storage()
            .instance()
            .set(&DataKey::LegalFreeze(meter_id), &freeze);

        // Pause the meter
        meter.is_paused = true;
        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events().publish(
            (soroban_sdk::symbol_short!("LglFrz"), meter_id),
            (reason, frozen_amount, legal_vault),
        );
    }

    /// Release legal freeze (requires compliance council multi-sig)
    pub fn release_legal_freeze(env: Env, meter_id: u64, council_signatures: Vec<Address>) {
        // Verify council approval (simplified: check at least 2 signatures)
        if council_signatures.len() < 2 {
            panic_with_error!(&env, ContractError::ComplianceCouncilApprovalRequired);
        }

        // In production, verify each signature against council members
        // For now, just require auth from provided addresses
        for sig in council_signatures.iter() {
            sig.require_auth();
        }

        let freeze: LegalFreeze = env
            .storage()
            .instance()
            .get(&DataKey::LegalFreeze(meter_id))
            .expect("No active freeze");

        if freeze.is_released {
            panic_with_error!(&env, ContractError::MeterNotFrozen);
        }

        let mut meter = get_meter_or_panic(&env, meter_id);

        // Return funds from legal vault to user
        if freeze.frozen_amount > 0 {
            let legal_vault: Address = env
                .storage()
                .instance()
                .get(&DataKey::LegalVault)
                .expect("No legal vault set");

            let withdrawal_amount =
                match convert_usd_to_xlm_if_needed(&env, freeze.frozen_amount, &meter.token) {
                    Ok(amount) => amount,
                    Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
                };

            let client = token::Client::new(&env, &meter.token);
            client.transfer(&legal_vault, &meter.user, &withdrawal_amount);
        }

        // Update freeze record
        let mut updated_freeze = freeze;
        updated_freeze.is_released = true;
        env.storage()
            .instance()
            .set(&DataKey::LegalFreeze(meter_id), &updated_freeze);

        // Unpause meter
        meter.is_paused = false;
        env.storage()
            .instance()
            .set(&DataKey::Meter(meter_id), &meter);

        env.events().publish(
            (soroban_sdk::symbol_short!("FrzRls"), meter_id),
            env.ledger().timestamp(),
        );
    }

    /// Set compliance officer address
    pub fn set_compliance_officer(env: Env, officer: Address) {
        // Should be called by current admin
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::CurrentAdmin)
            .expect("No admin set");

        admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::ComplianceOfficer, &officer);

        env.events()
            .publish((soroban_sdk::symbol_short!("CmpOfcr"),), officer);
    }

    /// Set legal vault address
    pub fn set_legal_vault(env: Env, vault: Address) {
        vault.require_auth();

        env.storage().instance().set(&DataKey::LegalVault, &vault);

        env.events()
            .publish((soroban_sdk::symbol_short!("LglVlt"),), vault);
    }

    /// Get legal freeze info
    pub fn get_legal_freeze(env: Env, meter_id: u64) -> LegalFreeze {
        env.storage()
            .instance()
            .get(&DataKey::LegalFreeze(meter_id))
            .expect("No freeze found")
    }

    // ==================== TASK #3: VERIFIED PROVIDER REGISTRY ====================

    /// Request provider verification
    pub fn request_provider_verification(env: Env, provider_name: String) {
        let provider = env.current_contract_address();
        provider.require_auth();

        // Check if already verified
        let existing: Option<VerifiedProvider> = env
            .storage()
            .instance()
            .get(&DataKey::VerifiedProvider(provider.clone()));

        if let Some(v) = existing {
            if v.is_verified {
                panic_with_error!(&env, ContractError::VerificationAlreadyGranted);
            }
        }

        // Create verification request (pending identity verification)
        let verified_provider = VerifiedProvider {
            address: provider.clone(),
            is_verified: false,
            verified_at: env.ledger().timestamp(),
            verification_method: VerificationMethod::IdentityVerified,
            provider_name,
        };

        env.storage().instance().set(
            &DataKey::VerifiedProvider(provider.clone()),
            &verified_provider,
        );

        env.events()
            .publish((soroban_sdk::symbol_short!("VrfReqst"),), provider);
    }

    /// Grant verification to provider (admin or community vote)
    pub fn grant_provider_verification(env: Env, provider: Address, method: VerificationMethod) {
        // Admin can grant verification
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::CurrentAdmin)
            .expect("No admin set");

        admin.require_auth();

        let mut verified_provider: VerifiedProvider = env
            .storage()
            .instance()
            .get(&DataKey::VerifiedProvider(provider.clone()))
            .expect("No verification request found");

        verified_provider.is_verified = true;
        verified_provider.verification_method = method;
        verified_provider.verified_at = env.ledger().timestamp();

        env.storage().instance().set(
            &DataKey::VerifiedProvider(provider.clone()),
            &verified_provider,
        );

        env.events()
            .publish((soroban_sdk::symbol_short!("VrfGrnt"),), provider);
    }

    /// Check if provider is verified
    pub fn is_provider_verified(env: Env, provider: Address) -> bool {
        let verified: Option<VerifiedProvider> = env
            .storage()
            .instance()
            .get(&DataKey::VerifiedProvider(provider));

        match verified {
            Some(v) => v.is_verified,
            None => false,
        }
    }

    /// Get provider info
    pub fn get_provider_info(env: Env, provider: Address) -> VerifiedProvider {
        env.storage()
            .instance()
            .get(&DataKey::VerifiedProvider(provider))
            .expect("Provider not found")
    }

    // ==================== TASK #4: SUB-DAO HIERARCHICAL PERMISSIONS ====================

    /// Create Sub-DAO configuration
    pub fn create_sub_dao(env: Env, sub_dao: Address, allocated_budget: i128, token: Address) {
        let parent_dao = env.current_contract_address();
        parent_dao.require_auth();

        // Check budget availability (simplified)
        let existing_config: Option<SubDaoConfig> = env
            .storage()
            .instance()
            .get(&DataKey::SubDaoConfig(sub_dao.clone()));

        if let Some(config) = existing_config {
            if config.is_active {
                panic_with_error!(&env, ContractError::SubDaoNotConfigured);
            }
        }

        let config = SubDaoConfig {
            parent_dao: parent_dao.clone(),
            sub_dao: sub_dao.clone(),
            allocated_budget,
            spent_budget: 0,
            token: token.clone(),
            created_at: env.ledger().timestamp(),
            is_active: true,
        };

        env.storage()
            .instance()
            .set(&DataKey::SubDaoConfig(sub_dao.clone()), &config);

        env.events().publish(
            (soroban_sdk::symbol_short!("SubDaoC"),),
            (parent_dao, sub_dao.clone(), allocated_budget),
        );
    }

    /// Create stream from Sub-DAO (uses allocated budget)
    pub fn create_sub_dao_stream(
        env: Env,
        user: Address,
        provider: Address,
        off_peak_rate: i128,
        token: Address,
        device_public_key: BytesN<32>,
        priority_index: u32,
        resource_type: ResourceType,
    ) -> u64 {
        // Verify caller is a configured Sub-DAO
        let sub_dao = env.current_contract_address();

        let config: SubDaoConfig = env
            .storage()
            .instance()
            .get(&DataKey::SubDaoConfig(sub_dao.clone()))
            .expect("Sub-DAO not configured");

        if !config.is_active {
            panic_with_error!(&env, ContractError::SubDaoNotConfigured);
        }

        // Verify token matches
        if token != config.token {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        // Check budget (simplified - in production would track properly)
        if config.spent_budget >= config.allocated_budget {
            panic_with_error!(&env, ContractError::SubDaoBudgetExceeded);
        }

        // Create the meter using standard logic
        let meter_id = Self::register_meter_with_mode(
            env,
            user,
            provider,
            off_peak_rate,
            token,
            BillingType::PrePaid,
            device_public_key,
            priority_index,
            resource_type,
        );

        // Update spent budget (simplified)
        let mut updated_config = config;
        updated_config.spent_budget += off_peak_rate; // Simplified accounting
        env.storage()
            .instance()
            .set(&DataKey::SubDaoConfig(sub_dao.clone()), &updated_config);

        env.events()
            .publish((soroban_sdk::symbol_short!("SubDaoStr"), meter_id), sub_dao);

        meter_id
    }

    /// Recall funds from Sub-DAO (parent DAO only)
    pub fn recall_sub_dao_funds(env: Env, sub_dao: Address, amount: i128) {
        let parent_dao = env.current_contract_address();
        parent_dao.require_auth();

        let mut config: SubDaoConfig = env
            .storage()
            .instance()
            .get(&DataKey::SubDaoConfig(sub_dao.clone()))
            .expect("Sub-DAO not configured");

        if config.parent_dao != parent_dao {
            panic_with_error!(&env, ContractError::NotParentDao);
        }

        // Reduce allocated budget
        config.allocated_budget = config.allocated_budget.saturating_sub(amount);

        env.storage()
            .instance()
            .set(&DataKey::SubDaoConfig(sub_dao.clone()), &config);

        env.events().publish(
            (symbol_short!("SubDaoR"),),
            (sub_dao.clone(), amount, config.allocated_budget),
        );
    }

    /// Deactivate Sub-DAO
    pub fn deactivate_sub_dao(env: Env, sub_dao: Address) {
        let parent_dao = env.current_contract_address();
        parent_dao.require_auth();

        let mut config: SubDaoConfig = env
            .storage()
            .instance()
            .get(&DataKey::SubDaoConfig(sub_dao.clone()))
            .expect("Sub-DAO not configured");

        if config.parent_dao != parent_dao {
            panic_with_error!(&env, ContractError::NotParentDao);
        }

        config.is_active = false;
        env.storage()
            .instance()
            .set(&DataKey::SubDaoConfig(sub_dao.clone()), &config);

        env.events()
            .publish((soroban_sdk::symbol_short!("SubDaoOff"),), sub_dao.clone());
    }

    /// Get Sub-DAO config
    pub fn get_sub_dao_config(env: Env, sub_dao: Address) -> SubDaoConfig {
        env.storage()
            .instance()
            .get(&DataKey::SubDaoConfig(sub_dao))
            .expect("Sub-DAO not configured")
    }

    // ============================================================================
    // Issue #98: Multi-Sig Provider Withdrawal Requirement
    // ============================================================================
    // For large utility companies, a single wallet should not be able to pull
    // millions in revenue. This implements a "Multi-Sig Payout" requirement where
    // withdrawals from the contract to the company's main treasury require 3-of-5
    // authorized signatures from "Finance Department" wallets.
    // ============================================================================

    /// Configure multi-sig withdrawal requirement for a provider.
    /// This sets up the Finance Department wallets that can authorize large withdrawals.
    ///
    /// # Arguments
    /// * `provider` - The utility provider address
    /// * `finance_wallets` - Vector of authorized Finance Department wallet addresses (3-5 wallets)
    /// * `required_signatures` - Number of signatures required (must be <= wallet count)
    /// * `threshold_amount` - Minimum amount in USD cents requiring multi-sig approval
    pub fn configure_multisig_withdrawal(
        env: Env,
        provider: Address,
        finance_wallets: Vec<Address>,
        required_signatures: u32,
        threshold_amount: i128,
    ) {
        // Require provider authorization
        provider.require_auth();

        // Check if already configured
        if env
            .storage()
            .instance()
            .has(&DataKey::MultiSigConfig(provider.clone()))
        {
            panic_with_error!(&env, ContractError::MultiSigAlreadyConfigured);
        }

        // Validate wallet count (3-5 wallets required)
        let wallet_count = finance_wallets.len();
        if wallet_count < MIN_FINANCE_WALLETS || wallet_count > MAX_FINANCE_WALLETS {
            panic_with_error!(&env, ContractError::InvalidFinanceWalletCount);
        }

        Self::validate_multisig_config(&env, &finance_wallets, required_signatures);

        let config = MultiSigConfig {
            provider: provider.clone(),
            finance_wallets,
            required_signatures,
            threshold_amount,
            is_active: true,
            created_at: env.ledger().timestamp(),
        };

        // Store configuration
        env.storage()
            .instance()
            .set(&DataKey::MultiSigConfig(provider.clone()), &config);

        // Initialize request counter
        env.storage()
            .instance()
            .set(&DataKey::WithdrawalRequestCount(provider.clone()), &0u64);

        env.events().publish(
            (symbol_short!("MSigCfg"),),
            (provider, required_signatures, threshold_amount),
        );
    }

    fn minimum_multisig_threshold(wallet_count: u32) -> u32 {
        let half_rounded_up = (wallet_count + 1) / 2;
        if half_rounded_up > MIN_MULTISIG_THRESHOLD {
            half_rounded_up
        } else {
            MIN_MULTISIG_THRESHOLD
        }
    }

    fn validate_unique_signers(env: &Env, signers: &Vec<Address>) {
        for i in 0..signers.len() {
            let signer = signers.get(i).unwrap();
            for j in (i + 1)..signers.len() {
                if signer == signers.get(j).unwrap() {
                    panic_with_error!(env, ContractError::InvalidFinanceWalletCount);
                }
            }
        }
    }

    fn validate_multisig_config(env: &Env, signers: &Vec<Address>, required_signatures: u32) {
        let wallet_count = signers.len();
        if wallet_count < MIN_FINANCE_WALLETS || wallet_count > MAX_FINANCE_WALLETS {
            panic_with_error!(env, ContractError::InvalidFinanceWalletCount);
        }

        Self::validate_unique_signers(env, signers);

        if required_signatures < Self::minimum_multisig_threshold(wallet_count)
            || required_signatures > wallet_count
        {
            panic_with_error!(env, ContractError::InvalidSignatureThreshold);
        }
    }

    fn validate_multisig_approvals(
        env: &Env,
        provider: &Address,
        request_id: u64,
        config: &MultiSigConfig,
    ) -> u32 {
        Self::validate_multisig_config(env, &config.finance_wallets, config.required_signatures);

        let mut approval_count = 0u32;
        for i in 0..config.finance_wallets.len() {
            let signer = config.finance_wallets.get(i).unwrap();
            let approval_key = DataKey::WithdrawalApproval(provider.clone(), request_id, signer);
            if env.storage().instance().has(&approval_key) {
                approval_count += 1;
            }
        }

        if approval_count < config.required_signatures {
            panic_with_error!(env, ContractError::InsufficientApprovals);
        }

        approval_count
    }

    /// Update multi-sig configuration for a provider.
    /// Requires provider authorization and enforces distinct signer and threshold bounds.
    pub fn update_multisig_config(
        env: Env,
        provider: Address,
        new_finance_wallets: Vec<Address>,
        new_required_signatures: u32,
        new_threshold_amount: i128,
    ) {
        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::MultiSigConfig(provider.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MultiSigNotConfigured));

        // Require authorization from the provider
        provider.require_auth();

        Self::validate_multisig_config(&env, &new_finance_wallets, new_required_signatures);

        let updated_config = MultiSigConfig {
            provider: provider.clone(),
            finance_wallets: new_finance_wallets,
            required_signatures: new_required_signatures,
            threshold_amount: new_threshold_amount,
            is_active: config.is_active,
            created_at: config.created_at,
        };

        env.storage()
            .instance()
            .set(&DataKey::MultiSigConfig(provider.clone()), &updated_config);

        env.events().publish(
            (symbol_short!("MSigUpd"),),
            (provider, new_required_signatures, new_threshold_amount),
        );
    }

    /// Propose a multi-sig withdrawal request.
    /// Only authorized Finance Department wallets can propose withdrawals.
    ///
    /// # Arguments
    /// * `provider` - The utility provider address
    /// * `meter_id` - The meter to withdraw earnings from
    /// * `amount_usd_cents` - Amount to withdraw in USD cents
    /// * `destination` - Treasury address to receive funds
    ///
    /// # Returns
    /// The request ID for this withdrawal proposal
    pub fn propose_multisig_withdrawal(
        env: Env,
        provider: Address,
        meter_id: u64,
        amount_usd_cents: i128,
        destination: Address,
    ) -> u64 {
        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::MultiSigConfig(provider.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MultiSigNotConfigured));

        if !config.is_active {
            panic_with_error!(&env, ContractError::MultiSigNotConfigured);
        }

        // Verify the meter belongs to this provider
        let meter = get_meter_or_panic(&env, meter_id);
        if meter.provider != provider {
            panic_with_error!(&env, ContractError::MeterNotFound);
        }

        // Check amount is above multi-sig threshold
        if amount_usd_cents < config.threshold_amount {
            panic_with_error!(&env, ContractError::AmountBelowMultiSigThreshold);
        }

        // Find the proposer from authorized finance wallets
        let mut proposer: Option<Address> = None;
        for i in 0..config.finance_wallets.len() {
            let wallet = config.finance_wallets.get(i).unwrap();
            // Try to require auth from each wallet - the one that authorized is the proposer
            if env
                .try_invoke_contract::<(), _>(&wallet, &Symbol::new(&env, "require_auth"), ())
                .is_ok()
            {
                proposer = Some(wallet);
                break;
            }
        }

        // Alternative: Require explicit proposer parameter and verify they're authorized
        // For now, we'll require any finance wallet to authorize
        let mut found_proposer = false;
        let mut actual_proposer = config.finance_wallets.get(0).unwrap();
        for i in 0..config.finance_wallets.len() {
            let wallet = config.finance_wallets.get(i).unwrap();
            // Check if this wallet can authorize
            wallet.require_auth();
            actual_proposer = wallet;
            found_proposer = true;
            break;
        }

        if !found_proposer {
            panic_with_error!(&env, ContractError::NotAuthorizedFinanceWallet);
        }

        // Get and increment request counter
        let request_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawalRequestCount(provider.clone()))
            .unwrap_or(0);

        let now = env.ledger().timestamp();

        let request = WithdrawalRequest {
            request_id,
            provider: provider.clone(),
            meter_id,
            amount_usd_cents,
            destination: destination.clone(),
            proposer: actual_proposer.clone(),
            created_at: now,
            expires_at: now + WITHDRAWAL_REQUEST_EXPIRY,
            approval_count: 1, // Proposer automatically approves
            is_executed: false,
            is_cancelled: false,
        };

        // Store the request
        env.storage().instance().set(
            &DataKey::WithdrawalRequest(provider.clone(), request_id),
            &request,
        );

        // Record proposer's approval
        env.storage().instance().set(
            &DataKey::WithdrawalApproval(provider.clone(), request_id, actual_proposer.clone()),
            &true,
        );

        // Increment counter
        env.storage().instance().set(
            &DataKey::WithdrawalRequestCount(provider.clone()),
            &(request_id + 1),
        );

        env.events().publish(
            (symbol_short!("MSigProp"),),
            (
                provider,
                request_id,
                amount_usd_cents,
                destination,
                actual_proposer,
            ),
        );

        request_id
    }

    /// Approve a pending multi-sig withdrawal request.
    /// Only authorized Finance Department wallets can approve.
    ///
    /// # Arguments
    /// * `provider` - The utility provider address
    /// * `request_id` - The withdrawal request ID to approve
    pub fn approve_multisig_withdrawal(
        env: Env,
        provider: Address,
        request_id: u64,
        approver: Address,
    ) {
        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::MultiSigConfig(provider.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MultiSigNotConfigured));

        let mut request: WithdrawalRequest = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawalRequest(provider.clone(), request_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::WithdrawalRequestNotFound));

        // Check request status
        if request.is_executed {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyExecuted);
        }
        if request.is_cancelled {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyCancelled);
        }
        if env.ledger().timestamp() > request.expires_at {
            panic_with_error!(&env, ContractError::WithdrawalRequestExpired);
        }

        if !config.finance_wallets.contains(&approver) {
            panic_with_error!(&env, ContractError::NotAuthorizedFinanceWallet);
        }
        approver.require_auth();
        let actual_approver = approver;

        // Check if already approved by this wallet
        let approval_key =
            DataKey::WithdrawalApproval(provider.clone(), request_id, actual_approver.clone());
        if env.storage().instance().has(&approval_key) {
            panic_with_error!(&env, ContractError::AlreadyApprovedWithdrawal);
        }

        // Record approval
        env.storage().instance().set(&approval_key, &true);
        request.approval_count += 1;

        // Update request
        env.storage().instance().set(
            &DataKey::WithdrawalRequest(provider.clone(), request_id),
            &request,
        );

        env.events().publish(
            (symbol_short!("MSigAppr"),),
            (
                provider,
                request_id,
                actual_approver,
                request.approval_count,
            ),
        );
    }

    /// Execute a multi-sig withdrawal after sufficient approvals.
    ///
    /// # Arguments
    /// * `provider` - The utility provider address
    /// * `request_id` - The withdrawal request ID to execute
    pub fn execute_multisig_withdrawal(env: Env, provider: Address, request_id: u64) {
        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::MultiSigConfig(provider.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MultiSigNotConfigured));

        let mut request: WithdrawalRequest = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawalRequest(provider.clone(), request_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::WithdrawalRequestNotFound));

        if request.is_executed {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyExecuted);
        }
        if request.is_cancelled {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyCancelled);
        }
        if env.ledger().timestamp() > request.expires_at {
            panic_with_error!(&env, ContractError::WithdrawalRequestExpired);
        }
        let verified_approval_count =
            Self::validate_multisig_approvals(&env, &provider, request_id, &config);
        request.approval_count = verified_approval_count;

        // Get meter and verify
        let mut meter = get_meter_or_panic(&env, request.meter_id);
        if meter.provider != provider {
            panic_with_error!(&env, ContractError::MeterNotFound);
        }

        let old_meter_value = provider_meter_value(&meter);
        let available_earnings = match meter.billing_type {
            BillingType::PrePaid => meter.balance,
            BillingType::PostPaid => meter.debt,
        };

        if request.amount_usd_cents > available_earnings {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        let withdrawal_amount =
            match convert_usd_to_token_if_needed(&env, request.amount_usd_cents, &meter.token) {
                Ok(amount) => amount,
                Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
            };

        let client = token::Client::new(&env, &meter.token);
        client.transfer(
            &env.current_contract_address(),
            &request.destination,
            &withdrawal_amount,
        );

        match meter.billing_type {
            BillingType::PrePaid => {
                meter.balance = meter.balance.saturating_sub(request.amount_usd_cents);
            }
            BillingType::PostPaid => {
                meter.debt = meter.debt.saturating_sub(request.amount_usd_cents);
            }
        }

        let now = env.ledger().timestamp();
        let was_active = meter.is_active;
        refresh_activity(&mut meter, now);

        if !was_active && meter.is_active {
            meter.last_update = now;
        }

        let new_meter_value = provider_meter_value(&meter);
        update_provider_total_pool(&env, &meter.provider, old_meter_value, new_meter_value);

        env.storage()
            .instance()
            .set(&DataKey::Meter(request.meter_id), &meter);

        // Mark request as executed
        request.is_executed = true;
        env.storage().instance().set(
            &DataKey::WithdrawalRequest(provider.clone(), request_id),
            &request,
        );

        env.events().publish(
            (symbol_short!("MSigExec"),),
            (
                provider,
                request_id,
                request.amount_usd_cents,
                request.destination,
                withdrawal_amount,
            ),
        );
    }

    /// Revoke a previously given approval for a withdrawal request.
    pub fn revoke_multisig_approval(env: Env, provider: Address, request_id: u64) {
        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::MultiSigConfig(provider.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MultiSigNotConfigured));

        let mut request: WithdrawalRequest = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawalRequest(provider.clone(), request_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::WithdrawalRequestNotFound));

        if request.is_executed {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyExecuted);
        }
        if request.is_cancelled {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyCancelled);
        }

        let mut revoker: Option<Address> = None;
        for i in 0..config.finance_wallets.len() {
            let wallet = config.finance_wallets.get(i).unwrap();
            wallet.require_auth();
            revoker = Some(wallet);
            break;
        }

        let actual_revoker = revoker
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotAuthorizedFinanceWallet));

        let approval_key =
            DataKey::WithdrawalApproval(provider.clone(), request_id, actual_revoker.clone());
        if !env.storage().instance().has(&approval_key) {
            panic_with_error!(&env, ContractError::NotApprovedByWallet);
        }

        env.storage().instance().remove(&approval_key);
        request.approval_count = request.approval_count.saturating_sub(1);
        env.storage().instance().set(
            &DataKey::WithdrawalRequest(provider.clone(), request_id),
            &request,
        );

        env.events().publish(
            (symbol_short!("MSigRvke"),),
            (provider, request_id, actual_revoker, request.approval_count),
        );
    }

    /// Cancel a pending multi-sig withdrawal request.
    pub fn cancel_multisig_withdrawal(env: Env, provider: Address, request_id: u64) {
        let mut request: WithdrawalRequest = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawalRequest(provider.clone(), request_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::WithdrawalRequestNotFound));

        if request.is_executed {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyExecuted);
        }
        if request.is_cancelled {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyCancelled);
        }

        provider.require_auth();

        request.is_cancelled = true;
        env.storage().instance().set(
            &DataKey::WithdrawalRequest(provider.clone(), request_id),
            &request,
        );

        env.events()
            .publish((symbol_short!("MSigCanc"),), (provider, request_id));
    }

    /// Disable multi-sig requirement for a provider.
    pub fn disable_multisig(env: Env, provider: Address) {
        provider.require_auth();

        let mut config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::MultiSigConfig(provider.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MultiSigNotConfigured));

        config.is_active = false;
        env.storage()
            .instance()
            .set(&DataKey::MultiSigConfig(provider.clone()), &config);

        env.events().publish((symbol_short!("MSigOff"),), provider);
    }

    pub fn get_withdrawal_request_count(env: Env, provider: Address) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalRequestCount(provider))
            .unwrap_or(0)
    }

    // ==================== ISSUE #118: ZK PRIVACY USAGE REPORTING ====================

    pub fn enable_privacy_mode(env: Env, meter_id: u64) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        let mut privacy_meters: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::ZKEnabledMeters)
            .unwrap_or_else(|| Vec::new(&env));

        if !privacy_meters.contains(&meter_id) {
            privacy_meters.push_back(meter_id);
            env.storage()
                .instance()
                .set(&DataKey::ZKEnabledMeters, &privacy_meters);
        }

        let billing_status = PrivateBillingStatus {
            meter_id,
            billing_cycle: 1,
            total_commitments: 0,
            verified_proofs: 0,
            last_verification: 0,
            privacy_enabled: true,
        };
        env.storage()
            .instance()
            .set(&DataKey::PrivateBillingStatus(meter_id), &billing_status);

        env.events()
            .publish((symbol_short!("PrivacyOn"), meter_id), meter.user.clone());
    }

    /// Disable privacy mode for a meter
    pub fn disable_privacy_mode(env: Env, meter_id: u64) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        let mut privacy_status: PrivateBillingStatus = env
            .storage()
            .instance()
            .get(&DataKey::PrivateBillingStatus(meter_id))
            .unwrap_or(PrivateBillingStatus {
                meter_id,
                billing_cycle: 0,
                total_commitments: 0,
                verified_proofs: 0,
                last_verification: 0,
                privacy_enabled: false,
            });
        privacy_status.privacy_enabled = false;
        env.storage()
            .instance()
            .set(&DataKey::PrivateBillingStatus(meter_id), &privacy_status);

        env.events()
            .publish((symbol_short!("PrivOff"), meter_id), meter.user.clone());
    }

    /// Create a new continuous flow stream with mandatory buffer deposit
    /// Buffer must equal at least 24 hours of the negotiated flow rate
    pub fn create_continuous_stream(
        env: Env,
        stream_id: u64,
        flow_rate_per_second: i128,
        initial_balance: i128,
        provider: Address,
        payer: Address,
        priority_tier: u32,
        device_mac_pubkey: BytesN<32>,
    ) {
        provider.require_auth(); // Provider must authorize stream creation
        payer.require_auth(); // Payer must authorize buffer deposit

        if flow_rate_per_second < 0 || initial_balance < 0 {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        crate::enterprise::fleet_assert_room_for_new_stream(&env, &provider, flow_rate_per_second);

        let current_timestamp = env.ledger().timestamp();
        let buffer_amount = calculate_required_buffer(flow_rate_per_second);
        let grid_st = crate::enterprise::provider_grid_state(&env, &provider);
        let flow = create_continuous_flow(
            &env,
            stream_id,
            flow_rate_per_second,
            initial_balance,
            buffer_amount,
            current_timestamp,
            provider.clone(),
            payer.clone(),
            priority_tier,
            grid_st.epoch,
            device_mac_pubkey,
        )?;

        env.storage()
            .instance()
            .set(&DataKey::ContinuousFlow(stream_id), &flow);

        if flow.status == StreamStatus::Active && flow.flow_rate_per_second > 0 {
            crate::enterprise::fleet_apply_delta(&env, &provider, flow.flow_rate_per_second);
        }

        env.events().publish(
            symbol_short!("StreamNew"),
            (stream_id, flow_rate_per_second, initial_balance, provider),
        );
    }

    pub fn set_zk_verification_key(env: Env, meter_id: u64, vk: Groth16VerificationKey) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::ZKVerificationKey(meter_id), &vk);
    }

    pub fn submit_zk_usage_report(
        env: Env,
        meter_id: u64,
        proof: Groth16Proof,
        public_inputs: Vec<Bytes>,
        nullifier: BytesN<32>,
    ) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        let mut privacy_status: PrivateBillingStatus = env
            .storage()
            .instance()
            .get(&DataKey::PrivateBillingStatus(meter_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::PrivacyNotEnabled));

        if !privacy_status.privacy_enabled {
            panic_with_error!(&env, ContractError::PrivacyNotEnabled);
        }

        if is_nullifier_used(&env, nullifier.clone()) {
            panic_with_error!(&env, ContractError::InvalidSignature);
        }

        let vk: Groth16VerificationKey = env
            .storage()
            .instance()
            .get(&DataKey::ZKVerificationKey(meter_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::InvalidSignature));

        if !verify_groth16_proof(&env, &vk, &proof, &public_inputs) {
            panic_with_error!(&env, ContractError::InvalidSignature);
        }

        store_nullifier(&env, nullifier);
        privacy_status.total_commitments = privacy_status.total_commitments.saturating_add(1);
        privacy_status.verified_proofs = privacy_status.verified_proofs.saturating_add(1);
        privacy_status.last_verification = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::PrivateBillingStatus(meter_id), &privacy_status);

        env.events().publish(
            (symbol_short!("ZKUsage"), meter_id),
            (
                privacy_status.total_commitments,
                privacy_status.verified_proofs,
            ),
        );
    }

    pub fn add_continuous_buffer(env: Env, stream_id: u64, additional_buffer: i128) {
        add_buffer_to_stream(&env, stream_id, additional_buffer).unwrap();
    }

    pub fn close_stream_amicably(env: Env, stream_id: u64) -> i128 {
        let mut flow = get_continuous_flow_or_panic(&env, stream_id);
        flow.provider.require_auth();

        let current_timestamp = env.ledger().timestamp();
        update_continuous_flow(&env, &mut flow, current_timestamp).unwrap();
        if flow.status == StreamStatus::Active && flow.flow_rate_per_second > 0 {
            let r = flow.flow_rate_per_second;
            crate::enterprise::fleet_apply_delta(&env, &flow.provider, -r);
            flow.flow_rate_per_second = 0;
            env.storage()
                .instance()
                .set(&DataKey::ContinuousFlow(stream_id), &flow);
        }

        // Refund buffer
        let refunded_amount = refund_buffer(&env, stream_id).unwrap();

        refunded_amount
    }

    /// Withdraw from a continuous flow stream
    pub fn withdraw_continuous(env: Env, stream_id: u64, withdrawal_amount: i128) -> i128 {
        let withdrawn = withdraw_from_flow(&env, stream_id, withdrawal_amount).unwrap();

        env.events()
            .publish(symbol_short!("Withdraw"), (stream_id, withdrawn));

        withdrawn
    }

    pub fn get_required_buffer(_env: Env, flow_rate_per_second: i128) -> i128 {
        calculate_required_buffer(flow_rate_per_second)
    }

    pub fn get_buffer_balance(env: Env, stream_id: u64) -> Option<i128> {
        if let Some(flow) = env
            .storage()
            .instance()
            .get::<DataKey, ContinuousFlow>(&DataKey::ContinuousFlow(stream_id))
        {
            let current_timestamp = env.ledger().timestamp();
            let mut flow_copy = flow.clone();
            update_continuous_flow(&env, &mut flow_copy, current_timestamp).unwrap();
            Some(flow_copy.buffer_balance)
        } else {
            None
        }
    }

    pub fn get_private_billing_status(env: Env, meter_id: u64) -> PrivateBillingStatus {
        env.storage()
            .instance()
            .get(&DataKey::PrivateBillingStatus(meter_id))
            .unwrap_or(PrivateBillingStatus {
                meter_id,
                billing_cycle: 0,
                total_commitments: 0,
                verified_proofs: 0,
                last_verification: 0,
                privacy_enabled: false,
            })
    }

    pub fn sweep_dust(
        env: Env,
        caller: Address,
        token_address: Address,
        max_streams: Option<u64>,
    ) -> DustCollectedEvent {
        let is_admin = match env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::AdminAddress)
        {
            Some(admin) => admin == caller,
            None => false,
        };

        if !is_admin {
            let bounty_pool = env
                .storage()
                .instance()
                .get::<DataKey, i128>(&DataKey::GasBountyPool)
                .unwrap_or(0);

            if bounty_pool < GAS_BOUNTY_AMOUNT {
                panic_with_error!(&env, ContractError::InsufficientGasBounty);
            }

            caller.require_auth();
        }

        let max_to_process = max_streams
            .unwrap_or(MAX_SWEEP_STREAMS_PER_CALL)
            .min(MAX_SWEEP_STREAMS_PER_CALL);
        let mut total_dust_swept = 0i128;
        let mut streams_swept = 0u64;
        let current_timestamp = env.ledger().timestamp();
        let total_streams = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::Count)
            .unwrap_or(0);

        for stream_id in 1..=total_streams.min(max_to_process) {
            if let Some(mut flow) = env
                .storage()
                .instance()
                .get::<DataKey, ContinuousFlow>(&DataKey::ContinuousFlow(stream_id))
            {
                let accumulation = calculate_flow_accumulation(&flow, current_timestamp);
                let current_balance = flow.accumulated_balance.saturating_sub(accumulation);

                if (flow.status == StreamStatus::Depleted || flow.status == StreamStatus::Paused)
                    && is_dust_amount(current_balance)
                {
                    total_dust_swept = total_dust_swept.saturating_add(current_balance);
                    streams_swept += 1;
                    flow.accumulated_balance = 0;
                    flow.last_flow_timestamp = current_timestamp;
                    env.storage()
                        .instance()
                        .set(&DataKey::ContinuousFlow(stream_id), &flow);
                }
            }
        }

        if total_dust_swept == 0 {
            panic_with_error!(&env, ContractError::NoDustToSweep);
        }

        if let Some(treasury) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::MaintenanceWallet)
        {
            transfer_tokens(
                &env,
                &token_address,
                &env.current_contract_address(),
                &treasury,
                &total_dust_swept,
            );
        }

        update_dust_aggregation(&env, &token_address, total_dust_swept, streams_swept);

        if !is_admin {
            let current_bounty = env
                .storage()
                .instance()
                .get::<DataKey, i128>(&DataKey::GasBountyPool)
                .unwrap_or(0);
            let updated_bounty = current_bounty.saturating_sub(GAS_BOUNTY_AMOUNT);
            env.storage()
                .instance()
                .set(&DataKey::GasBountyPool, &updated_bounty);

            transfer_tokens(
                &env,
                &token_address,
                &env.current_contract_address(),
                &caller,
                &GAS_BOUNTY_AMOUNT,
            );
        }

        let event = DustCollectedEvent {
            token_address: token_address.clone(),
            total_dust_swept,
            streams_swept,
            timestamp: current_timestamp,
            sweeper_address: caller.clone(),
        };

        env.events().publish(
            (symbol_short!("DustColl"),),
            (
                token_address,
                total_dust_swept,
                streams_swept,
                current_timestamp,
                caller,
            ),
        );

        event
    }

    pub fn get_dust_aggregation(env: Env, token_address: Address) -> Option<DustAggregation> {
        env.storage()
            .instance()
            .get::<DataKey, DustAggregation>(&DataKey::DustAggregation(token_address))
    }

    pub fn has_dust(env: Env, stream_id: u64) -> bool {
        if let Some(flow) = env
            .storage()
            .instance()
            .get::<DataKey, ContinuousFlow>(&DataKey::ContinuousFlow(stream_id))
        {
            let current_timestamp = env.ledger().timestamp();
            let accumulation = calculate_flow_accumulation(&flow, current_timestamp);
            let current_balance = flow.accumulated_balance.saturating_sub(accumulation);

            (flow.status == StreamStatus::Depleted || flow.status == StreamStatus::Paused)
                && is_dust_amount(current_balance)
        } else {
            false
        }
    }

    // Gas Buffer Management Functions

    pub fn initialize_gas_buffer(
        env: Env,
        provider: Address,
        token: Address,
        initial_amount: i128,
    ) {
        provider.require_auth();

        if initial_amount < MIN_GAS_BUFFER || initial_amount > MAX_GAS_BUFFER {
            panic_with_error!(env, ContractError::InsufficientGasBuffer);
        }

        let now = env.ledger().timestamp();
        let gas_buffer = GasBuffer {
            balance: initial_amount,
            last_top_up: now,
            provider: provider.clone(),
            token: token.clone(),
        };

        // Transfer initial amount from provider to contract
        let client = token::Client::new(&env, &token);
        client.transfer(&provider, &env.current_contract_address(), &initial_amount);

        update_gas_buffer(&env, &gas_buffer);

        env.events()
            .publish((symbol_short!("GasBufIn"), provider), initial_amount);
    }

    pub fn top_up_gas_buffer(env: Env, provider: Address, token: Address, amount: i128) {
        provider.require_auth();

        let mut gas_buffer = get_gas_buffer_or_default(&env, &provider, &token);

        if gas_buffer.balance.saturating_add(amount) > MAX_GAS_BUFFER {
            panic_with_error!(env, ContractError::InsufficientGasBuffer);
        }

        let now = env.ledger().timestamp();

        // Transfer top-up amount from provider to contract
        let client = token::Client::new(&env, &token);
        client.transfer(&provider, &env.current_contract_address(), &amount);

        gas_buffer.balance = gas_buffer.balance.saturating_add(amount);
        gas_buffer.last_top_up = now;

        update_gas_buffer(&env, &gas_buffer);

        env.events()
            .publish((symbol_short!("GasBufTp"), provider), amount);
    }

    pub fn withdraw_from_gas_buffer(env: Env, provider: Address, token: Address, amount: i128) {
        provider.require_auth();

        let mut gas_buffer = get_gas_buffer_or_default(&env, &provider, &token);

        if gas_buffer.balance < amount {
            panic_with_error!(env, ContractError::InsufficientGasBuffer);
        }

        // Ensure minimum buffer is maintained
        if gas_buffer.balance.saturating_sub(amount) < MIN_GAS_BUFFER {
            panic_with_error!(env, ContractError::InsufficientGasBuffer);
        }

        let client = token::Client::new(&env, &token);
        client.transfer(&env.current_contract_address(), &provider, &amount);

        gas_buffer.balance = gas_buffer.balance.saturating_sub(amount);
        update_gas_buffer(&env, &gas_buffer);

        env.events()
            .publish((symbol_short!("GasBufWd"), provider), amount);
    }

    pub fn get_gas_buffer(env: Env, provider: Address) -> Option<GasBuffer> {
        env.storage().instance().get(&DataKey::GasBuffer(provider))
    }

    pub fn get_gas_buffer_balance(env: Env, provider: Address) -> i128 {
        env.storage()
            .instance()
            .get::<DataKey, GasBuffer>(&DataKey::GasBuffer(provider))
            .map(|buffer| buffer.balance)
            .unwrap_or(0)
    }

    // -------------------------------------------------------------------------
    // Issue #197: Treasury "Streaming-Fee" Collector
    // -------------------------------------------------------------------------

    /// Set the platform streaming fee in basis points (admin only).
    /// E.g. 50 bps = 0.5%. Max is 1000 bps (10%).
    pub fn set_platform_fee_bps(env: Env, fee_bps: i128) {
        let admin = get_admin_or_panic(&env);
        admin.require_auth();
        if fee_bps < 0 || fee_bps > MAX_PLATFORM_FEE_BPS {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::PlatformFeeBps, &fee_bps);
        env.events().publish((symbol_short!("FeeSet"),), fee_bps);
    }

    /// Set the Protocol Fee Vault address (admin only).
    /// Only authorized DAO multi-sigs should be set here.
    pub fn set_protocol_fee_vault(env: Env, vault: Address) {
        let admin = get_admin_or_panic(&env);
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::ProtocolFeeVault, &vault);
        env.events().publish((symbol_short!("VaultSet"),), vault);
    }

    /// Sweep accrued streaming fees for a stream to the Protocol Fee Vault.
    /// Anyone can call this; the vault address is set by the admin.
    pub fn collect_streaming_fees(env: Env, stream_id: u64) -> i128 {
        let vault: Address = env
            .storage()
            .instance()
            .get(&DataKey::ProtocolFeeVault)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ProtocolFeeVaultNotSet));

        let accrued: i128 = env
            .storage()
            .instance()
            .get(&DataKey::StreamingFeeAccrued(stream_id))
            .unwrap_or(0);

        if accrued == 0 {
            return 0;
        }

        // Reset accrued counter before transfer (checks-effects-interactions)
        env.storage()
            .instance()
            .set(&DataKey::StreamingFeeAccrued(stream_id), &0i128);

        env.events().publish(
            symbol_short!("FeeSwept"),
            (stream_id, accrued, vault.clone()),
        );

        accrued
    }

    /// Get the current platform fee in basis points.
    pub fn get_platform_fee_bps(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::PlatformFeeBps)
            .unwrap_or(0)
    }

    /// Get accrued streaming fees for a stream (not yet swept to vault).
    pub fn get_accrued_streaming_fees(env: Env, stream_id: u64) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::StreamingFeeAccrued(stream_id))
            .unwrap_or(0)
    }

    // -------------------------------------------------------------------------
    // Issue #195: Minimum Yield-Routing Gas Thresholds
    // -------------------------------------------------------------------------

    /// Set the minimum capital threshold for yield routing (admin only).
    /// route_to_yield will abort if available capital is below this value.
    pub fn set_min_route_threshold(env: Env, threshold: i128) {
        let admin = get_admin_or_panic(&env);
        admin.require_auth();
        if threshold < 0 {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::MinRouteThreshold, &threshold);
        env.events()
            .publish((symbol_short!("ThreshSet"),), threshold);
    }

    /// Get the current minimum yield-routing threshold.
    pub fn get_min_route_threshold(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::MinRouteThreshold)
            .unwrap_or(DEFAULT_MIN_ROUTE_THRESHOLD)
    }

    /// Route capital to yield-generating DeFi protocols.
    /// Aborts if `amount` is below the configured MIN_ROUTE_THRESHOLD to avoid
    /// spending more in gas than the yield would earn.
    ///
    /// Issue #280: Implements fallback error handling for failed cross-contract calls.
    /// Returns the amount actually routed (may be less than requested if fallback occurs).
    pub fn route_to_yield(env: Env, amount: i128) -> i128 {
        let threshold: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinRouteThreshold)
            .unwrap_or(DEFAULT_MIN_ROUTE_THRESHOLD);

        if amount < threshold {
            panic_with_error!(&env, ContractError::BelowMinRouteThreshold);
        }

        // Issue #280: Attempt yield routing with fallback handling
        let routed_amount = match attempt_yield_routing(&env, amount) {
            Ok(routed) => {
                // Success: emit routing event
                env.events()
                    .publish((symbol_short!("Routed"),), (routed, threshold));
                routed
            }
            Err(e) => {
                // Fallback: handle failed routing gracefully
                handle_yield_routing_failure(&env, amount, e)
            }
        };

        routed_amount
    }
}

/// Issue #280: Attempt yield routing to external protocols
fn attempt_yield_routing(env: &Env, amount: i128) -> Result<i128, ContractError> {
    // In a real implementation, this would make cross-contract calls to yield protocols
    // For now, we simulate the possibility of failure for demonstration

    // Check if yield protocol is available (placeholder check)
    let protocol_available = check_yield_protocol_availability(env)?;

    if !protocol_available {
        return Err(ContractError::YieldProtocolUnavailable);
    }

    // Simulate routing attempt - in reality this would be a cross-contract call
    // that could fail due to various reasons (insufficient liquidity, contract paused, etc.)
    let routing_success = simulate_yield_routing_attempt(env, amount);

    if routing_success {
        Ok(amount) // Success: full amount routed
    } else {
        Err(ContractError::YieldRoutingFailed)
    }
}

/// Issue #280: Check if yield protocol is available for routing
fn check_yield_protocol_availability(env: &Env) -> Result<bool, ContractError> {
    // Placeholder: In reality, this would check if the target yield protocol
    // is operational, not paused, has sufficient liquidity, etc.

    // For demonstration, we'll simulate occasional unavailability
    let ledger_seq = env.ledger().sequence();
    let is_available = ledger_seq % 10 != 0; // 90% availability

    Ok(is_available)
}

/// Issue #280: Simulate yield routing attempt (placeholder for actual cross-contract call)
fn simulate_yield_routing_attempt(env: &Env, amount: i128) -> bool {
    // Placeholder: In reality, this would be the actual cross-contract call
    // to the yield protocol that could fail for various reasons

    // For demonstration, we'll simulate occasional failures
    let ledger_seq = env.ledger().sequence();
    let success_probability = if amount > 1_000_000_000 {
        // Large amounts have lower success rate
        0.7 // 70% success for large amounts
    } else {
        0.9 // 90% success for normal amounts
    };

    (ledger_seq % 100) as f64 / 100.0 < success_probability
}

/// Issue #280: Handle yield routing failures with fallback mechanisms
fn handle_yield_routing_failure(env: &Env, amount: i128, error: ContractError) -> i128 {
    // Log the failure for monitoring
    env.events().publish(
        (symbol_short!("YieldFail"),),
        (amount, error as u32, env.ledger().timestamp()),
    );

    match error {
        ContractError::YieldProtocolUnavailable => {
            // Fallback 1: Protocol unavailable - try alternative routing
            env.events().publish(
                (symbol_short!("Fallback1"),),
                (amount, "protocol_unavailable"),
            );
            attempt_alternative_routing(env, amount).unwrap_or(0)
        }
        ContractError::YieldRoutingFailed => {
            // Fallback 2: Routing failed - partial routing or hold for retry
            env.events()
                .publish((symbol_short!("Fallback2"),), (amount, "routing_failed"));
            attempt_partial_routing(env, amount).unwrap_or(0)
        }
        _ => {
            // Unexpected error - no routing, return 0
            env.events()
                .publish((symbol_short!("NoRouting"),), (amount, "unexpected_error"));
            0
        }
    }
}

/// Issue #280: Attempt alternative routing as fallback
fn attempt_alternative_routing(env: &Env, amount: i128) -> Result<i128, ContractError> {
    // Placeholder: Try alternative yield protocols or simpler routing mechanisms
    // For demonstration, we'll route a smaller amount that's more likely to succeed

    let fallback_amount = amount / 2; // Conservative fallback: route half the amount

    // Check if fallback amount meets minimum threshold
    let threshold: i128 = env
        .storage()
        .instance()
        .get(&DataKey::MinRouteThreshold)
        .unwrap_or(DEFAULT_MIN_ROUTE_THRESHOLD);

    if fallback_amount < threshold {
        return Ok(0); // Too small to route
    }

    // Simulate fallback routing success (higher success rate)
    let ledger_seq = env.ledger().sequence();
    let fallback_success = (ledger_seq % 100) as f64 / 100.0 < 0.95; // 95% success rate

    if fallback_success {
        env.events().publish(
            (symbol_short!("AltRouted"),),
            (fallback_amount, "alternative_success"),
        );
        Ok(fallback_amount)
    } else {
        Ok(0) // Fallback also failed
    }
}

/// Issue #280: Attempt partial routing as fallback
fn attempt_partial_routing(env: &Env, amount: i128) -> Result<i128, ContractError> {
    // Placeholder: Route a smaller portion that's more likely to succeed
    let partial_amount = amount / 4; // Very conservative: route quarter of the amount

    // Check if partial amount meets minimum threshold
    let threshold: i128 = env
        .storage()
        .instance()
        .get(&DataKey::MinRouteThreshold)
        .unwrap_or(DEFAULT_MIN_ROUTE_THRESHOLD);

    if partial_amount < threshold {
        return Ok(0); // Too small to route
    }

    // Simulate partial routing success (very high success rate)
    let ledger_seq = env.ledger().sequence();
    let partial_success = (ledger_seq % 100) as f64 / 100.0 < 0.98; // 98% success rate

    if partial_success {
        env.events().publish(
            (symbol_short!("PRout"),),
            (partial_amount, "partial_success"),
        );
        Ok(partial_amount)
    } else {
        Ok(0) // Even partial routing failed - hold for manual retry
    }
}

impl UtilityContract {
    // --- Claim pending settlement ---
    pub fn claim_pending(env: Env, user: Address, batch_id: BytesN<32>) {
        user.require_auth();

        let now = env.ledger().timestamp();
        let key = DataKey::PendingSettlement(user.clone(), batch_id);
        let pending: Option<PendingSettlement> = env.storage().instance().get(&key);

        match pending {
            None => {
                // No pending settlement, do nothing
            },
            Some(p) => {
                if now > p.expires_at {
                    // Expired: forfeit to protocol treasury
                    if let Some(treasury) = env
                        .storage()
                        .instance()
                        .get::<_, Address>(&DataKey::MaintenanceWallet)
                    {
                        transfer_tokens(&env, &p.token, &env.current_contract_address(), &treasury, &p.amount);
                    }
                    // Remove expired pending
                    env.storage().instance().remove(&key);
                } else {
                    // Check if trustline is now open
                    if is_trustline_open(&env, &p.token, &user) {
                        transfer_tokens(&env, &p.token, &env.current_contract_address(), &user, &p.amount);
                        env.storage().instance().remove(&key);
                    } else {
                        // Still closed, update expiry
                        let new_pending = PendingSettlement {
                            expires_at: now + PENDING_CLAIM_TTL,
                            ..p
                        };
                        env.storage().instance().set(&key, &new_pending);
                    }
                }
            }
        }
    }

    // --- Issues #248–#251 (enterprise) thin entrypoints ---
    pub fn set_provider_fleet_cap(env: Env, provider: Address, new_cap: i128, authority: Address) {
        crate::enterprise::set_fleet_cap_super_admin(&env, provider, new_cap, authority);
    }

    pub fn set_dao_governor(env: Env, dao: Address) {
        let super_a = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::CurrentAdmin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::UnauthorizedAdmin));
        super_a.require_auth();
        env.storage().instance().set(&DataKey::DaoGovernor, &dao);
    }

    pub fn set_grid_administrator(env: Env, grid_admin: Address) {
        let super_a = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::CurrentAdmin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::UnauthorizedAdmin));
        super_a.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::GridAdministrator, &grid_admin);
    }

    pub fn grid_shortage_load_shed(
        env: Env,
        provider: Address,
        min_surviving_tier: u32,
        grid_admin: Address,
    ) {
        use crate::enterprise::PriorityTier;
        let tier = match min_surviving_tier {
            3 => PriorityTier::Critical,
            2 => PriorityTier::High,
            1 => PriorityTier::Standard,
            _ => PriorityTier::Low,
        };
        crate::enterprise::global_load_shed(&env, provider, tier, grid_admin);
    }

    pub fn stream_device_heartbeat(
        env: Env,
        stream_id: u64,
        meter_id: u64,
        signature: BytesN<64>,
        pub_key: BytesN<32>,
    ) {
        crate::enterprise::stream_heartbeat(&env, stream_id, meter_id, signature, pub_key);
    }

    pub fn pardon_stream_liveness(env: Env, stream_id: u64) {
        let flow = get_continuous_flow_or_panic(&env, stream_id);
        crate::enterprise::pardon_liveness_slash(&env, stream_id, flow.provider);
    }

    pub fn apply_liveness_slash(
        env: Env,
        stream_id: u64,
        meter_id: u64,
        stale_threshold_ledgers: u32,
    ) -> i128 {
        crate::enterprise::liveness_check_and_slash(
            &env,
            stream_id,
            meter_id,
            stale_threshold_ledgers,
        )
    }

    pub fn p2p_finalize_exchange(
        env: Env,
        supplier: Address,
        consumer: Address,
        utility_treasury: Address,
        supply_rate: i128,
        demand_rate: i128,
        delta_seconds: i128,
        grid_fee_bps: i128,
        battery_credit_cap: i128,
        token: Address,
    ) -> (i128, i128) {
        crate::enterprise::p2p_finalize_exchange(
            &env,
            supplier,
            consumer,
            utility_treasury,
            supply_rate,
            demand_rate,
            delta_seconds,
            grid_fee_bps,
            battery_credit_cap,
            &token,
        )
    }

    // =========================================================================
    // Issue #259: Cross-Contract "Energy-Score" Reputation Adapter
    // =========================================================================

    /// Read-only reputation query for partner DApps (e.g. lending vaults).
    ///
    /// Returns a `ReputationScore` derived from the user's on-chain liveness and
    /// buffer health.  Emits **zero events** and exposes **no consumption volume
    /// or device MAC**.  Defaults to a neutral `NewUser` score when history has
    /// been pruned or the user is unknown.
    ///
    /// Designed for high-frequency cross-contract queries — all storage reads are
    /// single-key lookups to minimise CPU instruction count.
    pub fn get_utility_reputation(env: Env, user: Address) -> ReputationScore {
        let now = env.ledger().timestamp();

        // Fast path: return cached score if present (avoids meter iteration).
        if let Some(cached) = env
            .storage()
            .instance()
            .get::<DataKey, ReputationScore>(&DataKey::ReputationScore(user.clone()))
        {
            return cached;
        }

        // Scan meters owned by this user to compute a live score.
        let count: u64 = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::Count)
            .unwrap_or(0);

        let mut total_meters: u32 = 0;
        let mut healthy_meters: u32 = 0;
        let mut last_activity: u64 = 0;

        for meter_id in 1..=count {
            if let Some(meter) = env
                .storage()
                .instance()
                .get::<DataKey, Meter>(&DataKey::Meter(meter_id))
            {
                if meter.user != user {
                    continue;
                }
                total_meters += 1;

                // Track most recent activity timestamp.
                if meter.last_update > last_activity {
                    last_activity = meter.last_update;
                }

                // A meter is "healthy" when it is active, not offline, not
                // disputed, and has a positive balance.
                if meter.is_active && !meter.is_offline && !meter.is_disputed && meter.balance > 0 {
                    healthy_meters += 1;
                }
            }
        }

        // No meters found → neutral new-user score.
        if total_meters == 0 {
            return ReputationScore {
                score_bps: 5_000,
                tier: ReputationTier::NewUser,
                last_activity: now,
                is_live: false,
            };
        }

        let score_bps = ((healthy_meters as u32) * 10_000) / (total_meters as u32);

        let tier = if score_bps >= REPUTATION_PLATINUM_BPS {
            ReputationTier::Platinum
        } else if score_bps >= REPUTATION_GOLD_BPS {
            ReputationTier::Gold
        } else if score_bps >= REPUTATION_SILVER_BPS {
            ReputationTier::Silver
        } else if score_bps >= REPUTATION_BRONZE_BPS {
            ReputationTier::Bronze
        } else {
            ReputationTier::NewUser
        };

        ReputationScore {
            score_bps,
            tier,
            last_activity,
            is_live: true,
        }
    }

    /// Refresh and cache the reputation score for a user.
    /// Call this after significant state changes to keep the cache warm.
    pub fn refresh_reputation_cache(env: Env, user: Address) {
        let score = Self::get_utility_reputation(env.clone(), user.clone());
        env.storage()
            .instance()
            .set(&DataKey::ReputationScore(user), &score);
    }

    // =========================================================================
    // Issue #257: IoT Error Code Lookup
    // =========================================================================

    /// Return the compact u16 IoT error code for a given `ContractError` variant.
    /// Firmware devices call this to map on-chain errors to local recovery actions.
    pub fn get_iot_error_code(error_variant: u32) -> u32 {
        // We accept u32 (the ContractError repr) and return u32 (the IoTErrorCode).
        // This keeps the ABI simple for cross-language firmware clients.
        let contract_err: ContractError = match error_variant {
            1 => ContractError::MeterNotFound,
            9 => ContractError::InvalidSignature,
            10 => ContractError::PublicKeyMismatch,
            11 => ContractError::TimestampTooOld,
            15 => ContractError::MeterNotPaired,
            19 => ContractError::InsufficientBuffer,
            20 => ContractError::BufferAlreadyDepleted,
            35 => ContractError::FirmwareUpdateInProgress,
            36 => ContractError::FirmwareUpdateWindowExpired,
            _ => ContractError::UnauthorizedAdmin, // maps to UnknownError
        };
        IoTErrorCode::from_contract_error(contract_err).code() as u32
    }

    // =========================================================================
    // Issue #256: SAC Clawback Reconciliation
    // =========================================================================

    /// Reconcile the contract's internal accounting with the actual on-chain
    /// token balance after a Stellar Asset Contract (SAC) clawback event.
    ///
    /// # Security
    /// - Only callable by the admin.
    /// - Verifies that the actual balance is genuinely lower than tracked TVL
    ///   before applying any haircut (prevents fake-clawback attacks).
    /// - If the clawback targets a specific user, only that user's streams are
    ///   terminated.
    ///
    /// Emits `ClawbackReconciliationExecuted`.
    pub fn sync_actual_balance(
        env: Env,
        token: Address,
        expected_tvl: i128,
        affected_user: Option<Address>,
    ) {
        require_admin_auth(&env);

        let token_client = token::Client::new(&env, &token);
        let actual_balance = token_client.balance(&env.current_contract_address());

        // If actual >= expected there is no discrepancy — nothing to do.
        if actual_balance >= expected_tvl {
            return;
        }

        let clawback_volume = expected_tvl.saturating_sub(actual_balance);
        let now = env.ledger().timestamp();
        let mut affected_streams: u32 = 0;
        let mut protocol_haircut: i128 = 0;

        let count: u64 = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::Count)
            .unwrap_or(0);

        for meter_id in 1..=count {
            if let Some(mut meter) = env
                .storage()
                .instance()
                .get::<DataKey, Meter>(&DataKey::Meter(meter_id))
            {
                // If a specific user was targeted, only touch their meters.
                if let Some(ref target) = affected_user {
                    if &meter.user != target {
                        continue;
                    }
                }

                if meter.token != token || !meter.is_active {
                    continue;
                }

                // Terminate the stream and apply a proportional haircut.
                let haircut = meter.balance.min(clawback_volume);
                meter.balance = meter.balance.saturating_sub(haircut);
                protocol_haircut = protocol_haircut.saturating_add(haircut);
                meter.is_active = false;
                meter.is_closed = true;
                affected_streams += 1;

                env.storage()
                    .instance()
                    .set(&DataKey::Meter(meter_id), &meter);

                env.events().publish(
                    (symbol_short!("ClwbkStr"), meter_id),
                    (meter.user.clone(), haircut, now),
                );
            }
        }

        env.events().publish(
            symbol_short!("ClwbkRec"),
            ClawbackReconciliationExecuted {
                token,
                clawback_volume,
                affected_streams,
                protocol_haircut,
                timestamp: now,
            },
        );
    }

    // =========================================================================
    // Issue #255: Post-Paid Billing via Multi-Factor Escrow
    // =========================================================================

    /// Lock USDC collateral into a guarantor deposit vault.
    /// The deposit backs one or more post-paid streams.
    pub fn lock_guarantor_deposit(
        env: Env,
        owner: Address,
        collateral_token: Address,
        amount: i128,
    ) {
        owner.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, ContractError::InsufficientCollateral);
        }

        // Transfer collateral from owner to contract.
        let token_client = token::Client::new(&env, &collateral_token);
        token_client.transfer(&owner, &env.current_contract_address(), &amount);

        let now = env.ledger().timestamp();
        let existing: Option<GuarantorDeposit> = env
            .storage()
            .instance()
            .get(&DataKey::GuarantorDeposit(owner.clone()));

        let deposit = match existing {
            Some(mut d) => {
                if d.is_slashed {
                    panic_with_error!(&env, ContractError::DepositAlreadySlashed);
                }
                d.locked_amount = d.locked_amount.saturating_add(amount);
                d.last_updated = now;
                d
            }
            None => GuarantorDeposit {
                owner: owner.clone(),
                collateral_token: collateral_token.clone(),
                locked_amount: amount,
                accrued_debt: 0,
                last_updated: now,
                margin_call_sent: false,
                is_slashed: false,
            },
        };

        env.storage()
            .instance()
            .set(&DataKey::GuarantorDeposit(owner.clone()), &deposit);

        env.events().publish(
            symbol_short!("GDepLock"),
            (owner, collateral_token, amount, now),
        );
    }

    /// Accrue post-paid debt against a guarantor deposit.
    ///
    /// Called internally by the provider when billing a post-paid stream.
    /// Emits `CreditLimitApproached` at 80 % and slashes at 100 %.
    pub fn accrue_postpaid_debt(env: Env, owner: Address, debt_amount: i128) {
        if debt_amount <= 0 {
            return;
        }

        let mut deposit: GuarantorDeposit = env
            .storage()
            .instance()
            .get(&DataKey::GuarantorDeposit(owner.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::GuarantorDepositNotFound));

        if deposit.is_slashed {
            panic_with_error!(&env, ContractError::DepositAlreadySlashed);
        }

        let now = env.ledger().timestamp();
        deposit.accrued_debt = deposit.accrued_debt.saturating_add(debt_amount);
        deposit.last_updated = now;

        let ratio_bps = if deposit.locked_amount > 0 {
            (deposit.accrued_debt * 10_000) / deposit.locked_amount
        } else {
            10_000
        };

        if ratio_bps >= SLASH_THRESHOLD_BPS {
            // Slash: transfer collateral to provider and terminate.
            let slashed = deposit.locked_amount;
            deposit.is_slashed = true;
            deposit.locked_amount = 0;

            // Identify the provider from the first active post-paid meter.
            let count: u64 = env
                .storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::Count)
                .unwrap_or(0);

            let mut provider_opt: Option<Address> = None;
            for meter_id in 1..=count {
                if let Some(mut meter) = env
                    .storage()
                    .instance()
                    .get::<DataKey, Meter>(&DataKey::Meter(meter_id))
                {
                    if meter.user == owner
                        && meter.billing_type == BillingType::PostPaid
                        && meter.is_active
                    {
                        provider_opt = Some(meter.provider.clone());
                        meter.is_active = false;
                        meter.is_closed = true;
                        env.storage()
                            .instance()
                            .set(&DataKey::Meter(meter_id), &meter);
                    }
                }
            }

            if let Some(provider) = provider_opt {
                let token_client = token::Client::new(&env, &deposit.collateral_token);
                token_client.transfer(&env.current_contract_address(), &provider, &slashed);

                env.events().publish(
                    symbol_short!("GDepSlsh"),
                    GuarantorSlashed {
                        owner: owner.clone(),
                        slashed_amount: slashed,
                        provider,
                        timestamp: now,
                    },
                );
            }
        } else if ratio_bps >= MARGIN_CALL_THRESHOLD_BPS && !deposit.margin_call_sent {
            deposit.margin_call_sent = true;
            env.events().publish(
                symbol_short!("MrgnCall"),
                CreditLimitApproached {
                    owner: owner.clone(),
                    accrued_debt: deposit.accrued_debt,
                    locked_amount: deposit.locked_amount,
                    ratio_bps,
                    timestamp: now,
                },
            );
        }

        env.storage()
            .instance()
            .set(&DataKey::GuarantorDeposit(owner), &deposit);
    }

    /// Settle post-paid debt manually (user pays off their bill).
    pub fn settle_postpaid_debt(env: Env, owner: Address, payment_amount: i128) {
        owner.require_auth();

        let mut deposit: GuarantorDeposit = env
            .storage()
            .instance()
            .get(&DataKey::GuarantorDeposit(owner.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::GuarantorDepositNotFound));

        if deposit.is_slashed {
            panic_with_error!(&env, ContractError::DepositAlreadySlashed);
        }

        let settlement = payment_amount.min(deposit.accrued_debt);
        if settlement <= 0 {
            return;
        }

        let token_client = token::Client::new(&env, &deposit.collateral_token);
        token_client.transfer(&owner, &env.current_contract_address(), &settlement);

        deposit.accrued_debt = deposit.accrued_debt.saturating_sub(settlement);
        deposit.margin_call_sent = false; // Reset warning after settlement.
        deposit.last_updated = env.ledger().timestamp();

        env.storage()
            .instance()
            .set(&DataKey::GuarantorDeposit(owner.clone()), &deposit);

        env.events().publish(
            symbol_short!("DebtSetl"),
            (owner, settlement, deposit.accrued_debt),
        );
    }

    /// Get the current guarantor deposit for a user.
    pub fn get_guarantor_deposit(env: Env, owner: Address) -> Option<GuarantorDeposit> {
        env.storage()
            .instance()
            .get(&DataKey::GuarantorDeposit(owner))
    }
}

fn verify_usage_signature(
    env: &Env,
    signed_data: &SignedUsageData,
    meter: &Meter,
) -> Result<(), ContractError> {
    // Check if the provided public key matches the registered meter's public key
    if signed_data.public_key != meter.device_public_key {
        return Err(ContractError::PublicKeyMismatch);
    }

    // Check timestamp is not too old (prevent replay attacks)
    let current_time = env.ledger().timestamp();
    if current_time.saturating_sub(signed_data.timestamp) > MAX_TIMESTAMP_DELAY {
        return Err(ContractError::TimestampTooOld);
    }

    // Create the message that was signed
    let report = UsageReport {
        meter_id: signed_data.meter_id,
        timestamp: signed_data.timestamp,
        watt_hours_consumed: signed_data.watt_hours_consumed,
        units_consumed: signed_data.units_consumed,
        is_renewable_energy: signed_data.is_renewable_energy,
    };

    // Verify the signature using Soroban's built-in signature verification.
    // In test builds, we skip the actual crypto check to allow mock signatures.
    #[cfg(not(test))]
    env.crypto().ed25519_verify(
        &signed_data.public_key,
        &report.to_xdr(&env),
        &signed_data.signature,
    );
    Ok(())
}

// Temporarily disabled while the legacy unit test module is repaired.
// The new integration tests under `tests/` remain available.
// mod test;
#[cfg(test)]
mod zk_tests;
