//! Biller registry: an admin-managed list of payout destinations that a
//! campaign is allowed to pay. Contributors are protected by two rules:
//!
//! * a biller is only payable when it is both `verified` and `active`, and
//! * the payout address of a biller can only be changed through a two-step
//!   propose/apply flow with a 24 hour delay, so a compromised admin key cannot
//!   silently redirect funds before the committee notices.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, symbol_short, Address,
    BytesN, Env, Symbol,
};

/// Delay, in seconds, between proposing a new biller address and being able to apply it.
pub const ADDRESS_CHANGE_DELAY: u64 = 24 * 60 * 60;

/// Approximate ledgers per day at a 5 second close time.
const DAY_LEDGERS: u32 = 17_280;
/// Extend entries when fewer than this many ledgers of TTL remain.
const TTL_THRESHOLD: u32 = 30 * DAY_LEDGERS;
/// Extend entries to this many ledgers of TTL.
const TTL_EXTEND_TO: u32 = 120 * DAY_LEDGERS;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidCategory = 3,
    /// Biller with the given ID does not exist in the registry.
    BillerNotFound = 4,
    NoPendingChange = 5,
    DelayNotElapsed = 6,
    SameAddress = 7,
}

/// A payout destination. `name_hash` is the sha256 of the biller's legal name and
/// registration number, kept off-chain by the association; the hash lets anyone verify
/// a published record without storing personal data on the ledger.
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

/// A proposed address change waiting for the delay to elapse.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAddress {
    pub new_address: Address,
    pub proposed_at: u64,
    pub apply_after: u64,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Count,
    Biller(u32),
    Pending(u32),
}

