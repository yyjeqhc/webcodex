export const MAX_DOCX_BYTES = 10 * 1024 * 1024;
export const DOCX_CHUNK_BYTES = 512 * 1024;
const MIN_TRANSFER_DEADLINE_MS = 120_000;
const TRANSFER_BASE_MS = 60_000;
const TRANSFER_PER_CHUNK_MS = 20_000;
const MAX_TRANSFER_DEADLINE_MS = 15 * 60_000;

export function docxDocumentTransferDeadlineMs(bytes) {
  const chunks = Math.ceil(bytes / DOCX_CHUNK_BYTES);
  return Math.min(MAX_TRANSFER_DEADLINE_MS,
    Math.max(MIN_TRANSFER_DEADLINE_MS, TRANSFER_BASE_MS + chunks * TRANSFER_PER_CHUNK_MS));
}

export function validDocxIdentity(value) {
  return value && typeof value.project === "string" && value.project.length > 0 && value.project.length <= 512
    && typeof value.path === "string" && value.path.length > 0 && value.path.length <= 512 && /\.docx$/i.test(value.path)
    && !/^(?:\/|[A-Za-z]:)/.test(value.path) && !/[\\\0]/.test(value.path) && !value.path.split("/").includes("..")
    && typeof value.name === "string" && value.name.length > 0 && value.name.length <= 255
    && /^[0-9a-f]{64}$/.test(value.sha256) && Number.isSafeInteger(value.bytes) && value.bytes >= 4 && value.bytes <= MAX_DOCX_BYTES;
}

export async function readDocxDocument({ identity, request, current, progress = () => {}, now = () => performance.now() }) {
  if (!validDocxIdentity(identity)) throw new Error("Invalid DOCX identity");
  const deadline = now() + docxDocumentTransferDeadlineMs(identity.bytes);
  const bytes = new Uint8Array(identity.bytes);
  let offset = 0;
  while (offset < bytes.length) {
    if (!current()) throw new Error("DOCX reader closed");
    const remaining = deadline - now();
    if (remaining <= 0) throw new Error("Document loading timed out. Retry this version.");
    const { page, encoded } = await request(offset, remaining);
    if (!current()) throw new Error("DOCX reader closed");
    const length = Math.min(DOCX_CHUNK_BYTES, bytes.length - offset), next = offset + length;
    if (!page || page.project !== identity.project || page.path !== identity.path || page.sha256 !== identity.sha256
      || page.bytes_total !== bytes.length || page.byte_offset !== offset || page.complete !== (next === bytes.length)
      || page.next_byte_offset !== (next === bytes.length ? null : next)
      || typeof encoded !== "string" || encoded.length !== Math.ceil(length / 3) * 4
      || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(encoded)) throw new Error("Invalid DOCX chunk");
    const decoded = Uint8Array.from(atob(encoded), char => char.charCodeAt(0));
    if (decoded.length !== length) throw new Error("Invalid DOCX chunk length");
    bytes.set(decoded, offset); offset = next; progress(offset, bytes.length);
  }
  const digest = [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))].map(value => value.toString(16).padStart(2, "0")).join("");
  if (!current()) throw new Error("DOCX reader closed");
  if (now() >= deadline) throw new Error("Document loading timed out. Retry this version.");
  if (digest !== identity.sha256 || bytes[0] !== 80 || bytes[1] !== 75 || bytes[2] !== 3 || bytes[3] !== 4)
    throw new Error("Document version mismatch. Reopen the document.");
  return bytes;
}
