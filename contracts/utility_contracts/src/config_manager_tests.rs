#![cfg(test)]

use crate::config_manager::ConfigValue;
use crate::*;
use soroban_sdk::{symbol_short, vec, Address, Env};

fn setup() -> (Env, UtilityContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn register_gas_buffer_schema(client: &UtilityContractClient, admin: &Address) {
    client.register_config_schema(
        admin,
        &symbol_short!("gasbuf"),
        &config_manager::ConfigValueType::I128,
        &true,
        &Some(100i128),
        &Some(10_000i128),
        &Some(ConfigValue::I128(200)),
        &symbol_short!("gasbuffer"),
    );
}

#[test]
fn schema_registration_and_default_value_read() {
    let (_env, client, admin) = setup();
    register_gas_buffer_schema(&client, &admin);

    let schema = client.get_config_schema(&symbol_short!("gasbuf")).unwrap();
    assert!(schema.required);
    assert_eq!(schema.min_value, Some(100));
    assert_eq!(schema.max_value, Some(10_000));

    // No value has been staged/applied yet -> falls back to schema default.
    let value = client.get_config_value(&symbol_short!("gasbuf")).unwrap();
    assert_eq!(value, ConfigValue::I128(200));

    assert_eq!(client.get_config_version(), 0);
}

#[test]
fn staged_change_applies_after_window_and_bumps_version() {
    let (env, client, admin) = setup();
    register_gas_buffer_schema(&client, &admin);

    let change_id = client.propose_config_change(
        &admin,
        &vec![&env, symbol_short!("gasbuf")],
        &vec![&env, ConfigValue::I128(500)],
        &None,
    );
    assert!(change_id > 0);

    // Value hasn't changed yet - still the schema default.
    assert_eq!(
        client.get_config_value(&symbol_short!("gasbuf")),
        Some(ConfigValue::I128(200))
    );

    // Fast-forward past the default 48h staging window.
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 48 * 60 * 60 + 1);

    let new_version = client.apply_config_change();
    assert_eq!(new_version, 1);
    assert_eq!(client.get_config_version(), 1);
    assert_eq!(
        client.get_config_value(&symbol_short!("gasbuf")),
        Some(ConfigValue::I128(500))
    );
}

#[test]
#[should_panic]
fn applying_before_staging_window_elapses_panics() {
    let (env, client, admin) = setup();
    register_gas_buffer_schema(&client, &admin);

    client.propose_config_change(
        &admin,
        &vec![&env, symbol_short!("gasbuf")],
        &vec![&env, ConfigValue::I128(500)],
        &None,
    );

    // No time advance - staging window has not elapsed.
    client.apply_config_change();
}

#[test]
#[should_panic]
fn proposal_with_out_of_range_value_panics_immediately() {
    let (env, client, admin) = setup();
    register_gas_buffer_schema(&client, &admin);

    // 50_000 exceeds the schema's max_value of 10_000.
    client.propose_config_change(
        &admin,
        &vec![&env, symbol_short!("gasbuf")],
        &vec![&env, ConfigValue::I128(50_000)],
        &None,
    );
}

#[test]
fn veto_blocks_apply_when_threshold_reached() {
    let (env, client, admin) = setup();
    register_gas_buffer_schema(&client, &admin);

    // Register a handful of active users so the veto threshold (5%) is reachable.
    for _ in 0..5 {
        let user = Address::generate(&env);
        client.register_active_user(&user);
    }

    let change_id = client.propose_config_change(
        &admin,
        &vec![&env, symbol_short!("gasbuf")],
        &vec![&env, ConfigValue::I128(500)],
        &None,
    );

    let voter = Address::generate(&env);
    client.veto_config_change(&voter, &change_id);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 48 * 60 * 60 + 1);

    let result = client.try_apply_config_change();
    assert!(result.is_err());

    // Value never changed.
    assert_eq!(
        client.get_config_value(&symbol_short!("gasbuf")),
        Some(ConfigValue::I128(200))
    );
}

#[test]
fn emergency_override_bypasses_staging_and_rollback_restores_previous() {
    let (env, client, admin) = setup();
    register_gas_buffer_schema(&client, &admin);

    client.emergency_set_config_value(&admin, &symbol_short!("gasbuf"), &ConfigValue::I128(9_000));
    assert_eq!(
        client.get_config_value(&symbol_short!("gasbuf")),
        Some(ConfigValue::I128(9_000))
    );
    assert_eq!(client.get_config_version(), 1);

    client.emergency_set_config_value(&admin, &symbol_short!("gasbuf"), &ConfigValue::I128(1_000));
    assert_eq!(
        client.get_config_value(&symbol_short!("gasbuf")),
        Some(ConfigValue::I128(1_000))
    );

    client.rollback_config_field(&admin, &symbol_short!("gasbuf"));
    assert_eq!(
        client.get_config_value(&symbol_short!("gasbuf")),
        Some(ConfigValue::I128(9_000))
    );
    assert_eq!(client.get_config_version(), 3);

    let _ = env; // silence unused warning when auth mocking isn't otherwise referenced
}

#[test]
#[should_panic]
fn emergency_override_with_bad_type_panics() {
    let (_env, client, admin) = setup();
    register_gas_buffer_schema(&client, &admin);

    // gasbuf is declared I128, not Bool.
    client.emergency_set_config_value(&admin, &symbol_short!("gasbuf"), &ConfigValue::Bool(true));
}

#[test]
#[should_panic]
fn writing_a_field_with_no_schema_panics() {
    let (env, client, admin) = setup();

    client.propose_config_change(
        &admin,
        &vec![&env, symbol_short!("nosuch")],
        &vec![&env, ConfigValue::I128(1)],
        &None,
    );
}

#[test]
#[should_panic]
fn non_admin_cannot_register_schema() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);

    client.register_config_schema(
        &attacker,
        &symbol_short!("gasbuf"),
        &config_manager::ConfigValueType::I128,
        &true,
        &Some(100i128),
        &Some(10_000i128),
        &Some(ConfigValue::I128(200)),
        &symbol_short!("gasbuffer"),
    );
}
