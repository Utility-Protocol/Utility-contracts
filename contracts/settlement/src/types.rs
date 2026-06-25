use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug)]
pub struct SettlementArgs {
    pub token_address: Address,
    pub volume: i128,
    pub recipient: Address,
    pub min_expected_amount: Option<i128>,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct SettlementResult {
    pub net_amount: i128,
    pub fee_amount: i128,
    pub rate_used: i128,
}
