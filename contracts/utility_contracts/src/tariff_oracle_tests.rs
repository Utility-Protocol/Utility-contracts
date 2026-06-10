extern crate std;

use soroban_sdk::{testutils::Address as TestAddress, testutils::BytesN as TestBytesN, Env, Address, Vec};
use crate::tariff_oracle::{
    TariffOracle, DailyTariffSchedule, HourlyTariff, TariffTier, TariffUpdateProposal,
    FlowCalculationResult, TARIFF_NOTICE_PERIOD, HOURS_IN_DAY
};
use crate::{ContractError, DataKey};

#[cfg(test)]
pub mod tariff_oracle_tests {
    use super::*;

    /// Test basic tariff oracle initialization
    #[test]
    fn test_tariff_oracle_initialization() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        let grid_admin = TestAddress::random(&env);
        let initial_schedule = create_test_schedule(&env);

        // Initialize oracle
        TariffOracle::initialize(env.clone(), grid_admin.clone(), initial_schedule.clone());

        // Verify configuration
        assert!(TariffOracle::is_configured(env.clone()));
        assert_eq!(TariffOracle::get_grid_admin(env.clone()), grid_admin);

        // Verify schedule
        let stored_schedule = TariffOracle::get_current_schedule(env.clone());
        assert_eq!(stored_schedule.schedule_date, initial_schedule.schedule_date);
    }

    /// Test tariff update proposal with notice period
    #[test]
    fn test_tariff_update_proposal() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        let grid_admin = TestAddress::random(&env);
        let initial_schedule = create_test_schedule(&env);

        // Initialize oracle
        TariffOracle::initialize(env.clone(), grid_admin.clone(), initial_schedule);

        // Create new schedule proposal
        let new_schedule = create_peak_schedule(&env);
        let admin_signature = TestBytesN::random(&env);

        // Submit proposal
        let proposal_id = TariffOracle::propose_tariff_update(
            env.clone(),
            new_schedule.clone(),
            admin_signature,
        );

        // Verify proposal exists and is not executable yet
        let proposal = TariffOracle::get_tariff_proposal(env.clone(), proposal_id);
        assert!(!proposal.is_executed);
        assert_eq!(proposal.new_schedule.schedule_date, new_schedule.schedule_date);
        
        let current_time = env.ledger().timestamp();
        assert!(proposal.executable_at > current_time);
        assert_eq!(proposal.executable_at, current_time + TARIFF_NOTICE_PERIOD);
    }

    /// Test tariff update execution after notice period
    #[test]
    fn test_tariff_update_execution() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        let grid_admin = TestAddress::random(&env);
        let initial_schedule = create_test_schedule(&env);

        // Initialize oracle
        TariffOracle::initialize(env.clone(), grid_admin.clone(), initial_schedule);

        // Create and submit proposal
        let new_schedule = create_peak_schedule(&env);
        let admin_signature = TestBytesN::random(&env);
        let proposal_id = TariffOracle::propose_tariff_update(
            env.clone(),
            new_schedule.clone(),
            admin_signature,
        );

        // Try to execute before notice period (should fail)
        let result = std::panic::catch_unwind(|| {
            TariffOracle::execute_tariff_update(env.clone(), proposal_id);
        });
        assert!(result.is_err());

        // Advance time past notice period
        env.ledger().set_timestamp(env.ledger().timestamp() + TARIFF_NOTICE_PERIOD + 1);

        // Execute proposal (should succeed)
        TariffOracle::execute_tariff_update(env.clone(), proposal_id);

        // Verify schedule was updated
        let current_schedule = TariffOracle::get_current_schedule(env.clone());
        assert_eq!(current_schedule.schedule_date, new_schedule.schedule_date);

        // Verify proposal is marked as executed
        let proposal = TariffOracle::get_tariff_proposal(env.clone(), proposal_id);
        assert!(proposal.is_executed);
    }

    /// Test current flow rate calculation
    #[test]
    fn test_current_flow_rate_calculation() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        let grid_admin = TestAddress::random(&env);
        let initial_schedule = create_test_schedule(&env);

        // Initialize oracle
        TariffOracle::initialize(env.clone(), grid_admin, initial_schedule);

        // Test flow rate calculation
        let consumption_rate = 1000i128; // 1 kWh per second (scaled)
        let flow_rate = TariffOracle::calculate_current_flow_rate(env.clone(), consumption_rate);
        
        // Flow rate should be positive
        assert!(flow_rate > 0);

        // Test with different consumption rates
        let high_consumption = 5000i128;
        let high_flow_rate = TariffOracle::calculate_current_flow_rate(env.clone(), high_consumption);
        assert!(high_flow_rate > flow_rate);
    }

    /// Test flow calculation for period spanning multiple tariff windows
    /// This is the specific test case from Issue #261
    #[test]
    fn test_stream_spanning_midnight_blended_rate() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        let grid_admin = TestAddress::random(&env);
        let initial_schedule = create_test_schedule(&env);

        // Initialize oracle
        TariffOracle::initialize(env.clone(), grid_admin, initial_schedule);

        // Set up time period from 11:59 PM to 12:01 AM (spans midnight)
        let start_timestamp = 23 * 3600 + 59 * 60; // 23:59
        let end_timestamp = 24 * 3600 + 1 * 60;    // 00:01 next day
        let consumption_rate = 1000i128; // 1 kWh per second

        // Calculate flow for the period
        let result = TariffOracle::calculate_flow_for_period(
            env.clone(),
            start_timestamp,
            end_timestamp,
            consumption_rate,
        );

        // Verify results
        assert_eq!(result.duration_seconds, 120); // 2 minutes
        assert!(result.spanned_multiple_windows);
        assert!(result.windows_crossed >= 1);
        assert!(result.total_tokens > 0);

        // The weighted rate should be between the two tariff rates
        let tariff_23 = TariffOracle::get_current_tariff(env.clone(), 23);
        let tariff_0 = TariffOracle::get_current_tariff(env.clone(), 0);
        
        let expected_min_rate = tariff_23.rate_cents_per_kwh.min(tariff_0.rate_cents_per_kwh);
        let expected_max_rate = tariff_23.rate_cents_per_kwh.max(tariff_0.rate_cents_per_kwh);
        
        assert!(result.weighted_rate_per_second >= expected_min_rate);
        assert!(result.weighted_rate_per_second <= expected_max_rate);
    }

    /// Test flow calculation within single tariff window
    #[test]
    fn test_flow_within_single_window() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        let grid_admin = TestAddress::random(&env);
        let initial_schedule = create_test_schedule(&env);

        // Initialize oracle
        TariffOracle::initialize(env.clone(), grid_admin, initial_schedule);

        // Set up time period within same hour
        let start_timestamp = 10 * 3600; // 10:00 AM
        let end_timestamp = 10 * 3600 + 1800; // 10:30 AM
        let consumption_rate = 1000i128;

        // Calculate flow
        let result = TariffOracle::calculate_flow_for_period(
            env.clone(),
            start_timestamp,
            end_timestamp,
            consumption_rate,
        );

        // Verify results
        assert_eq!(result.duration_seconds, 1800); // 30 minutes
        assert!(!result.spanned_multiple_windows);
        assert_eq!(result.windows_crossed, 0);

        // Weighted rate should equal the single tariff rate
        let tariff_10 = TariffOracle::get_current_tariff(env.clone(), 10);
        assert_eq!(result.weighted_rate_per_second, tariff_10.rate_cents_per_kwh);
    }

    /// Test tariff schedule validation
    #[test]
    fn test_tariff_schedule_validation() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        let grid_admin = TestAddress::random(&env);

        // Test invalid schedule (wrong number of hours)
        let invalid_schedule = create_invalid_schedule(&env);
        let result = std::panic::catch_unwind(|| {
            TariffOracle::initialize(env.clone(), grid_admin.clone(), invalid_schedule);
        });
        assert!(result.is_err());

        // Test schedule with negative rates
        let negative_rate_schedule = create_schedule_with_negative_rates(&env);
        let result = std::panic::catch_unwind(|| {
            TariffOracle::initialize(env.clone(), grid_admin, negative_rate_schedule);
        });
        assert!(result.is_err());
    }

    /// Test default schedule fallback
    #[test]
    fn test_default_schedule_fallback() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        // Try to get tariff without initialization
        let tariff = TariffOracle::get_current_tariff(env.clone(), 12);
        
        // Should return default tariff
        assert!(tariff.rate_cents_per_kwh > 0);
        assert!(tariff.hour == 12);

        // Should not be configured
        assert!(!TariffOracle::is_configured(env.clone()));
    }

    /// Test temporary storage optimization
    #[test]
    fn test_temporary_storage_optimization() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        let grid_admin = TestAddress::random(&env);
        let initial_schedule = create_test_schedule(&env);

        // Initialize oracle
        TariffOracle::initialize(env.clone(), grid_admin, initial_schedule);

        // Verify schedule is stored in temporary storage
        let temp_schedule = env.storage().temporary().get::<DataKey, DailyTariffSchedule>(&DataKey::TodayTariffSchedule);
        assert!(temp_schedule.is_some());

        // Tariff queries should use temporary storage
        let tariff = TariffOracle::get_current_tariff(env.clone(), 15);
        assert!(tariff.rate_cents_per_kwh > 0);
    }

    /// Test renewable energy hours
    #[test]
    fn test_renewable_energy_hours() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        let grid_admin = TestAddress::random(&env);
        let initial_schedule = create_test_schedule(&env);

        // Initialize oracle
        TariffOracle::initialize(env.clone(), grid_admin, initial_schedule);

        // Check renewable hours (10-16 in our test schedule)
        for hour in 10..=16 {
            let tariff = TariffOracle::get_current_tariff(env.clone(), hour);
            assert!(tariff.is_renewable_hour);
        }

        // Check non-renewable hours
        for hour in 0..=9 {
            let tariff = TariffOracle::get_current_tariff(env.clone(), hour);
            assert!(!tariff.is_renewable_hour);
        }
    }

    /// Test multiple tariff window crossings
    #[test]
    fn test_multiple_window_crossings() {
        let env = Env::new();
        let contract_address = TestAddress::random(&env);
        env.register_contract(&contract_address, TariffOracle);

        let grid_admin = TestAddress::random(&env);
        let initial_schedule = create_test_schedule(&env);

        // Initialize oracle
        TariffOracle::initialize(env.clone(), grid_admin, initial_schedule);

        // Set up time period spanning multiple hours
        let start_timestamp = 8 * 3600; // 8:00 AM
        let end_timestamp = 14 * 3600;  // 2:00 PM
        let consumption_rate = 1000i128;

        // Calculate flow
        let result = TariffOracle::calculate_flow_for_period(
            env.clone(),
            start_timestamp,
            end_timestamp,
            consumption_rate,
        );

        // Verify multiple windows were crossed
        assert!(result.spanned_multiple_windows);
        assert!(result.windows_crossed >= 5); // Should cross at least 5 hour boundaries
        assert_eq!(result.duration_seconds, 6 * 3600); // 6 hours
    }

    /// Helper function to create peak-heavy tariff schedule
    fn create_peak_schedule(env: &Env) -> DailyTariffSchedule {
        let mut hourly_rates = Vec::new(env);
        
        for hour in 0..HOURS_IN_DAY {
            let (rate_cents, tier) = match hour {
                0..=5 | 22..=23 => (10, TariffTier::OffPeak),    // Night: off-peak
                6..=9 | 18..=21 => (25, TariffTier::Peak),       // Extended peak hours
                _ => (18, TariffTier::Standard),                  // Higher standard rate
            };
            
            hourly_rates.push_back(HourlyTariff {
                hour,
                rate_cents_per_kwh: rate_cents,
                tier,
                is_renewable_hour: matches!(hour, 10..=16),
            });
        }

        DailyTariffSchedule {
            hourly_rates,
            schedule_date: 20240102,
            signed_by: TestAddress::random(env),
            created_at: env.ledger().timestamp(),
            effective_at: env.ledger().timestamp(),
            admin_signature: TestBytesN::random(env),
        }
    }

    /// Helper function to create invalid schedule (wrong number of hours)
    fn create_invalid_schedule(env: &Env) -> DailyTariffSchedule {
        let mut hourly_rates = Vec::new(env);
        
        // Only create 20 hours instead of 24
        for hour in 0..20 {
            hourly_rates.push_back(HourlyTariff {
                hour,
                rate_cents_per_kwh: 10,
                tier: TariffTier::Standard,
                is_renewable_hour: false,
            });
        }

        DailyTariffSchedule {
            hourly_rates,
            schedule_date: 20240101,
            signed_by: TestAddress::random(env),
            created_at: env.ledger().timestamp(),
            effective_at: env.ledger().timestamp(),
            admin_signature: TestBytesN::random(env),
        }
    }

    /// Helper function to create schedule with negative rates
    fn create_schedule_with_negative_rates(env: &Env) -> DailyTariffSchedule {
        let mut hourly_rates = Vec::new(env);
        
        for hour in 0..HOURS_IN_DAY {
            hourly_rates.push_back(HourlyTariff {
                hour,
                rate_cents_per_kwh: -5, // Negative rate
                tier: TariffTier::Standard,
                is_renewable_hour: false,
            });
        }

        DailyTariffSchedule {
            hourly_rates,
            schedule_date: 20240101,
            signed_by: TestAddress::random(env),
            created_at: env.ledger().timestamp(),
            effective_at: env.ledger().timestamp(),
            admin_signature: TestBytesN::random(env),
        }
    }
}

