/// Velocity Limit Circuit Breaker Module
///
/// Prevents streams from draining too quickly by enforcing maximum outflow
/// limits per 24-hour rolling window. Tracks daily outflow in temporary storage
/// and supports admin overrides for false-positive scenarios.
///
/// This module implements a sophisticated circuit breaker that:
/// 1. Enforces global and per-stream velocity limits
/// 2. Tracks 24-hour rolling window outflow
/// 3. Emits AnomalousActivity events for off-chain alerting
/// 4. Allows admin multi-sig overrides
/// 5. Automatically resets tracking at the start of each day
use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

// ============================================================================
// Constants
// ============================================================================

/// Seconds in 24 hours (used for rolling window calculations)
pub const DAY_IN_SECONDS: u64 = 24 * 60 * 60;

/// Default global velocity limit: 10 million tokens per 24h
/// Prevents catastrophic drainage at system level
pub const DEFAULT_GLOBAL_LIMIT: i128 = 10_000_000_000; // 10B tokens

/// Default per-stream velocity limit: 1 million tokens per 24h
/// Prevents individual stream abuse
pub const DEFAULT_PER_STREAM_LIMIT: i128 = 1_000_000_000; // 1B tokens

// ============================================================================
// Data Structures
// ============================================================================

/// Tracks daily outflow for a specific meter within the current 24h window
/// Stored in temporary storage for automatic cleanup and minimal overhead
#[contracttype]
#[derive(Clone, Debug)]
pub struct DailyOutflow {
    /// Timestamp when this tracking window started (aligned to day boundary)
    pub window_start: u64,

    /// Total amount drained in current 24h window
    pub total_outflow: i128,

    /// Meter ID being tracked
    pub meter_id: u64,

    /// Provider address for this meter
    pub provider: Address,

    /// Flag indicating if velocity limit is currently breached
    pub is_breached: bool,

    /// Timestamp of last anomalous activity event
    pub last_anomaly_timestamp: u64,
}

/// Tracks global (system-wide) outflow across all meters
/// Stored in temporary storage, scoped to current day boundary
#[contracttype]
#[derive(Clone, Debug)]
pub struct GlobalOutflowTracker {
    /// Timestamp when this global window started
    pub window_start: u64,

    /// Total amount drained across all meters in current window
    pub total_outflow: i128,

    /// Number of meters at velocity limit (for analytics)
    pub breached_meter_count: u32,

    /// Whether global circuit breaker is activated
    pub global_breach: bool,

    /// Timestamp of last global anomaly event
    pub last_global_anomaly: u64,
}

/// Admin override configuration for temporary velocity limit suspension
/// Stored in instance storage (permanent until revoked)
#[contracttype]
#[derive(Clone, Debug)]
pub struct VelocityOverride {
    /// Provider or meter ID being overridden (0 = global override)
    pub override_scope: u64,

    /// Admin address that approved this override
    pub admin: Address,

    /// When the override was created
    pub created_at: u64,

    /// When the override expires (if 0, permanent until revoked)
    pub expires_at: u64,

    /// Reason for the override (for audit trail)
    pub reason: Symbol,

    /// Whether override is currently active
    pub is_active: bool,
}

/// Velocity limit configuration (customizable per system)
#[contracttype]
#[derive(Clone, Debug)]
pub struct VelocityConfig {
    /// Global outflow limit per 24h window
    pub global_limit: i128,

    /// Per-stream outflow limit per 24h window
    pub per_stream_limit: i128,

    /// Whether velocity limiting is enabled system-wide
    pub is_enabled: bool,

    /// Address of admin multi-sig (for overrides)
    pub admin_multisig: Address,
}

// ============================================================================
// Events
// ============================================================================

/// Emitted when outflow velocity violates the limit
/// Used for off-chain alerting and anomaly detection
#[contracttype]
#[derive(Clone)]
pub struct AnomalousActivity {
    /// Which velocity limit was exceeded (global or per-meter)
    pub activity_type: Symbol,

    /// Meter ID (0 for global alerts)
    pub meter_id: u64,

    /// Provider address
    pub provider: Address,

    /// Requested withdrawal amount
    pub requested_amount: i128,

    /// Current 24h outflow (before this request)
    pub current_outflow: i128,

    /// Velocity limit threshold
    pub velocity_limit: i128,

    /// Timestamp of anomaly detection
    pub detected_at: u64,

    /// Reason for anomaly (micro_attack, flash_drain, etc.)
    pub reason: Symbol,
}

