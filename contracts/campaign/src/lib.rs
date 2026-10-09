//! Campaign contract: a public contribution ledger for community and diaspora
//! fundraising with M-of-N committee approval of payouts that go straight to
//! billers registered in the `biller_registry` contract.
//!
//! Storage layout (see ARCHITECTURE.md for the reasoning):
//!
//! * `Campaign(id)`            one persistent entry per campaign (small, fixed size)
//! * `Entry(id, index)`        one persistent entry per contribution, append-only
//! * `Balance(id, address)`    one persistent entry per contributor (sum, used for refunds)
//! * `Payout(id, payout_id)`   one persistent entry per payout proposal
//!
//! Contributions are never stored in a single growing `Vec`: every contribution is
//! its own ledger entry addressed by an index counter, so `contribute` costs the same
//! for the 1st and the 10,000th gift and no entry ever approaches the ledger entry
//! size limit. Views page through the index.
#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype,
    symbol_short, token, vec, Address, BytesN, Env, Symbol, Vec,
};

/// Largest committee a campaign may have.
pub const MAX_COMMITTEE: u32 = 7;
/// Largest number of payout proposals per campaign (keeps the `payouts` view bounded).
pub const MAX_PAYOUTS: u32 = 64;
/// Largest page the `contributions` view returns.
pub const MAX_PAGE: u32 = 100;

