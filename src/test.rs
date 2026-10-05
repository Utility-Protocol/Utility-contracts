#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{symbol_short, Address, Env, Symbol};

const RESOURCE: Symbol = symbol_short!("SOLAR");

struct Fixture {
    env: Env,
    contract_id: Address,
    admin: Address,
    operator: Address,
    consumer: Address,
}

impl Fixture {
    /// Contract functions must run inside a contract frame; `as_contract`
    /// provides one and returns the function's own `Result` untouched.
    fn call<T>(&self, f: impl FnOnce() -> T) -> T {
        self.env.as_contract(&self.contract_id, f)
    }
}

fn fixture() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(UtilityProtocol, ());
    Fixture {
        contract_id,
        admin: Address::generate(&env),
        operator: Address::generate(&env),
        consumer: Address::generate(&env),
        env,
    }
}

/// admin + operator registered, tariff set to 10 per unit.
fn ready() -> Fixture {
    let f = fixture();
    assert_eq!(
        f.call(|| UtilityProtocol::initialize(f.env.clone(), f.admin.clone())),
        Ok(())
    );
    assert_eq!(
        f.call(|| {
            UtilityProtocol::register_operator(
                f.env.clone(),
                f.operator.clone(),
                RESOURCE,
                1_000,
            )
        }),
        Ok(())
    );
    assert_eq!(
        f.call(|| {
            UtilityProtocol::set_tariff(f.env.clone(), RESOURCE, 10)
        }),
        Ok(())
    );
    f
}

#[test]
fn initializes_once_and_rejects_reinit() {
    let f = fixture();
    assert_eq!(
        f.call(|| UtilityProtocol::initialize(f.env.clone(), f.admin.clone())),
        Ok(())
    );
    assert_eq!(
        f.call(|| UtilityProtocol::initialize(f.env.clone(), f.admin.clone())),
        Err(ContractError::AlreadyInitialized)
    );
}

#[test]
fn admin_can_set_and_read_tariff() {
    let f = fixture();
    f.call(|| UtilityProtocol::initialize(f.env.clone(), f.admin.clone()));

    assert_eq!(
        f.call(|| {
            UtilityProtocol::set_tariff(f.env.clone(), RESOURCE, 25)
        }),
        Ok(())
    );

    let t: TariffRate = f
        .call(|| UtilityProtocol::get_tariff(f.env.clone(), RESOURCE))
        .expect("tariff must be set");
    assert_eq!(t.resource_type, RESOURCE);
    assert_eq!(t.rate_per_unit, 25);
    assert_eq!(t.updated_at, f.env.ledger().timestamp());
}

#[test]
fn rejects_negative_tariff() {
    let f = fixture();
    f.call(|| UtilityProtocol::initialize(f.env.clone(), f.admin.clone()));
    assert_eq!(
        f.call(|| {
            UtilityProtocol::set_tariff(f.env.clone(), RESOURCE, -1)
        }),
        Err(ContractError::InvalidAmount)
    );
}

#[test]
fn registers_operator_with_stake() {
    let f = fixture();
    f.call(|| UtilityProtocol::initialize(f.env.clone(), f.admin.clone()));

    assert_eq!(
        f.call(|| {
            UtilityProtocol::register_operator(
                f.env.clone(),
                f.operator.clone(),
                RESOURCE,
                500_000,
            )
        }),
        Ok(())
    );

    let o: OperatorRecord = f
        .call(|| UtilityProtocol::get_operator(f.env.clone(), f.operator.clone()))
        .expect("operator must be registered");
    assert_eq!(o.operator, f.operator);
    assert_eq!(o.staked_amount, 500_000);
    assert_eq!(o.resource_type, RESOURCE);
    assert!(o.is_active);
    assert_eq!(o.accrued_earnings, 0);
    assert_eq!(o.total_paid, 0);
}

#[test]
fn rejects_duplicate_operator_registration() {
    let f = fixture();
    f.call(|| UtilityProtocol::initialize(f.env.clone(), f.admin.clone()));
    let _ = f.call(|| {
        UtilityProtocol::register_operator(f.env.clone(), f.operator.clone(), RESOURCE, 1_000)
    });
    assert_eq!(
        f.call(|| {
            UtilityProtocol::register_operator(f.env.clone(), f.operator.clone(), RESOURCE, 1_000)
        }),
        Err(ContractError::Unauthorized)
    );
}

#[test]
fn deposit_credits_escrow() {
    let f = fixture();
    f.call(|| UtilityProtocol::initialize(f.env.clone(), f.admin.clone()));

    let _ = f.call(|| {
        UtilityProtocol::deposit_escrow(f.env.clone(), f.consumer.clone(), 10_000)
    });
    let _ = f.call(|| {
        UtilityProtocol::deposit_escrow(f.env.clone(), f.consumer.clone(), 5_000)
    });

    let c: ConsumerRecord = f
        .call(|| UtilityProtocol::get_consumer(f.env.clone(), f.consumer.clone()))
        .expect("consumer must exist");
    assert_eq!(c.escrow_balance, 15_000);
    assert_eq!(c.total_units_consumed, 0);
}

