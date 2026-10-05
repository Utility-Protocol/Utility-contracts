#![no_std]

mod types;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};

use types::{ConsumerRecord, ContractError, DataKey, OperatorRecord, TariffRate};

#[contract]
pub struct UtilityProtocol;

fn admin(env: &Env) -> Result<Address, ContractError> {
    env.storage().persistent()
        .get::<_, Address>(&DataKey::Admin)
        .ok_or(ContractError::NotInitialized)
}

#[contractimpl]
impl UtilityProtocol {
    /// One-time bootstrap. Records `admin` as the tariff authority.
    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        if env.storage().persistent().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Onboard a generation node. The operator stakes hardware collateral and
    /// declares the resource class it meters (`SOLAR`, `WATER`, ...).
    pub fn register_operator(
        env: Env,
        operator: Address,
        resource_type: Symbol,
        initial_stake: i128,
    ) -> Result<(), ContractError> {
        admin(&env)?;
        if initial_stake <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        operator.require_auth();

        if env.storage().persistent().has(&DataKey::Operator(operator.clone())) {
            return Err(ContractError::Unauthorized);
        }

        let record = OperatorRecord {
            operator: operator.clone(),
            staked_amount: initial_stake,
            resource_type,
            is_active: true,
            accrued_earnings: 0,
            total_paid: 0,
        };
        env.storage().persistent().set(&DataKey::Operator(operator), &record);
        Ok(())
    }

    /// Set or update the per-unit tariff. Rejects negative rates.
    pub fn set_tariff(
        env: Env,
        resource_type: Symbol,
        rate_per_unit: i128,
    ) -> Result<(), ContractError> {
        let admin_addr = admin(&env)?;
        admin_addr.require_auth();
        if rate_per_unit < 0 {
            return Err(ContractError::InvalidAmount);
        }

        let rate = TariffRate {
            resource_type: resource_type.clone(),
            rate_per_unit,
            updated_at: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&DataKey::Tariff(resource_type), &rate);
        Ok(())
    }

    /// Pre-fund a consumer escrow account.
    pub fn deposit_escrow(env: Env, consumer: Address, amount: i128) -> Result<(), ContractError> {
        admin(&env)?;
        consumer.require_auth();
        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }

        let mut record = env
            .storage().persistent()
            .get::<_, ConsumerRecord>(&DataKey::Consumer(consumer.clone()))
            .unwrap_or(ConsumerRecord {
                consumer: consumer.clone(),
                escrow_balance: 0,
                total_units_consumed: 0,
                last_meter_sequence: 0,
                is_active: true,
            });

        record.escrow_balance = record
            .escrow_balance
            .checked_add(amount)
            .ok_or(ContractError::InvalidAmount)?;
        record.is_active = true;

        env.storage().persistent().set(&DataKey::Consumer(consumer), &record);
        Ok(())
    }

    /// Settle one meter reading on-chain.
    ///
    /// Auth is the operator's: the operator is the metering hardware and is the
    /// party whose accrued earnings the tick credits, so requiring the consumer
    /// signature here would make unattended polling impossible.
    ///
    /// `meter_sequence` must be strictly greater than the consumer's last
    /// accepted sequence, which makes replayed or reordered meter packets fail.
    pub fn record_usage_tick(
        env: Env,
        consumer: Address,
        operator: Address,
        resource_type: Symbol,
        units_drawn: u64,
        meter_sequence: u64,
    ) -> Result<(), ContractError> {
        admin(&env)?;
        operator.require_auth();

        let mut op = env
            .storage().persistent()
            .get::<_, OperatorRecord>(&DataKey::Operator(operator.clone()))
            .ok_or(ContractError::OperatorNotFound)?;
        if !op.is_active {
            return Err(ContractError::OperatorInactive);
        }

        let mut con = env
            .storage().persistent()
            .get::<_, ConsumerRecord>(&DataKey::Consumer(consumer.clone()))
            .ok_or(ContractError::ConsumerNotFound)?;
        if !con.is_active {
            return Err(ContractError::ConsumerNotFound);
        }

        // Replay / reordering guard.
        if meter_sequence <= con.last_meter_sequence {
            return Err(ContractError::InvalidSequence);
        }

        let tariff = env
            .storage().persistent()
            .get::<_, TariffRate>(&DataKey::Tariff(resource_type.clone()))
            .ok_or(ContractError::TariffNotFound)?;

        let total_cost = (units_drawn as i128)
            .checked_mul(tariff.rate_per_unit)
            .ok_or(ContractError::InvalidAmount)?;

        if con.escrow_balance < total_cost {
            return Err(ContractError::InsufficientEscrow);
        }

        con.escrow_balance -= total_cost;
        con.total_units_consumed = con
            .total_units_consumed
            .checked_add(units_drawn)
            .ok_or(ContractError::InvalidAmount)?;
        con.last_meter_sequence = meter_sequence;

        op.accrued_earnings = op
            .accrued_earnings
            .checked_add(total_cost)
            .ok_or(ContractError::InvalidAmount)?;

        env.storage().persistent().set(&DataKey::Consumer(consumer.clone()), &con);
        env.storage().persistent().set(&DataKey::Operator(operator.clone()), &op);

        let settled_volume: u64 = env
            .storage().persistent()
            .get(&DataKey::TotalSettledVolume)
            .unwrap_or(0);
        let new_volume = settled_volume
            .checked_add(units_drawn)
            .ok_or(ContractError::InvalidAmount)?;
        env.storage().persistent()
            .set(&DataKey::TotalSettledVolume, &new_volume);

        // Published manually rather than via `#[contractevent]` because that
        // macro prepends the contract address as topics[0]. The indexer filters
        // positionally on `util_tick` at topics[0], so the explicit 3-topic
        // layout is load-bearing here.
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("util_tick"), consumer, resource_type),
            (operator, units_drawn, total_cost, env.ledger().timestamp()),
        );

        Ok(())
    }

    /// Withdraw accrued earnings to the operator's stake accounting.
    pub fn settle_operator_payout(
        env: Env,
        operator: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        admin(&env)?;
        operator.require_auth();
        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }

        let mut op = env
            .storage().persistent()
            .get::<_, OperatorRecord>(&DataKey::Operator(operator.clone()))
            .ok_or(ContractError::OperatorNotFound)?;

        if op.accrued_earnings < amount {
            return Err(ContractError::InsufficientAccrued);
        }

        op.accrued_earnings -= amount;
        op.total_paid = op
            .total_paid
            .checked_add(amount)
            .ok_or(ContractError::InvalidAmount)?;

        env.storage().persistent().set(&DataKey::Operator(operator), &op);
        Ok(())
    }

    // ---- views ----

    pub fn get_consumer(env: Env, consumer: Address) -> Option<ConsumerRecord> {
        env.storage().persistent().get(&DataKey::Consumer(consumer))
    }

    pub fn get_operator(env: Env, operator: Address) -> Option<OperatorRecord> {
        env.storage().persistent().get(&DataKey::Operator(operator))
    }

    pub fn get_tariff(env: Env, resource_type: Symbol) -> Option<TariffRate> {
        env.storage().persistent().get(&DataKey::Tariff(resource_type))
    }

    pub fn get_total_settled_volume(env: Env) -> u64 {
        env.storage().persistent()
            .get(&DataKey::TotalSettledVolume)
            .unwrap_or(0)
    }
}

// mod probe;
mod test;