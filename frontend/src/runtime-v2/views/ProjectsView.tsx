import {
  displayProjectPath,
  projectFamilyId,
  projectFamilyName,
  projectVariantLabel,
} from "../../ui/projectPresentation.js";
import {
  Activity,
  ArrowUpRight,
  Folder,
  GitBranch,
  Monitor,
  Plus,
  Search,
} from "lucide-react";
import { Button, Modal, Select, TextInput } from "@mantine/core";
import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { ScopePicker } from "../../ui/ProjectPicker.js";
import type { RuntimeV2Client } from "../api/client.js";
import { registerProject } from "../api/projects.js";
import { PageHeader } from "../components/ui/PageHeader.js";
import { absoluteTime, projectDisplayName, relativeTime, shortId } from "../model/format.js";
import { phaseFromSession, workBucket } from "../model/work.js";
import type { ProjectRow, RunnerSummary, WindowSummary } from "../model/types.js";
import { useProjectSessions } from "../state/useProjectSessions.js";
import { useProjects } from "../state/useProjects.js";
import { useWindowWorkspace } from "../state/useWindowWorkspace.js";
import type { SessionLocation } from "../state/useSessionWorkspace.js";

type Props = {
  client: RuntimeV2Client;
  language: RuntimeLanguage;
  runners: RunnerSummary[];
  onOpenSession: (location: SessionLocation) => void;
  onOpenWindow: (windowKey: string) => void;
  onUnauthorized: () => void;
};

type ProjectFamily = {
  id: string;
  name: string;
  clientId: string;
  primary: ProjectRow;
  workspaces: ProjectRow[];
};

function projectRetainedSessionCount(project: ProjectRow): number {
  return project.sessions?.retained_sessions ?? project.sessions?.returned_sessions ?? 0;
}

function buildProjectFamilies(projects: ProjectRow[]): ProjectFamily[] {
  const grouped = new Map<string, ProjectRow[]>();
  for (const project of projects) {
    const id = projectFamilyId(project);
    const rows = grouped.get(id) || [];
    rows.push(project);
    grouped.set(id, rows);
  }
  return [...grouped.entries()]
    .map(([id, workspaces]) => {
      const primary = workspaces.find((project) => project.id === id) || workspaces.find((project) => !project.lineage) || workspaces[0];
      return {
        id,
        name: projectFamilyName(primary, projects),
        clientId: primary.client_id,
        primary,
        workspaces: workspaces.slice().sort((left, right) =>
          Number(Boolean(left.lineage)) - Number(Boolean(right.lineage)) ||
          projectVariantLabel(left).localeCompare(projectVariantLabel(right))),
      };
    })
    .sort((left, right) => left.name.localeCompare(right.name) || left.clientId.localeCompare(right.clientId));
}

function windowTime(window: WindowSummary): number {
  return window.last_seen_at_ms || window.last_tool_call_at_ms || window.last_meaningful_activity_at_ms || 0;
}

