import type { ProjectGit, ProjectsResponse } from "../model/types.js";
import type { RuntimeV2Client } from "./client.js";

export function fetchProjects(
  client: RuntimeV2Client,
  options: { runner?: string; query?: string; limit?: number },
  signal?: AbortSignal,
) {
  const payload: Record<string, unknown> = {};
  if (options.limit !== undefined) payload.limit = options.limit;
  if (options.runner) payload.client_id = options.runner;
  if (options.query) payload.query = options.query;
  return client.post<ProjectsResponse>("projects", payload, signal);
}

export function fetchProjectGit(client: RuntimeV2Client, project: string, signal?: AbortSignal) {
  return client.post<ProjectGit>("project-git", { project }, signal);
}

export function registerProject(
  client: RuntimeV2Client,
  clientId: string,
  path: string,
  signal?: AbortSignal,
) {
  return client.postAt<{ success?: boolean; project?: unknown }>(
    "/api/projects/",
    "resolve-or-register",
    { client_id: clientId, path },
    signal,
  );
}
