/// Configuration Management with Hot-Reload and Schema Validation (Issue #122)
///
/// System-wide configuration management for protocol parameters (tariff bounds,
/// gas buffer thresholds, velocity limits, and similar operational knobs) that
/// currently live as scattered constants across modules.
///
/// Soroban contracts have no runtime process to "hot-reload" in the traditional
/// server sense — every invocation already reads current storage. What this
/// module adds on top of that is the missing piece: **schema-validated writes**
/// so a bad value can never land in storage, plus a **staged rollout with a
/// veto window** so a config change takes effect automatically after a notice
/// period unless the community objects — matching the "Multi-sig veto for
/// fleet-level config changes (48h staging window)" security property already
/// documented in the README but not previously implemented. A monotonic
/// `ConfigVersion` counter and per-change events are what let off-chain
/// services (dashboard, meter simulator, webhook service) detect a change and
/// hot-reload their own local config in response — see
/// `usage-dashboard`/`meter-simulator` `config-hot-reloader` for the
/// off-chain half of this feature.
///
/// Design:
/// 1. **Schema** (`ConfigFieldSchema`) declares the type, required-ness, and
///    optional numeric bounds for a named config field. Admin-managed,
///    effective immediately (schema changes are a design-time operation).
/// 2. **Staged value changes** go through `propose_config_change` →
///    (`veto_config_change`)* → `apply_config_change`, mirroring the existing
///    `initiate_admin_transfer` / `veto_admin_transfer` / `execute_admin_transfer`
///    pattern used for admin transfers.
/// 3. **Emergency override** bypasses staging for a single field, admin-gated
///    and schema-validated, for incident response.
/// 4. **One-step rollback** restores a field's previous value from history.
///
/// All state-mutating functions in this module take an already-authenticated
/// caller `Address` as a trusted input — same convention as `velocity_limit`:
/// the caller (lib.rs) must have already verified `require_auth` and, where
/// applicable, that the caller matches the registered admin.
use soroban_sdk::{contracttype, panic_with_error, symbol_short, Address, Env, Symbol, Vec};

use crate::ContractError;

// ============================================================================
// Constants
// ============================================================================

/// Default staging/notice window before a proposed config change takes effect.
pub const DEFAULT_STAGING_WINDOW_SECONDS: u64 = 48 * 60 * 60; // 48 hours

/// Minimum staging window a proposer may request (prevents effectively-instant
/// changes from bypassing community review).
pub const MIN_STAGING_WINDOW_SECONDS: u64 = 60 * 60; // 1 hour

/// Maximum staging window a proposer may request.
pub const MAX_STAGING_WINDOW_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days

/// Maximum fields that may be batched into a single staged proposal, to keep
/// the apply step's storage writes and event count bounded.
pub const MAX_FIELDS_PER_PROPOSAL: u32 = 16;

/// Veto threshold, in basis points of total active users, above which a
/// staged change is blocked. Matches `VETO_THRESHOLD_BPS` used for admin
/// transfers (5%).
pub const CONFIG_VETO_THRESHOLD_BPS: i128 = 500;

/// Grace window after `effective_at` during which a change may still be
/// applied, mirroring the admin-transfer execution window.
pub const APPLY_GRACE_SECONDS: u64 = 24 * 60 * 60; // 24 hours

// ============================================================================
// Data Structures
// ============================================================================

/// The declared type of a config field. Used for schema validation.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigValueType {
    I128,
    U64,
    Bool,
    Symbol,
}

/// A typed config value. The variant used must match the field's declared
/// `ConfigValueType`.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigValue {
    I128(i128),
    U64(u64),
    Bool(bool),
    Symbol(Symbol),
}

/// Schema definition for a single config field.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ConfigFieldSchema {
    /// Declared type for this field; writes of a mismatched variant are rejected.
    pub value_type: ConfigValueType,

    /// Whether `get_config_value` should treat "unset" as an error condition
    /// for completeness audits (informational; does not block writes of
    /// *other* fields).
    pub required: bool,

    /// Inclusive lower bound for numeric types (I128/U64). Ignored for Bool/Symbol.
    pub min_value: Option<i128>,

    /// Inclusive upper bound for numeric types (I128/U64). Ignored for Bool/Symbol.
    pub max_value: Option<i128>,

    /// Default value returned by `get_config_value` when no value has been set yet.
    pub default_value: Option<ConfigValue>,

    /// Short human-readable description (audit trail context).
    pub description: Symbol,
}

