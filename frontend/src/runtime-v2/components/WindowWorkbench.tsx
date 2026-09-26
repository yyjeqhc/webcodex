import {
  Box,
  CircleDot,
  Clock3,
  FolderOpen,
  MessageSquare,
  Monitor,
  Search,
  Server,
} from "lucide-react";
import { CopyIdentity } from "./ui/CopyIdentity.js";
import { TextInput } from "@mantine/core";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  displayProjectPath,
  projectFamilyId,
  projectFamilyName,
  projectVariantLabel,
  sourceProjectRuntimeId,
} from "../../ui/projectPresentation.js";
import { ProjectPicker } from "../../ui/ProjectPicker.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { absoluteTime, clockTime, relativeTime, shortId } from "../model/format.js";
import type { ProjectRow, WindowSummary } from "../model/types.js";

import { useWindowWorkspace } from "../state/useWindowWorkspace.js";
import { WorkSurfaceSwitch, type WorkSurface } from "./GoalWorkbench.js";
import { WindowCollaboration } from "./WindowCollaboration.js";
import { WindowActivityFeed } from "./WindowActivityFeed.js";


type Props = {
  client: RuntimeV2Client;
  language: RuntimeLanguage;
  projects: ProjectRow[];
  surface: WorkSurface;
  onSurfaceChange: (surface: WorkSurface) => void;
  onUnauthorized: () => void;
  onOpenSessionRecord?: (project: string, sessionId: string) => void;
  requestedWindowKey?: string;
  requestedSessionId?: string;
  onRequestedWindowConsumed?: () => void;
};

type WindowNavigationRow = Pick<WindowSummary,
  "client_window_key" | "last_project" | "source" | "last_seen_at_ms" |
  "last_meaningful_activity_at_ms" | "last_activity_name" | "active_count">;

type ProjectFamily = {
  id: string;
  label: string;
  runner: string;
  projectIds: Set<string>;
  windowCount: number;
  activeWindowCount: number;
  lastObservedAt: number;
};

function windowObservedAt(window: WindowSummary): number {
  return window.last_seen_at_ms || window.last_tool_call_at_ms || window.last_meaningful_activity_at_ms || 0;
}

function projectFor(projects: ProjectRow[], id?: string): ProjectRow | undefined {
  return id ? projects.find((project) => project.id === id) : undefined;
}

function peerId(windowKey: string): string {
  return windowKey.length >= 32 ? "wc_peer_" + windowKey.slice(0, 32) : windowKey;
}

function buildProjectFamilies(projects: ProjectRow[], windows: WindowSummary[]): ProjectFamily[] {
  const groups = new Map<string, ProjectRow[]>();
  for (const project of projects) {
    const id = projectFamilyId(project);
    const rows = groups.get(id) || [];
    rows.push(project);
    groups.set(id, rows);
  }
  const families = [...groups.entries()].map(([id, rows]) => {
    const representative = rows.find((row) => row.id === id) || rows[0];
    return {
      id,
      label: projectFamilyName(representative, projects),
      runner: representative.client_id,
      projectIds: new Set(rows.map((row) => row.id)),
      windowCount: 0,
      activeWindowCount: 0,
      lastObservedAt: 0,
    };
  });
  const byId = new Map(families.map((family) => [family.id, family]));
  for (const window of windows) {
    const project = projectFor(projects, window.last_project);
    if (!project) continue;
    const family = byId.get(projectFamilyId(project));
    if (!family) continue;
    family.windowCount += 1;
    if (window.active_count > 0) family.activeWindowCount += 1;
    family.lastObservedAt = Math.max(family.lastObservedAt, windowObservedAt(window));
  }
  return families.sort((a, b) =>
    Number(b.activeWindowCount > 0) - Number(a.activeWindowCount > 0) ||
    Number(b.windowCount > 0) - Number(a.windowCount > 0) ||
    b.lastObservedAt - a.lastObservedAt ||
    a.label.localeCompare(b.label));
}

