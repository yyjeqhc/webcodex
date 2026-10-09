import JSZip from "jszip";
import { renderAsync } from "docx-preview";
import createDOMPurify from "dompurify";
import { MAX_DOCX_BYTES } from "./docx-reader.mjs";

export const MAX_EXPANDED_BYTES = 32 * 1024 * 1024;
const MAX_ENTRY_BYTES = 8 * 1024 * 1024, MAX_XML_BYTES = 8 * 1024 * 1024, MAX_XML_NODES = 60_000;

// Check central-directory bounds before a decompressor sees untrusted sizes.
export function inspectDocxZip(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length < 22 || bytes.length > MAX_DOCX_BYTES) throw new Error("Unsupported DOCX package size");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let end = bytes.length - 22;
  while (end >= Math.max(0, bytes.length - 65557) && !(view.getUint32(end, true) === 0x06054b50 && end + 22 + view.getUint16(end + 20, true) === bytes.length)) end--;
  if (end < Math.max(0, bytes.length - 65557)) throw new Error("Invalid DOCX ZIP directory");
  const count = view.getUint16(end + 10, true), size = view.getUint32(end + 12, true), start = view.getUint32(end + 16, true);
  if (!count || count > 2048 || view.getUint16(end + 4, true) || view.getUint16(end + 6, true)
    || view.getUint16(end + 8, true) !== count || start + size !== end) throw new Error("Unsupported DOCX ZIP directory");
  let offset = start, expanded = 0;
  const names = new Set();
  for (let index = 0; index < count; index++) {
    if (offset + 46 > end || view.getUint32(offset, true) !== 0x02014b50) throw new Error("Invalid DOCX ZIP entry");
    const flags = view.getUint16(offset + 8, true), compression = view.getUint16(offset + 10, true);
    const compressed = view.getUint32(offset + 20, true), uncompressed = view.getUint32(offset + 24, true);
    const length = view.getUint16(offset + 28, true), extra = view.getUint16(offset + 30, true), comment = view.getUint16(offset + 32, true);
    const local = view.getUint32(offset + 42, true);
    if ((flags & 1) || ![0, 8].includes(compression) || view.getUint16(offset + 34, true)
      || uncompressed > MAX_ENTRY_BYTES || local + 30 > start || compressed > bytes.length
      || offset + 46 + length + extra + comment > end) throw new Error("Unsupported DOCX ZIP entry");
    const name = new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(offset + 46, offset + 46 + length));
    if (!name || name.length > 512 || /[\\\0]/.test(name) || /^(?:\/|[A-Za-z]:)/.test(name) || name.includes("//")
      || name.split("/").some(part => part === ".." || part === ".") || names.has(name)) throw new Error("Unsafe DOCX ZIP path");
    if (view.getUint32(local, true) !== 0x04034b50 || local + 30 + view.getUint16(local + 26, true) + view.getUint16(local + 28, true) + compressed > start)
      throw new Error("Invalid DOCX local ZIP entry");
    const localName = new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(local + 30, local + 30 + view.getUint16(local + 26, true)));
    if (localName !== name) throw new Error("DOCX ZIP entry name mismatch");
    names.add(name); expanded += uncompressed;
    if (expanded > MAX_EXPANDED_BYTES) throw new Error("DOCX expands beyond the 32 MiB preview limit");
    offset += 46 + length + extra + comment;
  }
  if (offset !== end || !names.has("[Content_Types].xml") || !names.has("_rels/.rels") || !names.has("word/document.xml"))
    throw new Error("Not a supported DOCX document");
  return { names, expanded };
}

function readZipEntry(entry, limit, current) {
  return new Promise((resolve, reject) => {
    const stream = entry.internalStream("uint8array"), chunks = [];
    let size = 0, failed = false;
    stream.on("data", chunk => {
      if (failed) return;
      size += chunk.length;
      if (!current() || size > limit) { failed = true; stream.pause(); reject(new Error("DOCX expansion limit exceeded or reader closed")); return; }
      chunks.push(chunk);
    }).on("error", error => { if (!failed) { failed = true; reject(error); } }).on("end", () => {
      if (failed) return;
      const data = new Uint8Array(size); let offset = 0;
      for (const chunk of chunks) { data.set(chunk, offset); offset += chunk.length; }
      resolve(data);
    }).resume();
  });
}

