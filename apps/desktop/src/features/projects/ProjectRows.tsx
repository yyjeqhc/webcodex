import { useEffect, useState } from "react";
import { Badge, Button, Group, NavLink, Table, Text } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { FolderClosed, GitBranch, ArrowRight, Check } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import { useShellText } from "../../i18n/runtime-shell";
import { useProduct } from "../../i18n/product";
import type { GitSummary, WorkspaceProject } from "../../models/workspace";
import { sameProjectPath, projectName, useWorkspace, workspaceQuery } from "../workspace/WorkspaceContext";
import { observationTime } from "../workspace/WorkspaceStatus";

export function ProjectRows({ projects, onOpen }: { projects: WorkspaceProject[]; onOpen: (path: string) => void }) {
  const { state } = useWorkspace(); const p = useProduct(); const s = useShellText();
  const compact = useMediaQuery("(max-width: 600px)", undefined, { getInitialValueInEffect: false });
  if (compact) return <div className="workspace-project-mobile-list">
    {projects.map(project => <CompactProjectRow key={project.id || project.path} project={project}
      selected={sameProjectPath(project.path, state.project?.path)} busy={Boolean(state.current_operation)} onOpen={onOpen} />)}
  </div>;
  return <Table.ScrollContainer minWidth={560} className="workspace-project-table" type="native">
    <Table striped={false} highlightOnHover={false} verticalSpacing="sm" horizontalSpacing="sm" layout="fixed" aria-label={p("projects")}>
      <Table.Thead>
      <Table.Tr><Table.Th>{p("projects")}</Table.Th><Table.Th className="project-column-branch">{p("branch")}</Table.Th><Table.Th className="project-column-activity">{p("activeSessions")}</Table.Th><Table.Th className="project-column-updated">{p("lastUsed")}</Table.Th><Table.Th className="project-column-action"><span>{s("Project actions")}</span></Table.Th></Table.Tr>
      </Table.Thead>
      <Table.Tbody>{projects.map(project => <ProjectRow key={project.id || project.path} project={project}
        selected={sameProjectPath(project.path, state.project?.path)} busy={Boolean(state.current_operation)} onOpen={onOpen} />)}</Table.Tbody>
    </Table>
  </Table.ScrollContainer>;
}
function CompactProjectRow({ project, selected, busy, onOpen }: { project: WorkspaceProject; selected: boolean; busy: boolean; onOpen: (path: string) => void }) {
  const p = useProduct(); const s = useShellText();
  const name = projectName(project);
  const activity = project.sessions ? `${project.sessions.active_sessions}${project.sessions.sessions_truncated ? "+" : ""} ${p("activeSessions")}` : p(project.id ? "unknown" : "setup");
  return <NavLink component="button" type="button" className={"workspace-project-mobile-row" + (selected ? " selected" : "")}
    aria-label={`${s(selected ? "Current project" : "Use project")} ${name}`} aria-current={selected ? "true" : undefined} disabled={busy || !project.path || selected}
    leftSection={<FolderClosed size={18} strokeWidth={1.75} />}
    rightSection={selected ? <Check size={16} /> : <ArrowRight size={16} />}
    label={<Group gap="xs" wrap="nowrap"><Text size="sm" fw={650} truncate>{name}</Text>{selected && <Badge size="xs" variant="light" className="project-current-badge">{p("current")}</Badge>}</Group>}
    description={<Group gap="xs" wrap="nowrap"><Text size="xs" truncate title={project.path}>{project.path}</Text><Text size="xs" c="dimmed">· {activity}</Text></Group>}
    onClick={() => project.path && onOpen(project.path)} />;
}
function ProjectRow({ project, selected, busy, onOpen }: { project: WorkspaceProject; selected: boolean; busy: boolean; onOpen: (path: string) => void }) {
  const p = useProduct(); const s = useShellText(); const { locale } = useLocale();
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
  return <Table.Tr className={selected ? "selected" : ""} aria-label={name}>
    <Table.Td>
      <div className="project-table-name">
        <div className="project-avatar" aria-hidden="true"><FolderClosed size={17} strokeWidth={1.75} /></div>
        <div className="project-row-main"><div className="project-row-title"><h3>{name}</h3>{selected && <Badge size="xs" variant="light" className="project-current-badge">{p("current")}</Badge>}</div><span className="project-path" title={project.path}>{project.path}</span></div>
      </div>
    </Table.Td>
    <Table.Td className="project-column-branch"><span className="project-table-meta"><GitBranch size={14} aria-hidden="true" />{branch}</span></Table.Td>
    <Table.Td className="project-column-activity"><Badge className="project-activity-badge" size="sm" variant="light"
      color={project.sessions?.active_sessions ? "brand" : "gray"} aria-label={activity} title={activity}>{activityValue}</Badge></Table.Td>
    <Table.Td className="project-column-updated"><time className="project-table-time">{observationTime(project.sessions?.latest_updated_at ? project.sessions.latest_updated_at * 1000 : null, locale)}</time></Table.Td>
    <Table.Td className="project-column-action">
      <Button className="project-open-button" variant="subtle" color="brand" size="compact-sm" rightSection={selected ? <Check size={14} /> : <ArrowRight size={14} />}
        aria-label={`${s(selected ? "Current project" : "Use project")} ${name}`} aria-current={selected ? "true" : undefined} disabled={busy || !project.path || selected}
        onClick={() => project.path && onOpen(project.path)} data-webcodex-action="open-project" title={s("This activates the exact project in this Runner; it does not open an external editor.")}>{s(selected ? "Selected" : "Select project")}</Button>
    </Table.Td>
  </Table.Tr>;
}
