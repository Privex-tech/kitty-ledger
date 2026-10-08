# Demo

Everything below runs offline except the last section, which needs Stellar testnet and
was not executed in the build environment. Commands are given from
`stellar/kitty-ledger` unless noted; outputs are what this build produced.

## 1. Contracts: the whole journey inside the Soroban host

```bash
cargo test
```

Expected (abridged):

```
running 19 tests            # contracts/biller_registry
test test::scenario_registry_address_change_delay_enforced ... ok
...
test result: ok. 19 passed; 0 failed

running 44 tests            # contracts/campaign
test test::scenario_medical_campaign ... ok
test test::scenario_goal_missed_every_contributor_refunded_exactly ... ok
test test::approve_payout_executes_at_threshold_and_publishes_receipt ... ok
test test::approve_payout_rejects_duplicate_approvals ... ok
test test::propose_payout_rejects_unpayable_billers ... ok
test test::refund_after_deadline_with_goal_missed_needs_no_signature ... ok
...
test result: ok. 44 passed; 0 failed
```

`scenario_medical_campaign` seeds the 40 contributions from `data/seed/contributors.csv`
($3,311.82 against a $3,000.00 goal), rejects a proposal to the unverified clinic, runs the
$2,750.00 hospital deposit through a 3-of-5 committee (a cousin's approval and a duplicate
approval are refused), pays the $561.82 pharmacy bill, checks the contract balance is
zero, and closes. `scenario_goal_missed_...` runs the same 40 contributions against a
$10,000 goal, flips to refunding after the deadline with no signature, and returns every
contributor's exact amount. Ledger state after each test is recorded under
`contracts/*/test_snapshots/`.

```bash
stellar contract build
ls -l target/wasm32v1-none/release/*.wasm
```

```
10194  biller_registry.wasm
22935  campaign.wasm
```

## 2. App: offline tests

```bash
cd app && npm install && npm test
```

```
# tests 34
# pass 34
# fail 0
```

Covers CSV ingestion of the messy pledge list, the approval-threshold mirror, the export
format (JSON and CSV), CLI argument parsing, offline transaction building/decoding, and
the CLI binary itself (`csv`, `--dry-run`, error exits).

## 3. Validate the pledge list

```bash
cd app
node dist/src/cli.js csv ../data/seed/contributors.csv
```

```
  2  Wanjiru Kamau                 50.00  GDX7…KHLA  for mama Njeri, from the Houston chapter
  3  Kwame Boateng                 25.37  GDPG…BB7D  Pole sana. Get well soon
  4  Adaeze Okonkwo               100.00  GADC…RHFD  Nakuru alumni 2009
  5  Samuel Mwangi-O'Brien        500.00  GBAH…NKXK  hospital deposit - call me if short
  6  Fatuma Abdi                    5.00  GBH4…WNMC
  7  Tendai Moyo                   12.50  GBGR…T2BX  from Leeds
  ...
 13  REJECTED: invalid amount "fifty dollars": not a number
 30  REJECTED: invalid Stellar address "GABCDEF123"
40 valid rows, 2 rejected, total 3,311.82
```

(Addresses are printed in full by the CLI; shortened here.) "$25.37", "12,50", "USD 20"
and "  75.25  " all parse; the two malformed rows are reported with their line numbers.

## 4. Status and export from a snapshot (no RPC)

`data/seed/snapshot-medical.json` is the medical campaign after both payouts, in the
exact shape `kitty status --json` writes when it reads a live contract.

```bash
node dist/src/cli.js status --campaign 0 --from-snapshot ../data/seed/snapshot-medical.json --names web/names.json
```

```
Campaign #0 on CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC (testnet)
  state       Closed
  raised      3,311.82 / 3,000.00 (110.39%)
  paid out    3,311.82   refunded 0.00   held 0.00
  deadline    2025-10-24T00:00:00.000Z
  committee   3 of 5: Rev. Margaret Wambui, James Karanja, Lucy Nyokabi, Dr. Paul Gitau, Ann Wanjiru
  ledger      40 contributions from 40 addresses
  payouts     2 executed / 2 proposed, receipts total 3,311.82
  reconciles  yes
  verify      https://stellar.expert/explorer/testnet/contract/CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
  payout  biller  amount        approvals  status
  #0      #0          2,750.00  3/3        paid to Kenyatta National Hospital at 2025-10-06T00:00:00.000Z
  #1      #3            561.82  3/3        paid to Goodlife Pharmacy Kenyatta Ave at 2025-10-14T00:00:00.000Z
```

