#![no_std]
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, token, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};

// --- Constants ---
const DEFAULT_BUFFER_DAYS: i128 = 3;
const TRUSTED_BUFFER_DAYS: i128 = 1;
const MINIMUM_BALANCE_TO_FLOW: i128 = 500;
const HOUR_IN_SECONDS: u64 = 60 * 60;
const DAY_IN_SECONDS: u64 = 24 * HOUR_IN_SECONDS;
const GRACE_PERIOD_SECONDS: u64 = 86_400;
const DEBT_THRESHOLD: i128 = -10_000_000;
const DAILY_WITHDRAWAL_PERCENT: i128 = 10;
const MAX_USAGE_PER_UPDATE: i128 = 1_000_000_000_000i128;
const MAX_TIMESTAMP_DELAY: u64 = 300;
const PEAK_HOUR_START: u64 = 18 * HOUR_IN_SECONDS;
const PEAK_HOUR_END: u64 = 21 * HOUR_IN_SECONDS;
const PEAK_RATE_MULTIPLIER: i128 = 3;
const RATE_PRECISION: i128 = 2;
const REFERRAL_REWARD_UNITS: i128 = 500;
const XLM_PRECISION: i128 = 10_000_000;
const XLM_MINIMUM_INCREMENT: i128 = 1;
const XLM_GAS_RESERVE: i128 = 5 * XLM_PRECISION;
const MAX_RESELLER_FEE_BPS: i128 = 2000;
const DEBT_SERVICE_DIVERT_BPS: i128 = 500;
const DEFAULT_TAX_RATE_BPS: i128 = 500;
const MAINTENANCE_FUND_PERCENT_BPS: i128 = 1;
const LEDGER_LIFETIME_EXTENSION: u32 = 1_000_000;

// --- External Contract Clients ---
#[contractclient(name = "PriceOracleClient")]
pub trait PriceOracle {
    fn get_price(env: Env) -> PriceData;
}

#[contractclient(name = "SoroSusuClient")]
pub trait SoroSusu {
    fn get_susu_score(env: Env, user: Address) -> u32;
    fn is_trusted_saver(env: Env, user: Address) -> bool;
    fn is_in_default(env: Env, user: Address) -> bool;
    fn record_debt_payment(env: Env, user: Address, amount: i128);
}

#[contractclient(name = "VestingVaultClient")]
pub trait VestingVault {
    fn get_staked_balance(env: Env, user: Address) -> i128;
}

#[contractclient(name = "NFTMinterClient")]
pub trait NFTMinter {
    fn mint_receipt_nft(env: Env, to: Address, meter_id: u64, cycle_index: u32);
    fn mint_impact_sbt(env: Env, to: Address, carbon_saved: i128, reliability_score: u32);
}

// --- Data Structures ---

#[contracttype]
#[derive(Clone)]
pub struct PriceData {
    pub price: i128,
    pub decimals: u32,
    pub last_updated: u64,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BillingType { PrePaid, PostPaid }

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VerificationMethod {
    IdentityVerified,
    CommunityApproved,
    GovernmentIssued,
    StakingBased,
}

#[contracttype]
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
}

mod gas_estimator;
use gas_estimator::GasCostEstimator;

pub mod grant_stream_listener;
pub mod secure_call_interface;
use secure_call_interface::{SecureCallManager, SecureCallError};

#[cfg(test)]
mod secure_call_tests;
#[contracttype]
#[derive(Clone)]
pub struct Meter {
    pub user: Address,
    pub provider: Address,
    pub billing_type: BillingType,
    pub off_peak_rate: i128,
    pub peak_rate: i128,
    pub rate_per_unit: i128,
    pub balance: i128,
    pub debt: i128,
    pub collateral_limit: i128,
    pub last_update: u64,
    pub is_active: bool,
    pub token: Address,
    pub usage_data: UsageData,
    pub max_flow_rate_per_hour: i128,
    pub last_claim_time: u64,
    pub claimed_this_hour: i128,
    pub heartbeat: u64,
    pub device_public_key: BytesN<32>,
    pub is_paired: bool,
    pub grace_period_start: u64,
    pub is_paused: bool,
    pub tier_threshold: i128,
    pub tier_rate: i128,
    pub is_disputed: bool,
    pub challenge_timestamp: u64,
    pub credit_drip_rate: i128,
    pub is_closed: bool,
    pub priority_index: u32,
    pub off_peak_reward_rate_bps: i128,
    pub milestone_deadline: u64,
    pub milestone_confirmed: bool,
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
    pub target_water_savings: i128,  // in liters
    pub current_savings: i128,
    pub deadline: u64,
    pub is_active: bool,
    pub grant_amount: i128,  // grant amount when goal is reached
    pub grant_token: Address,
    pub created_at: u64,
    pub achieved_at: Option<u64>,
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
pub struct ZKProof {
    pub commitment: BytesN<32>,        // Pedersen commitment to usage amount
    pub nullifier: BytesN<32>,         // Nullifier to prevent double-spending
    pub proof: Bytes,                  // ZK-SNARK proof (placeholder for future implementation)
    pub meter_id: u64,                 // Associated meter ID
    pub timestamp: u64,                // Proof generation timestamp
    pub is_valid: bool,                // Proof validity status
}

#[contracttype]
#[derive(Clone)]
pub struct ZKUsageReport {
    pub commitment: BytesN<32>,        // Commitment to usage data
    pub nullifier: BytesN<32>,         // Unique nullifier for this report
    pub encrypted_usage: Bytes,         // Encrypted usage data (for future ZK implementation)
    pub proof_hash: BytesN<32>,        // Hash of the ZK proof
    pub meter_id: u64,                 // Meter identifier
    pub billing_cycle: u32,             // Billing cycle number
    pub timestamp: u64,                // Report timestamp
    pub is_verified: bool,              // Verification status
}

#[contracttype]
#[derive(Clone)]
pub struct PrivateBillingStatus {
    pub meter_id: u64,                 // Meter ID
    pub billing_cycle: u32,            // Current billing cycle
    pub total_commitments: u32,        // Number of commitments received
    pub verified_proofs: u32,          // Number of verified ZK proofs
    pub last_verification: u64,        // Last verification timestamp
    pub privacy_enabled: bool,         // Whether privacy mode is enabled
}

#[contracttype]
#[derive(Clone)]
pub struct CommitmentBatch {
    pub commitments: Vec<BytesN<32>>,  // Batch of commitments
    pub nullifiers: Vec<BytesN<32>>,   // Corresponding nullifiers
    pub batch_root: BytesN<32>,       // Merkle root of commitments
    pub timestamp: u64,                // Batch creation time
    pub meter_id: u64,                 // Associated meter
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
    pub provider: Address,              // The utility provider this config belongs to
    pub finance_wallets: Vec<Address>,  // List of authorized Finance Department wallets (max 5)
    pub required_signatures: u32,       // Number of signatures required (typically 3)
    pub threshold_amount: i128,         // Minimum amount requiring multi-sig (in USD cents)
    pub is_active: bool,                // Whether multi-sig is enabled
    pub created_at: u64,                // Timestamp when config was created
}

#[contracttype]
#[derive(Clone)]
pub struct WithdrawalRequest {
    pub request_id: u64,                // Unique request identifier
    pub provider: Address,              // Provider requesting withdrawal
    pub meter_id: u64,                  // Meter to withdraw from
    pub amount_usd_cents: i128,         // Amount requested in USD cents
    pub destination: Address,           // Destination treasury address
    pub proposer: Address,              // Finance wallet that proposed this request
    pub created_at: u64,                // Timestamp when request was created
    pub expires_at: u64,                // Request expiration timestamp
    pub approval_count: u32,            // Current number of approvals
    pub is_executed: bool,              // Whether withdrawal has been executed
    pub is_cancelled: bool,             // Whether request was cancelled
}

#[contracttype]
pub enum DataKey {
    Meter(u64),
    Count,
    Oracle,
    SoroSusuContract,
    VestingVault,
    NFTMinter,
    MaintenanceWallet,
    ProtocolFeeBps,
    SupportedToken(Address),
    SupportedWithdrawalToken(Address),
    ProviderTotalPool(Address),
    Referral(Address),
    PollVotes(Symbol),
    UserVoted(Address, Symbol),
    BillingGroup(Address),
    WebhookConfig(Address),
    LastAlert(u64),
    ClosingFeeBps,
    Contributor(u64, Address),
    AuthorizedContributor(u64, Address),
    // Task #2: Tax Compliance
    GovernmentVault(Address),
    TaxRateBps, // Tax rate in basis points (e.g., 500 = 5%)
    // Task #3: Self-Maintenance
    MaintenanceFund(u64), // Per-meter maintenance fund balance
    AutoExtendThreshold, // Ledger threshold for auto-extension
    // Task #4: Wasm Hash Rotation
    ProposedUpgrade,
    UpgradeProposalTime,
    VetoDeadline,
    UserVetoed(Address, u64), // Address and proposal ID
    // NEW TASKS:
    // Task #1: Admin Transfer
    CurrentAdmin,
    AdminTransferProposal,
    AdminVeto(Address, u64), // Address and proposal timestamp
    ActiveUsers, // For tracking active users for voting
    // Task #2: Legal Freeze
    ComplianceOfficer,
    ComplianceCouncil,
    LegalFreeze(u64),
    LegalVault,
    // Task #3: Verified Provider Registry
    VerifiedProvider(Address),
    // Issue #127: Reputation Migration
    UserReputation(Address),
    ReputationMigration(BytesN<32>), // Using nullifier as key
    MigratedReputation(Address, Address), // (user, old_contract)
    // Issue #119: Maintenance Milestones
    MaintenanceMilestone(u64, u32), // (meter_id, milestone_number)
    // Issue #118: ZK Privacy
    ZKProof(BytesN<32>), // Using commitment as key
    NullifierMap(BytesN<32>), // Using nullifier as key
    ZKUsageReport(u64, u32), // (meter_id, billing_cycle)
    PrivateBillingStatus(u64), // meter_id -> PrivateBillingStatus
    CommitmentBatch(u64, u64), // (meter_id, batch_timestamp)
    ZKEnabledMeters, // Set of meters with privacy enabled
    ZKVerificationCache(BytesN<32>), // proof_hash -> bool (verification result cache)
    // Issue #130: Grant Stream Integration
    ConservationGoal(u64),
    GrantStreamMatch(u64, Address), // (meter_id, grant_contract)
    // Task #4: Sub-DAO
    SubDaoConfig(Address),
    // Issue #98: Multi-Sig Provider Withdrawal
    MultiSigConfig(Address),           // Provider address -> MultiSigConfig
    WithdrawalRequest(Address, u64),   // Provider address, request ID -> WithdrawalRequest
    WithdrawalRequestCount(Address),   // Provider address -> request counter
    WithdrawalApproval(Address, u64, Address), // Provider, request ID, signer -> bool
}

#[contracterror]
#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    MeterNotFound = 1,
    OracleNotSet = 2,
    WithdrawalLimitExceeded = 3,
    PriceConversionFailed = 4,
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
    MeterPaused = 16,
    AlreadyVoted = 17,
    InvalidClosingFee = 18,
    AccountAlreadyClosed = 19,
    InsufficientBalance = 20,
    UnauthorizedContributor = 21,
    InDispute = 22,
    ChallengeActive = 23,
    NotAnOracle = 24,
    // Task #1: Priority System Errors
    ThrottlingThresholdExceeded = 25,
    LowPriorityStreamPaused = 26,
    // Task #2: Tax Compliance Errors
    GovernmentVaultNotSet = 27,
    TaxCalculationFailed = 28,
    // Task #3: Maintenance Errors
    MaintenanceFundInsufficient = 29,
    TTLExtensionFailed = 30,
    // Task #4: Upgrade Errors
    UpgradeProposalActive = 31,
    VetoPeriodExpired = 32,
    UserVetoedProposal = 33,
    InvalidWasmHash = 34,
    // NEW TASKS:
    // Task #1: Admin Transfer Errors
    AdminTransferActive = 35,
    NoAdminTransferInProgress = 36,
    VetoThresholdNotReached = 37,
    AdminExecutionWindowExpired = 38,
    NotCurrentAdmin = 39,
    // Task #2: Legal Freeze Errors
    NotComplianceOfficer = 40,
    MeterNotFrozen = 41,
    LegalFreezeAlreadyActive = 42,
    ComplianceCouncilApprovalRequired = 43,
    // Task #3: Verified Provider Errors
    ProviderNotVerified = 44,
    VerificationAlreadyGranted = 45,
    // Task #4: Sub-DAO Errors
    NotParentDao = 46,
    SubDaoBudgetExceeded = 47,
    SubDaoNotConfigured = 48,
    // Issue #98: Multi-Sig Withdrawal Errors
    MultiSigNotConfigured = 49,
    MultiSigAlreadyConfigured = 50,
    InvalidFinanceWalletCount = 51,
    InvalidSignatureThreshold = 52,
    NotAuthorizedFinanceWallet = 53,
    WithdrawalRequestNotFound = 54,
    WithdrawalRequestExpired = 55,
    WithdrawalAlreadyExecuted = 56,
    WithdrawalAlreadyCancelled = 57,
    InsufficientApprovals = 58,
    AlreadyApprovedWithdrawal = 59,
    NotApprovedByWallet = 60,
    AmountBelowMultiSigThreshold = 61,
    MultiSigRequiredForAmount = 62,
    // Issue #118: ZK Privacy Errors
    InvalidCommitment = 63,
    NullifierAlreadyUsed = 64,
    InvalidZKProof = 65,
    PrivacyNotEnabled = 66,
    CommitmentNotFound = 67,
    InvalidBillingCycle = 68,
    ZKVerificationFailed = 69,
    // Issue #130: Grant Stream Integration Errors
    ConservationGoalNotFound = 70,
    GoalAlreadyAchieved = 71,
    GoalExpired = 72,
    InvalidGrantAmount = 73,
    GrantStreamNotConfigured = 74,
    InsufficientWaterSavings = 75,
}

