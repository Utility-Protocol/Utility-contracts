#![cfg(test)]

extern crate std;

use crate::*;
use soroban_sdk::{
    testutils::{Address as TestAddress, Ledger as TestLedger},
    Address, Env, Symbol,
};

#[test]
fn test_pause_stream_stops_flow_calculation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = TestAddress::generate(&env);
    let stream_id = 1u64;
    let flow_rate = 1000i128; // 1000 micro-stroops per second
    let initial_balance = 1000000i128; // 1 XLM in stroops

    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

    // Simulate time passage (100 seconds)
    env.ledger().set_timestamp(100);

    // Pause the stream
    client.pause_stream(&stream_id);

    // Get stream state after pause
    let paused_flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(paused_flow.status, StreamStatus::Paused);
    assert_eq!(paused_flow.paused_at, 100);
    assert_eq!(paused_flow.flow_rate_per_second, 0);
    assert_eq!(paused_flow.accumulated_balance, 900000); // 1000000 - (1000 * 100)

    // Simulate additional time passage (50 more seconds)
    env.ledger().set_timestamp(150);

    // Balance should not have changed during pause
    let still_paused_flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(still_paused_flow.accumulated_balance, 900000);
}

#[test]
fn test_resume_stream_adjusts_timeline() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = TestAddress::generate(&env);
    let stream_id = 2u64;
    let flow_rate = 1000i128;
    let initial_balance = 1000000i128;

    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

    // Simulate time passage (100 seconds)
    env.ledger().set_timestamp(100);

    // Pause the stream
    client.pause_stream(&stream_id);

    // Simulate pause duration (50 seconds)
    env.ledger().set_timestamp(150);

    // Resume with new flow rate
    let new_flow_rate = 2000i128;
    client.resume_stream(&stream_id, &new_flow_rate);

    // Get stream state after resume
    let resumed_flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(resumed_flow.status, StreamStatus::Active);
    assert_eq!(resumed_flow.paused_at, 0); // Pause timestamp cleared
    assert_eq!(resumed_flow.flow_rate_per_second, new_flow_rate);
    assert_eq!(resumed_flow.accumulated_balance, 900000); // Balance unchanged during pause
    assert_eq!(resumed_flow.last_flow_timestamp, 150); // Timestamp reset to resume time

    // Simulate time passage after resume (25 seconds)
    env.ledger().set_timestamp(175);

    // Check that flow calculation resumed correctly
    let final_balance = client.get_continuous_balance(&stream_id).unwrap();
    let expected_balance = 900000 - (2000 * 25); // 900000 - 50000
    assert_eq!(final_balance, expected_balance);
}

#[test]
fn test_provider_access_control() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = TestAddress::generate(&env);
    let unauthorized_user = TestAddress::generate(&env);
    let stream_id = 3u64;
    let flow_rate = 1000i128;
    let initial_balance = 1000000i128;

    // Create stream with provider
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

    // Try to pause with unauthorized user (should fail)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.pause_stream(&stream_id);
    }));
    assert!(result.is_err());

    // Try to resume with unauthorized user (should fail)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.resume_stream(&stream_id, &2000i128);
    }));
    assert!(result.is_err());

    // Pause with authorized provider (should succeed)
    client.pause_stream(&stream_id);
    let paused_flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(paused_flow.status, StreamStatus::Paused);
}

#[test]
fn test_edge_case_depleted_during_pause() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = TestAddress::generate(&env);
    let stream_id = 4u64;
    let flow_rate = 1000i128;
    let initial_balance = 1000i128; // Very small balance

    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

    // Simulate time passage to deplete the stream (1 second)
    env.ledger().set_timestamp(1);

    // Pause the stream (should work even with depleted balance)
    client.pause_stream(&stream_id);

    // Try to resume depleted stream (should fail)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.resume_stream(&stream_id, &2000i128);
    }));
    assert!(result.is_err());

    // Verify stream is marked as depleted
    let depleted_flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(depleted_flow.status, StreamStatus::Depleted);
}

