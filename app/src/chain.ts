/**
 * Transaction building, read-only simulation and submission for the campaign and
 * registry contracts. Building is fully offline (tested without a network); only
 * `simulateView`, `submit` and `fetchSnapshot` talk to a Soroban RPC.
 */
import {
  Account,
  Address,
  BASE_FEE,
  Contract,
  Keypair,
  Networks,
  type Operation,
  Transaction,
  TransactionBuilder,
  contract as contractNs,
  nativeToScVal,
  rpc,
  scValToNative,
  xdr,
} from "@stellar/stellar-sdk";
import type { CampaignView, ContributionView, LedgerSnapshot, NetworkName, PayoutView } from "./export.js";

export interface ChainConfig {
  rpcUrl: string;
  networkPassphrase: string;
}

/** Well-known empty account the SDK uses as the source of read-only simulations. */
export const NULL_ACCOUNT: string = contractNs.NULL_ACCOUNT;

export function networkName(passphrase: string): NetworkName {
  if (passphrase === Networks.PUBLIC) return "public";
  if (passphrase === Networks.TESTNET) return "testnet";
  if (passphrase === Networks.FUTURENET) return "futurenet";
  return "local";
}

// ------------------------------------------------------------------ argument encoders

export const arg = {
  u32: (n: number): xdr.ScVal => nativeToScVal(n, { type: "u32" }),
  u64: (n: number | bigint): xdr.ScVal => nativeToScVal(n, { type: "u64" }),
  i128: (n: bigint): xdr.ScVal => nativeToScVal(n, { type: "i128" }),
  bool: (b: boolean): xdr.ScVal => xdr.ScVal.scvBool(b),
  symbol: (s: string): xdr.ScVal => xdr.ScVal.scvSymbol(s),
  address: (a: string): xdr.ScVal => new Address(a).toScVal(),
  bytes32: (b: Buffer): xdr.ScVal => {
    if (b.length !== 32) throw new Error(`expected 32 bytes, got ${b.length}`);
    return xdr.ScVal.scvBytes(b);
  },
  addresses: (as: string[]): xdr.ScVal => xdr.ScVal.scvVec(as.map((a) => new Address(a).toScVal())),
};

// ------------------------------------------------------------------ invocation building

export interface Invocation {
  contractId: string;
  method: string;
  args: xdr.ScVal[];
}

export function invocationOp(inv: Invocation): xdr.Operation {
  return new Contract(inv.contractId).call(inv.method, ...inv.args);
}

export function buildTransaction(
  source: Account,
  inv: Invocation,
  networkPassphrase: string,
  fee: string = BASE_FEE,
  timeoutSeconds = 120,
): Transaction {
  return new TransactionBuilder(source, { fee, networkPassphrase })
    .addOperation(invocationOp(inv))
    .setTimeout(timeoutSeconds)
    .build();
}

/** 
 * Decode the single invoke-host-function operation of a transaction (used by tests and `--dry-run`).
 * Throws an error if the transaction does not contain exactly one invocation.
 */
export function decodeInvocation(tx: Transaction): Invocation {
  const op = tx.operations[0] as Operation.InvokeHostFunction | undefined;
  if (!op || op.type !== "invokeHostFunction") throw new Error("not an invoke host function op");
  const fn = op.func;
  if (fn.type !== "hostFunctionTypeInvokeContract") throw new Error("not a contract invocation");
  const inv = fn.invokeContract;
  return {
    contractId: Address.fromScAddress(inv.contractAddress).toString(),
    method: String(inv.functionName),
    args: [...inv.args],
  };
}

// ------------------------------------------------------------------ campaign calls

export interface CreateParams {
  organizer: string;
  token: string;
  goal: bigint;
  deadline: number;
  committee: string[];
  threshold: number;
  registry: string;
  titleHash: Buffer;
}