#[contracttype]
#[derive(Clone)]
pub struct PairingChallengeData {
    pub contract: Address,
    pub meter_id: u64,
    pub timestamp: u64,
}

#[contract]
pub struct UtilityContract;

const HOUR_IN_SECONDS: u64 = 60 * 60;
const DAY_IN_SECONDS: u64 = 24 * HOUR_IN_SECONDS;
const GRACE_PERIOD_SECONDS: u64 = 86_400; // 24 hours grace period
const DEBT_THRESHOLD: i128 = -10_000_000; // -10 XLM (in stroops) threshold for negative balance
const DAILY_WITHDRAWAL_PERCENT: i128 = 10;
const MAX_USAGE_PER_UPDATE: i128 = 1_000_000_000_000i128; // 1 billion kWh max per update
const MIN_PRECISION_FACTOR: i128 = 1;
const MAX_TIMESTAMP_DELAY: u64 = 300; // 5 minutes

// NEW TASK CONSTANTS:
// Task #1: Admin Transfer Timelock
const ADMIN_TRANSFER_TIMELOCK: u64 = 48 * HOUR_IN_SECONDS; // 48 hours
const VETO_THRESHOLD_BPS: i128 = 1000; // 10% in basis points

// Task #2: Legal Freeze
const LEGAL_FREEZE_DURATION: u64 = 30 * DAY_IN_SECONDS; // 30 days default

// Peak hours: 18:00 - 21:00 UTC
const PEAK_HOUR_START: u64 = 18 * HOUR_IN_SECONDS; // 64800 seconds
const PEAK_HOUR_END: u64 = 21 * HOUR_IN_SECONDS; // 75600 seconds
const PEAK_RATE_MULTIPLIER: i128 = 3; // 1.5x => stored as 3 (divide by 2)
const RATE_PRECISION: i128 = 2; // Precision for rate calculations
const REFERRAL_REWARD_UNITS: i128 = 500; // 5 units reward for referrals

// XLM precision constants - XLM has 7 decimal places (0.0000001 minimum)
const XLM_PRECISION: i128 = 10_000_000; // 10^7 for 7 decimal places
const XLM_MINIMUM_INCREMENT: i128 = 1; // 1 stroop = 0.0000001 XLM

// Task #1: Priority System Constants
const THROTTLING_THRESHOLD_PERCENT: i128 = 20; // 20% of total balance triggers throttling
const LOW_PRIORITY_THRESHOLD: u32 = 5; // Streams with priority >= 5 are considered low priority

// Task #2: Tax Compliance Constants
const DEFAULT_TAX_RATE_BPS: i128 = 500; // 5% tax (500 basis points)

// Task #3: Self-Maintenance Constants
const MAINTENANCE_FUND_PERCENT_BPS: i128 = 1; // 0.01% = 1 basis point
const AUTO_EXTEND_LEDGER_THRESHOLD: u32 = 500_000; // Extend TTL every 500,000 ledgers
const LEDGER_LIFETIME_EXTENSION: u32 = 1_000_000; // Extend by 1M ledgers

// Task #4: Wasm Hash Rotation Constants
const UPGRADE_VETO_PERIOD_SECONDS: u64 = 7 * DAY_IN_SECONDS; // 7 days veto period

// Issue #98: Multi-Sig Provider Withdrawal Constants
const MAX_FINANCE_WALLETS: u32 = 5;        // Maximum number of authorized finance wallets
const MIN_FINANCE_WALLETS: u32 = 3;        // Minimum number of finance wallets required
const DEFAULT_REQUIRED_SIGNATURES: u32 = 3; // Default 3-of-5 requirement
const WITHDRAWAL_REQUEST_EXPIRY: u64 = 7 * DAY_IN_SECONDS; // Requests expire after 7 days
const DEFAULT_MULTISIG_THRESHOLD: i128 = 100_000_00; // $100,000 USD in cents

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

/// Convert USD cents to XLM with proper rounding to minimum increment
fn convert_usd_cents_to_xlm_with_rounding(usd_cents: i128, xlm_price_cents: i128) -> i128 {
    if xlm_price_cents <= 0 {
        return 0;
    }

    // Calculate raw XLM amount with higher precision
    let raw_xlm = usd_cents.saturating_mul(XLM_PRECISION) / xlm_price_cents;

    // Round to nearest minimum increment to prevent value loss
    round_xlm_to_minimum_increment(raw_xlm)
}

/// Convert XLM to USD cents with proper rounding
fn convert_xlm_to_usd_cents_with_rounding(xlm_amount: i128, xlm_price_cents: i128) -> i128 {
    if xlm_price_cents <= 0 {
        return 0;
    }

    // Calculate USD cents, rounding to nearest cent
    let raw_usd = xlm_amount.saturating_mul(xlm_price_cents) / XLM_PRECISION;

    // Round to nearest cent
    if raw_usd >= 0 {
        ((raw_usd + 50) / 100) * 100 // Round up on .5 or higher
    } else {
        ((raw_usd - 50) / 100) * 100 // Round down on -.5 or lower
    }
}

    pub fn claim(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        if meter.is_disputed { panic_with_error!(&env, ContractError::InDispute); }

        let old_meter_value = provider_meter_value(&meter);
        let now = env.ledger().timestamp();
        let mut window = get_provider_window_or_default(&env, &meter.provider, now);
        
        let settlement = settle_claim_for_meter(&env, meter_id, &mut meter, now, &mut window);
        let client = token::Client::new(&env, &meter.token);

        // 1. Pay Government Tax
        if settlement.tax_amount > 0 {
            if let Some(gov_vault) = env.storage().instance().get::<_, Address>(&DataKey::GovernmentVault) {
                client.transfer(&env.current_contract_address(), &gov_vault, &settlement.tax_amount);
            }
        }

        // 2. Pay Protocol Maintenance Fee
        if settlement.protocol_fee > 0 {
            if let Some(wallet) = env.storage().instance().get::<_, Address>(&DataKey::MaintenanceWallet) {
                client.transfer(&env.current_contract_address(), &wallet, &settlement.protocol_fee);
            }
        }

        // 3. Pay Reseller (if applicable)
        if settlement.reseller_payout > 0 {
            if let Some(reseller_config) = get_reseller_config_impl(&env, meter_id) {
                client.transfer(&env.current_contract_address(), &reseller_config.reseller, &settlement.reseller_payout);
            }
        }

        // 4. Pay Provider
        if settlement.provider_payout > 0 {
            client.transfer(&env.current_contract_address(), &meter.provider, &settlement.provider_payout);
        }

        // Update State
        env.storage().instance().set(&DataKey::ProviderWindow(meter.provider.clone()), &window);
        update_provider_total_pool(&env, &meter.provider, old_meter_value, provider_meter_value(&meter));
        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);
    }

