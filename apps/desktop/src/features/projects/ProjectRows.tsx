import { useEffect, useState } from "react";
import { Badge, Button, Table } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { FolderClosed, GitBranch } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import type { GitSummary, WorkspaceProject } from "../../models/workspace";
import { displayProjectPath, projectName, useWorkspace, workspaceQuery } from "../workspace/WorkspaceContext";
import { projectRunnerId } from "./project-device";
import { observationTime } from "../workspace/WorkspaceStatus";

export function ProjectRows({ projects, onUnregister, busy = false }: { projects: WorkspaceProject[]; onUnregister?: (project: WorkspaceProject) => void; busy?: boolean }) {
  const p = useProduct();
  const compact = useMediaQuery("(max-width: 600px)", undefined, { getInitialValueInEffect: false });
  if (compact) return <div className="workspace-project-mobile-list">
    {projects.map(project => <ProjectRow key={project.id || project.path} project={project} onUnregister={onUnregister} busy={busy} compact />)}  </div>;
  return <Table.ScrollContainer minWidth={onUnregister ? 680 : 560} className="workspace-project-table" type="native">
    <Table striped={false} highlightOnHover={false} verticalSpacing="sm" horizontalSpacing="sm" layout="fixed" aria-label={p("projects")}>
      <Table.Thead><Table.Tr><Table.Th>{p("projects")}</Table.Th><Table.Th className="project-column-branch">{p("branch")}</Table.Th><Table.Th className="project-column-activity">{p("activeSessions")}</Table.Th><Table.Th className="project-column-updated">{p("lastUsed")}</Table.Th>{onUnregister && <Table.Th className="project-column-action">{p("manage")}</Table.Th>}</Table.Tr></Table.Thead>
      <Table.Tbody>{projects.map(project => <ProjectRow key={project.id || project.path} project={project} onUnregister={onUnregister} busy={busy} />)}</Table.Tbody>
    </Table>
  </Table.ScrollContainer>;
}
function ProjectRow({ project, compact = false, onUnregister, busy }: { project: WorkspaceProject; compact?: boolean; onUnregister?: (project: WorkspaceProject) => void; busy?: boolean }) {  const p = useProduct(); const { locale } = useLocale();
  const [git, setGit] = useState<GitSummary | null>(null);
  const [gitStatus, setGitStatus] = useState<"idle" | "loading" | "ready" | "unconfirmed">("idle");
  const { revision, busy: workspaceBusy, state } = useWorkspace();
  useEffect(() => { setGit(null); setGitStatus("idle"); }, [project.id, project.connected]);
  useEffect(() => {
    if (workspaceBusy) return;
    let cancelled = false;

    if (project.id && project.connected) {
      // Refresh is a fresh observation boundary. Do not keep showing a branch
      // from an older successful probe while the current workspace is missing
      // or otherwise unavailable.
      setGit(null);
      setGitStatus("loading");
      void workspaceQuery<GitSummary>({ kind: "project_git", project: project.id })
        .then(value => { if (!cancelled) { setGit(value); setGitStatus("ready"); } })
        .catch(() => { if (!cancelled) { setGit(null); setGitStatus("unconfirmed"); } });
    } else {
      setGit(null);
      setGitStatus("idle");
    }
    return () => { cancelled = true; };
  }, [project.id, project.connected, revision, workspaceBusy]);
  const name = projectName(project);
  const branch = gitStatus === "unconfirmed" || !project.connected ? p("projectStatusUnconfirmed") : git?.branch || (git?.non_git_project ? p("notGit") : "—");
  const changes = <ProjectGitChanges git={git} loading={gitStatus === "loading"} />;
  const activity = project.sessions ? `${project.sessions.active_sessions}${project.sessions.sessions_truncated ? "+" : ""} ${p("activeSessions")}` : p(project.id ? "unknown" : "setup");
  const activityValue = project.sessions ? `${project.sessions.active_sessions}${project.sessions.sessions_truncated ? "+" : ""}` : "—";
  const path = displayProjectPath(project.path);
  const runner = projectRunnerId(project);
  const origin = runner ? <span className="project-table-meta">{p("executionDevice")} · {runner === state.workspace_runner?.client_id ? p("thisComputer") : runner}</span> : null;
  const updated = observationTime(project.sessions?.latest_updated_at ? project.sessions.latest_updated_at * 1000 : null, locale);
  const remove = onUnregister && runner === state.workspace_runner?.client_id && <Button variant="subtle" color="red" size="compact-sm" disabled={busy || !project.id || !project.connected}
    aria-label={`${p("unregisterProject")} ${name}`} onClick={() => onUnregister(project)}>{p("unregisterProject")}</Button>;
  if (compact) return <article className="workspace-project-mobile-row" aria-label={name}>
    <h3>{name}</h3><div className="project-path" title={path}>{path}</div>{origin}
    <div className="project-row-meta"><span>{branch}</span><span>{activity}</span><time>{updated}</time></div>{changes}{remove}
  </article>;
  return <Table.Tr aria-label={name}>
    <Table.Td>
      <div className="project-table-name">
        <div className="project-avatar" aria-hidden="true"><FolderClosed size={17} strokeWidth={1.75} /></div>
        <div className="project-row-main"><div className="project-row-title"><h3>{name}</h3></div><span className="project-path" title={path}>{path}</span>{origin}</div>
      </div>
    </Table.Td>
    <Table.Td className="project-column-branch"><span className="project-table-meta"><GitBranch size={14} aria-hidden="true" />{branch}</span>{changes}</Table.Td>
    <Table.Td className="project-column-activity"><Badge className="project-activity-badge" size="sm" variant="light"
      color={project.sessions?.active_sessions ? "brand" : "gray"} aria-label={activity} title={activity}>{activityValue}</Badge></Table.Td>
    <Table.Td className="project-column-updated"><time className="project-table-time">{updated}</time></Table.Td>
    {onUnregister && <Table.Td className="project-column-action">{remove}</Table.Td>}
  </Table.Tr>;
}

function ProjectGitChanges({ git, loading }: { git: GitSummary | null; loading: boolean }) {
  const p = useProduct();
  if (loading) return <span className="project-git-state">{p("loading")}</span>;
  // Failed/offline rows already expose their unconfirmed branch observation.
  if (!git || git.non_git_project) return null;
  if (!git.git_available || git.clean == null) return <span className="project-git-state">{p("projectStatusUnconfirmed")}</span>;
  if (git.clean) return <span className="project-git-state">{p("clean")}</span>;
  const count = git.files_total;
  const label = typeof count === "number" && count > 0
    ? p("projectGitChangeCount").replace("{count}", String(count)) : p("projectGitHasChanges");
  const files = git.files || [];
  return <div className="project-git-state">
    {files.length ? <details className="project-git-changes"><summary>{label}</summary>
      <ul aria-label={p("files")}>{files.map((file, index) => <li key={`${file.path}:${index}`}>
        {file.status && <code className="project-git-file-status">{file.status}</code>}<span>{file.path}</span>
      </li>)}</ul>
    </details> : <span>{label}</span>}
    {git.files_truncated && <p className="field-help">{p("projectGitPartialFiles")}</p>}
  </div>;
}
