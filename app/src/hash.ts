import { createHash } from "node:crypto";

/** sha256 of a UTF-8 string, as a 32-byte Buffer (what the contracts store). */
export function sha256(text: string): Buffer {
  return createHash("sha256").update(text, "utf8").digest();
}

export function sha256Hex(text: string): string {
  return sha256(text).toString("hex");
}

/**
 * Memo and purpose hashes are always the sha256 of the trimmed, whitespace-collapsed
 * note so that a contributor can recompute it from what they typed.
 */
export function normaliseNote(note: string | undefined | null): string {
  return String(note ?? "").trim().replace(/\s+/g, " ");
}

export function noteHash(note: string | undefined | null): Buffer {
  return sha256(normaliseNote(note));
}

export function noteHashHex(note: string | undefined | null): string {
  return noteHash(note).toString("hex");
}
