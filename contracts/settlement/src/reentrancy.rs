//! Cross-contract reentrancy protection.
//!
//! The settlement entry points yield control to untrusted contracts at the
//! cross-contract call boundary: the price oracle (`get_price_value`) and the
//! settlement token (`transfer`, via [`crate::token_utils::collect_fee`] and the
//! net-amount transfer). A malicious token or oracle can re-enter the settlement
//! contract before the original invocation finishes and manipulate intermediate
//! state (e.g. double-spend a fee transfer, settle twice off one authorization).
//!
//! [`ReentrancyGuard`] is a strict, RAII mutex: acquiring it while it is already
//! held panics, so **any** re-entry into a guarded entry point aborts the whole
//! transaction. It is acquired before any external call and released when the
//! guard goes out of scope.
//!
//! ## Why a strict mutex (no re-entry) rather than a bounded callback depth
//!
//! For payment finalization there is no legitimate reason to re-enter while a
//! settlement is in flight, so the safest control is to forbid re-entry entirely.
//! That is strictly stronger than allowing a bounded depth and removes the class
//! of bug where "a little" reentrancy is still exploitable.
//!
//! ## Cleanup on the panic path
//!
//! On normal return the lock is cleared by [`Drop`]. On a panic the Soroban host
//! reverts **all** storage writes made by the invocation, so the lock is
//! discarded automatically — important because Wasm builds use `panic = "abort"`,
//! where `Drop` does not run during unwinding. Both paths therefore leave the
//! lock clear for the next top-level invocation.

use soroban_sdk::{panic_with_error, Env};

use crate::storage::DataKey;
use crate::SettlementError;

/// TTL (in ledgers) for the lock entry. The lock only needs to outlive the
/// current invocation (a single ledger); it is cleared on return and reverted on
/// panic, so this is a safety floor rather than a functional dependency.
const LOCK_TTL_LEDGERS: u32 = 1;

/// RAII reentrancy mutex. Acquire at the top of every externally-callable entry
/// point that performs cross-contract calls.
pub struct ReentrancyGuard<'a> {
    env: &'a Env,
}

impl<'a> ReentrancyGuard<'a> {
    /// Acquire the lock. Panics with [`SettlementError::ReentrantCall`] if it is
    /// already held — i.e. if this is a reentrant call.
    pub fn new(env: &'a Env) -> Self {
        let locked: bool = env
            .storage()
            .temporary()
            .get(&DataKey::ReentrancyLock)
            .unwrap_or(false);

        if locked {
            panic_with_error!(env, SettlementError::ReentrantCall);
        }

        env.storage()
            .temporary()
            .set(&DataKey::ReentrancyLock, &true);
        env.storage().temporary().extend_ttl(
            &DataKey::ReentrancyLock,
            LOCK_TTL_LEDGERS,
            LOCK_TTL_LEDGERS,
        );

        ReentrancyGuard { env }
    }
}

impl Drop for ReentrancyGuard<'_> {
    fn drop(&mut self) {
        // Release on normal completion. (On panic the host revert handles this.)
        self.env
            .storage()
            .temporary()
            .set(&DataKey::ReentrancyLock, &false);
    }
}
