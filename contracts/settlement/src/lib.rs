#![no_std]

mod constants;
mod conversion;
mod fees;
mod rate_application;
mod reentrancy;
mod storage;
mod token_utils;
mod types;

use soroban_sdk::{contract, contractclient, contracterror, contractimpl, panic_with_error, Address, Env};

#[cfg(test)]
mod test;

use crate::constants::MAX_FEE_RATE_BPS;
use crate::conversion::convert_to_settlement_currency;
use crate::fees::compute_fee;
use crate::reentrancy::ReentrancyGuard;
use crate::token_utils::collect_fee;
use crate::types::{SettlementArgs, SettlementResult};

/// Cross-contract interface for the PriceOracle.
#[contractclient(name = "PriceOracleClient")]
pub trait PriceOracle {
    fn get_price_value(env: Env) -> i128;
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SettlementError {
    InvalidFeeRate = 1,
    InsufficientBalance = 2,
    SlippageExceeded = 3,
    /// A cross-contract callback attempted to re-enter a guarded entry point.
    ReentrantCall = 4,
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
        // Acquire before any cross-contract call; released on scope exit.
        let _guard = ReentrancyGuard::new(&env);

        if rate_bps > MAX_FEE_RATE_BPS {
            panic_with_error!(&env, SettlementError::InvalidFeeRate);
        }

        if amount <= 0 {
            return (0, 0);
        }

        payer.require_auth();

        let fee = collect_fee(&env, &token, &payer, &fee_collector, amount, rate_bps);
        let net_amount = amount.saturating_sub(fee);

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

    /// Finalize settlement with oracle-based currency conversion and slippage protection.
    ///
    /// Converts resource token volume to settlement currency using the current
    /// oracle exchange rate, with both protocol-enforced and user-defined slippage bounds.
    /// Fee is deducted from the settlement amount before transfer.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `oracle` - Address of the price oracle contract
    /// * `payer` - Address funding the settlement
    /// * `fee_collector` - Address collecting the protocol fee
    /// * `args` - Settlement parameters (token, volume, recipient, min_expected_amount)
    /// * `rate_bps` - Fee rate in basis points
    ///
    /// # Returns
    /// SettlementResult containing net_amount, fee_amount, and rate_used
    pub fn finalize_settlement(
        env: Env,
        oracle: Address,
        payer: Address,
        fee_collector: Address,
        args: SettlementArgs,
        rate_bps: u32,
    ) -> SettlementResult {
        // Acquire before any cross-contract call (oracle / token); released on
        // scope exit. Blocks a malicious token or oracle from re-entering
        // finalize_settlement before this invocation completes.
        let _guard = ReentrancyGuard::new(&env);

        if rate_bps > MAX_FEE_RATE_BPS {
            panic_with_error!(&env, SettlementError::InvalidFeeRate);
        }

        if args.volume <= 0 {
            return SettlementResult {
                net_amount: 0,
                fee_amount: 0,
                rate_used: 0,
            };
        }

        payer.require_auth();

        let rate = {
            let oracle_client = PriceOracleClient::new(&env, &oracle);
            oracle_client.get_price_value()
        };

        let settlement_amount = convert_to_settlement_currency(
            &env,
            &oracle,
            args.volume,
            args.min_expected_amount,
        );

        let fee = collect_fee(
            &env,
            &args.token_address,
            &payer,
            &fee_collector,
            settlement_amount,
            rate_bps,
        );
        let net_amount = settlement_amount.saturating_sub(fee);

        if net_amount > 0 {
            let token_client = soroban_sdk::token::Client::new(&env, &args.token_address);
            token_client.transfer(&payer, &args.recipient, &net_amount);
        }

        SettlementResult {
            net_amount,
            fee_amount: fee,
            rate_used: rate,
        }
    }
}
