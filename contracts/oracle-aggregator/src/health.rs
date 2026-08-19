//! Oracle health monitoring (issue #130, step 5).
//!
//! Every read attempt updates per-provider telemetry: success, failure, and
//! staleness counters plus last-success/failure timestamps and the most recent
//! value. Operators query [`crate::OracleAggregator::get_health`] and
//! [`crate::OracleAggregator::get_health_summary`] to detect a downed or
//! manipulated feed, and the contract publishes events on provider health
//! transitions so indexers can alert without polling.

use crate::constants::BPS_DENOMINATOR;
use crate::types::ProviderHealth;

/// A fresh, empty health record for `provider`.
pub fn default_health(provider: &soroban_sdk::Address) -> ProviderHealth {
    ProviderHealth {
        provider: provider.clone(),
        total_reads: 0,
        successful_reads: 0,
        failed_reads: 0,
        stale_reads: 0,
        last_success_at: 0,
        last_failure_at: 0,
        last_value: 0,
        is_healthy: false,
    }
}

/// Record a successful, fresh read at `now` returning `value`.
pub fn record_success(health: &mut ProviderHealth, now: u64, value: i128) {
    health.total_reads = health.total_reads.saturating_add(1);
    health.successful_reads = health.successful_reads.saturating_add(1);
    health.last_success_at = now;
    health.last_value = value;
    health.is_healthy = true;
}

/// Record a failed read (cross-contract error or invalid value) at `now`.
pub fn record_failure(health: &mut ProviderHealth, now: u64) {
    health.total_reads = health.total_reads.saturating_add(1);
    health.failed_reads = health.failed_reads.saturating_add(1);
    health.last_failure_at = now;
    health.is_healthy = false;
}

/// Record a read rejected for staleness at `now`.
pub fn record_stale(health: &mut ProviderHealth, now: u64) {
    health.total_reads = health.total_reads.saturating_add(1);
    health.stale_reads = health.stale_reads.saturating_add(1);
    health.last_failure_at = now;
    health.is_healthy = false;
}

/// Success rate as basis points (0 for a provider never read).
pub fn success_rate_bps(health: &ProviderHealth) -> u32 {
    if health.total_reads == 0 {
        return 0;
    }
    let numerator = health.successful_reads as u128 * BPS_DENOMINATOR as u128;
    (numerator / health.total_reads as u128) as u32
}

/// Whether the provider's most recent read succeeded.
pub fn is_healthy(health: &ProviderHealth) -> bool {
    health.is_healthy
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn default_health_is_unhealthy_and_zeroed() {
        let env = Env::default();
        let provider = soroban_sdk::Address::generate(&env);
        let h = default_health(&provider);
        assert_eq!(h.provider, provider);
        assert_eq!(h.total_reads, 0);
        assert!(!h.is_healthy);
        assert_eq!(success_rate_bps(&h), 0);
    }

    #[test]
    fn success_and_failure_transitions() {
        let env = Env::default();
        let provider = soroban_sdk::Address::generate(&env);
        let mut h = default_health(&provider);

        record_success(&mut h, 10, 123);
        assert!(h.is_healthy);
        assert_eq!(h.successful_reads, 1);
        assert_eq!(h.last_value, 123);
        assert_eq!(success_rate_bps(&h), 10_000);

        record_failure(&mut h, 20);
        assert!(!h.is_healthy);
        assert_eq!(h.failed_reads, 1);
        assert_eq!(success_rate_bps(&h), 5_000); // 1/2

        record_stale(&mut h, 30);
        assert!(!h.is_healthy);
        assert_eq!(h.stale_reads, 1);
        assert_eq!(h.total_reads, 3);
    }
}
