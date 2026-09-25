import { useEffect, useState } from "react";
import { Badge, Button, Table } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { FolderClosed, GitBranch } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import type { GitSummary, WorkspaceProject } from "../../models/workspace";
import { displayProjectPath, projectName, useWorkspace, workspaceQuery } from "../workspace/WorkspaceContext";
import { observationTime } from "../workspace/WorkspaceStatus";

export function ProjectRows({ projects, onUnregister, busy = false }: { projects: WorkspaceProject[]; onUnregister?: (project: WorkspaceProject) => void; busy?: boolean }) {
  const p = useProduct();
  const compact = useMediaQuery("(max-width: 600px)", undefined, { getInitialValueInEffect: false });
  if (compact) return <div className="workspace-project-mobile-list">
    {projects.map(project => <ProjectRow key={project.id || project.path} project={project} onUnregister={onUnregister} busy={busy} compact />)}
  </div>;
  return <Table.ScrollContainer minWidth={onUnregister ? 680 : 560} className="workspace-project-table" type="native">
    <Table striped={false} highlightOnHover={false} verticalSpacing="sm" horizontalSpacing="sm" layout="fixed" aria-label={p("projects")}>
      <Table.Thead><Table.Tr><Table.Th>{p("projects")}</Table.Th><Table.Th className="project-column-branch">{p("branch")}</Table.Th><Table.Th className="project-column-activity">{p("activeSessions")}</Table.Th><Table.Th className="project-column-updated">{p("lastUsed")}</Table.Th>{onUnregister && <Table.Th className="project-column-action">{p("manage")}</Table.Th>}</Table.Tr></Table.Thead>
      <Table.Tbody>{projects.map(project => <ProjectRow key={project.id || project.path} project={project} onUnregister={onUnregister} busy={busy} />)}</Table.Tbody>
    </Table>
  </Table.ScrollContainer>;
}
function ProjectRow({ project, compact = false, onUnregister, busy }: { project: WorkspaceProject; compact?: boolean; onUnregister?: (project: WorkspaceProject) => void; busy?: boolean }) {
  const p = useProduct(); const { locale } = useLocale();
  const [git, setGit] = useState<GitSummary | null>(null);
  const { revision } = useWorkspace();
  useEffect(() => {
    let cancelled = false;
    setGit(null);
    if (project.id && project.connected) void workspaceQuery<GitSummary>({ kind: "project_git", project: project.id }).then(value => { if (!cancelled) setGit(value); }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [project.id, project.connected, revision]);
  const name = projectName(project);
  const branch = git?.branch || (git?.non_git_project ? p("notGit") : "—");
  const activity = project.sessions ? `${project.sessions.active_sessions}${project.sessions.sessions_truncated ? "+" : ""} ${p("activeSessions")}` : p(project.id ? "unknown" : "setup");
  const activityValue = project.sessions ? `${project.sessions.active_sessions}${project.sessions.sessions_truncated ? "+" : ""}` : "—";
  const path = displayProjectPath(project.path);
  const updated = observationTime(project.sessions?.latest_updated_at ? project.sessions.latest_updated_at * 1000 : null, locale);
  const remove = onUnregister && <Button variant="subtle" color="red" size="compact-sm" disabled={busy || !project.id || !project.connected}
    aria-label={`${p("unregisterProject")} ${name}`} onClick={() => onUnregister(project)}>{p("unregisterProject")}</Button>;
  if (compact) return <article className="workspace-project-mobile-row" aria-label={name}>
    <h3>{name}</h3><div className="project-path" title={path}>{path}</div>
    <div className="project-row-meta"><span>{branch}</span><span>{activity}</span><time>{updated}</time></div>{remove}
  </article>;
  return <Table.Tr aria-label={name}>
    <Table.Td>
      <div className="project-table-name">
        <div className="project-avatar" aria-hidden="true"><FolderClosed size={17} strokeWidth={1.75} /></div>
        <div className="project-row-main"><div className="project-row-title"><h3>{name}</h3></div><span className="project-path" title={path}>{path}</span></div>
      </div>
    </Table.Td>
    <Table.Td className="project-column-branch"><span className="project-table-meta"><GitBranch size={14} aria-hidden="true" />{branch}</span></Table.Td>
    <Table.Td className="project-column-activity"><Badge className="project-activity-badge" size="sm" variant="light"
      color={project.sessions?.active_sessions ? "brand" : "gray"} aria-label={activity} title={activity}>{activityValue}</Badge></Table.Td>
    <Table.Td className="project-column-updated"><time className="project-table-time">{updated}</time></Table.Td>
    {onUnregister && <Table.Td className="project-column-action">{remove}</Table.Td>}
  </Table.Tr>;
}
