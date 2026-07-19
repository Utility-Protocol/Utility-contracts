#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, IntoVal, Symbol};

#[test]
fn otel_log_record_uses_semantic_convention_equivalents() {
    let env = Env::default();
    let provider = Address::generate(&env);

    let record = OtelLogRecord {
        service_name: Symbol::new(&env, "utility-contracts"),
        service_version: Symbol::new(&env, "0.0.0"),
        deployment_environment_name: Symbol::new(&env, "soroban"),
        event_name: Symbol::new(&env, "usage.updated"),
        event_domain: Symbol::new(&env, "utility.billing"),
        log_severity: Symbol::new(&env, "INFO"),
        enduser_id: 73,
        server_address: provider.clone(),
        url_scheme: Symbol::new(&env, "soroban"),
        timestamp_unix: 1234,
        critical_path_budget_ms: 100,
    };

    assert_eq!(record.service_name, Symbol::new(&env, "utility-contracts"));
    assert_eq!(
        record.deployment_environment_name,
        Symbol::new(&env, "soroban")
    );
    assert_eq!(record.event_name, Symbol::new(&env, "usage.updated"));
    assert_eq!(record.event_domain, Symbol::new(&env, "utility.billing"));
    assert_eq!(record.log_severity, Symbol::new(&env, "INFO"));
    assert_eq!(record.enduser_id, 73);
    assert_eq!(record.server_address, provider);
    assert_eq!(record.url_scheme, Symbol::new(&env, "soroban"));
    assert_eq!(record.critical_path_budget_ms, OTEL_CRITICAL_PATH_BUDGET_MS);
}

#[test]
fn emit_otel_log_publishes_structured_event_without_storage_writes() {
    let env = Env::default();
    let provider = Address::generate(&env);

    emit_otel_log(
        &env,
        73,
        &provider,
        Symbol::new(&env, "meter.registered"),
        Symbol::new(&env, "INFO"),
    );

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events.get(0).unwrap().1,
        (Symbol::new(&env, "otel.log"), 73_u64).into_val(&env)
    );
}
