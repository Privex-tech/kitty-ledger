# Validation

This file separates what was researched, what was observed while building, what the
team assumes, what is hypothesised, what is simulated, and what has actually been
validated (nothing yet). Sources are cited by section of the research dossier.

## Researched evidence

From the research dossier (problem P32) (community, diaspora and
mutual-aid fundraising with transparent disbursement) and the final selection (section 04) (Kitty Ledger):

- **Workflow today.** Emergency -> WhatsApp group -> M-Pesa/mobile-money contributions to
  one trusted individual's phone -> cash handed over; no ledger, treasurer discretion.
- **GoFundMe unavailability.** GoFundMe is unavailable in nearly all of sub-Saharan Africa
  and cannot pay out to African bank accounts; African alternatives exist (OneKitty,
  SAPEO at 0% fees, M-Changa) but hold the funds themselves and limit donors to Africa
  (04-P32, verified: sapeo.org, thecrowdspace.com).
- **Fees.** Remittance fees of 5-8% on the cross-border leg (04-P32, "Cost").
- **Fraud.** Kenya recorded Sh29.9B in cybercrime losses in 2025 with payment fraud the
  most common category; 2.3% of transaction attempts are suspected fraud (04-P32,
  verified: allafrica.com, eastleighvoice.co.ke).
- **Demand signal.** Diaspora philanthropy is rising in Ethiopia, Ghana, Senegal, Uganda
  and Zimbabwe (04-P32, verified: brookings.edu).
- **Competitors.** M-Changa, OneKitty, SAPEO (operator custody, Africa-only donors);
  GoFundMe (unavailable); StreamGive, Homebound-Remit, OphirPay on Stellar (early, no
  diaspora-association channel) (08 §04 "Competitors", 04-P32 "Alternatives").
- **Why Stellar.** Anchors and MoneyGram cash-out for billers and beneficiaries, sub-cent
  fees on $5-$50 gifts, USDC/local stablecoins; Arbitrum has no fiat rail for this user
  and gas on tiny gifts (08 §04).
- **Evidence caveat carried over from 04 §0.** Numeric claims rest on the source page as
  surfaced by search, not on a full re-read; 04 also notes that Soroban crowdfunding
  prototypes are plentiful (3-40 repos per problem, 0-2 stars) and that validated demand
  and distribution, not code, are the constraint. The scorecard in 08 rates this
  project's problem evidence as the weakest of the Stellar five and keeps it because the
  chain rationale is strong and the experiment is cheap.

## Observed facts (this build)

- Both contracts compile to wasm (`biller_registry.wasm` 11,273 bytes, `campaign.wasm`
  25,490 bytes with the current toolchain) and pass 63 tests executed inside the Soroban host: 19 registry, 44
  campaign, including the two scenario tests and negative tests for every auth, state,
  deadline and threshold rule.
- The CLI and its 35 offline tests run without network access; the messy seed CSV yields
  exactly 40 valid contributors totalling $3,311.82 and rejects 2 rows with reasons.
- Testnet deployment and a full campaign were executed on 2026-11-05 (UTC); see "Testnet
  end-to-end run" below. An earlier offline build environment could not reach testnet.
- No organiser, contributor, committee member or biller has seen or used the software.

## Team assumptions

- Diaspora contributors can obtain USDC on Stellar (wallet or SEP-24 anchor) with less
  friction than they tolerate today at Western Union or via a relative's M-Pesa.
- Billers (hospitals, schools, funeral homes, pharmacies) will accept payment to a
  Stellar address that an anchor or MoneyGram turns into KES/NGN/GHS, once the
  association's committee introduces them.
- The association secretary is willing to run a CLI or a thin web form, and to publish
  the memo notes so the hashes on chain can be checked.
- A 24 h address-change delay is long enough for a committee to notice and object.

## Hypotheses

- H1: When every payout has a public receipt, "where did the money go?" questions per
  campaign drop measurably and contributors give again on the next campaign.
- H2: Biller-direct payouts are the organiser's real objection-killer; committee
  approvals matter to contributors more than to organisers.
- H3: The ledger page is opened by a majority of contributors at least once during a
  campaign when it is linked from the WhatsApp broadcast.

## Simulated / demo data

- `data/seed/contributors.csv`: 40 diaspora contributors (realistic names, countries and
  memo notes, amounts $5.00-$500.00 with odd cents) plus two malformed rows (an amount in
  words, a truncated address). Total $3,311.82; the addresses are freshly generated keys.
