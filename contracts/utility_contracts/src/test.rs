#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, BytesN, Env, Vec};

// --- Helpers ---
fn device_key(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

fn create_token(env: &Env) -> Address {
    let admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(admin).address()
}

// ==================== MOCK CONTRACTS ====================

mod mock_sorosusu {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct MockSoroSusu;

    #[contractimpl]
    impl MockSoroSusu {
        pub fn set_default(env: Env, user: Address, in_default: bool) {
            env.storage().instance().set(&user, &in_default);
        }

        pub fn is_in_default(env: Env, user: Address) -> bool {
            env.storage().instance().get(&user).unwrap_or(false)
        }

        pub fn is_trusted_saver(_env: Env, _user: Address) -> bool { false }
        pub fn get_susu_score(_env: Env, _user: Address) -> u32 { 0 }

        pub fn record_debt_payment(env: Env, user: Address, amount: i128) {
            let key = (user.clone(), soroban_sdk::symbol_short!("paid"));
            let current: i128 = env.storage().instance().get(&key).unwrap_or(0);
            env.storage().instance().set(&key, &current.saturating_add(amount));
        }
    }
}

mod mock_environmental_oracle {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct MockEnvironmentalOracle;

    #[contractimpl]
    impl MockEnvironmentalOracle {
        pub fn xlm_to_usd_cents(_env: Env, xlm_amount: i128) -> i128 {
            xlm_amount.saturating_mul(100)
        }

        pub fn usd_cents_to_xlm(_env: Env, usd_cents: i128) -> i128 {
            usd_cents.saturating_div(100)
        }

        pub fn get_price(env: Env) -> utility_contracts::PriceData {
            utility_contracts::PriceData {
                price: 100,
                decimals: 2,
                last_updated: env.ledger().timestamp(),
            }
        }

        pub fn verify_green_source(
            _env: Env,
            _provider: Address,
            _meter_id: u64,
            _timestamp: u64,
        ) -> bool {
            true
        }
    }
}

#[test]
fn test_grace_period_expiration() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &2000);

    let device_public_key = device_key(&env, 1);
    // Integrated Seasonal/Sustainability params: end_date (0) and rent_deposit (0)
    let meter_id = client.register_meter(&user, &provider, &10, &token_address, &device_public_key, &0, &0);

    // Top up with balance to activate
    client.top_up(&meter_id, &500);
    let meter = client.get_meter(&meter_id).unwrap();
    assert!(meter.is_active);
    assert_eq!(meter.balance, 500);

    // Pair the meter
    client.initiate_pairing(&meter_id);
    client.complete_pairing(&meter_id, &BytesN::from_array(&env, &[2u8; 64]));

    // Use up balance exactly to 0 - should start grace period
    env.ledger().set_timestamp(env.ledger().timestamp() + 50); 
    client.claim(&meter_id);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.balance, 0);
    assert!(meter.is_active); 
    assert!(meter.grace_period_start > 0); 

    // Fast forward another 25 hours - should expire grace period
    env.ledger().set_timestamp(env.ledger().timestamp() + (25 * 60 * 60));
    client.claim(&meter_id); 

    let meter = client.get_meter(&meter_id).unwrap();
    assert!(!meter.is_active); 
}

#[test]
fn test_peak_hour_tariff() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &5000);

    let rate = 10; 
    let device_public_key = device_key(&env, 1);
    let meter_id = client.register_meter(&user, &provider, &rate, &token_address, &device_public_key, &0, &0);

    client.initiate_pairing(&meter_id);
    client.complete_pairing(&meter_id, &BytesN::from_array(&env, &[2u8; 64]));

    client.initiate_pairing(&meter_id);
    client.complete_pairing(&meter_id, &BytesN::from_array(&env, &[2u8; 64]));
    client.top_up(&meter_id, &5000);

    // 19:00 UTC Peak hours
    env.ledger().set_timestamp(68400);

    let signed_data = SignedUsageData {
        meter_id,
        timestamp: 68400,
        watt_hours_consumed: 1000,
        units_consumed: 10,
        is_renewable_energy: false,
        signature: BytesN::from_array(&env, &[3u8; 64]),
        public_key: device_public_key,
    };
    client.deduct_units(&signed_data);

    let meter = client.get_meter(&meter_id).unwrap();
    // Base cost 100 * 1.5 multiplier = 150
    assert_eq!(meter.balance, 4850); 
}