/// A stored, live config value with its provenance.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ConfigEntry {
    pub value: ConfigValue,
    pub version: u64,
    pub updated_by: Address,
    pub updated_at: u64,
}

/// A staged, not-yet-effective batch of config changes.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PendingConfigChange {
    pub proposer: Address,
    pub fields: Vec<Symbol>,
    pub values: Vec<ConfigValue>,
    pub proposed_at: u64,
    pub effective_at: u64,
    pub veto_count: u32,
    pub is_active: bool,
}

// ============================================================================
// Events
// ============================================================================

#[contracttype]
#[derive(Clone)]
pub struct SchemaFieldRegistered {
    pub field: Symbol,
    pub value_type: ConfigValueType,
    pub required: bool,
    pub registered_by: Address,
    pub registered_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct ConfigChangeProposed {
    pub change_id: u64,
    pub proposer: Address,
    pub field_count: u32,
    pub effective_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct ConfigChangeVetoed {
    pub change_id: u64,
    pub voter: Address,
    pub veto_count: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct ConfigChangeCancelled {
    pub change_id: u64,
    pub cancelled_by: Address,
}

#[contracttype]
#[derive(Clone)]
pub struct ConfigApplied {
    pub change_id: u64,
    pub new_version: u64,
    pub field_count: u32,
    pub applied_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct ConfigFieldChanged {
    pub field: Symbol,
    pub new_version: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct ConfigEmergencyOverride {
    pub field: Symbol,
    pub new_version: u64,
    pub admin: Address,
    pub applied_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct ConfigFieldRolledBack {
    pub field: Symbol,
    pub new_version: u64,
    pub rolled_back_by: Address,
}

// ============================================================================
// Storage Keys
// ============================================================================

#[contracttype]
#[derive(Clone)]
pub enum ConfigDataKey {
    /// Field name -> schema definition.
    Schema(Symbol),
    /// Field name -> current live value.
    FieldValue(Symbol),
    /// Field name -> previous value (single-step rollback support).
    FieldHistory(Symbol),
    /// The single in-flight staged change, if any.
    PendingChange,
    /// Monotonic version counter, bumped on every applied change.
    ConfigVersion,
    /// (voter, change_id) -> already vetoed, to prevent double-voting.
    ConfigVeto(Address, u64),
}

// ============================================================================
// Schema Management
// ============================================================================

/// Register or update the schema for a config field. Admin-gated by the
/// caller (lib.rs must have already verified `require_auth`).
pub fn register_schema_field(
    env: &Env,
    admin: Address,
    field: Symbol,
    value_type: ConfigValueType,
    required: bool,
    min_value: Option<i128>,
    max_value: Option<i128>,
    default_value: Option<ConfigValue>,
    description: Symbol,
) {
    if let (Some(min), Some(max)) = (min_value, max_value) {
        if min > max {
            panic_with_error!(env, ContractError::ConfigValueOutOfRange);
        }
    }

    let schema = ConfigFieldSchema {
        value_type: value_type.clone(),
        required,
        min_value,
        max_value,
        default_value: default_value.clone(),
        description,
    };

    if let Some(ref default) = default_value {
        validate_value(env, &schema, default);
    }

    env.storage()
        .instance()
        .set(&ConfigDataKey::Schema(field.clone()), &schema);

    env.events().publish(
        (symbol_short!("schema"),),
        SchemaFieldRegistered {
            field,
            value_type,
            required,
            registered_by: admin,
            registered_at: env.ledger().timestamp(),
        },
    );
}

pub fn get_schema(env: &Env, field: &Symbol) -> Option<ConfigFieldSchema> {
    env.storage()
        .instance()
        .get(&ConfigDataKey::Schema(field.clone()))
}

/// Validate a value against a field's schema. Panics with a descriptive
/// `ContractError` on any mismatch so validation failures never fail silently.
pub fn validate_value(env: &Env, schema: &ConfigFieldSchema, value: &ConfigValue) {
    let type_matches = matches!(
        (&schema.value_type, value),
        (ConfigValueType::I128, ConfigValue::I128(_))
            | (ConfigValueType::U64, ConfigValue::U64(_))
            | (ConfigValueType::Bool, ConfigValue::Bool(_))
            | (ConfigValueType::Symbol, ConfigValue::Symbol(_))
    );

    if !type_matches {
        panic_with_error!(env, ContractError::ConfigTypeMismatch);
    }

    let numeric = match value {
        ConfigValue::I128(v) => Some(*v),
        ConfigValue::U64(v) => Some(*v as i128),
        _ => None,
    };

    if let Some(v) = numeric {
        if let Some(min) = schema.min_value {
            if v < min {
                panic_with_error!(env, ContractError::ConfigValueOutOfRange);
            }
        }
        if let Some(max) = schema.max_value {
            if v > max {
                panic_with_error!(env, ContractError::ConfigValueOutOfRange);
            }
        }
    }
}

/// Look up the schema for `field` and validate `value` against it. Panics with
/// `ConfigFieldNotInSchema` if the field has no registered schema — every
/// config write must go through a declared field, by design.
fn validate_field_write(env: &Env, field: &Symbol, value: &ConfigValue) -> ConfigFieldSchema {
    let schema = match get_schema(env, field) {
        Some(schema) => schema,
        None => panic_with_error!(env, ContractError::ConfigFieldNotInSchema),
    };
    validate_value(env, &schema, value);
    schema
}

// ============================================================================
// Reads
// ============================================================================

pub fn get_config_version(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&ConfigDataKey::ConfigVersion)
        .unwrap_or(0)
}

fn bump_config_version(env: &Env) -> u64 {
    let next = get_config_version(env) + 1;
    env.storage()
        .instance()
        .set(&ConfigDataKey::ConfigVersion, &next);
    next
}

/// Get the current live value for `field`, falling back to the schema's
/// default when unset. Returns `None` if the field has no value and no
/// schema default.
pub fn get_config_value(env: &Env, field: &Symbol) -> Option<ConfigValue> {
    if let Some(entry) = env
        .storage()
        .instance()
        .get::<_, ConfigEntry>(&ConfigDataKey::FieldValue(field.clone()))
    {
        return Some(entry.value);
    }

    get_schema(env, field).and_then(|schema| schema.default_value)
}

fn write_field(env: &Env, field: &Symbol, value: ConfigValue, updated_by: &Address, now: u64) {
    // Preserve the current value as history for one-step rollback, if present.
    if let Some(current) = env
        .storage()
        .instance()
        .get::<_, ConfigEntry>(&ConfigDataKey::FieldValue(field.clone()))
    {
        env.storage()
            .instance()
            .set(&ConfigDataKey::FieldHistory(field.clone()), &current.value);
    }

    let new_version = bump_config_version(env);

    env.storage().instance().set(
        &ConfigDataKey::FieldValue(field.clone()),
        &ConfigEntry {
            value,
            version: new_version,
            updated_by: updated_by.clone(),
            updated_at: now,
        },
    );

    env.events().publish(
        (symbol_short!("cfgfield"),),
        ConfigFieldChanged {
            field: field.clone(),
            new_version,
        },
    );
}

// ============================================================================
// Staged Change Lifecycle
// ============================================================================

/// Propose a batch of config changes. Every field/value pair is validated
/// against its schema immediately (fail fast) even though it is re-validated
/// again at apply time in case the schema was tightened during staging.
///
/// Returns the `change_id` (the proposal's `proposed_at` timestamp), used to
/// reference this change in `veto_config_change`.
pub fn propose_config_change(
    env: &Env,
    proposer: Address,
    fields: Vec<Symbol>,
    values: Vec<ConfigValue>,
    staging_window_override: Option<u64>,
) -> u64 {
    if fields.len() != values.len() {
        panic_with_error!(env, ContractError::ConfigTypeMismatch);
    }
    if fields.is_empty() || fields.len() > MAX_FIELDS_PER_PROPOSAL {
        panic_with_error!(env, ContractError::ConfigTooManyFields);
    }

    if let Some(existing) = env
        .storage()
        .instance()
        .get::<_, PendingConfigChange>(&ConfigDataKey::PendingChange)
    {
        let now = env.ledger().timestamp();
        if existing.is_active && now < existing.effective_at + APPLY_GRACE_SECONDS {
            panic_with_error!(env, ContractError::ConfigChangeAlreadyPending);
        }
    }

    // Fail fast: validate every field against its current schema.
    for i in 0..fields.len() {
        validate_field_write(env, &fields.get(i).unwrap(), &values.get(i).unwrap());
    }

    let window = staging_window_override.unwrap_or(DEFAULT_STAGING_WINDOW_SECONDS);
    if window < MIN_STAGING_WINDOW_SECONDS || window > MAX_STAGING_WINDOW_SECONDS {
        panic_with_error!(env, ContractError::ConfigValueOutOfRange);
    }

    let now = env.ledger().timestamp();
    let effective_at = now + window;

    let change = PendingConfigChange {
        proposer: proposer.clone(),
        fields: fields.clone(),
        values,
        proposed_at: now,
        effective_at,
        veto_count: 0,
        is_active: true,
    };

    env.storage()
        .instance()
        .set(&ConfigDataKey::PendingChange, &change);

    env.events().publish(
        (symbol_short!("proposed"),),
        ConfigChangeProposed {
            change_id: now,
            proposer,
            field_count: fields.len() as u32,
            effective_at,
        },
    );

    now
}

/// Cast a veto against the currently pending change. `total_active_users` is
/// supplied by the caller (read from the main contract's own user registry)
/// so this module stays decoupled from the crate-root `DataKey` enum.
pub fn veto_config_change(env: &Env, voter: Address, change_id: u64) -> u32 {
    let mut change: PendingConfigChange = env
        .storage()
        .instance()
        .get(&ConfigDataKey::PendingChange)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::ConfigChangeNotPending));

    if !change.is_active || change.proposed_at != change_id {
        panic_with_error!(env, ContractError::ConfigChangeNotPending);
    }
    if env.ledger().timestamp() >= change.effective_at {
        panic_with_error!(env, ContractError::ConfigStagingWindowNotElapsed);
    }

    let veto_key = ConfigDataKey::ConfigVeto(voter.clone(), change_id);
    let already_vetoed: bool = env.storage().instance().get(&veto_key).unwrap_or(false);
    if already_vetoed {
        panic_with_error!(env, ContractError::ConfigChangeAlreadyPending);
    }

    env.storage().instance().set(&veto_key, &true);

    change.veto_count += 1;
    env.storage()
        .instance()
        .set(&ConfigDataKey::PendingChange, &change);

    env.events().publish(
        (symbol_short!("cfgveto"),),
        ConfigChangeVetoed {
            change_id,
            voter,
            veto_count: change.veto_count,
        },
    );

    change.veto_count
}

/// Cancel a pending change before it takes effect. Caller (lib.rs) verifies
/// this is either the original proposer or the admin.
pub fn cancel_pending_config_change(env: &Env, canceller: Address) {
    let mut change: PendingConfigChange = env
        .storage()
        .instance()
        .get(&ConfigDataKey::PendingChange)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::ConfigChangeNotPending));

    if !change.is_active {
        panic_with_error!(env, ContractError::ConfigChangeNotPending);
    }

    change.is_active = false;
    env.storage()
        .instance()
        .set(&ConfigDataKey::PendingChange, &change);

    env.events().publish(
        (symbol_short!("cfgcancl"),),
        ConfigChangeCancelled {
            change_id: change.proposed_at,
            cancelled_by: canceller,
        },
    );
}

/// Apply a pending change once its staging window has elapsed, provided the
/// veto threshold was not reached. Re-validates every field against the
/// *current* schema before writing anything (defensive: schema may have been
/// tightened during staging).
pub fn apply_config_change(env: &Env, total_active_users: u32) -> u64 {
    let change: PendingConfigChange = env
        .storage()
        .instance()
        .get(&ConfigDataKey::PendingChange)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::ConfigChangeNotPending));

    if !change.is_active {
        panic_with_error!(env, ContractError::ConfigChangeNotPending);
    }

    let now = env.ledger().timestamp();
    if now < change.effective_at {
        panic_with_error!(env, ContractError::ConfigStagingWindowNotElapsed);
    }
    if now > change.effective_at + APPLY_GRACE_SECONDS {
        panic_with_error!(env, ContractError::ConfigChangeNotPending);
    }

    let veto_threshold =
        (total_active_users as i128 * CONFIG_VETO_THRESHOLD_BPS / 10_000) as u32;
    if change.veto_count >= veto_threshold.max(1) && total_active_users > 0 {
        panic_with_error!(env, ContractError::ConfigVetoThresholdReached);
    }

    // Re-validate every field against the current schema before writing.
    for i in 0..change.fields.len() {
        validate_field_write(env, &change.fields.get(i).unwrap(), &change.values.get(i).unwrap());
    }

    let mut new_version = get_config_version(env);
    for i in 0..change.fields.len() {
        write_field(
            env,
            &change.fields.get(i).unwrap(),
            change.values.get(i).unwrap(),
            &change.proposer,
            now,
        );
        new_version = get_config_version(env);
    }

    env.storage().instance().remove(&ConfigDataKey::PendingChange);

    env.events().publish(
        (symbol_short!("applied"),),
        ConfigApplied {
            change_id: change.proposed_at,
            new_version,
            field_count: change.fields.len() as u32,
            applied_at: now,
        },
    );

    new_version
}

