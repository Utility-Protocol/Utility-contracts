//! Sponsorship Policy Engine for Gasless Relay (Issue #131)
//!
//! Manages gas sponsorship policies for approved operations during user onboarding.
//! Enables fine-grained control over which operations are sponsored and how.

use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, String, Symbol, Vec,
};

/// Operation eligibility for gas sponsorship
#[contracttype]
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum SponsorshipStatus {
    /// Not eligible for sponsorship
    NotSponsored = 0,
    /// Eligible for full sponsorship (all gas covered)
    FullySponsored = 1,
    /// Eligible for partial sponsorship (percentage covered)
    PartiallySponsored = 2,
    /// Temporarily suspended pending review
    Suspended = 3,
}

/// Detailed sponsorship policy for operations
#[contracttype]
#[derive(Clone, Debug)]
pub struct DetailedSponsorshipPolicy {
    /// Operation identifier
    pub operation_id: Symbol,
    /// Current sponsorship status
    pub status: SponsorshipStatus,
    /// Maximum gas limit (in Soroban units)
    pub max_gas: u64,
    /// Percentage of gas to sponsor (0-100 for partial, 100 for full)
    pub sponsorship_percentage: u32,
    /// Maximum daily sponsored transactions
    pub daily_tx_limit: u32,
    /// Cost per transaction in stroops
    pub cost_per_tx: i128,
    /// Creation timestamp
    pub created_at: u64,
    /// Last updated timestamp
    pub updated_at: u64,
    /// Policy administrator
    pub admin: Address,
    /// Policy description
    pub description: String,
}

/// Operation statistics for monitoring
#[contracttype]
#[derive(Clone, Debug)]
pub struct OperationStats {
    /// Operation identifier
    pub operation_id: Symbol,
    /// Total transactions sponsored
    pub total_sponsored: u64,
    /// Total gas sponsored (in units)
    pub total_gas_sponsored: u128,
    /// Total cost incurred (in stroops)
    pub total_cost_incurred: i128,
    /// Transactions in current day
    pub transactions_today: u32,
    /// Last reset timestamp
    pub last_reset: u64,
}

/// Sponsorship Eligibility Result
#[contracttype]
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum EligibilityCheckResult {
    /// Operation is eligible for sponsorship
    Eligible = 1,
    /// Operation is not in the approved list
    OperationNotFound = 2,
    /// Operation is suspended
    OperationSuspended = 3,
    /// Daily limit exceeded
    DailyLimitExceeded = 4,
    /// Gas limit would be exceeded
    GasLimitExceeded = 5,
    /// Cost would exceed remaining pool balance
    InsufficientPoolBalance = 6,
}

/// Policy Engine for Sponsorship Management
#[contract]
pub struct SponsorshipPolicyEngine;

#[contractimpl]
impl SponsorshipPolicyEngine {
    /// Initialize the policy engine
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `admin` - Initial administrator
    pub fn init_policy_engine(env: Env, admin: Address) -> Result<(), u32> {
        let admin_key = Symbol::new(&env, "policy_admin");
        env.storage().instance().set(&admin_key, &admin);
        Ok(())
    }

    /// Create a new sponsorship policy
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `operation_id` - The operation identifier
    /// * `policy` - The policy details
    pub fn create_policy(
        env: Env,
        operation_id: Symbol,
        policy: DetailedSponsorshipPolicy,
    ) -> Result<(), u32> {
        // Verify admin
        let admin_key = Symbol::new(&env, "policy_admin");
        let admin: Address = env.storage().instance().get(&admin_key).unwrap();
        if env.invoker() != admin {
            return Err(1); // Unauthorized
        }

        // Validate policy
        if policy.sponsorship_percentage > 100 {
            return Err(2); // Invalid percentage
        }

        // Store policy
        let policies_key = Symbol::new(&env, "policies");
        let mut policies: Vec<(Symbol, DetailedSponsorshipPolicy)> = env.storage()
            .instance()
            .get(&policies_key)
            .unwrap_or(Vec::new(&env));

        // Check if policy already exists
        for i in 0..policies.len() {
            let (op_id, _) = policies.get(i).unwrap();
            if op_id == operation_id {
                return Err(3); // Policy already exists
            }
        }

        policies.push_back((operation_id.clone(), policy));
        env.storage().instance().set(&policies_key, &policies);

        // Initialize stats for this operation
        let stats = OperationStats {
            operation_id,
            total_sponsored: 0,
            total_gas_sponsored: 0,
            total_cost_incurred: 0,
            transactions_today: 0,
            last_reset: env.ledger().timestamp(),
        };

        let stats_key = Symbol::new(&env, &format!("stats_{}", &operation_id));
        env.storage().instance().set(&stats_key, &stats);

        Ok(())
    }

