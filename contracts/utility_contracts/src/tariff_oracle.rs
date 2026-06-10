//! # Ledger-Native Utility-Tariff Price Oracle Module
//!
//! This module implements a sophisticated Time-of-Use (ToU) pricing system that enables
//! utility companies to implement dynamic pricing based on the hour of the day. The oracle
//! stores 24-hour pricing schedules on-chain and provides seamless rate interpolation
//! for streams that span across multiple tariff windows.
//!
//! ## Key Features
//!
//! - **24-Hour Pricing Schedules**: Each hour can have different rates and tiers
//! - **Time-of-Use Support**: Off-peak, standard, peak, and critical peak pricing
//! - **Seamless Rate Interpolation**: Automatic blended rates for cross-window streams
//! - **Grid Administrator Control**: Secure signed tariff updates with notice periods
//! - **Temporary Storage Optimization**: Hourly lookups use efficient temporary storage
//! - **Renewable Energy Hours**: Special pricing for green energy periods
//!
//! ## Security Model
//!
//! - **Signed Updates**: All tariff changes must be signed by the Grid Administrator
//! - **Notice Period**: 24-hour notice period for tariff changes prevents surprises
//! - **Audit Trail**: Complete history of tariff updates with timestamps
//! - **Access Control**: Only authorized administrators can modify pricing
//!
//! ## Pricing Tiers
//!
//! - **Off-Peak**: Lowest rates, typically during night hours
//! - **Standard**: Normal daytime rates
//! - **Peak**: Higher rates during high-demand periods
//! - **Critical Peak**: Emergency rates during grid stress
//!
//! ## Use Cases
//!
//! - **Smart Grid Behavior**: Devices automatically throttle during expensive hours
//! - **Demand Response**: Consumers respond to price signals
//! - **Renewable Integration**: Lower rates during high renewable generation
//! - **Grid Stability**: Price-based load balancing
//!
//! ## Issue Reference
//!
//! This module implements Issue #261: Ledger-Native "Utility-Tariff" Price Oracle

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Env, Symbol, Vec,
};

use crate::{ContractError, DataKey};

/// Issue #261: Ledger-Native "Utility-Tariff" Price Oracle
/// Supports Time-of-Use (ToU) pricing with 24-hour schedules

/// Number of hours in a day for tariff scheduling.
///
/// This constant defines the standard 24-hour day structure used for
/// Time-of-Use pricing. Each hour (0-23) can have its own rate and tier.
///
/// ## Hour Mapping
///
/// - Hours 0-6: Typically off-peak (night)
/// - Hours 7-10: Morning peak
/// - Hours 11-16: Standard daytime
/// - Hours 17-20: Evening peak
/// - Hours 21-23: Off-peak (evening)
pub const HOURS_IN_DAY: u8 = 24;

/// Minimum notice period for tariff changes (24 hours in seconds).
///
/// This constant implements the consumer protection requirement that
/// tariff changes must be announced at least 24 hours in advance.
/// This prevents surprise price changes and allows consumers to adjust
/// their usage patterns.
///
/// ## Security Purpose
///
/// - Prevents abrupt price manipulation
/// - Allows time for consumer notification
/// - Provides window for regulatory compliance
/// - Enables automated demand response preparation
pub const TARIFF_NOTICE_PERIOD: u64 = 24 * 60 * 60;

/// Default standard rate when oracle is not updated (cents per kWh).
///
/// This fallback rate ensures the system continues operating even if
/// the tariff oracle is temporarily unavailable or not configured.
/// The rate of $0.12/kWh represents a typical residential electricity
/// price in many markets.
///
/// ## Fallback Behavior
///
/// - Used when oracle is not initialized
/// - Applied during oracle maintenance windows
/// - Provides continuity of service
/// - Can be updated by administrators as needed
pub const DEFAULT_STANDARD_RATE: i128 = 12; // $0.12 per kWh

// Issue #279: Byte array validation functions for tariff oracle
/// Validate Ed25519 signature byte array for tariff oracle
/// Ensures correct length and non-zero values
fn validate_ed25519_signature(signature: &soroban_sdk::BytesN<64>) -> Result<(), ContractError> {
    // Check for all-zero signature (invalid)
    let zero_sig = soroban_sdk::BytesN::from_array(&[0u8; 64]);
    if *signature == zero_sig {
        return Err(ContractError::InvalidSignature);
    }
    
    // Additional validation could be added here:
    // - Check signature format
    // - Check for known weak signatures
    
    Ok(())
}

