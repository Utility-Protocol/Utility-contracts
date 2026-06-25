/// Default protocol fee rate in basis points (1% = 100 bps)
#[allow(dead_code)]
pub const FEE_RATE_BPS: u32 = 100;

/// Minimum fee rate in basis points (0.01%)
#[allow(dead_code)]
pub const MIN_FEE_RATE_BPS: u32 = 1;

/// Maximum fee rate in basis points (10%)
pub const MAX_FEE_RATE_BPS: u32 = 1000;

/// Maximum settlement amount with 7-decimal precision
#[allow(dead_code)]
pub const MAX_SETTLEMENT: i128 = 1_000_000_000_000_000_000; // 1e18

/// Denominator for basis points calculations
pub const BPS_DENOMINATOR: u32 = 10000;

/// Maximum allowed slippage in basis points (100 bps = 1%)
/// Range: [1, 500] (0.01% to 5%)
pub const MAX_SLIPPAGE_BPS: u32 = 100;

/// Maximum settlement volume per call (1M tokens with 7 decimals)
#[allow(dead_code)]
pub const MAX_VOLUME: i128 = 10_000_000_000_000; // 1_000_000 * 1e7

/// Maximum oracle rate with 7 decimals
#[allow(dead_code)]
pub const MAX_RATE: i128 = 10_000_000_000_000; // 1_000_000 * 1e7

/// Denominator for fixed-point operations (7 decimal places)
pub const DECIMAL_DENOMINATOR: i128 = 10_000_000; // 1e7
