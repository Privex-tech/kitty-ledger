import { test } from "node:test";
import assert from "node:assert/strict";
import { AmountError, centsToStroops, formatUnits, parseAmountToStroops, sumStroops, withThousands } from "../src/amounts.js";

test("parses the ways people type money in a spreadsheet", () => {
  const cases: Array<[string, bigint]> = [
    ["$52.37", 523_700_000n],
    ["USD 20", 200_000_000n],
    ["usd20", 200_000_000n],
    ["20 USDC", 200_000_000n],
    ["  75.25  ", 752_500_000n],
    ["5", 50_000_000n],
    ["499.99", 4_999_900_000n],
    ["12,50", 125_000_000n], // decimal comma
    ["1,250.00", 12_500_000_000n], // thousands separator
    ["1.250,00", 12_500_000_000n], // European thousands + decimal comma
    ["0.0000001", 1n],
    ["+8.75", 87_500_000n],
    ["3311.82", 33_118_200_000n],
  ];
  for (const [input, expected] of cases) {
    assert.equal(parseAmountToStroops(input), expected, `input ${JSON.stringify(input)}`);
  }
});

test("rejects blanks, words, negatives, zero, too many decimals and ambiguous commas", () => {
  const bad = ["", "   ", "fifty dollars", "-5", "(5.00)", "0", "0.00", "1.23456789", "1,2,3", "12,345,6", "5..5", "NaN"];
  for (const input of bad) {
    assert.throws(() => parseAmountToStroops(input), AmountError, `input ${JSON.stringify(input)}`);
  }
});

test("formats stroops without floating point drift", () => {
  assert.equal(formatUnits(523_700_000n), "52.37");
  assert.equal(formatUnits(500_000_000n), "50.00");
  assert.equal(formatUnits(1n), "0.0000001");
  assert.equal(formatUnits(33_118_200_000n), "3311.82");
  assert.equal(formatUnits(0n), "0.00");
  assert.equal(formatUnits(-125_000_000n), "-12.50");
  assert.equal(withThousands("3311.82"), "3,311.82");
  assert.equal(withThousands("1234567.5"), "1,234,567.5");
  assert.equal(withThousands("-1234.00"), "-1,234.00");
  assert.equal(withThousands("12"), "12");
});

test("round trips every parse through format", () => {
  for (const s of ["5.00", "25.37", "499.99", "3311.82", "0.0000001", "1000000.00"]) {
    assert.equal(formatUnits(parseAmountToStroops(s)), s);
  }
});

test("cents helper and sums match the seeded campaign", () => {
  assert.equal(centsToStroops(331_182), 33_118_200_000n);
  assert.equal(sumStroops([centsToStroops(275_000), centsToStroops(56_182)]), centsToStroops(331_182));
});
