use soroban_sdk::{contracttype, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DegradationConfig {
    pub shedding_level: u32,             // 0 = Normal, 1 = Moderate, 2 = High, 3 = Critical
    pub max_active_streams: u32,         // Total active streams capacity
    pub is_zk_bypass_allowed: bool,      // Bypasses heavy cryptographic proof checks
    pub is_dust_sweeper_suspended: bool, // Suspends non-critical cleanup tasks
    pub is_postpaid_restricted: bool,    // Rejects high-risk postpaid accounts
    pub active_feature_flags: Vec<Symbol>, // Explicit list of active features
}

impl DegradationConfig {
    pub fn default(env: &Env) -> Self {
        let mut active_features = Vec::new(env);
        // Default active features
        active_features.push_back(Symbol::new(env, "billing"));
        active_features.push_back(Symbol::new(env, "liveness"));
        active_features.push_back(Symbol::new(env, "dust_sweeper"));
        active_features.push_back(Symbol::new(env, "zk_validation"));
        active_features.push_back(Symbol::new(env, "postpaid_streams"));

        Self {
            shedding_level: 0,
            max_active_streams: 10_000,
            is_zk_bypass_allowed: false,
            is_dust_sweeper_suspended: false,
            is_postpaid_restricted: false,
            active_feature_flags: active_features,
        }
    }
}

pub struct GracefulDegradation;

impl GracefulDegradation {
    /// Determines if a specific feature is currently enabled, taking the current
    /// capacity shedding level and custom feature flags list into account.
    pub fn is_feature_enabled(env: &Env, config: &DegradationConfig, feature: Symbol) -> bool {
        // Core features that can never be disabled to prevent total system failure
        let is_billing = feature == Symbol::new(env, "billing");
        let is_liveness = feature == Symbol::new(env, "liveness");
        if is_billing || is_liveness {
            return true;
        }

        // If shedding is CRITICAL, non-essential systems are shed aggressively
        if config.shedding_level >= 3 {
            if feature == Symbol::new(env, "dust_sweeper") && config.is_dust_sweeper_suspended {
                return false;
            }
            if feature == Symbol::new(env, "zk_validation") && config.is_zk_bypass_allowed {
                // If ZK bypass is allowed, we don't enforce strict ZK checks
                return false;
            }
        }

        if config.shedding_level >= 2 {
            if feature == Symbol::new(env, "postpaid_streams") && config.is_postpaid_restricted {
                return false;
            }
        }

        // Check if the feature flag is explicitly enabled using iterator
        let mut has_feature = false;
        for f in config.active_feature_flags.iter() {
            if f == feature {
                has_feature = true;
                break;
            }
        }
        has_feature
    }

    /// Verifies if the system has sufficient capacity for additional streams.
    pub fn check_capacity_limits(config: &DegradationConfig, current_count: u32) -> bool {
        // If shedding level is critical, restrict incoming traffic to 90% of capacity
        let limit = if config.shedding_level >= 3 {
            (config.max_active_streams * 9) / 10
        } else if config.shedding_level >= 2 {
            (config.max_active_streams * 95) / 100
        } else {
            config.max_active_streams
        };

        current_count < limit
    }

    /// Dynamically adjusts telemetry / reporting polling interval (in seconds).
    /// Sheds device reporting workload during network congestion.
    pub fn get_polling_interval_seconds(config: &DegradationConfig) -> u32 {
        match config.shedding_level {
            0 => 5,    // Normal high-frequency 5-second updates
            1 => 15,   // Moderate degradation: 15 seconds
            2 => 30,   // High degradation: 30 seconds
            _ => 120,  // Critical/extreme shedding: 120 seconds (2 mins)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, Symbol};

    #[test]
    fn test_default_config() {
        let env = Env::default();
        let config = DegradationConfig::default(&env);

        assert_eq!(config.shedding_level, 0);
        assert_eq!(config.max_active_streams, 10_000);
        assert!(!config.is_zk_bypass_allowed);
        assert!(!config.is_dust_sweeper_suspended);
        assert!(!config.is_postpaid_restricted);

        // Core and standard features should be enabled
        assert!(GracefulDegradation::is_feature_enabled(&env, &config, Symbol::new(&env, "billing")));
        assert!(GracefulDegradation::is_feature_enabled(&env, &config, Symbol::new(&env, "liveness")));
        assert!(GracefulDegradation::is_feature_enabled(&env, &config, Symbol::new(&env, "dust_sweeper")));
        assert!(GracefulDegradation::is_feature_enabled(&env, &config, Symbol::new(&env, "zk_validation")));
    }

    #[test]
    fn test_feature_shedding_level_critical() {
        let env = Env::default();
        let mut config = DegradationConfig::default(&env);

        // Elevate degradation to level 3 (Critical)
        config.shedding_level = 3;
        config.is_zk_bypass_allowed = true;
        config.is_dust_sweeper_suspended = true;

        // Core features MUST remain enabled
        assert!(GracefulDegradation::is_feature_enabled(&env, &config, Symbol::new(&env, "billing")));
        assert!(GracefulDegradation::is_feature_enabled(&env, &config, Symbol::new(&env, "liveness")));

        // Optional features are shed
        assert!(!GracefulDegradation::is_feature_enabled(&env, &config, Symbol::new(&env, "dust_sweeper")));
        assert!(!GracefulDegradation::is_feature_enabled(&env, &config, Symbol::new(&env, "zk_validation")));
    }

    #[test]
    fn test_capacity_limits() {
        let env = Env::default();
        let mut config = DegradationConfig::default(&env);
        config.max_active_streams = 1000;

        // Level 0 (Normal): full capacity
        assert!(GracefulDegradation::check_capacity_limits(&config, 999));
        assert!(!GracefulDegradation::check_capacity_limits(&config, 1000));

        // Level 2 (High): 95% capacity gate
        config.shedding_level = 2;
        assert!(GracefulDegradation::check_capacity_limits(&config, 949));
        assert!(!GracefulDegradation::check_capacity_limits(&config, 950));

        // Level 3 (Critical): 90% capacity gate
        config.shedding_level = 3;
        assert!(GracefulDegradation::check_capacity_limits(&config, 899));
        assert!(!GracefulDegradation::check_capacity_limits(&config, 900));
    }

    #[test]
    fn test_polling_intervals() {
        let env = Env::default();
        let mut config = DegradationConfig::default(&env);

        config.shedding_level = 0;
        assert_eq!(GracefulDegradation::get_polling_interval_seconds(&config), 5);

        config.shedding_level = 1;
        assert_eq!(GracefulDegradation::get_polling_interval_seconds(&config), 15);

        config.shedding_level = 2;
        assert_eq!(GracefulDegradation::get_polling_interval_seconds(&config), 30);

        config.shedding_level = 3;
        assert_eq!(GracefulDegradation::get_polling_interval_seconds(&config), 120);
    }

    #[test]
    fn test_property_monotonically_increasing_intervals() {
        // Randomized/property verification simulation:
        // Polling interval should always increase or remain equal as shedding_level increases.
        let env = Env::default();
        let mut config = DegradationConfig::default(&env);

        let mut last_interval = 0;
        for level in 0..=4 {
            config.shedding_level = level;
            let current_interval = GracefulDegradation::get_polling_interval_seconds(&config);
            assert!(current_interval >= last_interval, "Interval must be monotonic");
            last_interval = current_interval;
        }
    }
}