#[test]
fn test_pause_only_active_streams() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = TestAddress::generate(&env);
    let stream_id = 5u64;
    let flow_rate = 1000i128;
    let initial_balance = 0i128; // Start with paused stream

    // Create paused stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

    // Try to pause already paused stream (should fail)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.pause_stream(&stream_id);
    }));
    assert!(result.is_err());

    // Add balance to activate stream
    client.add_continuous_balance(&stream_id, &1000000i128);

    // Now pause should work
    client.pause_stream(&stream_id);
    let paused_flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(paused_flow.status, StreamStatus::Paused);
}

#[test]
fn test_resume_only_paused_streams() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = TestAddress::generate(&env);
    let stream_id = 6u64;
    let flow_rate = 1000i128;
    let initial_balance = 1000000i128;

    // Create active stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

    // Try to resume active stream (should fail)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.resume_stream(&stream_id, &2000i128);
    }));
    assert!(result.is_err());

    // Pause first
    client.pause_stream(&stream_id);

    // Now resume should work
    client.resume_stream(&stream_id, &2000i128);
    let resumed_flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(resumed_flow.status, StreamStatus::Active);
    assert_eq!(resumed_flow.flow_rate_per_second, 2000i128);
}

#[test]
fn test_flow_math_adjustment_post_resume() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = TestAddress::generate(&env);
    let stream_id = 7u64;
    let flow_rate = 1000i128;
    let initial_balance = 1000000i128;

    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

    // Simulate time passage (100 seconds)
    env.ledger().set_timestamp(100);

    // Pause
    client.pause_stream(&stream_id);

    // Balance after 100 seconds of flow
    let paused_balance = client.get_continuous_balance(&stream_id).unwrap();
    assert_eq!(paused_balance, 900000); // 1000000 - (1000 * 100)

    // Simulate pause duration (50 seconds)
    env.ledger().set_timestamp(150);

    // Resume with same flow rate
    client.resume_stream(&stream_id, &flow_rate);

    // Simulate additional time (50 seconds)
    env.ledger().set_timestamp(200);

    // Final balance should account for:
    // - Initial 100 seconds of flow: 1000000 - 100000 = 900000
    // - 50 seconds of pause: no change
    // - 50 seconds after resume: 900000 - 50000 = 850000
    let final_balance = client.get_continuous_balance(&stream_id).unwrap();
    assert_eq!(final_balance, 850000);

    // Verify depletion time calculation is adjusted
    let depletion_time = client.calculate_continuous_depletion(&stream_id).unwrap();
    let expected_depletion = 200 + (850000 / flow_rate) as u64; // Current time + remaining seconds
    assert_eq!(depletion_time, expected_depletion);
}

#[test]
fn test_zero_flow_rate_resume_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = TestAddress::generate(&env);
    let stream_id = 8u64;
    let flow_rate = 1000i128;
    let initial_balance = 1000000i128;

    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

    // Pause
    client.pause_stream(&stream_id);

    // Try to resume with zero flow rate (should fail)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.resume_stream(&stream_id, &0i128);
    }));
    assert!(result.is_err());

    // Try to resume with negative flow rate (should fail)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.resume_stream(&stream_id, &-1000i128);
    }));
    assert!(result.is_err());
}

#[test]
fn test_pause_resume_events_emitted() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = TestAddress::generate(&env);
    let stream_id = 9u64;
    let flow_rate = 1000i128;
    let initial_balance = 1000000i128;

    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider);

    // Simulate time passage
    env.ledger().set_timestamp(100);

    // Pause and check for event
    client.pause_stream(&stream_id);

    // Check for StreamPaused event
    let events = env.events().all();
    let pause_event_found = events.iter().any(|(topics, _data)| {
        topics[0] == Symbol::new(&env, "StreamPaused") && topics[1] == stream_id.into()
    });
    assert!(pause_event_found);

    // Simulate pause duration
    env.ledger().set_timestamp(150);

    // Resume and check for event
    client.resume_stream(&stream_id, &2000i128);

    // Check for StreamResumed event
    let events_after_resume = env.events().all();
    let resume_event_found = events_after_resume.iter().any(|(topics, _data)| {
        topics[0] == Symbol::new(&env, "StreamResumed") && topics[1] == stream_id.into()
    });
    assert!(resume_event_found);
}
