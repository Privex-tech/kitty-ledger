import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { centsToStroops } from "../src/amounts.js";
import { parseContributorsCsv } from "../src/csv.js";
import {
  accountLink,
  contractLink,
  displayName,
  shortAddress,
  snapshotFromJson,
  snapshotToJson,
  summarise,
  toExportCsv,
  toExportJson,
  txLink,
  type LedgerSnapshot,
} from "../src/export.js";
import { sha256Hex } from "../src/hash.js";

const SEED = new URL("../../../data/seed/contributors.csv", import.meta.url);
const BILLERS = new URL("../../../data/seed/billers.json", import.meta.url);
const CAMPAIGN = new URL("../../../data/seed/campaign.json", import.meta.url);
const NAMES = new URL("../../web/names.json", import.meta.url);

const CONTRACT = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
const T0 = 1_758_672_000;

/** The medical campaign after both payouts, built from the seed files. */
function medicalSnapshot(): LedgerSnapshot {
  const rows = parseContributorsCsv(readFileSync(SEED, "utf8")).rows;
  const billers = JSON.parse(readFileSync(BILLERS, "utf8")).billers as Array<{ id: number; address: string }>;
  const seed = JSON.parse(readFileSync(CAMPAIGN, "utf8")) as { committee: Array<{ address: string }>; threshold: number };
  const raised = rows.reduce((a, r) => a + r.amount, 0n);
  const committee = seed.committee.map((m) => m.address);
  return {
    network: "testnet",
    contractId: CONTRACT,
    campaignId: 0,
    fetchedAt: "2026-09-24T12:00:00.000Z",
    campaign: {
      organizer: "GAORGANIZERXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      token: "CTOKENXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      goal: centsToStroops(300_000),
      raised,
      deadline: T0 + 30 * 86_400,
      committee,
      threshold: seed.threshold,
      registry: "CREGISTRYXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      state: "Closed",
      title_hash: sha256Hex("Mama Njeri surgery fund - Nakuru chapter"),
      paid_out: raised,
      refunded: 0n,
      contribution_count: rows.length,
      payout_count: 2,
      created_at: T0,
    },
    contributions: rows.map((r, i) => ({
      index: i,
      from: r.address,
      amount: r.amount,
      memo_hash: r.memoHash,
      ledger_time: T0 + i * 6 * 3600,
    })),
    payouts: [
      {
        id: 0,
        biller_id: 0,
        amount: centsToStroops(275_000),
        purpose_hash: sha256Hex("KNH admission deposit, invoice KNH-2211"),
        approvals: [committee[0], committee[1], committee[3]],
        executed: true,
        proposer: committee[0],
        paid_to: billers[0].address,
        executed_at: T0 + 12 * 86_400,
      },
      {
        id: 1,
        biller_id: 3,
        amount: centsToStroops(56_182),
        purpose_hash: sha256Hex("Goodlife Pharmacy post-op medication, receipt GLP-14-0093"),
        approvals: [committee[2], committee[4], committee[1]],
        executed: true,
        proposer: committee[2],
        paid_to: billers[3].address,
        executed_at: T0 + 20 * 86_400,
      },
    ],
  };
}

test("summary reconciles receipts to paid_out and the ledger to raised", () => {
  const s = summarise(medicalSnapshot());
  assert.equal(s.goal, "3000.00");
  assert.equal(s.raised, "3311.82");
  assert.equal(s.paid_out, "3311.82");
  assert.equal(s.available, "0.00");
  assert.equal(s.percent_of_goal, 110.39);
  assert.equal(s.contributors, 40);
  assert.equal(s.contributions, 40);
  assert.equal(s.payouts_proposed, 2);
  assert.equal(s.payouts_executed, 2);
  assert.equal(s.receipts_total, "3311.82");
  assert.equal(s.reconciles, true);
});

test("summary flags a ledger that does not add up", () => {
  const snap = medicalSnapshot();
  snap.payouts[1].executed = false;
  snap.payouts[1].paid_to = null;
  assert.equal(summarise(snap).reconciles, false, "receipts no longer match paid_out");

  const snap2 = medicalSnapshot();
  snap2.contributions[0].amount += 1n;
  assert.equal(summarise(snap2).reconciles, false, "ledger no longer matches raised");

  const partial = medicalSnapshot();
  partial.contributions = partial.contributions.slice(0, 10);
  assert.equal(summarise(partial).reconciles, true, "a partial page is not a mismatch");
});