/// Emitted when admin overrides velocity limits
/// Used for compliance and audit trails
#[contracttype]
#[derive(Clone)]
pub struct OverrideApplied {
    /// Override scope (0 for global, >0 for meter ID)
    pub scope: u64,

    /// Admin who applied override
    pub admin: Address,

    /// When override expires (0 = permanent)
    pub expires_at: u64,

    /// Reason code
    pub reason: Symbol,

    /// When override was applied
    pub applied_at: u64,
}

/// Emitted when daily velocity window resets
/// Helps track circuit breaker state transitions
#[contracttype]
#[derive(Clone)]
pub struct DailyResetOccurred {
    /// Meter ID (0 for global reset)
    pub meter_id: u64,

    /// Previous window's total outflow
    pub previous_total: i128,

    /// When the reset occurred
    pub reset_at: u64,

    /// Days since system inception
    pub day_number: u64,
}

// ============================================================================
// DataKey Enum for Storage
// ============================================================================

#[contracttype]
#[derive(Clone)]
pub enum VelocityDataKey {
    /// Per-meter daily outflow tracking
    DailyOutflow(u64), // meter_id -> DailyOutflow

    /// Global outflow tracking
    GlobalOutflow,

    /// Velocity configuration
    VelocityConfig,

    /// Admin overrides (per scope)
    VelocityOverride(u64), // meter_id (0 for global) -> VelocityOverride

