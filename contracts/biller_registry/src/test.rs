#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, BytesN as _, Events as _, Ledger as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{xdr, Bytes, Env, IntoVal};

struct Fixture {
    env: Env,
    admin: Address,
    contract: Address,
    client: BillerRegistryClient<'static>,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = env.register(BillerRegistry, ());
    let client = BillerRegistryClient::new(&env, &contract);
    client.init(&admin);
    Fixture {
        env,
        admin,
        contract,
        client,
    }
}

fn name_hash(env: &Env, name: &str) -> BytesN<32> {
    env.crypto()
        .sha256(&Bytes::from_slice(env, name.as_bytes()))
        .into()
}

/// Topic names (first topic symbol) of the events published by the last invocation.
fn last_event_names(env: &Env) -> std::vec::Vec<std::string::String> {
    env.events()
        .all()
        .events()
        .iter()
        .map(|e| {
            let xdr::ContractEventBody::V0(v0) = &e.body;
            match v0.topics.first() {
                Some(xdr::ScVal::Symbol(s)) => s.0.to_utf8_string_lossy(),
                _ => std::string::String::from("?"),
            }
        })
        .collect()
}

fn register_hospital(f: &Fixture) -> u32 {
    let addr = Address::generate(&f.env);
    f.client.register_biller(
        &name_hash(&f.env, "Kenyatta National Hospital / KNH-INV-2211"),
        &symbol_short!("hospital"),
        &addr,
    )
}

// ------------------------------------------------------------------ init

#[test]
fn init_sets_admin_and_emits_event() {
    let f = setup();
    assert_eq!(f.client.admin(), f.admin);
    assert_eq!(f.client.biller_count(), 0);
}

#[test]
fn init_twice_is_rejected() {
    let f = setup();
    let other = Address::generate(&f.env);
    assert_eq!(
        f.client.try_init(&other),
        Err(Ok(Error::AlreadyInitialized))
    );
    assert_eq!(f.client.admin(), f.admin);
}

#[test]
fn calls_before_init_are_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(BillerRegistry, ());
    let client = BillerRegistryClient::new(&env, &contract);
    let res = client.try_register_biller(
        &BytesN::random(&env),
        &symbol_short!("school"),
        &Address::generate(&env),
    );
    assert_eq!(res, Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_admin(), Err(Ok(Error::NotInitialized)));
}

// ------------------------------------------------------------------ register

#[test]
fn register_biller_assigns_sequential_ids_and_starts_unverified() {
    let f = setup();
    let hash = name_hash(&f.env, "Moi Girls High School, Eldoret");
    let addr = Address::generate(&f.env);
    let id0 = register_hospital(&f);
    let id1 = f
        .client
        .register_biller(&hash, &symbol_short!("school"), &addr);
    // Read the event buffer before any further invocation clears it.
    assert_eq!(last_event_names(&f.env), std::vec!["biller_registered"]);
    assert_eq!(id0, 0);
    assert_eq!(id1, 1);
    assert_eq!(f.client.biller_count(), 2);

    let b = f.client.biller(&id1);
    assert_eq!(
        b,
        Biller {
            id: 1,
            name_hash: hash,
            category: symbol_short!("school"),
            address: addr,
            verified: false,
            active: true,
        }
    );
    // Registered but not yet verified: cannot be paid.
    assert!(!f.client.is_payable(&id1));
}

#[test]
fn register_biller_rejects_unknown_category() {
    let f = setup();
    let res = f.client.try_register_biller(
        &BytesN::random(&f.env),
        &symbol_short!("casino"),
        &Address::generate(&f.env),
    );
    assert_eq!(res, Err(Ok(Error::InvalidCategory)));
    assert_eq!(f.client.biller_count(), 0);
}

#[test]
fn register_biller_accepts_every_known_category() {
    let f = setup();
    for cat in [
        symbol_short!("hospital"),
        symbol_short!("school"),
        symbol_short!("funeral"),
        symbol_short!("utility"),
        symbol_short!("other"),
    ] {
        f.client
            .register_biller(&BytesN::random(&f.env), &cat, &Address::generate(&f.env));
    }
    assert_eq!(f.client.biller_count(), 5);
}

