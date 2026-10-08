import { test } from "node:test";
import assert from "node:assert/strict";
import {
  approvalsRemaining,
  approvePayout,
  MAX_PAYOUTS,
  proposePayout,
  validateCommittee,
  type CommitteeRules,
  type PayoutDraft,
} from "../src/approvals.js";
import { centsToStroops } from "../src/amounts.js";

const M = ["M0", "M1", "M2", "M3", "M4"];
const payable = (id: number) => id !== 4; // biller 4 is the unverified clinic

function rules(over: Partial<CommitteeRules> = {}): CommitteeRules {
  return {
    committee: M,
    threshold: 3,
    state: "Funded",
    available: centsToStroops(331_182),
    payoutCount: 0,
    ...over,
  };
}

test("committee validation mirrors create()", () => {
  assert.deepEqual(validateCommittee([], 1), { ok: false, error: "InvalidCommittee" });
  assert.deepEqual(validateCommittee(Array.from({ length: 8 }, (_, i) => `M${i}`), 3), { ok: false, error: "InvalidCommittee" });
  assert.deepEqual(validateCommittee(["A", "B", "A"], 2), { ok: false, error: "InvalidCommittee" });
  assert.deepEqual(validateCommittee(["A", "B"], 0), { ok: false, error: "InvalidThreshold" });
  assert.deepEqual(validateCommittee(["A", "B"], 3), { ok: false, error: "InvalidThreshold" });
  assert.deepEqual(validateCommittee(["A", "B"], 1.5), { ok: false, error: "InvalidThreshold" });
  assert.equal(validateCommittee(["A"], 1).ok, true);
  assert.equal(validateCommittee(Array.from({ length: 7 }, (_, i) => `M${i}`), 7).ok, true);
});

test("propose rejects the same things the contract rejects", () => {
  const amt = centsToStroops(275_000);
  assert.equal(proposePayout(rules({ state: "Open" }), "M0", 0, amt, "h", payable).ok, false);
  assert.deepEqual(proposePayout(rules({ state: "Open" }), "M0", 0, amt, "h", payable), { ok: false, error: "NotFunded" });
  assert.deepEqual(proposePayout(rules(), "cousin", 0, amt, "h", payable), { ok: false, error: "NotCommitteeMember" });
  assert.deepEqual(proposePayout(rules(), "M0", 0, 0n, "h", payable), { ok: false, error: "InvalidAmount" });
  assert.deepEqual(proposePayout(rules(), "M0", 0, -1n, "h", payable), { ok: false, error: "InvalidAmount" });
  assert.deepEqual(proposePayout(rules(), "M0", 0, centsToStroops(331_183), "h", payable), { ok: false, error: "InsufficientFunds" });
  assert.deepEqual(proposePayout(rules({ payoutCount: MAX_PAYOUTS }), "M0", 0, amt, "h", payable), { ok: false, error: "TooManyPayouts" });
  assert.deepEqual(proposePayout(rules(), "M0", 4, amt, "h", payable), { ok: false, error: "BillerNotPayable" });
});

test("proposer's approval counts first; threshold 1 executes immediately", () => {
  const r = proposePayout(rules(), "M2", 3, centsToStroops(56_182), "pharmacy", payable);
  assert.ok(r.ok);
  assert.deepEqual(r.value.payout.approvals, ["M2"]);
  assert.equal(r.value.executes, false);
  assert.equal(r.value.payout.executed, false);
  assert.equal(approvalsRemaining(rules(), r.value.payout), 2);

  const one = proposePayout(rules({ committee: ["Solo"], threshold: 1 }), "Solo", 0, 1n, "h", payable);
  assert.ok(one.ok);
  assert.equal(one.value.executes, true);
  assert.equal(one.value.payout.executed, true);
});

test("3-of-5 approval flow with the seeded hospital deposit", () => {
  const first = proposePayout(rules(), "M0", 0, centsToStroops(275_000), "KNH deposit", payable);
  assert.ok(first.ok);
  let payout: PayoutDraft = first.value.payout;

  assert.deepEqual(approvePayout(rules(), payout, "M0"), { ok: false, error: "AlreadyApproved" });
  assert.deepEqual(approvePayout(rules(), payout, "cousin"), { ok: false, error: "NotCommitteeMember" });

  const second = approvePayout(rules(), payout, "M1");
  assert.ok(second.ok);
  assert.equal(second.value.executes, false);
  payout = second.value.payout;
  assert.deepEqual(payout.approvals, ["M0", "M1"]);
  assert.deepEqual(approvePayout(rules(), payout, "M1"), { ok: false, error: "AlreadyApproved" });
  assert.equal(approvalsRemaining(rules(), payout), 1);

  const third = approvePayout(rules(), payout, "M3", payable);
  assert.ok(third.ok);
  assert.equal(third.value.executes, true);
  payout = third.value.payout;
  assert.equal(payout.executed, true);
  assert.deepEqual(payout.approvals, ["M0", "M1", "M3"]);
  assert.equal(approvalsRemaining(rules(), payout), 0);

  assert.deepEqual(approvePayout(rules(), payout, "M4"), { ok: false, error: "AlreadyExecuted" });
  assert.deepEqual(approvePayout(rules({ state: "Refunding" }), payout, "M4"), { ok: false, error: "NotFunded" });
});

test("execution-time checks: deactivated biller and funds spent by an earlier payout", () => {
  const draft = proposePayout(rules(), "M0", 1, centsToStroops(8_000), "school", payable);
  assert.ok(draft.ok);
  let payout = draft.value.payout;
  const two = approvePayout(rules(), payout, "M1");
  assert.ok(two.ok);
  payout = two.value.payout;

  assert.deepEqual(approvePayout(rules(), payout, "M2", () => false), { ok: false, error: "BillerNotPayable" });
  assert.deepEqual(approvePayout(rules({ available: centsToStroops(7_999) }), payout, "M2"), { ok: false, error: "InsufficientFunds" });
  // A non-final approval does not consult the biller or the balance.
  const r = approvePayout(rules({ threshold: 4, available: 0n }), payout, "M2", () => false);
  assert.ok(r.ok);
  assert.equal(r.value.executes, false);
});