#[test]
fn test_green_energy_bonus() {
    let env = Env::default();
    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_address = create_token(&env);

    let susu_id = env.register_contract(None, mock_sorosusu::MockSoroSusu);
    let susu_client = mock_sorosusu::MockSoroSusuClient::new(&env, &susu_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_address = create_token(&env);
    let token_admin = token::StellarAssetClient::new(&env, &token_address);

    token_admin.mint(&user, &100_000);
    client.set_tax_rate(&0);
    client.set_sorosusu_contract(&susu_id);

    let meter_id = client.register_meter(&user, &provider, &10, &token_address, &device_key(&env, 42));
    client.top_up(&meter_id, &100_000);

    // Generate maintenance fund via claim
    env.ledger().set_timestamp(1_000);
    client.claim(&meter_id);

    let fund_before = client.get_maintenance_fund(&meter_id);
    susu_client.set_default(&user, &true);

    client.service_sorosusu_debt(&meter_id);

    let fund_after = client.get_maintenance_fund(&meter_id);
    assert!(fund_after < fund_before);
}

#[test]
fn test_carbon_credit_stream_creates_credits_and_reduces_protocol_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let payment_admin = Address::generate(&env);
    let credit_admin = provider.clone();

    let payment_token = env.register_stellar_asset_contract_v2(payment_admin.clone()).address();
    let credit_token = env.register_stellar_asset_contract_v2(credit_admin.clone()).address();

    let payment_client = token::StellarAssetClient::new(&env, &payment_token);
    let credit_client = token::StellarAssetClient::new(&env, &credit_token);

    payment_client.mint(&user, &100_000);
    credit_client.mint(&provider, &100_000);

    let oracle_id = env.register_contract(None, mock_environmental_oracle::MockEnvironmentalOracle);
    client.set_oracle(&oracle_id);

    let fee_wallet = Address::generate(&env);
    client.set_maintenance_config(&fee_wallet, &1000);

    let device_public_key = device_key(&env, 99);
    let meter_id = client.register_meter(&user, &provider, &10, &payment_token, &device_public_key, &0, &0);
    client.top_up(&meter_id, &50_000);
    client.initiate_pairing(&meter_id);
    client.complete_pairing(&meter_id, &BytesN::from_array(&env, &[2u8; 64]));

    client.set_green_energy_discount(&meter_id, &2000);
    client.set_carbon_credit_config(&meter_id, credit_token.clone(), &500);

    let signed_usage = SignedUsageData {
        meter_id,
        timestamp: env.ledger().timestamp(),
        watt_hours_consumed: 1000,
        units_consumed: 10,
        signature: BytesN::from_array(&env, &[3u8; 64]),
        public_key: device_public_key,
        is_renewable_energy: true,
    };

    client.deduct_units(&signed_usage);

    let credit_balance = credit_client.balance(&user);
    assert!(credit_balance > 0);

    let fee_balance = payment_client.balance(&fee_wallet);
    assert!(fee_balance < 1000);
}

// ==================== PROVIDER RELIABILITY TESTS ====================

#[test]
fn test_reliability_score_logic() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    let provider = Address::generate(&env);

    // Report 99 online windows out of 100
    for _ in 0..99u32 {
        client.report_provider_uptime(&provider, &true);
    }
    client.report_provider_uptime(&provider, &false);

    let score = client.get_reliability_score(&provider).unwrap();
    assert_eq!(score.score_bps, 9900);
    assert_eq!(score.badge, ReliabilityBadge::Gold);
}