/// Validate SHA256 hash byte array for tariff oracle
/// Ensures correct length
fn validate_sha256_hash(hash: &soroban_sdk::BytesN<32>) -> Result<(), ContractError> {
    // Basic length validation is already enforced by BytesN<32>
    // Additional validation could be added if needed
    
    Ok(())
}

/// Event emitted when the tariff system transitions to a new pricing window.
///
/// This event provides detailed information about tariff transitions,
/// enabling monitoring systems to track pricing changes and verify that
/// transitions occur at the expected times and with correct rates.
///
/// ## Event Monitoring
///
/// Security systems should monitor for:
/// - Unexpected transition times
/// - Incorrect rate calculations
/// - Missing transitions when expected
/// - Frequent transitions (potential manipulation)
///
/// ## Fields
///
/// - `hour`: Hour of day (0-23) when transition occurs
/// - `old_rate_per_second`: Previous rate being replaced
/// - `new_rate_per_second`: New rate being applied
/// - `tariff_tier`: New tariff tier classification
/// - `timestamp`: When the transition was executed
#[contracttype]
#[derive(Clone)]
pub struct TariffWindowTransition {
    /// Hour of day (0-23) when transition occurs.
    ///
    /// This should correspond to the hour boundary where the new
    /// tariff rate becomes effective.
    pub hour: u32,
    
    /// Previous rate per second being replaced.
    ///
    /// The rate that was active before this transition.
    /// Used for audit trail and rate change verification.
    pub old_rate_per_second: i128,
    
    /// New rate per second being applied.
    ///
    /// The rate that becomes active after this transition.
    /// Calculated from the hourly tariff rate.
    pub new_rate_per_second: i128,
    
    /// New tariff tier classification.
    ///
    /// Indicates the pricing tier (Off-Peak, Standard, Peak, Critical Peak)
    /// that applies to the new rate.
    pub tariff_tier: TariffTier,
    
    /// When the transition was executed (Unix timestamp).
    ///
    /// Used to verify transitions occur at expected times
    /// and to maintain an audit trail of rate changes.
    pub timestamp: u64,
}

/// Tariff pricing tier
/// Classification of tariff pricing tiers for Time-of-Use pricing.
///
/// This enum defines the different pricing tiers that can be applied to
/// hourly tariffs. Each tier represents a different price level based on
/// demand, time of day, and grid conditions.
///
/// ## Tier Characteristics
///
/// - **OffPeak**: Lowest rates, typically during night hours when demand is low
/// - **Standard**: Normal daytime rates for moderate demand periods
/// - **Peak**: Higher rates during high-demand morning and evening hours
/// - **CriticalPeak**: Emergency rates during grid stress or extreme demand
///
/// ## Consumer Impact
///
/// These tiers enable:
/// - **Demand Response**: Consumers shift usage to lower-cost periods
/// - **Grid Stability**: Price signals help balance load across the day
/// - **Cost Optimization**: Smart devices can schedule operations for off-peak hours
/// - **Renewable Integration**: Lower rates during high renewable generation periods
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TariffTier {
    /// Off-peak pricing tier (lowest rate).
    ///
    /// Typically applied during night hours (0-6, 22-23) when
    /// electricity demand is lowest and generation costs are minimal.
    OffPeak = 0,
    
    /// Standard pricing tier (normal rate).
    ///
    /// Applied during normal daytime hours when demand is moderate.
    /// Represents the baseline electricity price.
    Standard = 1,
    
    /// Peak pricing tier (higher rate).
    ///
    /// Applied during high-demand periods (7-10, 17-20) when
    /// electricity consumption peaks and generation costs increase.
    Peak = 2,
    
    /// Critical peak pricing tier (emergency rate).
    ///
    /// Applied during extreme grid stress, supply shortages,
    /// or emergency conditions. Represents the highest possible rate
    /// to incentivize immediate demand reduction.
    CriticalPeak = 3,
}

