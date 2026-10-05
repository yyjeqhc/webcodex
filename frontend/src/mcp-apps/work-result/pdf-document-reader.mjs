import { MAX_PDF_BYTES } from "./pdf-reader.mjs";

export const APP_ARTIFACT_CHUNK_BYTES = 512 * 1024;
const base64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;
const decode = value => Uint8Array.from(atob(value), char => char.charCodeAt(0));
const MIN_TRANSFER_DEADLINE_MS = 120_000;
const TRANSFER_BASE_MS = 60_000;
const TRANSFER_PER_CHUNK_MS = 20_000;
const MAX_TRANSFER_DEADLINE_MS = 15 * 60_000;

export function pdfDocumentTransferDeadlineMs(bytes) {
  const chunks = Math.ceil(bytes / APP_ARTIFACT_CHUNK_BYTES);
  return Math.min(MAX_TRANSFER_DEADLINE_MS,
    Math.max(MIN_TRANSFER_DEADLINE_MS, TRANSFER_BASE_MS + chunks * TRANSFER_PER_CHUNK_MS));
}

export function privateToolMetadata(result, key) {
  const candidates = [result, result?.result, result?.toolResult, result?.tool_result];
  for (const candidate of candidates) {
    if (!candidate || typeof candidate !== "object") continue;
    const value = candidate._meta?.[key] ?? candidate.meta?.[key];
    if (value && typeof value === "object") return value;
  }
  return null;
}

async function verifyPdfDocument(bytes, identity, current, now, deadline) {
  const digest = [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))]
    .map(value => value.toString(16).padStart(2, "0")).join("");
  if (!current()) throw new Error("PDF preview closed");
  if (now() >= deadline) throw new Error("PDF loading timed out · retry this version");
  if (digest !== identity.sha256 || new TextDecoder().decode(bytes.subarray(0, 5)) !== "%PDF-")
    throw new Error("PDF version mismatch · reopen the document");
  return bytes;
}

// Presentation Apps share one version-fenced 512 KiB Host-facing artifact bridge. The bridge
// reauthorizes Project access on every call; digest identity never grants authority.
export async function readPdfDocument({
  identity, request, current, progress = () => {}, now = () => performance.now(),
}) {
  const deadline = now() + pdfDocumentTransferDeadlineMs(identity.bytes);
  if (!Number.isSafeInteger(identity.bytes) || identity.bytes < 5 || identity.bytes > MAX_PDF_BYTES
    || !/^[0-9a-f]{64}$/.test(identity.sha256)) throw new Error("Invalid PDF document identity");

  const bytes = new Uint8Array(identity.bytes);
  let offset = 0;
  while (offset < bytes.length) {
    if (!current()) throw new Error("PDF preview closed");
    const remaining = deadline - now();
    if (remaining <= 0) throw new Error("PDF loading timed out · retry this version");
    const { page, encoded } = await request(offset, remaining);
    if (!current()) throw new Error("PDF preview closed");
    if (!page || page.project !== identity.project || page.path !== identity.path || page.sha256 !== identity.sha256
      || page.bytes_total !== bytes.length || page.byte_offset !== offset
      || typeof encoded !== "string" || encoded.length > Math.ceil(APP_ARTIFACT_CHUNK_BYTES / 3) * 4
      || !base64.test(encoded))
      throw new Error("Invalid presentation artifact chunk");
    const decoded = decode(encoded);
    const next = offset + decoded.length;
    if (decoded.length !== Math.min(APP_ARTIFACT_CHUNK_BYTES, bytes.length - offset)
      || page.complete !== (next === bytes.length) || page.next_byte_offset !== (page.complete ? null : next))
      throw new Error("Invalid presentation artifact continuation");
    bytes.set(decoded, offset); offset = next; progress(offset, bytes.length);
  }
  return verifyPdfDocument(bytes, identity, current, now, deadline);
}
