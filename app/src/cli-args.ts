/**
 * Argument parsing for the `kitty` CLI. Pure and synchronous so it can be unit-tested
 * without touching the network or the environment.
 */

export const COMMANDS = [
  "create",
  "contribute",
  "propose",
  "approve",
  "refund",
  "close",
  "biller-register",
  "biller-verify",
  "biller-active",
  "biller-propose-address",
  "biller-apply-address",
  "biller-cancel-address",
  "biller-status",
  "status",
  "export",
  "csv",
  "help",
] as const;
export type Command = (typeof COMMANDS)[number];

export interface ParsedArgs {
  command: Command;
  options: Record<string, string>;
  flags: Set<string>;
  positional: string[];
}

export class UsageError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "UsageError";
  }
}

const REQUIRED: Record<Command, string[]> = {
  create: ["goal", "deadline", "committee", "threshold"],
  contribute: ["campaign", "amount"],
  propose: ["campaign", "biller", "amount", "purpose"],
  approve: ["campaign", "payout"],
  refund: ["campaign"],
  close: ["campaign"],
  "biller-register": ["name", "category", "address"],
  "biller-verify": ["biller"],
  "biller-active": ["biller"],
  "biller-propose-address": ["biller", "address"],
  "biller-apply-address": ["biller"],
  "biller-cancel-address": ["biller"],
  "biller-status": ["biller"],
  status: ["campaign"],
  export: ["campaign"],
  csv: [],
  help: [],
};

export const BILLER_CATEGORIES = ["hospital", "school", "funeral", "utility", "other"] as const;

const KNOWN_FLAGS = new Set(["dry-run", "json", "verbose", "help", "revoke", "deactivate"]);

export function usage(): string {
  return `kitty - Kitty Ledger CLI (community campaigns on Stellar)

Usage: kitty <command> [options]

Commands
  create      --goal <usd> --deadline <YYYY-MM-DD|unix> --committee <G..,G..> --threshold <n> [--title <text>]
  contribute  --campaign <id> --amount <usd> [--memo <text>]
  propose     --campaign <id> --biller <id> --amount <usd> --purpose <text>
  approve     --campaign <id> --payout <id>
  refund      --campaign <id> [--claim <G...>]        switch to refunding, or claim one contributor's refund
  close       --campaign <id>
  biller-register         --name <text> --category <hospital|school|funeral|utility|other> --address <G...>   (registry admin)
  biller-verify           --biller <id> [--revoke]                 mark a biller payable, or revoke   (registry admin)
  biller-active           --biller <id> [--deactivate]             re-enable or immediately disable   (registry admin)
  biller-propose-address  --biller <id> --address <G...>           start the 24 h address change      (registry admin)
  biller-apply-address    --biller <id>                            apply it once the delay has passed (registry admin)
  biller-cancel-address   --biller <id>                            drop a pending address change      (registry admin)
  biller-status           --biller <id>                            read the biller record and whether it is payable
  status      --campaign <id> [--names <file.json>]
  export      --campaign <id> [--format json|csv] [--out <file>] [--names <file.json>] [--from-snapshot <file>]
  csv         <contributors.csv>                     validate a pledge list and print totals

Flags
  --dry-run   build the unsigned transaction and print its XDR instead of submitting
  --json      machine-readable output

Environment (see .env.example): RPC_URL, NETWORK_PASSPHRASE, CAMPAIGN_CONTRACT_ID,
REGISTRY_CONTRACT_ID, TOKEN_CONTRACT_ID, SECRET_KEY, NAMES_FILE
`;
}

export function parseArgs(argv: string[]): ParsedArgs {
  if (argv.length === 0) throw new UsageError("missing command\n\n" + usage());
  const [cmd, ...rest] = argv;
  if (cmd === "--help" || cmd === "-h") return { command: "help", options: {}, flags: new Set(), positional: [] };
  if (!(COMMANDS as readonly string[]).includes(cmd)) {
    throw new UsageError(`unknown command "${cmd}"\n\n` + usage());
  }
  const command = cmd as Command;
  const options: Record<string, string> = {};
  const flags = new Set<string>();
  const positional: string[] = [];

  for (let i = 0; i < rest.length; i++) {
    const tok = rest[i];
    if (tok.startsWith("--")) {
      const eq = tok.indexOf("=");
      const key = (eq >= 0 ? tok.slice(2, eq) : tok.slice(2)).trim();
      if (key === "") throw new UsageError(`malformed option "${tok}"`);
      if (eq >= 0) {
        options[key] = tok.slice(eq + 1);
      } else if (KNOWN_FLAGS.has(key)) {
        flags.add(key);
      } else {
        const next = rest[i + 1];
        if (next === undefined || next.startsWith("--")) throw new UsageError(`option --${key} needs a value`);
        options[key] = next;
        i++;
      }
    } else {
      positional.push(tok);
    }
  }

  for (const req of REQUIRED[command]) {
    if (!(req in options) || options[req].trim() === "") {
      throw new UsageError(`${command}: missing required option --${req}\n\n` + usage());
    }
  }
  if (command === "csv" && positional.length !== 1) throw new UsageError("csv: expected exactly one file path");
  if ("category" in options && !(BILLER_CATEGORIES as readonly string[]).includes(options.category)) {
    throw new UsageError(`--category must be one of ${BILLER_CATEGORIES.join(", ")}, got "${options.category}"`);
  }
  if ("format" in options && !["json", "csv"].includes(options.format)) {
    throw new UsageError(`export: --format must be json or csv, got "${options.format}"`);
  }
  for (const key of ["campaign", "payout", "biller", "threshold"]) {
    if (key in options && !/^\d+$/.test(options[key])) {
      throw new UsageError(`--${key} must be a non-negative integer, got "${options[key]}"`);
    }
  }
  return { command, options, flags, positional };
}

/** "YYYY-MM-DD" (end of that day, UTC), ISO datetime, or unix seconds -> unix seconds. */
export function parseDeadline(input: string, now: number = Math.floor(Date.now() / 1000)): number {
  const s = input.trim();
  let ts: number;
  if (/^\d{9,11}$/.test(s)) {
    ts = Number(s);
  } else if (/^\d{4}-\d{2}-\d{2}$/.test(s)) {
    ts = Math.floor(Date.parse(`${s}T23:59:59Z`) / 1000);
  } else {
    ts = Math.floor(Date.parse(s) / 1000);
  }
  if (!Number.isFinite(ts)) throw new UsageError(`cannot parse deadline "${input}"`);
  if (ts <= now) throw new UsageError(`deadline "${input}" is in the past`);
  return ts;
}

/** Parses a comma- or space-separated list of committee member addresses. */
export function parseCommittee(input: string): string[] {
  const members = input
    .split(/[,\s]+/)
    .map((m) => m.trim())
    .filter((m) => m !== "");
  if (members.length === 0) throw new UsageError("committee is empty");
  return members;
}