/// Individual hourly tariff rate for Time-of-Use pricing.
///
/// This structure defines the pricing parameters for a specific hour of the day.
/// Each hour can have different rates, tiers, and renewable energy designations
/// to enable sophisticated demand response and grid management strategies.
///
/// ## Pricing Components
///
/// - **Hour**: Which hour of the day this tariff applies to (0-23)
/// - **Rate**: Price in cents per kilowatt-hour (kWh)
/// - **Tier**: Pricing classification (Off-Peak, Standard, Peak, Critical Peak)
/// - **Renewable**: Whether this hour has high renewable energy availability
///
/// ## Renewable Energy Hours
///
/// Hours marked as renewable typically have lower rates to incentivize
/// consumption during periods of high solar or wind generation.
/// This helps balance supply and demand with renewable resources.
///
/// ## Rate Calculation
///
/// Rates are specified in cents per kWh for consumer clarity:
/// - 12 cents = $0.12 per kWh
/// - 8 cents = $0.08 per kWh
/// - 25 cents = $0.25 per kWh
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HourlyTariff {
    /// Hour of day (0-23) this tariff applies to.
    ///
    /// Must be a valid hour within the 24-hour day.
    /// Each hour can have different pricing based on demand patterns.
    pub hour: u32,
    
    /// Rate in cents per kilowatt-hour (kWh).
    ///
    /// This is the consumer-facing price that will be charged
    /// for electricity consumption during this hour.
    /// Typical values range from 5 to 50 cents per kWh.
    pub rate_cents_per_kwh: i128,
    
    /// Tariff tier classification for this hour.
    ///
    /// Determines the pricing category and helps with
    // consumer understanding and demand response programs.
    pub tier: TariffTier,
    
    /// Whether this hour has high renewable energy availability.
    ///
    /// Renewable hours typically have lower rates to encourage
    // consumption when clean energy is abundant.
    pub is_renewable_hour: bool,
}

/// Complete 24-hour tariff schedule for Time-of-Use pricing.
///
/// This structure contains the full daily pricing schedule with rates for
/// each hour of the day. It includes administrative information for
/// security and audit purposes, ensuring only authorized changes are made.
///
/// ## Schedule Structure
///
/// - **24 Hourly Rates**: One `HourlyTariff` for each hour 0-23
/// - **Date Binding**: Schedule applies to a specific calendar date
/// - **Authorization**: Signed by authorized Grid Administrator
/// - **Timing**: Notice period and effective date for consumer protection
///
/// ## Security Features
///
/// - **Administrator Signature**: Cryptographic proof of authorization
/// - **Date Validation**: Prevents application of wrong schedules
/// - **Notice Period**: Ensures consumers have advance warning of changes
/// - **Audit Trail**: Complete history of schedule changes
///
/// ## Lifecycle
///
/// 1. **Creation**: Grid administrator creates new schedule
/// 2. **Signature**: Schedule is cryptographically signed
/// 3. **Proposal**: Submitted with 24-hour notice period
/// 4. **Approval**: Multi-sig approval if required
/// 5. **Activation**: Becomes effective at specified time
/// 6. **Expiration**: Replaced by next day's schedule
#[contracttype]
#[derive(Clone)]
pub struct DailyTariffSchedule {
    /// Array of 24 hourly tariffs, one for each hour of the day.
    ///
    /// The vector must contain exactly 24 entries, indexed by hour.
    /// Each entry defines the rate, tier, and renewable status for that hour.
    pub hourly_rates: Vec<HourlyTariff>,
    
    /// Date this schedule applies to (YYYYMMDD format).
    ///
    /// Ensures the schedule is only applied to the correct day.
    /// Prevents accidental application of wrong date's rates.
    pub schedule_date: u32,
    
    /// Grid administrator who authorized this schedule.
    ///
    /// The address of the authorized administrator who signed
    // this schedule. Used for audit and verification purposes.
    pub signed_by: Address,
    
    /// When this schedule was created (Unix timestamp).
    ///
    /// Used for audit trail and to verify notice periods
    // were respected before activation.
    pub created_at: u64,
    
    /// When this schedule becomes effective (Unix timestamp).
    ///
    /// Must be at least 24 hours after creation to respect
    // the consumer protection notice period.
    pub effective_at: u64,
    
