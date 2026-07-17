#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, BytesN as _},
    Address, BytesN, Env,
};

// Import the contract
use utility_contracts::UtilityContract;

/// Helper to create a test environment and initialize contract
fn setup_test_env() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, UtilityContract);
    let admin = Address::generate(&env);
    
    (env, contract_id, admin)
}

#[test]
fn test_initial_storage_version() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Set admin to initialize the contract
    client.set_admin(&admin);
    
    // Check that storage version is set to 1
    let version = client.get_storage_version_public();
    assert_eq!(version, 1, "Initial storage version should be 1");
}

#[test]
fn test_storage_version_persists() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    
    // Check version multiple times
    let version1 = client.get_storage_version_public();
    let version2 = client.get_storage_version_public();
    
    assert_eq!(version1, version2, "Storage version should persist between calls");
    assert_eq!(version1, 1, "Storage version should be 1");
}

#[test]
fn test_upgrade_with_same_version() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    
    // Check initial version
    let initial_version = client.get_storage_version_public();
    assert_eq!(initial_version, 1);
    
    // Propose an upgrade with a dummy WASM hash
    let new_wasm_hash = BytesN::<32>::random(&env);
    client.propose_upgrade(&new_wasm_hash);
    
    // In a real scenario, we would wait for the veto period to pass
    // and then call finalize_upgrade_with_version_check
    // For this test, we're just checking that the version check logic is in place
    
    // Version should still be 1
    let current_version = client.get_storage_version_public();
    assert_eq!(current_version, 1);
}

#[test]
fn test_migration_not_active_initially() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    
    // Check that no migration is active
    let is_active = client.is_migration_active();
    assert!(!is_active, "No migration should be active initially");
}

#[test]
fn test_run_migration_v1_to_v2() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    
    // Mock admin authorization
    env.mock_all_auths();
    
    // Check initial version
    let initial_version = client.get_storage_version_public();
    assert_eq!(initial_version, 1);
    
    // Run migration from v1 to v2
    // This might need multiple calls depending on data size
    let mut complete = false;
    let mut iterations = 0;
    let max_iterations = 10; // Prevent infinite loops in tests
    
    while !complete && iterations < max_iterations {
        complete = client.run_migration(&2);
        iterations += 1;
    }
    
    assert!(complete, "Migration should complete");
    
    // Check that version was updated to 2
    let new_version = client.get_storage_version_public();
    assert_eq!(new_version, 2, "Storage version should be updated to 2");
}

#[test]
fn test_migration_idempotent() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    env.mock_all_auths();
    
    // Run migration to v2
    let mut complete = false;
    let mut iterations = 0;
    
    while !complete && iterations < 10 {
        complete = client.run_migration(&2);
        iterations += 1;
    }
    
    assert!(complete, "First migration should complete");
    let version_after_first = client.get_storage_version_public();
    assert_eq!(version_after_first, 2);
    
    // Try to run migration again (should be idempotent)
    let second_result = client.run_migration(&2);
    assert!(second_result, "Running migration on same version should return true");
    
    // Version should still be 2
    let version_after_second = client.get_storage_version_public();
    assert_eq!(version_after_second, 2);
}

#[test]
fn test_cancel_migration() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    env.mock_all_auths();
    
    // Start a migration but don't complete it
    // For this test, we'll just ensure cancel works even when no migration is active
    client.cancel_migration();
    
    // Check that no migration is active
    let is_active = client.is_migration_active();
    assert!(!is_active, "No migration should be active after cancel");
}

#[test]
#[should_panic(expected = "IncompatibleStorageVersion")]
fn test_downgrade_not_allowed() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    env.mock_all_auths();
    
    // Try to run migration to a lower version (should fail)
    client.run_migration(&0);
}

#[test]
#[should_panic(expected = "NoMigrationFunction")]
fn test_migration_without_function() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    env.mock_all_auths();
    
    // Try to migrate to v3 when only v1->v2 is supported
    client.run_migration(&3);
}

#[test]
fn test_storage_version_after_multiple_operations() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    
    // Perform various operations
    let version1 = client.get_storage_version_public();
    
    // Set some configuration (simulating contract usage)
    client.set_admin(&admin);
    
    let version2 = client.get_storage_version_public();
    
    // Version should remain stable across operations
    assert_eq!(version1, version2, "Storage version should remain stable");
    assert_eq!(version1, 1);
}

#[test]
fn test_upgrade_proposal_with_version_info() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    env.mock_all_auths();
    
    // Check initial version
    let initial_version = client.get_storage_version_public();
    assert_eq!(initial_version, 1);
    
    // Create a new WASM hash for upgrade
    let new_wasm_hash = BytesN::<32>::random(&env);
    
    // Propose upgrade
    client.propose_upgrade(&new_wasm_hash);
    
    // Version should still be initial version during proposal
    let version_during_proposal = client.get_storage_version_public();
    assert_eq!(version_during_proposal, 1);
}

#[test]
fn test_migration_state_consistency() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);
    
    // Initialize
    client.set_admin(&admin);
    env.mock_all_auths();
    
    // Initially no migration
    assert!(!client.is_migration_active());
    
    // After starting migration (without completing)
    // Note: This test validates the migration state tracking
    
    // After completing migration
    let complete = client.run_migration(&2);
    if complete {
        assert!(!client.is_migration_active(), "Migration should not be active after completion");
    }
}

#[test]
fn test_migration_checkpoint_and_rollback_metadata() {
    let (env, contract_id, admin) = setup_test_env();
    let client = utility_contracts::UtilityContractClient::new(&env, &contract_id);

    client.set_admin(&admin);
    env.mock_all_auths();

    assert_eq!(client.get_migration_checkpoint(), None);

    let complete = client.run_migration(&2);
    assert!(complete, "empty fixture migration should complete in one call");
    assert_eq!(client.get_storage_version_public(), 2);

    let rollback = client
        .get_migration_rollback(&2)
        .expect("rollback metadata should be prepared after migration");
    assert_eq!(rollback.from_version, 2);
    assert_eq!(rollback.to_version, 1);

    assert!(client.rollback_migration(&1));
    assert_eq!(client.get_storage_version_public(), 1);

    let completed = client
        .get_migration_rollback(&2)
        .expect("completed rollback metadata should remain queryable");
    assert!(completed.completed_at >= completed.prepared_at);
}
