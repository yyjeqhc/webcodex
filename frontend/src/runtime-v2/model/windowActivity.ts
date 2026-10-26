import type { WindowActivity } from "./types.js";

const MAX_MERGED_WINDOW_ACTIVITY = 2_000;

/** A call's mutable result, duration, Project and Session facts are not its identity. */
export function windowActivityIdentity(row: WindowActivity): string {
  return row.server_trace_id ? "trace:" + row.server_trace_id
    : [row.started_at_ms, row.method, row.tool_name || ""].join("|");
}

/** Exact comparison of JSON-shaped response facts, including optional/new fields.
 * Identity or a selected-field fingerprint alone cannot prove content unchanged. */
export function sameWindowValue(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (!left || !right || typeof left !== "object" || typeof right !== "object") return false;
  if (Array.isArray(left) || Array.isArray(right)) {
    return Array.isArray(left) && Array.isArray(right) && left.length === right.length &&
      left.every((value, index) => sameWindowValue(value, right[index]));
  }
  const a = left as Record<string, unknown>, b = right as Record<string, unknown>;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length &&
    keys.every(key => Object.prototype.hasOwnProperty.call(b, key) && sameWindowValue(a[key], b[key]));
}

export function shareWindowArray<T>(current: T[], incoming: T[]): T[] {
  if (current === incoming) return current;
  const rows = incoming.map((row, index) => sameWindowValue(current[index], row) ? current[index] : row);
  return rows.length === current.length && rows.every((row, index) => row === current[index]) ? current : rows;
}

function activityEntries(rows: WindowActivity[]): Array<[string, WindowActivity]> {
  const occurrences = new Map<string, number>();
  return rows.map(row => {
    const identity = windowActivityIdentity(row);
    if (row.server_trace_id) return [identity, row];
    // Older observations may lack a trace. Preserve repeated invocations at the
    // same millisecond rather than collapsing them by their mutable result.
    const occurrence = occurrences.get(identity) || 0;
    occurrences.set(identity, occurrence + 1);
    return [identity + "#" + occurrence, row];
  });
}

export function mergeWindowActivity(
  current: WindowActivity[], incoming: WindowActivity[], preferIncoming = true,
): { activity: WindowActivity[]; truncated: boolean } {
  // Primary reads usually repeat the retained head. Compare only that head;
  // unchanged polls need neither a full-history map nor a sort/merged array.
  if (current.length <= MAX_MERGED_WINDOW_ACTIVITY && incoming.length <= current.length &&
      incoming.every((row, index) => sameWindowValue(row, current[index]))) {
    return { activity: current, truncated: false };
  }
  const byKey = new Map(activityEntries(current));
  let changed = false;
  for (const [key, row] of activityEntries(incoming)) {
    const previous = byKey.get(key);
    if (!previous || (preferIncoming && !sameWindowValue(previous, row))) {
      byKey.set(key, row);
      changed = true;
    }
  }
  if (!changed && current.length <= MAX_MERGED_WINDOW_ACTIVITY) return { activity: current, truncated: false };
  const activity = [...byKey.values()].sort((left, right) =>
    right.started_at_ms - left.started_at_ms || right.ended_at_ms - left.ended_at_ms ||
    windowActivityIdentity(left).localeCompare(windowActivityIdentity(right)));
  const truncated = activity.length > MAX_MERGED_WINDOW_ACTIVITY;
  if (truncated) activity.length = MAX_MERGED_WINDOW_ACTIVITY;
  return { activity, truncated };
}
