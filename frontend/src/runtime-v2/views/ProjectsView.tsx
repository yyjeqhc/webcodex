import { displayProjectPath } from "../../ui/projectPresentation.js";
import {
  ArrowUpRight,
  Folder,
  GitBranch,
  Monitor,
  Search,
  Plus,
} from "lucide-react";
import { Button, Modal, Select, TextInput } from "@mantine/core";
import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { ScopePicker } from "../../ui/ProjectPicker.js";
import type { RuntimeV2Client } from "../api/client.js";
import { registerProject } from "../api/projects.js";
import { PageHeader } from "../components/ui/PageHeader.js";
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
    <main className="page ui-workbench-surface">
      <PageHeader title={t("Projects")} actions={
        <>
          <span className="quiet-pill">
            {projectsState.availability === "stale" ? t("stale") :
              projectsState.availability === "loading" ? t("Loading projects…") :
                String(projectsState.total) + " " + t("projects")}
          </span>
          <Button className="runtime-primary" type="button" leftSection={<Plus size={16} />} onClick={() => { setAddOpen(true); setAddStatus(""); }}>{t("Add Project")}</Button>
        </>
      } />

      <div className="filter-bar project-filter-bar">
        <TextInput className="project-filter-input" leftSection={<Search size={16} />}
          aria-label={t("Search projects")}
          placeholder={t("Filter by Project name, id, Runner, or workspace path")}
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
        {projectsState.projects.map((project) => {
          const git = projectsState.gitByProject.get(project.id);
          const retained = projectRetainedSessionCount(project);
          const active = project.sessions?.active_sessions ?? 0;
          const selected = selectedProject?.id === project.id;
          return (
            <button
              className={"project-card ui-entity-row" + (selected ? " selected" : "")}
              key={project.id}
              type="button"
              onClick={() => openProject(project.id)}
              data-testid={"project-card-" + project.id}
            >
              <div className="project-card-head">
                <span className="project-icon"><Folder size={18} /></span>
                <span>
                  <strong title={project.id}>{projectDisplayName(project.name, project.id, project.path)}</strong>
                  <small title={displayProjectPath(project.path) || project.id}>{displayProjectPath(project.path) || project.client_id}</small>
                </span>
                <span className={"status-pill " + (project.connected ? "good" : "warn")}>
                  {project.connected ? t("online") : t("offline")}
                </span>
              </div>
              <div className="project-card-body">
                <div><GitBranch size={14} /><span title={String(git?.branch || "")}>{git?.branch || t("Not checked")}</span></div>
                <div><Monitor size={14} /><span>{retained} {t("Sessions")} · {active} {t("active")}</span></div>
              </div>
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
              <h2>{projectDisplayName(selectedProject.name, selectedProject.id, selectedProject.path)} · {t("Sessions")}</h2>
            </div>
            <span className="quiet-pill">{sessionsState.total} {t("Sessions")}</span>
          </div>
          <div className="session-table">
            {sessionsState.sessions.map((session) => {
              const bucket = workBucket(session);
              const windows = sessionsState.windowCountBySession.get(session.session_id);
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