#[test]
fn usage_tick_deducts_escrow_and_credits_operator() {
    let f = ready();
    let _ = f.call(|| {
        UtilityProtocol::deposit_escrow(f.env.clone(), f.consumer.clone(), 1_000)
    });

    assert_eq!(
        f.call(|| {
            UtilityProtocol::record_usage_tick(
                f.env.clone(),
                f.consumer.clone(),
                f.operator.clone(),
                RESOURCE,
                20,
                1,
            )
        }),
        Ok(())
    );

    let c: ConsumerRecord = f
        .call(|| UtilityProtocol::get_consumer(f.env.clone(), f.consumer.clone()))
        .expect("consumer must exist");
    assert_eq!(c.escrow_balance, 800); // 1000 - (20 units * 10 rate)
    assert_eq!(c.total_units_consumed, 20);
    assert_eq!(c.last_meter_sequence, 1);

    let o: OperatorRecord = f
        .call(|| UtilityProtocol::get_operator(f.env.clone(), f.operator.clone()))
        .expect("operator must exist");
    assert_eq!(o.accrued_earnings, 200);

    assert_eq!(
        f.call(|| UtilityProtocol::get_total_settled_volume(f.env.clone())),
        20
    );
}

#[test]
fn usage_tick_emits_structured_event() {
    let f = ready();
    let _ = f.call(|| {
        UtilityProtocol::deposit_escrow(f.env.clone(), f.consumer.clone(), 1_000)
    });
    let _ = f.call(|| {
        UtilityProtocol::record_usage_tick(
            f.env.clone(),
            f.consumer.clone(),
            f.operator.clone(),
            RESOURCE,
            5,
            1,
        )
    });

    let events = f.env.events().all();
    let raw = events.events();
    assert_eq!(raw.len(), 1, "expected exactly one util_tick event");

    let ev = raw.last().unwrap();
    let ContractEventBody::V0(v0) = &ev.body;
    let _ = v0;

    // Topics: (util_tick, consumer, resource_type). VecM derefs to Vec.
    assert_eq!(v0.topics.len(), 3);
    assert!(
        matches!(v0.topics[0], ScVal::Symbol(_)),
        "first topic must be the event-name symbol"
    );
    assert!(
        matches!(v0.topics[1], ScVal::Address(_)),
        "second topic must be the consumer address"
    );
    assert!(
        matches!(v0.topics[2], ScVal::Symbol(_)),
        "third topic must be the resource_type symbol"
    );

    // Data: (operator, units_drawn, total_cost, timestamp) -> Vec of 4
    match &v0.data {
        ScVal::Vec(Some(items)) => assert_eq!(items.len(), 4),
        other => panic!("expected Vec data payload, got {other:?}"),
    }
}

#[test]
fn rejects_replayed_and_out_of_order_sequences() {
    let f = ready();
    let _ = f.call(|| {
        UtilityProtocol::deposit_escrow(f.env.clone(), f.consumer.clone(), 10_000)
    });

    let tick = |seq: u64| {
        f.call(|| {
            UtilityProtocol::record_usage_tick(
                f.env.clone(),
                f.consumer.clone(),
                f.operator.clone(),
                RESOURCE,
                10,
                seq,
            )
        })
    };

    // sequence 5 accepted
    assert_eq!(tick(5), Ok(()));

    // exact replay of 5 rejected
    assert_eq!(tick(5), Err(ContractError::InvalidSequence));

    // reordered (older) packet rejected
    assert_eq!(tick(4), Err(ContractError::InvalidSequence));

    // sequence 0 rejected: must be strictly greater than 0
    assert_eq!(tick(0), Err(ContractError::InvalidSequence));

    // only the first tick was ever applied
    let c: ConsumerRecord = f
        .call(|| UtilityProtocol::get_consumer(f.env.clone(), f.consumer.clone()))
        .expect("consumer must exist");
    assert_eq!(c.total_units_consumed, 10);
    assert_eq!(c.escrow_balance, 9_900);
}

#[test]
fn rejects_usage_when_escrow_exhausted() {
    let f = ready();
    // tariff is 10/unit; fund 500 so exactly 50 units are affordable
    let _ = f.call(|| {
        UtilityProtocol::deposit_escrow(f.env.clone(), f.consumer.clone(), 500)
    });

    assert_eq!(
        f.call(|| {
            UtilityProtocol::record_usage_tick(
                f.env.clone(),
                f.consumer.clone(),
                f.operator.clone(),
                RESOURCE,
                50,
                1,
            )
        }),
        Ok(())
    );

    // any further draw now fails
    assert_eq!(
        f.call(|| {
            UtilityProtocol::record_usage_tick(
                f.env.clone(),
                f.consumer.clone(),
                f.operator.clone(),
                RESOURCE,
                1,
                2,
            )
        }),
        Err(ContractError::InsufficientEscrow)
    );

    let c: ConsumerRecord = f
        .call(|| UtilityProtocol::get_consumer(f.env.clone(), f.consumer.clone()))
        .expect("consumer must exist");
    assert_eq!(c.escrow_balance, 0);
    assert_eq!(c.total_units_consumed, 50);
}