    /// Update an existing policy
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `operation_id` - The operation to update
    /// * `new_policy` - The updated policy
    pub fn update_policy(
        env: Env,
        operation_id: Symbol,
        new_policy: DetailedSponsorshipPolicy,
    ) -> Result<(), u32> {
        // Verify admin
        let admin_key = Symbol::new(&env, "policy_admin");
        let admin: Address = env.storage().instance().get(&admin_key).unwrap();
        if env.invoker() != admin {
            return Err(1); // Unauthorized
        }

        let policies_key = Symbol::new(&env, "policies");
        let mut policies: Vec<(Symbol, DetailedSponsorshipPolicy)> = env.storage()
            .instance()
            .get(&policies_key)
            .unwrap_or(Vec::new(&env));

        let mut found = false;
        for i in 0..policies.len() {
            let (op_id, _) = policies.get(i).unwrap();
            if op_id == operation_id {
                let mut updated_policy = new_policy.clone();
                updated_policy.updated_at = env.ledger().timestamp();
                policies.set(i, (operation_id, updated_policy));
                found = true;
                break;
            }
        }

        if !found {
            return Err(4); // Policy not found
        }

        env.storage().instance().set(&policies_key, &policies);
        Ok(())
    }

    /// Check if an operation is eligible for sponsorship
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `operation_id` - The operation to check
    /// * `gas_needed` - The gas required for the operation
    /// * `pool_balance` - The current sponsorship pool balance
    pub fn check_eligibility(
        env: Env,
        operation_id: Symbol,
        gas_needed: u64,
        pool_balance: i128,
    ) -> EligibilityCheckResult {
        let policies_key = Symbol::new(&env, "policies");
        let policies: Vec<(Symbol, DetailedSponsorshipPolicy)> = env.storage()
            .instance()
            .get(&policies_key)
            .unwrap_or(Vec::new(&env));

        // Find the policy
        let mut policy = None;
        for i in 0..policies.len() {
            let (op_id, p) = policies.get(i).unwrap();
            if op_id == operation_id {
                policy = Some(p);
                break;
            }
        }

        let policy = match policy {
            Some(p) => p,
            None => return EligibilityCheckResult::OperationNotFound,
        };

        // Check status
        if policy.status == SponsorshipStatus::Suspended {
            return EligibilityCheckResult::OperationSuspended;
        }

        // Check gas limit
        if gas_needed > policy.max_gas {
            return EligibilityCheckResult::GasLimitExceeded;
        }

        // Check daily limit
        let stats_key = Symbol::new(&env, &format!("stats_{}", &operation_id));
        let stats: OperationStats = env.storage().instance().get(&stats_key).unwrap_or(
            OperationStats {
                operation_id: operation_id.clone(),
                total_sponsored: 0,
                total_gas_sponsored: 0,
                total_cost_incurred: 0,
                transactions_today: 0,
                last_reset: env.ledger().timestamp(),
            },
        );

        if stats.transactions_today >= policy.daily_tx_limit {
            return EligibilityCheckResult::DailyLimitExceeded;
        }

        // Check pool balance
        if pool_balance < policy.cost_per_tx {
            return EligibilityCheckResult::InsufficientPoolBalance;
        }

        EligibilityCheckResult::Eligible
    }

    /// Get policy details
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `operation_id` - The operation to get details for
    pub fn get_policy(
        env: Env,
        operation_id: Symbol,
    ) -> Option<DetailedSponsorshipPolicy> {
        let policies_key = Symbol::new(&env, "policies");
        let policies: Vec<(Symbol, DetailedSponsorshipPolicy)> = env.storage()
            .instance()
            .get(&policies_key)
            .unwrap_or(Vec::new(&env));

        for i in 0..policies.len() {
            let (op_id, policy) = policies.get(i).unwrap();
            if op_id == operation_id {
                return Some(policy);
            }
        }

        None
    }

    /// Get operation statistics
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `operation_id` - The operation to get stats for
    pub fn get_operation_stats(env: Env, operation_id: Symbol) -> Option<OperationStats> {
        let stats_key = Symbol::new(&env, &format!("stats_{}", &operation_id));
        env.storage().instance().get(&stats_key)
    }

