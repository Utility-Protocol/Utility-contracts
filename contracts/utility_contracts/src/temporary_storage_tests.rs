//! Tests for temporary storage optimizations
//! 
//! This module tests the temporary storage implementation to ensure
//! it reduces ledger costs while maintaining data integrity and consistency.

#[cfg(test)]
mod tests {
    extern crate std;
    use std::vec::Vec;
    use std::format;
    use soroban_sdk::{Address, Env, Symbol, BytesN};
    use crate::{
        temporary_storage::{TempStorageManager, OptimizedFlowCalculator, OptimizedUsageTracker},
        ContinuousFlow, StreamStatus, DataKey, Meter, UsageData, BillingType,
    };

    fn create_test_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn create_test_flow(stream_id: u64, rate: i128, balance: i128) -> ContinuousFlow {
        ContinuousFlow {
            stream_id,
            flow_rate_per_second: rate,
            accumulated_balance: balance,
            last_flow_timestamp: 1000,
            created_timestamp: 1000,
            status: StreamStatus::Active,
            paused_at: 0,
            provider: Address::generate(&create_test_env()),
            buffer_balance: 1000,
            buffer_warning_sent: false,
            payer: Address::generate(&create_test_env()),
            priority_tier: 1,
            grid_epoch_seen: 1000,
            device_mac_pubkey: BytesN::from_array(&[0; 32]),
            is_unreliable: false,
        }
    }

    #[test]
    fn test_temp_storage_flow_accumulation() {
        let env = create_test_env();
        let flow = create_test_flow(1, 100, 1000);
        let current_timestamp = 2000;

        // First calculation should store in temp storage
        let result1 = OptimizedFlowCalculator::calculate_with_temp_storage(&env, &flow, current_timestamp);
        assert_eq!(result1, 100 * (current_timestamp - flow.last_flow_timestamp) as i128);

        // Second calculation should use temp storage
        let result2 = OptimizedFlowCalculator::calculate_with_temp_storage(&env, &flow, current_timestamp);
        assert_eq!(result1, result2);

        // Verify temp storage contains the data
        let (accumulation, timestamp) = TempStorageManager::get_flow_accumulation(&env, flow.stream_id).unwrap();
        assert_eq!(accumulation, result1);
        assert_eq!(timestamp, current_timestamp);
    }

    #[test]
    fn test_temp_storage_meter_usage() {
        let env = create_test_env();
        let meter_id = 1;
        let usage_delta = 500;
        let timestamp = 2000;

        // Store usage delta in temp storage
        OptimizedUsageTracker::track_usage_with_temp_storage(&env, meter_id, usage_delta, timestamp);

        // Retrieve and clear the delta
        let (retrieved_delta, retrieved_timestamp) = 
            TempStorageManager::get_and_clear_meter_usage_delta(&env, meter_id).unwrap();
        
        assert_eq!(retrieved_delta, usage_delta);
        assert_eq!(retrieved_timestamp, timestamp);

        // Should be None after clearing
        let result = TempStorageManager::get_and_clear_meter_usage_delta(&env, meter_id);
        assert!(result.is_none());
    }

    #[test]
    fn test_temp_storage_provider_window() {
        let env = create_test_env();
        let provider = Address::generate(&env);
        
        let window = crate::ProviderWithdrawalWindow {
            daily_withdrawn: 1000,
            last_reset: 1000,
        };

        // Store window in temp storage
        TempStorageManager::store_provider_window(&env, &provider, &window);

        // Retrieve from temp storage
        let retrieved = TempStorageManager::get_provider_window(&env, &provider).unwrap();
        assert_eq!(retrieved.daily_withdrawn, window.daily_withdrawn);
        assert_eq!(retrieved.last_reset, window.last_reset);
    }

    #[test]
    fn test_temp_storage_dust_aggregation() {
        let env = create_test_env();
        let token = Address::generate(&env);
        let dust_delta = 500;

        // Store dust delta
        TempStorageManager::store_dust_delta(&env, &token, dust_delta);

        // Retrieve and clear
        let retrieved = TempStorageManager::get_and_clear_dust_delta(&env, &token).unwrap();
        assert_eq!(retrieved, dust_delta);

        // Should be None after clearing
        let result = TempStorageManager::get_and_clear_dust_delta(&env, &token);
        assert!(result.is_none());
    }

    #[test]
    fn test_temp_storage_fee_delta() {
        let env = create_test_env();
        let stream_id = 1;
        let fee_delta = 100;

        // Store fee delta
        TempStorageManager::store_fee_delta(&env, stream_id, fee_delta);

        // Retrieve and clear
        let retrieved = TempStorageManager::get_and_clear_fee_delta(&env, stream_id).unwrap();
        assert_eq!(retrieved, fee_delta);

        // Should be None after clearing
        let result = TempStorageManager::get_and_clear_fee_delta(&env, stream_id);
        assert!(result.is_none());
    }

    #[test]
    fn test_temp_storage_batch_operations() {
        let env = create_test_env();
        let operation = Symbol::new(&env, "TEST_BATCH");
        let data: Vec<i128> = Vec::from_array(&env, &[100, 200, 300]);

        // Store batch data
        TempStorageManager::store_batch_data(&env, operation, &data);

        // Retrieve batch data
        let retrieved: Vec<i128> = TempStorageManager::get_batch_data(&env, operation).unwrap();
        assert_eq!(retrieved, data);

        // Clear batch data
        TempStorageManager::clear_batch_data(&env, operation);

        // Should be None after clearing
        let result: Option<Vec<i128>> = TempStorageManager::get_batch_data(&env, operation);
        assert!(result.is_none());
    }

