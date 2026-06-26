use soroban_sdk::{Address, Env};

use crate::storage::{
    get_allowance as storage_get_allowance, set_allowance as storage_set_allowance,
    get_balance as storage_get_balance, set_balance as storage_set_balance,
    MAX_ALLOWANCE
};

/// Set allowance for a spender.
///
/// # Warning
///
/// This function is vulnerable to a race condition. If the owner changes the allowance
/// from N to M, a spender could potentially spend both N and M tokens if they
/// submit a transaction just before the allowance change.
/// Use `increase_allowance` and `decrease_allowance` to avoid this.
pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
    owner.require_auth();

    if amount < 0 {
        panic!("Amount must be non-negative");
    }

    if amount > MAX_ALLOWANCE {
        panic!("Max allowance exceeded");
    }

    storage_set_allowance(&env, &owner, &spender, amount);

    // Emit event
    env.events().publish(
        (soroban_sdk::symbol_short!("approve"), owner, spender),
        amount,
    );
}

/// Increase allowance for a spender.
pub fn increase_allowance(env: Env, owner: Address, spender: Address, delta: i128) {
    owner.require_auth();

    if delta < 0 {
        panic!("Delta must be non-negative");
    }

    let current = storage_get_allowance(&env, &owner, &spender);
    let new = current.checked_add(delta).expect("Allowance overflow");

    if new > MAX_ALLOWANCE {
        panic!("Max allowance exceeded");
    }

    storage_set_allowance(&env, &owner, &spender, new);

    // Emit event
    env.events().publish(
        (soroban_sdk::symbol_short!("approve"), owner, spender),
        new,
    );
}

/// Decrease allowance for a spender.
pub fn decrease_allowance(env: Env, owner: Address, spender: Address, delta: i128) {
    owner.require_auth();

    if delta < 0 {
        panic!("Delta must be non-negative");
    }

    let current = storage_get_allowance(&env, &owner, &spender);
    if current < delta {
        panic!("Allowance underflow");
    }
    let new = current.checked_sub(delta).expect("Allowance underflow");

    storage_set_allowance(&env, &owner, &spender, new);

    // Emit event
    env.events().publish(
        (soroban_sdk::symbol_short!("approve"), owner, spender),
        new,
    );
}

/// Get allowance for a spender from an owner.
pub fn get_allowance(env: Env, owner: Address, spender: Address) -> i128 {
    storage_get_allowance(&env, &owner, &spender)
}

/// Transfer tokens from one address to another using an allowance.
pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
    spender.require_auth();

    if amount <= 0 {
        panic!("Amount must be positive");
    }

    // Check and update allowance
    let current_allowance = storage_get_allowance(&env, &from, &spender);
    if current_allowance < amount {
        panic!("Insufficient allowance");
    }
    let new_allowance = current_allowance.checked_sub(amount).expect("Allowance underflow");
    storage_set_allowance(&env, &from, &spender, new_allowance);

    // Check and update balances
    if from == to {
        let balance = storage_get_balance(&env, &from);
        if balance < amount {
            panic!("Insufficient balance");
        }
        // Balance remains unchanged if from == to, but we still checked it.
    } else {
        let from_balance = storage_get_balance(&env, &from);
        if from_balance < amount {
            panic!("Insufficient balance");
        }
        let new_from_balance = from_balance.checked_sub(amount).expect("Balance underflow");
        let to_balance = storage_get_balance(&env, &to);
        let new_to_balance = to_balance.checked_add(amount).expect("Balance overflow");

        storage_set_balance(&env, &from, new_from_balance);
        storage_set_balance(&env, &to, new_to_balance);
    }

    // Emit event
    env.events().publish(
        (soroban_sdk::symbol_short!("transfer"), from, to),
        amount,
    );
}
