#![cfg(test)]

extern crate std;

use crate::*;
use soroban_sdk::{
    testutils::{Address as TestAddress, Ledger as TestLedger},
    Address, Env,
};
use std::vec;
use std::format;

#[cfg(test)]
mod pause_resume_fuzz {
    use super::*;

    #[test]
    fn test_rapid_pause_resume_cycles() {
        let env = Env::default();
        let contract_id = env.register_contract(None, UtilityContract);
        let client = UtilityContractClient::new(&env, &contract_id);

        let provider = TestAddress::generate(&env);
        let stream_id = 100u64;
        let flow_rate = 1000i128;
        let initial_balance = 10000000i128; // Large balance for many cycles

        // Create stream
        client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

        // Perform rapid pause/resume cycles
        for cycle in 0..100 {
            let timestamp = cycle * 10; // 10 seconds per cycle
            env.ledger().set_timestamp(timestamp);

            // Pause
            client.pause_stream(&stream_id);
            
            // Verify paused state
            let paused_flow = client.get_continuous_flow(&stream_id).unwrap();
            assert_eq!(paused_flow.status, StreamStatus::Paused);
            assert_eq!(paused_flow.paused_at, timestamp);

            // Resume immediately with varying flow rates
            let new_flow_rate = 1000 + (cycle as i128 * 10);
            env.ledger().set_timestamp(timestamp + 1); // 1 second later
            client.resume_stream(&stream_id, &new_flow_rate);

            // Verify resumed state
            let resumed_flow = client.get_continuous_flow(&stream_id).unwrap();
            assert_eq!(resumed_flow.status, StreamStatus::Active);
            assert_eq!(resumed_flow.flow_rate_per_second, new_flow_rate);
            assert_eq!(resumed_flow.paused_at, 0);
        }

        // Verify final balance is reasonable (should be > 0)
        let final_balance = client.get_continuous_balance(&stream_id).unwrap();
        assert!(final_balance > 0);
        assert!(final_balance < initial_balance); // Should have decreased
    }

