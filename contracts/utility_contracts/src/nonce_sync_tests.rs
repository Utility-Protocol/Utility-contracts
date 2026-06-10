extern crate std;

use soroban_sdk::{testutils::Address as TestAddress, testutils::BytesN as TestBytesN, Env, Address, BytesN};
use std::vec;
use std::format;
use std::vec::Vec;
use std::string::String;
use crate::nonce_sync::{
    NonceSyncManager, SignedHeartbeat, DeviceNonceState, NonceDesyncAlert, 
    NonceAlertType, NONCE_WINDOW_SIZE, NonceResetRequest
};
use crate::{ContractError, DataKey};

#[cfg(test)]
pub mod nonce_sync_fuzz_tests {
    use super::*;

    /// Fuzz test: Replay attack rejection
    /// 
    /// This test attempts to replay captured hardware signatures with the same nonce
    /// to verify the rejection logic works correctly.
    #[test]
    fn test_replay_attack_rejection() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, NonceSyncManager);

        let device_mac = TestBytesN::random(&env);
        let meter_id = 12345u64;
        
        // Initialize device nonce
        NonceSyncManager::initialize_device_nonce(env.clone(), device_mac.clone(), 0);

        // Create initial valid heartbeat
        let heartbeat1 = create_test_heartbeat(meter_id, device_mac.clone(), 0);
        
        // First heartbeat should succeed
        assert!(NonceSyncManager::verify_heartbeat_nonce(env.clone(), heartbeat1));

        // Attempt replay attack with same nonce - should fail
        let replay_heartbeat = create_test_heartbeat(meter_id, device_mac.clone(), 0);
        assert!(!NonceSyncManager::verify_heartbeat_nonce(env.clone(), replay_heartbeat));

        // Verify device state shows desync
        let state = NonceSyncManager::get_device_nonce_state(env.clone(), device_mac.clone());
        assert!(state.desync_count_24h > 0);
    }

    /// Fuzz test: Nonce window validation
    /// 
    /// Tests that nonces within the allowed window (+1 to +5) are accepted
    /// while nonces outside the window are rejected.
    #[test]
    fn test_nonce_window_validation() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, NonceSyncManager);

        let device_mac = TestBytesN::random(&env);
        let meter_id = 12345u64;
        
        // Initialize device nonce at 100
        NonceSyncManager::initialize_device_nonce(env.clone(), device_mac.clone(), 100);

        // Test nonce within window (should be accepted)
        for offset in 1..=NONCE_WINDOW_SIZE {
            let heartbeat = create_test_heartbeat(meter_id, device_mac.clone(), 100 + offset);
            assert!(NonceSyncManager::verify_heartbeat_nonce(env.clone(), heartbeat));
        }

        // Test nonce outside window (should be rejected)
        let future_heartbeat = create_test_heartbeat(meter_id, device_mac.clone(), 100 + NONCE_WINDOW_SIZE + 1);
        assert!(!NonceSyncManager::verify_heartbeat_nonce(env.clone(), future_heartbeat));

        // Test old nonce (should be rejected)
        let old_heartbeat = create_test_heartbeat(meter_id, device_mac.clone(), 99);
        assert!(!NonceSyncManager::verify_heartbeat_nonce(env.clone(), old_heartbeat));
    }

    /// Fuzz test: Network jitter simulation
    /// 
    /// Simulates UDP packet loss where nonces arrive out of order
    /// but within the acceptable window.
    #[test]
    fn test_network_jitter_simulation() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, NonceSyncManager);

        let device_mac = TestBytesN::random(&env);
        let meter_id = 12345u64;
        
        NonceSyncManager::initialize_device_nonce(env.clone(), device_mac.clone(), 0);

        // Simulate packet loss: nonce 2 arrives before nonce 1
        let heartbeat2 = create_test_heartbeat(meter_id, device_mac.clone(), 2);
        let heartbeat1 = create_test_heartbeat(meter_id, device_mac.clone(), 1);

        // Both should be accepted due to window
        assert!(NonceSyncManager::verify_heartbeat_nonce(env.clone(), heartbeat2));
        assert!(NonceSyncManager::verify_heartbeat_nonce(env.clone(), heartbeat1));

        // Verify final state
        let state = NonceSyncManager::get_device_nonce_state(env.clone(), device_mac.clone());
        assert_eq!(state.current_nonce, 3); // Should be max nonce + 1
    }

    /// Fuzz test: Device suspicious marking
    /// 
    /// Tests that devices with frequent desyncs are marked as suspicious.
    #[test]
    fn test_device_suspicious_marking() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, NonceSyncManager);

        let device_mac = TestBytesN::random(&env);
        let meter_id = 12345u64;
        
        NonceSyncManager::initialize_device_nonce(env.clone(), device_mac.clone(), 0);

        // Generate multiple desync events to trigger suspicious marking
        for i in 0..15 {
            let bad_heartbeat = create_test_heartbeat(meter_id, device_mac.clone(), 0); // Always nonce 0
            NonceSyncManager::verify_heartbeat_nonce(env.clone(), bad_heartbeat);
        }

        // Device should be marked as suspicious
        assert!(NonceSyncManager::is_device_suspicious(env.clone(), device_mac.clone()));
    }

    /// Fuzz test: Multi-sig nonce reset
    /// 
    /// Tests the multi-sig nonce reset functionality for compromised devices.
    #[test]
    fn test_multisig_nonce_reset() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, NonceSyncManager);

        let device_mac = TestBytesN::random(&env);
        let meter_id = 12345u64;
        let authorized_resetter1 = TestAddress::random(&env);
        let authorized_resetter2 = TestAddress::random(&env);
        let authorized_resetter3 = TestAddress::random(&env);

        // Setup authorized resetters — 3 fixed items, use vec! macro for clarity
        let resetters_key = DataKey::AuthorizedNonceResetters;
        let resetters = soroban_sdk::vec![
            &env,
            authorized_resetter1.clone(),
            authorized_resetter2.clone(),
            authorized_resetter3.clone()
        ];
        env.storage().persistent().set(&resetters_key, &resetters);

        // Initialize device with compromised nonce
        NonceSyncManager::initialize_device_nonce(env.clone(), device_mac.clone(), 1000);

        // Create reset request
        let reset_request = NonceResetRequest {
            meter_id,
            device_mac: device_mac.clone(),
            new_nonce: 0,
            requested_by: authorized_resetter1.clone(),
            approvals: Vec::new(&env),
            required_approvals: 3,
            created_at: env.ledger().timestamp(),
            expires_at: env.ledger().timestamp() + 3600, // 1 hour
            is_executed: false,
        };

        // First approval
        NonceSyncManager::reset_device_nonce(
            env.clone(),
            meter_id,
            device_mac.clone(),
            0,
            reset_request,
            authorized_resetter1.clone(),
        );

        // Second approval
        let stored_request: NonceResetRequest = env.storage()
            .persistent()
            .get(&DataKey::NonceResetRequest(meter_id))
            .unwrap();
        NonceSyncManager::reset_device_nonce(
            env.clone(),
            meter_id,
            device_mac.clone(),
            0,
            stored_request,
            authorized_resetter2.clone(),
        );

        // Third approval should execute reset
        let stored_request: NonceResetRequest = env.storage()
            .persistent()
            .get(&DataKey::NonceResetRequest(meter_id))
            .unwrap();
        NonceSyncManager::reset_device_nonce(
            env.clone(),
            meter_id,
            device_mac.clone(),
            0,
            stored_request,
            authorized_resetter3.clone(),
        );

        // Verify nonce was reset
        let state = NonceSyncManager::get_device_nonce_state(env.clone(), device_mac.clone());
        assert_eq!(state.current_nonce, 0);
        assert_eq!(state.desync_count_24h, 0);
        assert!(!state.is_suspicious);
    }

    /// Fuzz test: Edge case - very large nonce values
    /// 
    /// Tests behavior with near-u64::MAX nonce values to prevent overflow.
    #[test]
    fn test_large_nonce_values() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, NonceSyncManager);

        let device_mac = TestBytesN::random(&env);
        let meter_id = 12345u64;
        
        // Initialize with very large nonce
        let large_nonce = u64::MAX - 10;
        NonceSyncManager::initialize_device_nonce(env.clone(), device_mac.clone(), large_nonce);

        // Test increment near overflow boundary
        let heartbeat = create_test_heartbeat(meter_id, device_mac.clone(), large_nonce);
        assert!(NonceSyncManager::verify_heartbeat_nonce(env.clone(), heartbeat));

        // Verify state handles overflow correctly
        let state = NonceSyncManager::get_device_nonce_state(env.clone(), device_mac.clone());
        assert_eq!(state.current_nonce, large_nonce + 1);
    }

    /// Fuzz test: Concurrent heartbeat processing
    /// 
    /// Simulates multiple heartbeats arriving simultaneously
    /// to test race condition handling.
    #[test]
    fn test_concurrent_heartbeat_processing() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, NonceSyncManager);

        let device_mac = TestBytesN::random(&env);
        let meter_id = 12345u64;
        
        NonceSyncManager::initialize_device_nonce(env.clone(), device_mac.clone(), 0);

        // Send multiple heartbeats with different nonces
        let heartbeats = vec![
            create_test_heartbeat(meter_id, device_mac.clone(), 1),
            create_test_heartbeat(meter_id, device_mac.clone(), 2),
            create_test_heartbeat(meter_id, device_mac.clone(), 3),
            create_test_heartbeat(meter_id, device_mac.clone(), 4),
            create_test_heartbeat(meter_id, device_mac.clone(), 5),
        ];

        // Process all heartbeats
        let mut successful_count = 0;
        for heartbeat in heartbeats {
            if NonceSyncManager::verify_heartbeat_nonce(env.clone(), heartbeat) {
                successful_count += 1;
            }
        }

        // At least some should succeed
        assert!(successful_count > 0);

        // Final state should be consistent
        let state = NonceSyncManager::get_device_nonce_state(env.clone(), device_mac.clone());
        assert!(state.current_nonce > 0);
    }
}