export const campaignCall = {
  create: (contractId: string, p: CreateParams): Invocation => ({
    contractId,
    method: "create",
    args: [
      arg.address(p.organizer),
      arg.address(p.token),
      arg.i128(p.goal),
      arg.u64(p.deadline),
      arg.addresses(p.committee),
      arg.u32(p.threshold),
      arg.address(p.registry),
      arg.bytes32(p.titleHash),
    ],
  }),
  contribute: (contractId: string, id: number, from: string, amount: bigint, memoHash: Buffer): Invocation => ({
    contractId,
    method: "contribute",
    args: [arg.u32(id), arg.address(from), arg.i128(amount), arg.bytes32(memoHash)],
  }),
  proposePayout: (
    contractId: string,
    id: number,
    proposer: string,
    billerId: number,
    amount: bigint,
    purposeHash: Buffer,
  ): Invocation => ({
    contractId,
    method: "propose_payout",
    args: [arg.u32(id), arg.address(proposer), arg.u32(billerId), arg.i128(amount), arg.bytes32(purposeHash)],
  }),
  approvePayout: (contractId: string, id: number, approver: string, payoutId: number): Invocation => ({
    contractId,
    method: "approve_payout",
    args: [arg.u32(id), arg.address(approver), arg.u32(payoutId)],
  }),
  refund: (contractId: string, id: number): Invocation => ({ contractId, method: "refund", args: [arg.u32(id)] }),
  claimRefund: (contractId: string, id: number, contributor: string): Invocation => ({
    contractId,
    method: "claim_refund",
    args: [arg.u32(id), arg.address(contributor)],
  }),
  close: (contractId: string, id: number): Invocation => ({ contractId, method: "close", args: [arg.u32(id)] }),
  campaign: (contractId: string, id: number): Invocation => ({ contractId, method: "campaign", args: [arg.u32(id)] }),
  campaignCount: (contractId: string): Invocation => ({ contractId, method: "campaign_count", args: [] }),
  contributions: (contractId: string, id: number, start: number, limit: number): Invocation => ({
    contractId,
    method: "contributions",
    args: [arg.u32(id), arg.u32(start), arg.u32(limit)],
  }),
  payouts: (contractId: string, id: number): Invocation => ({ contractId, method: "payouts", args: [arg.u32(id)] }),
};

export const registryCall = {
  init: (contractId: string, admin: string): Invocation => ({ contractId, method: "init", args: [arg.address(admin)] }),
  registerBiller: (contractId: string, nameHash: Buffer, category: string, address: string): Invocation => ({
    contractId,
    method: "register_biller",
    args: [arg.bytes32(nameHash), arg.symbol(category), arg.address(address)],
  }),
  setVerified: (contractId: string, id: number, verified: boolean): Invocation => ({
    contractId,
    method: "set_verified",
    args: [arg.u32(id), arg.bool(verified)],
  }),
  biller: (contractId: string, id: number): Invocation => ({ contractId, method: "biller", args: [arg.u32(id)] }),
  isPayable: (contractId: string, id: number): Invocation => ({ contractId, method: "is_payable", args: [arg.u32(id)] }),
};

// ------------------------------------------------------------------ decoding contract values

function hex(v: unknown): string {
  return Buffer.isBuffer(v) ? v.toString("hex") : String(v ?? "");
}

function num(v: unknown): number {
  return Number(v ?? 0);
}

function big(v: unknown): bigint {
  return BigInt((v as bigint | number | string) ?? 0);
}

/** Unit-variant enums decode as a one-element array holding the variant name. */
function enumName(v: unknown): string {
  return Array.isArray(v) ? String(v[0]) : String(v);
}

export function toCampaignView(native: any): CampaignView {
  return {
    organizer: String(native.organizer),
    token: String(native.token),
    goal: big(native.goal),
    raised: big(native.raised),
    deadline: num(native.deadline),
    committee: (native.committee as unknown[]).map(String),
    threshold: num(native.threshold),
    registry: String(native.registry),
    state: enumName(native.state) as CampaignView["state"],
    title_hash: hex(native.title_hash),
    paid_out: big(native.paid_out),
    refunded: big(native.refunded),
    contribution_count: num(native.contribution_count),
    payout_count: num(native.payout_count),
    created_at: num(native.created_at),
  };
}

