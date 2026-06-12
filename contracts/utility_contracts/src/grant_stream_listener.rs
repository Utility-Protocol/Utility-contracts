#![no_std]
use super::secure_call_interface::{SecureCallError, SecureCallManager};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, BytesN, Env, String, Symbol, Vec,
};

// --- Grant Stream Listener Contract ---
// This contract listens for GoalReached events from IoT-Billing-Service and processes grant matches

#[contracttype]
#[derive(Clone)]
pub struct GrantMatch {
    pub goal_id: u64,
    pub provider: Address,
    pub water_savings: i128,
    pub grant_amount: i128,
    pub grant_token: Address,
    pub achieved_at: u64,
    pub processed: bool,
    pub processed_at: Option<u64>,
    pub maintenance_months_covered: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct GrantConfig {
    pub admin: Address,
    pub treasury: Address,
    pub enabled: bool,
    pub max_grant_per_month: i128,
    pub total_granted: i128,
}

#[contracttype]
pub enum GrantDataKey {
    GrantMatch(u64),
    GrantConfig,
    MatchCount,
    ProviderTotalGrants(Address),
    MonthlyGrantLimit(u64, u32), // (year_month, provider_id)
}

#[contracterror]
#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum GrantError {
    GrantAlreadyProcessed = 1,
    GrantNotFound = 2,
    InsufficientTreasuryBalance = 3,
    GrantDisabled = 4,
    InvalidGrantAmount = 5,
    MonthlyLimitExceeded = 6,
    Unauthorized = 7,
}

#[contract]
pub struct GrantStreamListener;

#[contractimpl]
impl GrantStreamListener {
    /// Initialize the grant stream listener
    pub fn initialize(env: Env, admin: Address, treasury: Address) {
        if env
            .storage()
            .instance()
            .get::<_, GrantConfig>(&GrantDataKey::GrantConfig)
            .is_some()
        {
            panic_with_error!(&env, GrantError::GrantAlreadyProcessed);
        }

        let config = GrantConfig {
            admin: admin.clone(),
            treasury: treasury.clone(),
            enabled: true,
            max_grant_per_month: 1_000_000_00, // $10,000 USD in cents
            total_granted: 0,
        };

        env.storage()
            .instance()
            .set(&GrantDataKey::GrantConfig, &config);
        env.storage()
            .instance()
            .set(&GrantDataKey::MatchCount, &0u64);

        env.events()
            .publish((symbol_short!("GrantInit"),), (admin, treasury));
    }

    /// Called by IoT-Billing-Service when a conservation goal is reached
    pub fn on_goal_reached(
        env: Env,
        billing_contract: Address,
        goal_event: super::GoalReachedEvent,
    ) {
        let config: GrantConfig = env
            .storage()
            .instance()
            .get(&GrantDataKey::GrantConfig)
            .unwrap_or_else(|| panic_with_error!(&env, GrantError::GrantNotFound));

        if !config.enabled {
            panic_with_error!(&env, GrantError::GrantDisabled);
        }

        // Check if this grant has already been processed
        if let Some(existing_match) = env
            .storage()
            .instance()
            .get::<_, GrantMatch>(&GrantDataKey::GrantMatch(goal_event.goal_id))
        {
            if existing_match.processed {
                panic_with_error!(&env, GrantError::GrantAlreadyProcessed);
            }
        }

        // Verify the goal event by calling back to the utility drip contract using secure interface
        let mut args = Vec::new(&env);
        args.push_back(goal_event.goal_id.into());

        match SecureCallManager::secure_call::<super::ConservationGoal>(
            &env,
            &billing_contract,
            &Symbol::new(&env, "get_conservation_goal"),
            args,
            Some(20_000_000), // Conservative gas limit for goal verification
        ) {
            Ok(conservation_goal) => {
                // Verify the goal event matches the stored goal
                if conservation_goal.provider != goal_event.provider
                    || conservation_goal.target_water_savings != goal_event.water_savings
                    || !conservation_goal.is_active
                {
                    panic_with_error!(&env, GrantError::GrantNotFound);
                }
            }
            Err(_) => {
                panic_with_error!(&env, GrantError::GrantNotFound);
            }
        }

        // Calculate maintenance months covered (simplified: 1 month per $1000)
        let maintenance_months_covered = (goal_event.grant_amount / 100_000_00) as u32; // $1000 = 100,000 cents
        let months_to_cover = maintenance_months_covered.min(12); // Cap at 12 months

        // Check monthly grant limit
        let year_month = Self::get_year_month(env.ledger().timestamp());
        let monthly_limit_key =
            GrantDataKey::MonthlyGrantLimit(year_month, goal_event.goal_id as u32);
        let current_monthly_grants = env
            .storage()
            .instance()
            .get::<_, i128>(&monthly_limit_key)
            .unwrap_or(0);

        if current_monthly_grants + goal_event.grant_amount > config.max_grant_per_month {
            panic_with_error!(&env, GrantError::MonthlyLimitExceeded);
        }

        // Check treasury balance
        let token_client = token::Client::new(&env, &goal_event.grant_token);
        let treasury_balance = token_client.balance(&config.treasury);

        if treasury_balance < goal_event.grant_amount {
            panic_with_error!(&env, GrantError::InsufficientTreasuryBalance);
        }

        // Create grant match record
        let grant_match = GrantMatch {
            goal_id: goal_event.goal_id,
            provider: goal_event.provider.clone(),
            water_savings: goal_event.water_savings,
            grant_amount: goal_event.grant_amount,
            grant_token: goal_event.grant_token.clone(),
            achieved_at: goal_event.achieved_at,
            processed: true,
            processed_at: Some(env.ledger().timestamp()),
            maintenance_months_covered: months_to_cover,
        };

        // Store grant match
        env.storage()
            .instance()
            .set(&GrantDataKey::GrantMatch(goal_event.goal_id), &grant_match);

        // Update match count
        let mut count: u64 = env
            .storage()
            .instance()
            .get(&GrantDataKey::MatchCount)
            .unwrap_or(0);
        count += 1;
        env.storage()
            .instance()
            .set(&GrantDataKey::MatchCount, &count);

        // Update provider total grants
        let provider_total_key = GrantDataKey::ProviderTotalGrants(goal_event.provider.clone());
        let mut provider_total = env
            .storage()
            .instance()
            .get::<_, i128>(&provider_total_key)
            .unwrap_or(0);
        provider_total += goal_event.grant_amount;
        env.storage()
            .instance()
            .set(&provider_total_key, &provider_total);

        // Update monthly grants
        env.storage().instance().set(
            &monthly_limit_key,
            &(current_monthly_grants + goal_event.grant_amount),
        );

        // Update total granted
        let mut updated_config = config.clone();
        updated_config.total_granted += goal_event.grant_amount;
        env.storage()
            .instance()
            .set(&GrantDataKey::GrantConfig, &updated_config);

        // Transfer grant from treasury to provider
        token_client.transfer(
            &config.treasury,
            &goal_event.provider,
            &goal_event.grant_amount,
        );

        // Emit grant processed event
        env.events().publish(
            (symbol_short!("GrantProc"), goal_event.goal_id),
            (
                goal_event.provider.clone(),
                goal_event.grant_amount,
                months_to_cover,
                goal_event.water_savings,
            ),
        );
    }

