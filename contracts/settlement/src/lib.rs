#![no_std]

mod constants;
mod fees;
mod token_utils;

use soroban_sdk::{contract, contractimpl, contracterror, panic_with_error, Address, Env};

#[cfg(test)]
mod test;

use crate::constants::MAX_FEE_RATE_BPS;
use crate::fees::compute_fee;
use crate::token_utils::collect_fee;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SettlementError {
    InvalidFeeRate = 1,
    InsufficientBalance = 2,
}

#[contract]
pub struct SettlementContract;

#[contractimpl]
impl SettlementContract {
    /// Settle a payment and collect the protocol fee.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `token` - Token contract address
    /// * `payer` - Address paying the settlement
    /// * `payee` - Address receiving the net settlement
    /// * `fee_collector` - Address collecting the protocol fee
    /// * `amount` - Gross settlement amount
    /// * `rate_bps` - Fee rate in basis points
    ///
    /// # Returns
    /// (net_amount, fee_amount)
    pub fn settle(
        env: Env,
        token: Address,
        payer: Address,
        payee: Address,
        fee_collector: Address,
        amount: i128,
        rate_bps: u32,
    ) -> (i128, i128) {
        if rate_bps > MAX_FEE_RATE_BPS {
            panic_with_error!(&env, SettlementError::InvalidFeeRate);
        }

        if amount <= 0 {
            return (0, 0);
        }

        payer.require_auth();

        let fee = collect_fee(&env, &token, &payer, &fee_collector, amount, rate_bps);
        let net_amount = amount.saturating_sub(fee);

        // Transfer net amount to payee
        if net_amount > 0 {
            let token_client = soroban_sdk::token::Client::new(&env, &token);
            token_client.transfer(&payer, &payee, &net_amount);
        }

        (net_amount, fee)
    }

    /// Compute the fee for a given amount and rate (pure, no side effects).
    pub fn calculate_fee(_env: Env, amount: i128, rate_bps: u32) -> i128 {
        compute_fee(amount, rate_bps)
    }
}