// ============================================================================
// Emergency Override & Rollback
// ============================================================================

/// Bypass staging entirely for a single field. Admin-gated by the caller;
/// still schema-validated. Intended for incident response, not routine
/// tuning — hence the distinct, loudly-named event for off-chain alerting.
pub fn emergency_set_config_value(env: &Env, admin: Address, field: Symbol, value: ConfigValue) {
    validate_field_write(env, &field, &value);

    let now = env.ledger().timestamp();
    write_field(env, &field, value, &admin, now);

    env.events().publish(
        (symbol_short!("cfgemrg"),),
        ConfigEmergencyOverride {
            field,
            new_version: get_config_version(env),
            admin,
            applied_at: now,
        },
    );
}

/// Restore a field to its previous value, one step back. Admin-gated by the
/// caller. Panics with `ConfigNoPreviousValue` if there is no history entry.
pub fn rollback_config_field(env: &Env, admin: Address, field: Symbol) {
    let previous: ConfigValue = env
        .storage()
        .instance()
        .get(&ConfigDataKey::FieldHistory(field.clone()))
        .unwrap_or_else(|| panic_with_error!(env, ContractError::ConfigNoPreviousValue));

    // Re-validate against the current schema — the schema may have changed
    // since the previous value was live.
    validate_field_write(env, &field, &previous);

    let now = env.ledger().timestamp();
    write_field(env, &field, previous, &admin, now);

    // Clear history so a second rollback doesn't bounce back to the value we
    // just replaced.
    env.storage()
        .instance()
        .remove(&ConfigDataKey::FieldHistory(field.clone()));

    env.events().publish(
        (symbol_short!("cfgrlbck"),),
        ConfigFieldRolledBack {
            field,
            new_version: get_config_version(env),
            rolled_back_by: admin,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i128_schema(min: Option<i128>, max: Option<i128>) -> ConfigFieldSchema {
        ConfigFieldSchema {
            value_type: ConfigValueType::I128,
            required: true,
            min_value: min,
            max_value: max,
            default_value: None,
            description: symbol_short!("testdesc"),
        }
    }

    #[test]
    fn accepts_value_within_bounds() {
        let env = Env::default();
        let schema = i128_schema(Some(0), Some(100));
        validate_value(&env, &schema, &ConfigValue::I128(50));
    }

    #[test]
    #[should_panic]
    fn rejects_value_below_min() {
        let env = Env::default();
        let schema = i128_schema(Some(10), Some(100));
        validate_value(&env, &schema, &ConfigValue::I128(5));
    }

    #[test]
    #[should_panic]
    fn rejects_value_above_max() {
        let env = Env::default();
        let schema = i128_schema(Some(0), Some(100));
        validate_value(&env, &schema, &ConfigValue::I128(101));
    }

    #[test]
    #[should_panic]
    fn rejects_type_mismatch() {
        let env = Env::default();
        let schema = i128_schema(None, None);
        validate_value(&env, &schema, &ConfigValue::Bool(true));
    }

    #[test]
    fn u64_values_are_range_checked_via_i128_cast() {
        let env = Env::default();
        let schema = ConfigFieldSchema {
            value_type: ConfigValueType::U64,
            required: false,
            min_value: Some(0),
            max_value: Some(1_000),
            default_value: None,
            description: symbol_short!("testdesc"),
        };
        validate_value(&env, &schema, &ConfigValue::U64(500));
    }
}