export function ProjectsView({ client, language, runners, onOpenSession, onOpenWindow, onUnauthorized }: Props) {
  const t = (value: string) => translate(value, language);
  const projectsState = useProjects(client, true, onUnauthorized);
  const windows = useWindowWorkspace(client, true, onUnauthorized, { refreshMs: 5_000, loadDetail: false });
  const [selectedFamilyId, setSelectedFamilyId] = useState("");
  const [selectedProjectId, setSelectedProjectId] = useState("");
  const [addOpen, setAddOpen] = useState(false);
  const [addRunner, setAddRunner] = useState("");
  const [addPath, setAddPath] = useState("");
  const [addStatus, setAddStatus] = useState("");
  const [addPending, setAddPending] = useState(false);
  const addRequest = useRef<AbortController | null>(null);
  const detailsSection = useRef<HTMLElement | null>(null);

  const families = useMemo(() => buildProjectFamilies(projectsState.projects), [projectsState.projects]);
  const selectedFamily = useMemo(
    () => families.find((family) => family.id === selectedFamilyId) || families[0],
    [families, selectedFamilyId],
  );
  const selectedProject = useMemo(
    () => selectedFamily?.workspaces.find((project) => project.id === selectedProjectId) || selectedFamily?.primary,
    [selectedFamily, selectedProjectId],
  );
  const sessionsState = useProjectSessions(client, Boolean(selectedProject), selectedProject?.id || "", onUnauthorized);

  const familyWindows = useMemo(() => {
    if (!selectedFamily) return [];
    const workspaceIds = new Set(selectedFamily.workspaces.map((project) => project.id));
    return windows.windows
      .filter((window) => Boolean(window.last_project && workspaceIds.has(window.last_project)))
      .sort((left, right) =>
        Number(right.active_count > 0) - Number(left.active_count > 0) ||
        windowTime(right) - windowTime(left));
  }, [selectedFamily, windows.windows]);

  useEffect(() => {
    if (!selectedFamily) {
      if (selectedFamilyId) setSelectedFamilyId("");
      if (selectedProjectId) setSelectedProjectId("");
      return;
    }
    if (selectedFamily.id !== selectedFamilyId) setSelectedFamilyId(selectedFamily.id);
    if (!selectedProject || selectedProject.id !== selectedProjectId) setSelectedProjectId(selectedFamily.primary.id);
  }, [selectedFamily?.id, selectedFamily?.primary.id, selectedFamilyId, selectedProject?.id, selectedProjectId]);

  useEffect(() => {
    if (!addRunner && runners.length) setAddRunner(projectsState.runner || runners[0].client_id);
  }, [addRunner, projectsState.runner, runners]);

  useEffect(() => () => addRequest.current?.abort(), []);

  const openFamily = (family: ProjectFamily) => {
    setSelectedFamilyId(family.id);
    setSelectedProjectId(family.primary.id);
    window.setTimeout(() => detailsSection.current?.scrollIntoView?.({ behavior: "smooth", block: "start" }), 0);
  };

  const submitAddProject = async (event: FormEvent) => {
    event.preventDefault();
    const path = addPath.trim();
    if (addPending || !addRunner || !path) return;
    const controller = new AbortController();
    addRequest.current?.abort();
    addRequest.current = controller;
    setAddPending(true);
    setAddStatus(t("Adding project…"));
    try {
      const response = await registerProject(client, addRunner, path, controller.signal);
      if (addRequest.current !== controller || !response) return;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.ok && response.data?.success === true) {
        setAddOpen(false);
        setAddPath("");
        setAddStatus("");
        projectsState.refresh();
        return;
      }
      setAddStatus(t(response.status === 0
        ? "The result could not be confirmed. Refresh Projects before trying again."
        : "Project could not be added. Check the folder and Runner access."));
    } finally {
      if (addRequest.current === controller) addRequest.current = null;
      setAddPending(false);
    }
  };

  return (
    <main className="page ui-workbench-surface">
      <PageHeader title={t("Projects")} actions={
        <>
          <span className="quiet-pill">
            {projectsState.availability === "stale" ? t("stale") :
              projectsState.availability === "loading" ? t("Loading projects…") :
                String(families.length) + " " + t("projects")}
          </span>
          <Button className="runtime-primary" type="button" leftSection={<Plus size={16} />} onClick={() => { setAddOpen(true); setAddStatus(""); }}>{t("Add Project")}</Button>
        </>
      } />

      <div className="filter-bar project-filter-bar">
        <TextInput className="project-filter-input" leftSection={<Search size={16} />}
          aria-label={t("Search projects")}
          placeholder={t("Filter by Project name, Runner, or workspace path")}
          value={projectsState.query}
          onChange={(event) => projectsState.setQuery(event.currentTarget.value)}
        />
        <ScopePicker kind="runner" className="runner-picker" label={t("Runner")} allLabel={t("All Runners")} emptyLabel={t("No matching Runners")} searchLabel={t("Search Runners")}
          value={projectsState.runner} onChange={projectsState.setRunner} options={runners.map((runner) => ({ value: runner.client_id, label: runner.client_id }))} />
      </div>

      {projectsState.availability === "denied" && (
        <div className="empty-panel wide"><strong>{t("Projects unavailable. Refresh to try again.")}</strong></div>
      )}

      <div className="project-grid" data-testid="project-grid">
        {families.map((family) => {
          const git = projectsState.gitByProject.get(family.primary.id);
          const retained = family.workspaces.reduce((sum, project) => sum + projectRetainedSessionCount(project), 0);
          const activeSessions = family.workspaces.reduce((sum, project) => sum + (project.sessions?.active_sessions ?? 0), 0);
          const workspaceIds = new Set(family.workspaces.map((project) => project.id));
          const observedWindows = windows.windows.filter((window) => Boolean(window.last_project && workspaceIds.has(window.last_project)));
          const activeWindows = observedWindows.filter((window) => window.active_count > 0).length;
          const worktreeCount = family.workspaces.filter((project) => project.lineage).length;
          const selected = selectedFamily?.id === family.id;
          return (
            <button
              className={"project-card ui-entity-row" + (selected ? " selected" : "")}
              key={family.id}
              type="button"
              onClick={() => openFamily(family)}
              data-testid={"project-card-" + family.primary.id}
            >
              <div className="project-card-head">
                <span className="project-icon"><Folder size={18} /></span>
                <span>
                  <strong title={family.id}>{family.name}</strong>
                  <small title={displayProjectPath(family.primary.path) || family.primary.id}>{family.clientId} · {displayProjectPath(family.primary.path) || t("workspace path unavailable")}</small>
                </span>
                <span className={"status-pill " + (family.workspaces.some((project) => project.connected) ? "good" : "warn")}>
                  {family.workspaces.some((project) => project.connected) ? t("online") : t("offline")}
                </span>
              </div>
              <div className="project-card-body project-family-card-body">
                <div><GitBranch size={14} /><span title={String(git?.branch || "")}>{git?.branch || t("Not checked")}</span></div>
                <div><Folder size={14} /><span>{family.workspaces.length} {t("workspaces")} · {worktreeCount} {t("worktrees")}</span></div>
                <div><Monitor size={14} /><span>{observedWindows.length} {t("Windows")} · {activeWindows} {t("active")}</span></div>
                <div><Activity size={14} /><span>{retained} {t("Session links")} · {activeSessions} {t("active")}</span></div>
              </div>
              <span className="project-open">{t("View workspaces and activity")} <ArrowUpRight size={14} /></span>
            </button>
          );
        })}
      </div>

      {projectsState.availability === "available" && !families.length && (
        <div className="empty-panel wide"><Folder size={19} /><strong>{t("No matching projects")}</strong></div>
      )}
      {projectsState.truncated && (
        <div className="inventory-note wide">{t("Project inventory is bounded. Narrow the search to find omitted Projects.")}</div>
      )}

      {selectedFamily && selectedProject && (
        <section className="project-family-details" ref={detailsSection}>
          <div className="section-heading project-family-heading">
            <div>
              <h2>{selectedFamily.name}</h2>
              <p>{selectedFamily.clientId} · {selectedFamily.workspaces.length} {t("workspaces")}</p>
            </div>
          </div>

          <section className="project-detail-block">
            <div className="section-heading">
              <div><h2>{t("Window activity")}</h2><p>{t("Recent Windows whose latest Project evidence belongs to this project.")}</p></div>
              <span className="quiet-pill">{familyWindows.length} {t("Windows")}</span>
            </div>
            <div className="project-window-list">
              {familyWindows.map((window) => {
                const workspace = selectedFamily.workspaces.find((project) => project.id === window.last_project);
                return (
                  <button className="project-window-row ui-entity-row" type="button" key={window.client_window_key} onClick={() => onOpenWindow(window.client_window_key)}>
                    <span className={"work-state-dot " + (window.active_count ? "running" : "recent")} />
                    <span className="project-session-main">
                      <strong>{window.last_activity_name || t("Observed Window")}</strong>
                      <small>{workspace ? projectVariantLabel(workspace) : t("Workspace not observed")} · Window {shortId(window.client_window_key)}</small>
                    </span>
                    <span className={window.active_count ? "status-pill running" : "status-pill"}>{window.active_count ? t("Active") : t("Idle")}</span>
                    <time title={absoluteTime(windowTime(window))}>{relativeTime(windowTime(window))}</time>
                    <ArrowUpRight size={14} />
                  </button>
                );
              })}
              {windows.availability === "loading" && <div className="empty-inline">{t("Loading Window activity…")}</div>}
              {windows.availability === "available" && !familyWindows.length && <div className="empty-inline">{t("No Window activity observed for this project.")}</div>}
            </div>
          </section>

          <section className="project-detail-block">
            <div className="section-heading">
              <div><h2>{t("Workspaces")}</h2><p>{t("The primary checkout and its managed worktrees belong to one human project.")}</p></div>
            </div>
            <div className="project-workspace-list">
              {selectedFamily.workspaces.map((workspace) => {
                const git = projectsState.gitByProject.get(workspace.id);
                return (
                  <button
                    className={"project-workspace-row ui-entity-row" + (workspace.id === selectedProject.id ? " selected" : "")}
                    type="button"
                    key={workspace.id}
                    onClick={() => setSelectedProjectId(workspace.id)}
                    data-testid={"project-workspace-" + workspace.id}
                  >
                    <span className="project-icon compact"><Folder size={15} /></span>
                    <span className="project-session-main">
                      <strong>{projectVariantLabel(workspace)}</strong>
                      <small title={displayProjectPath(workspace.path)}>{displayProjectPath(workspace.path) || workspace.id}</small>
                    </span>
                    <span className="workspace-branch"><GitBranch size={13} />{git?.branch || t("Not checked")}</span>
                    <span className={"status-pill " + (workspace.connected ? "good" : "warn")}>{workspace.connected ? t("online") : t("offline")}</span>
                  </button>
                );
              })}
            </div>
          </section>

          <section className="project-sessions project-detail-block" data-testid="project-active-sessions">
            <div className="section-heading">
              <div>
                <h2>{projectVariantLabel(selectedProject)} · {t("Linked Session evidence")}</h2>
                <p>{t("Sessions are optional relations for this exact workspace, not the project identity.")}</p>
              </div>
              <span className="quiet-pill">{sessionsState.total} {t("Sessions")}</span>
            </div>
            <div className="session-table">
              {sessionsState.sessions.map((session) => {
                const bucket = workBucket(session);
                const linkedWindows = sessionsState.windowCountBySession.get(session.session_id);
                return (
                  <button
                    className="project-session-row ui-entity-row"
                    key={session.session_id}
                    type="button"
                    onClick={() => onOpenSession({
                      projectId: selectedProject.id,
                      projectName: projectDisplayName(selectedProject.name, selectedProject.id, selectedProject.path),
                      runner: selectedProject.client_id,
                      sessionId: session.session_id,
                    })}
                  >
                    <span className={"session-live-dot " + bucket} />
                    <span className="project-session-main">
                      <strong>{session.title}</strong>
                      <small>{phaseFromSession(session)}</small>
                    </span>
                    <span className="project-session-windows">
                      <Monitor size={13} />
                      {linkedWindows === undefined ? "…" : linkedWindows === null ? "—" : linkedWindows} {t("Windows")}
                    </span>
                    <span className={"status-pill " + (bucket === "attention" ? "warn" : bucket === "running" ? "running" : "good")}>
                      {t(bucket === "attention" ? "Needs attention" : bucket === "running" ? "Running" : session.lifecycle)}
                    </span>
                    <time>{relativeTime(session.updated_at)}</time>
                    <ArrowUpRight size={14} />
                  </button>
                );
              })}
              {sessionsState.availability === "loading" && <div className="empty-inline">{t("Loading Sessions…")}</div>}
              {sessionsState.availability === "available" && !sessionsState.sessions.length && <div className="empty-inline">{t("No Workflow Sessions retained for this workspace.")}</div>}
              {sessionsState.availability === "denied" && <div className="empty-inline">{t("Session list unavailable. Check access to this Project.")}</div>}
              {sessionsState.truncated && <div className="inventory-note">{t("Project Session inventory is bounded; older retained Sessions are not loaded here.")}</div>}
            </div>
          </section>
        </section>
      )}

      <Modal opened={addOpen} onClose={() => { if (!addPending) setAddOpen(false); }}
        closeOnEscape={!addPending} closeOnClickOutside={!addPending} withCloseButton={!addPending}
        closeButtonProps={{ "aria-label": t("Close") }}
        title={t("Add Project")} centered size="lg" className="runtime-project-modal">
        <form className="runtime-project-form" onSubmit={(event) => void submitAddProject(event)}>
          <Select required label={t("Runner")} value={addRunner} onChange={(value) => setAddRunner(value || "")}
            data={runners.map((runner) => ({ value: runner.client_id, label: runner.client_id }))} searchable comboboxProps={{ withinPortal: true }} />
          <TextInput required label={t("Project folder")} maxLength={4096} autoComplete="off" spellCheck={false}
            value={addPath} onChange={(event) => setAddPath(event.currentTarget.value)} placeholder={t("Absolute folder path on the selected Runner")} />
          {addStatus && <p className="modal-status" role={addStatus.includes("could") || addStatus.includes("无法") ? "alert" : "status"}>{addStatus}</p>}
          <div className="modal-actions"><Button type="button" variant="default" disabled={addPending} onClick={() => setAddOpen(false)}>{t("Cancel")}</Button>
            <Button className="runtime-primary" type="submit" loading={addPending} disabled={!addRunner || !addPath.trim()}>{t("Add Project")}</Button></div>
        </form>
      </Modal>
    </main>
  );
}
