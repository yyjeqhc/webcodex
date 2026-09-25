import { desktopApi } from "../../lib/desktop-api";
import type { UnregisterObservation, WorkspaceProject } from "../../models/workspace";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { useState } from "react";
import { Button, TextInput } from "@mantine/core";
import type { DesktopState } from "../../models/topology";
import { useProduct } from "../../i18n/product";
import { displayProjectPath, projectName, useWorkspace } from "../workspace/WorkspaceContext";
import { ProjectRows } from "./ProjectRows";

export function ProjectsPanel({ state, onChooseProject, onState }: { state: DesktopState; onChooseProject: () => void; onState: (state: DesktopState) => void }) {
  const p = useProduct(); const workspace = useWorkspace(); const [query, setQuery] = useState("");
  const [removal, setRemoval] = useState<UnregisterObservation | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const prepare = async (project: WorkspaceProject) => {
    if (busy || state.current_operation) return;
    setBusy(true); setError(false);
    try { setRemoval(await desktopApi.prepareProjectUnregister(project.id)); }
    catch { setError(true); } finally { setBusy(false); }
  };
  const unregister = async () => {
    if (!removal || busy || state.current_operation) return;
    setBusy(true); setError(false);
    try { onState(await desktopApi.unregisterProject(removal)); workspace.removeProject(removal.project); }
    catch { setError(true); }
    finally { setRemoval(null); setBusy(false); workspace.refresh(); }
  };
  const rows = workspace.projects.filter(project => `${projectName(project)} ${displayProjectPath(project.path)}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  return <section className="page-section workspace-page" aria-labelledby="projects-title" data-webcodex-page="projects">
    <header className="page-heading-row"><div><span className="eyebrow">Runner · {workspace.runner?.client_id || p("workspace")}</span><h1 id="projects-title">{p("runnerProjects")} <span className="heading-count">{workspace.projects.length}</span></h1></div>
      <Button className="primary-button" onClick={onChooseProject} disabled={Boolean(state.current_operation)} data-webcodex-action="add-project">{p("addProject")}</Button></header>
    <div className="workspace-search"><TextInput id="projects-search" label={p("search")} type="search" value={query} onChange={event => setQuery(event.currentTarget.value)} /></div>
    {workspace.error && <div className="workspace-notice" role="alert">{p("loadError")} <button className="text-button" onClick={workspace.refresh}>{p("refresh")}</button></div>}
    <ProjectRows projects={rows} onUnregister={prepare} busy={busy || Boolean(state.current_operation)} />
    {error && <p role="alert" className="workspace-notice">{p("unregisterError")}</p>}
    {removal && <WorkspaceDialog title={p("unregisterProject")} onClose={() => setRemoval(null)} busy={busy}>
      <p>{p("unregisterDescription")}</p><code>{displayProjectPath(removal.path)}</code>
      <p><code>{removal.project}</code></p>
      <Button variant="default" onClick={() => setRemoval(null)} disabled={busy}>{p("cancel")}</Button>
      <Button color="red" onClick={() => void unregister()} disabled={busy || Boolean(state.current_operation)}>{p("unregisterProject")}</Button>
    </WorkspaceDialog>}
    {!rows.length && <p className="workspace-empty">{p(query ? "noMatches" : "noProjects")}</p>}
    {workspace.runner?.projects_truncated && <p className="workspace-notice">{p("partial")}</p>}
  </section>;
}
