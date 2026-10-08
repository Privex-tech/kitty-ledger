/**
 * Contributor CSV ingestion. Organisers keep the pledge list in a spreadsheet exported
 * from WhatsApp polls and M-Pesa statements; it arrives with stray spaces, currency
 * symbols, decimal commas, blank lines and the odd broken row. This parser accepts the
 * messy-but-meaningful rows and reports the rest with a line number and a reason.
 */
import { StrKey } from "@stellar/stellar-sdk";
import { AmountError, parseAmountToStroops } from "./amounts.js";
import { noteHashHex, normaliseNote } from "./hash.js";

export interface ContributorRow {
  line: number;
  name: string;
  address: string;
  amount: bigint;
  memo: string;
  memoHash: string;
  country: string;
}

export interface RejectedRow {
  line: number;
  raw: string;
  reason: string;
}

export interface CsvIngestion {
  rows: ContributorRow[];
  rejected: RejectedRow[];
  total: bigint;
}

const REQUIRED = ["name", "address", "amount"] as const;

/** RFC 4180-ish line splitter: quoted fields, doubled quotes, CRLF, BOM. */
export function splitCsvLine(line: string): string[] {
  const out: string[] = [];
  let cur = "";
  let inQuotes = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (inQuotes) {
      if (ch === '"') {
        if (line[i + 1] === '"') {
          cur += '"';
          i++;
        } else {
          inQuotes = false;
        }
      } else {
        cur += ch;
      }
    } else if (ch === '"') {
      inQuotes = true;
    } else if (ch === ",") {
      out.push(cur);
      cur = "";
    } else {
      cur += ch;
    }
  }
  out.push(cur);
  return out;
}

export function parseContributorsCsv(text: string): CsvIngestion {
  const lines = text.replace(/^﻿/, "").split(/\r?\n/);
  const rows: ContributorRow[] = [];
  const rejected: RejectedRow[] = [];

  let header: string[] | null = null;
  let headerLine = 0;
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const lineNo = i + 1;
    if (raw.trim() === "" || raw.trim().startsWith("#")) continue;
    if (!header) {
      header = splitCsvLine(raw).map((h) => h.trim().toLowerCase());
      headerLine = lineNo;
      for (const col of REQUIRED) {
        if (!header.includes(col)) {
          throw new Error(`CSV header on line ${headerLine} is missing the "${col}" column`);
        }
      }
      continue;
    }
    const cells = splitCsvLine(raw);
    const get = (col: string) => {
      const idx = header!.indexOf(col);
      return idx >= 0 ? (cells[idx] ?? "").trim() : "";
    };
    if (cells.length < header.length && cells.every((c) => c.trim() === "")) continue;

    const name = get("name").replace(/\s+/g, " ");
    const address = get("address").toUpperCase();
    const amountRaw = get("amount");
    const memo = normaliseNote(get("memo"));
    const country = get("country").toUpperCase();

    if (name === "") {
      rejected.push({ line: lineNo, raw, reason: "missing name" });
      continue;
    }
    if (!StrKey.isValidEd25519PublicKey(address)) {
      rejected.push({ line: lineNo, raw, reason: `invalid Stellar address "${get("address")}"` });
      continue;
    }
    let amount: bigint;
    try {
      amount = parseAmountToStroops(amountRaw);
    } catch (e) {
      const reason = e instanceof AmountError ? e.message : String(e);
      rejected.push({ line: lineNo, raw, reason });
      continue;
    }
    rows.push({ line: lineNo, name, address, amount, memo, memoHash: noteHashHex(memo), country });
  }
  if (!header) throw new Error("CSV has no header row");
  const total = rows.reduce((acc, r) => acc + r.amount, 0n);
  return { rows, rejected, total };
}

/** Per-address totals, as the contract's `contribution(id, addr)` view would report them. */
export function totalsByAddress(rows: ContributorRow[]): Map<string, bigint> {
  const m = new Map<string, bigint>();
  for (const r of rows) m.set(r.address, (m.get(r.address) ?? 0n) + r.amount);
  return m;
}
