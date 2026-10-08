#![cfg(test)]
extern crate std;

use super::*;
use biller_registry::{BillerRegistry, BillerRegistryClient};
use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::testutils::{
    Address as _, AuthorizedFunction, AuthorizedInvocation, BytesN as _, Events as _, Ledger as _,
    MockAuth, MockAuthInvoke,
};
use soroban_sdk::{token, xdr, Bytes, Env, IntoVal};

/// One whole token unit (USDC on Stellar has 7 decimals).
const UNIT: i128 = 10_000_000;
/// One cent.
const CENT: i128 = UNIT / 100;
/// 2025-09-24T00:00:00Z
const T0: u64 = 1_758_672_000;
const DAY: u64 = 86_400;

/// The 40 seeded contributions from data/seed/contributors.csv, in cents. They sum to
/// $3,311.82; the medical campaign's goal is $3,000.00.
const SEED_CENTS: [i128; 40] = [
    5000, 2537, 10000, 50000, 500, 1250, 7525, 2000, 3333, 25000, 4713, 1500, 6060, 1099, 20000,
    875, 12000, 4500, 555, 30000, 1999, 6666, 3000, 2222, 15000, 777, 8888, 4000, 1111, 49999,
    2750, 5500, 909, 17500, 1313, 3535, 9000, 606, 6142, 1818,
];
const SEED_TOTAL: i128 = 331_182 * CENT;

fn cents(c: i128) -> i128 {
    c * CENT
}

fn h(env: &Env, s: &str) -> BytesN<32> {
    env.crypto()
        .sha256(&Bytes::from_slice(env, s.as_bytes()))
        .into()
}

/// Biller ids as registered by `World::new`.
const HOSPITAL: u32 = 0;
const SCHOOL: u32 = 1;
const FUNERAL: u32 = 2;
const PHARMACY: u32 = 3;
const UNVERIFIED_CLINIC: u32 = 4;

struct World {
    env: Env,
    token: Address,
    sac: token::StellarAssetClient<'static>,
    tok: token::TokenClient<'static>,
    registry: Address,
    reg: BillerRegistryClient<'static>,
    contract: Address,
    client: CampaignContractClient<'static>,
    organizer: Address,
    committee: std::vec::Vec<Address>,
}

impl World {
    fn new(committee_size: usize) -> World {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = T0);

        let token_admin = Address::generate(&env);
        let sac_contract = env.register_stellar_asset_contract_v2(token_admin);
        let token = sac_contract.address();
        let sac = token::StellarAssetClient::new(&env, &token);
        let tok = token::TokenClient::new(&env, &token);

        let registry = env.register(BillerRegistry, ());
        let reg = BillerRegistryClient::new(&env, &registry);
        reg.init(&Address::generate(&env));
        let billers = [
            ("Kenyatta National Hospital / KNH-2211", symbol_short!("hospital"), true),
            ("Moi Girls High School Eldoret / MGHS", symbol_short!("school"), true),
            ("Lee Funeral Home Nairobi / LFH", symbol_short!("funeral"), true),
            ("Goodlife Pharmacy Kenyatta Ave / GLP-14", symbol_short!("other"), true),
            ("Mwangi Clinic & Chemist / pending KYC", symbol_short!("hospital"), false),
        ];
        for (i, (name, category, verified)) in billers.iter().enumerate() {
            let id = reg.register_biller(&h(&env, name), category, &Address::generate(&env));
            assert_eq!(id as usize, i);
            if *verified {
                reg.set_verified(&id, &true);
            }
        }

        let contract = env.register(CampaignContract, ());
        let client = CampaignContractClient::new(&env, &contract);
        let organizer = Address::generate(&env);
        let committee: std::vec::Vec<Address> =
            (0..committee_size).map(|_| Address::generate(&env)).collect();

        World {
            env,
            token,
            sac,
            tok,
            registry,
            reg,
            contract,
            client,
            organizer,
            committee,
        }
    }

    fn committee_vec(&self) -> Vec<Address> {
        let mut v = Vec::new(&self.env);
        for a in &self.committee {
            v.push_back(a.clone());
        }
        v
    }

    fn create(&self, goal: i128, threshold: u32) -> u32 {
        self.client.create(
            &self.organizer,
            &self.token,
            &goal,
            &(T0 + 30 * DAY),
            &self.committee_vec(),
            &threshold,
            &self.registry,
            &h(&self.env, "Mama Njeri surgery fund - Nakuru chapter"),
        )
    }

    fn funded_contributor(&self, amount: i128) -> Address {
        let a = Address::generate(&self.env);
        self.sac.mint(&a, &amount);
        a
    }

    fn give(&self, id: u32, amount: i128, memo: &str) -> Address {
        let a = self.funded_contributor(amount);
        self.client
            .contribute(&id, &a, &amount, &h(&self.env, memo));
        a
    }

    fn biller_address(&self, biller_id: u32) -> Address {
        self.reg.biller(&biller_id).address
    }

    fn last_event_names(&self) -> std::vec::Vec<std::string::String> {
        self.env
            .events()
            .all()
            .filter_by_contract(&self.contract)
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

    fn campaign_ttl(&self, id: u32) -> u32 {
        self.env.as_contract(&self.contract, || {
            self.env
                .storage()
                .persistent()
                .get_ttl(&DataKey::Campaign(id))
        })
    }
}

// ================================================================== create

