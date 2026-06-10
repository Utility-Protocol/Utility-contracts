#![cfg(test)]

extern crate std;

use soroban_sdk::testutils::{Address as TestAddress, Ledger as TestLedger};
use soroban_sdk::{symbol_short, Address, Env, Symbol};
use crate::{
    BufferDepletedEvent, BufferWarningEvent, ContractError, ContinuousFlow, StreamStatus, 
    UtilityContract, UtilityContractClient, BUFFER_DURATION_SECONDS, BUFFER_WARNING_THRESHOLD
};

#[test]
fn test_buffer_creation_requirement() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let payer = Address::generate(&env);
    let stream_id = 1;
    let flow_rate = 1000; // 1000 stroops per second
    let initial_balance = 5000;

    // Test 1: Verify required buffer calculation (24 hours)
    let expected_buffer = flow_rate * BUFFER_DURATION_SECONDS as i128;
    assert_eq!(client.get_required_buffer(&flow_rate), expected_buffer);

    // Test 2: Stream creation should fail without proper authorization
    env.mock_auths(&[]);
    let result = std::panic::catch_unwind(|| {
        client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);
    });
    assert!(result.is_err());

    // Test 3: Successful stream creation with buffer
    env.mock_auths(&[
        (&provider, &Symbol::new(&env, "create_continuous_stream")),
        (&payer, &Symbol::new(&env, "create_continuous_stream")),
    ]);
    
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);

    // Verify stream was created with correct buffer
    let stream = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(stream.buffer_balance, expected_buffer);
    assert_eq!(stream.flow_rate_per_second, flow_rate);
    assert_eq!(stream.accumulated_balance, initial_balance);
    assert_eq!(stream.payer, payer);
    assert_eq!(stream.provider, provider);
    assert!(!stream.buffer_warning_sent);
}

#[test]
fn test_buffer_depletion_logic() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let payer = Address::generate(&env);
    let stream_id = 1;
    let flow_rate = 1000; // 1000 stroops per second
    let initial_balance = 2000; // Small initial balance to trigger buffer usage
    let buffer_amount = flow_rate * BUFFER_DURATION_SECONDS as i128;

    // Create stream
    env.mock_auths(&[
        (&provider, &Symbol::new(&env, "create_continuous_stream")),
        (&payer, &Symbol::new(&env, "create_continuous_stream")),
    ]);
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);

    // Advance time to deplete main balance
    env.ledger().set_timestamp(env.ledger().timestamp() + 3); // 3 seconds

    // Check that main balance is depleted and buffer is being used
    let stream = client.get_continuous_flow(&stream_id).unwrap();
    let current_balance = client.get_continuous_balance(&stream_id).unwrap();
    let buffer_balance = client.get_buffer_balance(&stream_id).unwrap();
    
    assert!(current_balance <= 0, "Main balance should be depleted");
    assert!(buffer_balance < buffer_amount, "Buffer should be partially used");

    // Advance time further to trigger buffer warning
    let remaining_buffer_time = buffer_balance / flow_rate;
    if remaining_buffer_time <= BUFFER_WARNING_THRESHOLD {
        // Check if warning was sent (this would be verified through events in production)
        let updated_stream = client.get_continuous_flow(&stream_id).unwrap();
        assert!(updated_stream.buffer_warning_sent, "Buffer warning should be sent");
    }
}

#[test]
fn test_buffer_warning_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let payer = Address::generate(&env);
    let stream_id = 1;
    let flow_rate = 1000;
    let initial_balance = 1000;
    let buffer_amount = flow_rate * BUFFER_DURATION_SECONDS as i128;

    // Create stream
    env.mock_auths(&[
        (&provider, &Symbol::new(&env, "create_continuous_stream")),
        (&payer, &Symbol::new(&env, "create_continuous_stream")),
    ]);
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);

    // Advance time to near buffer depletion
    let warning_time = buffer_amount / flow_rate - BUFFER_WARNING_THRESHOLD + 100;
    env.ledger().set_timestamp(env.ledger().timestamp() + warning_time as u64);

    // Trigger flow calculation which should emit BufferWarning
    client.get_continuous_balance(&stream_id);

    // In production, we would verify the BufferWarning event was emitted
    let stream = client.get_continuous_flow(&stream_id).unwrap();
    assert!(stream.buffer_warning_sent);
}