- `data/seed/billers.json`: hospital, school, funeral home, pharmacy (verified) and a
  clinic pending KYC (unverified). `data/seed/campaign.json`: goal $3,000.00, 3-of-5
  committee, two planned payouts ($2,750.00 hospital deposit, $561.82 pharmacy).
- `scenario_medical_campaign` and `scenario_goal_missed_every_contributor_refunded_exactly`
  replay this data inside the Soroban host; `test_snapshots/` holds the recorded ledger
  state of every test.

## Actual validation

Technical validation: done on testnet, see below. User validation: none yet. No pilot, no
interview, no measured baseline; the team is the only user so far.

## Testnet end-to-end run (executed 2026-11-05 UTC)

The team ran both contracts on Stellar testnet through the `kitty` CLI, using throwaway keys
(`stellar keys generate --fund`), a wrapped test asset `KUSD` (7 decimals) and wasm built with
`stellar contract build`. Every row is a transaction anyone can open on stellar.expert.

| Item | Value |
|---|---|
| Biller registry | [`CAFBA3ZK272HVXXLSUNTAIN2TJOT37LQJBXW5TDAZYZFNNI4QUALIKQC`](https://stellar.expert/explorer/testnet/contract/CAFBA3ZK272HVXXLSUNTAIN2TJOT37LQJBXW5TDAZYZFNNI4QUALIKQC) |
| Campaign contract | [`CDZXXSO76KUOUG4ORYU77EYX47MGSLIHVQTD7ATESI5V7D45ORFQOY4H`](https://stellar.expert/explorer/testnet/contract/CDZXXSO76KUOUG4ORYU77EYX47MGSLIHVQTD7ATESI5V7D45ORFQOY4H) |
| KUSD token (SAC) | [`CDVX3BP3ZPFY4AH5SMDGNURGYKOC3FLZSHFLVJPZ462QGRD3D5ZJARZK`](https://stellar.expert/explorer/testnet/contract/CDVX3BP3ZPFY4AH5SMDGNURGYKOC3FLZSHFLVJPZ462QGRD3D5ZJARZK) |
| wasm sizes | `biller_registry.wasm` 11,273 bytes, `campaign.wasm` 25,490 bytes |

**Campaign 0: goal met, committee payout.** Goal 3,000.00, 2-of-3 committee.

| Step | CLI command | Result |
|---|---|---|
| Register biller "Kenyatta National Hospital" | `biller-register` | [`d3781a92…`](https://stellar.expert/explorer/testnet/tx/d3781a9242aaa9de81fee189aa48de70b8ec7a41aa8f3575960809103fa9d659), id 0 |
| Verify biller | `biller-verify` | [`484d72d9…`](https://stellar.expert/explorer/testnet/tx/484d72d9cb9c7c438755efd2560679d5d6adee00dab342628c00335897f9c214); `biller-status` shows `payable: true` |
| Create campaign | `create` | [`7debca20…`](https://stellar.expert/explorer/testnet/tx/7debca20245344f38b45221463907741fcd580dfb5824e4f953a27c9611eb689) |
| Contribute 2,000.00 and 1,100.50 | `contribute` (two keys) | [`29c816a2…`](https://stellar.expert/explorer/testnet/tx/29c816a2dd12c4f684caf590d51a73b7fa4e85d8e47704ba9a30c66549c4ab31), [`752e6677…`](https://stellar.expert/explorer/testnet/tx/752e6677d41a0ad8cdbd5c7aedf8a9ee608bb9f9206573bd1f9497a654a70670); raised 3,100.50, state `Funded` |
| Propose 2,750.00 payout | `propose` | [`34a416d2…`](https://stellar.expert/explorer/testnet/tx/34a416d238785f5860f68e5a3fb502d92661f2f90466b18367e6fa328bf44011) (1 of 2 approvals) |
| Second approval | `approve` | [`6c423ed1…`](https://stellar.expert/explorer/testnet/tx/6c423ed1bcde9d016997470766ce6252f7ae9ef91bce52b5afeace6f571d9b39); payout executed, receipt published |

Observed afterwards: biller token balance exactly 2,750.00; `status` reports paid out 2,750.00,
held 350.50, `reconciles yes`. Ledger export: `data/validation/testnet-campaign-0.json`.

**Campaign 1: goal missed, exact refund.** Goal 1,000.00, 100.00 contributed, deadline 150 s out.

| Step | Result |
|---|---|
| Create, contribute 100.00 | [`ee392ef0…`](https://stellar.expert/explorer/testnet/tx/ee392ef0650a4e6c2f2c03461bbb135e702bb337d26befc147a919c05b08dc19), [`f719da5b…`](https://stellar.expert/explorer/testnet/tx/f719da5b1d9be2a00e47672862740aec6ebdf5bef4d5028b34a00e84d808fed3) |
| `refund` after the deadline, signed by a committee member who is not the organiser | [`9a596742…`](https://stellar.expert/explorer/testnet/tx/9a596742b479c74243ca8b1ad229bb28b02fdd7e3b868e6ecaff590a88469a3b); state `Refunding` |
| `refund --claim` for the contributor | [`20be326f…`](https://stellar.expert/explorer/testnet/tx/20be326fa117a41d6e98c123a8e7c456f452b6ea0e83f47c9255d48a06621075); contributor balance rose by exactly 100.00 (2,299.50 to 2,399.50) |
| `close` by the organiser at zero balance | [`effabbb7…`](https://stellar.expert/explorer/testnet/tx/effabbb7c48061190bf079a3c3033f8b57eb1a0544fe9208e7bb6318682b1102) |

Not exercised on testnet: the 24 h address-change delay (covered by the contract tests only).
A first attempt at the contributions failed with a token error because the test wallets were
under-funded (500 minted, 2,000 requested); the CLI surfaced the contract error and the
unsigned XDR. Wallets were re-funded and the steps above are the successful re-run.


## Baseline and success metric

- **Baseline (researched, not measured):** 0% of payouts in a WhatsApp/M-Pesa campaign
  have a public, independently verifiable receipt; committee approval is informal and
  unrecorded; the number of "where did the money go?" questions per campaign is unknown
  and will be counted in the pilot.
- **Success metric (from 08 §04):** share of payouts with a public receipt 0% -> 100%;
  committee approvals recorded on chain 100%; contributor "where did the money go?"
  questions per campaign: pilot baseline -> measured drop. Secondary: share of
  contributors who open the ledger page; repeat-contribution rate on the next campaign.

## Experiment plan: two associations, one real campaign each

1. Recruit two diaspora associations (hometown association or church chapter with an
   existing WhatsApp giving group) and one biller each that will accept an
   anchor/MoneyGram settlement.
2. Deploy both contracts to testnet (`scripts/deploy-testnet.sh`), register the biller,
   verify it with a bank letter or invoice header, create the campaign, and post the
   ledger page link in the WhatsApp group.
3. Run the campaign for its natural duration (typically 2-6 weeks). Count: contributors,
   contributions, page opens (from the association's link shortener), payouts with a
   receipt, "where did the money go?" messages in the group (before/after comparison
   against the association's previous campaign thread).
4. Interview five contributors and the committee after the campaign: did the receipt
   change anything, would they use it again, what broke (wallet onboarding is the
   expected answer).
5. Decide: if neither association completes a campaign, the recorded replacement is SDP
   restricted-use vouchers (08 "Internal scorecard").

## Killer questions

1. **Would the user care if it disappeared?** Plausible from the researched evidence
   (GoFundMe absence, fraud losses, opaque disbursement); untested with a real association.
2. **Did anyone outside the team use it?** No. The testnet run above was performed by the team.
3. **Before/after measured?** No. The baseline is researched, the metric is defined, the
   count starts in the pilot.
4. **Value lost without AI?** None; there is no AI component by design.
5. **Reason to keep using after the demo?** Hypothesis H1: receipts reduce donor fatigue
   and make the next campaign easier; not demonstrated.
6. **Would someone pay?** Hypothesis: an FX spread plus a small platform fee on payouts
   (08 §04 "Commercial"); the association's alternative (5-8% remittance fees) sets the
   ceiling. Untested.
7. **Does the chain create value?** Yes, materially: transparency to dozens of
   contributors without an operator holding funds, escrow no single treasurer controls,
   and M-of-N approval enforced by code rather than by trust (04-P32 "What blockchain
   changes"). The tests show each of these properties holding against a hostile
   organiser, cousin or admin key.
8. **Why this chain?** The cash legs: anchors and MoneyGram cash-out exist on Stellar
   for the currencies these users need, and fees do not eat a $5 gift. Arbitrum has
   neither the fiat rail for this user nor gift-sized fees (08 §04).
