// The HTML controller supplies the exact pinned Project/Session/snapshot/path.
// Binary bytes come only from the App-private channel, never text fallback.
export const MAX_PDF_BYTES = 20 * 1024 * 1024;
export const PDF_PAGE_BYTES = 128 * 1024;
export const PDF_REASONS = Object.freeze({
  deleted: "最终版本中无此 PDF · 已删除",
  symlink: "符号链接 · 无法预览 PDF",
  submodule: "子模块 · 无法预览 PDF",
  unsupported_file_type: "该文件类型不支持 PDF 预览",
  too_large: "PDF 超过 20 MiB 的预览上限",
  not_pdf: "此文件没有受支持的 PDF 文件头",
});

export async function readPdf({ identity, request, current, progress = () => {}, now = () => performance.now() }) {
  const deadline = now() + 120_000;
  let bytes = null, offset = 0;
  do {
    if (!current()) throw new Error("PDF preview closed");
    const remaining = deadline - now();
    if (remaining <= 0) throw new Error("PDF 加载超时 · 请重试预览");
    const { page, encoded } = await request(offset, remaining);
    if (!current()) throw new Error("PDF preview closed");
    if (!page || page.project !== identity.project || (page.session_id ?? null) !== (identity.session_id ?? null)
      || page.snapshot_id !== identity.snapshot_id || page.path !== identity.path || page.view !== "pdf")
      throw new Error("Invalid PDF snapshot identity");
    if (page.unavailable_reason !== undefined) {
      if (!Object.hasOwn(PDF_REASONS, page.unavailable_reason)) throw new Error("Invalid PDF unavailable reason");
      throw new Error(PDF_REASONS[page.unavailable_reason]);
    }
    if (page.byte_offset !== offset || !Number.isSafeInteger(page.bytes_total) || page.bytes_total <= 0
      || page.bytes_total > MAX_PDF_BYTES || typeof page.complete !== "boolean"
      || (bytes && page.bytes_total !== bytes.length) || typeof encoded !== "string"
      || encoded.length > Math.ceil(PDF_PAGE_BYTES / 3) * 4 || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(encoded))
      throw new Error("Invalid PDF chunk bounds");
    const decoded = Uint8Array.from(atob(encoded), char => char.charCodeAt(0));
    const next = offset + decoded.length;
    if (decoded.length !== Math.min(PDF_PAGE_BYTES, page.bytes_total - offset)
      || decoded.length === 0 || page.complete !== (next === page.bytes_total)
      || page.next_byte_offset !== (page.complete ? null : next)) throw new Error("Invalid PDF continuation");
    bytes ||= new Uint8Array(page.bytes_total);
    bytes.set(decoded, offset); offset = next;
    progress(offset, bytes.length);
  } while (offset < bytes.length);
  return bytes;
}
