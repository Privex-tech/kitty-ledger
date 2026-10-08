# Architecture

## Components

```
 contributors (wallets)        committee (3-7 keys)        organiser          registry admin
        |  contribute                | propose / approve        | create/close      | register/verify
        v                            v                          v                   v
 +-------------------------------------------------------------------+   +--------------------+
 |  campaign contract  (one instance, many campaigns by id)          |-->|  biller_registry   |
 |  ledger of contributions - balances - payouts - state machine     |   |  is_payable(id)    |
 +-------------------------------------------------------------------+   |  biller(id)        |
        | token.transfer (SAC: USDC)                                       +--------------------+
        v
   biller address  --->  anchor / MoneyGram cash-out (partner integration, not code)

 app/web/ledger.html  --simulateTransaction-->  Soroban RPC   (public page, read-only)
 app/src/cli.ts       --prepare/sign/send----->  Soroban RPC   (organiser / committee / contributor)
```

Both contracts are `#![no_std]` Soroban crates on `soroban-sdk 28.0.0`, built for
`wasm32v1-none`. All functions return `Result<_, Error>` with `#[contracterror]` codes,
so the CLI and the tests get named errors rather than opaque traps. `biller_registry`
declares `crate-type = ["cdylib", "rlib"]`: the `rlib` exists only so the campaign
crate's tests can register the real registry contract in the same Soroban host (a
dev-dependency) and exercise the actual cross-contract `is_payable`/`biller` calls
instead of a mock; the wasm build uses the `cdylib` alone. The campaign crate does not
link the registry crate: it declares the two-function interface with `#[contractclient]`
and a mirror of the `Biller` type, so its wasm exports only its own functions.

## Contract interfaces

### `biller_registry`

| Function | Auth | Effect |
|---|---|---|
| `init(admin)` | none (once) | sets the admin |
| `register_biller(name_hash, category, address) -> id` | admin | category in {hospital, school, funeral, utility, other}; starts `active = true`, `verified = false` |
| `set_verified(id, bool)` / `set_active(id, bool)` | admin | flip the two flags |
| `propose_address(id, new) -> apply_after` | admin | records a pending change, `apply_after = now + 24h`; re-proposing restarts the clock |
| `apply_address(id)` | admin | applies once `now >= apply_after`, clears the pending change |
| `cancel_address(id)` | admin | drops the pending change |
| `biller(id)`, `pending_address(id)`, `is_payable(id)`, `admin()`, `biller_count()` | none | views; `is_payable = exists && verified && active` (never panics) |

The spec's "update_address with a 24 h delay" is the `propose_address` / `apply_address`
pair. During the delay the old address stays payable; if the old address is the problem,
the admin uses `set_active(id, false)` which takes effect immediately.

Events: `registry_initialized`, `biller_registered`, `biller_verified`, `biller_active`,
`address_proposed`, `address_applied`, `address_cancelled`.

### `campaign`

| Function | Auth | Rules |
|---|---|---|
| `create(organizer, token, goal, deadline, committee, threshold, registry, title_hash) -> id` | organizer | goal > 0, deadline > now, 1..=7 distinct members, 1 <= threshold <= len |
| `contribute(id, from, amount, memo_hash)` | from | state Open or Funded, `now <= deadline`, amount > 0; token moves into the contract; appends a ledger entry; Open -> Funded when `raised >= goal` |
| `propose_payout(id, proposer, biller_id, amount, purpose_hash) -> payout_id` | committee member | state Funded, `is_payable(biller_id)` via cross-contract call, `0 < amount <= raised - paid_out - refunded`, at most 64 proposals; proposer counts as the first approval; executes immediately if threshold is 1 |
| `approve_payout(id, approver, payout_id)` | committee member | once per member; at the threshold: re-check `is_payable`, fetch the biller's current address, transfer, mark executed, publish `receipt` |
| `refund(id)` | none if `state == Open && now > deadline && raised < goal`; organizer otherwise, only while `paid_out == 0` | state -> Refunding |
| `claim_refund(id, contributor)` | none (funds can only go to the recorded contributor) | pays the contributor's full balance, zeroes it, `refunded += amount` |
| `close(id)` | organizer | requires `raised - paid_out - refunded == 0`; state -> Closed |
| `campaign(id)`, `available(id)`, `contribution(id, addr)`, `contribution_at(id, i)`, `contributions(id, start, limit<=100)`, `payout(id, pid)`, `payouts(id)`, `is_member(id, addr)`, `campaign_count()` | none | views |

The spec lists `propose_payout(id, biller_id, ...)` and `approve_payout(id, payout_id)`;
Soroban's `require_auth` needs the signer as an argument, so both take the member address.

Events on every transition: `campaign_created`, `contributed`, `goal_reached`,
`payout_proposed`, `payout_approved`, `receipt`, `refunding_started`, `refunded`,
`campaign_closed`. `receipt` carries `(campaign_id, payout_id)` as topics and
`{biller_id, to, amount, purpose_hash, approvals, ledger_time}` as data; it is the thing
a contributor can cite.

## State machine

