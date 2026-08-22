use soroban_sdk::{contracttype, Address, Env, Symbol, Val, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchOperation {
    pub contract: Address,
    pub function: Symbol,
    pub args: Vec<Val>,
}

pub fn execute_batch(env: &Env, ops: Vec<BatchOperation>) -> Vec<Val> {
    if ops.len() > 20 {
        panic!("Batch exceeds maximum of 20 operations");
    }

    let mut results: Vec<Val> = Vec::new(env);

    for op in ops.iter() {
        // Any failure here will bubble up and panic, reverting the entire transaction.
        // This satisfies "Atomic execution (all or nothing)" and "rollback on failure".
        let res: Val = env.invoke_contract(&op.contract, &op.function, op.args);
        results.push_back(res);
    }

    results
}

pub fn estimate_batch_gas(_env: &Env, ops: Vec<BatchOperation>) -> u64 {
    // Basic heuristic for estimating gas usage for batch operations off-chain.
    // Base cost + (per operation overhead * number of operations).
    let base_cost = 10_000;
    let per_op_cost = 5_000;
    
    let total_cost = base_cost + (ops.len() as u64 * per_op_cost);
    total_cost
}
