/**
 * The export format: a self-contained JSON snapshot of one campaign (and a CSV of its
 * ledger) that an organiser can post in the WhatsApp group or hand to an auditor. The
 * same JSON can be pasted into web/ledger.html to render the page without an RPC.
 */
import { formatUnits, sumStroops } from "./amounts.js";

export const EXPORT_FORMAT_VERSION = 1;

export type NetworkName = "testnet" | "public" | "futurenet" | "local";

export interface CampaignView {
  organizer: string;
  token: string;
  goal: bigint;
  raised: bigint;
  deadline: number;
  committee: string[];
  threshold: number;
  registry: string;
  state: "Open" | "Funded" | "Refunding" | "Closed";
  title_hash: string;
  paid_out: bigint;
  refunded: bigint;
  contribution_count: number;
  payout_count: number;
  created_at: number;
}

export interface ContributionView {
  index: number;
  from: string;
  amount: bigint;
  memo_hash: string;
  ledger_time: number;
}

export interface PayoutView {
  id: number;
  biller_id: number;
  amount: bigint;
  purpose_hash: string;
  approvals: string[];
  executed: boolean;
  proposer: string;
  paid_to: string | null;
  executed_at: number;
}

export interface LedgerSnapshot {
  network: NetworkName;
  contractId: string;
  campaignId: number;
  fetchedAt: string;
  campaign: CampaignView;
  contributions: ContributionView[];
  payouts: PayoutView[];
}

export type DisplayNames = Record<string, string>;

/** stellar.expert link pattern used everywhere "verify on Stellar" appears. */
export function explorerBase(network: NetworkName): string {
  return `https://stellar.expert/explorer/${network === "local" ? "testnet" : network}`;
}

export function contractLink(network: NetworkName, contractId: string): string {
  return `${explorerBase(network)}/contract/${contractId}`;
}

export function accountLink(network: NetworkName, address: string): string {
  return address.startsWith("C")
    ? `${explorerBase(network)}/contract/${address}`
    : `${explorerBase(network)}/account/${address}`;
}

export function txLink(network: NetworkName, hash: string): string {
  return `${explorerBase(network)}/tx/${hash}`;
}

export function shortAddress(address: string): string {
  return address.length > 12 ? `${address.slice(0, 4)}…${address.slice(-4)}` : address;
}

export function displayName(names: DisplayNames | undefined, address: string): string {
  return names?.[address] ?? shortAddress(address);
}

export function isoTime(unixSeconds: number): string {
  return unixSeconds ? new Date(unixSeconds * 1000).toISOString() : "";
}

export interface ExportSummary {
  goal: string;
  raised: string;
  paid_out: string;
  refunded: string;
  available: string;
  percent_of_goal: number;
  contributors: number;
  contributions: number;
  payouts_proposed: number;
  payouts_executed: number;
  receipts_total: string;
  reconciles: boolean;
}

/**
 * Summarises a snapshot into a set of key metrics and a reconciliation flag.
 */
export function summarise(s: LedgerSnapshot): ExportSummary {
  const c = s.campaign;
  const available = c.raised - c.paid_out - c.refunded;
  const executed = s.payouts.filter((p) => p.executed);
  const receipts = sumStroops(executed.map((p) => p.amount));
  const ledgerTotal = sumStroops(s.contributions.map((x) => x.amount));
  const uniqueContributors = new Set(s.contributions.map((x) => x.from)).size;
  return {
    goal: formatUnits(c.goal),
    raised: formatUnits(c.raised),
    paid_out: formatUnits(c.paid_out),
    refunded: formatUnits(c.refunded),
    available: formatUnits(available),
    percent_of_goal: c.goal > 0n ? Number((c.raised * 10000n) / c.goal) / 100 : 0,
    contributors: uniqueContributors,
    contributions: s.contributions.length,
    payouts_proposed: s.payouts.length,
    payouts_executed: executed.length,
    receipts_total: formatUnits(receipts),
    // Every executed payout must add up to paid_out, and the ledger must add up to raised
    // when the export holds the whole ledger.
    reconciles:
      receipts === c.paid_out &&
      (s.contributions.length !== c.contribution_count || ledgerTotal === c.raised),
  };
}

