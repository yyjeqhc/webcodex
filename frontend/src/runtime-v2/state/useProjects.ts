import { useCallback, useEffect, useRef, useState } from "react";
import { fetchProjectGit, fetchProjects } from "../api/projects.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, ProjectGit, ProjectRow } from "../model/types.js";

const MAX_GIT_ENRICHMENT = 24;
const GIT_CONCURRENCY = 3;

export type ProjectsState = {
  availability: Availability;
  projects: ProjectRow[];
  total: number;
  truncated: boolean;
  query: string;
  runner: string;
  setQuery: (value: string) => void;
  setRunner: (value: string) => void;
  gitByProject: Map<string, ProjectGit | null>;
  refresh: () => void;
};

export function useProjects(
  client: RuntimeV2Client,
  enabled: boolean,
  onUnauthorized: () => void,
): ProjectsState {
  const [availability, setAvailability] = useState<Availability>("idle");
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [total, setTotal] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [query, setQuery] = useState("");
  const [runner, setRunner] = useState("");
  const [revision, setRevision] = useState(0);
  const [gitByProject, setGitByProject] = useState<Map<string, ProjectGit | null>>(new Map());
  const listRequest = useRef<AbortController | null>(null);
  const enrichmentRequest = useRef<AbortController | null>(null);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  const stableQuery = useDebouncedValue(query, 220);

  useEffect(() => {
    if (!enabled) {
      listRequest.current?.abort();
      enrichmentRequest.current?.abort();
      setAvailability("idle");
      return;
    }
    const controller = new AbortController();
    listRequest.current?.abort();
    listRequest.current = controller;
    setAvailability((value) => (value === "idle" ? "loading" : value));
    void fetchProjects(client, { runner, query: stableQuery }, controller.signal).then((response) => {
      if (listRequest.current !== controller || !response) return;
      listRequest.current = null;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403) {
        setProjects([]);
        setTotal(0);
        setTruncated(false);
        setAvailability("denied");
        return;
      }
      if (!response.ok || !response.data) {
        setAvailability((current) => current === "available" || current === "stale" ? "stale" : "error");
        return;
      }
      setProjects(response.data.projects || []);
      setTotal(Math.max(response.data.total || 0, response.data.projects?.length || 0));
      setTruncated(Boolean(response.data.truncated));
      setAvailability("available");
    });
    return () => controller.abort();
  }, [client, enabled, onUnauthorized, revision, runner, stableQuery]);

  useEffect(() => {
    if (!enabled || availability !== "available") return;
    const targets = projects.slice(0, MAX_GIT_ENRICHMENT).filter((project) => !gitByProject.has(project.id));
    if (!targets.length) return;
    const controller = new AbortController();
    enrichmentRequest.current?.abort();
    enrichmentRequest.current = controller;
    let cursor = 0;
    let running = 0;
    let disposed = false;

    const next = () => {
      while (!disposed && !controller.signal.aborted && running < GIT_CONCURRENCY && cursor < targets.length) {
        const project = targets[cursor++];
        running += 1;
        void fetchProjectGit(client, project.id, controller.signal)
          .then((response) => {
            if (disposed || controller.signal.aborted) return;
            setGitByProject((existing) => {
              const updated = new Map(existing);
              updated.set(project.id, response?.ok && response.data ? response.data : null);
              return updated;
            });
          })
          .finally(() => {
            running -= 1;
            next();
          });
      }
    };
    next();
    return () => {
      disposed = true;
      controller.abort();
    };
  }, [availability, client, enabled, projects]);

  useEffect(() => {
    if (!enabled) return;
    const timer = window.setInterval(refresh, 30_000);
    return () => window.clearInterval(timer);
  }, [enabled, refresh]);

  return {
    availability,
    projects,
    total,
    truncated,
    query,
    runner,
    setQuery,
    setRunner,
    gitByProject,
    refresh,
  };
}

function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(timer);
  }, [delayMs, value]);
  return debounced;
}