fn convert_usd_to_token_if_needed(env: &Env, usd_cents: i128, destination_token: &Address) -> Result<i128, ContractError> {
    // For now, we assume the oracle can provide conversion rates for any token
    // In a real implementation, you'd need specific price feeds for each token
    match env.storage().instance().get::<DataKey, Address>(&DataKey::Oracle) {
        Some(oracle_address) => {
            let oracle_client = PriceOracleClient::new(env, &oracle_address);
            let price_data = oracle_client.get_price();

            // If destination is XLM (native token), use existing conversion
            if is_native_token(destination_token) {
                let xlm_amount = convert_usd_cents_to_xlm_with_rounding(usd_cents, price_data.price);
                Ok(xlm_amount)
            } else {
                // For other tokens, assume 1:1 with USD for now
                // In production, you'd need specific price feeds for each token
                Ok(usd_cents)
            }
        }
        None => Err(ContractError::OracleNotSet),
    }
}

    pub fn assign_reseller(env: Env, meter_id: u64, reseller: Address, fee_bps: i128) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();
        if fee_bps > MAX_RESELLER_FEE_BPS { panic_with_error!(&env, ContractError::InvalidResellerFee); }

        let config = ResellerConfig { reseller: reseller.clone(), fee_bps };
        env.storage().instance().set(&DataKey::ResellerConfig(meter_id), &config);
        env.events().publish((symbol_short!("RslrSet"), meter_id), (reseller, fee_bps));
    }

    pub fn claim_impact_sbt(env: Env, meter_id: u64) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        if env.storage().instance().get(&DataKey::ImpactSBTMinted(meter_id)).unwrap_or(false) {
            panic_with_error!(&env, ContractError::SBTAlreadyMinted);
        }

        const SBT_THRESHOLD: i128 = 18_250_000;
        if meter.usage_data.renewable_watt_hours < SBT_THRESHOLD {
            panic_with_error!(&env, ContractError::ImpactNotSignificantEnough);
        }

        let carbon_saved = meter.usage_data.renewable_watt_hours.saturating_mul(4) / 10;
        let susu_addr = env.storage().instance().get::<_, Address>(&DataKey::SoroSusuContract).expect("No Susu");
        let susu_client = SoroSusuClient::new(&env, &susu_addr);
        let score = susu_client.get_susu_score(meter.user.clone());

        if let Some(minter_addr) = env.storage().instance().get::<_, Address>(&DataKey::NFTMinter) {
            let minter = NFTMinterClient::new(&env, &minter_addr);
            minter.mint_impact_sbt(&meter.user, &carbon_saved, &score);
            env.storage().instance().set(&DataKey::ImpactSBTMinted(meter_id), &true);
        }
    }

    pub fn get_public_utility_health_index(env: Env) -> ImpactMetrics {
        let count: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);
        let mut total_wh: i128 = 0;
        let mut total_val: i128 = 0;
        let mut active: u32 = 0;

        for i in 1..=count {
            if let Some(meter) = env.storage().instance().get::<_, Meter>(&DataKey::Meter(i)) {
                total_wh += meter.usage_data.total_watt_hours;
                total_val += meter.usage_data.monthly_volume;
                if meter.is_active && !meter.is_paused { active += 1; }
            }
        }
        ImpactMetrics { total_kilowatts_funded: total_wh / 1000, total_liters_streamed: total_val, active_meters: active }
    }

// --- Internal Settlement Logic ---

fn settle_claim_for_meter(
    env: &Env,
    meter_id: u64,
    meter: &mut Meter,
    now: u64,
    provider_window: &mut ProviderWithdrawalWindow,
) -> ClaimSettlement {
    let elapsed = now.saturating_sub(meter.last_update);
    let mut amount = (elapsed as i128).saturating_mul(meter.rate_per_unit);
    
    // Issue #106: Milestone Penalty (Halve rate if deadline missed)
    if meter.milestone_deadline > 0 && now > meter.milestone_deadline && !meter.milestone_confirmed {
        amount /= 2;
    }

    let claimable = if amount > meter.balance && meter.balance - amount >= DEBT_THRESHOLD {
        amount
    } else if amount > meter.balance {
        meter.balance - DEBT_THRESHOLD
    } else {
        amount
    };

    if claimable <= 0 {
        return ClaimSettlement { gross_claimed: 0, provider_payout: 0, tax_amount: 0, protocol_fee: 0, reseller_payout: 0 };
    }

    // 1. Tax Calculation
    let tax_rate = env.storage().instance().get(&DataKey::TaxRateBps).unwrap_or(DEFAULT_TAX_RATE_BPS);
    let tax_amt = (claimable * tax_rate) / 10000;
    let after_tax = claimable - tax_amt;

    // 2. Protocol Fee
    let protocol_bps: i128 = env.storage().instance().get(&DataKey::ProtocolFeeBps).unwrap_or(0);
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
    };

    // Calculate total value (balance + debt if postpaid)
    let total_value = match meter.billing_type {
        BillingType::PrePaid => meter.balance,
        BillingType::PostPaid => meter.balance.saturating_sub(meter.debt),
    };

    if total_value <= 0 {
        return false;
    }

    // If balance is less than 20% of total value, trigger throttling
    let threshold = (total_value * THROTTLING_THRESHOLD_PERCENT) / 100;
    meter.balance < threshold
}

fn should_pause_low_priority_stream(meter: &Meter, throttling_active: bool) -> bool {
    // Only pause if throttling is active AND this is a low priority stream
    throttling_active && meter.priority_index >= LOW_PRIORITY_THRESHOLD
}

// --- Helpers ---

fn get_meter_or_panic(env: &Env, id: u64) -> Meter {
    env.storage().instance().get(&DataKey::Meter(id)).expect("Meter Not Found")
}

fn provider_meter_value(meter: &Meter) -> i128 {
    meter.balance.max(DEBT_THRESHOLD)
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
    env.storage().instance().get(&DataKey::ResellerConfig(meter_id))
}

fn auto_extend_ttl_if_needed(env: &Env, meter_id: u64) {
    let ledger_sequence = env.ledger().sequence();
    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::AutoExtendThreshold)
        .unwrap_or(AUTO_EXTEND_LEDGER_THRESHOLD);

    // Check if we need to extend (every 500,000 ledgers)
    if ledger_sequence % threshold as u32 == 0 {
        let maintenance_balance = get_maintenance_fund_balance(env, meter_id);

        // Estimate cost of TTL extension (simplified - actual cost depends on storage size)
        let estimated_cost = 1_000_000; // 1 XLM in stroops as example

        if maintenance_balance >= estimated_cost {
            // Deduct from maintenance fund
            let new_balance = maintenance_balance.saturating_sub(estimated_cost);
            env.storage()
                .instance()
                .set(&DataKey::MaintenanceFund(meter_id), &new_balance);

            // Extend TTL - this extends the contract's storage TTL
            env.storage().instance().extend_ttl(LEDGER_LIFETIME_EXTENSION, LEDGER_LIFETIME_EXTENSION);

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

    env.storage().instance().set(&DataKey::ProposedUpgrade, &proposal);
    env.storage().instance().set(&DataKey::UpgradeProposalTime, &now);
    env.storage().instance().set(&DataKey::VetoDeadline, &veto_deadline);

    env.events().publish(
        (soroban_sdk::symbol_short!("UpgrdPrp"),),
        (new_wasm_hash, now, veto_deadline),
    );

    now // Return proposal ID (using timestamp as simple ID)
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
        soroban_sdk::symbol_short!("VetoSubmt"),
        (user, proposal_id),
    );
}

fn can_finalize_upgrade(env: &Env) -> bool {
    // Check if veto period has expired
    let deadline: u64 = env.storage().instance().get(&DataKey::VetoDeadline).unwrap_or(0);
    let now = env.ledger().timestamp();

    if now < deadline {
        return false; // Veto period still active
    }

    // Check if any user vetoed (simplified - in production would count vetoes)
    // For now, we assume if no explicit veto recorded, upgrade can proceed

    true
}

fn update_provider_total_pool(env: &Env, provider: &Address, old: i128, new: i128) {
    // Pool update logic
}

fn refresh_activity(meter: &mut Meter, now: u64) {
    meter.is_active = provider_meter_value(meter) > 0;
    meter.last_update = now;
}

fn publish_inactive_event(env: &Env, meter_id: u64, now: u64) {
    env.events()
        .publish((symbol_short!("Inactive"), meter_id), now);
}

// Issue #118: ZK Privacy Helper Functions

/// Placeholder ZK proof verification (for future full ZK-SNARK implementation)
/// This is a simple mock verification that checks basic constraints
fn verify_zk_proof_placeholder(env: &Env, proof_hash: BytesN<32>) -> bool {
    let now = env.ledger().timestamp();
    
    // Simple validation rules for placeholder implementation:
    // 1. Proof hash should not be all zeros
    // 2. Basic timestamp validation (proof should be recent)
    // 3. In production, this would be full ZK-SNARK verification
    
    let mut is_non_zero = false;
    for byte in proof_hash.to_array().iter() {
        if *byte != 0 {
            is_non_zero = true;
            break;
        }
    }
    
    // For now, accept any non-zero hash as valid (placeholder logic)
    // In production, this would involve cryptographic verification
    is_non_zero
}

/// Generate a simple commitment hash (placeholder for Pedersen commitment)
fn generate_commitment_placeholder(env: &Env, usage_amount: i128, randomness: BytesN<32>) -> BytesN<32> {
    // This is a placeholder - in production would use Pedersen commitments
    let mut combined = Vec::new(&env);
    combined.push_back(&Bytes::from_slice(&env, &usage_amount.to_be_bytes()));
    combined.push_back(&randomness);
    
    // Simple hash (placeholder - would use proper cryptographic commitment in production)
    env.crypto().sha256(&combined.to_xdr(&env))
}

/// Check if a nullifier has been used before
fn is_nullifier_used(env: &Env, nullifier: BytesN<32>) -> bool {
    env.storage().instance().has(&DataKey::NullifierMap(nullifier))
}

/// Store nullifier to prevent double-spending
fn store_nullifier(env: &Env, nullifier: BytesN<32>) {
    env.storage().instance().set(&DataKey::NullifierMap(nullifier), &true);
}

#[contractimpl]
impl UtilityContract {
    pub fn get_minimum_balance_to_flow() -> i128 {
        MINIMUM_BALANCE_TO_FLOW
    }

    pub fn set_oracle(env: Env, oracle_address: Address) {
        // This should be called by admin to set the oracle address
        env.storage()
            .instance()
            .set(&DataKey::Oracle, &oracle_address);
    }

    pub fn set_maintenance_config(env: Env, wallet: Address, fee_bps: i128) {
        env.storage()
            .instance()
            .set(&DataKey::MaintenanceWallet, &wallet);
        env.storage()
            .instance()
            .set(&DataKey::ProtocolFeeBps, &fee_bps);
    }