#[test]
fn create_stores_campaign_and_records_organizer_auth() {
    let w = World::new(3);
    let title = h(&w.env, "Mama Njeri surgery fund - Nakuru chapter");
    let id = w.create(cents(300_000), 2);
    let auths = w.env.auths();
    assert_eq!(id, 0);
    assert_eq!(w.last_event_names(), std::vec!["campaign_created"]);
    assert_eq!(w.client.campaign_count(), 1);

    let c = w.client.campaign(&id);
    assert_eq!(
        c,
        Campaign {
            organizer: w.organizer.clone(),
            token: w.token.clone(),
            goal: cents(300_000),
            raised: 0,
            deadline: T0 + 30 * DAY,
            committee: w.committee_vec(),
            threshold: 2,
            registry: w.registry.clone(),
            state: State::Open,
            title_hash: title.clone(),
            paid_out: 0,
            refunded: 0,
            contribution_count: 0,
            payout_count: 0,
            created_at: T0,
        }
    );
    assert_eq!(
        auths,
        std::vec![(
            w.organizer.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    w.contract.clone(),
                    symbol_short!("create"),
                    (
                        w.organizer.clone(),
                        w.token.clone(),
                        cents(300_000),
                        T0 + 30 * DAY,
                        w.committee_vec(),
                        2u32,
                        w.registry.clone(),
                        title,
                    )
                        .into_val(&w.env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
    // A second campaign gets the next id.
    assert_eq!(w.create(cents(1000), 1), 1);
    assert_eq!(w.client.campaign_count(), 2);
}

#[test]
fn create_extends_entry_ttl() {
    let w = World::new(2);
    let id = w.create(cents(1000), 1);
    assert!(w.campaign_ttl(id) >= TTL_EXTEND_TO - 1);
}

#[test]
fn create_rejects_bad_goal_and_deadline() {
    let w = World::new(3);
    let committee = w.committee_vec();
    let title = BytesN::random(&w.env);
    for goal in [0i128, -1] {
        let res = w.client.try_create(
            &w.organizer,
            &w.token,
            &goal,
            &(T0 + DAY),
            &committee,
            &2,
            &w.registry,
            &title,
        );
        assert_eq!(res, Err(Ok(Error::InvalidGoal)));
    }
    for deadline in [0u64, T0 - 1, T0] {
        let res = w.client.try_create(
            &w.organizer,
            &w.token,
            &cents(1000),
            &deadline,
            &committee,
            &2,
            &w.registry,
            &title,
        );
        assert_eq!(res, Err(Ok(Error::InvalidDeadline)));
    }
    assert_eq!(w.client.campaign_count(), 0);
}

#[test]
fn create_rejects_bad_committee() {
    let w = World::new(8);
    let title = BytesN::random(&w.env);
    let attempt = |committee: Vec<Address>, threshold: u32| {
        w.client.try_create(
            &w.organizer,
            &w.token,
            &cents(1000),
            &(T0 + DAY),
            &committee,
            &threshold,
            &w.registry,
            &title,
        )
    };
    // Empty committee.
    assert_eq!(
        attempt(Vec::new(&w.env), 1),
        Err(Ok(Error::InvalidCommittee))
    );
    // Eight members.
    assert_eq!(
        attempt(w.committee_vec(), 3),
        Err(Ok(Error::InvalidCommittee))
    );
    // Duplicate member.
    let dup = vec![
        &w.env,
        w.committee[0].clone(),
        w.committee[1].clone(),
        w.committee[0].clone(),
    ];
    assert_eq!(attempt(dup, 2), Err(Ok(Error::InvalidCommittee)));
    // Threshold out of range.
    let three = vec![
        &w.env,
        w.committee[0].clone(),
        w.committee[1].clone(),
        w.committee[2].clone(),
    ];
    assert_eq!(
        attempt(three.clone(), 0),
        Err(Ok(Error::InvalidThreshold))
    );
    assert_eq!(attempt(three.clone(), 4), Err(Ok(Error::InvalidThreshold)));
    // Boundaries that are fine: 1-of-1 and 7-of-7.
    assert!(attempt(vec![&w.env, w.committee[0].clone()], 1).is_ok());
    let mut seven = Vec::new(&w.env);
    for a in w.committee.iter().take(7) {
        seven.push_back(a.clone());
    }
    assert!(attempt(seven, 7).is_ok());
}

#[test]
fn create_requires_organizer_auth() {
    let w = World::new(2);
    let intruder = Address::generate(&w.env);
    let committee = w.committee_vec();
    let title = BytesN::random(&w.env);
    let args = (
        w.organizer.clone(),
        w.token.clone(),
        cents(1000),
        T0 + DAY,
        committee.clone(),
        1u32,
        w.registry.clone(),
        title.clone(),
    );
    w.env.mock_auths(&[MockAuth {
        address: &intruder,
        invoke: &MockAuthInvoke {
            contract: &w.contract,
            fn_name: "create",
            args: args.into_val(&w.env),
            sub_invokes: &[],
        },
    }]);
    let res = w.client.try_create(
        &w.organizer,
        &w.token,
        &cents(1000),
        &(T0 + DAY),
        &committee,
        &1,
        &w.registry,
        &title,
    );
    assert!(matches!(res, Err(Err(_))), "expected auth failure, got {res:?}");
    assert_eq!(w.client.campaign_count(), 0);
}

// ================================================================== contribute

#[test]
fn contribute_records_entry_balance_and_moves_tokens() {
    let w = World::new(3);
    let id = w.create(cents(300_000), 2);
    let alice = w.funded_contributor(cents(10_000));
    let memo = h(&w.env, "for mama Njeri, from Houston");
    w.env.ledger().with_mut(|l| l.timestamp = T0 + 3600);

    w.client.contribute(&id, &alice, &cents(2537), &memo);
    assert_eq!(w.last_event_names(), std::vec!["contributed"]);

    assert_eq!(w.tok.balance(&alice), cents(10_000 - 2537));
    assert_eq!(w.tok.balance(&w.contract), cents(2537));
    let c = w.client.campaign(&id);
    assert_eq!(c.raised, cents(2537));
    assert_eq!(c.contribution_count, 1);
    assert_eq!(c.state, State::Open);
    assert_eq!(w.client.contribution(&id, &alice), cents(2537));
    assert_eq!(
        w.client.contribution_at(&id, &0),
        Contribution {
            from: alice.clone(),
            amount: cents(2537),
            memo_hash: memo,
            ledger_time: T0 + 3600,
        }
    );
    assert_eq!(w.client.available(&id), cents(2537));
}

#[test]
fn contribute_twice_from_same_address_appends_two_entries_and_sums_balance() {
    let w = World::new(3);
    let id = w.create(cents(300_000), 2);
    let wanjiru = w.funded_contributor(cents(10_000));
    w.client
        .contribute(&id, &wanjiru, &cents(5000), &h(&w.env, "first"));
    w.client
        .contribute(&id, &wanjiru, &cents(1250), &h(&w.env, "top up"));
    let c = w.client.campaign(&id);
    assert_eq!(c.contribution_count, 2);
    assert_eq!(c.raised, cents(6250));
    assert_eq!(w.client.contribution(&id, &wanjiru), cents(6250));
    let page = w.client.contributions(&id, &0, &10);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get_unchecked(1).amount, cents(1250));
}

#[test]
fn contribute_reaching_goal_moves_campaign_to_funded() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    w.give(id, cents(6000), "a");
    assert_eq!(w.client.campaign(&id).state, State::Open);
    w.give(id, cents(4000), "b");
    assert_eq!(
        w.last_event_names(),
        std::vec!["contributed", "goal_reached"]
    );
    assert_eq!(w.client.campaign(&id).state, State::Funded);
    // Over-funding is accepted while the deadline has not passed.
    w.give(id, cents(500), "late but welcome");
    let c = w.client.campaign(&id);
    assert_eq!(c.state, State::Funded);
    assert_eq!(c.raised, cents(10_500));
}

#[test]
fn contribute_rejects_non_positive_amounts() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    let a = w.funded_contributor(cents(100));
    let memo = BytesN::random(&w.env);
    assert_eq!(
        w.client.try_contribute(&id, &a, &0, &memo),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        w.client.try_contribute(&id, &a, &-5, &memo),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(w.client.campaign(&id).raised, 0);
}

#[test]
fn contribute_after_deadline_is_rejected() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    let a = w.funded_contributor(cents(100));
    w.env.ledger().with_mut(|l| l.timestamp = T0 + 30 * DAY);
    // On the deadline second itself: still accepted.
    w.client
        .contribute(&id, &a, &cents(10), &BytesN::random(&w.env));
    w.env.ledger().with_mut(|l| l.timestamp = T0 + 30 * DAY + 1);
    assert_eq!(
        w.client
            .try_contribute(&id, &a, &cents(10), &BytesN::random(&w.env)),
        Err(Ok(Error::DeadlinePassed))
    );
}

#[test]
fn contribute_in_refunding_or_closed_state_is_rejected() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    w.give(id, cents(100), "x");
    w.client.refund(&id); // organizer cancels before any payout
    let a = w.funded_contributor(cents(100));
    assert_eq!(
        w.client
            .try_contribute(&id, &a, &cents(10), &BytesN::random(&w.env)),
        Err(Ok(Error::NotAcceptingContributions))
    );
    let empty = w.create(cents(10_000), 2);
    w.client.close(&empty);
    assert_eq!(
        w.client
            .try_contribute(&empty, &a, &cents(10), &BytesN::random(&w.env)),
        Err(Ok(Error::NotAcceptingContributions))
    );
}