export function WindowWorkbench({
  client,
  language,
  projects,
  surface,
  onSurfaceChange,
  onUnauthorized,
  onOpenSessionRecord,
  requestedWindowKey,
  requestedSessionId,
  onRequestedWindowConsumed,
}: Props) {
  const t = (value: string) => translate(value, language);
  const windows = useWindowWorkspace(client, true, onUnauthorized, { refreshMs: 3_000, loadDetail: true, initialWindowKey: requestedWindowKey });
  const listRef = useRef<HTMLDivElement | null>(null);
  const selectedRowRef = useRef<HTMLButtonElement | null>(null);
  const [search, setSearch] = useState("");
  const [projectFamily, setProjectFamily] = useState("");
  const [centerTab, setCenterTab] = useState<"window" | "collaboration">("window");
  const [selectedSessionId, setSelectedSessionId] = useState("");
  const families = useMemo(() => buildProjectFamilies(projects, windows.windows), [projects, windows.windows]);

  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    const family = families.find((row) => row.id === projectFamily);
    return windows.windows
      .filter((window) => {
        if (family && (!window.last_project || !family.projectIds.has(window.last_project))) return false;
        if (!query) return true;
        const project = projectFor(projects, window.last_project);
        const sourceId = project ? sourceProjectRuntimeId(project) : undefined;
        const source = projectFor(projects, sourceId);
        const values = [
          window.client_window_key,
          peerId(window.client_window_key),
          window.source,
          window.last_activity_name || "",
          window.last_activity_status || "",
          window.last_project || "",
          project?.name || "",
          project?.path || "",
          project?.client_id || "",
          source?.name || "",
          source?.path || "",
          window.active_count ? "active running" : "idle",
        ];
        return values.some((value) => value.toLowerCase().includes(query));
      })
      .sort((a, b) =>
        Number(b.active_count > 0) - Number(a.active_count > 0) ||
        windowObservedAt(b) - windowObservedAt(a) ||
        a.client_window_key.localeCompare(b.client_window_key));
  }, [families, projectFamily, projects, search, windows.windows]);

  const detail = windows.detail;
  useEffect(() => {
    if (!windows.selectedKey) return;
    setCenterTab("window");
    setSelectedSessionId("");
  }, [windows.selectedKey]);
  useEffect(() => {
    if (!requestedWindowKey) return;
    if (windows.selectedKey !== requestedWindowKey) {
      windows.select(requestedWindowKey);
      return;
    }
    setSearch("");
    setProjectFamily("");
    setCenterTab("window");
    setSelectedSessionId(requestedSessionId || "");
    onRequestedWindowConsumed?.();
  }, [onRequestedWindowConsumed, requestedWindowKey, requestedSessionId, windows.selectedKey, windows.select]);
  const selectedSummary = windows.windows.find((row) => row.client_window_key === windows.selectedKey);
  const activeRequest = detail?.active_requests.slice().sort((a, b) => b.started_at_ms - a.started_at_ms)[0];
  const currentProjectId = activeRequest?.project || selectedSummary?.last_project || detail?.activity[0]?.project;
  const currentProject = projectFor(projects, currentProjectId);
  const sourceProject = currentProject ? projectFor(projects, sourceProjectRuntimeId(currentProject)) : undefined;
  const currentActivity = activeRequest?.tool_name ||
    selectedSummary?.last_activity_name ||
    detail?.activity[0]?.tool_name ||
    detail?.activity[0]?.method;
  const lastObservedAt = detail?.last_seen_at_ms || selectedSummary?.last_seen_at_ms;
  const firstObservedAt = detail?.first_seen_at_ms || selectedSummary?.first_seen_at_ms;
  const isActive = (detail?.active_count ?? selectedSummary?.active_count ?? 0) > 0;
  const currentProjectName = currentProject
    ? projectFamilyName(sourceProject || currentProject, projects)
    : undefined;
  const currentWorkspaceName = currentProject?.lineage
    ? projectVariantLabel(currentProject)
    : currentProjectName;
  const currentMachine = currentProject?.client_id;
  const currentDirectory = displayProjectPath(currentProject?.path);
  const currentProjectAddress = currentProject?.id;
  const hasSelectedWindow = Boolean(selectedSummary || detail);
  const visibleActiveCount = detail?.active_count ?? selectedSummary?.active_count ?? 0;
  const visibleActivityCount = (detail?.activity_returned || 0) + visibleActiveCount;

  // Keep the exact selected resource represented even when a bounded inventory
  // or a user filter omits it. Derive its presentation from the same detail as
  // the main pane rather than assigning another Window's Project to it.
  const selectedNavigationRow: WindowNavigationRow | undefined = detail ? {
    client_window_key: detail.client_window_key,
    source: detail.source,
    last_project: currentProjectId,
    last_seen_at_ms: detail.last_seen_at_ms,
    last_meaningful_activity_at_ms: detail.last_meaningful_activity_at_ms,
    last_activity_name: currentActivity,
    active_count: detail.active_count,
  } : selectedSummary;
  const selectedIsPinned = Boolean(windows.selectedKey && !filtered.some(row => row.client_window_key === windows.selectedKey));
  useEffect(() => {
    const list = listRef.current;
    const row = selectedRowRef.current;
    if (!list || !row || list.scrollHeight <= list.clientHeight) return;
    const viewport = list.getBoundingClientRect();
    const bounds = row.getBoundingClientRect();
    if (bounds.top < viewport.top) list.scrollTop -= viewport.top - bounds.top;
    else if (bounds.bottom > viewport.bottom) list.scrollTop += bounds.bottom - viewport.bottom;
  }, [windows.selectedKey, selectedIsPinned, Boolean(selectedNavigationRow)]);

  const renderWindow = (window: WindowNavigationRow) => {
    const project = projectFor(projects, window.last_project);
    const source = project ? projectFor(projects, sourceProjectRuntimeId(project)) : undefined;
    const activity = window.last_activity_name || t("Observed Window");
    const observedAt = window.last_meaningful_activity_at_ms || window.last_seen_at_ms;
    const projectLabel = project
      ? projectFamilyName(source || project, projects)
      : window.last_project || t("Project information unavailable");
    const workspaceLabel = project?.lineage ? projectVariantLabel(project) : projectLabel;
    return (
      <button
        type="button"
        className={"window-work-row" + (windows.selectedKey === window.client_window_key ? " selected" : "")}
        aria-current={windows.selectedKey === window.client_window_key ? "true" : undefined}
        ref={windows.selectedKey === window.client_window_key ? selectedRowRef : undefined}
        onClick={() => windows.select(window.client_window_key)}
        key={window.client_window_key}
        data-testid={"work-window-row-" + window.client_window_key}
      >
        <span className={"work-state-dot " + (window.active_count ? "running" : "recent")} />
        <span className="window-work-row-main">
          <strong>{workspaceLabel}</strong>
          <small title={window.client_window_key}>{t("Window")} · {shortId(window.client_window_key, 6, 4)}</small>
          <small>{project?.lineage ? projectLabel + " · " : ""}{activity}</small>
          <small title={displayProjectPath(project?.path || source?.path)}>
            {[
              project?.client_id || t("Runner unavailable"),
              displayProjectPath(project?.path || source?.path),
            ].filter(Boolean).join(" · ")}
          </small>
        </span>
        <span className="window-work-row-side">
          {window.active_count > 0
            ? <em>{window.active_count} {t("active")}</em>
            : <time title={absoluteTime(observedAt)}>{relativeTime(observedAt)}</time>}
        </span>
      </button>
    );
  };

  return (
    <div className="work-layout window-primary-workbench" data-testid="window-primary-workbench">
      <aside className="work-list-panel window-primary-list">
        <div className="work-list-header">
          <div><span className="eyebrow">{t("Live work")}</span><h1>{t("Work")}</h1></div>
          <WorkSurfaceSwitch surface={surface} onSurfaceChange={onSurfaceChange} language={language} />
        </div>
        <div className="window-work-filters">
          <button type="button" className="text-button" onClick={windows.refresh}>{t("Refresh")}</button>
          <ProjectPicker
            label={t("Projects")}
            allLabel={t("All projects")}
            emptyLabel={t("No matching projects")}
            searchLabel={t("Search projects")}
            value={projectFamily}
            onChange={setProjectFamily}
            options={families.map((row) => ({
              value: row.id,
              label: row.label,
              detail: row.windowCount
                ? [
                    row.activeWindowCount ? row.activeWindowCount + " " + t("active") : "",
                    row.windowCount + " " + t(row.windowCount === 1 ? "Window" : "Windows"),
                    row.runner,
                  ].filter(Boolean).join(" · ")
                : t("No Window activity") + " · " + row.runner,
            }))}
          />
          <TextInput
            type="search"
            aria-label={t("Search Windows")}
            leftSection={<Search size={14} />}
            value={search}
            onChange={(event) => setSearch(event.currentTarget.value)}
            placeholder={t("Search windows or projects…")}
          />
        </div>
        <div className="work-list-scroll" ref={listRef}>
          {selectedIsPinned && <div className="window-current-selection">
            <div className="window-list-summary"><span>{t("Current Window")}</span></div>
            {selectedNavigationRow ? renderWindow(selectedNavigationRow) : (
              <button className="window-work-row selected" type="button" aria-current="true"
                ref={selectedRowRef} data-testid={"work-window-row-" + windows.selectedKey}
                onClick={windows.refresh}>
                <Monitor size={14} />
                <span className="window-work-row-main">
                  <strong title={windows.selectedKey}>{t("Window")} · {shortId(windows.selectedKey, 6, 4)}</strong>
                  <small role="status">{t(windows.detailAvailability === "error" || windows.detailAvailability === "denied" ? "Window activity unavailable" : "Loading recent activity…")}</small>
                </span>
              </button>
            )}
          </div>}
          <div className="window-list-summary">
            <span>{filtered.length} {t("Windows")}</span>
            <small>{filtered.filter((row) => row.active_count > 0).length} {t("active")}</small>
          </div>
          {filtered.map(row => renderWindow(row.client_window_key === windows.selectedKey && selectedNavigationRow ? selectedNavigationRow : row))}
          {windows.availability === "loading" && <div className="empty-inline">{t("Loading Window activity…")}</div>}
          {windows.availability === "stale" && <div className="inventory-note">{t("Window activity refresh failed; showing previous observations.")}</div>}
          {(windows.availability === "error" || windows.availability === "denied") && <div className="empty-inline">{t("Window activity unavailable")}</div>}
          {windows.availability === "available" && !filtered.length && <div className="empty-panel"><Monitor size={18} /><strong>{t("No matching Windows")}</strong></div>}
          {windows.truncated && <div className="inventory-note">{t("Window inventory is bounded; not all observed Windows are loaded.")}</div>}
        </div>
      </aside>

      <main className="window-work-main ui-workbench-surface">
        {hasSelectedWindow ? (
          <>
            <header className="window-work-header">
              <div>
                <div className="breadcrumbs">
                  {currentProject?.lineage ? (
                    <><span>{currentProjectName || t("Project")}</span><span>/</span><span>{t("Workspace")}</span></>
                  ) : (
                    <span>{t("Project")}</span>
                  )}
                </div>
                <h2>{currentWorkspaceName || currentActivity || t("Window")}</h2>
                <div className="window-header-facts">
                  <span className="window-header-fact" title={windows.selectedKey} data-testid="current-window-identity">
                    <Monitor size={14} /><small>{t("Window")}</small>
                    <CopyIdentity key={windows.selectedKey} value={windows.selectedKey} label={t("Window")} language={language} />
                  </span>
                  {currentMachine && (
                    <span className="window-header-fact" title={currentMachine}>
                      <Server size={14} />
                      <small>{t("Machine")}</small>
                      <strong>{currentMachine}</strong>
                    </span>
                  )}
                  {currentDirectory && (
                    <span className="window-header-fact" title={currentDirectory}>
                      <FolderOpen size={14} />
                      <small>{t("Directory")}</small>
                      <strong>{currentDirectory}</strong>
                    </span>
                  )}
                  {currentProjectAddress && (
                    <span className="window-header-fact" title={currentProjectAddress}>
                      <Box size={14} />
                      <small>{t("Project address")}</small>
                      <strong>{currentProjectAddress}</strong>
                    </span>
                  )}
                  {firstObservedAt && (
                    <span className="window-header-fact" title={absoluteTime(firstObservedAt)}>
                      <Clock3 size={14} />
                      <small>{t("First active")}</small>
                      <strong>{absoluteTime(firstObservedAt)}</strong>
                    </span>
                  )}
                  {currentActivity && (
                    <span className="window-header-fact window-header-activity">
                      <CircleDot size={12} />
                      <small>{t("Latest activity")}</small>
                      <strong>{currentActivity}{lastObservedAt ? " · " + clockTime(lastObservedAt) : ""}</strong>
                    </span>
                  )}
                </div>
              </div>
              <span className={"quiet-pill " + (isActive ? "running" : "")}>
                <CircleDot size={11} />
                {isActive
                  ? visibleActiveCount + " " + t("active")
                  : t("Idle") + " · " + relativeTime(lastObservedAt)}
              </span>
            </header>

            <div className="session-view-tabs window-center-tabs" role="tablist" aria-label={t("Window views")}>
              <button
                id="window-activity-tab"
                role="tab"
                aria-selected={centerTab === "window"}
                aria-controls="window-activity-panel"
                className={centerTab === "window" ? "active" : ""}
                type="button"
                onClick={() => setCenterTab("window")}
              >
                <Monitor size={14} />
                {t("Window")}
                <span>{visibleActivityCount}</span>
              </button>
              <button
                id="window-collaboration-tab"
                role="tab"
                aria-selected={centerTab === "collaboration"}
                aria-controls="window-collaboration-panel"
                className={centerTab === "collaboration" ? "active" : ""}
                type="button"
                onClick={() => setCenterTab("collaboration")}
              >
                <MessageSquare size={14} />
                {t("Collaboration")}
              </button>
            </div>

            <div
              id="window-activity-panel"
              className="window-work-scroll session-center-pane"
              role="tabpanel"
              aria-labelledby="window-activity-tab"
              hidden={centerTab !== "window"}
            >
              {windows.detailAvailability === "stale" && <div className="inventory-note">{t("Window activity refresh failed; showing previous observations.")}</div>}
              {windows.detailHydrating && detail?.detail_level !== "full" && (
                <div className="window-progressive-note">{t("Loading history…")}</div>
              )}
              {detail ? (
                <WindowActivityFeed
                  key={detail.client_window_key}
                  detail={detail}
                  projects={projects}
                  language={language}
                  selectedSessionId={selectedSessionId}
                  onSelectSession={setSelectedSessionId}
                  onOpenSessionRecord={onOpenSessionRecord}
                />
              ) : (
                <div className="empty-inline">
                  {t(
                    windows.detailAvailability === "error" || windows.detailAvailability === "denied"
                      ? "Window activity unavailable"
                      : "Loading recent activity…",
                  )}
                </div>
              )}
            </div>

            <div
              id="window-collaboration-panel"
              className="window-work-scroll session-center-pane window-collaboration-pane"
              role="tabpanel"
              aria-labelledby="window-collaboration-tab"
              hidden={centerTab !== "collaboration"}
            >
              {windows.selectedKey && (
                <WindowCollaboration
                  key={windows.selectedKey}
                  client={client}
                  windowKey={windows.selectedKey}
                  active={centerTab === "collaboration"}
                  selectedSessionId={selectedSessionId}
                  language={language}
                  onUnauthorized={onUnauthorized}
                />
              )}
            </div>
          </>
        ) : (
          <div className="empty-work">
            <Monitor size={22} />
            <h2>{t(windows.detailAvailability === "loading" ? "Loading Window activity…" : windows.detailAvailability === "error" || windows.detailAvailability === "denied" ? "Window activity unavailable" : "Select a window")}</h2>
            <p>{t("Choose a window to see its tool calls.")}</p>
          </div>
        )}
      </main>
    </div>
  );
}
