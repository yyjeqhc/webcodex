import { displayProjectPath, projectFamilyName, projectVariantLabel, sourceProjectRuntimeId } from "../../ui/projectPresentation.js";
import type { ProjectRow, WindowSummary } from "./types.js";

// Same recency interval as Peer discovery; this never asserts host presence.
export const WINDOW_RECENT_MS = 10 * 60 * 1_000;
export const WINDOW_TIERS = ["running", "recent", "today", "earlier"] as const;
export type WindowTier = typeof WINDOW_TIERS[number];
export const WINDOW_TIER_TEXT: Record<WindowTier, string> = {
  running: "Running", recent: "Within 10 minutes", today: "Today", earlier: "Earlier",
};
export type WindowBoardRow = ReturnType<typeof windowBoardRow>;

export function windowBoardRow(window: WindowSummary, projects: ReadonlyMap<string, ProjectRow>, now: number, windowText = "Window") {
  const project = projects.get(window.last_project || "");
  const source = project && projects.get(sourceProjectRuntimeId(project) || "");
  const projectName = project ? projectFamilyName(source || project, projects) : "";
  const workspaceName = project?.lineage ? projectVariantLabel(project) : projectName;
  const lastActiveAt = window.last_meaningful_activity_at_ms ?? window.last_seen_at_ms;
  const tier: WindowTier = window.active_count > 0 ? "running"
    : now - lastActiveAt <= WINDOW_RECENT_MS ? "recent"
    : new Date(now).toDateString() === new Date(lastActiveAt).toDateString() ? "today" : "earlier";
  const defaultName = `${workspaceName || windowText} · ${window.client_window_key.slice(0, 4)}`;
  return {
    window, tier, defaultName, displayName: defaultName, projectName, workspaceName,
    sourceLabel: ({ "openai-session": "ChatGPT", "openai-conversation": "ChatGPT", "codex-thread": "Codex", "client-window-header": "Header" } as Record<string, string>)[window.source] || window.source,
    machine: project?.client_id || "", path: displayProjectPath(project?.path),
    firstSeenAt: window.first_seen_at_ms, lastActiveAt,
    action: window.last_activity_name || "", linkedSessions: window.linked_session_count,
  };
}

export function compareWindowRows(a: WindowBoardRow, b: WindowBoardRow) {
  return Number(b.tier === "running") - Number(a.tier === "running") || b.lastActiveAt - a.lastActiveAt ||
    a.window.client_window_key.localeCompare(b.window.client_window_key);
}

export function matchesWindowRow(row: WindowBoardRow, query: string) {
  const needle = query.trim().toLocaleLowerCase();
  return [row.displayName, row.projectName, row.workspaceName, row.path, row.machine, row.sourceLabel,
    row.action, row.window.source, row.window.last_project, row.window.client_window_key,
    "wc_peer_" + row.window.client_window_key.slice(0, 32)]
    .some(value => value?.toLocaleLowerCase().includes(needle));
}
