import { useCallback, useEffect, useRef, useState } from "react";
import { fetchGoal, fetchGoals } from "../api/goals.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { GoalDetailResponse, GoalListItem } from "../model/goals.js";
import type { Availability } from "../model/types.js";
import { useObservationRequest } from "./useObservationRequest.js";
import { useVisibleRefresh } from "./useVisibleRefresh.js";

export type GoalWorkspaceState = {
  availability: Availability;
  detailAvailability: Availability;
  goals: GoalListItem[];
  total: number;
  truncated: boolean;
  selectedGoalId: string;
  detail: GoalDetailResponse | null;
  selectGoal: (goalId: string) => void;
  refresh: () => void;
};

export function useGoalWorkspace(client: RuntimeV2Client, enabled: boolean, onUnauthorized: () => void): GoalWorkspaceState {
  const [availability, setAvailability] = useState<Availability>("idle");
  const [detailAvailability, setDetailAvailability] = useState<Availability>("idle");
  const [goals, setGoals] = useState<GoalListItem[]>([]);
  const [total, setTotal] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [selectedGoalId, setSelectedGoalId] = useState("");
  const [detail, setDetail] = useState<GoalDetailResponse | null>(null);
  const [listRevision, setListRevision] = useState(0);
  const [detailRevision, setDetailRevision] = useState(0);
  const listRequest = useObservationRequest();
  const detailRequest = useObservationRequest();
  const loadedGoal = useRef("");
  const refresh = useCallback(() => {
    listRequest.requestRefresh(() => setListRevision(value => value + 1));
    detailRequest.requestRefresh(() => setDetailRevision(value => value + 1));
  }, [listRequest, detailRequest]);
  // Periodic reads skip occupied slots; only explicit refreshes queue a follow-up.
  useVisibleRefresh(enabled, () => {
    if (!listRequest.pending) setListRevision(value => value + 1);
    if (!detailRequest.pending) setDetailRevision(value => value + 1);
  }, 5_000);

  useEffect(() => {
    listRequest.cancel();
    if (!enabled) { setAvailability("idle"); return; }
    setAvailability(value => value === "idle" ? "loading" : value);
    void listRequest.run(signal => fetchGoals(client, undefined, signal), response => {
      if (response?.status === 401) { onUnauthorized(); return; }
      if (response?.status === 403) {
        setGoals([]); setTotal(0); setSelectedGoalId(""); setDetail(null);
        setAvailability("denied"); setDetailAvailability("denied"); return;
      }
      if (!response?.ok || !response.data) {
        setAvailability(value => value === "available" || value === "stale" ? "stale" : "error"); return;
      }
      const rows = Array.isArray(response.data.goals) ? response.data.goals : [];
      rows.sort((a, b) => Number(b.lifecycle === "active") - Number(a.lifecycle === "active") || b.updated_at_unix_ms - a.updated_at_unix_ms);
      setGoals(rows); setTotal(Math.max(response.data.total || 0, rows.length));
      setTruncated(Boolean(response.data.truncated)); setAvailability("available");
      setSelectedGoalId(current => rows.some(row => row.goal_id === current) ? current : rows[0]?.goal_id || "");
    });
    return () => listRequest.cancel();
  }, [client, enabled, onUnauthorized, listRevision, listRequest]);

  useEffect(() => {
    detailRequest.cancel();
    if (!enabled || !selectedGoalId) {
      loadedGoal.current = ""; setDetail(null); setDetailAvailability("idle"); return;
    }
    const changed = loadedGoal.current !== selectedGoalId;
    loadedGoal.current = selectedGoalId;
    if (changed) { setDetail(null); setDetailAvailability("loading"); }
    void detailRequest.run(signal => fetchGoal(client, selectedGoalId, signal), response => {
      if (response?.status === 401) { onUnauthorized(); return; }
      if (response?.status === 403 || response?.status === 404) {
        setDetail(null); setDetailAvailability("denied"); return;
      }
      if (!response?.ok || !response.data || response.data.goal.summary.goal_id !== selectedGoalId) {
        setDetailAvailability(value => value === "available" || value === "stale" ? "stale" : "error"); return;
      }
      setDetail(response.data); setDetailAvailability("available");
    });
    return () => detailRequest.cancel();
  }, [client, enabled, onUnauthorized, detailRevision, selectedGoalId, detailRequest]);

  return { availability, detailAvailability, goals, total, truncated, selectedGoalId, detail, selectGoal: setSelectedGoalId, refresh };
}
