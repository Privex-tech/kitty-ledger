#!/usr/bin/env node
/**
 * `kitty` command line. Every command builds a contract invocation; with an RPC in
 * reach it is signed with SECRET_KEY and submitted, otherwise (or with --dry-run) the
 * unsigned transaction XDR is printed so it can be signed and sent elsewhere.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { Account, Keypair, Networks } from "@stellar/stellar-sdk";
import { formatUnits, parseAmountToStroops, withThousands } from "./amounts.js";
import { approvalsRemaining } from "./approvals.js";
import {
  buildTransaction,
  campaignCall,
  fetchSnapshot,
  simulateView,
  submit,
  type ChainConfig,
  type Invocation,
} from "./chain.js";
import { parseArgs, parseCommittee, parseDeadline, usage, UsageError, type ParsedArgs } from "./cli-args.js";
import { parseContributorsCsv } from "./csv.js";
import {
  accountLink,
  contractLink,
  displayName,
  snapshotFromJson,
  snapshotToJson,
  summarise,
  toExportCsv,
  toExportJson,
  txLink,
  type DisplayNames,
  type LedgerSnapshot,
} from "./export.js";
import { noteHash } from "./hash.js";

interface Env {
  rpcUrl: string;
  networkPassphrase: string;
  campaignContractId: string;
  registryContractId: string;
  tokenContractId: string;
  secretKey: string;
  namesFile: string;
}

function readEnv(): Env {
  const e = process.env;
  return {
    rpcUrl: e.RPC_URL ?? "https://soroban-testnet.stellar.org",
    networkPassphrase: e.NETWORK_PASSPHRASE ?? Networks.TESTNET,
    campaignContractId: e.CAMPAIGN_CONTRACT_ID ?? "",
    registryContractId: e.REGISTRY_CONTRACT_ID ?? "",
    tokenContractId: e.TOKEN_CONTRACT_ID ?? "",
    secretKey: e.SECRET_KEY ?? "",
    namesFile: e.NAMES_FILE ?? "",
  };
}

function need(value: string, what: string): string {
  if (!value) throw new UsageError(`${what} is not set (see .env.example)`);
  return value;
}

function loadNames(path: string | undefined): DisplayNames | undefined {
  if (!path) return undefined;
  return JSON.parse(readFileSync(path, "utf8")) as DisplayNames;
}

function signer(env: Env): Keypair {
  return Keypair.fromSecret(need(env.secretKey, "SECRET_KEY"));
}

/** Submit if we can reach the RPC; otherwise print the unsigned XDR and exit 2. */
async function sendOrPrint(env: Env, args: ParsedArgs, inv: Invocation, kp: Keypair): Promise<void> {
  const cfg: ChainConfig = { rpcUrl: env.rpcUrl, networkPassphrase: env.networkPassphrase };
  if (args.flags.has("dry-run")) {
    const tx = buildTransaction(new Account(kp.publicKey(), "0"), inv, env.networkPassphrase);
    console.log(`dry run: ${inv.method} on ${inv.contractId}`);
    console.log(tx.toXDR());
    return;
  }
  try {
    const res = await submit(cfg, kp, inv);
    const out = { ok: true, method: inv.method, hash: res.hash, status: res.status, returnValue: res.returnValue };
    if (args.flags.has("json")) console.log(JSON.stringify(out, bigintReplacer, 2));
    else {
      console.log(`${inv.method}: ${res.status} (${res.hash})`);
      if (res.returnValue !== undefined) console.log(`returned: ${String(res.returnValue)}`);
      console.log(`verify: ${txLink(networkOf(env), res.hash)}`);
    }
  } catch (err) {
    const tx = buildTransaction(new Account(kp.publicKey(), "0"), inv, env.networkPassphrase);
    console.error(`could not submit ${inv.method} through ${env.rpcUrl}: ${(err as Error).message}`);
    console.error("unsigned transaction XDR (sequence number 0; re-sequence before signing):");
    console.log(tx.toXDR());
    process.exitCode = 2;
  }
}

function networkOf(env: Env) {
  return env.networkPassphrase === Networks.PUBLIC ? "public" : "testnet";
}

function bigintReplacer(_k: string, v: unknown) {
  return typeof v === "bigint" ? v.toString() : v;
}

async function loadSnapshot(env: Env, args: ParsedArgs): Promise<LedgerSnapshot> {
  if (args.options["from-snapshot"]) {
    return snapshotFromJson(readFileSync(args.options["from-snapshot"], "utf8"));
  }
  const cfg: ChainConfig = { rpcUrl: env.rpcUrl, networkPassphrase: env.networkPassphrase };
  return fetchSnapshot(cfg, need(env.campaignContractId, "CAMPAIGN_CONTRACT_ID"), Number(args.options.campaign));
}

function printStatus(s: LedgerSnapshot, names?: DisplayNames): void {
  const c = s.campaign;
  const sum = summarise(s);
  console.log(`Campaign #${s.campaignId} on ${s.contractId} (${s.network})`);
  console.log(`  state       ${c.state}`);
  console.log(`  raised      ${withThousands(sum.raised)} / ${withThousands(sum.goal)} (${sum.percent_of_goal}%)`);
  console.log(`  paid out    ${withThousands(sum.paid_out)}   refunded ${withThousands(sum.refunded)}   held ${withThousands(sum.available)}`);
  console.log(`  deadline    ${new Date(c.deadline * 1000).toISOString()}`);
  console.log(`  committee   ${c.threshold} of ${c.committee.length}: ${c.committee.map((a) => displayName(names, a)).join(", ")}`);
  console.log(`  ledger      ${sum.contributions} contributions from ${sum.contributors} addresses`);
  console.log(`  payouts     ${sum.payouts_executed} executed / ${sum.payouts_proposed} proposed, receipts total ${withThousands(sum.receipts_total)}`);
  console.log(`  reconciles  ${sum.reconciles ? "yes" : "NO"}`);
  console.log(`  verify      ${contractLink(s.network, s.contractId)}`);
  if (s.payouts.length) {
    console.log("  payout  biller  amount        approvals  status");
    for (const p of s.payouts) {
      const status = p.executed
        ? `paid to ${displayName(names, p.paid_to ?? "")} at ${new Date(p.executed_at * 1000).toISOString()}`
        : `needs ${approvalsRemaining(c, { ...p, purposeHash: p.purpose_hash, billerId: p.biller_id })} more`;
      console.log(`  #${p.id}      #${p.biller_id}      ${withThousands(formatUnits(p.amount)).padStart(12)}  ${p.approvals.length}/${c.threshold}        ${status}`);
    }
  }
}

