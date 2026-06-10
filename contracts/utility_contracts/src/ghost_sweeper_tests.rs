extern crate std;

use soroban_sdk::{testutils::Address as TestAddress, testutils::BytesN as TestBytesN, Env, Address, Vec};
use crate::ghost_sweeper::{
    GhostSweeper, GhostStreamPruned, StreamArchive, PruneReason, GhostStreamCandidate,
    SweeperResult, SweeperStatistics, GHOST_STREAM_THRESHOLD_DAYS
};
use crate::{ContractError, DataKey, ContinuousFlow, StreamStatus};

#[cfg(test)]
pub mod ghost_sweeper_tests {
    use super::*;

    /// Test basic ghost stream pruning
    #[test]
    fn test_prune_ghost_stream_basic() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, GhostSweeper);

        let relayer = TestAddress::random(&env);
        let provider = TestAddress::random(&env);
        let payer = TestAddress::random(&env);
        let stream_id = 1u64;

        // Create a ghost stream (zero balance for >90 days)
        let ghost_stream = create_ghost_stream(&env, stream_id, provider.clone(), payer.clone());
        let stream_key = DataKey::ContinuousFlow(stream_id);
        env.storage().persistent().set(&stream_key, &ghost_stream);

        // Prune the ghost stream
        let gas_bounty = GhostSweeper::prune_ghost_stream(env.clone(), stream_id, relayer.clone());

        // Verify bounty was paid
        assert!(gas_bounty > 0);

        // Verify stream was removed
        assert!(!env.storage().persistent().has(&stream_key));

        // Verify archive was created
        let archive_key = DataKey::StreamArchive(stream_id);
        assert!(env.storage().persistent().has(&archive_key));

        // Verify archive data
        let archive: StreamArchive = env.storage().persistent().get(&archive_key).unwrap();
        assert_eq!(archive.stream_id, stream_id);
        assert_eq!(archive.prune_reason, PruneReason::ZeroBalanceExpired);

        // Verify statistics were updated
        let stats = GhostSweeper::get_sweeper_statistics(env.clone());
        assert_eq!(stats.total_streams_pruned, 1);
        assert_eq!(stats.total_gas_bounty_paid, gas_bounty);
    }

    /// Test pruning eligibility check
    #[test]
    fn test_pruning_eligibility() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, GhostSweeper);

        let provider = TestAddress::random(&env);
        let payer = TestAddress::random(&env);
        let stream_id = 1u64;

        // Create a stream that's not eligible (recent activity)
        let active_stream = create_active_stream(&env, stream_id, provider.clone(), payer.clone());
        let stream_key = DataKey::ContinuousFlow(stream_id);
        env.storage().persistent().set(&stream_key, &active_stream);

        // Check eligibility
        let candidate = GhostSweeper::check_stream_eligibility(env.clone(), stream_id);
        assert!(candidate.is_some());
        assert!(!candidate.unwrap().is_eligible_for_pruning);

        // Try to prune (should fail)
        let relayer = TestAddress::random(&env);
        let result = std::panic::catch_unwind(|| {
            GhostSweeper::prune_ghost_stream(env.clone(), stream_id, relayer);
        });
        assert!(result.is_err());
    }

    /// Test stream with pending buffer cannot be pruned
    #[test]
    fn test_stream_with_pending_buffer() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, GhostSweeper);

        let provider = TestAddress::random(&env);
        let payer = TestAddress::random(&env);
        let stream_id = 1u64;
        let relayer = TestAddress::random(&env);

        // Create a ghost stream with buffer balance
        let mut ghost_stream = create_ghost_stream(&env, stream_id, provider, payer);
        ghost_stream.buffer_balance = 1000; // Has pending buffer

        let stream_key = DataKey::ContinuousFlow(stream_id);
        env.storage().persistent().set(&stream_key, &ghost_stream);

        // Try to prune (should fail)
        let result = std::panic::catch_unwind(|| {
            GhostSweeper::prune_ghost_stream(env.clone(), stream_id, relayer);
        });
        assert!(result.is_err());
    }

    /// Test batch pruning of multiple ghost streams
    #[test]
    fn test_batch_prune_ghost_streams() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, GhostSweeper);

        let provider = TestAddress::random(&env);
        let payer = TestAddress::random(&env);
        let relayer = TestAddress::random(&env);

        // Create multiple ghost streams — 5 fixed IDs, pre-built with vec! macro
        let stream_ids = soroban_sdk::vec![&env, 1u64, 2u64, 3u64, 4u64, 5u64];
        for stream_id in stream_ids.iter() {
            let ghost_stream = create_ghost_stream(&env, stream_id, provider.clone(), payer.clone());
            let stream_key = DataKey::ContinuousFlow(stream_id);
            env.storage().persistent().set(&stream_key, &ghost_stream);
        }

        // Batch prune
        let result = GhostSweeper::batch_prune_ghost_streams(env.clone(), stream_ids, relayer.clone());

        // Verify results
        assert_eq!(result.streams_pruned, 5);
        assert!(result.total_bytes_reclaimed > 0);
        assert!(result.total_gas_bounty_paid > 0);
        assert_eq!(result.relayer, relayer);

        // Verify statistics
        let stats = GhostSweeper::get_sweeper_statistics(env.clone());
        assert_eq!(stats.total_streams_pruned, 5);
        assert_eq!(stats.total_sweep_operations, 1);
    }

    /// Test ghost stream candidates listing
    #[test]
    fn test_get_ghost_stream_candidates() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, GhostSweeper);

        let provider = TestAddress::random(&env);
        let payer = TestAddress::random(&env);

        // Create mix of ghost and active streams
        for i in 1..=10 {
            let stream_id = i;
            let stream = if i <= 3 {
                create_ghost_stream(&env, stream_id, provider.clone(), payer.clone())
            } else {
                create_active_stream(&env, stream_id, provider.clone(), payer.clone())
            };
            
            let stream_key = DataKey::ContinuousFlow(stream_id);
            env.storage().persistent().set(&stream_key, &stream);
        }

        // Get candidates
        let candidates = GhostSweeper::get_ghost_stream_candidates(env.clone(), 20);

        // Should have at least the 3 ghost streams
        assert!(candidates.len() >= 3);

        // Check that ghost streams are marked as eligible
        let eligible_count = candidates.iter()
            .filter(|c| c.is_eligible_for_pruning)
            .count();
        assert!(eligible_count >= 3);
    }

    /// Test archive integrity verification
    #[test]
    fn test_archive_integrity() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, GhostSweeper);

        let relayer = TestAddress::random(&env);
        let provider = TestAddress::random(&env);
        let payer = TestAddress::random(&env);
        let stream_id = 1u64;

        // Create and prune ghost stream
        let ghost_stream = create_ghost_stream(&env, stream_id, provider.clone(), payer.clone());
        let stream_key = DataKey::ContinuousFlow(stream_id);
        env.storage().persistent().set(&stream_key, &ghost_stream);

        GhostSweeper::prune_ghost_stream(env.clone(), stream_id, relayer);

        // Retrieve archive
        let archive = GhostSweeper::get_stream_archive(env.clone(), stream_id);
        assert!(archive.is_some());

        let archive = archive.unwrap();
        
        // Verify archive integrity
        assert_eq!(archive.stream_id, stream_id);
        assert_eq!(archive.provider, provider);
        assert_eq!(archive.payer, payer);
        assert_eq!(archive.final_balance, ghost_stream.accumulated_balance);
        assert_eq!(archive.prune_reason, PruneReason::ZeroBalanceExpired);
        assert!(archive.data_hash != TestBytesN::from_array(&[0u8; 32]));
    }

    /// Test gas bounty calculation
    #[test]
    fn test_gas_bounty_calculation() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, GhostSweeper);

        let relayer = TestAddress::random(&env);
        let provider = TestAddress::random(&env);
        let payer = TestAddress::random(&env);

        // Create streams with different sizes
        let small_stream_id = 1u64;
        let large_stream_id = 2u64;

        let small_stream = create_ghost_stream(&env, small_stream_id, provider.clone(), payer.clone());
        let large_stream = create_large_ghost_stream(&env, large_stream_id, provider.clone(), payer.clone());

        let small_key = DataKey::ContinuousFlow(small_stream_id);
        let large_key = DataKey::ContinuousFlow(large_stream_id);
        env.storage().persistent().set(&small_key, &small_stream);
        env.storage().persistent().set(&large_key, &large_stream);

        // Prune both streams
        let small_bounty = GhostSweeper::prune_ghost_stream(env.clone(), small_stream_id, relayer.clone());
        let large_bounty = GhostSweeper::prune_ghost_stream(env.clone(), large_stream_id, relayer);

        // Larger stream should have higher bounty
        assert!(large_bounty > small_bounty);
        assert!(small_bounty > 0);
    }

    /// Test storage decay simulation
    #[test]
    fn test_storage_decay_simulation() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, GhostSweeper);

        let provider = TestAddress::random(&env);
        let payer = TestAddress::random(&env);
        let relayer = TestAddress::random(&env);

        // Simulate long-term storage decay
        let mut total_initial_storage = 0u64;
        let stream_ids: Vec<u64> = (1..=100).collect();
        
        for stream_id in stream_ids.iter() {
            let stream = create_ghost_stream(&env, *stream_id, provider.clone(), payer.clone());
            let stream_key = DataKey::ContinuousFlow(*stream_id);
            env.storage().persistent().set(&stream_key, &stream);
            
            // Estimate storage size
            total_initial_storage += 500; // Estimated per stream
        }

        // Run sweeper
        let stream_ids_vec = Vec::from_array(&env, stream_ids);
        let result = GhostSweeper::batch_prune_ghost_streams(env.clone(), stream_ids_vec, relayer);

        // Verify storage recovery
        assert_eq!(result.streams_pruned, 100);
        assert!(result.total_bytes_reclaimed > total_initial_storage * 80 / 100); // At least 80% recovered

        // Verify final statistics
        let stats = GhostSweeper::get_sweeper_statistics(env.clone());
        assert_eq!(stats.total_streams_pruned, 100);
        assert!(stats.total_bytes_reclaimed > 0);
        assert!(stats.total_gas_bounty_paid > 0);
    }

    /// Test edge case: stream not found
    #[test]
    fn test_stream_not_found() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, GhostSweeper);

        let relayer = TestAddress::random(&env);
        let stream_id = 999u64; // Non-existent stream

        // Try to prune non-existent stream
        let result = std::panic::catch_unwind(|| {
            GhostSweeper::prune_ghost_stream(env.clone(), stream_id, relayer);
        });
        assert!(result.is_err());
    }

    /// Test edge case: MAC address mapping cleanup
    #[test]
    fn test_mac_address_cleanup() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, GhostSweeper);

        let relayer = TestAddress::random(&env);
        let provider = TestAddress::random(&env);
        let payer = TestAddress::random(&env);
        let stream_id = 1u64;
        let device_mac = TestBytesN::random(&env);

        // Create stream with MAC address
        let mut ghost_stream = create_ghost_stream(&env, stream_id, provider, payer);
        ghost_stream.device_mac_pubkey = device_mac.clone();

        let stream_key = DataKey::ContinuousFlow(stream_id);
        let mac_key = DataKey::DeviceHash(device_mac.clone());
        
        env.storage().persistent().set(&stream_key, &ghost_stream);
        env.storage().persistent().set(&mac_key, &"some_mac_data");

        // Prune stream
        GhostSweeper::prune_ghost_stream(env.clone(), stream_id, relayer);

        // Verify MAC mapping was also removed
        assert!(!env.storage().persistent().has(&mac_key));
        assert!(!env.storage().persistent().has(&stream_key));
    }

    /// Helper function to create ghost stream
    fn create_ghost_stream(env: &Env, stream_id: u64, provider: Address, payer: Address) -> ContinuousFlow {
        let current_time = env.ledger().timestamp();
        let old_timestamp = current_time - (GHOST_STREAM_THRESHOLD_DAYS + 10) * 24 * 60 * 60;

        ContinuousFlow {
            stream_id,
            flow_rate_per_second: 0,
            accumulated_balance: 0, // Zero balance
            last_flow_timestamp: old_timestamp,
            created_timestamp: old_timestamp,
            status: StreamStatus::Depleted,
            paused_at: 0,
            provider,
            payer,
            buffer_balance: 0, // No buffer
            buffer_warning_sent: false,
            priority_tier: 1,
            grid_epoch_seen: old_timestamp,
            device_mac_pubkey: TestBytesN::random(env),
            is_unreliable: false,
        }
    }

    /// Helper function to create active stream
    fn create_active_stream(env: &Env, stream_id: u64, provider: Address, payer: Address) -> ContinuousFlow {
        let current_time = env.ledger().timestamp();
        let recent_timestamp = current_time - 24 * 60 * 60; // 1 day ago

        ContinuousFlow {
            stream_id,
            flow_rate_per_second: 1000,
            accumulated_balance: 5000, // Has balance
            last_flow_timestamp: recent_timestamp,
            created_timestamp: recent_timestamp,
            status: StreamStatus::Active,
            paused_at: 0,
            provider,
            payer,
            buffer_balance: 1000,
            buffer_warning_sent: false,
            priority_tier: 1,
            grid_epoch_seen: recent_timestamp,
            device_mac_pubkey: TestBytesN::random(env),
            is_unreliable: false,
        }
    }

    /// Helper function to create large ghost stream
    fn create_large_ghost_stream(env: &Env, stream_id: u64, provider: Address, payer: Address) -> ContinuousFlow {
        let mut stream = create_ghost_stream(env, stream_id, provider, payer);
        
        // Add some data to increase size
        stream.flow_rate_per_second = 1000000;
        stream.accumulated_balance = 0;
        
        stream
    }
}

