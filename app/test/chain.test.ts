import { test } from "node:test";
import assert from "node:assert/strict";
import { Account, Keypair, Networks, StrKey, TransactionBuilder, scValToNative, xdr } from "@stellar/stellar-sdk";
import {
  NULL_ACCOUNT,
  arg,
  buildTransaction,
  campaignCall,
  decodeInvocation,
  networkName,
  registryCall,
  toCampaignView,
  toContributionView,
  toPayoutView,
} from "../src/chain.js";
import { centsToStroops } from "../src/amounts.js";
import { noteHash } from "../src/hash.js";

const CONTRACT = StrKey.encodeContract(Buffer.alloc(32, 1));
const REGISTRY = StrKey.encodeContract(Buffer.alloc(32, 2));
const TOKEN = StrKey.encodeContract(Buffer.alloc(32, 3));

test("create builds a transaction whose XDR decodes back to the same call", () => {
  const organizer = Keypair.random().publicKey();
  const committee = [Keypair.random().publicKey(), Keypair.random().publicKey(), Keypair.random().publicKey()];
  const inv = campaignCall.create(CONTRACT, {
    organizer,
    token: TOKEN,
    goal: centsToStroops(300_000),
    deadline: 1_761_263_999,
    committee,
    threshold: 2,
    registry: REGISTRY,
    titleHash: noteHash("Mama Njeri surgery fund"),
  });
  const tx = buildTransaction(new Account(organizer, "41"), inv, Networks.TESTNET);
  assert.equal(tx.operations.length, 1);
  assert.equal(tx.sequence, "42");

  const back = TransactionBuilder.fromXDR(tx.toXDR(), Networks.TESTNET);
  const decoded = decodeInvocation(back as any);
  assert.equal(decoded.contractId, CONTRACT);
  assert.equal(decoded.method, "create");
  assert.deepEqual(
    decoded.args.map((a) => a.type),
    ["scvAddress", "scvAddress", "scvI128", "scvU64", "scvVec", "scvU32", "scvAddress", "scvBytes"],
  );
  assert.equal(scValToNative(decoded.args[0]), organizer);
  assert.equal(scValToNative(decoded.args[2]), 30_000_000_000n);
  assert.equal(scValToNative(decoded.args[3]), 1_761_263_999n);
  assert.deepEqual(scValToNative(decoded.args[4]), committee);
  assert.equal(scValToNative(decoded.args[5]), 2);
  assert.equal(Buffer.from(scValToNative(decoded.args[7])).toString("hex"), noteHash("Mama Njeri surgery fund").toString("hex"));
});

test("every campaign and registry call encodes the argument types the contracts expect", () => {
  const who = Keypair.random().publicKey();
  const kinds = (i: { args: xdr.ScVal[] }) => i.args.map((a) => a.type);
  assert.deepEqual(kinds(campaignCall.contribute(CONTRACT, 0, who, 1n, Buffer.alloc(32))), ["scvU32", "scvAddress", "scvI128", "scvBytes"]);
  assert.deepEqual(kinds(campaignCall.proposePayout(CONTRACT, 0, who, 3, 1n, Buffer.alloc(32))), ["scvU32", "scvAddress", "scvU32", "scvI128", "scvBytes"]);
  assert.deepEqual(kinds(campaignCall.approvePayout(CONTRACT, 0, who, 1)), ["scvU32", "scvAddress", "scvU32"]);
  assert.deepEqual(kinds(campaignCall.refund(CONTRACT, 0)), ["scvU32"]);
  assert.deepEqual(kinds(campaignCall.claimRefund(CONTRACT, 0, who)), ["scvU32", "scvAddress"]);
  assert.deepEqual(kinds(campaignCall.close(CONTRACT, 0)), ["scvU32"]);
  assert.deepEqual(kinds(campaignCall.campaign(CONTRACT, 0)), ["scvU32"]);
  assert.deepEqual(kinds(campaignCall.campaignCount(CONTRACT)), []);
  assert.deepEqual(kinds(campaignCall.contributions(CONTRACT, 0, 0, 100)), ["scvU32", "scvU32", "scvU32"]);
  assert.deepEqual(kinds(campaignCall.payouts(CONTRACT, 0)), ["scvU32"]);
  assert.deepEqual(kinds(registryCall.init(REGISTRY, who)), ["scvAddress"]);
  assert.deepEqual(kinds(registryCall.registerBiller(REGISTRY, Buffer.alloc(32), "hospital", who)), ["scvBytes", "scvSymbol", "scvAddress"]);
  assert.deepEqual(kinds(registryCall.setVerified(REGISTRY, 0, true)), ["scvU32", "scvBool"]);
  assert.deepEqual(kinds(registryCall.isPayable(REGISTRY, 4)), ["scvU32"]);
  assert.equal(registryCall.biller(REGISTRY, 2).method, "biller");
  assert.throws(() => arg.bytes32(Buffer.alloc(31)), /expected 32 bytes/);
});

test("read-only simulations are built from the null account", () => {
  assert.equal(NULL_ACCOUNT, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF");
  const tx = buildTransaction(new Account(NULL_ACCOUNT, "0"), campaignCall.campaign(CONTRACT, 0), Networks.TESTNET);
  assert.equal(tx.source, NULL_ACCOUNT);
  assert.equal(tx.fee, "100");
});

test("network names from passphrases", () => {
  assert.equal(networkName(Networks.TESTNET), "testnet");
  assert.equal(networkName(Networks.PUBLIC), "public");
  assert.equal(networkName(Networks.FUTURENET), "futurenet");
  assert.equal(networkName("Standalone Network ; February 2017"), "local");
});

test("decodes the shapes scValToNative produces for the contract types", () => {
  const a = Keypair.random().publicKey();
  const b = Keypair.random().publicKey();
  const campaign = toCampaignView({
    organizer: a,
    token: TOKEN,
    goal: 30_000_000_000n,
    raised: 33_118_200_000n,
    deadline: 1_761_263_999n,
    committee: [a, b],
    threshold: 2,
    registry: REGISTRY,
    state: ["Funded"],
    title_hash: Buffer.alloc(32, 9),
    paid_out: 0n,
    refunded: 0n,
    contribution_count: 40,
    payout_count: 0,
    created_at: 1_758_672_000n,
  });
  assert.equal(campaign.state, "Funded");
  assert.equal(campaign.deadline, 1_761_263_999);
  assert.equal(campaign.raised, 33_118_200_000n);
  assert.equal(campaign.title_hash, "09".repeat(32));
  assert.deepEqual(campaign.committee, [a, b]);

  const c = toContributionView({ from: a, amount: 523_700_000n, memo_hash: Buffer.alloc(32, 1), ledger_time: 5n }, 7);
  assert.deepEqual(c, { index: 7, from: a, amount: 523_700_000n, memo_hash: "01".repeat(32), ledger_time: 5 });

  const pending = toPayoutView({ id: 0, biller_id: 3, amount: 1n, purpose_hash: Buffer.alloc(32), approvals: [a], executed: false, proposer: a, paid_to: null, executed_at: 0n });
  assert.equal(pending.paid_to, null);
  assert.equal(pending.executed_at, 0);
  const done = toPayoutView({ id: 1, biller_id: 0, amount: 27_500_000_000n, purpose_hash: Buffer.alloc(32), approvals: [a, b], executed: true, proposer: a, paid_to: b, executed_at: 99n });
  assert.equal(done.paid_to, b);
  assert.equal(done.amount, 27_500_000_000n);
  assert.deepEqual(done.approvals, [a, b]);
});