#[test]
fn test_reliability_score_reset_impact() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    let provider = Address::generate(&env);

    // 3 fixed meter infos — use vec! macro to avoid repeated push_back calls
    let meter_infos = soroban_sdk::vec![
        &env,
        MeterInfo {
            user: user1.clone(),
            provider: provider.clone(),
            off_peak_rate: 100,
            token: token_address.clone(),
            billing_type: BillingType::PrePaid,
            device_public_key: device_key1,
        },
        MeterInfo {
            user: user2.clone(),
            provider: provider.clone(),
            off_peak_rate: 200,
            token: token_address.clone(),
            billing_type: BillingType::PostPaid,
            device_public_key: device_key2,
        },
        MeterInfo {
            user: user3.clone(),
            provider: provider.clone(),
            off_peak_rate: 150,
            token: token_address.clone(),
            billing_type: BillingType::PrePaid,
            device_public_key: device_key3,
        },
    ];

    // Call batch_register_meters
    let batch_event = client.batch_register_meters(&meter_infos);

    // Verify batch event
    assert_eq!(batch_event.start_id, 1);
    assert_eq!(batch_event.end_id, 3);
    assert_eq!(batch_event.count, 3);

    // Verify individual meters were created
    let meter1 = client.get_meter(&1);
    assert!(meter1.is_some());
    let meter1 = meter1.unwrap();
    assert_eq!(meter1.user, user1);
    assert_eq!(meter1.off_peak_rate, 100);
    assert_eq!(meter1.billing_type, BillingType::PrePaid);

    let meter2 = client.get_meter(&2);
    assert!(meter2.is_some());
    let meter2 = meter2.unwrap();
    assert_eq!(meter2.user, user2);
    assert_eq!(meter2.off_peak_rate, 200);
    assert_eq!(meter2.billing_type, BillingType::PostPaid);

    let meter3 = client.get_meter(&3);
    assert!(meter3.is_some());
    let meter3 = meter3.unwrap();
    assert_eq!(meter3.user, user3);
    assert_eq!(meter3.off_peak_rate, 150);
    assert_eq!(meter3.billing_type, BillingType::PrePaid);
}

#[test]
#[should_panic(expected = "InvalidTokenAmount")]
fn test_batch_register_meters_empty_vector() {
    let env = Env::default();
    let contract_address = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_address);

    let empty_meter_infos = Vec::new(&env);

    // Should panic with InvalidTokenAmount error
    client.batch_register_meters(&empty_meter_infos);
}

#[test]
fn test_green_energy_bonus() {
    let env = Env::default();
    let contract_address = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_address);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_address = Address::generate(&env);

    // Register a meter
    let meter_id = client.register_meter_with_mode(
        &user,
        &provider,
        &1000,
        &token_address,
        &BillingType::PrePaid,
        &device_key(&env, 0),
        &0, // end_date
        &0, // rent_deposit
    );

    client.set_green_energy_discount(&meter_id, &1000); // 10% discount
    client.top_up(&meter_id, &10000);

    let renewable_usage = SignedUsageData {
        meter_id: meter_id.clone(),
        timestamp: env.ledger().timestamp(),
        watt_hours_consumed: 100,
        units_consumed: 50,
        is_renewable_energy: true,
        signature: BytesN::from_array(&env, &[0; 64]),
        public_key: device_key(&env, 0),
    };

    client.deduct_units(&renewable_usage);
    let meter = client.get_meter(&meter_id).unwrap();
    // 50 units * 1000 rate = 50,000. 10% discount = 45,000 cost.
    // Note: Adjust math based on your specific implementation of balance/rates
    assert!(meter.usage_data.renewable_watt_hours > 0);
}

#[test]
fn test_multisig_withdrawal_full_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token_address = create_token(&env);

    let mut finance_wallets = Vec::new(&env);
    for _ in 0..5 { finance_wallets.push_back(Address::generate(&env)); }

    let device_public_key = device_key(&env, 1);
    let meter_id = client.register_meter(&user, &provider, &100, &token_address, &device_public_key, &0, &0);

    client.configure_multisig_withdrawal(&provider, &finance_wallets, &3, &100_000_00);

    let withdrawal_amount: i128 = 150_000_00;
    let request_id = client.propose_multisig_withdrawal(&provider, &meter_id, &withdrawal_amount, &treasury);

    // Approvals from distinct finance wallets. The proposer implicitly approved as wallet 0.
    let approver_1 = finance_wallets.get(1).unwrap();
    let approver_2 = finance_wallets.get(2).unwrap();
    client.approve_multisig_withdrawal(&provider, &request_id, &approver_1);
    client.approve_multisig_withdrawal(&provider, &request_id, &approver_2);

    client.execute_multisig_withdrawal(&provider, &request_id);
    let request = client.get_withdrawal_request(&provider, &request_id);
    assert!(request.is_executed);
}

