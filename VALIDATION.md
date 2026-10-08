# Validation

This file separates what was researched, what was observed while building, what the
team assumes, what is hypothesised, what is simulated, and what has actually been
validated (nothing yet). Sources are cited by section of the research dossier.

## Researched evidence

From `research/04-problems-consumer-creator-services.md` §P32 (community, diaspora and
mutual-aid fundraising with transparent disbursement) and `research/08-final-selection.md`
§04 (Kitty Ledger):

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

- Both contracts compile to wasm (`biller_registry.wasm` 10,194 bytes, `campaign.wasm`
  22,935 bytes) and pass 63 tests executed inside the Soroban host: 19 registry, 44
  campaign, including the two scenario tests and negative tests for every auth, state,
  deadline and threshold rule.
- The CLI and its 34 offline tests run without network access; the messy seed CSV yields
  exactly 40 valid contributors totalling $3,311.82 and rejects 2 rows with reasons.
- Testnet deployment was **not** attempted: soroban-testnet.stellar.org, Horizon and
  Friendbot were unreachable from the build environment (see `/TOOLCHAIN.md`).
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

None yet. No deployment, no pilot, no interview, no measured baseline.

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
2. **Did anyone outside the team use it?** No.
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
