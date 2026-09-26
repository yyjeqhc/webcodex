import { useCallback, useEffect, useRef, useState } from "react";
import { fetchProjects } from "../api/projects.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, ProjectRow } from "../model/types.js";

export type ProjectsState = {
  availability: Availability;
  projects: ProjectRow[];
  total: number;
  truncated: boolean;
  query: string;
  runner: string;
  setQuery: (value: string) => void;
  setRunner: (value: string) => void;
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
  const listRequest = useRef<AbortController | null>(null);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  const stableQuery = useDebouncedValue(query, 220);

  useEffect(() => {
    if (!enabled) {
      listRequest.current?.abort();
      setAvailability("idle");
      return;
    }
    const controller = new AbortController();
    listRequest.current?.abort();
    listRequest.current = controller;
    setAvailability((value) => (value === "idle" ? "loading" : value));
    void fetchProjects(client, { runner, query: stableQuery }, controller.signal).then((response) => {
      if (controller.signal.aborted || listRequest.current !== controller || !response) return;
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
    return () => {
      controller.abort();
      if (listRequest.current === controller) listRequest.current = null;
    };
  }, [client, enabled, onUnauthorized, revision, runner, stableQuery]);

  useEffect(() => {
    if (!enabled) return;
    const timer = window.setInterval(() => {
      if (!listRequest.current) refresh();
    }, 30_000);
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
