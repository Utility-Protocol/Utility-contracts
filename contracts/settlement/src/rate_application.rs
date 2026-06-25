use soroban_sdk::{Address, Env};

use crate::PriceOracleClient;

/// Fetch the current exchange rate from the price oracle.
/// Returns the rate as an i128 with 7 decimal places.
pub fn get_rate(env: &Env, oracle: &Address) -> i128 {
    let client = PriceOracleClient::new(env, oracle);
    client.get_price_value()
}