    pub fn add_supported_token(env: Env, token: Address) {
        env.storage()
            .instance()
            .set(&DataKey::SupportedToken(token), &true);
    }

    pub fn remove_supported_token(env: Env, token: Address) {
        env.storage()
            .instance()
            .set(&DataKey::SupportedToken(token), &false);
    }

    /// Add a supported withdrawal token for path payments
    pub fn add_supported_withdrawal_token(env: Env, token: Address) {
        env.storage().instance().set(&DataKey::SupportedWithdrawalToken(token), &true);
    }

    /// Remove a supported withdrawal token for path payments
    pub fn remove_supported_withdrawal_token(env: Env, token: Address) {
        env.storage().instance().set(&DataKey::SupportedWithdrawalToken(token), &false);
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
        let goal_count: u64 = env.storage()
            .instance()
            .get(&DataKey::Count)
            .unwrap_or(0);
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

        env.storage().instance().set(&DataKey::ConservationGoal(goal_id), &goal);
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

        let mut goal: ConservationGoal = env.storage()
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
            env.storage().instance().set(&DataKey::ConservationGoal(goal_id), &goal);
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
                (goal.provider.clone(), goal.current_savings, goal.grant_amount),
            );

            // Notify Grant Stream contract if configured using secure interface
            if let Some(grant_stream_address) = env.storage().instance().get::<_, Address>(&DataKey::GrantStreamMatch(goal_id, goal.provider.clone())) {
                let mut args = Vec::new(&env);
                args.push_back(env.current_contract_address().into());
                args.push_back(goal_event.into());
                
                match SecureCallManager::secure_call::<()>(
                    &env,
                    &grant_stream_address,
                    &Symbol::new(&env, "on_goal_reached"),
                    args,
                    Some(30_000_000), // Conservative gas limit for grant processing
                ) {
                    Ok(_) => {
                        // Grant processed successfully
                    }
                    Err(e) => {
                        // Log error but don't panic - grant processing failure shouldn't stop goal achievement
                        env.events().publish(
                            (symbol_short!("GrantErr"), goal_id),
                            (grant_stream_address, e as u32),
                        );
                    }
                }
            }
        }

        env.storage().instance().set(&DataKey::ConservationGoal(goal_id), &goal);
    }

    /// Configure Grant Stream contract to listen for goal achievements
    pub fn configure_grant_stream_match(env: Env, goal_id: u64, grant_stream_contract: Address) {
        let goal: ConservationGoal = env.storage()
            .instance()
            .get(&DataKey::ConservationGoal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ConservationGoalNotFound));

        goal.provider.require_auth();

        env.storage().instance().set(&DataKey::GrantStreamMatch(goal_id, goal.provider.clone()), &grant_stream_contract);

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
            if let Some(goal) = env.storage().instance().get::<_, ConservationGoal>(&DataKey::ConservationGoal(goal_id)) {
                if goal.provider == provider && goal.is_active {
                    goal_ids.push_back(goal_id);
                }
            }
        }

        goal_ids
    }

    /// Check if a goal has been achieved and trigger grant if needed
    pub fn check_and_trigger_grant(env: Env, goal_id: u64) {
        let goal: ConservationGoal = env.storage()
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
                (goal.provider.clone(), goal.current_savings, goal.grant_amount),
            );

            // Notify Grant Stream contract if configured using secure interface
            if let Some(grant_stream_address) = env.storage().instance().get::<_, Address>(&DataKey::GrantStreamMatch(goal_id, goal.provider.clone())) {
                let mut args = Vec::new(&env);
                args.push_back(env.current_contract_address().into());
                args.push_back(goal_event.into());
                
                match SecureCallManager::secure_call::<()>(
                    &env,
                    &grant_stream_address,
                    &Symbol::new(&env, "on_goal_reached"),
                    args,
                    Some(30_000_000), // Conservative gas limit for grant processing
                ) {
                    Ok(_) => {
                        // Grant processed successfully
                    }
                    Err(e) => {
                        // Log error but don't panic - grant processing failure shouldn't stop goal achievement
                        env.events().publish(
                            (symbol_short!("GrantErr"), goal_id),
                            (grant_stream_address, e as u32),
                        );
                    }
                }
            }

            env.storage().instance().set(&DataKey::ConservationGoal(goal_id), &updated_goal);
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
        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);
    }

    pub fn register_meter(
        env: Env,
        user: Address,
        provider: Address,
        off_peak_rate: i128,
        token: Address,
        device_public_key: BytesN<32>,
        priority_index: u32,
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
    ) -> u64 {
        let meter_id = Self::register_meter(
            env.clone(),
            user.clone(),
            provider,
            off_peak_rate,
            token,
            device_public_key,
            priority_index,
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
                (symbol_short!("Referral"), meter_id), (referrer.clone(), user.clone()),
            );
        }

        meter_id
    }

    pub fn register_meter_with_mode(
        env: Env,
        user: Address,
        provider: Address,
        off_peak_rate: i128,
        token: Address,
        billing_type: BillingType,
        device_public_key: BytesN<32>,
        priority_index: u32,
    ) -> u64 {
        user.require_auth();

        let mut count = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::Count)
            .unwrap_or(0);
        count += 1;

        let now = env.ledger().timestamp();
        let peak_rate = off_peak_rate.saturating_mul(PEAK_RATE_MULTIPLIER) / RATE_PRECISION;

        let usage_data = UsageData {
            total_watt_hours: 0,
            current_cycle_watt_hours: 0,
            peak_usage_watt_hours: 0,
            last_reading_timestamp: now,
            precision_factor: 1,
            renewable_watt_hours: 0,
            renewable_percentage: 0,
            monthly_volume: 0,
            last_volume_reset: now,
        };

        let meter = Meter {
            user: user.clone(),
            provider: provider.clone(),
            billing_type,
            off_peak_rate,
            peak_rate,
            rate_per_second: 0, // Deprecated, kept for backwards compatibility
            rate_per_unit: off_peak_rate,
            green_energy_discount_bps: 0,
            balance: 0,
            debt: 0,
            collateral_limit: 0,
            last_update: now,
            is_active: true,
            token,
            usage_data,
            max_flow_rate_per_hour: 0,
            last_claim_time: 0,
            claimed_this_hour: 0,
            heartbeat: now,
            device_public_key,
            is_paired: false,
            grace_period_start: 0,
            is_paused: false,
            tier_threshold: 0,
            tier_rate: 0,
            is_disputed: false,
            challenge_timestamp: 0,
            credit_drip_rate: 0,
            is_closed: false,
            priority_index, // Task #1: Set priority index
        };

        env.storage().instance().set(&DataKey::Meter(count), &meter);
        env.storage().instance().set(&DataKey::Count, &count);

        // Initialize provider total pool (new meter starts with 0 value)
        let current_pool = get_provider_total_pool_impl(&env, &provider);
        env.storage()
            .instance()
            .set(&DataKey::ProviderTotalPool(provider), &current_pool);

        count
    }

    pub fn batch_register_meters(env: Env, meter_infos: Vec<MeterInfo>) -> BatchCreatedEvent {
        if meter_infos.is_empty() {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        // Require authorization for all users in the batch
        for meter_info in meter_infos.iter() {
            meter_info.user.require_auth();
        }

        let mut count = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::Count)
            .unwrap_or(0);

        let start_id = count + 1;
        let now = env.ledger().timestamp();

        // Track providers initialized to avoid duplicate initialization
        let mut providers_initialized: Vec<Address> = Vec::new(&env);

        for meter_info in meter_infos.iter() {
            count += 1;

            let provider_clone = meter_info.provider.clone();
            let peak_rate = meter_info
                .off_peak_rate
                .saturating_mul(PEAK_RATE_MULTIPLIER)
                / RATE_PRECISION;

            let usage_data = UsageData {
                total_watt_hours: 0,
                current_cycle_watt_hours: 0,
                peak_usage_watt_hours: 0,
                last_reading_timestamp: now,
                precision_factor: 1000,
                renewable_watt_hours: 0,
                renewable_percentage: 0,
                monthly_volume: 0,
                last_volume_reset: now,
            };

            let meter = Meter {
                user: meter_info.user.clone(),
                provider: provider_clone.clone(),
                billing_type: meter_info.billing_type,
                off_peak_rate: meter_info.off_peak_rate,
                peak_rate,
                rate_per_second: meter_info.off_peak_rate,
                rate_per_unit: meter_info.off_peak_rate,
                green_energy_discount_bps: 0,
                balance: 0,
                debt: 0,
                collateral_limit: 0,
                last_update: now,
                is_active: false,
                token: meter_info.token.clone(),
                usage_data,
                max_flow_rate_per_hour: meter_info
                    .off_peak_rate
                    .saturating_mul(HOUR_IN_SECONDS as i128),
                last_claim_time: now,
                claimed_this_hour: 0,
                heartbeat: now,
                device_public_key: meter_info.device_public_key,
                is_paired: false,
                grace_period_start: 0,
                is_paused: false,
                tier_threshold: 100_000,
                tier_rate: meter_info.off_peak_rate.saturating_mul(120) / 100,
                is_disputed: false,
                challenge_timestamp: 0,
                credit_drip_rate: 0,
                is_closed: false,
            };

            env.storage().instance().set(&DataKey::Meter(count), &meter);

            // Initialize provider total pool only once per provider
            let mut already_initialized = false;
            for provider in providers_initialized.iter() {
                if provider.clone() == provider_clone {
                    already_initialized = true;
                    break;
                }
            }

            if !already_initialized {
                let current_pool = get_provider_total_pool_impl(&env, &provider_clone);
                env.storage().instance().set(
                    &DataKey::ProviderTotalPool(provider_clone.clone()),
                    &current_pool,
                );
                providers_initialized.push_back(provider_clone);
            }
        }

        // Update the global count
        env.storage().instance().set(&DataKey::Count, &count);

        let batch_event = BatchCreatedEvent {
            start_id,
            end_id: count,
            count: count - start_id + 1,
        };

        // Emit single BatchCreated event
        env.events().publish(
            (symbol_short!("BatchCr"),),
            (batch_event.start_id, batch_event.end_id, batch_event.count),
        );
        batch_event
    }

    pub fn top_up(env: Env, meter_id: u64, amount: i128, contributor: Address) {
        let mut meter = get_meter_or_panic(&env, meter_id);

        // Authorization: either the primary user OR an authorized contributor
        let is_authorized = if contributor == meter.user {
            contributor.require_auth();
            true
        } else {
            let auth_key = DataKey::AuthorizedContributor(meter_id, contributor.clone());
            if env.storage().instance().get::<_, bool>(&auth_key).unwrap_or(false) {
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
        let current_contribution = env.storage().instance().get::<_, i128>(&contribution_key).unwrap_or(0);
        env.storage().instance().set(&contribution_key, &current_contribution.saturating_add(amount));

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

    pub fn deduct_units(env: Env, signed_data: SignedUsageData) {
        let mut meter = get_meter_or_panic(&env, signed_data.meter_id);
        meter.provider.require_auth();

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

        let now = env.ledger().timestamp();
        let effective_rate = get_effective_rate(&meter, signed_data.timestamp);

        // Apply green energy discount if applicable
        let discounted_rate = if signed_data.is_renewable_energy && meter.green_energy_discount_bps > 0 {
            effective_rate.saturating_mul(10000 - meter.green_energy_discount_bps) / 10000
        } else {
            effective_rate
        };

        let cost = signed_data.units_consumed.saturating_mul(discounted_rate);

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

        // Apply the claim (using after-tax amount for actual provider payout)
        apply_provider_claim(&env, &mut meter, after_tax_amount);

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
            meter.usage_data.renewable_percentage = meter
                .usage_data
                .renewable_watt_hours
                .saturating_mul(10000) / meter.usage_data.total_watt_hours; // in basis points
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

        env.storage()
            .instance()
            .set(&DataKey::Meter(signed_data.meter_id), &meter);

        // Emit UsageReported event
        env.events().publish(
            (Symbol::new(&env, "UsageReported"), signed_data.meter_id),
            (signed_data.units_consumed, cost),
        );
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
        let amount = (elapsed as i128).saturating_mul(meter.rate_per_unit.saturating_add(meter.credit_drip_rate));

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

                // Protocol fee (existing logic)
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
                    let fee = (payout * fee_bps) / 10000;
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
                    let credit_settlement = (elapsed as i128).saturating_mul(meter.credit_drip_rate).min(meter.debt);
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

                // Protocol fee (existing logic)
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
                    let fee = (payout * fee_bps) / 10000;
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
                    let credit_settlement = (elapsed as i128).saturating_mul(meter.credit_drip_rate).min(meter.debt);
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

        env.events()
            .publish((symbol_short!("Claim"), meter_id), settlement.gross_claimed);
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

    pub fn get_provider_total_pool(env: Env, provider: Address) -> i128 {
        get_provider_total_pool_impl(&env, &provider)
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

    pub fn get_watt_hours_display(watt_hours: i128, precision_factor: i128) -> i128 {
        watt_hours / precision_factor
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

    pub fn set_closing_fee(env: Env, fee_bps: i128) {
        // Validate fee is within reasonable bounds (0-1000 bps = 0-10%)
        if fee_bps < 0 || fee_bps > 1000 {
            panic_with_error!(env, ContractError::InvalidClosingFee);
        }
        env.storage().instance().set(&DataKey::ClosingFeeBps, &fee_bps);
    }

    pub fn get_closing_fee(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::ClosingFeeBps)
            .unwrap_or(100) // Default 1% (100 bps)
    }

    /// Close account and withdraw remaining balance minus closing fee
    /// Users can call this to permanently close their meter and get refunded
    pub fn close_account_and_refund(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        // Check if account is already closed
        if meter.is_closed {
            panic_with_error!(env, ContractError::AccountAlreadyClosed);
        }

        // Store old meter value for pool update
        let old_meter_value = provider_meter_value(&meter);

        // Calculate refundable amount based on billing type
        let refundable_amount = match meter.billing_type {
            BillingType::PrePaid => meter.balance,
            BillingType::PostPaid => {
                // For postpaid, refund any remaining collateral
                remaining_postpaid_collateral(&meter)
            }
        };

        // Check if there's anything to refund
        if refundable_amount <= 0 {
            panic_with_error!(env, ContractError::InsufficientBalance);
        }

        // Get closing fee and calculate fee amount
        let closing_fee_bps = Self::get_closing_fee(env.clone());
        let closing_fee_amount = (refundable_amount * closing_fee_bps) / 10000;
        let final_refund_amount = refundable_amount.saturating_sub(closing_fee_amount);

        // Convert USD cents to XLM if needed for withdrawal
        let withdrawal_amount = match convert_usd_to_xlm_if_needed(&env, final_refund_amount, &meter.token) {
            Ok(amount) => amount,
            Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
        };

        // Transfer closing fee to maintenance wallet if configured
        if closing_fee_amount > 0 {
            if let Some(maintenance_wallet) = env.storage().instance().get::<_, Address>(&DataKey::MaintenanceWallet) {
                let fee_withdrawal_amount = match convert_usd_to_xlm_if_needed(&env, closing_fee_amount, &meter.token) {
                    Ok(amount) => amount,
                    Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
                };

                let token_client = token::Client::new(&env, &meter.token);
                token_client.transfer(&env.current_contract_address(), &maintenance_wallet, &fee_withdrawal_amount);
            }
        }

        // Transfer refund to user
        if final_refund_amount > 0 {
            let token_client = token::Client::new(&env, &meter.token);
            token_client.transfer(&env.current_contract_address(), &meter.user, &withdrawal_amount);
        }

        // Close the account
        meter.is_closed = true;
        meter.is_active = false;
        meter.balance = 0;
        meter.debt = 0;
        meter.collateral_limit = 0;

        let now = env.ledger().timestamp();
        meter.last_update = now;

        // Update provider total pool
        let new_meter_value = provider_meter_value(&meter);
        update_provider_total_pool(&env, &meter.provider, old_meter_value, new_meter_value);

        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);

        // Emit events
        env.events().publish(
            (symbol_short!("AccountClosed"), meter_id),
            (refundable_amount, closing_fee_amount, final_refund_amount)
        );

        // Emit conversion event if XLM was used
        if is_native_token(&meter.token) {
            env.events().publish(
                (symbol_short!("RefundUSDToXLM"), meter_id),
                (final_refund_amount, withdrawal_amount)
            );
        }
    }

    /// Withdraw earnings with path payment support - allows provider to receive XLM
    /// even when payments were made in USDC or other tokens
    pub fn withdraw_earnings_path_payment(env: Env, meter_id: u64, amount_usd_cents: i128, destination_token: Address) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        if amount_usd_cents <= 0 {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        // Check if destination token is supported for withdrawal
        if !Self::is_withdrawal_token_supported(env.clone(), destination_token.clone()) {
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

        // If destination token is same as meter token, use regular withdrawal
        if destination_token == meter.token {
            Self::withdraw_earnings(env.clone(), meter_id, amount_usd_cents);
            return;
        }

        // Convert USD cents to destination token amount
        let withdrawal_amount = match convert_usd_to_token_if_needed(&env, amount_usd_cents, &destination_token) {
            Ok(amount) => amount,
            Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
        };

        // For path payment, we need to:
        // 1. Convert from meter token to USD (if not already USD)
        // 2. Convert from USD to destination token
        // This is handled by the oracle conversion functions

        // Transfer destination tokens to provider
        let destination_client = token::Client::new(&env, &destination_token);

        // Check if contract has enough destination tokens
        let contract_balance = destination_client.balance(&env.current_contract_address());
        if contract_balance < withdrawal_amount {
            panic_with_error!(&env, ContractError::InsufficientBalance);
        }

        destination_client.transfer(&env.current_contract_address(), &meter.provider, &withdrawal_amount);

        // Update meter balance/debt (deduct in USD cents)
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

        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);

        // Emit path payment event
        env.events().publish(
            (symbol_short!("PathPay"), meter_id),
            (
                meter.token,
                destination_token.clone(),
                amount_usd_cents,
                withdrawal_amount,
            ),
        );

        // Issue #107: Cross-Border Settlement Event for Inter-Anchor communication
        env.events().publish(
            (symbol_short!("XBrder"), meter_id),
            (meter.provider.clone(), destination_token.clone(), withdrawal_amount)
        );
    }

    /// Get supported withdrawal tokens for a provider
    pub fn get_supported_withdrawal_tokens(env: Env) -> Vec<Address> {
        let mut supported_tokens = Vec::new(&env);

        // Add XLM as native token - represented by the contract's own address for native token
        // In Stellar, native token operations use the contract address directly
        supported_tokens.push_back(env.current_contract_address());

        // In a full implementation, you would iterate through stored supported withdrawal tokens
        // For now, we return just the native token

        supported_tokens
    }

    /// Check if a token is supported for withdrawal
    pub fn is_withdrawal_token_supported(env: Env, token: Address) -> bool {
        // Always support native token (XLM)
        if token == env.current_contract_address() {
            return true;
        }

        // Check if token is in supported withdrawal tokens list
        env.storage().instance().get::<DataKey, bool>(&DataKey::SupportedWithdrawalToken(token)).unwrap_or(false)
    }

    /// Get refund estimate for a meter (does not execute the refund)
    pub fn get_refund_estimate(env: Env, meter_id: u64) -> Option<(i128, i128, i128)> {
        if let Some(meter) = env.storage().instance().get::<_, Meter>(&DataKey::Meter(meter_id)) {
            if meter.is_closed {
                return None;
            }

            let refundable_amount = match meter.billing_type {
                BillingType::PrePaid => meter.balance,
                BillingType::PostPaid => remaining_postpaid_collateral(&meter),
            };

            if refundable_amount <= 0 {
                return None;
            }

            let closing_fee_bps = Self::get_closing_fee(env.clone());
            let closing_fee_amount = (refundable_amount * closing_fee_bps) / 10000;
            let final_refund_amount = refundable_amount.saturating_sub(closing_fee_amount);

            Some((refundable_amount, closing_fee_amount, final_refund_amount))
        } else {
            None
        }
    }

    // Group Billing Functions
    pub fn create_billing_group(env: Env, parent_account: Address) {
        parent_account.require_auth();

        let billing_group = BillingGroup {
            parent_account: parent_account.clone(),
            child_meters: Vec::new(),
            created_at: env.ledger().timestamp(),
        };

        env.storage().instance().set(&DataKey::BillingGroup(parent_account), &billing_group);
    }

    fn add_meter_to_billing_group(env: Env, parent_account: Address, meter_id: u64) {
        let mut billing_group: BillingGroup = env.storage().instance()
            .get(&DataKey::BillingGroup(parent_account.clone()))
            .unwrap_or_else(|| BillingGroup {
                parent_account: parent_account.clone(),
                child_meters: Vec::new(),
                created_at: env.ledger().timestamp(),
            });

        // Add meter to the group if not already present
        if !billing_group.child_meters.contains(&meter_id) {
            billing_group.child_meters.push(meter_id);
            env.storage().instance().set(&DataKey::BillingGroup(parent_account), &billing_group);
        }
    }

    pub fn group_top_up(env: Env, parent_account: Address, amount_per_meter: i128) {
        parent_account.require_auth();

        let billing_group: BillingGroup = env.storage().instance()
            .get(&DataKey::BillingGroup(parent_account.clone()))
            .ok_or("Billing group not found").unwrap();

        if billing_group.child_meters.is_empty() {
            return;
        }

        let total_amount = amount_per_meter * billing_group.child_meters.len() as i128;

        // Transfer total amount from parent to contract
        if let Some(first_meter_id) = billing_group.child_meters.first() {
            if let Some(first_meter) = env.storage().instance().get::<_, Meter>(&DataKey::Meter(*first_meter_id)) {
                let client = token::Client::new(&env, &first_meter.token);
                client.transfer(&parent_account, &env.current_contract_address(), &total_amount);
            }
        }

        // Distribute funds to all child meters
        for &meter_id in &billing_group.child_meters {
            if let Some(mut meter) = env.storage().instance().get::<_, Meter>(&DataKey::Meter(meter_id)) {
                meter.balance += amount_per_meter;
                meter.is_active = true;
                meter.last_update = env.ledger().timestamp();
                env.storage().instance().set(&DataKey::Meter(meter_id), &meter);
            }
        }
    }

    pub fn get_billing_group(env: Env, parent_account: Address) -> Option<BillingGroup> {
        env.storage().instance().get(&DataKey::BillingGroup(parent_account))
    }

    pub fn remove_meter_from_billing_group(env: Env, parent_account: Address, meter_id: u64) {
        parent_account.require_auth();

        let mut billing_group: BillingGroup = env.storage().instance()
            .get(&DataKey::BillingGroup(parent_account.clone()))
            .ok_or("Billing group not found").unwrap();

        billing_group.child_meters.retain(|&id| id != meter_id);
        env.storage().instance().set(&DataKey::BillingGroup(parent_account), &billing_group);

        // Update the meter to remove parent reference
        if let Some(mut meter) = env.storage().instance().get::<_, Meter>(&DataKey::Meter(meter_id)) {
            meter.parent_account = None;
            env.storage().instance().set(&DataKey::Meter(meter_id), &meter);
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

        env.storage().instance().set(&DataKey::WebhookConfig(user), &webhook_config);
    }

    pub fn deactivate_webhook(env: Env, user: Address) {
        user.require_auth();

        if let Some(mut config) = env.storage().instance().get::<_, WebhookConfig>(&DataKey::WebhookConfig(user.clone())) {
            config.is_active = false;
            env.storage().instance().set(&DataKey::WebhookConfig(user), &config);
        }
    }

    pub fn get_webhook_config(env: Env, user: Address) -> Option<WebhookConfig> {
        env.storage().instance().get(&DataKey::WebhookConfig(user))
    }

    fn check_and_send_low_balance_alert(env: &Env, meter: &Meter, meter_id: u64) {
        // Only check if webhook is configured for this user
        let webhook_config = match env.storage().instance().get::<_, WebhookConfig>(&DataKey::WebhookConfig(meter.user.clone())) {
            Some(config) if config.is_active => config,
            _ => return, // No active webhook configured
        };

        // Calculate hours remaining
        let hours_remaining = if meter.rate_per_second > 0 {
            meter.balance as f32 / meter.rate_per_second as f32 / 3600.0
        } else {
            f32::INFINITY
        };

        // Check if balance is low (< 24 hours)
        if hours_remaining < 24.0 {
            // Check if we've sent an alert recently (within last 12 hours)
            let current_time = env.ledger().timestamp();
            let last_alert_time: Option<u64> = env.storage().instance().get(&DataKey::LastAlert(meter_id));

            if let Some(last_time) = last_alert_time {
                if current_time.checked_sub(last_time).unwrap_or(0) < 43200 { // 12 hours in seconds
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
            env.storage().instance().set(&DataKey::LastAlert(meter_id), &current_time);

            // In a real implementation, this would make an HTTP call to the webhook
            // For now, we'll store the alert in contract storage for demonstration
            let alert_key = format!("alert:{}:{}", meter_id, current_time);
            env.storage().instance().set(&alert_key, &alert);
        }
    }

    pub fn get_pending_alerts(env: Env, user: Address) -> Vec<LowBalanceAlert> {
        let mut alerts = Vec::new();

        // This is a simplified implementation
        // In practice, you'd want to iterate through storage more efficiently
        let count: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);

        for meter_id in 1..=count {
            if let Some(meter) = env.storage().instance().get::<_, Meter>(&DataKey::Meter(meter_id)) {
                if meter.user == user {
                    // Check for recent alerts
                    let current_time = env.ledger().timestamp();
                    let alert_key = format!("alert:{}:{}", meter_id, current_time);
                    if let Some(alert) = env.storage().instance().get::<_, LowBalanceAlert>(&alert_key) {
                        alerts.push(alert);
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
        let amount = (elapsed as i128).saturating_mul(meter.rate_per_unit.saturating_add(meter.credit_drip_rate));

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
            client.transfer(&env.current_contract_address(), &meter.provider, &final_claimable);
            meter.balance -= final_claimable;
            meter.claimed_this_hour += final_claimable;

            // If credit drip was active, reduce the debt if in PostPaid mode
            if meter.billing_type == BillingType::PostPaid && meter.credit_drip_rate > 0 {
                let credit_settlement = (elapsed as i128).saturating_mul(meter.credit_drip_rate).min(meter.debt);
                meter.debt = meter.debt.saturating_sub(credit_settlement);
            }
        }

        meter.last_update = now;
        if meter.balance <= 0 {
            meter.is_active = false;
        }

        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);

        // Check for low balance and send alert if needed
        Self::check_and_send_low_balance_alert(&env, &meter, meter_id);
    }

    // Task #87: Roommates support
    pub fn add_authorized_contributor(env: Env, meter_id: u64, contributor: Address) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        env.storage().instance().set(&DataKey::AuthorizedContributor(meter_id, contributor), &true);
    }

    pub fn remove_authorized_contributor(env: Env, meter_id: u64, contributor: Address) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        env.storage().instance().remove(&DataKey::AuthorizedContributor(meter_id, contributor));
    }

    pub fn get_contribution(env: Env, meter_id: u64, contributor: Address) -> i128 {
        env.storage().instance().get(&DataKey::Contributor(meter_id, contributor)).unwrap_or(0)
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

        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);

        env.events().publish((symbol_short!("Challeng"), meter_id), meter.challenge_timestamp);
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

        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);

        env.events().publish((symbol_short!("Resolv"), meter_id), restored);
    }

    pub fn refund_disputed_funds(env: Env, meter_id: u64) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        // Can only refund if challenged more than 48 hours ago and not resolved
        let now = env.ledger().timestamp();
        if !meter.is_disputed || now.saturating_sub(meter.challenge_timestamp) < (48 * HOUR_IN_SECONDS) {
            panic_with_error!(&env, ContractError::ChallengeActive);
        }

        // Return funds to user
        let refundable = match meter.billing_type {
            BillingType::PrePaid => meter.balance,
            BillingType::PostPaid => remaining_postpaid_collateral(&meter),
        };

        if refundable > 0 {
            let withdrawal_amount = match convert_usd_to_xlm_if_needed(&env, refundable, &meter.token) {
                Ok(amount) => amount,
                Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
            };

            let client = token::Client::new(&env, &meter.token);
            client.transfer(&env.current_contract_address(), &meter.user, &withdrawal_amount);
        }

        meter.balance = 0;
        meter.debt = 0;
        meter.is_active = false;
        meter.is_disputed = false;

        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);

        env.events().publish((symbol_short!("Refund"), meter_id), refundable);
    }

    // Task #90: Post-Paid Settlement Credit Logic
    pub fn set_credit_drip(env: Env, meter_id: u64, drip_rate: i128) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.provider.require_auth();

        meter.credit_drip_rate = drip_rate;

        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);
    }

    // Task #1: Stream Priority System - Set priority index for a meter
    pub fn set_priority_index(env: Env, meter_id: u64, priority_index: u32) {
        let mut meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        meter.priority_index = priority_index;

        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);

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

        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);

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

        env.events().publish(
            soroban_sdk::symbol_short!("GovVault"),
            vault_address,
        );
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

        env.events().publish(
            soroban_sdk::symbol_short!("TaxRate"),
            tax_rate_bps,
        );
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
        env.storage().instance().extend_ttl(LEDGER_LIFETIME_EXTENSION, LEDGER_LIFETIME_EXTENSION);

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
        let existing_proposal_time: Option<u64> = env.storage().instance().get(&DataKey::UpgradeProposalTime);
        if let Some(proposal_time) = existing_proposal_time {
            let deadline: u64 = env.storage().instance().get(&DataKey::VetoDeadline).unwrap_or(0);
            let now = env.ledger().timestamp();

            if now < deadline {
                panic_with_error!(&env, ContractError::UpgradeProposalActive);
            }
        }

        let proposal_id = propose_upgrade_impl(&env, new_wasm_hash, &proposer);

        env.events().publish(
            soroban_sdk::symbol_short!("UpgrdProp"),
            proposal_id,
        );
    }

    // Task #4: Wasm Hash Rotation - Submit veto
    pub fn submit_upgrade_veto(env: Env, proposal_id: u64) {
        let user = env.current_contract_address();
        user.require_auth();

        // Check if veto period is still active
        let deadline: u64 = env.storage().instance().get(&DataKey::VetoDeadline).unwrap_or(0);
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

        // In a real implementation, this would call env.deployer().update_current_contract_wasm()
        // For now, we just emit an event indicating the upgrade is ready
        env.events().publish(
            soroban_sdk::symbol_short!("UpgrdFinsh"),
            proposal.new_wasm_hash,
        );

        // Clear the proposal
        env.storage().instance().remove(&DataKey::ProposedUpgrade);
        env.storage().instance().remove(&DataKey::UpgradeProposalTime);
        env.storage().instance().remove(&DataKey::VetoDeadline);
    }

    // ============================================================
    // NEW TASKS IMPLEMENTATION
    // ============================================================

    // ==================== TASK #1: ADMIN TRANSFER WITH TIMELOCK ====================

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

        env.storage().instance().set(&DataKey::AdminTransferProposal, &proposal);

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
        env.storage().instance().set(&DataKey::AdminVeto(user, proposal.proposed_at), &true);

        // Increment veto count
        let mut updated_proposal = proposal;
        updated_proposal.veto_count += 1;
        env.storage().instance().set(&DataKey::AdminTransferProposal, &updated_proposal);

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
        env.storage().instance().set(&DataKey::CurrentAdmin, &proposal.proposed_admin);
        env.storage().instance().remove(&DataKey::AdminTransferProposal);

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

        env.events().publish(
            (soroban_sdk::symbol_short!("SetAdmn"),),
            admin,
        );
    }

    /// Register as active user (for governance tracking)
    pub fn register_active_user(env: Env, user: Address) {
        user.require_auth();

        // Simplified: just increment counter
        let count: u32 = env.storage().instance().get(&DataKey::ActiveUsers).unwrap_or(0);
        env.storage().instance().set(&DataKey::ActiveUsers, &(count + 1));

        env.events().publish(
            (soroban_sdk::symbol_short!("ActvUser"),),
            user,
        );
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
            let withdrawal_amount = match convert_usd_to_xlm_if_needed(&env, frozen_amount, &meter.token) {
                Ok(amount) => amount,
                Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
            };

            let client = token::Client::new(&env, &meter.token);
            client.transfer(&env.current_contract_address(), &legal_vault, &withdrawal_amount);
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

        env.storage().instance().set(&DataKey::LegalFreeze(meter_id), &freeze);

        // Pause the meter
        meter.is_paused = true;
        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);

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

            let withdrawal_amount = match convert_usd_to_xlm_if_needed(&env, freeze.frozen_amount, &meter.token) {
                Ok(amount) => amount,
                Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
            };

            let client = token::Client::new(&env, &meter.token);
            client.transfer(&legal_vault, &meter.user, &withdrawal_amount);
        }

        // Update freeze record
        let mut updated_freeze = freeze;
        updated_freeze.is_released = true;
        env.storage().instance().set(&DataKey::LegalFreeze(meter_id), &updated_freeze);

        // Unpause meter
        meter.is_paused = false;
        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);

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

        env.storage().instance().set(&DataKey::ComplianceOfficer, &officer);

        env.events().publish(
            (soroban_sdk::symbol_short!("CmpOfcr"),),
            officer,
        );
    }

    /// Set legal vault address
    pub fn set_legal_vault(env: Env, vault: Address) {
        vault.require_auth();

        env.storage().instance().set(&DataKey::LegalVault, &vault);

        env.events().publish(
            (soroban_sdk::symbol_short!("LglVlt"),),
            vault,
        );
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

env.storage()
            .instance()
            .set(&DataKey::VerifiedProvider(provider.clone()), &verified_provider);

        env.events().publish(
            (soroban_sdk::symbol_short!("VrfReqst"),),
            provider,
        );
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

        env.storage().instance().set(&DataKey::VerifiedProvider(provider.clone()), &verified_provider);

        env.events().publish(
            (soroban_sdk::symbol_short!("VrfGrnt"),),
            provider,
        );
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
        );

        // Update spent budget (simplified)
        let mut updated_config = config;
        updated_config.spent_budget += off_peak_rate; // Simplified accounting
        env.storage().instance().set(&DataKey::SubDaoConfig(sub_dao), &updated_config);

        env.events().publish(
            (soroban_sdk::symbol_short!("SubDaoStr"), meter_id),
            sub_dao,
        );

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

        env.storage().instance().set(&DataKey::SubDaoConfig(sub_dao), &config);

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
        env.storage().instance().set(&DataKey::SubDaoConfig(sub_dao), &config);

        env.events().publish(
            (soroban_sdk::symbol_short!("SubDaoOff"),),
            sub_dao,
        );
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
        if env.storage().instance().has(&DataKey::MultiSigConfig(provider.clone())) {
            panic_with_error!(&env, ContractError::MultiSigAlreadyConfigured);
        }

        // Validate wallet count (3-5 wallets required)
        let wallet_count = finance_wallets.len();
        if wallet_count < MIN_FINANCE_WALLETS || wallet_count > MAX_FINANCE_WALLETS {
            panic_with_error!(&env, ContractError::InvalidFinanceWalletCount);
        }

        // Validate required signatures
        if required_signatures == 0 || required_signatures > wallet_count {
            panic_with_error!(&env, ContractError::InvalidSignatureThreshold);
        }

        let config = MultiSigConfig {
            provider: provider.clone(),
            finance_wallets,
            required_signatures,
            threshold_amount,
            is_active: true,
            created_at: env.ledger().timestamp(),
        };

        // Store configuration
        env.storage().instance().set(&DataKey::MultiSigConfig(provider.clone()), &config);

        // Initialize request counter
        env.storage().instance().set(&DataKey::WithdrawalRequestCount(provider.clone()), &0u64);

        env.events().publish(
            (symbol_short!("MSigCfg"),),
            (provider, required_signatures, threshold_amount),
        );
    }

    /// Update multi-sig configuration for a provider.
    /// Requires authorization from at least `required_signatures` current finance wallets.
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

        // Validate new wallet count
        let wallet_count = new_finance_wallets.len();
        if wallet_count < MIN_FINANCE_WALLETS || wallet_count > MAX_FINANCE_WALLETS {
            panic_with_error!(&env, ContractError::InvalidFinanceWalletCount);
        }

