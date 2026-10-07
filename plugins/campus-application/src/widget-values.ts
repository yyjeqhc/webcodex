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
  if (normalized && normalized === normalizeChoice(path.join(""))) return "confirmed";
  // A leaf alone does not establish the selected ancestry, and is not evidence
  // that re-running a completed composite action is needed.
  return normalized === expected.at(-1) ? "unresolved" : "mismatch";
}
