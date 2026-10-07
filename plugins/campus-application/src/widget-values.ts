import type { SnapshotNode } from "./form-cache.js";

export function normalizeChoice(value: string): string {
  return value.normalize("NFKC").trim().replace(/\s+/g, " ").toLowerCase();
}
export function validChoicePath(path: readonly string[] | undefined): path is readonly string[] {
  return path !== undefined && path.length >= 1 && path.length <= 4
    && path.every(value => normalizeChoice(value).length > 0
      && !value.includes("\0") && Buffer.byteLength(value) <= 4096);
}

export function choiceReadback(node: SnapshotNode, path: readonly string[]): "confirmed" | "mismatch" | "unresolved" {
  const value = node.value ?? (node.selected === true ? node.name : undefined);
  if (value === undefined) return "unresolved";
  const normalized = normalizeChoice(value);
  if (path.length === 1) return normalized === normalizeChoice(path[0]!) ? "confirmed" : "mismatch";
  const observed = value.split(/\s*(?:\/|>|→|›|»)\s*/u).map(normalizeChoice);
  const expected = path.map(normalizeChoice);
  if (observed.length === expected.length && observed.every((part, i) => part === expected[i])) return "confirmed";
  // An undelimited value (including a leaf or concatenated path) cannot establish
  // ancestry. Preserve step boundaries so ["AB", "C"] and ["A", "BC"] differ.
  return observed.length === 1 ? "unresolved" : "mismatch";
}
export function canonicalDate(value: string): string | undefined {
  const text = value.normalize("NFKC").trim()
    .replace(/^(?:预计(?:于)?|expected(?:\s+in)?)\s*/iu, "").replace(/\s+/g, "");
  const parts = text.match(/^(\d{4})[-/.](\d{1,2})(?:[-/.](\d{1,2}))?$/u)
    ?? text.match(/^(\d{4})年(\d{1,2})月(?:(\d{1,2})日)?$/u);
  if (!parts) return;
  const year = Number(parts[1]), month = Number(parts[2]), day = parts[3] === undefined ? undefined : Number(parts[3]);
  if (year < 1 || month < 1 || month > 12) return;
  const leap = year % 400 === 0 || year % 4 === 0 && year % 100 !== 0;
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  if (day !== undefined && (day < 1 || day > days[month - 1]!)) return;
  return parts[1] + "-" + String(month).padStart(2, "0")
    + (day === undefined ? "" : "-" + String(day).padStart(2, "0"));
}

export function dateForControl(value: string, node: SnapshotNode): string | undefined {
  const date = canonicalDate(value);
  if (!date) return;
  const type = node.form_context?.input_type?.toLowerCase();
  const hint = node.form_context?.component_hint ?? "";
  const placeholder = (node.form_context?.placeholder ?? "").replace(/\s+/g, "");
  if (type === "date") return date.length === 10 ? date : undefined;
  if (type === "month") return date.slice(0, 7);
  // Campus profiles intentionally carry date precision, not invented clock time.
  // A native datetime-local requires a time component, so fail during planning
  // instead of issuing a Browser set_value that HTML will deterministically reject.
  if (type === "datetime-local") return undefined;
  if (/^y{4}[-/.]m{2}[-/.]d{2}$/iu.test(placeholder)) return date.length === 10 ? date : undefined;
  if (/month.?picker/iu.test(hint) || /^y{4}[-/.]m{2}$/iu.test(placeholder)
    || /年月(?!日)/u.test(node.name)) return date.slice(0, 7);
  return date;
}

