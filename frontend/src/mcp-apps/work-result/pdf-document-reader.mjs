import { MAX_PDF_BYTES, PDF_PAGE_BYTES } from "./pdf-reader.mjs";

// A version fence selects bytes, not authority. The server reauthorizes each RPC.
export async function readPdfDocument({ identity, request, current, progress = () => {}, now = () => performance.now() }) {
  const deadline = now() + 120_000;
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
      || typeof encoded !== "string" || encoded.length > Math.ceil(PDF_PAGE_BYTES / 3) * 4
      || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(encoded))
      throw new Error("Invalid PDF document chunk");
    const decoded = Uint8Array.from(atob(encoded), char => char.charCodeAt(0));
    const next = offset + decoded.length;
    if (decoded.length !== Math.min(PDF_PAGE_BYTES, bytes.length - offset)
      || page.complete !== (next === bytes.length) || page.next_byte_offset !== (page.complete ? null : next))
      throw new Error("Invalid PDF document continuation");
    bytes.set(decoded, offset); offset = next; progress(offset, bytes.length);
  }
  const digest = [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))].map(value => value.toString(16).padStart(2, "0")).join("");
  if (!current()) throw new Error("PDF preview closed");
  if (now() >= deadline) throw new Error("PDF loading timed out · retry this version");
  if (digest !== identity.sha256 || new TextDecoder().decode(bytes.subarray(0, 5)) !== "%PDF-")
    throw new Error("PDF version mismatch · reopen the document");
  return bytes;
}