#[test]
fn register_biller_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract = env.register(BillerRegistry, ());
    let client = BillerRegistryClient::new(&env, &contract);
    client.mock_all_auths().init(&admin);

    let intruder = Address::generate(&env);
    let hash = BytesN::random(&env);
    let addr = Address::generate(&env);
    // Only the intruder signs: the admin's require_auth must fail.
    env.mock_auths(&[MockAuth {
        address: &intruder,
        invoke: &MockAuthInvoke {
            contract: &contract,
            fn_name: "register_biller",
            args: (hash.clone(), symbol_short!("hospital"), addr.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let res = client.try_register_biller(&hash, &symbol_short!("hospital"), &addr);
    assert!(matches!(res, Err(Err(_))), "expected an auth failure, got {res:?}");
    assert_eq!(client.biller_count(), 0);

    // With the admin signing it works, and the auth tree records the admin only.
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract,
            fn_name: "register_biller",
            args: (hash.clone(), symbol_short!("hospital"), addr.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let id = client.register_biller(&hash, &symbol_short!("hospital"), &addr);
    assert_eq!(id, 0);
    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, admin);
}

// ------------------------------------------------------------------ verified / active

#[test]
fn set_verified_and_set_active_drive_is_payable() {
    let f = setup();
    let id = register_hospital(&f);
    assert!(!f.client.is_payable(&id));

    f.client.set_verified(&id, &true);
    assert_eq!(last_event_names(&f.env), std::vec!["biller_verified"]);
    assert!(f.client.biller(&id).verified);
    assert!(f.client.is_payable(&id));

    f.client.set_active(&id, &false);
    assert_eq!(last_event_names(&f.env), std::vec!["biller_active"]);
    assert!(!f.client.biller(&id).active);
    assert!(!f.client.is_payable(&id));

    f.client.set_active(&id, &true);
    assert!(f.client.is_payable(&id));

    f.client.set_verified(&id, &false);
    assert!(!f.client.is_payable(&id));
}

#[test]
fn is_payable_is_false_for_unknown_id() {
    let f = setup();
    assert!(!f.client.is_payable(&42));
}

#[test]
fn biller_view_rejects_unknown_id() {
    let f = setup();
    assert_eq!(f.client.try_biller(&7), Err(Ok(Error::BillerNotFound)));
    assert_eq!(
        f.client.try_set_verified(&7, &true),
        Err(Ok(Error::BillerNotFound))
    );
    assert_eq!(
        f.client.try_set_active(&7, &false),
        Err(Ok(Error::BillerNotFound))
    );
}

#[test]
fn set_verified_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract = env.register(BillerRegistry, ());
    let client = BillerRegistryClient::new(&env, &contract);
    client.mock_all_auths().init(&admin);
    let id = client.mock_all_auths().register_biller(
        &BytesN::random(&env),
        &symbol_short!("funeral"),
        &Address::generate(&env),
    );

    let intruder = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &intruder,
        invoke: &MockAuthInvoke {
            contract: &contract,
            fn_name: "set_verified",
            args: (id, true).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(matches!(client.try_set_verified(&id, &true), Err(Err(_))));
    assert!(!client.is_payable(&id));

    env.mock_auths(&[MockAuth {
        address: &intruder,
        invoke: &MockAuthInvoke {
            contract: &contract,
            fn_name: "set_active",
            args: (id, false).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(matches!(client.try_set_active(&id, &false), Err(Err(_))));
    assert!(client.biller(&id).active);
}

// ------------------------------------------------------------------ address change delay

#[test]
fn propose_address_records_pending_change_with_24h_delay() {
    let f = setup();
    let id = register_hospital(&f);
    let original = f.client.biller(&id).address.clone();
    f.env.ledger().with_mut(|l| l.timestamp = 1_700_000_000);

    let new_addr = Address::generate(&f.env);
    let apply_after = f.client.propose_address(&id, &new_addr);
    assert_eq!(apply_after, 1_700_000_000 + ADDRESS_CHANGE_DELAY);
    assert_eq!(last_event_names(&f.env), std::vec!["address_proposed"]);
    assert_eq!(
        f.client.pending_address(&id),
        Some(PendingAddress {
            new_address: new_addr,
            proposed_at: 1_700_000_000,
            apply_after,
        })
    );
    // Nothing changes until it is applied.
    assert_eq!(f.client.biller(&id).address, original);
}

#[test]
fn propose_address_rejects_same_address_and_unknown_biller() {
    let f = setup();
    let id = register_hospital(&f);
    let current = f.client.biller(&id).address.clone();
    assert_eq!(
        f.client.try_propose_address(&id, &current),
        Err(Ok(Error::SameAddress))
    );
    assert_eq!(
        f.client.try_propose_address(&99, &Address::generate(&f.env)),
        Err(Ok(Error::BillerNotFound))
    );
}

#[test]
fn apply_address_before_delay_is_rejected() {
    let f = setup();
    let id = register_hospital(&f);
    f.env.ledger().with_mut(|l| l.timestamp = 1_700_000_000);
    let new_addr = Address::generate(&f.env);
    f.client.propose_address(&id, &new_addr);

    // One second short of the delay.
    f.env
        .ledger()
        .with_mut(|l| l.timestamp = 1_700_000_000 + ADDRESS_CHANGE_DELAY - 1);
    assert_eq!(
        f.client.try_apply_address(&id),
        Err(Ok(Error::DelayNotElapsed))
    );
    assert_ne!(f.client.biller(&id).address, new_addr);
}

#[test]
fn apply_address_without_pending_change_is_rejected() {
    let f = setup();
    let id = register_hospital(&f);
    assert_eq!(
        f.client.try_apply_address(&id),
        Err(Ok(Error::NoPendingChange))
    );
    assert_eq!(
        f.client.try_cancel_address(&id),
        Err(Ok(Error::NoPendingChange))
    );
}

#[test]
fn cancel_address_clears_pending_change() {
    let f = setup();
    let id = register_hospital(&f);
    let original = f.client.biller(&id).address.clone();
    f.client
        .propose_address(&id, &Address::generate(&f.env));
    f.client.cancel_address(&id);
    assert_eq!(last_event_names(&f.env), std::vec!["address_cancelled"]);
    assert_eq!(f.client.pending_address(&id), None);
    f.env
        .ledger()
        .with_mut(|l| l.timestamp += ADDRESS_CHANGE_DELAY + 1);
    assert_eq!(
        f.client.try_apply_address(&id),
        Err(Ok(Error::NoPendingChange))
    );
    assert_eq!(f.client.biller(&id).address, original);
}

#[test]
fn reproposing_restarts_the_clock() {
    let f = setup();
    let id = register_hospital(&f);
    f.env.ledger().with_mut(|l| l.timestamp = 1_000);
    f.client
        .propose_address(&id, &Address::generate(&f.env));
    // 20 hours later the admin proposes a different address.
    f.env.ledger().with_mut(|l| l.timestamp = 1_000 + 20 * 3600);
    let second = Address::generate(&f.env);
    let apply_after = f.client.propose_address(&id, &second);
    assert_eq!(apply_after, 1_000 + 20 * 3600 + ADDRESS_CHANGE_DELAY);
    // 25 hours after the first proposal is still too early for the second one.
    f.env.ledger().with_mut(|l| l.timestamp = 1_000 + 25 * 3600);
    assert_eq!(
        f.client.try_apply_address(&id),
        Err(Ok(Error::DelayNotElapsed))
    );
}

#[test]
fn propose_and_apply_address_require_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract = env.register(BillerRegistry, ());
    let client = BillerRegistryClient::new(&env, &contract);
    client.mock_all_auths().init(&admin);
    let id = client.mock_all_auths().register_biller(
        &BytesN::random(&env),
        &symbol_short!("utility"),
        &Address::generate(&env),
    );

    let intruder = Address::generate(&env);
    let new_addr = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &intruder,
        invoke: &MockAuthInvoke {
            contract: &contract,
            fn_name: "propose_address",
            args: (id, new_addr.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(matches!(
        client.try_propose_address(&id, &new_addr),
        Err(Err(_))
    ));
    assert_eq!(client.pending_address(&id), None);

    client.mock_all_auths().propose_address(&id, &new_addr);
    env.ledger()
        .with_mut(|l| l.timestamp += ADDRESS_CHANGE_DELAY);
    env.mock_auths(&[MockAuth {
        address: &intruder,
        invoke: &MockAuthInvoke {
            contract: &contract,
            fn_name: "apply_address",
            args: (id,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(matches!(client.try_apply_address(&id), Err(Err(_))));
    assert_ne!(client.biller(&id).address, new_addr);
}

/// Scenario: the hospital moves its collection account. The admin proposes the change,
/// the committee has a full day to object, and the change only lands once the delay has
/// elapsed. Payability is unaffected throughout.
#[test]
fn scenario_registry_address_change_delay_enforced() {
    let f = setup();
    let t0: u64 = 1_758_672_000; // 2025-09-24T00:00:00Z
    f.env.ledger().with_mut(|l| l.timestamp = t0);

    let id = register_hospital(&f);
    f.client.set_verified(&id, &true);
    let old_addr = f.client.biller(&id).address.clone();
    assert!(f.client.is_payable(&id));

    let new_addr = Address::generate(&f.env);
    let apply_after = f.client.propose_address(&id, &new_addr);
    assert_eq!(apply_after, t0 + 86_400);

    // Same block: rejected.
    assert_eq!(
        f.client.try_apply_address(&id),
        Err(Ok(Error::DelayNotElapsed))
    );
    // 23h59m59s later: still rejected.
    f.env.ledger().with_mut(|l| l.timestamp = t0 + 86_399);
    assert_eq!(
        f.client.try_apply_address(&id),
        Err(Ok(Error::DelayNotElapsed))
    );
    assert_eq!(f.client.biller(&id).address, old_addr);
    assert!(f.client.is_payable(&id));

    // Exactly 24h later: applied.
    f.env.ledger().with_mut(|l| l.timestamp = t0 + 86_400);
    f.client.apply_address(&id);
    assert_eq!(last_event_names(&f.env), std::vec!["address_applied"]);
    let b = f.client.biller(&id);
    assert_eq!(b.address, new_addr);
    assert!(b.verified && b.active);
    assert!(f.client.is_payable(&id));
    assert_eq!(f.client.pending_address(&id), None);

    // Applying again has nothing to apply.
    assert_eq!(
        f.client.try_apply_address(&id),
        Err(Ok(Error::NoPendingChange))
    );
    // The contract address is what the campaign contract will call.
    assert_eq!(f.client.address, f.contract);
}