#[test]
fn contribute_to_unknown_campaign_is_rejected() {
    let w = World::new(3);
    let a = w.funded_contributor(cents(100));
    assert_eq!(
        w.client
            .try_contribute(&9, &a, &cents(10), &BytesN::random(&w.env)),
        Err(Ok(Error::CampaignNotFound))
    );
    assert_eq!(w.client.try_campaign(&9), Err(Ok(Error::CampaignNotFound)));
}

#[test]
fn contribute_requires_contributor_auth_including_token_transfer() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    let alice = w.funded_contributor(cents(100));
    let mallory = Address::generate(&w.env);
    let memo = BytesN::random(&w.env);

    // Mallory signs a contribution "from" Alice: rejected.
    w.env.mock_auths(&[MockAuth {
        address: &mallory,
        invoke: &MockAuthInvoke {
            contract: &w.contract,
            fn_name: "contribute",
            args: (id, alice.clone(), cents(50), memo.clone()).into_val(&w.env),
            sub_invokes: &[],
        },
    }]);
    let res = w.client.try_contribute(&id, &alice, &cents(50), &memo);
    assert!(matches!(res, Err(Err(_))), "expected auth failure, got {res:?}");
    assert_eq!(w.tok.balance(&alice), cents(100));

    // Alice signs, authorising both the call and the token transfer under it.
    w.env.mock_auths(&[MockAuth {
        address: &alice,
        invoke: &MockAuthInvoke {
            contract: &w.contract,
            fn_name: "contribute",
            args: (id, alice.clone(), cents(50), memo.clone()).into_val(&w.env),
            sub_invokes: &[MockAuthInvoke {
                contract: &w.token,
                fn_name: "transfer",
                args: (alice.clone(), w.contract.clone(), cents(50)).into_val(&w.env),
                sub_invokes: &[],
            }],
        },
    }]);
    w.client.contribute(&id, &alice, &cents(50), &memo);
    assert_eq!(w.tok.balance(&alice), cents(50));
    assert_eq!(w.client.contribution(&id, &alice), cents(50));
}

#[test]
fn contribute_without_token_balance_fails_and_records_nothing() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    let broke = Address::generate(&w.env);
    let res = w
        .client
        .try_contribute(&id, &broke, &cents(10), &BytesN::random(&w.env));
    // The Stellar Asset Contract's balance error propagates as a raw contract error
    // code; what matters here is that the call failed and rolled back.
    assert!(res.is_err(), "expected the token transfer to fail, got {res:?}");
    let c = w.client.campaign(&id);
    assert_eq!(c.raised, 0);
    assert_eq!(c.contribution_count, 0);
}

// ================================================================== propose_payout

fn funded_campaign(w: &World, goal_cents: i128, threshold: u32) -> u32 {
    let id = w.create(cents(goal_cents), threshold);
    w.give(id, cents(goal_cents), "whole goal");
    assert_eq!(w.client.campaign(&id).state, State::Funded);
    id
}

#[test]
fn propose_payout_records_proposal_with_proposer_as_first_approval() {
    let w = World::new(5);
    let id = funded_campaign(&w, 300_000, 3);
    let purpose = h(&w.env, "KNH admission deposit, invoice 2211");
    let pid = w
        .client
        .propose_payout(&id, &w.committee[0], &HOSPITAL, &cents(275_000), &purpose);
    assert_eq!(pid, 0);
    assert_eq!(
        w.last_event_names(),
        std::vec!["payout_proposed", "payout_approved"]
    );
    let p = w.client.payout(&id, &pid);
    assert_eq!(
        p,
        Payout {
            id: 0,
            biller_id: HOSPITAL,
            amount: cents(275_000),
            purpose_hash: purpose,
            approvals: vec![&w.env, w.committee[0].clone()],
            executed: false,
            proposer: w.committee[0].clone(),
            paid_to: None,
            executed_at: 0,
        }
    );
    assert_eq!(w.client.campaign(&id).payout_count, 1);
    assert_eq!(w.client.campaign(&id).paid_out, 0);
    assert_eq!(w.tok.balance(&w.contract), cents(300_000));
}

#[test]
fn propose_payout_rejected_while_goal_not_reached() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    w.give(id, cents(9_999), "almost");
    assert_eq!(
        w.client.try_propose_payout(
            &id,
            &w.committee[0],
            &HOSPITAL,
            &cents(100),
            &BytesN::random(&w.env)
        ),
        Err(Ok(Error::NotFunded))
    );
}

