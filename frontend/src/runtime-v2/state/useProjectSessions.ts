import { useCallback, useEffect, useRef, useState } from "react";
import { fetchProjectSessions } from "../api/sessions.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, SessionListItem } from "../model/types.js";

export type ProjectSessionsState = {
  availability: Availability;
  sessions: SessionListItem[];
  total: number;
  truncated: boolean;
};

export function useProjectSessions(
  client: RuntimeV2Client,
  enabled: boolean,
  projectId: string,
  onUnauthorized: () => void,
): ProjectSessionsState {
  const [availability, setAvailability] = useState<Availability>("idle");
  const [sessions, setSessions] = useState<SessionListItem[]>([]);
  const [total, setTotal] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [revision, setRevision] = useState(0);
  const listRequest = useRef<AbortController | null>(null);
  const activeProject = useRef("");
  const refresh = useCallback(() => {
    if (!listRequest.current) setRevision((value) => value + 1);
  }, []);

  useEffect(() => {
    listRequest.current?.abort();
    if (!enabled || !projectId) {
      activeProject.current = "";
      setSessions([]);
      setTotal(0);
      setTruncated(false);
      setAvailability("idle");
      return;
    }
    const projectChanged = activeProject.current !== projectId;
    activeProject.current = projectId;
    if (projectChanged) {
      setSessions([]);
      setTotal(0);
      setTruncated(false);
      setAvailability("loading");
    } else {
      setAvailability((current) => current === "idle" ? "loading" : current);
    }
    const controller = new AbortController();
    listRequest.current = controller;
    void fetchProjectSessions(client, projectId, controller.signal).then((response) => {
      if (controller.signal.aborted || listRequest.current !== controller || !response) return;
      listRequest.current = null;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403 || response.status === 404) {
        setSessions([]);
        setTotal(0);
        setTruncated(false);
        setAvailability("denied");
        return;
      }
      if (!response.ok || !response.data) {
        setAvailability((current) => current === "available" || current === "stale" ? "stale" : "error");
        return;
      }
      setSessions(response.data.sessions || []);
      setTotal(Math.max(response.data.total || 0, response.data.sessions?.length || 0));
      setTruncated(Boolean(response.data.truncated));
      setAvailability("available");
    });
    return () => {
      controller.abort();
      if (listRequest.current === controller) listRequest.current = null;
    };
  }, [client, enabled, onUnauthorized, projectId, revision]);

  useEffect(() => {
    if (!enabled || !projectId) return;
    const timer = window.setInterval(refresh, 15_000);
    return () => window.clearInterval(timer);
  }, [enabled, projectId, refresh]);

  return { availability, sessions, total, truncated };
}