```bash
node dist/src/cli.js export --campaign 0 --from-snapshot ../data/seed/snapshot-medical.json --names web/names.json --format csv --out /tmp/ledger.csv
node dist/src/cli.js export --campaign 0 --from-snapshot ../data/seed/snapshot-medical.json --names web/names.json > /tmp/ledger.json
```

`/tmp/ledger.csv` has a header, 40 `contribution` rows and 2 `receipt` rows:

```
section,index,time,address,name,amount,hash,detail
contribution,0,2025-09-24T00:00:00.000Z,GDX7…KHLA,Wanjiru Kamau,50.00,812255ae…,https://stellar.expert/explorer/testnet/account/GDX7…KHLA
...
receipt,0,2025-10-06T00:00:00.000Z,GB3G…WW3Y,biller #0,2750.00,6380af09…,3/3 approvals
receipt,1,2025-10-14T00:00:00.000Z,GCGI…RO7O,biller #3,561.82,850ce0a0…,3/3 approvals
```

`/tmp/ledger.json` carries the same data with names, decimal amounts and a `summary`:

```json
{ "goal": "3000.00", "raised": "3311.82", "paid_out": "3311.82", "refunded": "0.00",
  "available": "0.00", "percent_of_goal": 110.39, "contributors": 40, "contributions": 40,
  "payouts_proposed": 2, "payouts_executed": 2, "receipts_total": "3311.82", "reconciles": true }
```

## 5. Build a transaction without a network

```bash
SECRET_KEY=$(node -e "console.log(require('@stellar/stellar-sdk').Keypair.random().secret())") \
CAMPAIGN_CONTRACT_ID=CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC \
node dist/src/cli.js contribute --campaign 0 --amount '$25.37' --memo "Pole sana" --dry-run
```

```
dry run: contribute on CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
AAAAAgAAAADF9ABOK7yF4eH6JjQTU2EbToO7MBSea9oZadVc6Q1sLwAAAGQAAAAAAAAAAQAAAAEAAAAA…
```

The XDR decodes to `contribute(0, <your G address>, 253700000, sha256("Pole sana"))`
(the test `kitty contribute --dry-run ...` checks exactly that). Without `--dry-run` the
CLI tries the RPC; when it is unreachable it prints the same XDR and exits with code 2.

## 6. The public ledger page

```bash
cd app/web && python3 -m http.server 8080     # so the page can fetch names.json next to it
```

Open <http://localhost:8080/ledger.html>. Two ways to fill it:

- **Offline:** under "Snapshot", choose `data/seed/snapshot-medical.json`. The page renders
  the hero figure `3,311.82`, the meter at 110.39% of the $3,000.00 goal, four stat tiles
  (40 contributions / 3,311.82 paid to billers / 0.00 held / 3 of 5 committee), the two
  payouts with their three approvers each and "paid" badges with receipt links, the
  40-row contribution table with display names from `names.json`, and the committee.
  Status line: "rendered from snapshot … (not live)".
- **Live:** paste a campaign contract id and RPC URL (or open
  `ledger.html?contract=C…&campaign=0&network=testnet`) and press "Load from chain". The
  page simulates `campaign`, `contributions` (pages of 100) and `payouts` from the null
  account, so it needs no key and no server. Every address and the contract link to
  stellar.expert under "verify".

The page loads `@stellar/stellar-sdk@17.1.0` from jsDelivr; without internet only the
snapshot path works.

## 7. Testnet (written, not executed here)

```bash
scripts/build.sh                 # tests + wasm + app
scripts/deploy-testnet.sh        # keys via friendbot, SAC test token, both contracts, seed billers, seed campaign
cp .env.example .env             # paste the ids the script prints
cd app
node dist/src/cli.js contribute --campaign 0 --amount 25.37 --memo "Pole sana"        # contributor key
node dist/src/cli.js propose    --campaign 0 --biller 0 --amount 2750 --purpose "KNH admission deposit"   # committee key
node dist/src/cli.js approve    --campaign 0 --payout 0                              # second, then third committee key
node dist/src/cli.js status     --campaign 0 --names web/names.json
node dist/src/cli.js export     --campaign 0 --format json --out ledger.json
```

Expected: each command prints `method: SUCCESS (<tx hash>)` and a stellar.expert link;
the third `approve` moves the funds to the hospital's registered address and `status`
shows `1 executed / 1 proposed`. This path depends on network access that the build
environment did not have, so it is documented, not demonstrated.