/// Property-based tests for ghost sweeper
#[cfg(test)]
mod ghost_sweeper_property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn test_gas_bounty_properties(
            storage_bytes in 200u64..2000u64,
        ) {
            let env = Env::new();
            let contract_address = TestAddress::random(&env);
            env.register_contract(&contract_address, GhostSweeper);

            // Property: bounty should be proportional to storage size
            let bounty = GhostSweeper::calculate_gas_bounty(storage_bytes);
            
            prop_assert!(bounty > 0);
            
            // Bounty should scale reasonably with storage
            let expected_min_bounty = storage_bytes as i128 * 500 / 10000; // 5% minimum
            prop_assert!(bounty >= expected_min_bounty);
        }

        #[test]
        fn test_pruning_eligibility_properties(
            days_inactive in 0u64..200u64,
            has_balance in bool::ANY,
            has_buffer in bool::ANY,
        ) {
            let env = Env::new();
            let contract_address = TestAddress::random(&env);
            env.register_contract(&contract_address, GhostSweeper);

            let provider = TestAddress::random(&env);
            let payer = TestAddress::random(&env);
            let stream_id = 1u64;

            // Create stream with specified properties
            let current_time = env.ledger().timestamp();
            let inactive_timestamp = current_time - days_inactive * 24 * 60 * 60;

            let mut stream = ContinuousFlow {
                stream_id,
                flow_rate_per_second: 0,
                accumulated_balance: if has_balance { 1000 } else { 0 },
                last_flow_timestamp: inactive_timestamp,
                created_timestamp: inactive_timestamp,
                status: if has_balance { StreamStatus::Active } else { StreamStatus::Depleted },
                paused_at: 0,
                provider,
                payer,
                buffer_balance: if has_buffer { 500 } else { 0 },
                buffer_warning_sent: false,
                priority_tier: 1,
                grid_epoch_seen: inactive_timestamp,
                device_mac_pubkey: TestBytesN::random(&env),
                is_unreliable: false,
            };

            let stream_key = DataKey::ContinuousFlow(stream_id);
            env.storage().persistent().set(&stream_key, &stream);

            // Check eligibility
            let candidate = GhostSweeper::check_stream_eligibility(env.clone(), stream_id);
            
            if let Some(candidate) = candidate {
                // Property: streams with balance or buffer should not be eligible
                if has_balance || has_buffer {
                    prop_assert!(!candidate.is_eligible_for_pruning);
                }

                // Property: streams inactive for less than threshold should not be eligible
                if days_inactive < GHOST_STREAM_THRESHOLD_DAYS {
                    prop_assert!(!candidate.is_eligible_for_pruning);
                }

                // Property: days_zero_balance should match days_inactive for zero-balance streams
                if !has_balance {
                    prop_assert_eq!(candidate.days_zero_balance, days_inactive);
                }
            }
        }
    }
}
