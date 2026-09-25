import {
  CircleDot,
  Monitor,
  Search,
} from "lucide-react";
import { TextInput } from "@mantine/core";
import { useEffect, useMemo, useState } from "react";
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
import { absoluteTime, relativeTime, shortId } from "../model/format.js";
import type { ProjectRow, WindowSummary } from "../model/types.js";
import type { SessionLocation } from "../state/useSessionWorkspace.js";
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
  onOpenSession: (location: SessionLocation) => void;
  onUnauthorized: () => void;
  requestedWindowKey?: string;
  onRequestedWindowConsumed?: () => void;
};

type ProjectFamily = {
  id: string;
  label: string;
  detail: string;
  projectIds: Set<string>;
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

function buildProjectFamilies(projects: ProjectRow[]): ProjectFamily[] {
  const groups = new Map<string, ProjectRow[]>();
  for (const project of projects) {
    const id = projectFamilyId(project);
    const rows = groups.get(id) || [];
    rows.push(project);
    groups.set(id, rows);
  }
  return [...groups.entries()].map(([id, rows]) => {
    const representative = rows.find((row) => row.id === id) || rows[0];
    const worktrees = rows.filter((row) => row.lineage).length;
    return {
      id,
      label: projectFamilyName(representative, projects),
      detail: representative.client_id + (worktrees ? " · " + worktrees + " worktree" + (worktrees === 1 ? "" : "s") : ""),
      projectIds: new Set(rows.map((row) => row.id)),
    };
  }).sort((a, b) => a.label.localeCompare(b.label));
}

export function WindowWorkbench({
  client,
  language,
  projects,
  surface,
  onSurfaceChange,
  onOpenSession,
  onUnauthorized,
  requestedWindowKey,
  onRequestedWindowConsumed,
}: Props) {
  const t = (value: string) => translate(value, language);
  const windows = useWindowWorkspace(client, true, onUnauthorized, { refreshMs: 3_000, loadDetail: true });
  const [tab, setTab] = useState<"activity" | "collaboration">("activity");
  const [search, setSearch] = useState("");
  const [projectFamily, setProjectFamily] = useState("");
  const families = useMemo(() => buildProjectFamilies(projects), [projects]);

  useEffect(() => {
    if (!requestedWindowKey) return;
    if (!windows.windows.some((window) => window.client_window_key === requestedWindowKey)) return;
    windows.select(requestedWindowKey);
    onRequestedWindowConsumed?.();
  }, [onRequestedWindowConsumed, requestedWindowKey, windows.windows]);

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
  const selectedSummary = windows.windows.find((row) => row.client_window_key === windows.selectedKey);
  const activeRequest = detail?.active_requests.slice().sort((a, b) => b.started_at_ms - a.started_at_ms)[0];
  const currentProjectId = activeRequest?.project || selectedSummary?.last_project || detail?.activity[0]?.project;
  const currentProject = projectFor(projects, currentProjectId);
  const sourceProject = currentProject ? projectFor(projects, sourceProjectRuntimeId(currentProject)) : undefined;
  const currentActivity = activeRequest?.tool_name ||
    selectedSummary?.last_activity_name ||
    detail?.activity[0]?.activity_presentation ||
    detail?.activity[0]?.tool_name ||
    detail?.activity[0]?.method;
  const lastObservedAt = detail?.last_seen_at_ms || selectedSummary?.last_seen_at_ms;
  const isActive = Boolean(detail?.active_count || selectedSummary?.active_count);

  return (
    <div className="work-layout window-primary-workbench" data-testid="window-primary-workbench">
      <aside className="work-list-panel window-primary-list">
        <div className="work-list-header">
          <div><span className="eyebrow">{t("Live work")}</span><h1>{t("Work")}</h1></div>
          <WorkSurfaceSwitch surface={surface} onSurfaceChange={onSurfaceChange} language={language} />
        </div>
        <div className="window-work-filters">
          <ProjectPicker
            label={t("Project filter")}
            allLabel={t("All project workspaces")}
            emptyLabel={t("No matching projects")}
            searchLabel={t("Search projects")}
            value={projectFamily}
            onChange={setProjectFamily}
            options={families.map((row) => ({ value: row.id, label: row.label, detail: row.detail }))}
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
        <div className="work-list-scroll">
          <div className="window-list-summary">
            <span>{filtered.length} {t("Windows")}</span>
            <small>{filtered.filter((row) => row.active_count > 0).length} {t("active")}</small>
          </div>
          {filtered.map((window) => {
            const project = projectFor(projects, window.last_project);
            const source = project ? projectFor(projects, sourceProjectRuntimeId(project)) : undefined;
            const title = window.last_activity_name || t("Window");
            const observedAt = windowObservedAt(window);
            const projectLabel = project
              ? projectFamilyName(source || project, projects)
              : window.last_project || t("Project information unavailable");
            return (
              <button
                type="button"
                className={"window-work-row" + (windows.selectedKey === window.client_window_key ? " selected" : "")}
                onClick={() => windows.select(window.client_window_key)}
                key={window.client_window_key}
                data-testid={"work-window-row-" + window.client_window_key}
              >
                <span className={"work-state-dot " + (window.active_count ? "running" : "recent")} />
                <span className="window-work-row-main">
                  <strong>{title}</strong>
                  <small>{projectLabel}{project?.lineage ? " · " + projectVariantLabel(project) : ""}</small>
                  <small>{project?.client_id || t("Runner unavailable")} · {t("Window")} {shortId(window.client_window_key)}</small>
                </span>
                <span className="window-work-row-side">
                  {window.active_count > 0
                    ? <em>{window.active_count} {t("active")}</em>
                    : <time title={absoluteTime(observedAt)}>{relativeTime(observedAt)}</time>}
                </span>
              </button>
            );
          })}
          {windows.availability === "loading" && <div className="empty-inline">{t("Loading Window activity…")}</div>}
          {windows.availability === "stale" && <div className="inventory-note">{t("Window activity refresh failed; showing previous observations.")}</div>}
          {(windows.availability === "error" || windows.availability === "denied") && <div className="empty-inline">{t("Window activity unavailable")}</div>}
          {windows.availability === "available" && !filtered.length && <div className="empty-panel"><Monitor size={18} /><strong>{t("No matching Windows")}</strong></div>}
          {windows.truncated && <div className="inventory-note">{t("Window inventory is bounded; not all observed Windows are loaded.")}</div>}
        </div>
      </aside>

      <main className="window-work-main ui-workbench-surface">
        {detail ? (
          <>
            <header className="window-work-header">
              <div>
                <div className="breadcrumbs"><span>{t("Window")}</span><span>/</span><span title={detail.client_window_key}>{shortId(detail.client_window_key)}</span></div>
                <h2>{currentActivity || t("Window")}</h2>
                <p>
                  {currentProject ? projectFamilyName(sourceProject || currentProject, projects) : t("Project information unavailable")}
                  {" · "}{t("Last activity")} {absoluteTime(lastObservedAt)}
                </p>
              </div>
              <span className={"quiet-pill " + (isActive ? "running" : "")}>
                <CircleDot size={11} /> {isActive ? detail.active_count + " " + t("active") : t("Idle")}
              </span>
            </header>

            <div className="session-view-tabs" role="tablist" aria-label={t("Work view")}>
              {(["activity", "collaboration"] as const).map((value) => (
                <button key={value} type="button" role="tab" id={"window-" + value + "-tab"}
                  aria-selected={tab === value} aria-controls={"window-" + value + "-panel"}
                  className={tab === value ? "active" : ""} onClick={() => setTab(value)}>
                  {t(value === "activity" ? "Window activity" : "Window collaboration")}
                </button>
              ))}
            </div>
            <div className="window-work-scroll" role="tabpanel" id="window-activity-panel" aria-labelledby="window-activity-tab" hidden={tab !== "activity"}>
              {windows.detailAvailability === "stale" && <div className="inventory-note">{t("Window activity refresh failed; showing previous observations.")}</div>}
              {currentProject && <div className="window-work-location">
                <span>{t(projectVariantLabel(currentProject))}</span>
                <span>{currentProject.client_id}</span>
                <span title={displayProjectPath(currentProject.path)}>{displayProjectPath(currentProject.path)}</span>
              </div>}
              <WindowActivityFeed
                key={detail.client_window_key}
                detail={detail}
                projects={projects}
                language={language}
                onOpenSession={onOpenSession}
              />
            </div>
            <section className="window-collaboration-pane" role="tabpanel" id="window-collaboration-panel" aria-labelledby="window-collaboration-tab" hidden={tab !== "collaboration"}>
              <WindowCollaboration key={detail.client_window_key} client={client} detail={detail} projects={projects} language={language} onUnauthorized={onUnauthorized} onOpenSession={onOpenSession} />
            </section>
          </>
        ) : (
          <div className="empty-work">
            <Monitor size={22} />
            <h2>{t(windows.detailAvailability === "loading" ? "Loading Window activity…" : windows.detailAvailability === "error" || windows.detailAvailability === "denied" ? "Window activity unavailable" : "Select a window")}</h2>
            <p>{t("Choose a window to see its activity and collaboration.")}</p>
          </div>
        )}
      </main>
    </div>
  );
}
