import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { Keypair, StrKey, TransactionBuilder, Networks, scValToNative } from "@stellar/stellar-sdk";
import { parseArgs, parseCommittee, parseDeadline, UsageError, usage } from "../src/cli-args.js";
import { decodeInvocation } from "../src/chain.js";

const CLI = fileURLToPath(new URL("../src/cli.js", import.meta.url));
const SEED = fileURLToPath(new URL("../../../data/seed/contributors.csv", import.meta.url));
const CONTRACT = StrKey.encodeContract(Buffer.alloc(32, 7));

test("parses every command with its required options", () => {
  const cases: Array<[string[], string]> = [
    [["create", "--goal", "3000", "--deadline", "2026-12-31", "--committee", "GA,GB,GC", "--threshold", "2"], "create"],
    [["contribute", "--campaign", "0", "--amount", "25.37", "--memo", "for mama"], "contribute"],
    [["propose", "--campaign=0", "--biller=0", "--amount=2750", "--purpose=KNH deposit"], "propose"],
    [["approve", "--campaign", "0", "--payout", "0"], "approve"],
    [["refund", "--campaign", "0"], "refund"],
    [["refund", "--campaign", "0", "--claim", "GABC"], "refund"],
    [["close", "--campaign", "0"], "close"],
    [["status", "--campaign", "0", "--json"], "status"],
    [["export", "--campaign", "0", "--format", "csv", "--out", "x.csv"], "export"],
    [["csv", "data/seed/contributors.csv"], "csv"],
    [["help"], "help"],
    [["--help"], "help"],
  ];
  for (const [argv, command] of cases) {
    assert.equal(parseArgs(argv).command, command, argv.join(" "));
  }
  const p = parseArgs(["contribute", "--campaign", "3", "--amount", "5", "--dry-run", "--json"]);
  assert.deepEqual(p.options, { campaign: "3", amount: "5" });
  assert.deepEqual([...p.flags].sort(), ["dry-run", "json"]);
  assert.deepEqual(parseArgs(["csv", "list.csv"]).positional, ["list.csv"]);
});

test("rejects unknown commands, missing options and bad values", () => {
  assert.throws(() => parseArgs([]), UsageError);
  assert.throws(() => parseArgs(["donate"]), /unknown command "donate"/);
  assert.throws(() => parseArgs(["create", "--goal", "3000"]), /missing required option --deadline/);
  assert.throws(() => parseArgs(["contribute", "--campaign", "0"]), /missing required option --amount/);
  assert.throws(() => parseArgs(["approve", "--campaign", "0", "--payout"]), /--payout needs a value/);
  assert.throws(() => parseArgs(["approve", "--campaign", "x", "--payout", "0"]), /--campaign must be a non-negative integer/);
  assert.throws(() => parseArgs(["approve", "--campaign", "0", "--payout", "-1"]), /--payout must be a non-negative integer/);
  assert.throws(() => parseArgs(["export", "--campaign", "0", "--format", "xml"]), /--format must be json or csv/);
  assert.throws(() => parseArgs(["csv"]), /expected exactly one file path/);
  assert.throws(() => parseArgs(["status", "--campaign", ""]), /missing required option --campaign/);
  assert.throws(() => parseArgs(["status", "--", "0"]), /malformed option/);
  assert.match(usage(), /kitty <command>/);
});

test("deadline and committee parsing", () => {
  const now = Date.UTC(2026, 8, 24) / 1000;
  assert.equal(parseDeadline("2026-10-24", now), Date.UTC(2026, 9, 24, 23, 59, 59) / 1000);
  assert.equal(parseDeadline("1800000000", now), 1_800_000_000);
  assert.equal(parseDeadline("2026-10-24T10:00:00Z", now), Date.UTC(2026, 9, 24, 10) / 1000);
  assert.throws(() => parseDeadline("2020-01-01", now), /in the past/);
  assert.throws(() => parseDeadline("next tuesday", now), /cannot parse deadline/);
  assert.deepEqual(parseCommittee("GA, GB,GC  GD"), ["GA", "GB", "GC", "GD"]);
  assert.throws(() => parseCommittee(" , "), /committee is empty/);
});

test("kitty csv validates the seed list offline", () => {
  const out = execFileSync(process.execPath, [CLI, "csv", SEED], { encoding: "utf8" });
  assert.match(out, /40 valid rows, 2 rejected, total 3,311\.82/);
  assert.match(out, /13  REJECTED: invalid amount "fifty dollars"/);
  assert.match(out, /30  REJECTED: invalid Stellar address "GABCDEF123"/);
  const json = JSON.parse(execFileSync(process.execPath, [CLI, "csv", SEED, "--json"], { encoding: "utf8" }));
  assert.equal(json.rows.length, 40);
  assert.equal(json.total, "3311.82");
});

test("kitty contribute --dry-run prints an unsigned transaction without any RPC", () => {
  const kp = Keypair.random();
  const out = execFileSync(
    process.execPath,
    [CLI, "contribute", "--campaign", "0", "--amount", "$25.37", "--memo", "Pole sana", "--dry-run"],
    {
      encoding: "utf8",
      env: {
        ...process.env,
        SECRET_KEY: kp.secret(),
        CAMPAIGN_CONTRACT_ID: CONTRACT,
        NETWORK_PASSPHRASE: Networks.TESTNET,
        RPC_URL: "http://127.0.0.1:9",
      },
    },
  );
  const lines = out.trim().split("\n");
  assert.equal(lines[0], `dry run: contribute on ${CONTRACT}`);
  const tx = TransactionBuilder.fromXDR(lines[1], Networks.TESTNET);
  assert.ok("operations" in tx);
  const inv = decodeInvocation(tx as any);
  assert.equal(inv.method, "contribute");
  assert.equal(inv.contractId, CONTRACT);
  assert.equal(scValToNative(inv.args[0]), 0);
  assert.equal(scValToNative(inv.args[1]), kp.publicKey());
  assert.equal(scValToNative(inv.args[2]), 253_700_000n);
  assert.equal(Buffer.from(scValToNative(inv.args[3])).length, 32);
});

test("kitty fails clearly when the signing key is missing and on usage errors", () => {
  const run = (argv: string[]) => {
    try {
      execFileSync(process.execPath, [CLI, ...argv], {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
        env: { ...process.env, SECRET_KEY: "", CAMPAIGN_CONTRACT_ID: CONTRACT },
      });
      return { status: 0, stderr: "" };
    } catch (e: any) {
      return { status: e.status as number, stderr: String(e.stderr) };
    }
  };
  const noKey = run(["approve", "--campaign", "0", "--payout", "0", "--dry-run"]);
  assert.equal(noKey.status, 64);
  assert.match(noKey.stderr, /SECRET_KEY is not set/);
  const bad = run(["approve", "--campaign", "0"]);
  assert.equal(bad.status, 64);
  assert.match(bad.stderr, /missing required option --payout/);
});

test("kitty status without a reachable RPC exits non-zero with a readable error", () => {
  try {
    execFileSync(process.execPath, [CLI, "status", "--campaign", "0"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, CAMPAIGN_CONTRACT_ID: CONTRACT, RPC_URL: "http://127.0.0.1:9" },
      timeout: 20_000,
    });
    assert.fail("expected a failure");
  } catch (e: any) {
    assert.equal(e.status, 1);
    assert.ok(String(e.stderr).trim().length > 0);
  }
});