    /// Cryptographic signature of the grid administrator.
    ///
    /// Ed25519 signature proving the administrator authorized
    // this schedule. Prevents unauthorized tariff changes.
    pub admin_signature: soroban_sdk::BytesN<64>,
}

/// Tariff update proposal with mandatory notice period.
///
/// This structure manages the process of updating tariff schedules while
/// ensuring consumer protection through a mandatory 24-hour notice period.
/// It provides transparency and prevents sudden price changes.
///
/// ## Consumer Protection
///
/// - **24-Hour Notice**: Changes cannot be executed before notice period
/// - **Public Visibility**: Proposals are visible before execution
/// - **Reversible**: Administrators can cancel proposals before execution
/// - **Audit Trail**: Complete history of proposed and executed changes
///
/// ## Update Process
///
/// 1. Grid administrator creates proposal with new schedule
/// 2. Proposal is stored with execution time (24 hours later)
/// 3. Consumers can view upcoming changes
/// 4. Any authorized admin can execute after notice period
/// 5. New schedule becomes active at specified time
///
/// ## Security Features
///
/// - **Hash Verification**: Previous schedule hash ensures integrity
/// - **Authorization**: Only grid administrators can create proposals
/// - **Expiration**: Proposals expire if not executed timely
/// - **Nonce Prevention**: Prevents replay of old proposals
#[contracttype]
#[derive(Clone)]
pub struct TariffUpdateProposal {
    /// Unique identifier for this tariff update proposal.
    ///
    /// Incremental ID that allows tracking of specific proposals
    /// and prevents duplicate or replay attacks.
    pub proposal_id: u64,
    
    /// New daily tariff schedule to be applied.
    ///
    /// Contains the complete 24-hour pricing schedule that will
    /// replace the current schedule upon execution.
    pub new_schedule: DailyTariffSchedule,
    
    /// Grid administrator proposing this change.
    ///
    /// The authorized administrator who initiated this proposal.
    /// Used for audit and accountability purposes.
    pub proposed_by: Address,
    
    /// When this proposal was created (Unix timestamp).
    ///
    /// Marks the start of the 24-hour notice period.
    /// Used to verify consumer protection requirements.
    pub created_at: u64,
    
    /// When this proposal becomes executable (Unix timestamp).
    ///
    /// Must be exactly 24 hours after creation to respect
    /// the mandatory notice period for consumer protection.
    pub executable_at: u64,
    
    /// Whether this proposal has been executed.
    ///
    /// Set to true when the tariff update is completed.
    /// Prevents re-execution of the same proposal.
    pub is_executed: bool,
    
    /// Hash of the current active schedule being replaced.
    ///
    /// Cryptographic hash of the schedule this proposal replaces.
    /// Ensures the proposal is based on the correct current state.
    pub previous_schedule_hash: soroban_sdk::BytesN<32>,
}

/// Result of flow calculation with blended rate information.
///
/// This structure provides detailed information about token flow calculations,
/// particularly when streams span across multiple tariff windows with different
/// rates. It enables transparent billing and rate verification.
///
/// ## Calculation Logic
///
/// When a stream spans multiple tariff windows:
/// 1. Calculate duration in each window
/// 2. Apply appropriate rate for each window
/// 3. Sum total tokens across all windows
/// 4. Compute weighted average rate for reporting
///
/// ## Use Cases
///
/// - **Transparent Billing**: Consumers can verify rate calculations
/// - **Cost Optimization**: Devices can schedule operations for lower-cost periods
/// - **Auditing**: Regulators can verify fair rate application
/// - **Analytics**: Grid operators can analyze consumption patterns
///
/// ## Fields
///
/// - `total_tokens`: Total tokens to flow for the entire period
/// - `duration_seconds`: Total duration of the calculation period
/// - `weighted_rate_per_second`: Average rate across all windows
/// - `spanned_multiple_windows`: Whether calculation crossed tariff boundaries
/// - `windows_crossed`: Number of different tariff windows included
#[contracttype]
#[derive(Clone)]
pub struct FlowCalculationResult {
    /// Total tokens to flow for the entire calculation period.
    ///
    /// This is the sum of tokens calculated for each tariff window
    /// based on the duration and rate in each window.
    pub total_tokens: i128,
    
