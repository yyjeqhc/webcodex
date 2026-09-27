import { useState } from "react";
import { TextInput } from "@mantine/core";
import { useProduct } from "../../i18n/product";
import { displayProjectPath, projectName, useWorkspace } from "../workspace/WorkspaceContext";
import { ProjectRows } from "./ProjectRows";

export function ProjectsPanel() {
  const p = useProduct();
  const workspace = useWorkspace();
  const [query, setQuery] = useState("");
  const rows = workspace.projects.filter(project =>
    `${projectName(project)} ${displayProjectPath(project.path)}`
      .toLocaleLowerCase()
      .includes(query.trim().toLocaleLowerCase()),
  );
  return <section className="page-section workspace-page" aria-labelledby="projects-title" data-webcodex-page="projects">
    <header className="page-heading-row"><div><span className="eyebrow">Runner · {workspace.runner?.client_id || p("workspace")}</span><h1 id="projects-title">{p("runnerProjects")} <span className="heading-count">{workspace.projects.length}</span></h1></div></header>
    <div className="workspace-search"><TextInput id="projects-search" label={p("search")} type="search" value={query} onChange={event => setQuery(event.currentTarget.value)} /></div>
    {workspace.error && <div className="workspace-notice" role="alert">{p("loadError")} <button className="text-button" onClick={workspace.refresh}>{p("refresh")}</button></div>}
    <ProjectRows projects={rows} />
    {!rows.length && <p className="workspace-empty">{p(query ? "noMatches" : "noProjects")}</p>}
    {workspace.runner?.projects_truncated && <p className="workspace-notice">{p("partial")}</p>}
  </section>;
}