#[test]
fn propose_payout_rejects_non_member() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let outsider = Address::generate(&w.env);
    assert_eq!(
        w.client.try_propose_payout(
            &id,
            &outsider,
            &HOSPITAL,
            &cents(100),
            &BytesN::random(&w.env)
        ),
        Err(Ok(Error::NotCommitteeMember))
    );
    // The organizer is not automatically a committee member either.
    assert_eq!(
        w.client.try_propose_payout(
            &id,
            &w.organizer,
            &HOSPITAL,
            &cents(100),
            &BytesN::random(&w.env)
        ),
        Err(Ok(Error::NotCommitteeMember))
    );
}

#[test]
fn propose_payout_rejects_unpayable_billers() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let purpose = BytesN::random(&w.env);
    // Registered but never verified.
    assert_eq!(
        w.client
            .try_propose_payout(&id, &w.committee[0], &UNVERIFIED_CLINIC, &cents(100), &purpose),
        Err(Ok(Error::BillerNotPayable))
    );
    // Unknown id.
    assert_eq!(
        w.client
            .try_propose_payout(&id, &w.committee[0], &77, &cents(100), &purpose),
        Err(Ok(Error::BillerNotPayable))
    );
    // Verified but deactivated by the registry admin.
    w.reg.set_active(&SCHOOL, &false);
    assert_eq!(
        w.client
            .try_propose_payout(&id, &w.committee[0], &SCHOOL, &cents(100), &purpose),
        Err(Ok(Error::BillerNotPayable))
    );
    w.reg.set_active(&SCHOOL, &true);
    assert!(w
        .client
        .try_propose_payout(&id, &w.committee[0], &SCHOOL, &cents(100), &purpose)
        .is_ok());
}

#[test]
fn propose_payout_rejects_bad_amounts() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let purpose = BytesN::random(&w.env);
    for amount in [0i128, -1] {
        assert_eq!(
            w.client
                .try_propose_payout(&id, &w.committee[0], &HOSPITAL, &amount, &purpose),
            Err(Ok(Error::InvalidAmount))
        );
    }
    assert_eq!(
        w.client
            .try_propose_payout(&id, &w.committee[0], &HOSPITAL, &cents(10_001), &purpose),
        Err(Ok(Error::InsufficientFunds))
    );
    // Exactly the available amount is fine.
    assert!(w
        .client
        .try_propose_payout(&id, &w.committee[0], &HOSPITAL, &cents(10_000), &purpose)
        .is_ok());
}

#[test]
fn propose_payout_with_threshold_one_executes_immediately() {
    let w = World::new(1);
    let id = funded_campaign(&w, 10_000, 1);
    let hospital = w.biller_address(HOSPITAL);
    let pid = w.client.propose_payout(
        &id,
        &w.committee[0],
        &HOSPITAL,
        &cents(4_000),
        &BytesN::random(&w.env),
    );
    assert_eq!(
        w.last_event_names(),
        std::vec!["payout_proposed", "payout_approved", "receipt"]
    );
    let p = w.client.payout(&id, &pid);
    assert!(p.executed);
    assert_eq!(p.paid_to, Some(hospital.clone()));
    assert_eq!(p.executed_at, T0);
    assert_eq!(w.tok.balance(&hospital), cents(4_000));
    assert_eq!(w.client.available(&id), cents(6_000));
}

#[test]
fn propose_payout_requires_proposer_auth() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let purpose = BytesN::random(&w.env);
    let outsider = Address::generate(&w.env);
    w.env.mock_auths(&[MockAuth {
        address: &outsider,
        invoke: &MockAuthInvoke {
            contract: &w.contract,
            fn_name: "propose_payout",
            args: (id, w.committee[0].clone(), HOSPITAL, cents(100), purpose.clone())
                .into_val(&w.env),
            sub_invokes: &[],
        },
    }]);
    let res = w
        .client
        .try_propose_payout(&id, &w.committee[0], &HOSPITAL, &cents(100), &purpose);
    assert!(matches!(res, Err(Err(_))));
    assert_eq!(w.client.campaign(&id).payout_count, 0);
}

#[test]
fn propose_payout_is_capped_at_max_payouts() {
    let w = World::new(3);
    w.env.cost_estimate().budget().reset_unlimited();
    let id = funded_campaign(&w, 100_000, 3);
    for _ in 0..MAX_PAYOUTS {
        w.client.propose_payout(
            &id,
            &w.committee[0],
            &HOSPITAL,
            &cents(1),
            &BytesN::random(&w.env),
        );
    }
    assert_eq!(
        w.client.try_propose_payout(
            &id,
            &w.committee[0],
            &HOSPITAL,
            &cents(1),
            &BytesN::random(&w.env)
        ),
        Err(Ok(Error::TooManyPayouts))
    );
    assert_eq!(w.client.payouts(&id).len(), MAX_PAYOUTS);
}

// ================================================================== approve_payout

#[test]
fn approve_payout_executes_at_threshold_and_publishes_receipt() {
    let w = World::new(5);
    let id = funded_campaign(&w, 300_000, 3);
    let hospital = w.biller_address(HOSPITAL);
    let purpose = h(&w.env, "KNH admission deposit, invoice 2211");
    let pid = w
        .client
        .propose_payout(&id, &w.committee[0], &HOSPITAL, &cents(275_000), &purpose);

    w.env.ledger().with_mut(|l| l.timestamp = T0 + 2 * DAY);
    w.client.approve_payout(&id, &w.committee[1], &pid);
    assert_eq!(w.last_event_names(), std::vec!["payout_approved"]);
    let p = w.client.payout(&id, &pid);
    assert_eq!(p.approvals.len(), 2);
    assert!(!p.executed);
    assert_eq!(w.tok.balance(&hospital), 0);

    w.env.ledger().with_mut(|l| l.timestamp = T0 + 3 * DAY);
    w.client.approve_payout(&id, &w.committee[2], &pid);
    assert_eq!(
        w.last_event_names(),
        std::vec!["payout_approved", "receipt"]
    );
    // Only the approver authorised the call; the contract's own transfer needs no signature.
    let auths = w.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, w.committee[2]);

    let p = w.client.payout(&id, &pid);
    assert!(p.executed);
    assert_eq!(p.approvals.len(), 3);
    assert_eq!(p.paid_to, Some(hospital.clone()));
    assert_eq!(p.executed_at, T0 + 3 * DAY);
    assert_eq!(w.tok.balance(&hospital), cents(275_000));
    assert_eq!(w.tok.balance(&w.contract), cents(25_000));
    let c = w.client.campaign(&id);
    assert_eq!(c.paid_out, cents(275_000));
    assert_eq!(w.client.available(&id), cents(25_000));
    assert_eq!(c.state, State::Funded);
}

