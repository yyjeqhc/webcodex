import {
  Activity,
  ArrowUpRight,
  CircleDot,
  Clock3,
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
import { absoluteTime, projectDisplayName, relativeTime, shortId } from "../model/format.js";
import type { ProjectRow, WindowSummary } from "../model/types.js";
import type { SessionLocation } from "../state/useSessionWorkspace.js";
import { useWindowWorkspace } from "../state/useWindowWorkspace.js";
import { WorkSurfaceSwitch, type WorkSurface } from "./GoalWorkbench.js";
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
            placeholder={t("Search active tool, Project, Runner, or Window ID…")}
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
            const title = window.last_activity_name || t("Observed activity");
            const observedAt = windowObservedAt(window);
            const projectLabel = project
              ? projectFamilyName(source || project, projects)
              : window.last_project || t("No Project evidence");
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
                  <small>{project?.client_id || t("Runner not observed")} · Window {shortId(window.client_window_key)}</small>
                </span>
                <span className="window-work-row-side">
                  {window.active_count > 0
                    ? <em>{window.active_count} {t("active")}</em>
                    : <time title={absoluteTime(observedAt)}>{relativeTime(observedAt)}</time>}
                  {!window.last_activity_meaningful && window.last_activity_name && <small>{t("observe")}</small>}
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
        {windows.detailAvailability === "stale" && <div className="inventory-note">{t("Window activity refresh failed; showing previous observations.")}</div>}
        {detail ? (
          <>
            <header className="window-work-header">
              <div>
                <div className="breadcrumbs"><span>{t("Window")}</span><span>/</span><span title={detail.client_window_key}>{shortId(detail.client_window_key)}</span></div>
                <h2>{currentActivity || t("Observed Window")}</h2>
                <p>
                  {isActive ? t("WebCodex request active now") : t("Inactive for") + " " + relativeTime(lastObservedAt)}
                  {" · "}{t("last observed")} {absoluteTime(lastObservedAt)}
                </p>
              </div>
              <span className={"quiet-pill " + (isActive ? "running" : "")}>
                <CircleDot size={11} /> {isActive ? detail.active_count + " " + t("active") : t("Idle")}
              </span>
            </header>

            <div className="window-work-scroll">
              <section className="window-summary-strip" aria-label={t("Window summary")}>
                <div><span>{t("Project")}</span><strong>{currentProject ? projectFamilyName(sourceProject || currentProject, projects) : t("No Project evidence")}</strong><small>{currentProject?.lineage ? projectVariantLabel(currentProject) : currentProject ? t("Primary workspace") : "—"}</small></div>
                <div><span>{t("Runner")}</span><strong>{currentProject?.client_id || "—"}</strong><small>{currentProject ? displayProjectPath(currentProject.path) : t("Not observed")}</small></div>
                <div><span>{t("Window activity")}</span><strong>{isActive ? t("Active") : relativeTime(lastObservedAt)}</strong><small>{absoluteTime(lastObservedAt)}</small></div>
              </section>

              <WindowActivityFeed
                key={detail.client_window_key}
                detail={detail}
                projects={projects}
                language={language}
                onOpenSession={onOpenSession}
              />

              <details className="window-relations-disclosure window-linked-evidence">
                <summary>
                  <span><Activity size={15} /><strong>{t("Linked Session evidence")}</strong></span>
                  <span className="count-badge">{detail.sessions_returned}</span>
                </summary>
                <div className="linked-session-list">
                  {detail.linked_sessions.map((session) => {
                    const project = projectFor(projects, session.project);
                    return (
                      <button
                        type="button"
                        className="linked-session-row"
                        key={session.workflow_session_id}
                        disabled={!project}
                        onClick={() => project && onOpenSession({
                          projectId: project.id,
                          projectName: projectDisplayName(project.name, project.id, project.path),
                          runner: project.client_id,
                          sessionId: session.workflow_session_id,
                        })}
                      >
                        <span className="session-relation-dot" />
                        <span className="project-session-main"><strong>{session.title || shortId(session.workflow_session_id)}</strong><small>{project?.name || session.project || t("Project not exposed in relation")}</small></span>
                        <span className="relation-kind">{session.relations.join(" · ") || t("linked")}</span>
                        <time>{absoluteTime(session.last_linked_at_ms)}</time>
                        {project && <ArrowUpRight size={14} />}
                      </button>
                    );
                  })}
                  {!detail.linked_sessions.length && <div className="empty-inline">{t("No explicit Session link")}</div>}
                </div>
              </details>
            </div>
          </>
        ) : (
          <div className="empty-work">
            <Monitor size={22} />
            <h2>{t("Select an observed Window")}</h2>
            <p>{t("Window activity is shown even when no Workflow Session exists.")}</p>
          </div>
        )}
      </main>

      <aside className="inspector window-work-inspector" aria-label={t("Window context")}>
        <div className="inspector-header"><div><strong>{t("Window context")}</strong></div></div>
        <div className="inspector-content">
          {detail && (
            <>
              <div className="context-hero">
                <div className={"context-kicker " + (isActive ? "active" : "")}><Clock3 size={13} />{isActive ? t("ACTIVE") : t("IDLE")}</div>
                <strong>{currentActivity || t("Observed Window")}</strong>
                <p>{isActive ? t("A WebCodex request is currently executing in this Window.") : t("No WebCodex request for") + " " + relativeTime(lastObservedAt) + "."}</p>
              </div>
              <section className="inspector-section">
                <h3>{t("Current work")}</h3>
                <div className="fact-list">
                  <div><span>{t("Window")}</span><strong title={detail.client_window_key}>{shortId(detail.client_window_key)}</strong></div>
                  <div><span>{t("Project")}</span><strong>{currentProject ? projectFamilyName(sourceProject || currentProject, projects) : "—"}</strong></div>
                  <div><span>{t("Workspace")}</span><strong title={currentProject ? displayProjectPath(currentProject.path) : ""}>{currentProject ? projectVariantLabel(currentProject) : "—"}</strong></div>
                  <div><span>{t("Runner")}</span><strong>{currentProject?.client_id || "—"}</strong></div>
                  <div><span>{t("Active")}</span><strong>{detail.active_count}</strong></div>
                  <div><span>{t("Last activity")}</span><strong>{absoluteTime(lastObservedAt)}</strong></div>
                </div>
              </section>
              <section className="inspector-section">
                <h3>{t("Window collaboration")}</h3>
                <div className="fact-list">
                  <div><span>{t("Peer identity")}</span><strong title={peerId(detail.client_window_key)}>{shortId(peerId(detail.client_window_key), 12, 6)}</strong></div>
                </div>
                <p className="window-relation-note">{t("Peer identity belongs to this Window directly and does not require a Workflow Session.")}</p>
              </section>
              <section className="inspector-section">
                <h3>{t("Relations")}</h3>
                <div className="fact-list">
                  <div><span>{t("Sessions")}</span><strong>{detail.sessions_returned}</strong></div>
                  <div><span>{t("Source")}</span><strong>{detail.source}</strong></div>
                </div>
                <p className="window-relation-note">{t("Session links are optional evidence. Window activity remains visible without them.")}</p>
              </section>
            </>
          )}
        </div>
      </aside>
    </div>
  );
}
