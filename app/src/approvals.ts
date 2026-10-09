/**
 * Off-chain mirror of the campaign contract's payout rules. The CLI and the ledger page
 * use it to explain *why* a proposal or approval will be rejected before spending a fee,
 * and the tests pin the rules so a change on either side shows up.
 *
 * Source of truth: contracts/campaign/src/lib.rs (`propose_payout`, `approve_payout`).
 */

export type CampaignState = "Open" | "Funded" | "Refunding" | "Closed";

export interface CommitteeRules {
  committee: string[];
  threshold: number;
  state: CampaignState;
  /** raised - paid_out - refunded */
  available: bigint;
  payoutCount: number;
}

export interface PayoutDraft {
  id: number;
  billerId: number;
  amount: bigint;
  purposeHash: string;
  approvals: string[];
  executed: boolean;
  proposer: string;
}

export const MAX_COMMITTEE = 7;
export const MAX_PAYOUTS = 64;

export type RuleError =
  | "NotFunded"
  | "NotCommitteeMember"
  | "InvalidAmount"
  | "InsufficientFunds"
  | "TooManyPayouts"
  | "BillerNotPayable"
  | "AlreadyExecuted"
  | "AlreadyApproved"
  | "InvalidCommittee"
  | "InvalidThreshold";

export type RuleResult<T> = { ok: true; value: T } | { ok: false; error: RuleError };

export function validateCommittee(committee: string[], threshold: number): RuleResult<true> {
  if (committee.length === 0 || committee.length > MAX_COMMITTEE) {
    return { ok: false, error: "InvalidCommittee" };
  }
  if (new Set(committee).size !== committee.length) return { ok: false, error: "InvalidCommittee" };
  if (!Number.isInteger(threshold) || threshold < 1 || threshold > committee.length) {
    return { ok: false, error: "InvalidThreshold" };
  }
  return { ok: true, value: true };
}

export function isMember(rules: Pick<CommitteeRules, "committee">, who: string): boolean {
  return rules.committee.includes(who);
}

/** Mirrors `propose_payout`. The proposer's approval counts as the first approval. */
export function proposePayout(
  rules: CommitteeRules,
  proposer: string,
  billerId: number,
  amount: bigint,
  purposeHash: string,
  isPayable: (billerId: number) => boolean,
): RuleResult<{ payout: PayoutDraft; executes: boolean }> {
  if (rules.state !== "Funded") return { ok: false, error: "NotFunded" };
  if (!isMember(rules, proposer)) return { ok: false, error: "NotCommitteeMember" };
  if (amount <= 0n) return { ok: false, error: "InvalidAmount" };
  if (amount > rules.available) return { ok: false, error: "InsufficientFunds" };
  if (rules.payoutCount >= MAX_PAYOUTS) return { ok: false, error: "TooManyPayouts" };
  if (!isPayable(billerId)) return { ok: false, error: "BillerNotPayable" };
  const payout: PayoutDraft = {
    id: rules.payoutCount,
    billerId,
    amount,
    purposeHash,
    approvals: [proposer],
    executed: false,
    proposer,
  };
  const executes = payout.approvals.length >= rules.threshold;
  return { ok: true, value: { payout: { ...payout, executed: executes }, executes } };
}

/** Mirrors `approve_payout`: one approval per member, execution at the threshold. */
export function approvePayout(
  rules: CommitteeRules,
  payout: PayoutDraft,
  approver: string,
  isPayable: (billerId: number) => boolean = () => true,
): RuleResult<{ payout: PayoutDraft; executes: boolean }> {
  if (rules.state !== "Funded") return { ok: false, error: "NotFunded" };
  if (!isMember(rules, approver)) return { ok: false, error: "NotCommitteeMember" };
  if (payout.executed) return { ok: false, error: "AlreadyExecuted" };
  if (payout.approvals.includes(approver)) return { ok: false, error: "AlreadyApproved" };
  const approvals = [...payout.approvals, approver];
  const executes = approvals.length >= rules.threshold;
  if (executes) {
    if (!isPayable(payout.billerId)) return { ok: false, error: "BillerNotPayable" };
    if (payout.amount > rules.available) return { ok: false, error: "InsufficientFunds" };
  }
  return { ok: true, value: { payout: { ...payout, approvals, executed: executes }, executes } };
}

/** 
 * How many more approvals a pending payout needs. 
 * Returns 0 if already executed.
 */
export function approvalsRemaining(rules: Pick<CommitteeRules, "threshold">, payout: PayoutDraft): number {
  return payout.executed ? 0 : Math.max(0, rules.threshold - payout.approvals.length);
}
