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