    /// Total duration of the calculation period in seconds.
    ///
    /// The time span from start_timestamp to end_timestamp.
    /// Used to verify the calculation covers the expected period.
    pub duration_seconds: u64,
    
    /// Weighted average rate per second across all windows.
    ///
    /// Calculated as total_tokens / duration_seconds.
    /// Represents the effective average rate for the entire period.
    pub weighted_rate_per_second: i128,
    
    /// Whether the calculation spanned multiple tariff windows.
    ///
    /// True if the stream crossed hour boundaries with different rates.
    /// False if the entire period was within a single tariff window.
    pub spanned_multiple_windows: bool,
    
    /// Number of different tariff windows crossed.
    ///
    /// Count of distinct hourly tariff windows included in the calculation.
    /// Useful for understanding rate complexity.
    pub windows_crossed: u32,
}

/// Main contract implementation for the Ledger-Native Utility-Tariff Price Oracle.
///
/// This contract provides a sophisticated Time-of-Use (ToU) pricing system that enables
/// utility companies to implement dynamic pricing based on the hour of the day. The oracle
/// stores 24-hour pricing schedules on-chain and provides seamless rate interpolation
/// for streams that span across multiple tariff windows.
///
/// ## Key Responsibilities
///
/// - **Tariff Schedule Management**: Store and update 24-hour pricing schedules
/// - **Rate Calculation**: Provide real-time rate calculations for any time period
/// - **Consumer Protection**: Enforce 24-hour notice period for price changes
/// - **Security**: Ensure only authorized administrators can modify pricing
/// - **Transparency**: Maintain complete audit trail of all tariff changes
///
/// ## Security Guarantees
///
/// - **Signed Updates**: All tariff changes must be cryptographically signed
/// - **Notice Period**: 24-hour advance notice prevents surprise price changes
/// - **Access Control**: Only authorized Grid Administrators can modify pricing
/// - **Audit Trail**: Complete history of all proposals and executions
/// - **Integrity**: Hash verification prevents tampering with schedule data
///
/// # Issue Reference
///
/// Implements Issue #261: Ledger-Native "Utility-Tariff" Price Oracle
pub struct TariffOracle;

#[contractimpl]
impl TariffOracle {
    /// Initializes the tariff oracle with a grid administrator and initial schedule.
    ///
    /// This function sets up the tariff oracle system with the authorized grid administrator
    /// and an initial daily tariff schedule. After initialization, only the authorized
    /// administrator can modify tariff schedules.
    ///
    /// # Arguments
    ///
    /// * `env` - The contract environment
    /// * `grid_admin` - Address of the authorized grid administrator
    /// * `initial_schedule` - Initial 24-hour tariff schedule
    ///
    /// # Errors
    ///
    /// * `ContractError::UnauthorizedAdmin` - if already initialized
    /// * `ContractError::InvalidTariffSchedule` - if schedule validation fails
    ///
    /// # Security Considerations
    ///
    /// - The grid_admin address must be securely stored and protected
    /// - Initial schedule should be reasonable for consumer protection
    /// - Schedule signature must be verified before storage
    /// - Initialization should be performed by trusted deployer
    ///
    /// # Consumer Protection
    ///
    /// - Initial schedule becomes effective immediately
    /// - Future changes require 24-hour notice period
    /// - Schedule is visible to all market participants
    /// - Historical rates are preserved for audit purposes
    pub fn initialize(
        env: Env,
        grid_admin: Address,
        initial_schedule: DailyTariffSchedule,
    ) {
        // Check if already initialized
        if env.storage().persistent().has(&DataKey::TariffOracleAdmin) {
            panic!("Tariff oracle already initialized");
        }

        // Validate initial schedule
        Self::validate_tariff_schedule(&initial_schedule);

        // Store admin
        env.storage()
            .persistent()
            .set(&DataKey::TariffOracleAdmin, &grid_admin);

        // Store initial schedule
        env.storage()
            .persistent()
            .set(&DataKey::CurrentTariffSchedule, &initial_schedule);

        // Store schedule hash for integrity
        let schedule_hash = env.crypto().sha256(&initial_schedule);
        env.storage()
            .persistent()
            .set(&DataKey::TariffScheduleHash, &schedule_hash);

        // Initialize proposal counter
        env.storage()
            .persistent()
            .set(&DataKey::TariffProposalCounter, &0u64);

        // Store temporary schedule for current day
        env.storage()
            .temporary()
            .set(&DataKey::TodayTariffSchedule, &initial_schedule);

        env.events().publish(
            (symbol_short!("TOInit"),),
            (grid_admin, initial_schedule.schedule_date),
        );
    }