/** JSON export. bigint -> decimal string of whole units; addresses annotated with names and links. */
export function toExportJson(s: LedgerSnapshot, names?: DisplayNames): string {
  const c = s.campaign;
  const doc = {
    format: "kitty-ledger-export",
    version: EXPORT_FORMAT_VERSION,
    network: s.network,
    contract_id: s.contractId,
    campaign_id: s.campaignId,
    fetched_at: s.fetchedAt,
    verify_url: contractLink(s.network, s.contractId),
    summary: summarise(s),
    campaign: {
      state: c.state,
      organizer: c.organizer,
      token: c.token,
      registry: c.registry,
      goal: formatUnits(c.goal),
      raised: formatUnits(c.raised),
      paid_out: formatUnits(c.paid_out),
      refunded: formatUnits(c.refunded),
      deadline: isoTime(c.deadline),
      created_at: isoTime(c.created_at),
      committee: c.committee.map((a) => ({ address: a, name: displayName(names, a) })),
      threshold: c.threshold,
      title_hash: c.title_hash,
    },
    contributions: s.contributions.map((x) => ({
      index: x.index,
      time: isoTime(x.ledger_time),
      from: x.from,
      name: displayName(names, x.from),
      amount: formatUnits(x.amount),
      memo_hash: x.memo_hash,
      verify_url: accountLink(s.network, x.from),
    })),
    payouts: s.payouts.map((p) => ({
      id: p.id,
      biller_id: p.biller_id,
      amount: formatUnits(p.amount),
      purpose_hash: p.purpose_hash,
      proposer: p.proposer,
      approvals: p.approvals.map((a) => ({ address: a, name: displayName(names, a) })),
      approvals_count: p.approvals.length,
      threshold: c.threshold,
      executed: p.executed,
      receipt: p.executed
        ? {
            paid_to: p.paid_to,
            executed_at: isoTime(p.executed_at),
            verify_url: p.paid_to ? accountLink(s.network, p.paid_to) : null,
          }
        : null,
    })),
  };
  return JSON.stringify(doc, null, 2) + "\n";
}

function csvCell(v: string | number | boolean | null | undefined): string {
  const s = v === null || v === undefined ? "" : String(v);
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** CSV export of the contribution ledger followed by the payout receipts. */
export function toExportCsv(s: LedgerSnapshot, names?: DisplayNames): string {
  const lines: string[] = [];
  lines.push("section,index,time,address,name,amount,hash,detail");
  for (const x of s.contributions) {
    lines.push(
      [
        "contribution",
        x.index,
        isoTime(x.ledger_time),
        x.from,
        displayName(names, x.from),
        formatUnits(x.amount),
        x.memo_hash,
        accountLink(s.network, x.from),
      ]
        .map(csvCell)
        .join(","),
    );
  }
  for (const p of s.payouts) {
    lines.push(
      [
        p.executed ? "receipt" : "proposal",
        p.id,
        p.executed ? isoTime(p.executed_at) : "",
        p.paid_to ?? "",
        `biller #${p.biller_id}`,
        formatUnits(p.amount),
        p.purpose_hash,
        `${p.approvals.length}/${s.campaign.threshold} approvals`,
      ]
        .map(csvCell)
        .join(","),
    );
  }
  return lines.join("\n") + "\n";
}

/** Parse a JSON export (or the raw snapshot form) back into a LedgerSnapshot. */
export function snapshotFromJson(text: string): LedgerSnapshot {
  const doc = JSON.parse(text);
  if (doc && doc.contractId && doc.campaign && Array.isArray(doc.contributions)) {
    return reviveSnapshot(doc);
  }
  throw new Error("not a kitty-ledger snapshot");
}

function reviveSnapshot(doc: any): LedgerSnapshot {
  const big = (v: unknown) => BigInt(v as string | number | bigint);
  return {
    network: doc.network,
    contractId: doc.contractId,
    campaignId: Number(doc.campaignId),
    fetchedAt: doc.fetchedAt,
    campaign: {
      ...doc.campaign,
      goal: big(doc.campaign.goal),
      raised: big(doc.campaign.raised),
      paid_out: big(doc.campaign.paid_out),
      refunded: big(doc.campaign.refunded),
    },
    contributions: doc.contributions.map((x: any) => ({ ...x, amount: big(x.amount) })),
    payouts: doc.payouts.map((p: any) => ({ ...p, amount: big(p.amount) })),
  };
}

/** Serialise a snapshot with bigints as strings (round-trips through snapshotFromJson). */
export function snapshotToJson(s: LedgerSnapshot): string {
  return JSON.stringify(s, (_k, v) => (typeof v === "bigint" ? v.toString() : v), 2) + "\n";
}