/// Helper function to create test tariff schedule
pub(crate) fn create_test_schedule(env: &Env) -> DailyTariffSchedule {
    let mut hourly_rates = Vec::new(env);
    
    for hour in 0..HOURS_IN_DAY {
        let (rate_cents, tier, is_renewable) = match hour {
            0..=6 | 22..=23 => (8, TariffTier::OffPeak, false),
            7..=10 | 17..=20 => (15, TariffTier::Peak, false),
            11..=16 => (12, TariffTier::Standard, true),
            _ => (10, TariffTier::Standard, false),
        };
        
        hourly_rates.push_back(HourlyTariff {
            hour,
            rate_cents_per_kwh: rate_cents,
            tier,
            is_renewable_hour: is_renewable,
        });
    }

    DailyTariffSchedule {
        hourly_rates,
        schedule_date: 20240101,
        signed_by: TestAddress::random(env),
        created_at: env.ledger().timestamp(),
        effective_at: env.ledger().timestamp(),
        admin_signature: TestBytesN::random(env),
    }
}

/// Property-based tests for tariff calculations
#[cfg(test)]
mod tariff_property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn test_flow_calculation_properties(
            start_hour in 0u8..23u8,
            duration_minutes in 1u64..180u64, // 1 minute to 3 hours
            consumption_rate in 100i128..10000i128,
        ) {
            let env = Env::new();
            let contract_address = TestAddress::random(&env);
            env.register_contract(&contract_address, TariffOracle);

            let grid_admin = TestAddress::random(&env);
            let initial_schedule = create_test_schedule(&env);

            // Initialize oracle
            TariffOracle::initialize(env.clone(), grid_admin, initial_schedule);

            let start_timestamp = (start_hour as u64) * 3600;
            let end_timestamp = start_timestamp + duration_minutes * 60;

            // Calculate flow
            let result = TariffOracle::calculate_flow_for_period(
                env.clone(),
                start_timestamp,
                end_timestamp,
                consumption_rate,
            );

            // Property: duration should match input
            prop_assert_eq!(result.duration_seconds, duration_minutes * 60);

            // Property: total tokens should be positive
            prop_assert!(result.total_tokens > 0);

            // Property: weighted rate should be reasonable
            prop_assert!(result.weighted_rate_per_second > 0);
            prop_assert!(result.weighted_rate_per_second < 100000); // Sanity check

            // Property: windows_crossed should not exceed duration in hours
            let max_possible_windows = (duration_minutes / 60 + 1) as u8;
            prop_assert!(result.windows_crossed <= max_possible_windows);
        }
    }
}