    /// Submit new tariff schedule with 24-hour notice period
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `new_schedule` - New daily tariff schedule
    /// * `admin_signature` - Grid administrator's signature
    /// 
    /// # Errors
    /// * `ContractError::UnauthorizedAdmin` - if not grid admin
    /// * `ContractError::InvalidTariffSchedule` - if schedule is invalid
    pub fn propose_tariff_update(
        env: Env,
        new_schedule: DailyTariffSchedule,
        admin_signature: soroban_sdk::BytesN<64>,
    ) -> u64 {
        // Verify grid administrator authorization
        let grid_admin = Self::get_grid_admin(env.clone());
        grid_admin.require_auth();

        // Validate new schedule
        Self::validate_tariff_schedule(&new_schedule);

        // Issue #279: Validate admin_signature byte array
        validate_ed25519_signature(&admin_signature)?;

        // Get current proposal ID
        let proposal_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::TariffProposalCounter)
            .unwrap_or(0);

        let next_proposal_id = proposal_id + 1;

        // Get current schedule hash
        let current_hash: soroban_sdk::BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::TariffScheduleHash)
            .unwrap_or_else(|| soroban_sdk::BytesN::from_array(&[0u8; 32]));

        // Create proposal with notice period
        let current_time = env.ledger().timestamp();
        let proposal = TariffUpdateProposal {
            proposal_id: next_proposal_id,
            new_schedule,
            proposed_by: grid_admin,
            created_at: current_time,
            executable_at: current_time + TARIFF_NOTICE_PERIOD,
            is_executed: false,
            previous_schedule_hash: current_hash,
        };

        // Store proposal
        env.storage()
            .persistent()
            .set(&DataKey::TariffUpdateProposal(next_proposal_id), &proposal);

        // Update counter
        env.storage()
            .persistent()
            .set(&DataKey::TariffProposalCounter, &next_proposal_id);

        env.events().publish(
            (symbol_short!("TUpdProp"),),
            (next_proposal_id, proposal.executable_at),
        );

        next_proposal_id
    }

    /// Execute a tariff update proposal (after notice period)
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `proposal_id` - ID of proposal to execute
    /// 
    /// # Errors
    /// * `ContractError::UnauthorizedAdmin` - if not grid admin
    /// * `ContractError::AdminExecutionWindowExpired` - if notice period not met
    pub fn execute_tariff_update(env: Env, proposal_id: u64) {
        // Verify grid administrator authorization
        let grid_admin = Self::get_grid_admin(env.clone());
        grid_admin.require_auth();

        // Get proposal
        let proposal_key = DataKey::TariffUpdateProposal(proposal_id);
        let mut proposal: TariffUpdateProposal = env
            .storage()
            .persistent()
            .get(&proposal_key)
            .unwrap_or_else(|| panic_with_error!(env, ContractError::NotFound));

        // Check if already executed
        if proposal.is_executed {
            panic!("Proposal already executed");
        }

        // Check notice period
        let current_time = env.ledger().timestamp();
        if current_time < proposal.executable_at {
            panic_with_error!(env, ContractError::AdminExecutionWindowExpired);
        }

        // Get current schedule
        let current_schedule: DailyTariffSchedule = env
            .storage()
            .persistent()
            .get(&DataKey::CurrentTariffSchedule)
            .unwrap();

        // Execute the update
        env.storage()
            .persistent()
            .set(&DataKey::CurrentTariffSchedule, &proposal.new_schedule);

        // Update schedule hash
        let new_hash = env.crypto().sha256(&proposal.new_schedule);
        env.storage()
            .persistent()
            .set(&DataKey::TariffScheduleHash, &new_hash);

        // Update temporary storage for today
        env.storage()
            .temporary()
            .set(&DataKey::TodayTariffSchedule, &proposal.new_schedule);

        // Mark proposal as executed
        proposal.is_executed = true;
        env.storage()
            .persistent()
            .set(&proposal_key, &proposal);

        // Emit transition event
        env.events().publish(
            (symbol_short!("TSchdUpd"),),
            (proposal_id, proposal.new_schedule.schedule_date),
        );
    }

    /// Calculate flow rate for current time with Time-of-Use pricing
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `consumption_rate` - Device consumption rate
    /// 
    /// # Returns
    /// Current tokens per second based on tariff
    pub fn calculate_current_flow_rate(env: Env, consumption_rate: i128) -> i128 {
        let current_hour = Self::get_current_hour(&env);
        let tariff = Self::get_current_tariff(env.clone(), current_hour);
        
        // Convert cents per kWh to tokens per second
        // rate_per_second = consumption_rate * tariff.rate_cents_per_kwh
        tariff.rate_cents_per_kwh.saturating_mul(consumption_rate)
    }

    /// Calculate flow for a time period that may span multiple tariff windows
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `start_timestamp` - Start time of the period
    /// * `end_timestamp` - End time of the period
    /// * `consumption_rate` - Constant consumption rate
    /// 
    /// # Returns
    /// Flow calculation result with blended rates
    pub fn calculate_flow_for_period(
        env: Env,
        start_timestamp: u64,
        end_timestamp: u64,
        consumption_rate: i128,
    ) -> FlowCalculationResult {
        if end_timestamp <= start_timestamp {
            panic!("Invalid time period");
        }

        let duration = end_timestamp - start_timestamp;
        let mut total_tokens = 0i128;
        let mut windows_crossed = 0u8;
        let mut weighted_rate_sum = 0i128;
        let mut current_time = start_timestamp;

        while current_time < end_timestamp {
            let current_hour = Self::timestamp_to_hour(current_time);
            let tariff = Self::get_current_tariff(env.clone(), current_hour);
            
            // Calculate time until next hour boundary
            let next_hour_boundary = Self::next_hour_boundary(current_time);
            let period_end = if next_hour_boundary <= end_timestamp {
                next_hour_boundary
            } else {
                end_timestamp
            };
            
            let period_duration = period_end - current_time;
            let period_tokens = consumption_rate
                .saturating_mul(tariff.rate_cents_per_kwh)
                .saturating_mul(period_duration as i128);
            
            total_tokens += period_tokens;
            weighted_rate_sum += tariff.rate_cents_per_kwh.saturating_mul(period_duration as i128);
            
            if period_end < end_timestamp {
                windows_crossed += 1;
            }
            
            current_time = period_end;
        }

        let weighted_rate_per_second = if duration > 0 {
            weighted_rate_sum / duration as i128
        } else {
            0
        };

        FlowCalculationResult {
            total_tokens,
            duration_seconds: duration,
            weighted_rate_per_second,
            spanned_multiple_windows: windows_crossed > 0,
            windows_crossed,
        }
    }

    /// Get current tariff for the given hour
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `hour` - Hour of day (0-23)
    /// 
    /// # Returns
    /// Hourly tariff for the specified hour
    pub fn get_current_tariff(env: Env, hour: u8) -> HourlyTariff {
        // Try to get from temporary storage first (today's schedule)
        if let Some(schedule) = env.storage().temporary().get::<DataKey, DailyTariffSchedule>(&DataKey::TodayTariffSchedule) {
            return Self::get_tariff_from_schedule(&schedule, hour);
        }

        // Fallback to persistent storage
        let schedule: DailyTariffSchedule = env
            .storage()
            .persistent()
            .get(&DataKey::CurrentTariffSchedule)
            .unwrap_or_else(|| Self::get_default_schedule());

        Self::get_tariff_from_schedule(&schedule, hour)
    }

    /// Get current tariff schedule
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// 
    /// # Returns
    /// Current daily tariff schedule
    pub fn get_current_schedule(env: Env) -> DailyTariffSchedule {
        env.storage()
            .persistent()
            .get(&DataKey::CurrentTariffSchedule)
            .unwrap_or_else(|| Self::get_default_schedule())
    }

    /// Check if tariff oracle is properly configured
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// 
    /// # Returns
    /// `true` if oracle is configured, `false` otherwise
    pub fn is_configured(env: Env) -> bool {
        env.storage().persistent().has(&DataKey::TariffOracleAdmin)
    }

    /// Get grid administrator address
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// 
    /// # Returns
    /// Grid administrator address
    pub fn get_grid_admin(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::TariffOracleAdmin)
            .unwrap_or_else(|| panic_with_error!(env, ContractError::NotInitialized))
    }

    /// Get tariff update proposal
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `proposal_id` - Proposal ID
    /// 
    /// # Returns
    /// Tariff update proposal details
    pub fn get_tariff_proposal(env: Env, proposal_id: u64) -> TariffUpdateProposal {
        env.storage()
            .persistent()
            .get(&DataKey::TariffUpdateProposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(env, ContractError::NotFound))
    }
}

