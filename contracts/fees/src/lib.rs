#![no_std]

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype,
    panic_with_error, symbol_short, Address, Bytes, BytesN, Env, Vec,
};

// ============================================================================
// Errors
// ============================================================================

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FeeError {
    NotAuthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    InvalidSplit = 4,
    PeriodNotClosed = 6,
    AlreadyClaimed = 7,
    NoFeeToDistribute = 8,
    MerkleProofFailed = 9,
    SweepNotDue = 10,
    AlreadySwept = 11,
    InsufficientBalance = 12,
    DuplicateRecipient = 13,
    ZeroAmount = 14,
    Overflow = 16,
}

// ============================================================================
// Data types
// ============================================================================

#[contracttype]
#[derive(Clone)]
pub struct SplitConfig {
    pub recipients: Vec<(Address, u32)>,
}

#[contracttype]
#[derive(Clone)]
pub struct PeriodData {
    pub period_id: u64,
    pub total_fees: i128,
    pub claimed: i128,
    pub merkle_root: BytesN<32>,
    pub end_ledger: u32,
    pub swept: bool,
}

// ============================================================================
// Storage keys
// ============================================================================

#[contracttype]
pub enum DataKey {
    Admin,
    SplitConfig,
    PendingFees,
    CurrentPeriod,
    Period(u64),
    Claimed(u64, Address),
    SweepDeadline,
    SweptPeriod(u64),
    NextPeriodId,
    Collectors,
}

// ============================================================================
// Contract interface
// ============================================================================

#[contractclient(name = "FeeDistributorClient")]
pub trait FeeDistributor {
    fn initialize(env: Env, admin: Address, split: SplitConfig);
    fn set_split(env: Env, split: SplitConfig);
    fn add_collector(env: Env, collector: Address);
    fn remove_collector(env: Env, collector: Address);
    fn deposit_fee(env: Env, source: Address, amount: i128);
    fn close_period(env: Env, merkle_root: BytesN<32>);
    fn claim(
        env: Env,
        period_id: u64,
        recipient: Address,
        amount: i128,
        merkle_proof: Vec<BytesN<32>>,
    );
    fn sweep(env: Env, period_id: u64, recipient: Address);
    fn get_split(env: Env) -> SplitConfig;
    fn get_period(env: Env, period_id: u64) -> PeriodData;
    fn get_pending_fees(env: Env) -> i128;
    fn get_current_period_id(env: Env) -> u64;
}

// ============================================================================
// Implementation
// ============================================================================

#[contract]
pub struct FeeDistributorContract;