#[test]
fn approve_payout_rejects_duplicate_approvals() {
    let w = World::new(5);
    let id = funded_campaign(&w, 10_000, 3);
    let pid = w.client.propose_payout(
        &id,
        &w.committee[0],
        &HOSPITAL,
        &cents(100),
        &BytesN::random(&w.env),
    );
    // The proposer already counts as an approval.
    assert_eq!(
        w.client.try_approve_payout(&id, &w.committee[0], &pid),
        Err(Ok(Error::AlreadyApproved))
    );
    w.client.approve_payout(&id, &w.committee[1], &pid);
    assert_eq!(
        w.client.try_approve_payout(&id, &w.committee[1], &pid),
        Err(Ok(Error::AlreadyApproved))
    );
    assert_eq!(w.client.payout(&id, &pid).approvals.len(), 2);
}

#[test]
fn approve_payout_rejects_non_member_and_unknown_payout() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let pid = w.client.propose_payout(
        &id,
        &w.committee[0],
        &HOSPITAL,
        &cents(100),
        &BytesN::random(&w.env),
    );
    let outsider = Address::generate(&w.env);
    assert_eq!(
        w.client.try_approve_payout(&id, &outsider, &pid),
        Err(Ok(Error::NotCommitteeMember))
    );
    assert_eq!(
        w.client.try_approve_payout(&id, &w.committee[1], &42),
        Err(Ok(Error::PayoutNotFound))
    );
    assert_eq!(
        w.client.try_approve_payout(&7, &w.committee[1], &pid),
        Err(Ok(Error::CampaignNotFound))
    );
}

#[test]
fn approve_payout_rejects_already_executed() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let pid = w.client.propose_payout(
        &id,
        &w.committee[0],
        &HOSPITAL,
        &cents(100),
        &BytesN::random(&w.env),
    );
    w.client.approve_payout(&id, &w.committee[1], &pid);
    assert!(w.client.payout(&id, &pid).executed);
    assert_eq!(
        w.client.try_approve_payout(&id, &w.committee[2], &pid),
        Err(Ok(Error::AlreadyExecuted))
    );
}

#[test]
fn approve_payout_rechecks_biller_at_execution_time() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let pid = w.client.propose_payout(
        &id,
        &w.committee[0],
        &SCHOOL,
        &cents(100),
        &BytesN::random(&w.env),
    );
    // Registry admin deactivates the school between proposal and final approval.
    w.reg.set_active(&SCHOOL, &false);
    assert_eq!(
        w.client.try_approve_payout(&id, &w.committee[1], &pid),
        Err(Ok(Error::BillerNotPayable))
    );
    let p = w.client.payout(&id, &pid);
    assert!(!p.executed);
    assert_eq!(p.approvals.len(), 1, "a failed execution records nothing");
    assert_eq!(w.tok.balance(&w.biller_address(SCHOOL)), 0);
    // Reactivated: the approval goes through.
    w.reg.set_active(&SCHOOL, &true);
    w.client.approve_payout(&id, &w.committee[1], &pid);
    assert!(w.client.payout(&id, &pid).executed);
}

#[test]
fn approve_payout_pays_the_registry_address_current_at_execution() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let old_addr = w.biller_address(HOSPITAL);
    let pid = w.client.propose_payout(
        &id,
        &w.committee[0],
        &HOSPITAL,
        &cents(100),
        &BytesN::random(&w.env),
    );
    let new_addr = Address::generate(&w.env);
    w.reg.propose_address(&HOSPITAL, &new_addr);
    w.env.ledger().with_mut(|l| l.timestamp += DAY);
    w.reg.apply_address(&HOSPITAL);
    w.client.approve_payout(&id, &w.committee[1], &pid);
    assert_eq!(w.client.payout(&id, &pid).paid_to, Some(new_addr.clone()));
    assert_eq!(w.tok.balance(&new_addr), cents(100));
    assert_eq!(w.tok.balance(&old_addr), 0);
}

#[test]
fn approve_payout_fails_when_funds_were_spent_by_an_earlier_payout() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let purpose = BytesN::random(&w.env);
    let p0 = w
        .client
        .propose_payout(&id, &w.committee[0], &HOSPITAL, &cents(8_000), &purpose);
    let p1 = w
        .client
        .propose_payout(&id, &w.committee[1], &FUNERAL, &cents(8_000), &purpose);
    w.client.approve_payout(&id, &w.committee[2], &p0);
    assert_eq!(w.client.available(&id), cents(2_000));
    assert_eq!(
        w.client.try_approve_payout(&id, &w.committee[2], &p1),
        Err(Ok(Error::InsufficientFunds))
    );
    assert!(!w.client.payout(&id, &p1).executed);
}

#[test]
fn approve_payout_requires_approver_auth() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let pid = w.client.propose_payout(
        &id,
        &w.committee[0],
        &HOSPITAL,
        &cents(100),
        &BytesN::random(&w.env),
    );
    let outsider = Address::generate(&w.env);
    w.env.mock_auths(&[MockAuth {
        address: &outsider,
        invoke: &MockAuthInvoke {
            contract: &w.contract,
            fn_name: "approve_payout",
            args: (id, w.committee[1].clone(), pid).into_val(&w.env),
            sub_invokes: &[],
        },
    }]);
    assert!(matches!(
        w.client.try_approve_payout(&id, &w.committee[1], &pid),
        Err(Err(_))
    ));
    assert!(!w.client.payout(&id, &pid).executed);

    w.env.mock_auths(&[MockAuth {
        address: &w.committee[1],
        invoke: &MockAuthInvoke {
            contract: &w.contract,
            fn_name: "approve_payout",
            args: (id, w.committee[1].clone(), pid).into_val(&w.env),
            sub_invokes: &[],
        },
    }]);
    w.client.approve_payout(&id, &w.committee[1], &pid);
    assert!(w.client.payout(&id, &pid).executed);
}

