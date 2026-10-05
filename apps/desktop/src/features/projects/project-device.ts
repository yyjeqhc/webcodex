import type { WorkspaceProject } from "../../models/workspace";

// Use the same exact Runner identity for display, filtering and local controls.
export function projectRunnerId(project: WorkspaceProject): string | undefined {
  return project.client_id || (project.id?.startsWith("agent:") ? project.id.split(":")[1] || undefined : undefined);
}
