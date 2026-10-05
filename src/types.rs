use soroban_sdk::{contracterror, contracttype, Address, Symbol};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Operator(Address),
    Consumer(Address),
    Tariff(Symbol),
    TotalSettledVolume,
}

#[derive(Clone)]
#[contracttype]
pub struct OperatorRecord {
    pub operator: Address,
    pub staked_amount: i128,
    pub resource_type: Symbol,
    pub is_active: bool,
    /// Accrued but not yet withdrawn settlement earnings.
    pub accrued_earnings: i128,
    /// Lifetime earnings already paid out via `settle_operator_payout`.
    pub total_paid: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct ConsumerRecord {
    pub consumer: Address,
    pub escrow_balance: i128,
    pub total_units_consumed: u64,
    pub last_meter_sequence: u64,
    pub is_active: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct TariffRate {
    pub resource_type: Symbol,
    pub rate_per_unit: i128,
    pub updated_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[contracterror]
#[repr(i32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ConsumerNotFound = 4,
    InsufficientEscrow = 5,
    OperatorNotFound = 6,
    InvalidSequence = 7,
    TariffNotFound = 8,
    InvalidAmount = 9,
    OperatorInactive = 10,
    InsufficientAccrued = 11,
}