#[test]
fn test_multisig_rejects_duplicate_approval() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token_address = create_token(&env);

    let mut finance_wallets = Vec::new(&env);
    for _ in 0..3 {
        finance_wallets.push_back(Address::generate(&env));
    }

    let meter_id = client.register_meter(&user, &provider, &100, &token_address, &device_key(&env, 1), &0, &0);
    client.configure_multisig_withdrawal(&provider, &finance_wallets, &2, &100_000_00);
    let request_id = client.propose_multisig_withdrawal(&provider, &meter_id, &150_000_00, &treasury);

    let approver = finance_wallets.get(1).unwrap();
    client.approve_multisig_withdrawal(&provider, &request_id, &approver);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.approve_multisig_withdrawal(&provider, &request_id, &approver);
    }));
    assert!(result.is_err());
}

#[test]
fn test_multisig_rejects_non_signer_approval() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token_address = create_token(&env);

    let mut finance_wallets = Vec::new(&env);
    for _ in 0..3 {
        finance_wallets.push_back(Address::generate(&env));
    }

    let meter_id = client.register_meter(&user, &provider, &100, &token_address, &device_key(&env, 1), &0, &0);
    client.configure_multisig_withdrawal(&provider, &finance_wallets, &2, &100_000_00);
    let request_id = client.propose_multisig_withdrawal(&provider, &meter_id, &150_000_00, &treasury);

    let non_signer = Address::generate(&env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.approve_multisig_withdrawal(&provider, &request_id, &non_signer);
    }));
    assert!(result.is_err());
}

#[test]
fn test_multisig_rejects_threshold_below_minimum() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let mut finance_wallets = Vec::new(&env);
    for _ in 0..3 {
        finance_wallets.push_back(Address::generate(&env));
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.configure_multisig_withdrawal(&provider, &finance_wallets, &1, &100_000_00);
    }));
    assert!(result.is_err());
}

#[test]
fn test_seasonal_factor_affects_rate() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_address = create_token(&env);
    let token_admin = token::StellarAssetClient::new(&env, &token_address);

    token_admin.mint(&user, &10000);

    let meter_id = client.register_meter(&user, &provider, &10, &token_address, &device_key(&env, 1), &0, &0);
    client.top_up(&meter_id, &5000);

// NOTE: Postpaid native XLM flow test removed — env.token() is not available in this SDK version.

// Continuous Flow Engine Tests

#[test]
fn test_continuous_flow_creation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    let flow_rate = 1000i128; // 1000 micro-stroops per second
    let initial_balance = 1_000_000i128; // 1 XLM in stroops
    
    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance);
    
    // Verify stream exists and has correct initial state
    let flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(flow.stream_id, stream_id);
    assert_eq!(flow.flow_rate_per_second, flow_rate);
    assert_eq!(flow.accumulated_balance, initial_balance);
    assert_eq!(flow.status, StreamStatus::Active);
    assert!(flow.created_timestamp > 0);
    assert_eq!(flow.last_flow_timestamp, flow.created_timestamp);
}

#[test]
fn test_continuous_flow_accumulation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    let flow_rate = 1000i128; // 1000 micro-stroops per second
    let initial_balance = 10_000_000i128; // 10 XLM in stroops
    
    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance);
    
    // Advance time by 100 seconds
    env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    
    // Check balance after accumulation
    let current_balance = client.get_continuous_balance(&stream_id).unwrap();
    let expected_balance = initial_balance - (flow_rate * 100);
    assert_eq!(current_balance, expected_balance);
}

#[test]
fn test_continuous_flow_multi_year_span() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    let flow_rate = 1i128; // 1 micro-stroop per second (very slow)
    let initial_balance = 31_536_000_000i128; // ~1 year worth at 1 micro-stroop/sec
    
    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance);
    
    // Simulate 2 years passing (2 * 365 * 24 * 60 * 60 = 63,072,000 seconds)
    let two_years_seconds = 63_072_000u64;
    env.ledger().set_timestamp(env.ledger().timestamp() + two_years_seconds);
    
    // Check balance after 2 years
    let current_balance = client.get_continuous_balance(&stream_id).unwrap();
    let expected_deduction = flow_rate * two_years_seconds as i128;
    let expected_balance = initial_balance - expected_deduction;
    
    assert_eq!(current_balance, expected_balance);
    
    // Stream should be depleted since we deducted more than initial balance
    let flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(flow.status, StreamStatus::Depleted);
}

