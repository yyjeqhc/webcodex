import { useState } from "react";
import { TextInput } from "@mantine/core";
import { useProduct } from "../../i18n/product";
import { displayProjectPath, projectName, useWorkspace } from "../workspace/WorkspaceContext";
import { ProjectRows } from "./ProjectRows";
import { RunnerDevices } from "./RunnerDevices";

export function ProjectsPanel({ onComputerSettings }: { onComputerSettings?: () => void }) {
  const p = useProduct();
  const workspace = useWorkspace();
  const [query, setQuery] = useState("");
  const rows = workspace.projects.filter(project =>
    `${projectName(project)} ${displayProjectPath(project.path)} ${project.client_id || ""}`
      .toLocaleLowerCase()
      .includes(query.trim().toLocaleLowerCase()),
  );
  return <section className="page-section workspace-page" aria-labelledby="projects-title" data-webcodex-page="projects">
    <header className="page-heading-row"><h1 id="projects-title">{p("projects")} <span className="heading-count">{workspace.projects.length}</span></h1><button type="button" className="secondary-button" onClick={workspace.refresh} disabled={workspace.busy}>{p("refresh")}</button></header>
    {workspace.runner && <p className="workspace-notice">{p("projectAccessScope")}</p>}
    <div className="workspace-search"><TextInput id="projects-search" label={p("search")} type="search" value={query} onChange={event => setQuery(event.currentTarget.value)} /></div>
    {workspace.error && <div className="workspace-notice" role="alert">{p(workspace.errorReason)}</div>}
    <ProjectRows projects={rows} />
    {!rows.length && !workspace.error && !workspace.loading && <p className="workspace-empty">{p(query ? "noMatches" : "noProjects")}</p>}    {workspace.runner?.projects_truncated && <p className="workspace-notice">{p("partial")}</p>}
    <RunnerDevices onComputerSettings={onComputerSettings} />
  </section>;
}