```
            contribute (raised >= goal)
   Open ------------------------------------> Funded
    |                                            |
    | refund(): deadline passed & goal missed    | refund(): organizer, no payout yet
    | (anyone)                                   | propose/approve -> receipts
    v                                            v
 Refunding --claim_refund (each)--> balance 0 --close (organizer)--> Closed
 Funded  --payouts to billers-----> balance 0 --close (organizer)--> Closed
```

Refunds are **exact** by construction: payouts only exist in `Funded`, and `refund` is
refused once `paid_out > 0`, so a campaign is either fully refundable or never refundable.
A pro-rata "excess refund" after payouts is not implemented; leftover funds are paid to
a biller by the committee (see Limits).

## Storage and the entry-size strategy

Soroban charges by ledger entry read/written and caps the size of a single entry. A
campaign with hundreds of contributions must not keep them in one growing `Vec`, where
every `contribute` would read and rewrite the whole list and eventually hit the entry
limit. The campaign contract therefore uses **an index counter plus one entry per item**:

| Key | Value | Notes |
|---|---|---|
| instance `Count` | u32 | next campaign id |
| persistent `Campaign(id)` | `Campaign` | fixed-size header: goal, raised, paid_out, refunded, counters, committee (<= 7) |
| persistent `Entry(id, index)` | `Contribution {from, amount, memo_hash, ledger_time}` | append-only public ledger; `contribute` writes exactly one new entry |
| persistent `Balance(id, address)` | i128 | per-contributor total, what `claim_refund` pays |
| persistent `Payout(id, payout_id)` | `Payout {..., approvals: Vec<Address>, executed, paid_to, executed_at}` | approvals are bounded by the committee size (<= 7) |

Consequences: `contribute` is O(1) in reads and writes regardless of how many gifts
came before; the ledger page reads `contributions(id, start, 100)` pages; `payouts(id)`
is bounded by `MAX_PAYOUTS = 64`. The tests page through the 40-contribution scenario
and reconcile the sum of pages to `raised`.

## Committee model

- The committee is fixed at creation (1..=7 distinct addresses) and cannot be changed; a
  different committee means a new campaign. The organiser is not a member unless listed.
- `threshold` approvals execute a payout. The proposer's approval is the first, so a
  3-of-5 committee needs two more signatures after a proposal.
- Approvals are recorded in the payout entry; a member can approve once. Execution
  happens inside the approving transaction, so there is no separate "execute" step that
  could be forgotten.
- The registry is consulted twice: at proposal (so a member cannot even propose an
  unverified biller) and at execution (so a biller deactivated in the meantime is not
  paid). The address paid is the registry's address at execution time and is snapshotted
  in `paid_to`.
- Failure mode: committee deadlock. Funds stay in the contract; the organiser can cancel
  and trigger exact refunds as long as no payout has executed.

## Trust assumptions

- The registry admin (the association's committee key or a multisig account) is trusted
  to verify billers honestly. The 24 h address-change delay and the public
  `address_proposed` event give contributors and committee members time to object; the
  admin cannot shorten it.
- Token: any SEP-41 token; USDC's Stellar Asset Contract in production. The campaign
  never mints or burns; it only holds and transfers.
- Off-chain metadata (campaign description, memo notes, biller names, purposes) is kept
  by the association and referenced on chain by sha256 hashes, so no personal data is
  written to the ledger and every published note can be checked against its hash.
- The ledger page trusts the RPC it is pointed at; the "verify on Stellar" links to
  stellar.expert give a second, independent reader of the same state.

## TTL

All campaign, contribution, balance and payout entries are persistent. Every state
transition extends the touched entries (and the contract instance) to 120 days when
fewer than 30 days remain (`TTL_THRESHOLD = 30 * 17_280`, `TTL_EXTEND_TO = 120 * 17_280`
ledgers at ~5 s). A campaign that is active within any 90-day window never expires;
an abandoned campaign's entries are archived by the network and can be restored with a
standard `RestoreFootprint` before use. Views do not extend TTL (they are simulated, not
submitted).

## Why Stellar

- The users' money arrives as USD/GBP/EUR/AED and must leave as KES/NGN/GHS at a
  hospital's bank or a pharmacy's M-Pesa paybill. Stellar anchors and MoneyGram Access
  already do that leg; the contract only has to pay a registered address.
- Gifts are $5-$50. Stellar fees are a fraction of a cent, so a $5.55 gift is not eaten
  by the network (research/08 §04, "Why Stellar").
- USDC on Stellar is the unit everyone in the flow understands; no bridging.
- Soroban's `require_auth` gives committee approvals real signatures without a custom
  multisig contract.

## Limits and known gaps

- No excess/pro-rata refund after a payout; leftovers go to a biller by committee decision.
- The committee cannot be rotated; a lost committee key reduces the effective quorum.
- `claim_refund` is push-style and permissionless; it cannot be abused (funds go only
  to the contributor) but a contributor whose account no longer has a USDC trustline
  will see the transfer fail until they re-add it.
- No on-chain campaign metadata beyond hashes; the page depends on the association
  publishing the notes and the `names.json` map.
- Anchor/MoneyGram cash-out is documented as a partner integration and not implemented.
- The ledger page loads the Stellar SDK from a CDN; offline it can only render an
  exported snapshot.
