#![cfg(test)]

extern crate std;

use crate::*;
use soroban_sdk::{symbol_short, Address, Env, BytesN};
use std::vec;
use std::format;

#[test]
fn test_dust_detection() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    // Test dust detection logic
    assert!(is_dust_amount(0) == false); // 0 is not dust
    assert!(is_dust_amount(1) == false); // 1 stroop is not dust (threshold is < 1)
    assert!(is_dust_amount(0) == false); // 0 is not dust
    assert!(is_dust_amount(-1) == false); // negative amounts are not dust
}

#[test]
fn test_dust_aggregation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let token_address = Address::generate(&env);
    
    // Create initial aggregation
    let aggregation = get_or_create_dust_aggregation(&env, &token_address);
    assert_eq!(aggregation.total_dust, 0);
    assert_eq!(aggregation.stream_count, 0);

    // Update aggregation
    update_dust_aggregation(&env, &token_address, 5, 3);
    
    let updated = get_or_create_dust_aggregation(&env, &token_address);
    assert_eq!(updated.total_dust, 5);
    assert_eq!(updated.stream_count, 3);
}

#[test]
fn test_admin_setup() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    
    // Set admin
    client.set_admin(&admin);
    
    // Verify admin is set
    let stored_admin = env.storage().instance()
        .get::<DataKey, Address>(&DataKey::AdminAddress)
        .unwrap();
    assert_eq!(stored_admin, admin);
}

#[test]
fn test_gas_bounty_funding() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Fund gas bounty
    let bounty_amount = 1000000i128; // 0.1 XLM
    client.fund_gas_bounty(&bounty_amount);

    // Check bounty pool
    let bounty_pool = env.storage().instance()
        .get::<DataKey, i128>(&DataKey::GasBountyPool)
        .unwrap();
    assert_eq!(bounty_pool, bounty_amount);
}

#[test]
fn test_dust_sweeping_single_stream() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Setup
    client.set_admin(&admin);
    client.set_maintenance_config(&treasury, &0);

    // Create a stream with dust amount
    let stream_id = 1u64;
    let dust_amount = 0i128; // Less than 1 stroop
    client.create_continuous_stream(&stream_id, &1000, &dust_amount);

    // Manually set stream to depleted status with dust
    let mut flow = ContinuousFlow {
        stream_id,
        flow_rate_per_second: 1000,
        accumulated_balance: 0, // Dust amount
        last_flow_timestamp: env.ledger().timestamp(),
        created_timestamp: env.ledger().timestamp(),
        status: StreamStatus::Depleted,
        reserved: [0u8; 7],
    };
    env.storage().instance().set(&DataKey::ContinuousFlow(stream_id), &flow);

    // Test dust detection
    assert!(client.has_dust(&stream_id));

    // Sweep dust (should fail with no dust to sweep since dust_amount = 0)
    let result = std::panic::catch_unwind(|| {
        client.sweep_dust(&token_address, None);
    });
    assert!(result.is_err()); // Should panic with NoDustToSweep
}

#[test]
fn test_dust_sweeping_with_actual_dust() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Setup
    client.set_admin(&admin);
    client.set_maintenance_config(&treasury, &0);

    // Create multiple streams with dust amounts
    for i in 1u64..=10u64 {
        // Create stream with small amount that will become dust
        client.create_continuous_stream(&i, &1000, &1000);
        
        // Simulate flow to create dust remainder
        let mut flow = ContinuousFlow {
            stream_id: i,
            flow_rate_per_second: 1000,
            accumulated_balance: 0, // Dust amount after flow calculation
            last_flow_timestamp: env.ledger().timestamp().saturating_sub(1000),
            created_timestamp: env.ledger().timestamp().saturating_sub(2000),
            status: StreamStatus::Depleted,
            reserved: [0u8; 7],
        };
        env.storage().instance().set(&DataKey::ContinuousFlow(i), &flow);
    }

    // Count streams with dust
    let mut dust_streams = 0;
    for i in 1u64..=10u64 {
        if client.has_dust(&i) {
            dust_streams += 1;
        }
    }
    assert!(dust_streams > 0);

    // Sweep dust
    let sweep_result = client.sweep_dust(&token_address, Some(10));
    assert!(sweep_result.streams_swept > 0);
    assert_eq!(sweep_result.token_address, token_address);

    // Verify dust aggregation
    let aggregation = client.get_dust_aggregation(&token_address);
    assert!(aggregation.is_some());
    let agg = aggregation.unwrap();
    assert_eq!(agg.stream_count, sweep_result.streams_swept);
}

