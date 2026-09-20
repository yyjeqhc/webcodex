import { useCallback, useEffect, useRef, useState } from "react";
import { fetchProjectSessions, fetchSessionDetail } from "../api/sessions.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, SessionListItem } from "../model/types.js";

const MAX_WINDOW_ENRICHMENT = 20;
const DETAIL_CONCURRENCY = 3;

export type ProjectSessionsState = {
  availability: Availability;
  sessions: SessionListItem[];
  total: number;
  truncated: boolean;
  windowCountBySession: Map<string, number | null>;
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
  const [windowCountBySession, setWindowCountBySession] = useState<Map<string, number | null>>(new Map());
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  const listRequest = useRef<AbortController | null>(null);
  const enrichmentRequest = useRef<AbortController | null>(null);
  const activeProject = useRef("");

  useEffect(() => {
    listRequest.current?.abort();
    enrichmentRequest.current?.abort();
    if (!enabled || !projectId) {
      activeProject.current = "";
      setSessions([]);
      setTotal(0);
      setTruncated(false);
      setWindowCountBySession(new Map());
      setAvailability("idle");
      return;
    }
    const projectChanged = activeProject.current !== projectId;
    activeProject.current = projectId;
    if (projectChanged) {
      setSessions([]);
      setTotal(0);
      setTruncated(false);
      setWindowCountBySession(new Map());
      setAvailability("loading");
    } else {
      setAvailability((current) => current === "idle" ? "loading" : current);
    }
    const controller = new AbortController();
    listRequest.current = controller;
    void fetchProjectSessions(client, projectId, controller.signal).then((response) => {
      if (listRequest.current !== controller || !response) return;
      listRequest.current = null;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403 || response.status === 404) {
        setSessions([]);
        setTotal(0);
        setTruncated(false);
        setWindowCountBySession(new Map());
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
    return () => controller.abort();
  }, [client, enabled, onUnauthorized, projectId, revision]);

  useEffect(() => {
    if (!enabled || availability !== "available" || !projectId) return;
    const targets = sessions
      .filter((session) => session.lifecycle === "active" || session.running_call || session.running_jobs > 0)
      .slice(0, MAX_WINDOW_ENRICHMENT);
    if (!targets.length) return;
    const controller = new AbortController();
    enrichmentRequest.current?.abort();
    enrichmentRequest.current = controller;
    let cursor = 0;
    let running = 0;
    let disposed = false;
    const next = () => {
      while (!disposed && !controller.signal.aborted && running < DETAIL_CONCURRENCY && cursor < targets.length) {
        const session = targets[cursor++];
        running += 1;
        void fetchSessionDetail(client, projectId, session.session_id, controller.signal, 1)
          .then((response) => {
            if (disposed || controller.signal.aborted) return;
            setWindowCountBySession((existing) => {
              const updated = new Map(existing);
              updated.set(session.session_id, response?.ok && response.data ? response.data.linked_windows.length : null);
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
  }, [availability, client, enabled, projectId, sessions]);

  useEffect(() => {
    if (!enabled || !projectId) return;
    const timer = window.setInterval(refresh, 15_000);
    return () => window.clearInterval(timer);
  }, [enabled, projectId, refresh]);

  return { availability, sessions, total, truncated, windowCountBySession };
}