    /// Record a sponsored transaction
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `operation_id` - The operation that was sponsored
    /// * `gas_used` - The actual gas used
    /// * `cost` - The cost incurred
    pub fn record_sponsored_transaction(
        env: Env,
        operation_id: Symbol,
        gas_used: u64,
        cost: i128,
    ) -> Result<(), u32> {
        let stats_key = Symbol::new(&env, &format!("stats_{}", &operation_id));
        let mut stats: OperationStats = env.storage()
            .instance()
            .get(&stats_key)
            .ok_or(1)?; // Stats not found

        stats.total_sponsored += 1;
        stats.total_gas_sponsored += gas_used as u128;
        stats.total_cost_incurred += cost;
        stats.transactions_today += 1;

        env.storage().instance().set(&stats_key, &stats);
        Ok(())
    }

    /// Suspend a sponsorship policy
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `operation_id` - The operation to suspend
    pub fn suspend_policy(env: Env, operation_id: Symbol) -> Result<(), u32> {
        // Verify admin
        let admin_key = Symbol::new(&env, "policy_admin");
        let admin: Address = env.storage().instance().get(&admin_key).unwrap();
        if env.invoker() != admin {
            return Err(1); // Unauthorized
        }

        let policies_key = Symbol::new(&env, "policies");
        let mut policies: Vec<(Symbol, DetailedSponsorshipPolicy)> = env.storage()
            .instance()
            .get(&policies_key)
            .unwrap_or(Vec::new(&env));

        let mut found = false;
        for i in 0..policies.len() {
            let (op_id, mut policy) = policies.get(i).unwrap();
            if op_id == operation_id {
                policy.status = SponsorshipStatus::Suspended;
                policies.set(i, (op_id, policy));
                found = true;
                break;
            }
        }

        if !found {
            return Err(2); // Policy not found
        }

        env.storage().instance().set(&policies_key, &policies);
        Ok(())
    }

    /// Resume a suspended policy
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `operation_id` - The operation to resume
    /// * `new_status` - The new status (FullySponsored or PartiallySponsored)
    pub fn resume_policy(env: Env, operation_id: Symbol, new_status: SponsorshipStatus) -> Result<(), u32> {
        // Verify admin
        let admin_key = Symbol::new(&env, "policy_admin");
        let admin: Address = env.storage().instance().get(&admin_key).unwrap();
        if env.invoker() != admin {
            return Err(1); // Unauthorized
        }

        if new_status == SponsorshipStatus::Suspended || new_status == SponsorshipStatus::NotSponsored {
            return Err(3); // Invalid status
        }

        let policies_key = Symbol::new(&env, "policies");
        let mut policies: Vec<(Symbol, DetailedSponsorshipPolicy)> = env.storage()
            .instance()
            .get(&policies_key)
            .unwrap_or(Vec::new(&env));

        let mut found = false;
        for i in 0..policies.len() {
            let (op_id, mut policy) = policies.get(i).unwrap();
            if op_id == operation_id {
                policy.status = new_status;
                policies.set(i, (op_id, policy));
                found = true;
                break;
            }
        }

        if !found {
            return Err(2); // Policy not found
        }

        env.storage().instance().set(&policies_key, &policies);
        Ok(())
    }

    /// List all active policies
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    pub fn list_active_policies(env: Env) -> Vec<Symbol> {
        let policies_key = Symbol::new(&env, "policies");
        let policies: Vec<(Symbol, DetailedSponsorshipPolicy)> = env.storage()
            .instance()
            .get(&policies_key)
            .unwrap_or(Vec::new(&env));

        let mut active_ops = Vec::new(&env);
        for i in 0..policies.len() {
            let (op_id, policy) = policies.get(i).unwrap();
            if policy.status != SponsorshipStatus::NotSponsored && policy.status != SponsorshipStatus::Suspended {
                active_ops.push_back(op_id);
            }
        }

        active_ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, String};

    #[test]
    fn test_policy_engine_initialization() {
        let env = Env::default();
        let engine = SponsorshipPolicyEngine;
        let admin = Address::generate(&env);

        let result = engine.init_policy_engine(env, admin);
        assert!(result.is_ok());
    }

    #[test]
    fn test_policy_creation() {
        let env = Env::default();
        let engine = SponsorshipPolicyEngine;
        let admin = Address::generate(&env);

        engine.init_policy_engine(env.clone(), admin).ok();

        let policy = DetailedSponsorshipPolicy {
            operation_id: Symbol::new(&env, "test_op"),
            status: SponsorshipStatus::FullySponsored,
            max_gas: 100_000,
            sponsorship_percentage: 100,
            daily_tx_limit: 10,
            cost_per_tx: 1_000,
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
            admin: admin.clone(),
            description: String::from_slice(&env, "Test operation"),
        };

        let result = engine.create_policy(env, Symbol::new(&env, "test_op"), policy);
        assert!(result.is_ok());
    }
}