// ---------------------------------------------------------------- events

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryInitialized {
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillerRegistered {
    #[topic]
    pub id: u32,
    pub category: Symbol,
    pub address: Address,
    pub name_hash: BytesN<32>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillerVerified {
    #[topic]
    pub id: u32,
    pub verified: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillerActive {
    #[topic]
    pub id: u32,
    pub active: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressProposed {
    #[topic]
    pub id: u32,
    pub current_address: Address,
    pub new_address: Address,
    pub apply_after: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressApplied {
    #[topic]
    pub id: u32,
    pub old_address: Address,
    pub new_address: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressCancelled {
    #[topic]
    pub id: u32,
    pub new_address: Address,
}

// ---------------------------------------------------------------- contract

#[contract]
pub struct BillerRegistry;

fn is_known_category(category: &Symbol) -> bool {
    *category == symbol_short!("hospital")
        || *category == symbol_short!("school")
        || *category == symbol_short!("funeral")
        || *category == symbol_short!("utility")
        || *category == symbol_short!("other")
}

fn admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get::<_, Address>(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn require_admin(env: &Env) -> Result<Address, Error> {
    let a = admin(env)?;
    a.require_auth();
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    Ok(a)
}

/// Loads a biller from storage and bumps its TTL to keep it active.
fn load_biller(env: &Env, id: u32) -> Result<Biller, Error> {
    let key = DataKey::Biller(id);
    let b = env
        .storage()
        .persistent()
        .get::<_, Biller>(&key)
        .ok_or(Error::BillerNotFound)?;
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    Ok(b)
}

fn load_pending(env: &Env, id: u32) -> Result<PendingAddress, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Pending(id))
        .ok_or(Error::NoPendingChange)
}

fn save_biller(env: &Env, b: &Biller) {
    let key = DataKey::Biller(b.id);
    env.storage().persistent().set(&key, b);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

#[contractimpl]
impl BillerRegistry {
    /// One-time initialisation with the admin (the association's committee key).
    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Count, &0u32);
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
        RegistryInitialized { admin }.publish(&env);
        Ok(())
    }

    /// Register a new biller. It starts `active` but not `verified`, so it cannot be paid
    /// until the admin has checked the destination (bank letter, hospital invoice header,
    /// anchor KYC) and called `set_verified`. Returns the new biller id.
    pub fn register_biller(
        env: Env,
        name_hash: BytesN<32>,
        category: Symbol,
        address: Address,
    ) -> Result<u32, Error> {
        require_admin(&env)?;
        if !is_known_category(&category) {
            return Err(Error::InvalidCategory);
        }
        let id: u32 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);
        let biller = Biller {
            id,
            name_hash: name_hash.clone(),
            category: category.clone(),
            address: address.clone(),
            verified: false,
            active: true,
        };
        save_biller(&env, &biller);
        env.storage().instance().set(&DataKey::Count, &(id + 1));
        BillerRegistered {
            id,
            category,
            address,
            name_hash,
        }
        .publish(&env);
        Ok(id)
    }

    pub fn set_verified(env: Env, id: u32, verified: bool) -> Result<(), Error> {
        require_admin(&env)?;
        let mut b = load_biller(&env, id)?;
        b.verified = verified;
        save_biller(&env, &b);
        BillerVerified { id, verified }.publish(&env);
        Ok(())
    }

    pub fn set_active(env: Env, id: u32, active: bool) -> Result<(), Error> {
        require_admin(&env)?;
        let mut b = load_biller(&env, id)?;
        b.active = active;
        save_biller(&env, &b);
        BillerActive { id, active }.publish(&env);
        Ok(())
    }

    /// Step one of an address change: record the new address and the earliest time it
    /// may be applied (`now + 24h`). Proposing again replaces the pending change and
    /// restarts the clock. Returns the `apply_after` timestamp.
    pub fn propose_address(env: Env, id: u32, new_address: Address) -> Result<u64, Error> {
        require_admin(&env)?;
        let b = load_biller(&env, id)?;
        if b.address == new_address {
            return Err(Error::SameAddress);
        }
        let now = env.ledger().timestamp();
        let pending = PendingAddress {
            new_address: new_address.clone(),
            proposed_at: now,
            apply_after: now + ADDRESS_CHANGE_DELAY,
        };
        let key = DataKey::Pending(id);
        env.storage().persistent().set(&key, &pending);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        AddressProposed {
            id,
            current_address: b.address,
            new_address,
            apply_after: pending.apply_after,
        }
        .publish(&env);
        Ok(pending.apply_after)
    }

    /// Step two: apply the pending change once the delay has elapsed.
    pub fn apply_address(env: Env, id: u32) -> Result<(), Error> {
        require_admin(&env)?;
        let mut b = load_biller(&env, id)?;
        let pending = load_pending(&env, id)?;
        if env.ledger().timestamp() < pending.apply_after {
            return Err(Error::DelayNotElapsed);
        }
        let old_address = b.address.clone();
        b.address = pending.new_address.clone();
        save_biller(&env, &b);
        env.storage().persistent().remove(&DataKey::Pending(id));
        AddressApplied {
            id,
            old_address,
            new_address: pending.new_address,
        }
        .publish(&env);
        Ok(())
    }

    /// Withdraw a pending address change.
    pub fn cancel_address(env: Env, id: u32) -> Result<(), Error> {
        require_admin(&env)?;
        load_biller(&env, id)?;
        let pending = load_pending(&env, id)?;
        env.storage().persistent().remove(&DataKey::Pending(id));
        AddressCancelled {
            id,
            new_address: pending.new_address,
        }
        .publish(&env);
        Ok(())
    }

    // ------------------------------------------------------------ views

    pub fn admin(env: Env) -> Result<Address, Error> {
        admin(&env)
    }

    pub fn biller_count(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Count).unwrap_or(0)
    }

    pub fn biller(env: Env, id: u32) -> Result<Biller, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Biller(id))
            .ok_or(Error::BillerNotFound)
    }

    pub fn pending_address(env: Env, id: u32) -> Option<PendingAddress> {
        env.storage().persistent().get(&DataKey::Pending(id))
    }

    /// True when a campaign may pay this biller: it exists, is verified and is active.
    /// Unknown ids return false rather than panicking so callers can use it as a guard.
    pub fn is_payable(env: Env, id: u32) -> bool {
        match env
            .storage()
            .persistent()
            .get::<_, Biller>(&DataKey::Biller(id))
        {
            Some(b) => b.verified && b.active,
            None => false,
        }
    }
}

#[cfg(test)]
mod test;
