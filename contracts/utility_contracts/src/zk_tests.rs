#![cfg(test)]
use super::*;
use soroban_sdk::{Env, Bytes, Vec, BytesN, testutils::Address as _};

#[test]
fn test_zk_privacy_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_address = Address::generate(&env);

    let device_public_key = BytesN::from_array(&env, &[1u8; 32]);
    let meter_id = client.register_meter(&user, &provider, &100, &token_address, &device_public_key, &0, &0);

    // Enable privacy mode
    client.enable_privacy_mode(&meter_id);
    assert!(client.is_privacy_enabled(&meter_id));

    // Set dummy verification key (random points, will not pass pairing check)
    let vk = Groth16VerificationKey {
        alpha_g1: Bytes::from_slice(&env, &[0u8; 64]),
        beta_g2: Bytes::from_slice(&env, &[0u8; 128]),
        gamma_g2: Bytes::from_slice(&env, &[0u8; 128]),
        delta_g2: Bytes::from_slice(&env, &[0u8; 128]),
        ic: {
            // 2 fixed IC points — use vec! macro to avoid push_back overhead
            soroban_sdk::vec![
                &env,
                Bytes::from_slice(&env, &[0u8; 64]), // IC[0]
                Bytes::from_slice(&env, &[0u8; 64]), // IC[1] (for amount)
            ]
        },
    };
    client.set_zk_verification_key(&meter_id, &vk);

    // Top up to have balance
    client.top_up(&meter_id, &10000);

    // Prepare mock proof
    let proof = Groth16Proof {
        a: Bytes::from_slice(&env, &[0u8; 64]),
        b: Bytes::from_slice(&env, &[0u8; 128]),
        c: Bytes::from_slice(&env, &[0u8; 64]),
    };

    // Amount = 10 units (serialized as 16-byte BE)
    let mut amount_arr = [0u8; 16];
    amount_arr[15] = 10;
    let amount_bytes = Bytes::from_slice(&env, &amount_arr);

    // 1 fixed public input — use vec! macro to avoid push_back overhead
    let public_inputs = soroban_sdk::vec![&env, amount_bytes];

    let nullifier = BytesN::from_array(&env, &[2u8; 32]);

    // This should fail because the dummy proof is invalid
    let result = env.try_invoke_contract::<(), ContractError>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "submit_zk_usage_report"),
        (meter_id, proof.clone(), public_inputs.clone(), nullifier.clone()),
    );

    assert!(result.is_err());
    // In a real scenario with valid proof, it would succeed and deduct balance.
}

#[test]
fn test_negate_g1() {
    let env = Env::default();
    let point = Bytes::from_slice(&env, &[1u8; 64]);
    let negated = negate_g1(&env, &point);
    
    assert_eq!(negated.slice(0..32), point.slice(0..32)); // X should be same
    assert_ne!(negated.slice(32..64), point.slice(32..64)); // Y should be different
}