#[test]
fn approve_payout_rejected_once_campaign_is_refunding() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let pid = w.client.propose_payout(
        &id,
        &w.committee[0],
        &HOSPITAL,
        &cents(100),
        &BytesN::random(&w.env),
    );
    w.client.refund(&id); // organizer cancels; no payout executed yet
    assert_eq!(
        w.client.try_approve_payout(&id, &w.committee[1], &pid),
        Err(Ok(Error::NotFunded))
    );
}

// ================================================================== refund / claim_refund

#[test]
fn refund_after_deadline_with_goal_missed_needs_no_signature() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    let a = w.give(id, cents(2_500), "x");
    w.env.ledger().with_mut(|l| l.timestamp = T0 + 30 * DAY + 1);
    // Enforcing mode with no authorization entries at all.
    w.env.mock_auths(&[]);
    w.client.refund(&id);
    assert_eq!(w.last_event_names(), std::vec!["refunding_started"]);
    assert_eq!(w.client.campaign(&id).state, State::Refunding);
    // Claiming is permissionless too: the funds can only go to the contributor.
    w.client.claim_refund(&id, &a);
    assert_eq!(w.tok.balance(&a), cents(2_500));
}

#[test]
fn refund_before_deadline_requires_organizer() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    w.give(id, cents(2_500), "x");
    // Deadline not passed: anyone else is refused.
    w.env.mock_auths(&[]);
    assert!(matches!(w.client.try_refund(&id), Err(Err(_))));
    assert_eq!(w.client.campaign(&id).state, State::Open);
    // The organizer may cancel.
    w.env.mock_auths(&[MockAuth {
        address: &w.organizer,
        invoke: &MockAuthInvoke {
            contract: &w.contract,
            fn_name: "refund",
            args: (id,).into_val(&w.env),
            sub_invokes: &[],
        },
    }]);
    w.client.refund(&id);
    assert_eq!(w.client.campaign(&id).state, State::Refunding);
}

#[test]
fn refund_after_deadline_with_goal_met_still_requires_organizer() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    w.env.ledger().with_mut(|l| l.timestamp = T0 + 31 * DAY);
    w.env.mock_auths(&[]);
    assert!(matches!(w.client.try_refund(&id), Err(Err(_))));
    w.env.mock_all_auths();
    w.client.refund(&id);
    assert_eq!(w.client.campaign(&id).state, State::Refunding);
}

#[test]
fn refund_rejected_after_a_payout_executed() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    let pid = w.client.propose_payout(
        &id,
        &w.committee[0],
        &HOSPITAL,
        &cents(100),
        &BytesN::random(&w.env),
    );
    w.client.approve_payout(&id, &w.committee[1], &pid);
    assert_eq!(w.client.try_refund(&id), Err(Ok(Error::RefundNotAllowed)));
    assert_eq!(w.client.campaign(&id).state, State::Funded);
}

#[test]
fn refund_rejected_when_already_refunding_or_closed() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    w.client.refund(&id);
    assert_eq!(w.client.try_refund(&id), Err(Ok(Error::RefundNotAllowed)));
    let id2 = w.create(cents(10_000), 2);
    w.client.close(&id2);
    assert_eq!(w.client.try_refund(&id2), Err(Ok(Error::RefundNotAllowed)));
}

#[test]
fn claim_refund_returns_exact_amount_once() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    let a = w.funded_contributor(cents(1_000));
    w.client
        .contribute(&id, &a, &cents(333), &BytesN::random(&w.env));
    w.client
        .contribute(&id, &a, &cents(667), &BytesN::random(&w.env));
    let b = w.give(id, cents(500), "y");
    w.client.refund(&id);

    w.client.claim_refund(&id, &a);
    assert_eq!(w.last_event_names(), std::vec!["refunded"]);
    assert_eq!(w.tok.balance(&a), cents(1_000));
    assert_eq!(w.client.contribution(&id, &a), 0);
    let c = w.client.campaign(&id);
    assert_eq!(c.refunded, cents(1_000));
    assert_eq!(c.raised, cents(1_500), "the ledger of what was given is never rewritten");
    assert_eq!(w.client.available(&id), cents(500));
    assert_eq!(
        w.client.try_claim_refund(&id, &a),
        Err(Ok(Error::NothingToRefund))
    );
    let stranger = Address::generate(&w.env);
    assert_eq!(
        w.client.try_claim_refund(&id, &stranger),
        Err(Ok(Error::NothingToRefund))
    );
    w.client.claim_refund(&id, &b);
    assert_eq!(w.tok.balance(&w.contract), 0);
}

#[test]
fn claim_refund_rejected_when_not_refunding() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    let a = w.give(id, cents(500), "y");
    assert_eq!(
        w.client.try_claim_refund(&id, &a),
        Err(Ok(Error::NotRefunding))
    );
    assert_eq!(w.tok.balance(&a), 0);
}

// ================================================================== close

#[test]
fn close_requires_zero_balance_and_organizer() {
    let w = World::new(3);
    let id = w.create(cents(10_000), 2);
    w.give(id, cents(500), "y");
    assert_eq!(w.client.try_close(&id), Err(Ok(Error::BalanceNotZero)));

    w.client.refund(&id);
    assert_eq!(w.client.try_close(&id), Err(Ok(Error::BalanceNotZero)));
    let a = w.client.contribution_at(&id, &0).from;
    w.client.claim_refund(&id, &a);

    let outsider = Address::generate(&w.env);
    w.env.mock_auths(&[MockAuth {
        address: &outsider,
        invoke: &MockAuthInvoke {
            contract: &w.contract,
            fn_name: "close",
            args: (id,).into_val(&w.env),
            sub_invokes: &[],
        },
    }]);
    assert!(matches!(w.client.try_close(&id), Err(Err(_))));

    w.env.mock_all_auths();
    w.client.close(&id);
    assert_eq!(w.last_event_names(), std::vec!["campaign_closed"]);
    assert_eq!(w.client.campaign(&id).state, State::Closed);
    assert_eq!(w.client.try_close(&id), Err(Ok(Error::AlreadyClosed)));
}

#[test]
fn close_empty_open_campaign() {
    let w = World::new(1);
    let id = w.create(cents(10_000), 1);
    w.client.close(&id);
    assert_eq!(w.client.campaign(&id).state, State::Closed);
}

// ================================================================== views

