/**
 * Money handling. Campaign tokens (USDC on Stellar) have 7 decimals; one "stroop" is
 * 10^-7 of a unit. All arithmetic is done in bigint stroops so that $499.99 stays
 * $499.99 and never becomes 499.98999999.
 */

export const DECIMALS = 7;
export const UNIT = 10n ** BigInt(DECIMALS);

/**
 * Thrown when the parsed amount fails basic constraints like being empty or negative.
 */
export class AmountError extends Error {
  constructor(
    public readonly input: string,
    reason: string,
  ) {
    super(`invalid amount "${input}": ${reason}`);
    this.name = "AmountError";
  }
}

/**
 * Parse a human-typed amount into stroops. Accepts the ways people actually write
 * money in a spreadsheet: "$52.37", "USD 20", " 75.25 ", "5", "1,250.00" and the
 * European decimal comma "12,50". Rejects blanks, words, negatives, zero and more
 * than 7 decimal places.
 */
export function parseAmountToStroops(raw: string): bigint {
  const input = String(raw ?? "");
  let s = input.trim();
  if (s === "") throw new AmountError(input, "empty");
  s = s.replace(/^(usd|usdc|us\$|\$)\s*/i, "").replace(/\s*(usd|usdc)$/i, "");
  s = s.replace(/\s+/g, "");
  if (s.startsWith("-") || s.startsWith("(")) throw new AmountError(input, "negative");
  s = s.replace(/^\+/, "");

  // Decide what commas mean.
  if (s.includes(",")) {
    const commaDecimal = /^\d{1,3}(\.\d{3})*,\d{1,7}$|^\d+,\d{1,2}$/.test(s);
    if (commaDecimal && !s.includes(".")) {
      s = s.replace(",", ".");
    } else if (commaDecimal) {
      // "1.250,00": dots are thousands separators, comma is decimal.
      s = s.replace(/\./g, "").replace(",", ".");
    } else if (/^\d{1,3}(,\d{3})+(\.\d{1,7})?$/.test(s)) {
      s = s.replace(/,/g, "");
    } else {
      throw new AmountError(input, "ambiguous use of commas");
    }
  }

  const m = /^(\d+)(?:\.(\d*))?$/.exec(s);
  if (!m) throw new AmountError(input, "not a number");
  const whole = m[1];
  const frac = m[2] ?? "";
  if (frac.length > DECIMALS) throw new AmountError(input, `more than ${DECIMALS} decimals`);
  const stroops = BigInt(whole) * UNIT + BigInt((frac + "0".repeat(DECIMALS)).slice(0, DECIMALS));
  if (stroops === 0n) throw new AmountError(input, "zero");
  return stroops;
}

/** Whole cents -> stroops (used by seed data that is authored in cents). */
export function centsToStroops(cents: number | bigint): bigint {
  return BigInt(cents) * (UNIT / 100n);
}

/**
 * Format stroops as a decimal string with at least two decimals and no trailing
 * zeros beyond that: 523700000n -> "52.37", 500000000n -> "50.00", 1n -> "0.0000001".
 */
export function formatUnits(stroops: bigint): string {
  const negative = stroops < 0n;
  const abs = negative ? -stroops : stroops;
  const whole = abs / UNIT;
  let frac = (abs % UNIT).toString().padStart(DECIMALS, "0");
  frac = frac.replace(/0+$/, "");
  if (frac.length < 2) frac = frac.padEnd(2, "0");
  return `${negative ? "-" : ""}${whole.toString()}.${frac}`;
}

/** "3311.82" -> "3,311.82" for display. */
export function withThousands(formatted: string): string {
  const [whole, frac] = formatted.split(".");
  const sign = whole.startsWith("-") ? "-" : "";
  const digits = sign ? whole.slice(1) : whole;
  const grouped = digits.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return frac === undefined ? `${sign}${grouped}` : `${sign}${grouped}.${frac}`;
}

/** Sums an iterable of stroops values into a total. */
export function sumStroops(values: Iterable<bigint>): bigint {
  let total = 0n;
  for (const v of values) total += v;
  return total;
}