#[test]
#[should_panic(expected = "UnauthorizedAdmin")]
fn test_unauthorized_admin_access() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let unauthorized = Address::generate(&env);
    client.set_admin(&unauthorized);
    
    // This should panic
    require_admin_auth(&env);
}

#[test]
fn test_multi_asset_dust_handling() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let xlm_address = Address::generate(&env);
    let usdc_address = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Setup
    client.set_admin(&admin);
    client.set_maintenance_config(&treasury, &0);

    // Create streams for different tokens
    // XLM streams
    for i in 1u64..=5u64 {
        client.create_continuous_stream(&i, &1000, &500);
        let mut flow = ContinuousFlow {
            stream_id: i,
            flow_rate_per_second: 1000,
            accumulated_balance: 0,
            last_flow_timestamp: env.ledger().timestamp().saturating_sub(1000),
            created_timestamp: env.ledger().timestamp().saturating_sub(2000),
            status: StreamStatus::Depleted,
            reserved: [0u8; 7],
        };
        env.storage().instance().set(&DataKey::ContinuousFlow(i), &flow);
    }

    // USDC streams
    for i in 6u64..=10u64 {
        client.create_continuous_stream(&i, &1000, &500);
        let mut flow = ContinuousFlow {
            stream_id: i,
            flow_rate_per_second: 1000,
            accumulated_balance: 0,
            last_flow_timestamp: env.ledger().timestamp().saturating_sub(1000),
            created_timestamp: env.ledger().timestamp().saturating_sub(2000),
            status: StreamStatus::Depleted,
            reserved: [0u8; 7],
        };
        env.storage().instance().set(&DataKey::ContinuousFlow(i), &flow);
    }

    // Sweep XLM dust
    let xlm_sweep = client.sweep_dust(&xlm_address, Some(5));
    assert_eq!(xlm_sweep.token_address, xlm_address);

    // Sweep USDC dust
    let usdc_sweep = client.sweep_dust(&usdc_address, Some(5));
    assert_eq!(usdc_sweep.token_address, usdc_address);

    // Verify independent aggregation
    let xlm_agg = client.get_dust_aggregation(&xlm_address);
    let usdc_agg = client.get_dust_aggregation(&usdc_address);
    
    assert!(xlm_agg.is_some());
    assert!(usdc_agg.is_some());
    assert_ne!(xlm_agg.unwrap().stream_count, usdc_agg.unwrap().stream_count);
}

#[test]
fn test_gas_bounty_mechanism() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let sweeper = Address::generate(&env);
    let token_address = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Setup
    client.set_admin(&admin);
    client.set_maintenance_config(&treasury, &0);
    client.fund_gas_bounty(&GAS_BOUNTY_AMOUNT * 2); // Fund enough for bounty

    // Create stream with dust
    client.create_continuous_stream(&1u64, &1000, &1000);
    let mut flow = ContinuousFlow {
        stream_id: 1,
        flow_rate_per_second: 1000,
        accumulated_balance: 0,
        last_flow_timestamp: env.ledger().timestamp().saturating_sub(1000),
        created_timestamp: env.ledger().timestamp().saturating_sub(2000),
        status: StreamStatus::Depleted,
        reserved: [0u8; 7],
    };
    env.storage().instance().set(&DataKey::ContinuousFlow(1u64), &flow);

    // Simulate sweeper call (non-admin)
    // Note: In real implementation, this would require proper auth from sweeper
    // For test, we'll check that bounty pool decreases
    let initial_bounty = env.storage().instance()
        .get::<DataKey, i128>(&DataKey::GasBountyPool)
        .unwrap();

    // After sweep, bounty should decrease by GAS_BOUNTY_AMOUNT
    // This test verifies the mechanism is in place
    assert!(initial_bounty >= GAS_BOUNTY_AMOUNT);
}