export function toContributionView(native: any, index: number): ContributionView {
  return {
    index,
    from: String(native.from),
    amount: big(native.amount),
    memo_hash: hex(native.memo_hash),
    ledger_time: num(native.ledger_time),
  };
}

export function toPayoutView(native: any): PayoutView {
  return {
    id: num(native.id),
    biller_id: num(native.biller_id),
    amount: big(native.amount),
    purpose_hash: hex(native.purpose_hash),
    approvals: (native.approvals as unknown[]).map(String),
    executed: Boolean(native.executed),
    proposer: String(native.proposer),
    paid_to: native.paid_to === null || native.paid_to === undefined ? null : String(native.paid_to),
    executed_at: num(native.executed_at),
  };
}

// ------------------------------------------------------------------ network calls

export function server(cfg: ChainConfig): rpc.Server {
  return new rpc.Server(cfg.rpcUrl, { allowHttp: cfg.rpcUrl.startsWith("http://") });
}

/** Simulate a read-only call from the null account and decode its return value. */
export async function simulateView(cfg: ChainConfig, inv: Invocation): Promise<any> {
  const tx = buildTransaction(new Account(NULL_ACCOUNT, "0"), inv, cfg.networkPassphrase);
  const sim = await server(cfg).simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) {
    throw new Error(`simulation of ${inv.method} failed: ${sim.error}`);
  }
  if (!rpc.Api.isSimulationSuccess(sim) || !sim.result) {
    throw new Error(`simulation of ${inv.method} returned no result`);
  }
  return scValToNative(sim.result.retval);
}

export interface SubmitResult {
  hash: string;
  status: string;
  returnValue?: unknown;
}

/** Prepare (simulate + footprint + fee), sign with `signer`, send and poll to completion. */
export async function submit(cfg: ChainConfig, signer: Keypair, inv: Invocation): Promise<SubmitResult> {
  const srv = server(cfg);
  const source = await srv.getAccount(signer.publicKey());
  const tx = buildTransaction(source, inv, cfg.networkPassphrase);
  const prepared = await srv.prepareTransaction(tx);
  prepared.sign(signer);
  const sent = await srv.sendTransaction(prepared);
  if (sent.status === "ERROR") {
    throw new Error(`transaction rejected: ${JSON.stringify(sent.errorResult ?? sent)}`);
  }
  const final = await srv.pollTransaction(sent.hash, { attempts: 30, sleepStrategy: () => 1500 });
  if (final.status !== rpc.Api.GetTransactionStatus.SUCCESS) {
    throw new Error(`transaction ${sent.hash} ended with status ${final.status}`);
  }
  const returnValue = final.returnValue ? scValToNative(final.returnValue) : undefined;
  return { hash: sent.hash, status: final.status, returnValue };
}

/** Read one whole campaign (paging through the contribution ledger) into a snapshot. */
export async function fetchSnapshot(cfg: ChainConfig, contractId: string, campaignId: number): Promise<LedgerSnapshot> {
  const campaign = toCampaignView(await simulateView(cfg, campaignCall.campaign(contractId, campaignId)));
  const contributions: ContributionView[] = [];
  const PAGE = 100;
  for (let start = 0; start < campaign.contribution_count; start += PAGE) {
    const page = (await simulateView(cfg, campaignCall.contributions(contractId, campaignId, start, PAGE))) as any[];
    page.forEach((entry, i) => contributions.push(toContributionView(entry, start + i)));
  }
  const payouts = ((await simulateView(cfg, campaignCall.payouts(contractId, campaignId))) as any[]).map(toPayoutView);
  return {
    network: networkName(cfg.networkPassphrase),
    contractId,
    campaignId,
    fetchedAt: new Date().toISOString(),
    campaign,
    contributions,
    payouts,
  };
}