    /// Get grant match details
    pub fn get_grant_match(env: Env, goal_id: u64) -> GrantMatch {
        env.storage()
            .instance()
            .get(&GrantDataKey::GrantMatch(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, GrantError::GrantNotFound))
    }

    /// Get all grant matches for a provider
    pub fn get_provider_grants(env: Env, provider: Address) -> Vec<u64> {
        let mut grant_ids = Vec::new(&env);
        let count: u64 = env
            .storage()
            .instance()
            .get(&GrantDataKey::MatchCount)
            .unwrap_or(0);

        for goal_id in 1..=count {
            if let Some(grant_match) = env
                .storage()
                .instance()
                .get::<_, GrantMatch>(&GrantDataKey::GrantMatch(goal_id))
            {
                if grant_match.provider == provider {
                    grant_ids.push_back(goal_id);
                }
            }
        }

        grant_ids
    }

    /// Get grant configuration
    pub fn get_grant_config(env: Env) -> GrantConfig {
        env.storage()
            .instance()
            .get(&GrantDataKey::GrantConfig)
            .unwrap_or_else(|| panic_with_error!(&env, GrantError::GrantNotFound))
    }

    /// Update grant configuration (admin only)
    pub fn update_grant_config(env: Env, enabled: bool, max_grant_per_month: i128) {
        let mut config: GrantConfig = env
            .storage()
            .instance()
            .get(&GrantDataKey::GrantConfig)
            .unwrap_or_else(|| panic_with_error!(&env, GrantError::GrantNotFound));

        config.admin.require_auth();

        if max_grant_per_month <= 0 {
            panic_with_error!(&env, GrantError::InvalidGrantAmount);
        }

        config.enabled = enabled;
        config.max_grant_per_month = max_grant_per_month;

        env.storage()
            .instance()
            .set(&GrantDataKey::GrantConfig, &config);

        env.events().publish(
            (symbol_short!("GrntCfgUp"),),
            (enabled, max_grant_per_month),
        );
    }

    /// Update treasury address (admin only)
    pub fn update_treasury(env: Env, new_treasury: Address) {
        let mut config: GrantConfig = env
            .storage()
            .instance()
            .get(&GrantDataKey::GrantConfig)
            .unwrap_or_else(|| panic_with_error!(&env, GrantError::GrantNotFound));

        config.admin.require_auth();

        let old_treasury = config.treasury.clone();
        config.treasury = new_treasury.clone();

        env.storage()
            .instance()
            .set(&GrantDataKey::GrantConfig, &config);

        env.events()
            .publish((symbol_short!("TreasUp"),), (old_treasury, new_treasury));
    }

    /// Get total grants awarded to a provider
    pub fn get_provider_total_grants(env: Env, provider: Address) -> i128 {
        env.storage()
            .instance()
            .get(&GrantDataKey::ProviderTotalGrants(provider))
            .unwrap_or(0)
    }

    /// Get grant statistics
    pub fn get_grant_statistics(env: Env) -> (u64, i128, i128) {
        let count: u64 = env
            .storage()
            .instance()
            .get(&GrantDataKey::MatchCount)
            .unwrap_or(0);
        let config: GrantConfig = env
            .storage()
            .instance()
            .get(&GrantDataKey::GrantConfig)
            .unwrap_or_else(|| panic_with_error!(&env, GrantError::GrantNotFound));

        (count, config.total_granted, config.max_grant_per_month)
    }

    /// Helper function to get year-month from timestamp
    fn get_year_month(timestamp: u64) -> u64 {
        let days_since_epoch = timestamp / 86_400; // Convert to days
        let years = days_since_epoch / 365;
        let remaining_days = days_since_epoch % 365;
        let months = remaining_days / 30; // Approximate months

        years * 100 + months // Format: YYYYMM
    }
}