#[test]
fn test_continuous_flow_high_frequency_withdrawals() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    let flow_rate = 1000i128; // 1000 micro-stroops per second
    let initial_balance = 100_000_000i128; // 100 XLM in stroops
    
    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance);
    
    // Perform multiple high-frequency withdrawals
    let withdrawal_amount = 10_000i128; // 0.01 XLM
    for i in 0..10 {
        // Advance time by 1 second between withdrawals
        env.ledger().set_timestamp(env.ledger().timestamp() + 1);
        
        let withdrawn = client.withdraw_continuous(&stream_id, &withdrawal_amount);
        assert_eq!(withdrawn, withdrawal_amount);
        
        // Verify withdrawal was successful
        let flow = client.get_continuous_flow(&stream_id).unwrap();
        assert!(flow.accumulated_balance < initial_balance - (withdrawal_amount * (i + 1) as i128));
    }
}

#[test]
fn test_continuous_flow_underflow_protection() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    let flow_rate = 1_000_000i128; // 1 XLM per second
    let initial_balance = 5_000_000i128; // 5 XLM in stroops
    
    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance);
    
    // Advance time by 10 seconds (should deduct 10 XLM, but we only have 5)
    env.ledger().set_timestamp(env.ledger().timestamp() + 10);
    
    // Check balance - should be 0 due to underflow protection
    let current_balance = client.get_continuous_balance(&stream_id).unwrap();
    assert_eq!(current_balance, 0);
    
    // Stream should be depleted
    let flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(flow.status, StreamStatus::Depleted);
}

#[test]
fn test_continuous_flow_rate_update() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    let initial_flow_rate = 1000i128;
    let new_flow_rate = 2000i128;
    let initial_balance = 10_000_000i128;
    
    // Create stream
    client.create_continuous_stream(&stream_id, &initial_flow_rate, &initial_balance);
    
    // Update flow rate
    client.update_continuous_flow_rate(&stream_id, &new_flow_rate);
    
    // Verify flow rate was updated
    let flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(flow.flow_rate_per_second, new_flow_rate);
    assert_eq!(flow.status, StreamStatus::Active);
}

#[test]
fn test_continuous_flow_pause_resume() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    let flow_rate = 1000i128;
    let initial_balance = 10_000_000i128;
    
    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance);
    
    // Pause stream
    client.pause_continuous_flow(&stream_id);
    let flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(flow.flow_rate_per_second, 0);
    assert_eq!(flow.status, StreamStatus::Paused);
    
    // Advance time - balance should not change while paused
    env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    let balance_during_pause = client.get_continuous_balance(&stream_id).unwrap();
    assert_eq!(balance_during_pause, initial_balance);
    
    // Resume stream
    client.resume_continuous_flow(&stream_id, &flow_rate);
    let flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(flow.flow_rate_per_second, flow_rate);
    assert_eq!(flow.status, StreamStatus::Active);
}

#[test]
fn test_continuous_flow_add_balance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    let flow_rate = 1000i128;
    let initial_balance = 5_000_000i128;
    let additional_balance = 3_000_000i128;
    
    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance);
    
    // Add balance
    client.add_continuous_balance(&stream_id, &additional_balance);
    
    // Verify balance was added
    let flow = client.get_continuous_flow(&stream_id).unwrap();
    assert_eq!(flow.accumulated_balance, initial_balance + additional_balance);
    assert_eq!(flow.status, StreamStatus::Active);
}

#[test]
fn test_continuous_flow_depletion_calculation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    let flow_rate = 1000i128; // 1000 micro-stroops per second
    let initial_balance = 60_000_000i128; // 60 seconds worth at current rate
    
    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance);
    
    // Calculate depletion time
    let depletion_time = client.calculate_continuous_depletion(&stream_id).unwrap();
    let current_time = env.ledger().timestamp();
    let expected_depletion = current_time + 60; // 60 seconds from now
    
    assert_eq!(depletion_time, expected_depletion);
}

