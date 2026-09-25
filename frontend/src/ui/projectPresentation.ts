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