    #[test]
    fn test_concurrent_pause_attempts() {
        let env = Env::default();
        let contract_id = env.register_contract(None, UtilityContract);
        let client = UtilityContractClient::new(&env, &contract_id);

        let provider = TestAddress::generate(&env);
        let stream_id = 101u64;
        let flow_rate = 1000i128;
        let initial_balance = 1000000i128;

        // Create stream
        client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

        env.ledger().set_timestamp(100);

        // Pause the stream
        client.pause_stream(&stream_id);

        // Try to pause again (should fail)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.pause_stream(&stream_id);
        }));
        assert!(result.is_err());

        // Verify stream is still paused
        let flow = client.get_continuous_flow(&stream_id).unwrap();
        assert_eq!(flow.status, StreamStatus::Paused);
        assert_eq!(flow.paused_at, 100);
    }

    #[test]
    fn test_concurrent_resume_attempts() {
        let env = Env::default();
        let contract_id = env.register_contract(None, UtilityContract);
        let client = UtilityContractClient::new(&env, &contract_id);

        let provider = TestAddress::generate(&env);
        let stream_id = 102u64;
        let flow_rate = 1000i128;
        let initial_balance = 1000000i128;

        // Create stream
        client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

        env.ledger().set_timestamp(100);

        // Pause the stream
        client.pause_stream(&stream_id);

        env.ledger().set_timestamp(150);

        // Resume the stream
        client.resume_stream(&stream_id, &2000i128);

        // Try to resume again (should fail)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.resume_stream(&stream_id, &3000i128);
        }));
        assert!(result.is_err());

        // Verify stream is still active with first resume parameters
        let flow = client.get_continuous_flow(&stream_id).unwrap();
        assert_eq!(flow.status, StreamStatus::Active);
        assert_eq!(flow.flow_rate_per_second, 2000i128);
    }

    #[test]
    fn test_rapid_timestamp_changes() {
        let env = Env::default();
        let contract_id = env.register_contract(None, UtilityContract);
        let client = UtilityContractClient::new(&env, &contract_id);

        let provider = TestAddress::generate(&env);
        let stream_id = 103u64;
        let flow_rate = 1000i128;
        let initial_balance = 1000000i128;

        // Create stream
        client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

        // Test with rapid timestamp changes including backwards timestamps
        let timestamps = vec![100, 150, 120, 200, 180, 250, 300];

        for (i, &timestamp) in timestamps.iter().enumerate() {
            env.ledger().set_timestamp(timestamp);

            if i % 2 == 0 {
                // Pause on even iterations
                client.pause_stream(&stream_id);
                let flow = client.get_continuous_flow(&stream_id).unwrap();
                assert_eq!(flow.status, StreamStatus::Paused);
            } else {
                // Resume on odd iterations
                client.resume_stream(&stream_id, &(1000 + i as i128 * 100));
                let flow = client.get_continuous_flow(&stream_id).unwrap();
                assert_eq!(flow.status, StreamStatus::Active);
            }
        }

        // Verify stream is in a consistent state
        let final_flow = client.get_continuous_flow(&stream_id).unwrap();
        assert!(final_flow.accumulated_balance >= 0);
        assert!(final_flow.flow_rate_per_second >= 0);
    }

    #[test]
    fn test_maximum_pause_duration() {
        let env = Env::default();
        let contract_id = env.register_contract(None, UtilityContract);
        let client = UtilityContractClient::new(&env, &contract_id);

        let provider = TestAddress::generate(&env);
        let stream_id = 104u64;
        let flow_rate = 1000i128;
        let initial_balance = 1000000i128;

        // Create stream
        client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

        // Let stream run for a bit
        env.ledger().set_timestamp(100);

        // Pause for a very long time (simulate years)
        client.pause_stream(&stream_id);

        let max_timestamp = u64::MAX / 2; // Use half of u64::MAX to avoid overflow
        env.ledger().set_timestamp(max_timestamp);

        // Resume after very long pause
        client.resume_stream(&stream_id, &flow_rate);

        // Verify stream works correctly after long pause
        let flow = client.get_continuous_flow(&stream_id).unwrap();
        assert_eq!(flow.status, StreamStatus::Active);
        assert_eq!(flow.paused_at, 0);
        assert_eq!(flow.last_flow_timestamp, max_timestamp);

        // Test flow calculation continues correctly
        env.ledger().set_timestamp(max_timestamp + 100);
        let balance_after_resume = client.get_continuous_balance(&stream_id).unwrap();
        assert!(balance_after_resume > 0);
    }

    #[test]
    fn test_zero_second_pause_resume() {
        let env = Env::default();
        let contract_id = env.register_contract(None, UtilityContract);
        let client = UtilityContractClient::new(&env, &contract_id);

        let provider = TestAddress::generate(&env);
        let stream_id = 105u64;
        let flow_rate = 1000i128;
        let initial_balance = 1000000i128;

        // Create stream
        client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

        env.ledger().set_timestamp(100);

        // Pause and resume immediately (same timestamp)
        client.pause_stream(&stream_id);
        client.resume_stream(&stream_id, &flow_rate);

        // Verify stream state
        let flow = client.get_continuous_flow(&stream_id).unwrap();
        assert_eq!(flow.status, StreamStatus::Active);
        assert_eq!(flow.paused_at, 0);
        assert_eq!(flow.last_flow_timestamp, 100); // Should be set to resume timestamp

        // Balance should be unchanged due to zero-duration pause
        let balance = client.get_continuous_balance(&stream_id).unwrap();
        assert_eq!(balance, 900000); // 1000000 - (1000 * 100)
    }

    #[test]
    fn test_boundary_conditions() {
        let env = Env::default();
        let contract_id = env.register_contract(None, UtilityContract);
        let client = UtilityContractClient::new(&env, &contract_id);

        let provider = TestAddress::generate(&env);

        // Test with minimum balance
        let stream_id_min = 106u64;
        client.create_continuous_stream(&stream_id_min, &1i128, &1i128, &provider);
        env.ledger().set_timestamp(1);
        client.pause_stream(&stream_id_min);
        client.resume_stream(&stream_id_min, &1i128);

        // Test with maximum reasonable values
        let stream_id_max = 107u64;
        let max_balance = i128::MAX / 1000; // Prevent overflow in calculations
        let max_flow_rate = 1000000i128;
        client.create_continuous_stream(&stream_id_max, &max_flow_rate, &max_balance, &provider);
        env.ledger().set_timestamp(1000);
        client.pause_stream(&stream_id_max);
        client.resume_stream(&stream_id_max, &max_flow_rate);

        // Verify both streams are in valid states
        let min_flow = client.get_continuous_flow(&stream_id_min).unwrap();
        assert_eq!(min_flow.status, StreamStatus::Active);

        let max_flow = client.get_continuous_flow(&stream_id_max).unwrap();
        assert_eq!(max_flow.status, StreamStatus::Active);
        assert!(max_flow.accumulated_balance > 0);
    }

    #[test]
    fn test_interleaved_operations() {
        let env = Env::default();
        let contract_id = env.register_contract(None, UtilityContract);
        let client = UtilityContractClient::new(&env, &contract_id);

        let provider = TestAddress::generate(&env);
        let stream_id = 108u64;
        let flow_rate = 1000i128;
        let initial_balance = 1000000i128;

        // Create stream
        client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

        // Interleave various operations rapidly
        for i in 0..50 {
            env.ledger().set_timestamp(i * 10);

            match i % 4 {
                0 => {
                    // Pause
                    client.pause_stream(&stream_id);
                }
                1 => {
                    // Resume
                    client.resume_stream(&stream_id, &(1000 + i as i128 * 10));
                }
                2 => {
                    // Add balance
                    client.add_continuous_balance(&stream_id, &10000i128);
                }
                3 => {
                    // Withdraw
                    let balance = client.get_continuous_balance(&stream_id).unwrap();
                    if balance > 1000 {
                        client.withdraw_continuous(&stream_id, &1000i128);
                    }
                }
                _ => unreachable!(),
            }

            // Verify stream is always in a valid state
            let flow = client.get_continuous_flow(&stream_id).unwrap();
            assert!(flow.accumulated_balance >= 0);
            assert!(flow.flow_rate_per_second >= 0);
        }

        // Final verification
        let final_flow = client.get_continuous_flow(&stream_id).unwrap();
        assert!(final_flow.accumulated_balance >= 0);
        assert!(final_flow.flow_rate_per_second >= 0);
    }
}
