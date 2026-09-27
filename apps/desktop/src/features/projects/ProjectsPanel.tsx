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
    `${projectName(project)} ${displayProjectPath(project.path)} ${project.client_id || ""}`
      .toLocaleLowerCase()
      .includes(query.trim().toLocaleLowerCase()),
  );
  return <section className="page-section workspace-page" aria-labelledby="projects-title" data-webcodex-page="projects">
    <header className="page-heading-row"><div><span className="eyebrow">{p("workspace")}</span><h1 id="projects-title">{p("projects")} <span className="heading-count">{workspace.projects.length}</span></h1></div></header>
    {workspace.runner && <p className="workspace-notice">{p("projectAccessScope")}</p>}
    {workspace.runners.length > 0 && <section className="form-card" aria-label={p("authorizedRunners")}>
      <h2>{p("authorizedRunners")}</h2>
      <ul>{workspace.runners.map(runner => {
        const stale = workspace.fleetStale || runner.status === "stale";
        const gui = !stale && runner.connected && runner.computer_session_availability === true;
        return <li key={runner.client_id}>
          <code>{runner.client_id}</code>{runner.client_id === workspace.runner?.client_id && <> · {p("thisComputer")}</>}
          {" · "}{p(stale ? "staleData" : runner.connected ? "online" : "offline")}
          {" · "}{p(gui ? "guiAvailable" : "guiUnavailable")}
        </li>;
      })}</ul>
    </section>}
    <div className="workspace-search"><TextInput id="projects-search" label={p("search")} type="search" value={query} onChange={event => setQuery(event.currentTarget.value)} /></div>
    {workspace.error && <div className="workspace-notice" role="alert">{p(workspace.errorReason)} <button className="text-button" onClick={workspace.refresh}>{p("refresh")}</button></div>}
    <ProjectRows projects={rows} />
    {!rows.length && !workspace.error && !workspace.loading && <p className="workspace-empty">{p(query ? "noMatches" : "noProjects")}</p>}    {workspace.runner?.projects_truncated && <p className="workspace-notice">{p("partial")}</p>}
  </section>;
}