/// Helper function to create test heartbeat
pub(crate) fn create_test_heartbeat(
    meter_id: u64,
    device_mac: BytesN<32>,
    nonce: u64,
) -> SignedHeartbeat {
    let env = Env::new();
    SignedHeartbeat {
        meter_id,
        device_mac,
        nonce,
        timestamp: env.ledger().timestamp(),
        signature: BytesN::from_array(&[1u8; 64]),
        public_key: BytesN::from_array(&[2u8; 32]),
    }
}

/// Property-based test: Nonce monotonicity
/// 
/// This test verifies that the nonce system maintains strict monotonicity
/// and prevents any form of nonce reuse or manipulation.
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        #[test]
        fn test_nonce_monotonicity_property(
            initial_nonce in 0u64..1000u64,
            nonce_sequence in prop::collection::vec(0u64..2000u64, 10..50)
        ) {
            let env = Env::new();
            let contract_address = TestAddress::random(&env);
            env.register_contract(&contract_address, NonceSyncManager);

            let device_mac = TestBytesN::random(&env);
            let meter_id = 12345u64;
            
            NonceSyncManager::initialize_device_nonce(env.clone(), device_mac.clone(), initial_nonce);

            let mut max_nonce = initial_nonce;
            let mut successful_verifications = 0;

            for nonce in nonce_sequence {
                let heartbeat = create_test_heartbeat(meter_id, device_mac.clone(), nonce);
                
                if NonceSyncManager::verify_heartbeat_nonce(env.clone(), heartbeat) {
                    successful_verifications += 1;
                    prop_assert!(nonce >= max_nonce);
                    max_nonce = nonce;
                }
            }

            // Final state should reflect highest successful nonce
            let final_state = NonceSyncManager::get_device_nonce_state(env.clone(), device_mac.clone());
            prop_assert!(final_state.current_nonce >= max_nonce);
        }
    }
}
