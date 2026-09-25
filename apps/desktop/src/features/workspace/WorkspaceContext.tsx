import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DesktopState } from "../../models/topology";
import type { RunnerOverview, WindowSummary, WorkspaceProject, WorkspaceRequest, WorkflowSession } from "../../models/workspace";

export function workspaceQuery<T>(request: WorkspaceRequest): Promise<T> {
  return invoke<T>("workspace_query", { request });
}
// UI fallback only; authoritative runtime IDs take precedence.
export function sameProjectPath(a?: string, b?: string): boolean {
  if (!a || !b) return false;
  const normalize = (value: string) => {
    const windows = /^[A-Za-z]:[\\/]/.test(value) || value.startsWith("\\\\");
    if (windows) {
      value = value.replace(/^\\\\\?\\UNC\\/i, "\\\\").replace(/^\\\\\?\\(?=[a-z]:\\)/i, "");
      return value.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
    }
    return value.replace(/\/+$/, "") || "/";
  };
  return normalize(a) === normalize(b);
}
export function sameProject(row: WorkspaceProject, saved: { runtime_project_id?: string | null; path?: string }): boolean {
  return row.id && saved.runtime_project_id
    ? row.id === saved.runtime_project_id
    : sameProjectPath(row.path, saved.path);
}
export function mergeProjects(runner: WorkspaceProject[], saved: { runtime_project_id?: string | null; path: string }[], complete = false): WorkspaceProject[] {
  const rows = [...runner];
  for (const project of complete ? [] : saved) {
    if (!rows.some(row => sameProject(row, project))) rows.push({
      id: project.runtime_project_id || "", path: project.path, connected: false,
    });
  }
  return rows.sort((a, b) => (b.sessions?.latest_updated_at || 0) - (a.sessions?.latest_updated_at || 0));
}
export function runnerInventoryComplete(runner: RunnerOverview | null | undefined): boolean {
  return Boolean(runner?.connected && runner.projects_available && !runner.projects_truncated
    && runner.visible_project_count === runner.projects.length);
}
export function sessionTitle(value: string): string {
  const title = value.trim().split(/\r?\n/).find(Boolean) || "Workflow Session";
  return title.length > 110 ? title.slice(0, 109) + "…" : title;
}
export { projectPresentationName as projectName, displayProjectPath } from "../../../../../frontend/src/ui/projectPresentation";
interface WorkspaceValue {
  state: DesktopState; runner: RunnerOverview | null; projects: WorkspaceProject[]; windows: WindowSummary[];
  sessions: WorkflowSession[]; loading: boolean; error: boolean; windowsError: boolean; refresh: () => void; removeProject: (id: string) => void; revision: number;
  selection: { kind: "session"; project: string; id: string } | { kind: "window"; id: string } | null;
  setSelection: (value: WorkspaceValue["selection"]) => void;
}
const WorkspaceContext = createContext<WorkspaceValue | null>(null);
export function WorkspaceProvider({ state, children }: { state: DesktopState; children: ReactNode }) {
  const key = JSON.stringify([state.topology?.server, state.workspace_runner]);
  const [snapshot, setSnapshot] = useState<{ key: string; runner: RunnerOverview | null; windows: WindowSummary[]; error: boolean; windowsError: boolean } | null>(null);
  const [loading, setLoading] = useState(false);
  const [revision, setRevision] = useState(0);
  const [selection, setSelection] = useState<WorkspaceValue["selection"]>(null);
  const [removed, setRemoved] = useState<string[]>([]);
  const removeProject = useCallback((id: string) => {
    setRemoved(ids => [...ids, id]);
    setSelection(current => current?.kind === "session" && current.project === id ? null : current);
  }, []);
  const refresh = useCallback(() => setRevision(value => value + 1), []);
  const busy = Boolean(state.current_operation);
  const ready = state.readiness.server === "ready" && state.readiness.runner === "ready";
  useEffect(() => {
    if (!ready || busy) return;
    let disposed = false;
    let timer: number | undefined;
    const poll = async () => {
      if (disposed) return;
      setLoading(true);
      const [runner, windows] = await Promise.allSettled([
        workspaceQuery<RunnerOverview>({ kind: "overview" }),
        workspaceQuery<{ windows: WindowSummary[] }>({ kind: "windows" }),
      ]);
      if (disposed) return;
      setSnapshot(old => ({
        key,
        runner: runner.status === "fulfilled" ? runner.value : old?.key === key ? old.runner : null,
        windows: windows.status === "fulfilled" ? windows.value.windows : old?.key === key ? old.windows : [],
        error: runner.status === "rejected", windowsError: windows.status === "rejected",
      }));
      if (runner.status === "fulfilled" && runnerInventoryComplete(runner.value)) {
        setRemoved(ids => ids.filter(id => runner.value.projects.some(project => project.id === id)));
      }
      setLoading(false);
      timer = window.setTimeout(() => { if (document.visibilityState === "visible") void poll(); }, 15_000);
    };
    const visible = () => { if (document.visibilityState === "visible") refresh(); };
    document.addEventListener("visibilitychange", visible);
    void poll();
    return () => { disposed = true; if (timer) window.clearTimeout(timer); document.removeEventListener("visibilitychange", visible); };
  }, [key, ready, busy, revision, refresh]);
  useEffect(() => { setSelection(null); setRemoved([]); }, [key]);
  const current = snapshot?.key === key ? snapshot : null;
  const projects = useMemo(() => mergeProjects(current?.runner?.projects || [],
    state.saved_projects || (state.project ? [state.project] : []), runnerInventoryComplete(current?.runner))
    .filter(project => !removed.includes(project.id)),
  [current?.runner, state.saved_projects, state.project, removed]);
  useEffect(() => {
    setSelection(current => current?.kind === "session" && !projects.some(project => project.id === current.project) ? null : current);
  }, [projects]);
  const ids = new Set(projects.map(project => project.id));
  return <WorkspaceContext.Provider value={{ state, runner: current?.runner || null, projects,
    windows: (current?.windows || []).filter(row => !row.last_project || ids.has(row.last_project)),
    sessions: (current?.runner?.recent_sessions?.sessions || []).filter(session => !session.project_id || ids.has(session.project_id)), loading: ready && !busy && (loading || !current),
    error: Boolean(current?.error), windowsError: Boolean(current?.windowsError), refresh, removeProject, revision, selection, setSelection,
  }}>{children}</WorkspaceContext.Provider>;
}
export function useWorkspace(): WorkspaceValue {
  const context = useContext(WorkspaceContext);
  if (!context) throw new Error("WorkspaceProvider is required");
  return context;
}