#[test]
fn rejects_unknown_consumer_operator_and_tariff() {
    let f = fixture();
    f.call(|| UtilityProtocol::initialize(f.env.clone(), f.admin.clone()));
    let _ = f.call(|| {
        UtilityProtocol::register_operator(f.env.clone(), f.operator.clone(), RESOURCE, 1_000)
    });

    // no consumer record yet
    assert_eq!(
        f.call(|| {
            UtilityProtocol::record_usage_tick(
                f.env.clone(),
                f.consumer.clone(),
                f.operator.clone(),
                RESOURCE,
                1,
                1,
            )
        }),
        Err(ContractError::ConsumerNotFound)
    );

    let _ = f.call(|| {
        UtilityProtocol::deposit_escrow(f.env.clone(), f.consumer.clone(), 100)
    });

    // tariff still unset
    assert_eq!(
        f.call(|| {
            UtilityProtocol::record_usage_tick(
                f.env.clone(),
                f.consumer.clone(),
                f.operator.clone(),
                RESOURCE,
                1,
                1,
            )
        }),
        Err(ContractError::TariffNotFound)
    );

    let _ = f.call(|| {
        UtilityProtocol::set_tariff(f.env.clone(), RESOURCE, 10)
    });

    // unregistered operator
    let stranger = Address::generate(&f.env);
    assert_eq!(
        f.call(|| {
            UtilityProtocol::record_usage_tick(
                f.env.clone(),
                f.consumer.clone(),
                stranger,
                RESOURCE,
                1,
                1,
            )
        }),
        Err(ContractError::OperatorNotFound)
    );
}

#[test]
fn settle_payout_moves_accrued_to_paid() {
    let f = ready();
    let _ = f.call(|| {
        UtilityProtocol::deposit_escrow(f.env.clone(), f.consumer.clone(), 1_000)
    });
    let _ = f.call(|| {
        UtilityProtocol::record_usage_tick(
            f.env.clone(),
            f.consumer.clone(),
            f.operator.clone(),
            RESOURCE,
            20,
            1,
        )
    });

    assert_eq!(
        f.call(|| {
            UtilityProtocol::settle_operator_payout(f.env.clone(), f.operator.clone(), 200)
        }),
        Ok(())
    );

    let o: OperatorRecord = f
        .call(|| UtilityProtocol::get_operator(f.env.clone(), f.operator.clone()))
        .expect("operator must exist");
    assert_eq!(o.accrued_earnings, 0);
    assert_eq!(o.total_paid, 200);

    // escrow is untouched: payout only moves stake accounting
    let c: ConsumerRecord = f
        .call(|| UtilityProtocol::get_consumer(f.env.clone(), f.consumer.clone()))
        .expect("consumer must exist");
    assert_eq!(c.escrow_balance, 800);

    // over-withdraw rejected
    assert_eq!(
        f.call(|| {
            UtilityProtocol::settle_operator_payout(f.env.clone(), f.operator.clone(), 1)
        }),
        Err(ContractError::InsufficientAccrued)
    );
}

#[test]
fn unsettled_earnings_accumulate_across_ticks() {
    let f = ready();
    let _ = f.call(|| {
        UtilityProtocol::deposit_escrow(f.env.clone(), f.consumer.clone(), 10_000)
    });

    for seq in 1..=3_u64 {
        f.call(|| {
            UtilityProtocol::record_usage_tick(
                f.env.clone(),
                f.consumer.clone(),
                f.operator.clone(),
                RESOURCE,
                10,
                seq,
            )
        });
    }

    let o: OperatorRecord = f
        .call(|| UtilityProtocol::get_operator(f.env.clone(), f.operator.clone()))
        .expect("operator must exist");
    assert_eq!(o.accrued_earnings, 300); // 3 ticks * (10 units * 10 rate)

    let c: ConsumerRecord = f
        .call(|| UtilityProtocol::get_consumer(f.env.clone(), f.consumer.clone()))
        .expect("consumer must exist");
    assert_eq!(c.escrow_balance, 9_700);
    assert_eq!(c.total_units_consumed, 30);
    assert_eq!(c.last_meter_sequence, 3);

    assert_eq!(
        f.call(|| UtilityProtocol::get_total_settled_volume(f.env.clone())),
        30
    );
}