#[test]
fn contributions_view_pages_through_the_ledger() {
    let w = World::new(1);
    let id = w.create(cents(1_000_000), 1);
    for i in 0..7 {
        w.give(id, cents(100 + i), "p");
    }
    let first = w.client.contributions(&id, &0, &3);
    assert_eq!(first.len(), 3);
    assert_eq!(first.get_unchecked(2).amount, cents(102));
    let second = w.client.contributions(&id, &3, &3);
    assert_eq!(second.get_unchecked(0).amount, cents(103));
    let tail = w.client.contributions(&id, &6, &3);
    assert_eq!(tail.len(), 1);
    assert_eq!(tail.get_unchecked(0).amount, cents(106));
    assert_eq!(w.client.contributions(&id, &7, &3).len(), 0);
    assert_eq!(w.client.contributions(&id, &u32::MAX, &MAX_PAGE).len(), 0);
    assert_eq!(
        w.client.try_contributions(&id, &0, &(MAX_PAGE + 1)),
        Err(Ok(Error::PageTooLarge))
    );
    assert_eq!(
        w.client.try_contribution_at(&id, &7),
        Err(Ok(Error::IndexOutOfRange))
    );
    assert_eq!(w.client.contribution(&id, &Address::generate(&w.env)), 0);
}

#[test]
fn payouts_and_membership_views() {
    let w = World::new(3);
    let id = funded_campaign(&w, 10_000, 2);
    assert_eq!(w.client.payouts(&id).len(), 0);
    let purpose = BytesN::random(&w.env);
    w.client
        .propose_payout(&id, &w.committee[0], &HOSPITAL, &cents(10), &purpose);
    w.client
        .propose_payout(&id, &w.committee[1], &SCHOOL, &cents(20), &purpose);
    let ps = w.client.payouts(&id);
    assert_eq!(ps.len(), 2);
    assert_eq!(ps.get_unchecked(0).biller_id, HOSPITAL);
    assert_eq!(ps.get_unchecked(1).biller_id, SCHOOL);
    assert_eq!(ps.get_unchecked(1).id, 1);
    assert_eq!(w.client.try_payout(&id, &2), Err(Ok(Error::PayoutNotFound)));
    assert!(w.client.is_member(&id, &w.committee[2]));
    assert!(!w.client.is_member(&id, &w.organizer));
    assert_eq!(w.client.try_payouts(&5), Err(Ok(Error::CampaignNotFound)));
}

// ================================================================== scenarios

/// Mama Njeri's surgery: 40 diaspora contributors, a 3-of-5 committee, the hospital
/// deposit and then the pharmacy bill paid straight to the billers, and the campaign
/// closed with a zero balance and a public receipt for every shilling.
#[test]
fn scenario_medical_campaign() {
    let w = World::new(5);
    w.env.cost_estimate().budget().reset_unlimited();
    let goal = cents(300_000); // $3,000.00
    let id = w.create(goal, 3);

    // ---- 40 contributions over ten days, $5.00 to $500.00 with odd cents.
    let mut contributors = std::vec::Vec::new();
    for (i, c) in SEED_CENTS.iter().enumerate() {
        w.env
            .ledger()
            .with_mut(|l| l.timestamp = T0 + (i as u64) * 6 * 3600);
        let memo = std::format!("seed contributor {i}");
        let who = w.give(id, cents(*c), &memo);
        contributors.push((who, cents(*c)));
        let cur = w.client.campaign(&id);
        let running: i128 = SEED_CENTS[..=i].iter().map(|c| cents(*c)).sum();
        assert_eq!(cur.raised, running);
        let expected_state = if running >= goal { State::Funded } else { State::Open };
        assert_eq!(cur.state, expected_state, "after contribution {i}");
    }
    let c = w.client.campaign(&id);
    assert_eq!(c.raised, SEED_TOTAL);
    assert_eq!(c.raised, 33_118_200_000);
    assert_eq!(c.contribution_count, 40);
    assert_eq!(c.state, State::Funded);
    assert_eq!(w.tok.balance(&w.contract), SEED_TOTAL);

    // The public ledger pages through all 40 entries and reconciles to `raised`.
    let page1 = w.client.contributions(&id, &0, &25);
    let page2 = w.client.contributions(&id, &25, &25);
    assert_eq!(page1.len(), 25);
    assert_eq!(page2.len(), 15);
    let mut sum = 0i128;
    for e in page1.iter().chain(page2.iter()) {
        sum += e.amount;
    }
    assert_eq!(sum, c.raised);
    assert_eq!(page1.get_unchecked(3).amount, cents(50_000));
    assert_eq!(page1.get_unchecked(3).from, contributors[3].0);
    for (who, amt) in &contributors {
        assert_eq!(w.client.contribution(&id, who), *amt);
    }

    // ---- A member tries to pay the unverified clinic: refused.
    let purpose_clinic = h(&w.env, "Mwangi Clinic consultation");
    assert_eq!(
        w.client.try_propose_payout(
            &id,
            &w.committee[0],
            &UNVERIFIED_CLINIC,
            &cents(15_000),
            &purpose_clinic
        ),
        Err(Ok(Error::BillerNotPayable))
    );
    assert_eq!(w.client.campaign(&id).payout_count, 0);

    // ---- Payout 0: hospital admission deposit $2,750.00, 3-of-5.
    w.env.ledger().with_mut(|l| l.timestamp = T0 + 11 * DAY);
    let hospital = w.biller_address(HOSPITAL);
    let purpose_hospital = h(&w.env, "KNH admission deposit, invoice KNH-2211");
    let p0 = w.client.propose_payout(
        &id,
        &w.committee[0],
        &HOSPITAL,
        &cents(275_000),
        &purpose_hospital,
    );
    assert_eq!(p0, 0);

    // A non-member (the organizer's cousin) tries to approve: refused.
    let cousin = Address::generate(&w.env);
    assert_eq!(
        w.client.try_approve_payout(&id, &cousin, &p0),
        Err(Ok(Error::NotCommitteeMember))
    );
    // The proposer tries to approve a second time: refused.
    assert_eq!(
        w.client.try_approve_payout(&id, &w.committee[0], &p0),
        Err(Ok(Error::AlreadyApproved))
    );
    w.client.approve_payout(&id, &w.committee[1], &p0);
    assert!(!w.client.payout(&id, &p0).executed);
    assert_eq!(w.tok.balance(&hospital), 0);
    // Member 1 tries again: refused.
    assert_eq!(
        w.client.try_approve_payout(&id, &w.committee[1], &p0),
        Err(Ok(Error::AlreadyApproved))
    );

    w.env.ledger().with_mut(|l| l.timestamp = T0 + 12 * DAY);
    w.client.approve_payout(&id, &w.committee[3], &p0);
    assert_eq!(
        w.last_event_names(),
        std::vec!["payout_approved", "receipt"]
    );
    let p = w.client.payout(&id, &p0);
    assert!(p.executed);
    assert_eq!(
        p.approvals,
        vec![
            &w.env,
            w.committee[0].clone(),
            w.committee[1].clone(),
            w.committee[3].clone()
        ]
    );
    assert_eq!(p.paid_to, Some(hospital.clone()));
    assert_eq!(p.executed_at, T0 + 12 * DAY);
    assert_eq!(w.tok.balance(&hospital), cents(275_000));
    assert_eq!(w.client.available(&id), cents(56_182));

    // A late approval on the executed payout is refused.
    assert_eq!(
        w.client.try_approve_payout(&id, &w.committee[4], &p0),
        Err(Ok(Error::AlreadyExecuted))
    );

    // ---- Payout 1: pharmacy $561.82 (everything that is left).
    w.env.ledger().with_mut(|l| l.timestamp = T0 + 20 * DAY);
    let pharmacy = w.biller_address(PHARMACY);
    let purpose_pharmacy = h(&w.env, "Goodlife Pharmacy post-op medication, receipt GLP-14-0093");
    // Asking for more than what is left is refused.
    assert_eq!(
        w.client.try_propose_payout(
            &id,
            &w.committee[2],
            &PHARMACY,
            &cents(56_183),
            &purpose_pharmacy
        ),
        Err(Ok(Error::InsufficientFunds))
    );
    let p1 = w.client.propose_payout(
        &id,
        &w.committee[2],
        &PHARMACY,
        &cents(56_182),
        &purpose_pharmacy,
    );
    assert_eq!(p1, 1);
    w.client.approve_payout(&id, &w.committee[4], &p1);
    w.client.approve_payout(&id, &w.committee[1], &p1);
    assert_eq!(
        w.last_event_names(),
        std::vec!["payout_approved", "receipt"]
    );
    assert_eq!(w.tok.balance(&pharmacy), cents(56_182));
    assert_eq!(w.tok.balance(&w.contract), 0);
    assert_eq!(w.client.available(&id), 0);

    let c = w.client.campaign(&id);
    assert_eq!(c.paid_out, SEED_TOTAL);
    assert_eq!(c.raised, SEED_TOTAL);
    assert_eq!(c.refunded, 0);
    assert_eq!(c.payout_count, 2);

    let payouts = w.client.payouts(&id);
    assert_eq!(payouts.len(), 2);
    assert!(payouts.iter().all(|p| p.executed));
    let receipted: i128 = payouts.iter().map(|p| p.amount).sum();
    assert_eq!(receipted, c.paid_out);

    // Once the balance is zero nobody can refund and the organizer can close.
    assert_eq!(w.client.try_refund(&id), Err(Ok(Error::RefundNotAllowed)));
    w.client.close(&id);
    assert_eq!(w.last_event_names(), std::vec!["campaign_closed"]);
    assert_eq!(w.client.campaign(&id).state, State::Closed);

    // Closed campaigns take nothing more.
    let late = w.funded_contributor(cents(1_000));
    assert_eq!(
        w.client
            .try_contribute(&id, &late, &cents(1_000), &h(&w.env, "too late")),
        Err(Ok(Error::NotAcceptingContributions))
    );
}

