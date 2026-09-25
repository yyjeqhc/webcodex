/** Presentation only. Never use the result for identity, grants or filesystem operations. */
export function displayProjectPath(path?: string): string {
  if (!path) return "";
  if (/^\\\\\?\\UNC\\/i.test(path)) return "\\\\" + path.slice(8);
  if (/^\\\\\?\\[a-z]:\\/i.test(path)) return path.slice(4);
  return path;
}

export function projectPresentationName(project: { name?: string; path?: string; id?: string }): string {
  const path = displayProjectPath(project.path);
  const name = project.name?.trim();
  const root = /^[a-z]:[\\/]*$/i.test(path) || /^\\\\[^\\/]+[\\/][^\\/]+[\\/]*$/.test(path);
  if (name && !(root && /^project$/i.test(name))) return name;
  return path.split(/[\\/]/).filter(Boolean).pop() || project.id || "Project";
}

export type PresentableProject = {
  id: string;
  client_id: string;
  name?: string;
  path?: string;
  lineage?: {
    kind: "managed_worktree_source";
    source_project_id: string;
    base_sha: string;
  };
};

/** Human grouping only. Identity and authority remain the exact Runtime Project id. */
export function sourceProjectRuntimeId(project: PresentableProject): string | undefined {
  const source = project.lineage?.source_project_id;
  return source ? `agent:${project.client_id}:${source}` : undefined;
}

export function projectFamilyId(project: PresentableProject): string {
  return sourceProjectRuntimeId(project) || project.id;
}

export function projectVariantLabel(project: PresentableProject): string {
  if (!project.lineage) return "Primary workspace";
  const path = displayProjectPath(project.path);
  const leaf = path.split(/[\\/]/).filter(Boolean).pop();
  return leaf || project.name?.trim() || "Managed worktree";
}

export function projectFamilyName(project: PresentableProject, projects: PresentableProject[]): string {
  const sourceId = sourceProjectRuntimeId(project);
  const source = sourceId ? projects.find((row) => row.id === sourceId) : project;
  return projectPresentationName(source || project);
}