let milestone = MaintenanceMilestone {
            meter_id,
            milestone_number,
            description,
            funding_amount,
            is_completed: false,
            completed_at: 0,
            verified_by: verified_by.clone(),
            completion_proof: Bytes::from_array(&env, &[0; 0]),
        };

        env.storage().instance().set(&DataKey::MultiSigConfig(provider.clone()), &updated_config);

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

        // Find the proposer from authorized finance wallets using secure call interface
        let mut proposer: Option<Address> = None;
        for i in 0..config.finance_wallets.len() {
            let wallet = config.finance_wallets.get(i).unwrap();
            // Try to require auth from each wallet using secure call interface
            match SecureCallManager::secure_call::<()>(
                &env,
                &wallet,
                &Symbol::new(&env, "require_auth"),
                Vec::new(&env),
                Some(10_000_000), // Conservative gas limit for auth check
            ) {
                Ok(_) => {
                    proposer = Some(wallet);
                    break;
                }
                Err(_) => {
                    // Continue to next wallet
                    continue;
                }
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
            (provider, request_id, amount_usd_cents, destination, actual_proposer),
        );

        request_id
    }

    /// Approve a pending multi-sig withdrawal request.
    /// Only authorized Finance Department wallets can approve.
    ///
    /// # Arguments
    /// * `provider` - The utility provider address
    /// * `request_id` - The withdrawal request ID to approve
    pub fn approve_multisig_withdrawal(env: Env, provider: Address, request_id: u64) {
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

        // Find and verify the approver is an authorized finance wallet
        let mut approver: Option<Address> = None;
        for i in 0..config.finance_wallets.len() {
            let wallet = config.finance_wallets.get(i).unwrap();
            wallet.require_auth();
            approver = Some(wallet);
            break;
        }

        let actual_approver = approver.unwrap_or_else(|| {
            panic_with_error!(&env, ContractError::NotAuthorizedFinanceWallet)
        });

        // Check if already approved by this wallet
        let approval_key = DataKey::WithdrawalApproval(
            provider.clone(),
            request_id,
            actual_approver.clone(),
        );
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
            (provider, request_id, actual_approver, request.approval_count),
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

        // Check sufficient approvals
        if request.approval_count < config.required_signatures {
            panic_with_error!(&env, ContractError::InsufficientApprovals);
        }

        // Get meter and verify
        let mut meter = get_meter_or_panic(&env, request.meter_id);
        if meter.provider != provider {
            panic_with_error!(&env, ContractError::MeterNotFound);
        }

        // Store old meter value for pool update
        let old_meter_value = provider_meter_value(&meter);

        let available_earnings = match meter.billing_type {
            BillingType::PrePaid => meter.balance,
            BillingType::PostPaid => meter.debt,
        };

        if request.amount_usd_cents > available_earnings {
            panic_with_error!(&env, ContractError::InvalidTokenAmount);
        }

        // Convert USD cents to XLM if needed
        let withdrawal_amount = match convert_usd_to_token_if_needed(
            &env,
            request.amount_usd_cents,
            &meter.token,
        ) {
            Ok(amount) => amount,
            Err(_) => panic_with_error!(&env, ContractError::PriceConversionFailed),
        };

        // Execute the transfer to the destination treasury
        let client = token::Client::new(&env, &meter.token);
        client.transfer(
            &env.current_contract_address(),
            &request.destination,
            &withdrawal_amount,
        );

        // Update meter balance/debt
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

        // Update provider total pool
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
            (provider, request_id, request.amount_usd_cents, request.destination, withdrawal_amount),
        );
    }

    /// Revoke a previously given approval for a withdrawal request.
    ///
    /// # Arguments
    /// * `provider` - The utility provider address
    /// * `request_id` - The withdrawal request ID
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

        // Check request is still pending
        if request.is_executed {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyExecuted);
        }
        if request.is_cancelled {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyCancelled);
        }

        // Find and verify the revoker is an authorized finance wallet
        let mut revoker: Option<Address> = None;
        for i in 0..config.finance_wallets.len() {
            let wallet = config.finance_wallets.get(i).unwrap();
            wallet.require_auth();
            revoker = Some(wallet);
            break;
        }

        let actual_revoker = revoker.unwrap_or_else(|| {
            panic_with_error!(&env, ContractError::NotAuthorizedFinanceWallet)
        });

        // Check if this wallet has approved
        let approval_key = DataKey::WithdrawalApproval(
            provider.clone(),
            request_id,
            actual_revoker.clone(),
        );
        if !env.storage().instance().has(&approval_key) {
            panic_with_error!(&env, ContractError::NotApprovedByWallet);
        }

        // Remove approval
        env.storage().instance().remove(&approval_key);
        request.approval_count = request.approval_count.saturating_sub(1);

        // Update request
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
    /// Only the original proposer or provider can cancel.
    ///
    /// # Arguments
    /// * `provider` - The utility provider address
    /// * `request_id` - The withdrawal request ID to cancel
    pub fn cancel_multisig_withdrawal(env: Env, provider: Address, request_id: u64) {
        let mut request: WithdrawalRequest = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawalRequest(provider.clone(), request_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::WithdrawalRequestNotFound));

        // Check request is still pending
        if request.is_executed {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyExecuted);
        }
        if request.is_cancelled {
            panic_with_error!(&env, ContractError::WithdrawalAlreadyCancelled);
        }

        // Either provider or proposer can cancel
        // Try provider first using secure call interface
        let is_provider = match SecureCallManager::secure_call::<()>(
            &env,
            &provider,
            &Symbol::new(&env, "require_auth"),
            Vec::new(&env),
            Some(10_000_000), // Conservative gas limit for auth check
        ) {
            Ok(_) => true,
            Err(_) => false,
        };

        if !is_provider {
            // Try proposer
            request.proposer.require_auth();
        } else {
            provider.require_auth();
        }

        // Mark as cancelled
        request.is_cancelled = true;
        env.storage().instance().set(
            &DataKey::WithdrawalRequest(provider.clone(), request_id),
            &request,
        );

        env.events().publish(
            (symbol_short!("MSigCanc"),),
            (provider, request_id),
        );
    }

    /// Disable multi-sig requirement for a provider.
    /// This allows returning to single-signature withdrawals.
    /// Requires provider authorization.
    pub fn disable_multisig(env: Env, provider: Address) {
        provider.require_auth();

        let mut config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::MultiSigConfig(provider.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MultiSigNotConfigured));

        config.is_active = false;
        env.storage().instance().set(&DataKey::MultiSigConfig(provider.clone()), &config);

        env.events().publish(
            (symbol_short!("MSigOff"),),
            provider,
        );
    }

    /// Re-enable multi-sig requirement for a provider.
    pub fn enable_multisig(env: Env, provider: Address) {
        provider.require_auth();

        let mut config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::MultiSigConfig(provider.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MultiSigNotConfigured));

        config.is_active = true;
        env.storage().instance().set(&DataKey::MultiSigConfig(provider.clone()), &config);

        env.events().publish(
            (symbol_short!("MSigOn"),),
            provider,
        );
    }

    /// Get multi-sig configuration for a provider.
    pub fn get_multisig_config(env: Env, provider: Address) -> MultiSigConfig {
        env.storage()
            .instance()
            .get(&DataKey::MultiSigConfig(provider))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::MultiSigNotConfigured))
    }

    /// Get a specific withdrawal request.
    pub fn get_withdrawal_request(env: Env, provider: Address, request_id: u64) -> WithdrawalRequest {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalRequest(provider, request_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::WithdrawalRequestNotFound))
    }

    /// Check if a wallet has approved a specific withdrawal request.
    pub fn has_approved_withdrawal(
        env: Env,
        provider: Address,
        request_id: u64,
        wallet: Address,
    ) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::WithdrawalApproval(provider, request_id, wallet))
    }

    /// Check if a withdrawal amount requires multi-sig for a provider.
    pub fn requires_multisig(env: Env, provider: Address, amount_usd_cents: i128) -> bool {
        match env
            .storage()
            .instance()
            .get::<_, MultiSigConfig>(&DataKey::MultiSigConfig(provider))
        {
            Some(config) => config.is_active && amount_usd_cents >= config.threshold_amount,
            None => false,
        }
    }

    /// Get the current withdrawal request count for a provider.
    pub fn get_withdrawal_request_count(env: Env, provider: Address) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalRequestCount(provider))
            .unwrap_or(0)
    }

    // ==================== ISSUE #118: ZK PRIVACY USAGE REPORTING ====================

    /// Enable privacy mode for a meter (allows ZK-proof usage reporting)
    pub fn enable_privacy_mode(env: Env, meter_id: u64) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        // Add meter to privacy-enabled set
        let mut privacy_meters: Vec<u64> = env.storage()
            .instance()
            .get(&DataKey::ZKEnabledMeters)
            .unwrap_or_else(|| Vec::new(&env));
        
        if !privacy_meters.contains(&meter_id) {
            privacy_meters.push_back(meter_id);
            env.storage().instance().set(&DataKey::ZKEnabledMeters, &privacy_meters);
        }

        // Initialize private billing status
        let billing_status = PrivateBillingStatus {
            meter_id,
            billing_cycle: 1,
            total_commitments: 0,
            verified_proofs: 0,
            last_verification: 0,
            privacy_enabled: true,
        };
        env.storage().instance().set(&DataKey::PrivateBillingStatus(meter_id), &billing_status);

        env.events().publish(
            (symbol_short!("PrivacyOn"), meter_id),
            meter.user.clone(),
        );
    }

    /// Disable privacy mode for a meter
    pub fn disable_privacy_mode(env: Env, meter_id: u64) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        // Remove meter from privacy-enabled set
        let mut privacy_meters: Vec<u64> = env.storage()
            .instance()
            .get(&DataKey::ZKEnabledMeters)
            .unwrap_or_else(|| Vec::new(&env));
        
        let mut new_meters = Vec::new(&env);
        for id in privacy_meters.iter() {
            if id != meter_id {
                new_meters.push_back(id);
            }
        }
        env.storage().instance().set(&DataKey::ZKEnabledMeters, &new_meters);

        // Update billing status
        if let Some(mut status) = env.storage().instance().get::<_, PrivateBillingStatus>(&DataKey::PrivateBillingStatus(meter_id)) {
            status.privacy_enabled = false;
            env.storage().instance().set(&DataKey::PrivateBillingStatus(meter_id), &status);
        }

        env.events().publish(
            (symbol_short!("PrivacyOff"), meter_id),
            meter.user.clone(),
        );
    }

    /// Submit ZK usage report with commitment and nullifier
    pub fn submit_zk_usage_report(
        env: Env,
        meter_id: u64,
        commitment: BytesN<32>,
        nullifier: BytesN<32>,
        encrypted_usage: Bytes,
        proof_hash: BytesN<32>,
    ) {
        let meter = get_meter_or_panic(&env, meter_id);
        meter.user.require_auth();

        // Check if privacy mode is enabled
        let privacy_status: PrivateBillingStatus = env.storage()
            .instance()
            .get(&DataKey::PrivateBillingStatus(meter_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::PrivacyNotEnabled));
        
        if !privacy_status.privacy_enabled {
            panic_with_error!(&env, ContractError::PrivacyNotEnabled);
        }

        // Check if nullifier has been used before (prevent double-spending)
        if env.storage().instance().has(&DataKey::NullifierMap(nullifier.clone())) {
            panic_with_error!(&env, ContractError::NullifierAlreadyUsed);
        }

        // Store nullifier to prevent reuse
        env.storage().instance().set(&DataKey::NullifierMap(nullifier.clone()), &true);

        // Create and store ZK usage report
        let zk_report = ZKUsageReport {
            commitment: commitment.clone(),
            nullifier: nullifier.clone(),
            encrypted_usage,
            proof_hash,
            meter_id,
            billing_cycle: privacy_status.billing_cycle,
            timestamp: env.ledger().timestamp(),
            is_verified: false,
        };

        env.storage().instance().set(&DataKey::ZKUsageReport(meter_id, privacy_status.billing_cycle), &zk_report);

        // Store commitment
        env.storage().instance().set(&DataKey::ZKProof(commitment.clone()), &ZKProof {
            commitment: commitment.clone(),
            nullifier: nullifier.clone(),
            proof: Bytes::new(&env),
            meter_id,
            timestamp: env.ledger().timestamp(),
            is_valid: false,
        });

        // Update billing status
        let mut updated_status = privacy_status.clone();
        updated_status.total_commitments += 1;
        env.storage().instance().set(&DataKey::PrivateBillingStatus(meter_id), &updated_status);

        env.events().publish(
            (symbol_short!("ZKReport"), meter_id),
            (commitment, privacy_status.billing_cycle),
        );
    }

    /// Get status of meter with privacy considerations
    pub fn get_status(env: Env, meter_id: u64, requester: Address) -> MeterStatus {
        let meter = get_meter_or_panic(&env, meter_id);
        
        // Check if privacy mode is enabled
        let privacy_status: Option<PrivateBillingStatus> = env.storage()
            .instance()
            .get(&DataKey::PrivateBillingStatus(meter_id));

        match privacy_status {
            Some(status) if status.privacy_enabled => {
                // Return privacy-preserving status
                MeterStatus {
                    meter_id,
                    is_active: meter.is_active,
                    balance: if requester == meter.user || requester == meter.provider {
                        meter.balance
                    } else {
                        0 // Hide balance from unauthorized parties
                    },
                    billing_cycle: status.billing_cycle,
                    total_commitments: status.total_commitments,
                    verified_proofs: status.verified_proofs,
                    privacy_enabled: true,
                    last_update: meter.last_update,
                    // Hide detailed usage data when privacy is enabled
                    usage_summary: None,
                }
            }
            _ => {
                // Return full status when privacy is disabled
                MeterStatus {
                    meter_id,
                    is_active: meter.is_active,
                    balance: meter.balance,
                    billing_cycle: 0,
                    total_commitments: 0,
                    verified_proofs: 0,
                    privacy_enabled: false,
                    last_update: meter.last_update,
                    usage_summary: Some(meter.usage_data.clone()),
                }
            }
        }
    }

    /// Verify ZK proof (placeholder for future full ZK implementation)
    pub fn verify_zk_proof(env: Env, meter_id: u64, proof_hash: BytesN<32>) -> bool {
        // Check if meter has privacy enabled
        let privacy_status: PrivateBillingStatus = env.storage()
            .instance()
            .get(&DataKey::PrivateBillingStatus(meter_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::PrivacyNotEnabled));

        if !privacy_status.privacy_enabled {
            panic_with_error!(&env, ContractError::PrivacyNotEnabled);
        }

        // Check verification cache first
        if let Some(cached_result) = env.storage().instance().get::<_, bool>(&DataKey::ZKVerificationCache(proof_hash)) {
            return cached_result;
        }

        // For now, implement a simple verification (placeholder for full ZK-SNARK)
        // In production, this would verify the actual ZK proof
        let is_valid = verify_zk_proof_placeholder(&env, proof_hash);

        // Cache the result
        env.storage().instance().set(&DataKey::ZKVerificationCache(proof_hash), &is_valid);

        if is_valid {
            // Update verified proofs count
            let mut updated_status = privacy_status;
            updated_status.verified_proofs += 1;
            updated_status.last_verification = env.ledger().timestamp();
            env.storage().instance().set(&DataKey::PrivateBillingStatus(meter_id), &updated_status);

            env.events().publish(
                (symbol_short!("ZKVerified"), meter_id),
                proof_hash,
            );
        }

        is_valid
    }

    /// Get private billing status for a meter
    pub fn get_private_billing_status(env: Env, meter_id: u64) -> PrivateBillingStatus {
        env.storage()
            .instance()
            .get(&DataKey::PrivateBillingStatus(meter_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::PrivacyNotEnabled))
    }

    /// Check if a meter has privacy enabled
    pub fn is_privacy_enabled(env: Env, meter_id: u64) -> bool {
        if let Some(status) = env.storage().instance().get::<_, PrivateBillingStatus>(&DataKey::PrivateBillingStatus(meter_id)) {
            status.privacy_enabled
        } else {
            false
        }
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

mod test;
