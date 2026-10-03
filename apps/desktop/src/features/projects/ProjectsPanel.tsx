import { useState } from "react";
import { Button, TextInput } from "@mantine/core";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState } from "../../models/topology";
import type { UnregisterObservation, WorkspaceProject } from "../../models/workspace";
import { useProduct } from "../../i18n/product";
import { displayProjectPath, projectName, useWorkspace } from "../workspace/WorkspaceContext";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { ProjectRows } from "./ProjectRows";
import { RunnerDevices } from "./RunnerDevices";
import { AddLocalProject } from "./AddLocalProject";

export function ProjectsPanel({ onComputerSettings, onState }: { onComputerSettings?: () => void; onState?: (state: DesktopState) => void }) {
  const p = useProduct();
  const workspace = useWorkspace();
  const [query, setQuery] = useState("");
  const [removal, setRemoval] = useState<UnregisterObservation | null>(null);
  const [removing, setRemoving] = useState(false);
  const [removeError, setRemoveError] = useState(false);
  const canManageProjects = Boolean(onState && workspace.state.workspace_runner);

  const prepareRemoval = async (project: WorkspaceProject) => {
    if (!canManageProjects || removing || workspace.state.current_operation || !project.id) return;
    setRemoving(true);
    setRemoveError(false);
    try {
      setRemoval(await desktopApi.prepareProjectUnregister(project.id));
    } catch {
      setRemoveError(true);
    } finally {
      setRemoving(false);
    }
  };

  const unregister = async () => {
    if (!removal || !onState || removing || workspace.state.current_operation) return;
    setRemoving(true);
    setRemoveError(false);
    try {
      onState(await desktopApi.unregisterProject(removal));
      workspace.removeProject(removal.project);
    } catch {
      setRemoveError(true);
    } finally {
      setRemoval(null);
      setRemoving(false);
      workspace.refresh();
    }
  };

  const rows = workspace.projects.filter(project =>
    `${projectName(project)} ${displayProjectPath(project.path)} ${project.client_id || ""}`
      .toLocaleLowerCase()
      .includes(query.trim().toLocaleLowerCase()),
  );
  return <section className="page-section workspace-page" aria-labelledby="projects-title" data-webcodex-page="projects">
    <header className="page-heading-row"><h1 id="projects-title">{p("projects")} <span className="heading-count">{workspace.projects.length}</span></h1><button type="button" className="secondary-button" onClick={workspace.refresh} disabled={workspace.busy}>{p("refresh")}</button></header>
    {onState && <AddLocalProject state={workspace.state} onState={onState} onAdded={workspace.refresh} />}
    {workspace.runner && <p className="workspace-notice">{p("projectAccessScope")}</p>}
    <div className="workspace-search"><TextInput id="projects-search" label={p("search")} type="search" value={query} onChange={event => setQuery(event.currentTarget.value)} /></div>
    {workspace.error && <div className="workspace-notice" role="alert">{p(workspace.errorReason)}</div>}
    <ProjectRows projects={rows} onUnregister={canManageProjects ? prepareRemoval : undefined} busy={removing || Boolean(workspace.state.current_operation)} />
    {removeError && <p role="alert" className="workspace-notice">{p("unregisterError")}</p>}
    {removal && <WorkspaceDialog title={p("unregisterProject")} onClose={() => setRemoval(null)} busy={removing}>
      <p>{p("unregisterDescription")}</p>
      <code>{displayProjectPath(removal.path)}</code>
      <div className="shell-actions">
        <Button variant="default" onClick={() => setRemoval(null)} disabled={removing}>{p("cancel")}</Button>
        <Button color="red" onClick={() => void unregister()} disabled={removing || Boolean(workspace.state.current_operation)}>{p("unregisterProject")}</Button>
      </div>
    </WorkspaceDialog>}
    {!rows.length && !workspace.error && !workspace.loading && <p className="workspace-empty">{p(query ? "noMatches" : "noProjects")}</p>}    {workspace.runner?.projects_truncated && <p className="workspace-notice">{p("partial")}</p>}
    <RunnerDevices onComputerSettings={onComputerSettings} />
  </section>;
}