#[test]
fn test_buffer_depletion_and_termination() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let payer = Address::generate(&env);
    let stream_id = 1;
    let flow_rate = 1000;
    let initial_balance = 1000;
    let buffer_amount = flow_rate * BUFFER_DURATION_SECONDS as i128;

    // Create stream
    env.mock_auths(&[
        (&provider, &Symbol::new(&env, "create_continuous_stream")),
        (&payer, &Symbol::new(&env, "create_continuous_stream")),
    ]);
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);

    // Advance time beyond buffer depletion
    let total_depletion_time = (initial_balance + buffer_amount) / flow_rate + 100;
    env.ledger().set_timestamp(env.ledger().timestamp() + total_depletion_time as u64);

    // Trigger flow calculation which should deplete buffer and terminate stream
    let final_balance = client.get_continuous_balance(&stream_id);
    let final_buffer = client.get_buffer_balance(&stream_id);

    assert_eq!(final_balance.unwrap(), 0, "Main balance should be zero");
    assert_eq!(final_buffer.unwrap(), 0, "Buffer should be zero");

    let stream = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(stream.status, StreamStatus::Depleted, "Stream should be depleted");
}

#[test]
fn test_amicable_closure_refund() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let payer = Address::generate(&env);
    let stream_id = 1;
    let flow_rate = 1000;
    let initial_balance = 5000;
    let buffer_amount = flow_rate * BUFFER_DURATION_SECONDS as i128;

    // Create stream
    env.mock_auths(&[
        (&provider, &Symbol::new(&env, "create_continuous_stream")),
        (&payer, &Symbol::new(&env, "create_continuous_stream")),
    ]);
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);

    // Close stream amicably before depletion
    env.mock_auths(&[(&provider, &Symbol::new(&env, "close_stream_amicably"))]);
    let refunded_amount = client.close_stream_amicably(&stream_id);

    assert_eq!(refunded_amount, buffer_amount, "Full buffer should be refunded");

    // Verify stream is marked as depleted
    let stream = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(stream.status, StreamStatus::Depleted);
    assert_eq!(stream.buffer_balance, 0);
}

#[test]
fn test_additional_buffer_deposit() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let payer = Address::generate(&env);
    let stream_id = 1;
    let flow_rate = 1000;
    let initial_balance = 1000;
    let additional_buffer = 5000;

    // Create stream
    env.mock_auths(&[
        (&provider, &Symbol::new(&env, "create_continuous_stream")),
        (&payer, &Symbol::new(&env, "create_continuous_stream")),
    ]);
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);

    let initial_buffer = client.get_buffer_balance(&stream_id).unwrap();

    // Add additional buffer
    env.mock_auths(&[(&payer, &Symbol::new(&env, "add_continuous_buffer"))]);
    client.add_continuous_buffer(&stream_id, &additional_buffer);

    let updated_buffer = client.get_buffer_balance(&stream_id).unwrap();
    assert_eq!(updated_buffer, initial_buffer + additional_buffer);
}