#[contractimpl]
impl FeeDistributor for FeeDistributorContract {
    fn initialize(env: Env, admin: Address, split: SplitConfig) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, FeeError::AlreadyInitialized);
        }
        validate_split(&env, &split);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::SplitConfig, &split);
        env.storage().instance().set(&DataKey::NextPeriodId, &0u64);
        env.storage().instance().set(&DataKey::PendingFees, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::SweepDeadline, &518400u32);
        env.storage()
            .instance()
            .set(&DataKey::Collectors, &Vec::<Address>::new(&env));
    }

    fn set_split(env: Env, split: SplitConfig) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::NotInitialized));
        admin.require_auth();
        validate_split(&env, &split);
        env.storage().instance().set(&DataKey::SplitConfig, &split);
        env.events().publish((symbol_short!("split"),), split.recipients);
    }

    fn add_collector(env: Env, collector: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::NotInitialized));
        admin.require_auth();
        let mut collectors: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Collectors)
            .unwrap_or_else(|| Vec::new(&env));
        if collectors.iter().any(|c| c == collector) {
            return;
        }
        collectors.push_back(collector);
        env.storage().instance().set(&DataKey::Collectors, &collectors);
    }

    fn remove_collector(env: Env, collector: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::NotInitialized));
        admin.require_auth();
        let collectors: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Collectors)
            .unwrap_or_else(|| Vec::new(&env));
        let mut filtered: Vec<Address> = Vec::new(&env);
        for c in collectors.iter() {
            if c != collector {
                filtered.push_back(c);
            }
        }
        env.storage().instance().set(&DataKey::Collectors, &filtered);
    }

    fn deposit_fee(env: Env, source: Address, amount: i128) {
        if amount <= 0 {
            panic_with_error!(&env, FeeError::ZeroAmount);
        }
        let collectors: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Collectors)
            .unwrap_or_else(|| Vec::new(&env));
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::NotInitialized));
        let is_collector = collectors.iter().any(|c| c == source);
        if !is_collector && source != admin {
            panic_with_error!(&env, FeeError::NotAuthorized);
        }
        let pending: i128 = env
            .storage()
            .instance()
            .get(&DataKey::PendingFees)
            .unwrap_or(0i128);
        let new_pending = pending
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));
        env.storage()
            .instance()
            .set(&DataKey::PendingFees, &new_pending);
        env.events()
            .publish((symbol_short!("deposit"),), (source, amount, new_pending));
    }

    fn close_period(env: Env, merkle_root: BytesN<32>) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::NotInitialized));
        admin.require_auth();
        let pending: i128 = env
            .storage()
            .instance()
            .get(&DataKey::PendingFees)
            .unwrap_or(0i128);
        if pending <= 0 {
            panic_with_error!(&env, FeeError::NoFeeToDistribute);
        }
        let period_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextPeriodId)
            .unwrap_or(0u64);
        let period = PeriodData {
            period_id,
            total_fees: pending,
            claimed: 0,
            merkle_root: merkle_root.clone(),
            end_ledger: env.ledger().sequence(),
            swept: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Period(period_id), &period);
        env.storage()
            .instance()
            .set(&DataKey::NextPeriodId, &(period_id + 1));
        env.storage().instance().set(&DataKey::PendingFees, &0i128);
        env.events()
            .publish((symbol_short!("period"),), (period_id, pending, merkle_root.clone()));
    }

    fn claim(
        env: Env,
        period_id: u64,
        recipient: Address,
        amount: i128,
        merkle_proof: Vec<BytesN<32>>,
    ) {
        recipient.require_auth();
        let mut period: PeriodData = env
            .storage()
            .persistent()
            .get(&DataKey::Period(period_id))
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::PeriodNotClosed));
        if period.swept {
            panic_with_error!(&env, FeeError::AlreadySwept);
        }
        let claimed_key = DataKey::Claimed(period_id, recipient.clone());
        if env.storage().persistent().has(&claimed_key) {
            panic_with_error!(&env, FeeError::AlreadyClaimed);
        }
        let r_bytes: Bytes = recipient.clone().to_xdr(&env);
        let a_bytes: Bytes = amount.to_xdr(&env);
        let mut combined = Bytes::new(&env);
        combined.append(&r_bytes);
        combined.append(&a_bytes);
        let leaf: BytesN<32> = env.crypto().sha256(&combined).into();

        let expected_root = verify_merkle_proof(&env, leaf, &merkle_proof);
        if expected_root != period.merkle_root {
            panic_with_error!(&env, FeeError::MerkleProofFailed);
        }
        if amount <= 0 {
            panic_with_error!(&env, FeeError::ZeroAmount);
        }
        let remaining = period
            .total_fees
            .checked_sub(period.claimed)
            .unwrap_or(0);
        if amount > remaining {
            panic_with_error!(&env, FeeError::InsufficientBalance);
        }
        period.claimed = period
            .claimed
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));
        env.storage()
            .persistent()
            .set(&DataKey::Period(period_id), &period);
        env.storage().persistent().set(&claimed_key, &true);
        env.events()
            .publish((symbol_short!("claim"),), (period_id, recipient, amount));
    }

    fn sweep(env: Env, period_id: u64, recipient: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::NotInitialized));
        admin.require_auth();
        let mut period: PeriodData = env
            .storage()
            .persistent()
            .get(&DataKey::Period(period_id))
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::PeriodNotClosed));
        if period.swept {
            panic_with_error!(&env, FeeError::AlreadySwept);
        }
        let deadline: u32 = env
            .storage()
            .instance()
            .get(&DataKey::SweepDeadline)
            .unwrap_or(518400u32);
        if env.ledger().sequence() < period.end_ledger + deadline {
            panic_with_error!(&env, FeeError::SweepNotDue);
        }
        let unclaimed = period
            .total_fees
            .checked_sub(period.claimed)
            .unwrap_or(0);
        if unclaimed <= 0 {
            panic_with_error!(&env, FeeError::NoFeeToDistribute);
        }
        period.swept = true;
        env.storage()
            .persistent()
            .set(&DataKey::Period(period_id), &period);
        env.storage()
            .persistent()
            .set(&DataKey::SweptPeriod(period_id), &true);
        env.events()
            .publish((symbol_short!("sweep"),), (period_id, recipient, unclaimed));
    }

    fn get_split(env: Env) -> SplitConfig {
        env.storage()
            .instance()
            .get(&DataKey::SplitConfig)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::NotInitialized))
    }

    fn get_period(env: Env, period_id: u64) -> PeriodData {
        env.storage()
            .persistent()
            .get(&DataKey::Period(period_id))
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::PeriodNotClosed))
    }

    fn get_pending_fees(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::PendingFees)
            .unwrap_or(0i128)
    }

    fn get_current_period_id(env: Env) -> u64 {
        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextPeriodId)
            .unwrap_or(0u64);
        if id == 0 { 0 } else { id - 1 }
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn validate_split(env: &Env, split: &SplitConfig) {
    if split.recipients.is_empty() {
        panic_with_error!(env, FeeError::InvalidSplit);
    }
    let mut total: u64 = 0;
    let mut seen: Vec<Address> = Vec::new(env);
    for (recipient, bps) in split.recipients.iter() {
        if bps == 0 || bps > 10000 {
            panic_with_error!(env, FeeError::InvalidSplit);
        }
        if seen.iter().any(|a| a == recipient) {
            panic_with_error!(env, FeeError::DuplicateRecipient);
        }
        seen.push_back(recipient);
        total = total
            .checked_add(bps as u64)
            .unwrap_or_else(|| panic_with_error!(env, FeeError::Overflow));
    }
    if total != 10000 {
        panic_with_error!(env, FeeError::InvalidSplit);
    }
}

#[cfg(test)]
mod test;

fn verify_merkle_proof(
    env: &Env,
    leaf: BytesN<32>,
    proof: &Vec<BytesN<32>>,
) -> BytesN<32> {
    let mut current = leaf;
    for sibling in proof.iter() {
        let mut combined = Bytes::new(env);
        if current.as_ref() <= sibling.as_ref() {
            let b: Bytes = current.into();
            combined.append(&b);
            let b: Bytes = sibling.into();
            combined.append(&b);
        } else {
            let b: Bytes = sibling.into();
            combined.append(&b);
            let b: Bytes = current.into();
            combined.append(&b);
        }
        current = env.crypto().sha256(&combined).into();
    }
    current
}