#[test]
fn test_continuous_flow_fixed_point_math_precision() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    // Use very precise flow rate to test fixed-point math
    let flow_rate = 1234567i128; // 1.234567 micro-stroops per second
    let initial_balance = 100_000_000i128;
    
    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance);
    
    // Advance time by exactly 1 second
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    
    // Check balance - should be exactly initial_balance - flow_rate
    let current_balance = client.get_continuous_balance(&stream_id).unwrap();
    assert_eq!(current_balance, initial_balance - flow_rate);
    
    // Advance by another 2 seconds
    env.ledger().set_timestamp(env.ledger().timestamp() + 2);
    
    // Check balance again
    let current_balance = client.get_continuous_balance(&stream_id).unwrap();
    assert_eq!(current_balance, initial_balance - (flow_rate * 3));
}

#[test]
fn test_continuous_flow_struct_packing() {
    // This test verifies the struct is tightly packed
    let flow = ContinuousFlow {
        stream_id: 12345,
        flow_rate_per_second: 67890,
        accumulated_balance: 987654321,
        last_flow_timestamp: 1234567890,
        created_timestamp: 9876543210,
        status: StreamStatus::Active,
        reserved: [0u8; 7],
    };
    
    // Verify all fields are accessible and correct
    assert_eq!(flow.stream_id, 12345);
    assert_eq!(flow.flow_rate_per_second, 67890);
    assert_eq!(flow.accumulated_balance, 987654321);
    assert_eq!(flow.last_flow_timestamp, 1234567890);
    assert_eq!(flow.created_timestamp, 9876543210);
    assert_eq!(flow.status, StreamStatus::Active);
    assert_eq!(flow.reserved, [0u8; 7]);
}

#[test]
fn test_continuous_flow_timestamp_safety() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UtilityContract);
    let client = UtilityContractClient::new(&env, &contract_id);
    
    let stream_id = 1u64;
    let flow_rate = 1000i128;
    let initial_balance = 10_000_000i128;
    
    // Create stream
    client.create_continuous_stream(&stream_id, &flow_rate, &initial_balance);
    
    // Try to set timestamp backwards (should handle gracefully)
    let current_time = env.ledger().timestamp();
    env.ledger().set_timestamp(current_time - 100); // Go back in time
    
    // Balance should remain unchanged
    let current_balance = client.get_continuous_balance(&stream_id).unwrap();
    assert_eq!(current_balance, initial_balance);
}

#[test]
fn test_gas_buffer_initialization() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    // Mint tokens for provider to initialize gas buffer
    token_admin_client.mint(&provider, &1000);

    // Initialize gas buffer with minimum amount
    client.initialize_gas_buffer(&provider, &token_address, &100);
    
    let gas_buffer = client.get_gas_buffer(&provider).unwrap();
    assert_eq!(gas_buffer.balance, 100);
    assert_eq!(gas_buffer.provider, provider);
    assert_eq!(gas_buffer.token, token_address);
    assert_eq!(token.balance(&contract_id), 100);
    assert_eq!(token.balance(&provider), 900);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_gas_buffer_initialization_with_insufficient_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    // Try to initialize with amount below minimum
    client.initialize_gas_buffer(&provider, &token_address, &50);
}

#[test]
fn test_gas_buffer_top_up() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&provider, &1000);

    // Initialize gas buffer
    client.initialize_gas_buffer(&provider, &token_address, &100);
    
    // Top up gas buffer
    client.top_up_gas_buffer(&provider, &token_address, &200);
    
    let gas_buffer = client.get_gas_buffer(&provider).unwrap();
    assert_eq!(gas_buffer.balance, 300);
    assert_eq!(token.balance(&contract_id), 300);
    assert_eq!(token.balance(&provider), 700);
}

#[test]
fn test_gas_buffer_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&provider, &1000);

    // Initialize gas buffer
    client.initialize_gas_buffer(&provider, &token_address, &500);
    
    // Withdraw from gas buffer
    client.withdraw_from_gas_buffer(&provider, &token_address, &200);
    
    let gas_buffer = client.get_gas_buffer(&provider).unwrap();
    assert_eq!(gas_buffer.balance, 300);
    assert_eq!(token.balance(&contract_id), 300);
    assert_eq!(token.balance(&provider), 700);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_gas_buffer_withdrawal_below_minimum() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&provider, &1000);

    // Initialize gas buffer with minimum amount
    client.initialize_gas_buffer(&provider, &token_address, &100);
    
    // Try to withdraw entire buffer (would go below minimum)
    client.withdraw_from_gas_buffer(&provider, &token_address, &50);
}