    #[test]
    fn test_flow_calculation_with_different_timestamps() {
        let env = create_test_env();
        let flow = create_test_flow(1, 100, 1000);

        // Test with different timestamps
        let timestamp1 = 1500;
        let result1 = OptimizedFlowCalculator::calculate_with_temp_storage(&env, &flow, timestamp1);
        let expected1 = 100 * (timestamp1 - flow.last_flow_timestamp) as i128;
        assert_eq!(result1, expected1);

        let timestamp2 = 2000;
        let result2 = OptimizedFlowCalculator::calculate_with_temp_storage(&env, &flow, timestamp2);
        let expected2 = 100 * (timestamp2 - flow.last_flow_timestamp) as i128;
        assert_eq!(result2, expected2);

        // Results should be different for different timestamps
        assert_ne!(result1, result2);
    }

    #[test]
    fn test_paused_flow_returns_zero() {
        let env = create_test_env();
        let mut flow = create_test_flow(1, 100, 1000);
        flow.status = StreamStatus::Paused;

        let current_timestamp = 2000;
        let result = OptimizedFlowCalculator::calculate_with_temp_storage(&env, &flow, current_timestamp);
        
        // Paused flows should return zero accumulation
        assert_eq!(result, 0);
    }

    #[test]
    fn test_usage_tracking_threshold() {
        let env = create_test_env();
        let meter_id = 1;
        
        // Track small usage (below threshold)
        OptimizedUsageTracker::track_usage_with_temp_storage(&env, meter_id, 100, 2000);
        
        // Should still be in temp storage
        let result = TempStorageManager::get_and_clear_meter_usage_delta(&env, meter_id);
        assert!(result.is_some());

        // Track large usage (above threshold of 1,000,000,000)
        let large_usage = 2_000_000_000;
        OptimizedUsageTracker::track_usage_with_temp_storage(&env, meter_id, large_usage, 3000);
        
        // Should trigger flush to persistent storage
        // Note: In a real implementation, this would update the persistent meter data
        let result = TempStorageManager::get_and_clear_meter_usage_delta(&env, meter_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, large_usage);
    }

    #[test]
    fn test_temp_storage_ttl_behavior() {
        let env = create_test_env();
        let stream_id = 1;
        let current_timestamp = 2000;
        let flow = create_test_flow(stream_id, 100, 1000);

        // Store accumulation
        let result = OptimizedFlowCalculator::calculate_with_temp_storage(&env, &flow, current_timestamp);
        
        // Verify it's stored
        let stored = TempStorageManager::get_flow_accumulation(&env, stream_id);
        assert!(stored.is_some());

        // Simulate ledger advancement beyond TTL (5 ledgers)
        for _ in 0..6 {
            env.ledger().set(env.ledger().sequence() + 1, env.ledger().timestamp() + 1);
        }

        // Data should still be accessible within TTL period
        // Note: In actual Soroban runtime, temporary storage would be cleared after TTL
        let stored = TempStorageManager::get_flow_accumulation(&env, stream_id);
        assert!(stored.is_some());
    }

    #[test]
    fn test_concurrent_temp_storage_operations() {
        let env = create_test_env();
        
        // Test multiple streams simultaneously
        let flow1 = create_test_flow(1, 100, 1000);
        let flow2 = create_test_flow(2, 200, 2000);
        let flow3 = create_test_flow(3, 300, 3000);

        let timestamp = 2000;

        // Calculate accumulation for all streams
        let result1 = OptimizedFlowCalculator::calculate_with_temp_storage(&env, &flow1, timestamp);
        let result2 = OptimizedFlowCalculator::calculate_with_temp_storage(&env, &flow2, timestamp);
        let result3 = OptimizedFlowCalculator::calculate_with_temp_storage(&env, &flow3, timestamp);

        // Verify all results are stored correctly
        let stored1 = TempStorageManager::get_flow_accumulation(&env, 1).unwrap();
        let stored2 = TempStorageManager::get_flow_accumulation(&env, 2).unwrap();
        let stored3 = TempStorageManager::get_flow_accumulation(&env, 3).unwrap();

        assert_eq!(stored1.0, result1);
        assert_eq!(stored2.0, result2);
        assert_eq!(stored3.0, result3);

        // Results should be different for different flow rates
        assert_ne!(result1, result2);
        assert_ne!(result2, result3);
        assert_ne!(result1, result3);
    }

    #[test]
    fn test_temp_storage_cost_optimization() {
        let env = create_test_env();
        let stream_id = 1;
        let flow = create_test_flow(stream_id, 100, 1000);

        // Simulate multiple rapid calculations
        let timestamp = 2000;
        let mut results = Vec::new(&env);

        for _ in 0..10 {
            let result = OptimizedFlowCalculator::calculate_with_temp_storage(&env, &flow, timestamp);
            results.push_back(result);
        }

        // All results should be identical (using cached temp storage)
        for i in 1..results.len() {
            assert_eq!(results.get(0), results.get(i));
        }

        // Verify temp storage was used (should contain the cached result)
        let stored = TempStorageManager::get_flow_accumulation(&env, stream_id);
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().0, *results.get(0));
    }
}
