import { createHash } from "node:crypto";

const HASH_ALGO = "sha256";

/** sha256 of a UTF-8 string, as a 32-byte Buffer (what the contracts store). */
export function sha256(text: string): Buffer {
  return createHash(HASH_ALGO).update(text, "utf8").digest();
}

/** sha256 of a UTF-8 string, as a hex string. */
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

/**
 * @returns The sha256 hash as a Buffer.
 */
export function noteHash(note: string | undefined | null): Buffer {
  return sha256(normaliseNote(note));
}

/**
 * @returns The sha256 hash as a hex string.
 */
export function noteHashHex(note: string | undefined | null): string {
  return noteHash(note).toString("hex");
}