/// The school-fees drive misses its goal: after the deadline anyone can flip the campaign
/// to refunding and all 40 contributors get back exactly what they gave.
#[test]
fn scenario_goal_missed_every_contributor_refunded_exactly() {
    let w = World::new(3);
    w.env.cost_estimate().budget().reset_unlimited();
    let goal = cents(1_000_000); // $10,000.00
    let id = w.create(goal, 2);

    let mut contributors = std::vec::Vec::new();
    for (i, c) in SEED_CENTS.iter().enumerate() {
        let who = w.funded_contributor(cents(*c) + cents(1)); // keep a cent to prove exactness
        w.client
            .contribute(&id, &who, &cents(*c), &h(&w.env, &std::format!("fees {i}")));
        contributors.push((who, cents(*c)));
    }
    let c = w.client.campaign(&id);
    assert_eq!(c.raised, SEED_TOTAL);
    assert_eq!(c.state, State::Open);
    assert!(c.raised < c.goal);

    // Nobody may propose payouts on an unfunded campaign.
    assert_eq!(
        w.client.try_propose_payout(
            &id,
            &w.committee[0],
            &SCHOOL,
            &cents(100),
            &BytesN::random(&w.env)
        ),
        Err(Ok(Error::NotFunded))
    );
    // Before the deadline, only the organizer could cancel; a stranger cannot.
    w.env.mock_auths(&[]);
    assert!(matches!(w.client.try_refund(&id), Err(Err(_))));
    // Contributors cannot claim before the switch.
    assert_eq!(
        w.client.try_claim_refund(&id, &contributors[0].0),
        Err(Ok(Error::NotRefunding))
    );

    // Deadline passes with the goal missed: permissionless switch to refunding.
    w.env.ledger().with_mut(|l| l.timestamp = T0 + 30 * DAY + 1);
    w.client.refund(&id);
    assert_eq!(w.last_event_names(), std::vec!["refunding_started"]);
    assert_eq!(w.client.campaign(&id).state, State::Refunding);

    // Every contributor claims (still with no authorization entries) and gets the exact amount.
    for (who, amt) in &contributors {
        w.client.claim_refund(&id, who);
        assert_eq!(w.tok.balance(who), *amt + cents(1));
        assert_eq!(w.client.contribution(&id, who), 0);
    }
    assert_eq!(w.tok.balance(&w.contract), 0);
    let c = w.client.campaign(&id);
    assert_eq!(c.refunded, SEED_TOTAL);
    assert_eq!(c.raised, SEED_TOTAL);
    assert_eq!(c.paid_out, 0);
    assert_eq!(c.contribution_count, 40, "the ledger keeps the history");
    assert_eq!(w.client.available(&id), 0);

    // A second claim is refused.
    assert_eq!(
        w.client.try_claim_refund(&id, &contributors[7].0),
        Err(Ok(Error::NothingToRefund))
    );

    // Organizer closes.
    w.env.mock_all_auths();
    w.client.close(&id);
    assert_eq!(w.client.campaign(&id).state, State::Closed);
}