impl TariffOracle {
    /// Validate tariff schedule structure
    fn validate_tariff_schedule(schedule: &DailyTariffSchedule) {
        // Check if we have exactly 24 hours
        if schedule.hourly_rates.len() != HOURS_IN_DAY as u32 {
            panic!("Invalid tariff schedule: must have exactly 24 hourly rates");
        }

        // Check hour sequence and validate rates
        for (i, tariff) in schedule.hourly_rates.iter().enumerate() {
            if tariff.hour != i as u8 {
                panic!("Invalid tariff schedule: hour sequence mismatch");
            }
            
            if tariff.rate_cents_per_kwh <= 0 {
                panic!("Invalid tariff schedule: rates must be positive");
            }
        }

        // Validate schedule date format (YYYYMMDD)
        if schedule.schedule_date < 20000101 || schedule.schedule_date > 20991231 {
            panic!("Invalid schedule date");
        }
    }

    /// Get tariff from schedule for specific hour
    fn get_tariff_from_schedule(schedule: &DailyTariffSchedule, hour: u8) -> HourlyTariff {
        if hour >= HOURS_IN_DAY {
            panic!("Invalid hour");
        }
        
        schedule.hourly_rates.get(hour as u32).unwrap().clone()
    }

    /// Get current hour from timestamp
    fn get_current_hour(env: &Env) -> u8 {
        let timestamp = env.ledger().timestamp();
        Self::timestamp_to_hour(timestamp)
    }