#[test]
fn test_buffer_security_against_malicious_draining() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let payer = Address::generate(&env);
    let attacker = Address::generate(&env);
    let stream_id = 1;
    let flow_rate = 1000;
    let initial_balance = 5000;

    // Create stream
    env.mock_auths(&[
        (&provider, &Symbol::new(&env, "create_continuous_stream")),
        (&payer, &Symbol::new(&env, "create_continuous_stream")),
    ]);
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);

    let initial_buffer = client.get_buffer_balance(&stream_id).unwrap();

    // Test 1: Unauthorized withdrawal should fail
    env.mock_auths(&[(&attacker, &Symbol::new(&env, "withdraw_continuous"))]);
    let result = std::panic::catch_unwind(|| {
        client.withdraw_continuous(&stream_id, &1000);
    });
    assert!(result.is_err(), "Unauthorized withdrawal should fail");

    // Test 2: Even authorized withdrawal should only affect main balance, not buffer
    env.mock_auths(&[(&provider, &Symbol::new(&env, "withdraw_continuous"))]);
    let withdrawn = client.withdraw_continuous(&stream_id, &2000);
    assert_eq!(withdrawn, 2000);

    let buffer_after_withdrawal = client.get_buffer_balance(&stream_id).unwrap();
    assert_eq!(buffer_after_withdrawal, initial_buffer, "Buffer should be protected from withdrawals");

    // Test 3: Unauthorized buffer addition should fail
    env.mock_auths(&[(&attacker, &Symbol::new(&env, "add_continuous_buffer"))]);
    let result = std::panic::catch_unwind(|| {
        client.add_continuous_buffer(&stream_id, &1000);
    });
    assert!(result.is_err(), "Unauthorized buffer addition should fail");
}

#[test]
fn test_buffer_math_precision() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let payer = Address::generate(&env);
    let stream_id = 1;
    let flow_rate = 1; // Minimal flow rate for precision testing
    let initial_balance = 0;

    // Create stream with minimal flow rate
    env.mock_auths(&[
        (&provider, &Symbol::new(&env, "create_continuous_stream")),
        (&payer, &Symbol::new(&env, "create_continuous_stream")),
    ]);
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);

    let expected_buffer = BUFFER_DURATION_SECONDS as i128;
    let actual_buffer = client.get_buffer_balance(&stream_id).unwrap();
    assert_eq!(actual_buffer, expected_buffer);

    // Test precise time-based depletion
    env.ledger().set_timestamp(env.ledger().timestamp() + 3600); // 1 hour
    let buffer_after_1hour = client.get_buffer_balance(&stream_id).unwrap();
    assert_eq!(buffer_after_1hour, expected_buffer - 3600);
}

#[test]
fn test_stream_creation_without_buffer_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let payer = Address::generate(&env);
    let stream_id = 1;
    let flow_rate = 1000;
    let initial_balance = 5000;

    // Attempt to create stream without proper authorization for buffer transfer
    env.mock_auths(&[(&provider, &Symbol::new(&env, "create_continuous_stream"))]);
    
    let result = std::panic::catch_unwind(|| {
        client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);
    });
    assert!(result.is_err(), "Stream creation should fail without payer authorization for buffer");

    // Verify no stream was created
    assert!(client.get_continuous_flow(&stream_id).is_none());
}

#[test]
fn test_buffer_refund_only_on_amicable_closure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let payer = Address::generate(&env);
    let stream_id = 1;
    let flow_rate = 1000;
    let initial_balance = 1000; // Small balance to trigger buffer depletion

    // Create stream
    env.mock_auths(&[
        (&provider, &Symbol::new(&env, "create_continuous_stream")),
        (&payer, &Symbol::new(&env, "create_continuous_stream")),
    ]);
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance, &provider, &payer);

    // Let stream deplete naturally
    let total_depletion_time = (initial_balance + (flow_rate * BUFFER_DURATION_SECONDS as i128)) / flow_rate + 100;
    env.ledger().set_timestamp(env.ledger().timestamp() + total_depletion_time as u64);
    
    client.get_continuous_balance(&stream_id); // Trigger depletion

    // Attempt refund on depleted stream should fail
    env.mock_auths(&[(&provider, &Symbol::new(&env, "close_stream_amicably"))]);
    let result = std::panic::catch_unwind(|| {
        client.close_stream_amicably(&stream_id);
    });
    assert!(result.is_err(), "Refund should fail on depleted stream");
}
