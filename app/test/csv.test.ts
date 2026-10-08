import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { parseContributorsCsv, splitCsvLine, totalsByAddress } from "../src/csv.js";
import { sha256Hex } from "../src/hash.js";

const SEED = new URL("../../../data/seed/contributors.csv", import.meta.url);

test("seed pledge list: 40 valid rows, 2 rejected, exact total $3,311.82", () => {
  const r = parseContributorsCsv(readFileSync(SEED, "utf8"));
  assert.equal(r.rows.length, 40);
  assert.equal(r.rejected.length, 2);
  assert.equal(r.total, 33_118_200_000n);

  const reasons = r.rejected.map((x) => `${x.line}: ${x.reason}`);
  assert.match(reasons[0], /^13: invalid amount "fifty dollars": not a number/);
  assert.match(reasons[1], /^30: invalid Stellar address "GABCDEF123"/);

  const byName = new Map(r.rows.map((row) => [row.name, row]));
  assert.equal(byName.get("Tendai Moyo")?.amount, 125_000_000n, "decimal comma");
  assert.equal(byName.get("Amara Diallo")?.amount, 200_000_000n, "USD prefix");
  assert.equal(byName.get("Ng'ang'a Kariuki")?.amount, 752_500_000n, "padded spaces");
  assert.equal(byName.get("Kwame Boateng")?.amount, 253_700_000n, "dollar sign");
  assert.equal(byName.get("Esther Njoki")?.amount, 4_999_900_000n);
  assert.equal(byName.get("Fatuma Abdi")?.amount, 50_000_000n);

  const amounts = r.rows.map((x) => x.amount);
  assert.equal(amounts.reduce((a, b) => (a < b ? a : b)), 50_000_000n, "min $5.00");
  assert.equal(amounts.reduce((a, b) => (a > b ? a : b)), 5_000_000_000n, "max $500.00");
  assert.ok(new Set(r.rows.map((x) => x.address)).size === 40, "40 distinct addresses");
  for (const row of r.rows) assert.match(row.address, /^G[A-Z2-7]{55}$/);
});

test("memo hashes are sha256 of the normalised note", () => {
  const r = parseContributorsCsv(readFileSync(SEED, "utf8"));
  const wanjiru = r.rows.find((x) => x.name === "Wanjiru Kamau")!;
  assert.equal(wanjiru.memo, "for mama Njeri, from the Houston chapter");
  assert.equal(wanjiru.memoHash, sha256Hex("for mama Njeri, from the Houston chapter"));
  const fatuma = r.rows.find((x) => x.name === "Fatuma Abdi")!;
  assert.equal(fatuma.memo, "");
  assert.equal(fatuma.memoHash, sha256Hex(""));
});

test("copes with BOM, CRLF, quoted commas, blank lines, comments and lowercase addresses", () => {
  const addr = "GAXG7A5PIURLIMXKMWI2BCPRBWVNGG2ILSKJIT3Z3IX52PBWJQ3FCXK7";
  const text =
    "﻿Name, Address ,Amount,Memo,Country\r\n" +
    "\r\n" +
    "# exported from WhatsApp poll\r\n" +
    `"Kamau, Wanjiru",${addr.toLowerCase()}," $12.50 ","says ""asante"", will top up",us\r\n` +
    `,${addr},5,,\r\n` +
    `Only Name,,5,,\r\n` +
    `Zero Giver,${addr},0.00,,KE\r\n` +
    `  Double   Space  ,${addr},7,memo   with   spaces,\r\n`;
  const r = parseContributorsCsv(text);
  assert.equal(r.rows.length, 2);
  assert.deepEqual(
    r.rows.map((x) => [x.line, x.name, x.address, x.amount, x.memo, x.country]),
    [
      [4, "Kamau, Wanjiru", addr, 125_000_000n, 'says "asante", will top up', "US"],
      [8, "Double Space", addr, 70_000_000n, "memo with spaces", ""],
    ],
  );
  assert.deepEqual(
    r.rejected.map((x) => [x.line, x.reason.split(":")[0]]),
    [
      [5, "missing name"],
      [6, 'invalid Stellar address ""'],
      [7, 'invalid amount "0.00"'],
    ],
  );
  assert.equal(r.total, 195_000_000n);
});

test("aggregates repeat givers per address like the contract's balance map", () => {
  const addr = "GAXG7A5PIURLIMXKMWI2BCPRBWVNGG2ILSKJIT3Z3IX52PBWJQ3FCXK7";
  const r = parseContributorsCsv(`name,address,amount\nA,${addr},5\nA again,${addr},1.25\n`);
  const totals = totalsByAddress(r.rows);
  assert.equal(totals.size, 1);
  assert.equal(totals.get(addr), 62_500_000n);
});

test("rejects a header without the required columns", () => {
  assert.throws(() => parseContributorsCsv("name,amount\nA,5\n"), /missing the "address" column/);
  assert.throws(() => parseContributorsCsv("\n\n"), /no header row/);
});

test("splitCsvLine handles quotes and empty cells", () => {
  assert.deepEqual(splitCsvLine('a,"b,c",,"d ""e"""'), ["a", "b,c", "", 'd "e"']);
  assert.deepEqual(splitCsvLine(""), [""]);
});