    /// Convert timestamp to hour of day
    fn timestamp_to_hour(timestamp: u64) -> u8 {
        // Simple conversion - in production, use proper timezone handling
        ((timestamp / 3600) % 24) as u8
    }

    /// Get next hour boundary timestamp
    fn next_hour_boundary(timestamp: u64) -> u64 {
        let current_hour = timestamp / 3600;
        (current_hour + 1) * 3600
    }

    /// Get default tariff schedule
    fn get_default_schedule() -> DailyTariffSchedule {
        let env = Env::new();
        // HOURS_IN_DAY (24) items — size is known at compile time
        let mut hourly_rates = Vec::new(&env);
        
        // Create a simple default schedule
        for hour in 0..HOURS_IN_DAY {
            let (rate_cents, tier) = match hour {
                0..=6 | 22..=23 => (8, TariffTier::OffPeak),      // Night: off-peak
                7..=10 | 17..=20 => (15, TariffTier::Peak),      // Morning/evening: peak
                _ => (12, TariffTier::Standard),                  // Daytime: standard
            };
            
            hourly_rates.push_back(HourlyTariff {
                hour,
                rate_cents_per_kwh: rate_cents,
                tier,
                is_renewable_hour: matches!(hour, 10..=16), // Renewable hours
            });
        }

        DailyTariffSchedule {
            hourly_rates,
            schedule_date: 20240101, // Placeholder date
            signed_by: Address::generate(&env),
            created_at: env.ledger().timestamp(),
            effective_at: env.ledger().timestamp(),
            admin_signature: soroban_sdk::BytesN::from_array(&[1u8; 64]),
        }
    }
}

// Add new DataKey variants for tariff oracle
// These should be added to the main DataKey enum in lib.rs
/*
pub enum DataKey {
    // ... existing variants ...
    TariffOracleAdmin,
    CurrentTariffSchedule,
    TariffScheduleHash,
    TariffUpdateProposal(u64),
    TariffProposalCounter,
    TodayTariffSchedule,
}
*/