test("JSON export carries names, amounts as decimals and verify links", () => {
  const names = JSON.parse(readFileSync(NAMES, "utf8")) as Record<string, string>;
  const snap = medicalSnapshot();
  const doc = JSON.parse(toExportJson(snap, names));
  assert.equal(doc.format, "kitty-ledger-export");
  assert.equal(doc.version, 1);
  assert.equal(doc.verify_url, `https://stellar.expert/explorer/testnet/contract/${CONTRACT}`);
  assert.equal(doc.campaign.state, "Closed");
  assert.equal(doc.campaign.goal, "3000.00");
  assert.equal(doc.campaign.deadline, "2025-10-24T00:00:00.000Z");
  assert.equal(doc.campaign.committee.length, 5);
  assert.equal(doc.campaign.committee[0].name, "Rev. Margaret Wambui");
  assert.equal(doc.contributions.length, 40);
  assert.equal(doc.contributions[0].name, "Wanjiru Kamau");
  assert.equal(doc.contributions[0].amount, "50.00");
  assert.equal(doc.contributions[3].amount, "500.00");
  assert.match(doc.contributions[0].verify_url, /^https:\/\/stellar\.expert\/explorer\/testnet\/account\/G/);
  assert.equal(doc.payouts[0].approvals_count, 3);
  assert.equal(doc.payouts[0].threshold, 3);
  assert.equal(doc.payouts[0].approvals[0].name, "Rev. Margaret Wambui");
  assert.equal(doc.payouts[0].receipt.executed_at, "2025-10-06T00:00:00.000Z");
  assert.equal(doc.payouts[0].receipt.paid_to, snap.payouts[0].paid_to);
  assert.equal(doc.payouts[1].amount, "561.82");
  assert.equal(doc.summary.reconciles, true);

  // Without a names map addresses are shortened, never invented.
  const anon = JSON.parse(toExportJson(snap));
  assert.match(anon.contributions[0].name, /^G[A-Z2-7]{3}…[A-Z2-7]{4}$/);
});

test("CSV export lists every contribution then every receipt, quoting as needed", () => {
  const snap = medicalSnapshot();
  const names = { [snap.contributions[0].from]: "Kamau, Wanjiru" };
  const lines = toExportCsv(snap, names).trimEnd().split("\n");
  assert.equal(lines[0], "section,index,time,address,name,amount,hash,detail");
  assert.equal(lines.length, 1 + 40 + 2);
  assert.match(lines[1], /^contribution,0,2025-09-24T00:00:00\.000Z,G[A-Z2-7]{55},"Kamau, Wanjiru",50\.00,[0-9a-f]{64},https:\/\/stellar\.expert/);
  assert.match(lines[41], /^receipt,0,.*,2750\.00,[0-9a-f]{64},3\/3 approvals$/);
  assert.match(lines[42], /^receipt,1,.*,561\.82,/);

  snap.payouts[1].executed = false;
  snap.payouts[1].paid_to = null;
  snap.payouts[1].approvals = [snap.campaign.committee[2]];
  const pending = toExportCsv(snap).trimEnd().split("\n");
  assert.match(pending[42], /^proposal,1,,,biller #3,561\.82,[0-9a-f]{64},1\/3 approvals$/);
});

test("snapshot JSON round-trips bigints exactly", () => {
  const snap = medicalSnapshot();
  const back = snapshotFromJson(snapshotToJson(snap));
  assert.deepEqual(back, snap);
  assert.throws(() => snapshotFromJson('{"hello":"world"}'), /not a kitty-ledger snapshot/);
});

test("stellar.expert link pattern", () => {
  assert.equal(contractLink("testnet", CONTRACT), `https://stellar.expert/explorer/testnet/contract/${CONTRACT}`);
  assert.equal(contractLink("public", CONTRACT), `https://stellar.expert/explorer/public/contract/${CONTRACT}`);
  assert.equal(contractLink("local", CONTRACT), `https://stellar.expert/explorer/testnet/contract/${CONTRACT}`);
  assert.equal(accountLink("testnet", "GABC"), "https://stellar.expert/explorer/testnet/account/GABC");
  assert.equal(accountLink("testnet", CONTRACT), `https://stellar.expert/explorer/testnet/contract/${CONTRACT}`);
  assert.equal(txLink("testnet", "abc"), "https://stellar.expert/explorer/testnet/tx/abc");
  assert.equal(shortAddress("GAXG7A5PIURLIMXKMWI2BCPRBWVNGG2ILSKJIT3Z3IX52PBWJQ3FCXK7"), "GAXG…CXK7");
  assert.equal(displayName({ GABC: "Bob" }, "GABC"), "Bob");
  assert.equal(displayName(undefined, "short"), "short");
});