const DAY_LEDGERS: u32 = 17_280;
const TTL_THRESHOLD: u32 = 30 * DAY_LEDGERS;
const TTL_EXTEND_TO: u32 = 120 * DAY_LEDGERS;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    CampaignNotFound = 1,
    InvalidCommittee = 2,
    InvalidThreshold = 3,
    InvalidGoal = 4,
    InvalidDeadline = 5,
    InvalidAmount = 6,
    NotAcceptingContributions = 7,
    DeadlinePassed = 8,
    NotFunded = 9,
    NotCommitteeMember = 10,
    BillerNotPayable = 11,
    InsufficientFunds = 12,
    PayoutNotFound = 13,
    AlreadyApproved = 14,
    AlreadyExecuted = 15,
    RefundNotAllowed = 16,
    NotRefunding = 17,
    NothingToRefund = 18,
    BalanceNotZero = 19,
    AlreadyClosed = 20,
    TooManyPayouts = 21,
    PageTooLarge = 22,
    IndexOutOfRange = 23,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    /// Accepting contributions, goal not yet reached.
    Open,
    /// Goal reached (equal to or greater than the target); committee may propose and approve payouts. Contributions are still
    /// accepted until the deadline.
    Funded,
    /// Contributors may claim their exact contribution back.
    Refunding,
    /// Balance is zero and the organizer closed the campaign. Terminal.
    Closed,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Campaign {
    pub organizer: Address,
    pub token: Address,
    pub goal: i128,
    pub raised: i128,
    pub deadline: u64,
    pub committee: Vec<Address>,
    pub threshold: u32,
    pub registry: Address,
    pub state: State,
    /// SHA-256 hash of the campaign description kept off-chain (WhatsApp post, PDF).
    pub title_hash: BytesN<32>,
    /// Sum of executed payouts.
    pub paid_out: i128,
    /// Sum of refunds already claimed.
    pub refunded: i128,
    /// Number of contribution entries (next index).
    pub contribution_count: u32,
    /// Number of payout proposals (next payout id).
    pub payout_count: u32,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contribution {
    pub from: Address,
    pub amount: i128,
    pub memo_hash: BytesN<32>,
    pub ledger_time: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payout {
    pub id: u32,
    pub biller_id: u32,
    pub amount: i128,
    pub purpose_hash: BytesN<32>,
    pub approvals: Vec<Address>,
    pub executed: bool,
    pub proposer: Address,
    /// Address the funds were sent to (snapshot of the registry at execution time).
    pub paid_to: Option<Address>,
    /// Ledger timestamp of execution, 0 while pending.
    pub executed_at: u64,
}

/// Mirror of `biller_registry::Biller` so this crate can decode the registry's answer
/// without linking the registry contract into this wasm.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Biller {
    pub id: u32,
    pub name_hash: BytesN<32>,
    pub category: Symbol,
    pub address: Address,
    pub verified: bool,
    pub active: bool,
}

/// The subset of the registry interface the campaign relies on.
#[contractclient(name = "RegistryClient")]
pub trait RegistryInterface {
    fn is_payable(env: Env, id: u32) -> bool;
    fn biller(env: Env, id: u32) -> Biller;
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Count,
    Campaign(u32),
    Entry(u32, u32),
    Balance(u32, Address),
    Payout(u32, u32),
}

// ---------------------------------------------------------------- events

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignCreated {
    #[topic]
    pub campaign_id: u32,
    pub organizer: Address,
    pub token: Address,
    pub goal: i128,
    pub deadline: u64,
    pub threshold: u32,
    pub committee_size: u32,
    pub registry: Address,
    pub title_hash: BytesN<32>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contributed {
    #[topic]
    pub campaign_id: u32,
    #[topic]
    pub index: u32,
    pub from: Address,
    pub amount: i128,
    pub memo_hash: BytesN<32>,
    pub raised: i128,
    pub ledger_time: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoalReached {
    #[topic]
    pub campaign_id: u32,
    pub raised: i128,
    pub goal: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayoutProposed {
    #[topic]
    pub campaign_id: u32,
    #[topic]
    pub payout_id: u32,
    pub proposer: Address,
    pub biller_id: u32,
    pub amount: i128,
    pub purpose_hash: BytesN<32>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayoutApproved {
    #[topic]
    pub campaign_id: u32,
    #[topic]
    pub payout_id: u32,
    pub approver: Address,
    pub approvals: u32,
    pub threshold: u32,
}

/// Emitted once per executed payout: the public receipt contributors can cite.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Receipt {
    #[topic]
    pub campaign_id: u32,
    #[topic]
    pub payout_id: u32,
    pub biller_id: u32,
    pub to: Address,
    pub amount: i128,
    pub purpose_hash: BytesN<32>,
    pub approvals: u32,
    pub ledger_time: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundingStarted {
    #[topic]
    pub campaign_id: u32,
    /// `deadline` (goal missed) or `cancelled` (organizer stopped it before any payout).
    pub reason: Symbol,
    pub raised: i128,
    pub goal: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Refunded {
    #[topic]
    pub campaign_id: u32,
    pub contributor: Address,
    pub amount: i128,
    pub ledger_time: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignClosed {
    #[topic]
    pub campaign_id: u32,
    pub raised: i128,
    pub paid_out: i128,
    pub refunded: i128,
}

// ---------------------------------------------------------------- helpers

fn extend_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Read a campaign without touching TTLs (for views).
fn read_campaign(env: &Env, id: u32) -> Result<Campaign, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Campaign(id))
        .ok_or(Error::CampaignNotFound)
}

/// Read a campaign and bump TTLs (for state transitions).
fn load_campaign(env: &Env, id: u32) -> Result<Campaign, Error> {
    let c = read_campaign(env, id)?;
    extend_persistent(env, &DataKey::Campaign(id));
    extend_instance(env);
    Ok(c)
}

fn save_campaign(env: &Env, id: u32, c: &Campaign) {
    let key = DataKey::Campaign(id);
    env.storage().persistent().set(&key, c);
    extend_persistent(env, &key);
}

fn read_payout(env: &Env, id: u32, payout_id: u32) -> Result<Payout, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Payout(id, payout_id))
        .ok_or(Error::PayoutNotFound)
}

fn save_payout(env: &Env, id: u32, p: &Payout) {
    let key = DataKey::Payout(id, p.id);
    env.storage().persistent().set(&key, p);
    extend_persistent(env, &key);
}

fn is_member(c: &Campaign, who: &Address) -> bool {
    c.committee.contains(who)
}

/// Helper to calculate available funds: raised - paid_out - refunded
fn available(c: &Campaign) -> i128 {
    c.raised - c.paid_out - c.refunded
}

/// Helper to construct a token client from the campaign's token address.
fn token_client(env: &Env, c: &Campaign) -> token::TokenClient<'static> {
    token::TokenClient::new(env, &c.token)
}

/// Transfer to the biller, mark the payout executed and publish the receipt.
/// The registry is consulted again at execution time: a biller that was deactivated
/// or un-verified after the proposal cannot be paid.
fn execute_payout(env: &Env, id: u32, c: &mut Campaign, p: &mut Payout) -> Result<(), Error> {
    let registry = RegistryClient::new(env, &c.registry);
    if !registry.is_payable(&p.biller_id) {
        return Err(Error::BillerNotPayable);
    }
    let biller = registry.biller(&p.biller_id);
    if p.amount > available(c) {
        return Err(Error::InsufficientFunds);
    }
    token_client(env, c).transfer(&env.current_contract_address(), &biller.address, &p.amount);
    let now = env.ledger().timestamp();
    c.paid_out += p.amount;
    p.executed = true;
    p.paid_to = Some(biller.address.clone());
    p.executed_at = now;
    Receipt {
        campaign_id: id,
        payout_id: p.id,
        biller_id: p.biller_id,
        to: biller.address,
        amount: p.amount,
        purpose_hash: p.purpose_hash.clone(),
        approvals: p.approvals.len(),
        ledger_time: now,
    }
    .publish(env);
    Ok(())
}

// ---------------------------------------------------------------- contract

#[contract]
pub struct CampaignContract;

#[contractimpl]
impl CampaignContract {
    /// Create a campaign. The organizer signs; the committee is 1..=7 distinct
    /// addresses and `threshold` approvals (1..=committee.len()) execute a payout.
    /// Returns the campaign id.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        env: Env,
        organizer: Address,
        token: Address,
        goal: i128,
        deadline: u64,
        committee: Vec<Address>,
        threshold: u32,
        registry: Address,
        title_hash: BytesN<32>,
    ) -> Result<u32, Error> {
        organizer.require_auth();
        if goal <= 0 {
            return Err(Error::InvalidGoal);
        }
        let now = env.ledger().timestamp();
        if deadline <= now {
            return Err(Error::InvalidDeadline);
        }
        let size = committee.len();
        if size == 0 || size > MAX_COMMITTEE {
            return Err(Error::InvalidCommittee);
        }
        for i in 0..size {
            let a = committee.get_unchecked(i);
            for j in (i + 1)..size {
                if committee.get_unchecked(j) == a {
                    return Err(Error::InvalidCommittee);
                }
            }
        }
        if threshold == 0 || threshold > size {
            return Err(Error::InvalidThreshold);
        }

        let id: u32 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);
        let c = Campaign {
            organizer: organizer.clone(),
            token: token.clone(),
            goal,
            raised: 0,
            deadline,
            committee,
            threshold,
            registry: registry.clone(),
            state: State::Open,
            title_hash: title_hash.clone(),
            paid_out: 0,
            refunded: 0,
            contribution_count: 0,
            payout_count: 0,
            created_at: now,
        };
        save_campaign(&env, id, &c);
        env.storage().instance().set(&DataKey::Count, &(id + 1));
        extend_instance(&env);
        CampaignCreated {
            campaign_id: id,
            organizer,
            token,
            goal,
            deadline,
            threshold,
            committee_size: size,
            registry,
            title_hash,
        }
        .publish(&env);
        Ok(id)
    }

    /// Contribute `amount` of the campaign token. `memo_hash` is the sha256 of the
    /// contributor's note ("for mama Njeri", "Houston chapter"). The contribution is
    /// appended to the public ledger and added to the contributor's refundable balance.
    pub fn contribute(
        env: Env,
        id: u32,
        from: Address,
        amount: i128,
        memo_hash: BytesN<32>,
    ) -> Result<(), Error> {
        from.require_auth();
        let mut c = load_campaign(&env, id)?;
        if c.state != State::Open && c.state != State::Funded {
            return Err(Error::NotAcceptingContributions);
        }
        let now = env.ledger().timestamp();
        if now > c.deadline {
            return Err(Error::DeadlinePassed);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        token_client(&env, &c).transfer(&from, &env.current_contract_address(), &amount);

        let index = c.contribution_count;
        let entry_key = DataKey::Entry(id, index);
        env.storage().persistent().set(
            &entry_key,
            &Contribution {
                from: from.clone(),
                amount,
                memo_hash: memo_hash.clone(),
                ledger_time: now,
            },
        );
        extend_persistent(&env, &entry_key);

        let bal_key = DataKey::Balance(id, from.clone());
        let prev: i128 = env.storage().persistent().get(&bal_key).unwrap_or(0);
        env.storage().persistent().set(&bal_key, &(prev + amount));
        extend_persistent(&env, &bal_key);

        c.contribution_count = index + 1;
        c.raised += amount;
        let reached_goal = c.state == State::Open && c.raised >= c.goal;
        if reached_goal {
            c.state = State::Funded;
        }
        save_campaign(&env, id, &c);

        Contributed {
            campaign_id: id,
            index,
            from,
            amount,
            memo_hash,
            raised: c.raised,
            ledger_time: now,
        }
        .publish(&env);
        if reached_goal {
            GoalReached {
                campaign_id: id,
                raised: c.raised,
                goal: c.goal,
            }
            .publish(&env);
        }
        Ok(())
    }

    /// A committee member proposes paying `amount` to registered biller `biller_id`.
    /// The proposer's approval is recorded as the first approval, so with a threshold
    /// of 1 the payout executes immediately. Returns the payout id.
    pub fn propose_payout(
        env: Env,
        id: u32,
        proposer: Address,
        biller_id: u32,
        amount: i128,
        purpose_hash: BytesN<32>,
    ) -> Result<u32, Error> {
        proposer.require_auth();
        let mut c = load_campaign(&env, id)?;
        if c.state != State::Funded {
            return Err(Error::NotFunded);
        }
        if !is_member(&c, &proposer) {
            return Err(Error::NotCommitteeMember);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if amount > available(&c) {
            return Err(Error::InsufficientFunds);
        }
        if c.payout_count >= MAX_PAYOUTS {
            return Err(Error::TooManyPayouts);
        }
        let registry = RegistryClient::new(&env, &c.registry);
        if !registry.is_payable(&biller_id) {
            return Err(Error::BillerNotPayable);
        }

        let payout_id = c.payout_count;
        let mut p = Payout {
            id: payout_id,
            biller_id,
            amount,
            purpose_hash: purpose_hash.clone(),
            approvals: vec![&env, proposer.clone()],
            executed: false,
            proposer: proposer.clone(),
            paid_to: None,
            executed_at: 0,
        };
        c.payout_count = payout_id + 1;

        PayoutProposed {
            campaign_id: id,
            payout_id,
            proposer: proposer.clone(),
            biller_id,
            amount,
            purpose_hash,
        }
        .publish(&env);
        PayoutApproved {
            campaign_id: id,
            payout_id,
            approver: proposer,
            approvals: 1,
            threshold: c.threshold,
        }
        .publish(&env);

        if p.approvals.len() >= c.threshold {
            execute_payout(&env, id, &mut c, &mut p)?;
        }
        save_payout(&env, id, &p);
        save_campaign(&env, id, &c);
        Ok(payout_id)
    }

    /// A committee member approves a pending payout (once each). When approvals reach
    /// the threshold the transfer to the biller happens in the same call and a
    /// `receipt` event is published.
    pub fn approve_payout(
        env: Env,
        id: u32,
        approver: Address,
        payout_id: u32,
    ) -> Result<(), Error> {
        approver.require_auth();
        let mut c = load_campaign(&env, id)?;
        if c.state != State::Funded {
            return Err(Error::NotFunded);
        }
        if !is_member(&c, &approver) {
            return Err(Error::NotCommitteeMember);
        }
        let mut p = read_payout(&env, id, payout_id)?;
        if p.executed {
            return Err(Error::AlreadyExecuted);
        }
        if p.approvals.contains(&approver) {
            return Err(Error::AlreadyApproved);
        }
        p.approvals.push_back(approver.clone());

        PayoutApproved {
            campaign_id: id,
            payout_id,
            approver,
            approvals: p.approvals.len(),
            threshold: c.threshold,
        }
        .publish(&env);

        if p.approvals.len() >= c.threshold {
            execute_payout(&env, id, &mut c, &mut p)?;
        }
        save_payout(&env, id, &p);
        save_campaign(&env, id, &c);
        Ok(())
    }

    /// Switch the campaign to `Refunding`. Two paths:
    ///
    /// * permissionless when the deadline has passed and the goal was not reached;
    /// * organizer-signed cancellation at any time before the first payout executes.
    ///
    /// Because payouts only happen in `Funded` and cancellation is impossible after a
    /// payout, every refund is exact: each contributor gets back precisely what they gave.
    pub fn refund(env: Env, id: u32) -> Result<(), Error> {
        let mut c = load_campaign(&env, id)?;
        if c.state == State::Refunding || c.state == State::Closed {
            return Err(Error::RefundNotAllowed);
        }
        let now = env.ledger().timestamp();
        let goal_missed = c.state == State::Open && now > c.deadline && c.raised < c.goal;
        let reason = if goal_missed {
            symbol_short!("deadline")
        } else {
            c.organizer.require_auth();
            if c.paid_out > 0 {
                return Err(Error::RefundNotAllowed);
            }
            symbol_short!("cancelled")
        };
        c.state = State::Refunding;
        save_campaign(&env, id, &c);
        RefundingStarted {
            campaign_id: id,
            reason,
            raised: c.raised,
            goal: c.goal,
        }
        .publish(&env);
        Ok(())
    }

    /// Send a contributor's whole balance back to them. Anyone may call it (the funds
    /// can only go to the recorded contributor address), so an organizer can push
    /// refunds for contributors who never come back to the page.
    pub fn claim_refund(env: Env, id: u32, contributor: Address) -> Result<(), Error> {
        let mut c = load_campaign(&env, id)?;
        if c.state != State::Refunding {
            return Err(Error::NotRefunding);
        }
        let bal_key = DataKey::Balance(id, contributor.clone());
        let amount: i128 = env.storage().persistent().get(&bal_key).unwrap_or(0);
        if amount <= 0 {
            return Err(Error::NothingToRefund);
        }
        env.storage().persistent().set(&bal_key, &0i128);
        token_client(&env, &c).transfer(&env.current_contract_address(), &contributor, &amount);
        c.refunded += amount;
        save_campaign(&env, id, &c);
        Refunded {
            campaign_id: id,
            contributor,
            amount,
            ledger_time: env.ledger().timestamp(),
        }
        .publish(&env);
        Ok(())
    }

    /// Close the campaign once nothing is left in it. Organizer-signed.
    pub fn close(env: Env, id: u32) -> Result<(), Error> {
        let mut c = load_campaign(&env, id)?;
        c.organizer.require_auth();
        if c.state == State::Closed {
            return Err(Error::AlreadyClosed);
        }
        if available(&c) != 0 {
            return Err(Error::BalanceNotZero);
        }
        c.state = State::Closed;
        save_campaign(&env, id, &c);
        CampaignClosed {
            campaign_id: id,
            raised: c.raised,
            paid_out: c.paid_out,
            refunded: c.refunded,
        }
        .publish(&env);
        Ok(())
    }

    // ------------------------------------------------------------ views

    pub fn campaign_count(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Count).unwrap_or(0)
    }

    pub fn campaign(env: Env, id: u32) -> Result<Campaign, Error> {
        read_campaign(&env, id)
    }

    /// Funds still held for this campaign: raised - paid_out - refunded.
    pub fn available(env: Env, id: u32) -> Result<i128, Error> {
        Ok(available(&read_campaign(&env, id)?))
    }

    /// Total contributed by `addr` that has not been refunded.
    pub fn contribution(env: Env, id: u32, addr: Address) -> Result<i128, Error> {
        read_campaign(&env, id)?;
        Ok(env
            .storage()
            .persistent()
            .get(&DataKey::Balance(id, addr))
            .unwrap_or(0))
    }

    /// The `index`-th contribution in the append-only ledger.
    pub fn contribution_at(env: Env, id: u32, index: u32) -> Result<Contribution, Error> {
        let c = read_campaign(&env, id)?;
        if index >= c.contribution_count {
            return Err(Error::IndexOutOfRange);
        }
        env.storage()
            .persistent()
            .get(&DataKey::Entry(id, index))
            .ok_or(Error::IndexOutOfRange)
    }

    /// A page of the contribution ledger: entries `[start, start + limit)`, clipped to
    /// the end of the ledger. `limit` must be at most `MAX_PAGE`.
    pub fn contributions(
        env: Env,
        id: u32,
        start: u32,
        limit: u32,
    ) -> Result<Vec<Contribution>, Error> {
        if limit > MAX_PAGE {
            return Err(Error::PageTooLarge);
        }
        let c = read_campaign(&env, id)?;
        let mut out = Vec::new(&env);
        let end = core::cmp::min(c.contribution_count, start.saturating_add(limit));
        let mut i = start;
        while i < end {
            let entry: Contribution = env
                .storage()
                .persistent()
                .get(&DataKey::Entry(id, i))
                .ok_or(Error::IndexOutOfRange)?;
            out.push_back(entry);
            i += 1;
        }
        Ok(out)
    }

    pub fn payout(env: Env, id: u32, payout_id: u32) -> Result<Payout, Error> {
        read_campaign(&env, id)?;
        read_payout(&env, id, payout_id)
    }

    /// Every payout proposal of the campaign, in proposal order.
    pub fn payouts(env: Env, id: u32) -> Result<Vec<Payout>, Error> {
        let c = read_campaign(&env, id)?;
        let mut out = Vec::new(&env);
        let mut i = 0;
        while i < c.payout_count {
            out.push_back(read_payout(&env, id, i)?);
            i += 1;
        }
        Ok(out)
    }

    pub fn is_member(env: Env, id: u32, addr: Address) -> Result<bool, Error> {
        Ok(is_member(&read_campaign(&env, id)?, &addr))
    }
}

#[cfg(test)]
mod test;
