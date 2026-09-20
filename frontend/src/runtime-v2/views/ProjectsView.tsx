import {
  ArrowUpRight,
  ChevronDown,
  Clock3,
  Folder,
  GitBranch,
  Monitor,
  Search,
} from "lucide-react";
import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { registerProject } from "../api/projects.js";
import { projectDisplayName, relativeTime } from "../model/format.js";
import { phaseFromSession, workBucket } from "../model/work.js";
import type { ProjectRow, RunnerSummary } from "../model/types.js";
import { useProjectSessions } from "../state/useProjectSessions.js";
import { useProjects } from "../state/useProjects.js";
import type { SessionLocation } from "../state/useSessionWorkspace.js";

type Props = {
  client: RuntimeV2Client;
  language: RuntimeLanguage;
  runners: RunnerSummary[];
  onOpenSession: (location: SessionLocation) => void;
  onUnauthorized: () => void;
};

function projectRetainedSessionCount(project: ProjectRow): number {
  return project.sessions?.retained_sessions ?? project.sessions?.returned_sessions ?? 0;
}

export function ProjectsView({ client, language, runners, onOpenSession, onUnauthorized }: Props) {
  const t = (value: string) => translate(value, language);
  const projectsState = useProjects(client, true, onUnauthorized);
  const [selectedProjectId, setSelectedProjectId] = useState("");
  const [addOpen, setAddOpen] = useState(false);
  const [addRunner, setAddRunner] = useState("");
  const [addPath, setAddPath] = useState("");
  const [addStatus, setAddStatus] = useState("");
  const [addPending, setAddPending] = useState(false);
  const addRequest = useRef<AbortController | null>(null);
  const sessionsSection = useRef<HTMLElement | null>(null);
  const selectedProject = useMemo(
    () => projectsState.projects.find((project) => project.id === selectedProjectId) || projectsState.projects[0],
    [projectsState.projects, selectedProjectId],
  );
  const sessionsState = useProjectSessions(client, Boolean(selectedProject), selectedProject?.id || "", onUnauthorized);

  useEffect(() => {
    if (selectedProject && selectedProject.id !== selectedProjectId) setSelectedProjectId(selectedProject.id);
    if (!selectedProject && selectedProjectId) setSelectedProjectId("");
  }, [selectedProject?.id, selectedProjectId]);

  useEffect(() => {
    if (!addRunner && runners.length) setAddRunner(projectsState.runner || runners[0].client_id);
  }, [addRunner, projectsState.runner, runners]);

  useEffect(() => () => addRequest.current?.abort(), []);

  const openProject = (projectId: string) => {
    setSelectedProjectId(projectId);
    window.setTimeout(() => sessionsSection.current?.scrollIntoView?.({ behavior: "smooth", block: "start" }), 0);
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
    <main className="page">
      <header className="page-heading">
        <div>
          <span className="eyebrow">{t("Repository workspace")}</span>
          <h1>{t("Projects")}</h1>
          <p>{t("Find a repository, then inspect the work Sessions currently active inside it.")}</p>
        </div>
        <div className="page-heading-actions">
          <span className="quiet-pill">
            {projectsState.availability === "stale" ? t("stale") :
              projectsState.availability === "loading" ? t("Loading projects…") :
                String(projectsState.total) + " " + t("projects")}
          </span>
          <button className="primary-button" type="button" onClick={() => { setAddOpen(true); setAddStatus(""); }}>{t("Add Project")}</button>
        </div>
      </header>

      <div className="filter-bar">
        <Search size={16} />
        <input
          aria-label={t("Search projects")}
          placeholder={t("Filter by Project name, id, Runner, or workspace path")}
          value={projectsState.query}
          onChange={(event) => projectsState.setQuery(event.target.value)}
        />
        <label className="filter-select">
          <span className="sr-only">{t("Runner")}</span>
          <select value={projectsState.runner} onChange={(event) => projectsState.setRunner(event.target.value)}>
            <option value="">{t("All Runners")}</option>
            {runners.map((runner) => <option key={runner.client_id} value={runner.client_id}>{runner.client_id}</option>)}
          </select>
          <ChevronDown size={14} />
        </label>
      </div>

      {projectsState.availability === "denied" && (
        <div className="empty-panel wide"><strong>{t("Projects unavailable. Refresh to try again.")}</strong></div>
      )}

      <div className="project-grid" data-testid="project-grid">
        {projectsState.projects.map((project) => {
          const git = projectsState.gitByProject.get(project.id);
          const retained = projectRetainedSessionCount(project);
          const active = project.sessions?.active_sessions ?? 0;
          const selected = selectedProject?.id === project.id;
          return (
            <button
              className={"project-card" + (selected ? " selected" : "")}
              key={project.id}
              type="button"
              onClick={() => openProject(project.id)}
              data-testid={"project-card-" + project.id}
            >
              <div className="project-card-head">
                <span className="project-icon"><Folder size={18} /></span>
                <span>
                  <strong title={project.id}>{projectDisplayName(project.name, project.id)}</strong>
                  <small>{project.client_id} · {project.project_ref || project.id}</small>
                </span>
                <span className={"status-pill " + (project.connected ? "good" : "warn")}>
                  {project.connected ? t("online") : t("offline")}
                </span>
              </div>
              <div className="project-card-body">
                <div><GitBranch size={14} /><span title={String(git?.branch || "")}>{git?.branch || t("Not checked")}</span></div>
                <div><Monitor size={14} /><span>{retained} {t("Sessions")} · {active} {t("active")}</span></div>
                <div><Clock3 size={14} /><span>{project.sessions?.latest_updated_at ? relativeTime(project.sessions.latest_updated_at) : "—"}</span></div>
              </div>
              {project.path && <code className="project-path" title={project.path}>{project.path}</code>}
              <span className="project-open">{t("View Sessions")} <ArrowUpRight size={14} /></span>
            </button>
          );
        })}
      </div>

      {projectsState.availability === "available" && !projectsState.projects.length && (
        <div className="empty-panel wide"><Folder size={19} /><strong>{t("No matching projects")}</strong></div>
      )}
      {projectsState.truncated && (
        <div className="inventory-note wide">{t("Project inventory is bounded. Narrow the search to find omitted Projects.")}</div>
      )}

      {selectedProject && (
        <section className="project-sessions" data-testid="project-active-sessions" ref={sessionsSection}>
          <div className="section-heading">
            <div>
              <h2>{projectDisplayName(selectedProject.name, selectedProject.id)} · {t("Sessions")}</h2>
              <p>{t("A Project may host multiple Sessions. Window counts are bounded, independently authorized evidence.")}</p>
            </div>
            <span className="quiet-pill">{sessionsState.total} {t("Sessions")}</span>
          </div>
          <div className="session-table">
            {sessionsState.sessions.map((session) => {
              const bucket = workBucket(session);
              const windows = sessionsState.windowCountBySession.get(session.session_id);
              return (
                <button
                  className="project-session-row"
                  key={session.session_id}
                  type="button"
                  onClick={() => onOpenSession({
                    projectId: selectedProject.id,
                    projectName: projectDisplayName(selectedProject.name, selectedProject.id),
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
                    {windows === undefined ? "…" : windows === null ? "—" : windows} {t("Windows")}
                  </span>
                  <span className={"status-pill " + (bucket === "attention" ? "warn" : bucket === "running" ? "running" : "good")}>
                    {t(bucket === "attention" ? "Needs attention" : bucket === "running" ? "Running" : session.lifecycle)}
                  </span>
                  <time>{relativeTime(session.updated_at)}</time>
                  <ArrowUpRight size={14} />
                </button>
              );
            })}
            {sessionsState.availability === "loading" && (
              <div className="empty-inline">{t("Loading Sessions…")}</div>
            )}
            {sessionsState.availability === "available" && !sessionsState.sessions.length && (
              <div className="empty-inline">{t("No Workflow Sessions retained for this Project.")}</div>
            )}
            {sessionsState.availability === "denied" && (
              <div className="empty-inline">{t("Session list unavailable. Check access to this Project.")}</div>
            )}
            {sessionsState.truncated && (
              <div className="inventory-note">{t("Project Session inventory is bounded; older retained Sessions are not loaded here.")}</div>
            )}
          </div>
        </section>
      )}
      {addOpen && (
        <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !addPending) setAddOpen(false); }}>
          <section className="modal-card" role="dialog" aria-modal="true" aria-labelledby="add-project-title">
            <header><div><span className="eyebrow">{t("Projects")}</span><h2 id="add-project-title">{t("Add Project")}</h2></div><button className="icon-button" type="button" onClick={() => !addPending && setAddOpen(false)} aria-label={t("Close")}>×</button></header>
            <form className="compact-form" onSubmit={(event) => void submitAddProject(event)}>
              <label>{t("Runner")}<select required value={addRunner} onChange={(event) => setAddRunner(event.target.value)}>{runners.map((runner) => <option key={runner.client_id} value={runner.client_id}>{runner.client_id}</option>)}</select></label>
              <label>{t("Project folder")}<input required maxLength={4096} autoComplete="off" spellCheck={false} value={addPath} onChange={(event) => setAddPath(event.target.value)} placeholder={t("Absolute folder path on the selected Runner")} /></label>
              {addStatus && <p className="modal-status" role={addStatus.includes("could") || addStatus.includes("无法") ? "alert" : "status"}>{addStatus}</p>}
              <div className="modal-actions"><button type="button" className="text-button" disabled={addPending} onClick={() => setAddOpen(false)}>{t("Cancel")}</button><button className="primary-button" type="submit" disabled={addPending || !addRunner || !addPath.trim()}>{addPending ? t("Adding project…") : t("Add Project")}</button></div>
            </form>
          </section>
        </div>
      )}
    </main>
  );
}