export async function main(argv: string[]): Promise<void> {
  const args = parseArgs(argv);
  const env = readEnv();
  const names = loadNames(args.options.names ?? env.namesFile);

  switch (args.command) {
    case "help":
      console.log(usage());
      return;

    case "csv": {
      const text = readFileSync(args.positional[0], "utf8");
      const r = parseContributorsCsv(text);
      if (args.flags.has("json")) {
        console.log(JSON.stringify({ rows: r.rows, rejected: r.rejected, total: formatUnits(r.total) }, bigintReplacer, 2));
        return;
      }
      for (const row of r.rows) {
        console.log(`${String(row.line).padStart(3)}  ${row.name.padEnd(24)} ${withThousands(formatUnits(row.amount)).padStart(10)}  ${row.address}  ${row.memo}`);
      }
      for (const rej of r.rejected) console.log(`${String(rej.line).padStart(3)}  REJECTED: ${rej.reason}`);
      console.log(`${r.rows.length} valid rows, ${r.rejected.length} rejected, total ${withThousands(formatUnits(r.total))}`);
      return;
    }

    case "create": {
      const kp = signer(env);
      const inv = campaignCall.create(need(env.campaignContractId, "CAMPAIGN_CONTRACT_ID"), {
        organizer: kp.publicKey(),
        token: need(env.tokenContractId, "TOKEN_CONTRACT_ID"),
        goal: parseAmountToStroops(args.options.goal),
        deadline: parseDeadline(args.options.deadline),
        committee: parseCommittee(args.options.committee),
        threshold: Number(args.options.threshold),
        registry: need(env.registryContractId, "REGISTRY_CONTRACT_ID"),
        titleHash: noteHash(args.options.title ?? ""),
      });
      await sendOrPrint(env, args, inv, kp);
      return;
    }

    case "contribute": {
      const kp = signer(env);
      const inv = campaignCall.contribute(
        need(env.campaignContractId, "CAMPAIGN_CONTRACT_ID"),
        Number(args.options.campaign),
        kp.publicKey(),
        parseAmountToStroops(args.options.amount),
        noteHash(args.options.memo ?? ""),
      );
      await sendOrPrint(env, args, inv, kp);
      return;
    }

    case "propose": {
      const kp = signer(env);
      const inv = campaignCall.proposePayout(
        need(env.campaignContractId, "CAMPAIGN_CONTRACT_ID"),
        Number(args.options.campaign),
        kp.publicKey(),
        Number(args.options.biller),
        parseAmountToStroops(args.options.amount),
        noteHash(args.options.purpose),
      );
      await sendOrPrint(env, args, inv, kp);
      return;
    }

    case "approve": {
      const kp = signer(env);
      const inv = campaignCall.approvePayout(
        need(env.campaignContractId, "CAMPAIGN_CONTRACT_ID"),
        Number(args.options.campaign),
        kp.publicKey(),
        Number(args.options.payout),
      );
      await sendOrPrint(env, args, inv, kp);
      return;
    }

    case "refund": {
      const kp = signer(env);
      const id = Number(args.options.campaign);
      const contractId = need(env.campaignContractId, "CAMPAIGN_CONTRACT_ID");
      const inv = args.options.claim
        ? campaignCall.claimRefund(contractId, id, args.options.claim)
        : campaignCall.refund(contractId, id);
      await sendOrPrint(env, args, inv, kp);
      return;
    }

    case "close": {
      const kp = signer(env);
      const inv = campaignCall.close(need(env.campaignContractId, "CAMPAIGN_CONTRACT_ID"), Number(args.options.campaign));
      await sendOrPrint(env, args, inv, kp);
      return;
    }

    case "status": {
      const s = await loadSnapshot(env, args);
      if (args.flags.has("json")) console.log(snapshotToJson(s));
      else printStatus(s, names);
      return;
    }

    case "export": {
      const s = await loadSnapshot(env, args);
      const format = args.options.format ?? "json";
      const body = format === "csv" ? toExportCsv(s, names) : toExportJson(s, names);
      if (args.options.out) {
        writeFileSync(args.options.out, body);
        console.log(`wrote ${args.options.out} (${format}, ${s.contributions.length} contributions, ${s.payouts.length} payouts)`);
        console.log(`verify: ${contractLink(s.network, s.contractId)}`);
      } else {
        process.stdout.write(body);
      }
      return;
    }
  }
}

const isEntrypoint = process.argv[1] && /cli\.js$/.test(process.argv[1]);
if (isEntrypoint) {
  main(process.argv.slice(2)).catch((err: unknown) => {
    if (err instanceof UsageError) {
      console.error(err.message);
      process.exitCode = 64;
    } else {
      console.error((err as Error).message ?? String(err));
      process.exitCode = 1;
    }
  });
}

// Re-exported for the offline tests.
export { simulateView, accountLink };