export async function prepareDocx(bytes, win, current = () => true) {
  const manifest = inspectDocxZip(bytes), zip = await JSZip.loadAsync(bytes);
  const purifier = createDOMPurify(win);
  let expanded = 0, xmlBytes = 0, nodes = 0;
  for (const name of manifest.names) {
    if (!current()) throw new Error("DOCX reader closed");
    const entry = zip.file(name);
    if (!entry) continue;
    const data = await readZipEntry(entry, Math.min(MAX_ENTRY_BYTES, MAX_EXPANDED_BYTES - expanded), current);
    expanded += data.length;
    if (data.length > MAX_ENTRY_BYTES || expanded > MAX_EXPANDED_BYTES) throw new Error("DOCX expansion limit exceeded");
    if (/\.(?:xml|rels)$/i.test(name)) {
      xmlBytes += data.length;
      if (xmlBytes > MAX_XML_BYTES) throw new Error("DOCX XML exceeds the preview limit");
      const text = new TextDecoder("utf-8", { fatal: true }).decode(data);
      if (/<!DOCTYPE|<!ENTITY/i.test(text)) throw new Error("Unsupported DOCX XML declaration");
      const parsed = new win.DOMParser().parseFromString(text, "application/xml");
      if (parsed.querySelector("parsererror")) throw new Error("Invalid DOCX XML");
      const walker = parsed.createTreeWalker(parsed, win.NodeFilter.SHOW_ALL);
      while (walker.nextNode()) { if (++nodes > MAX_XML_NODES) throw new Error("DOCX is too complex to preview"); }
      if (/\.rels$/i.test(name)) {
        for (const relationship of [...parsed.getElementsByTagNameNS("*", "Relationship")]) {
          if (relationship.getAttribute("TargetMode")?.toLowerCase() === "external") relationship.remove();
        }
        zip.file(name, new win.XMLSerializer().serializeToString(parsed));
      }
    } else if (/\.svg$/i.test(name)) {
      const clean = purifier.sanitize(new TextDecoder().decode(data), {
        USE_PROFILES: { svg: true, svgFilters: true },
        FORBID_TAGS: ["foreignObject", "script", "image", "a"], FORBID_ATTR: ["href", "xlink:href"],
      });
      zip.file(name, clean);
    }
  }
  if (!current()) throw new Error("DOCX reader closed");
  return zip.generateAsync({ type: "uint8array", compression: "STORE" });
}

function safeDeclarations(style) {
  for (const property of Array.from({ length: style.length }, (_, index) => style.item(index))) {
    const value = style.getPropertyValue(property);
    if (/[\\<>]|url\s*\(|expression\s*\(|javascript\s*:/i.test(value)
      || property === "z-index" || property === "behavior" || property === "-moz-binding"
      || (property === "position" && /fixed|sticky/i.test(value))) style.removeProperty(property);
  }
}

export function sanitizeDocx(container, styles, win) {
  const doc = container.ownerDocument, purifier = createDOMPurify(win), scoped = [];
  for (const source of styles.querySelectorAll("style")) {
    // Import and escaped tokens cannot cross the detached render boundary; unsafe declarations are stripped below.
    const text = source.textContent;
    if (text.length > 512 * 1024 || /[\\<]|@import/i.test(text)) continue;
    const probe = doc.createElement("style"); probe.media = "not all"; probe.textContent = text; doc.head.append(probe);
    try {
      const rules = [...(probe.sheet?.cssRules || [])];
      if (rules.length > 4096) throw new Error("DOCX styles exceed the preview limit");
      for (const rule of rules) {
        if (rule.type === win.CSSRule.STYLE_RULE && rule.selectorText.length <= 512) {
          safeDeclarations(rule.style);
          scoped.push(`${rule.selectorText.split(",").map(selector => `.docx-stage ${selector.trim()}`).join(",")} {${rule.style.cssText}}`);
        } else if (/^@counter-style docx-/i.test(rule.cssText) && !/[\\<>]|url\s*\(|expression\s*\(|javascript\s*:/i.test(rule.cssText)) scoped.push(rule.cssText);
      }
    } finally { probe.remove(); }
  }
  purifier.sanitize(container, { IN_PLACE: true, USE_PROFILES: { html: true, svg: true, mathMl: true },
    FORBID_TAGS: ["style", "iframe", "object", "embed", "script", "form", "input", "button", "link", "meta", "base"],
    FORBID_ATTR: ["href", "xlink:href", "srcset", "action", "formaction"],
    ALLOWED_URI_REGEXP: /^data:image\/(?:png|jpe?g|gif|webp|svg\+xml);/i,
  });
  const elements = container.querySelectorAll("*");
  if (elements.length > 40_000) throw new Error("DOCX rendered content exceeds the preview limit");
  for (const element of elements) { if (element.style) safeDeclarations(element.style); }
  const sheet = doc.createElement("style"); sheet.textContent = scoped.join("\n"); container.prepend(sheet);
}

export async function renderDocx(bytes, stage, current = () => true) {
  const win = stage.ownerDocument.defaultView;
  const prepared = await prepareDocx(bytes, win, current);
  if (!current()) throw new Error("DOCX reader closed");
  const content = stage.ownerDocument.createElement("div"), styles = stage.ownerDocument.createElement("div");
  await renderAsync(prepared, content, styles, {
    renderAltChunks: false, renderComments: false, renderChanges: false,
    ignoreFonts: true, useBase64URL: true, breakPages: true, ignoreLastRenderedPageBreak: false,
  });
  if (!current()) throw new Error("DOCX reader closed");
  sanitizeDocx(content, styles, win); stage.replaceChildren(content);
  return content.querySelectorAll("section.docx").length;
}