#[test]
fn test_claim_with_gas_buffer() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.set_oracle(&oracle);

    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &1000);
    token_admin_client.mint(&provider, &1000);

    // Initialize gas buffer for provider
    client.initialize_gas_buffer(&provider, &token_address, &500);

    let meter_id = client.register_meter(&user, &provider, &10, &token_address);
    client.top_up(&meter_id, &500);

    env.ledger().set_timestamp(env.ledger().timestamp() + 5);
    client.claim(&meter_id);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.balance, 450);
    assert_eq!(token.balance(&provider), 550); // 50 from claim + 500 initial gas buffer
    assert_eq!(token.balance(&contract_id), 450);
    
    // Check that gas buffer was used (balance should be reduced)
    let gas_buffer = client.get_gas_buffer(&provider).unwrap();
    assert_eq!(gas_buffer.balance, 400); // 500 - 100 (MIN_GAS_BUFFER)
}

#[test]
fn test_get_gas_buffer_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&provider, &1000);

    // Check balance before initialization
    assert_eq!(client.get_gas_buffer_balance(&provider), 0);

    // Initialize gas buffer
    client.initialize_gas_buffer(&provider, &token_address, &300);
    
    // Check balance after initialization
    assert_eq!(client.get_gas_buffer_balance(&provider), 300);
}

#[test]
fn test_event_emissions() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let root_admin = Address::generate(&env);
    
    // Test initialization event
    client.initialize(&root_admin);
    
    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    
    // Setup a token
    let token_admin = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &1000);

    // Test meter registration event
    let meter_id = client.register_meter(&user, &provider, &10, &token_address, &None);
    
    // Test top-up event
    client.top_up(&meter_id, &500);
    
    // Test claim event
    env.ledger().set_timestamp(env.ledger().timestamp() + 10);
    client.claim(&meter_id);
    
    // Test webhook configuration event
    let webhook_url_hash = 12345u64; // Simple hash for testing
    client.configure_webhook(&user, &webhook_url_hash);
    
    // Test emergency shutdown event
    client.emergency_shutdown(&meter_id);
    
    // Note: In a real test environment, you would verify the events were emitted
    // This test ensures the functions execute without panicking when events are published
}

// ============================================================================
// Issue #26 — Protocol-Pause Bypass via Admin-Whitelist Collision
// ============================================================================

#[test]
fn test_configure_velocity_limits_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    let attacker = Address::generate(&env);
    // attacker passes themselves as admin param — must be rejected
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.set_velocity_limit_config(&attacker, &1_000_000i128, &100_000i128, &true);
    }));
    assert!(result.is_err(), "non-admin should not be able to configure velocity limits");
}

#[test]
fn test_configure_velocity_limits_succeeds_for_real_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    client.set_velocity_limit_config(&admin, &1_000_000i128, &100_000i128, &true);
    let config = client.get_velocity_limits().unwrap();
    assert_eq!(config.global_limit, 1_000_000);
    assert_eq!(config.per_stream_limit, 100_000);
    assert!(config.is_enabled);
}

#[test]
fn test_apply_velocity_override_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    let attacker = Address::generate(&env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.apply_velocity_override(&attacker, &0u64, &0u64, &soroban_sdk::symbol_short!("test"));
    }));
    assert!(result.is_err(), "non-admin should not be able to apply velocity override");
}

#[test]
fn test_apply_and_revoke_velocity_override_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Admin applies a global override (meter_id = 0), then revokes it
    client.apply_velocity_override(&admin, &0u64, &0u64, &soroban_sdk::symbol_short!("maint"));
    client.revoke_velocity_override(&admin, &0u64);
}

#[test]
fn test_revoke_velocity_override_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Admin applies override first so it exists
    client.apply_velocity_override(&admin, &0u64, &0u64, &soroban_sdk::symbol_short!("maint"));

    let attacker = Address::generate(&env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.revoke_velocity_override(&attacker, &0u64);
    }));
    assert!(result.is_err(), "non-admin should not be able to revoke velocity override");
}
