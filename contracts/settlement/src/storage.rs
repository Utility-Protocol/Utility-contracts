//! Storage key definitions for the settlement contract.

use soroban_sdk::contracttype;

/// Storage keys used by the settlement contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Cross-contract reentrancy mutex. Held in *temporary* storage for the
    /// duration of a single contract invocation (see [`crate::reentrancy`]).
    ReentrancyLock,
}