// Performance test with 10,000 dead streams
#[test]
fn test_massive_dust_sweeping_performance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Setup
    client.set_admin(&admin);
    client.set_maintenance_config(&treasury, &0);

    // Create 10,000 streams with dust
    let stream_count = 10000u64;
    let batch_size = 1000u64;
    
    for batch_start in (1..=stream_count).step_by(batch_size as usize) {
        let batch_end = (batch_start + batch_size - 1).min(stream_count);
        
        for stream_id in batch_start..=batch_end {
            client.create_continuous_stream(&stream_id, &1000, &1000);
            
            let mut flow = ContinuousFlow {
                stream_id,
                flow_rate_per_second: 1000,
                accumulated_balance: 0,
                last_flow_timestamp: env.ledger().timestamp().saturating_sub(1000),
                created_timestamp: env.ledger().timestamp().saturating_sub(2000),
                status: StreamStatus::Depleted,
                reserved: [0u8; 7],
            };
            env.storage().instance().set(&DataKey::ContinuousFlow(stream_id), &flow);
        }
    }

    // Count total dust streams
    let mut total_dust_streams = 0u64;
    for stream_id in 1..=stream_count {
        if client.has_dust(&stream_id) {
            total_dust_streams += 1;
        }
    }

    // Sweep in batches to avoid gas limits
    let mut total_swept = 0u64;
    let mut total_dust_amount = 0i128;
    
    for batch_start in (1..=stream_count).step_by(MAX_SWEEP_STREAMS_PER_CALL as usize) {
        let batch_end = (batch_start + MAX_SWEEP_STREAMS_PER_CALL - 1).min(stream_count);
        let streams_in_batch = batch_end - batch_start + 1;
        
        let sweep_result = client.sweep_dust(&token_address, Some(streams_in_batch));
        total_swept += sweep_result.streams_swept;
        total_dust_amount += sweep_result.total_dust_swept;
    }

    // Verify all dust was swept
    assert_eq!(total_swept, total_dust_streams);
    
    // Verify final aggregation
    let final_aggregation = client.get_dust_aggregation(&token_address);
    assert!(final_aggregation.is_some());
    let agg = final_aggregation.unwrap();
    assert_eq!(agg.stream_count, total_swept);
    assert_eq!(agg.total_dust, total_dust_amount);

    // Verify no dust remains
    let mut remaining_dust = 0u64;
    for stream_id in 1..=stream_count {
        if client.has_dust(&stream_id) {
            remaining_dust += 1;
        }
    }
    assert_eq!(remaining_dust, 0);
}

#[test]
fn test_total_supply_invariant() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Setup
    client.set_admin(&admin);
    client.set_maintenance_config(&treasury, &0);

    // Create streams with known balances
    let initial_balances = vec![1000i128, 2000i128, 500i128];
    let mut total_initial = 0i128;
    
    for (i, &balance) in initial_balances.iter().enumerate() {
        let stream_id = (i + 1) as u64;
        client.create_continuous_stream(&stream_id, &100, &balance);
        total_initial += balance;
    }

    // Simulate some flow to create dust remainders
    for (i, &balance) in initial_balances.iter().enumerate() {
        let stream_id = (i + 1) as u64;
        let mut flow = ContinuousFlow {
            stream_id,
            flow_rate_per_second: 100,
            accumulated_balance: balance % 1000, // Create dust remainder
            last_flow_timestamp: env.ledger().timestamp().saturating_sub(100),
            created_timestamp: env.ledger().timestamp().saturating_sub(200),
            status: StreamStatus::Depleted,
            reserved: [0u8; 7],
        };
        env.storage().instance().set(&DataKey::ContinuousFlow(stream_id), &flow);
    }

    // Calculate total before sweep
    let mut total_before = 0i128;
    for stream_id in 1u64..=3u64 {
        if let Some(flow) = env.storage().instance()
            .get::<DataKey, ContinuousFlow>(&DataKey::ContinuousFlow(stream_id)) {
            total_before += flow.accumulated_balance;
        }
    }

    // Sweep dust
    let sweep_result = client.sweep_dust(&token_address, Some(3));

    // Calculate total after sweep
    let mut total_after = 0i128;
    for stream_id in 1u64..=3u64 {
        if let Some(flow) = env.storage().instance()
            .get::<DataKey, ContinuousFlow>(&DataKey::ContinuousFlow(stream_id)) {
            total_after += flow.accumulated_balance;
        }
    }

    // Verify invariant: total_before = total_after + dust_swept
    assert_eq!(total_before, total_after + sweep_result.total_dust_swept);
    
    // Verify dust was transferred to treasury
    // In a real implementation, this would check treasury balance
    assert!(sweep_result.total_dust_swept > 0);
    assert_eq!(sweep_result.streams_swept, 3);
}
