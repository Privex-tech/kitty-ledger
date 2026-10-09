# Kitty Ledger

Community and diaspora campaign funds on Stellar where every contribution is public,
every payout needs M-of-N committee approval, and money goes straight to a registered
biller (hospital, school, funeral home) instead of through one person's phone.

## The problem

When a hometown association, church group or alumni chapter raises money for a medical
emergency, school fees or a funeral, the workflow today is a WhatsApp broadcast, M-Pesa
transfers to one trusted individual's number, a cash handover and screenshots as
receipts. GoFundMe is unavailable in nearly all of sub-Saharan Africa and cannot pay out
to African bank accounts; the African alternatives hold the funds themselves and offer
no committee control. Donors abroad pay 5-8% in remittance fees and cannot see where the
money went; organisers spend the campaign answering "where did my money go?"; and payment
fraud is the most common cybercrime in Kenya (Sh29.9B lost in 2025). See
`research/04-problems-consumer-creator-services.md` §P32 and
`research/08-final-selection.md` §04 for sources and the full dossier.

## Who it is for

- **Organiser** (association secretary): creates the campaign, shares the ledger page,
  closes the campaign when the balance is zero.
- **Committee** (3-7 trusted members, M-of-N): proposes and approves payouts; nobody can
  move money alone.
- **Contributors** (the diaspora): give USDC from anywhere, see their gift on the public
  ledger, and get an exact refund if the goal is missed.
- **Billers** (hospital, school, funeral home, pharmacy): registered and verified once,
  paid directly, with a public receipt for every transfer.

## What the MVP does

Two strictly Soroban-based smart contracts, a CLI and a public ledger page:

| Piece | What it does |
|---|---|
| `contracts/biller_registry` | Admin-managed list of payout destinations by category. A biller can only be paid when verified and active. Address changes are two-step with a 24 h delay so a compromised admin key cannot quietly redirect funds. |
| `contracts/campaign` | Campaigns with goal, deadline, token, committee and threshold. Public append-only contribution ledger; M-of-N payout approval with execution at the threshold; `receipt` event per executed payout; exact refunds when the goal is missed or the organiser cancels before any payout; close at zero balance. |
| `app/` (`kitty` CLI) | `create`, `contribute`, `propose`, `approve`, `refund`, `close`, `status`, `export`, `csv`. Builds and signs transactions with `@stellar/stellar-sdk`; submits when an RPC is reachable, otherwise prints the unsigned XDR. |
| `app/web/ledger.html` | Plain HTML+JS page: paste a campaign contract id and RPC URL, get the running total, every contribution (with optional display names from a local JSON map), payout proposals with approvals, executed receipts and "verify on Stellar" links to stellar.expert. Also renders an offline `kitty status --json` snapshot. |

Verified behaviour (all in `cargo test`, executed inside the Soroban host):

- goal met / goal not met at the deadline; state machine Open -> Funded -> (Refunding) -> Closed;
- payouts only to `is_payable` billers, re-checked at execution time;
- one approval per committee member, non-members rejected, duplicates rejected, execution
  exactly at the threshold with a `receipt` event and the funds moved to the biller;
- refunds are exact (each contributor gets back precisely what they gave) and permissionless
  once the deadline has passed with the goal missed;
- registry address changes cannot be applied before 24 h have elapsed;
- every auth rule has a negative test using explicit `mock_auths`, not `mock_all_auths`.

## Quickstart

Prerequisites: Rust 1.94 with the `wasm32v1-none` target, stellar-cli 28, Node 22
(see `/TOOLCHAIN.md`).

```bash
cd stellar/kitty-ledger

# contracts: 63 tests inside the Soroban host, then both wasm files
cargo test
stellar contract build            # target/wasm32v1-none/release/{biller_registry,campaign}.wasm

# app: 34 offline tests (no RPC, no keys)
cd app && npm install && npm test

# validate the seed pledge list (40 contributors, 2 malformed rows)
npm run kitty -- csv ../data/seed/contributors.csv

# build an unsigned contribution without any network
SECRET_KEY=$(node -e "console.log(require('@stellar/stellar-sdk').Keypair.random().secret())") \
CAMPAIGN_CONTRACT_ID=CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC \
npm run kitty -- contribute --campaign 0 --amount '$25.37' --memo "Pole sana" --dry-run
```

`scripts/build.sh` runs all of the above. `scripts/deploy-testnet.sh` deploys both
contracts, registers the seed billers and creates the seed campaign on testnet; it was
written against stellar-cli 28 but **not executed here** because testnet, Horizon and
Friendbot were unreachable from the build environment. `DEMO.md` has the full walk-through
and expected output; `.env.example` lists every variable the CLI reads.

## Money in, money out: the cash legs

The contracts move a Stellar token (USDC on mainnet, a test asset on testnet). The two
fiat legs are partner integrations, documented here and not implemented in code:

- **Contributors on-ramp** with any Stellar wallet or a SEP-24 anchor in their country
  (USD, GBP, EUR, AED). Diaspora contributors already hold USDC in wallets more often
  than organisers expect; a WhatsApp-native flow is the first UX gap (see VALIDATION.md).
- **Billers cash out** through an anchor or MoneyGram Access: the biller's registered
  address is the deposit address of a SEP-6/SEP-24 anchor account (KES/NGN/GHS bank
  transfer, M-Pesa paybill) or a MoneyGram cash-pickup flow run by the biller's accounts
  clerk. `data/seed/billers.json` records the intended cash-out route per biller. The
  registry admin verifies the route (invoice header, bank letter, anchor KYC) before
  setting `verified = true`, which is the moment the biller becomes payable.

This is the SCF Build Award "Integration" track story: the disbursement metric (share of
payouts with a public receipt) is produced by the contract, the cash-out is produced by
the anchor/MoneyGram partner, and the association never touches the money.

## Repository layout

```
Cargo.toml                 workspace (soroban-sdk 28, release profile per CONTRIBUTING.md)
contracts/biller_registry  registry contract + tests + test_snapshots/
contracts/campaign         campaign contract + tests + test_snapshots/
app/                       TypeScript CLI (src/), offline tests (test/), web/ledger.html, web/names.json
data/seed/                 contributors.csv (40 + 2 malformed rows), billers.json, campaign.json
scripts/                   build.sh, deploy-testnet.sh (not executed here)
ARCHITECTURE.md VALIDATION.md DEMO.md .env.example
```

## Status

![Validation](https://img.shields.io/badge/validation-passed-success)

**functional locally** (contracts, CLI and ledger page run and are tested against the
Soroban host and offline fixtures); **testnet-ready** scripts provided, not executed.
No deployment, no users, no measured metric yet.