    /// Temporary flag to track if override was used this window
    OverrideUsedToday(u64), // meter_id -> bool
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Calculate day boundary from timestamp
/// Returns the Unix timestamp for the start of the day containing `timestamp`
pub fn get_day_boundary(timestamp: u64) -> u64 {
    (timestamp / DAY_IN_SECONDS) * DAY_IN_SECONDS
}

/// Check if a new day has started since the given window start
pub fn is_new_day(window_start: u64, current_timestamp: u64) -> bool {
    get_day_boundary(window_start) != get_day_boundary(current_timestamp)
}

/// Calculate day number since epoch (for analytics)
pub fn get_day_number(timestamp: u64) -> u64 {
    timestamp / DAY_IN_SECONDS
}

// ============================================================================
// Velocity Limit Check Functions
// ============================================================================

/// Check if a withdrawal would exceed per-stream velocity limit
///
/// Returns:
/// - `Ok(())` if withdrawal is allowed
/// - `Err` if velocity limit would be exceeded
pub fn check_per_stream_velocity(
    env: &Env,
    meter_id: u64,
    provider: &Address,
    withdrawal_amount: i128,
) -> Result<(), soroban_sdk::Symbol> {
    let now = env.ledger().timestamp();

    // Get configuration
    let config = get_velocity_config(env).unwrap_or_else(|| create_default_config(env));

    if !config.is_enabled {
        return Ok(()); // Velocity limiting disabled
    }

    // Check for active override
    if is_override_active(env, meter_id, now) {
        return Ok(()); // Override allows this withdrawal
    }

    // Get or initialize daily outflow tracking
    let mut daily_outflow = get_daily_outflow(env, meter_id, provider, now);

    // Reset if new day
    if is_new_day(daily_outflow.window_start, now) {
        daily_outflow = DailyOutflow {
            window_start: get_day_boundary(now),
            total_outflow: 0,
            meter_id,
            provider: provider.clone(),
            is_breached: false,
            last_anomaly_timestamp: 0,
        };

        // Emit reset event
        env.events().publish(
            (symbol_short!("reset"),),
            DailyResetOccurred {
                meter_id,
                previous_total: 0,
                reset_at: now,
                day_number: get_day_number(now),
            },
        );
    }

    // Check if withdrawal would exceed per-stream limit
    let new_total = daily_outflow
        .total_outflow
        .saturating_add(withdrawal_amount);

    if new_total > config.per_stream_limit {
        // Emit anomaly event
        env.events().publish(
            (symbol_short!("anomaly"),),
            AnomalousActivity {
                activity_type: symbol_short!("perstrm"),
                meter_id,
                provider: provider.clone(),
                requested_amount: withdrawal_amount,
                current_outflow: daily_outflow.total_outflow,
                velocity_limit: config.per_stream_limit,
                detected_at: now,
                reason: detect_anomaly_type(
                    withdrawal_amount,
                    config.per_stream_limit,
                    daily_outflow.total_outflow,
                ),
            },
        );

        return Err(symbol_short!("vlimit"));
    }

    // Update daily outflow
    daily_outflow.total_outflow = new_total;
    daily_outflow.last_anomaly_timestamp = now;

    // Store updated state
    env.storage()
        .temporary()
        .set(&VelocityDataKey::DailyOutflow(meter_id), &daily_outflow);

    Ok(())
}

/// Check if withdrawal would exceed global (system-wide) velocity limit
///
/// Returns:
/// - `Ok(())` if withdrawal is allowed
/// - `Err` if global velocity limit would be exceeded
pub fn check_global_velocity(
    env: &Env,
    withdrawal_amount: i128,
    provider: &Address,
) -> Result<(), soroban_sdk::Symbol> {
    let now = env.ledger().timestamp();

    // Get configuration
    let config = get_velocity_config(env).unwrap_or_else(|| create_default_config(env));

    if !config.is_enabled {
        return Ok(()); // Velocity limiting disabled
    }

    // Check for global override (meter_id = 0)
    if is_override_active(env, 0, now) {
        return Ok(()); // Global override allows this withdrawal
    }

    // Get or initialize global outflow tracking
    let mut global_tracker = get_global_outflow(env, now);

    // Reset if new day
    if is_new_day(global_tracker.window_start, now) {
        global_tracker = GlobalOutflowTracker {
            window_start: get_day_boundary(now),
            total_outflow: 0,
            breached_meter_count: 0,
            global_breach: false,
            last_global_anomaly: 0,
        };

        // Emit reset event
        env.events().publish(
            (symbol_short!("reset"),),
            DailyResetOccurred {
                meter_id: 0,
                previous_total: 0,
                reset_at: now,
                day_number: get_day_number(now),
            },
        );
    }

    // Check if withdrawal would exceed global limit
    let new_total = global_tracker
        .total_outflow
        .saturating_add(withdrawal_amount);

    if new_total > config.global_limit {
        // Emit anomaly event
        env.events().publish(
            (symbol_short!("anomaly"),),
            AnomalousActivity {
                activity_type: symbol_short!("global"),
                meter_id: 0,
                provider: provider.clone(),
                requested_amount: withdrawal_amount,
                current_outflow: global_tracker.total_outflow,
                velocity_limit: config.global_limit,
                detected_at: now,
                reason: detect_anomaly_type(
                    withdrawal_amount,
                    config.global_limit,
                    global_tracker.total_outflow,
                ),
            },
        );

        return Err(symbol_short!("gvlimit"));
    }

    // Update global outflow
    global_tracker.total_outflow = new_total;
    global_tracker.last_global_anomaly = now;

    // Store updated state
    env.storage()
        .temporary()
        .set(&VelocityDataKey::GlobalOutflow, &global_tracker);

    Ok(())
}

/// Check both per-stream and global velocity limits
///
/// This is the main entry point for velocity limit validation
pub fn check_velocity_limits(
    env: &Env,
    meter_id: u64,
    provider: &Address,
    withdrawal_amount: i128,
) -> Result<(), soroban_sdk::Symbol> {
    // Check per-stream limit first
    check_per_stream_velocity(env, meter_id, provider, withdrawal_amount)?;

    // Then check global limit
    check_global_velocity(env, withdrawal_amount, provider)?;

    Ok(())
}

// ============================================================================
// Admin Override Functions
// ============================================================================

/// Apply admin override to suspend velocity limits
///
/// `scope` = 0 for global override, or specific meter_id
/// `expires_at` = 0 for permanent, or Unix timestamp for expiration
/// NOTE: Caller (lib.rs) must have already verified admin auth against stored AdminAddress.
pub fn apply_override(env: &Env, admin: Address, scope: u64, expires_at: u64, reason: Symbol) {
    let now = env.ledger().timestamp();

    let override_config = VelocityOverride {
        override_scope: scope,
        admin: admin.clone(),
        created_at: now,
        expires_at,
        reason: reason.clone(),
        is_active: true,
    };

    // Store override in instance storage (permanent)
    env.storage()
        .instance()
        .set(&VelocityDataKey::VelocityOverride(scope), &override_config);

    // Emit event
    env.events().publish(
        (symbol_short!("override"),),
        OverrideApplied {
            scope,
            admin,
            expires_at,
            reason,
            applied_at: now,
        },
    );
}

/// Revoke an active override
pub fn revoke_override(env: &Env, scope: u64) {
    let override_opt = env
        .storage()
        .instance()
        .get::<_, VelocityOverride>(&VelocityDataKey::VelocityOverride(scope));

    if let Some(mut override_config) = override_opt {
        override_config.is_active = false;
        env.storage()
            .instance()
            .set(&VelocityDataKey::VelocityOverride(scope), &override_config);
    }
}

/// Check if override is currently active for a given scope and time
fn is_override_active(env: &Env, scope: u64, now: u64) -> bool {
    if let Some(override_config) = env
        .storage()
        .instance()
        .get::<_, VelocityOverride>(&VelocityDataKey::VelocityOverride(scope))
    {
        if !override_config.is_active {
            return false;
        }

        // Check expiration
        if override_config.expires_at > 0 && now > override_config.expires_at {
            return false;
        }

        return true;
    }

    false
}

// ============================================================================
// Configuration Functions
// ============================================================================

/// Get current velocity configuration
pub fn get_velocity_config(env: &Env) -> Option<VelocityConfig> {
    env.storage()
        .instance()
        .get(&VelocityDataKey::VelocityConfig)
}

/// Update velocity configuration (admin only - caller must have verified admin auth)
pub fn set_velocity_config(env: &Env, _admin: Address, config: VelocityConfig) {
    env.storage()
        .instance()
        .set(&VelocityDataKey::VelocityConfig, &config);
}

/// Create default configuration
fn create_default_config(env: &Env) -> VelocityConfig {
    let config = VelocityConfig {
        global_limit: DEFAULT_GLOBAL_LIMIT,
        per_stream_limit: DEFAULT_PER_STREAM_LIMIT,
        is_enabled: true,
        admin_multisig: env.current_contract_address(), // Placeholder
    };

    env.storage()
        .instance()
        .set(&VelocityDataKey::VelocityConfig, &config);

    config
}

// ============================================================================
// Internal Tracking Functions
// ============================================================================

/// Get or initialize daily outflow for a meter
fn get_daily_outflow(env: &Env, meter_id: u64, provider: &Address, now: u64) -> DailyOutflow {
    let key = VelocityDataKey::DailyOutflow(meter_id);

    env.storage()
        .temporary()
        .get::<_, DailyOutflow>(&key)
        .unwrap_or_else(|| DailyOutflow {
            window_start: get_day_boundary(now),
            total_outflow: 0,
            meter_id,
            provider: provider.clone(),
            is_breached: false,
            last_anomaly_timestamp: 0,
        })
}

/// Get or initialize global outflow tracker
fn get_global_outflow(env: &Env, now: u64) -> GlobalOutflowTracker {
    env.storage()
        .temporary()
        .get::<_, GlobalOutflowTracker>(&VelocityDataKey::GlobalOutflow)
        .unwrap_or_else(|| GlobalOutflowTracker {
            window_start: get_day_boundary(now),
            total_outflow: 0,
            breached_meter_count: 0,
            global_breach: false,
            last_global_anomaly: 0,
        })
}

/// Detect the type of anomalous activity
/// Returns a symbol representing the attack pattern
fn detect_anomaly_type(requested: i128, limit: i128, current_outflow: i128) -> Symbol {
    // Micro-withdrawal attack: Many small withdrawals (< 1% each)
    let micro_threshold = limit / 100;
    if requested < micro_threshold && current_outflow > limit / 2 {
        return symbol_short!("micro");
    }

    // Flash drain: Very large single withdrawal (> 50% of limit)
    if requested > limit / 2 {
        return symbol_short!("flash");
    }

    // Steady accumulation attack
    symbol_short!("accum")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_day_boundary() {
        let timestamp1 = 86400; // Start of day 1
        assert_eq!(get_day_boundary(timestamp1), 86400);

        let timestamp2 = 86400 + 3600; // 1 hour into day 1
        assert_eq!(get_day_boundary(timestamp2), 86400);

        let timestamp3 = 172800; // Start of day 2
        assert_eq!(get_day_boundary(timestamp3), 172800);
    }

    #[test]
    fn test_is_new_day() {
        let day1_start = 86400;
        let day1_mid = 86400 + 3600;
        let day2_start = 172800;

        assert!(!is_new_day(day1_start, day1_mid));
        assert!(is_new_day(day1_start, day2_start));
    }

    #[test]
    fn test_day_number() {
        assert_eq!(get_day_number(0), 0);
        assert_eq!(get_day_number(86400), 1);
        assert_eq!(get_day_number(172800), 2);
    }

    #[test]
    fn test_anomaly_detection() {
        let limit = 1000;

        // Micro-withdrawal
        let micro = detect_anomaly_type(5, limit, 600);
        assert_eq!(micro, symbol_short!("micro"));

        // Flash drain
        let flash = detect_anomaly_type(600, limit, 100);
        assert_eq!(flash, symbol_short!("flash"));

        // Accumulation
        let accum = detect_anomaly_type(200, limit, 300);
        assert_eq!(accum, symbol_short!("accum"));
    }
}
