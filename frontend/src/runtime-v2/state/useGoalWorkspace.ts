import { useCallback, useEffect, useRef, useState } from "react";
import { fetchGoal, fetchGoals } from "../api/goals.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { GoalDetailResponse, GoalListItem } from "../model/goals.js";
import type { Availability } from "../model/types.js";

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

export function useGoalWorkspace(
  client: RuntimeV2Client,
  enabled: boolean,
  onUnauthorized: () => void,
): GoalWorkspaceState {
  const [availability, setAvailability] = useState<Availability>("idle");
  const [detailAvailability, setDetailAvailability] = useState<Availability>("idle");
  const [goals, setGoals] = useState<GoalListItem[]>([]);
  const [total, setTotal] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [selectedGoalId, setSelectedGoalId] = useState("");
  const [detail, setDetail] = useState<GoalDetailResponse | null>(null);
  const [revision, setRevision] = useState(0);
  const listRequest = useRef<AbortController | null>(null);
  const detailRequest = useRef<AbortController | null>(null);
  const loadedGoal = useRef("");
  const refresh = useCallback(() => setRevision((value) => value + 1), []);

  useEffect(() => {
    listRequest.current?.abort();
    if (!enabled) {
      setAvailability("idle");
      return;
    }
    const controller = new AbortController();
    listRequest.current = controller;
    setAvailability((value) => value === "idle" ? "loading" : value);
    void fetchGoals(client, undefined, controller.signal).then((response) => {
      if (listRequest.current !== controller || !response) return;
      listRequest.current = null;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403) {
        setGoals([]);
        setTotal(0);
        setSelectedGoalId("");
        setDetail(null);
        setAvailability("denied");
        setDetailAvailability("denied");
        return;
      }
      if (!response.ok || !response.data) {
        setAvailability((value) => value === "available" || value === "stale" ? "stale" : "error");
        return;
      }
      const rows = Array.isArray(response.data.goals) ? response.data.goals : [];
      rows.sort((a, b) => {
        const active = Number(b.lifecycle === "active") - Number(a.lifecycle === "active");
        return active || b.updated_at_unix_ms - a.updated_at_unix_ms;
      });
      setGoals(rows);
      setTotal(Math.max(response.data.total || 0, rows.length));
      setTruncated(Boolean(response.data.truncated));
      setAvailability("available");
      setSelectedGoalId((current) => rows.some((row) => row.goal_id === current)
        ? current
        : rows[0]?.goal_id || "");
    });
    return () => controller.abort();
  }, [client, enabled, onUnauthorized, revision]);

  useEffect(() => {
    detailRequest.current?.abort();
    if (!enabled || !selectedGoalId) {
      loadedGoal.current = "";
      setDetail(null);
      setDetailAvailability("idle");
      return;
    }
    const changed = loadedGoal.current !== selectedGoalId;
    loadedGoal.current = selectedGoalId;
    if (changed) {
      setDetail(null);
      setDetailAvailability("loading");
    }
    const controller = new AbortController();
    detailRequest.current = controller;
    void fetchGoal(client, selectedGoalId, controller.signal).then((response) => {
      if (detailRequest.current !== controller || !response) return;
      detailRequest.current = null;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403 || response.status === 404) {
        setDetail(null);
        setDetailAvailability("denied");
        return;
      }
      if (!response.ok || !response.data || response.data.goal.summary.goal_id !== selectedGoalId) {
        setDetailAvailability((value) => value === "available" || value === "stale" ? "stale" : "error");
        return;
      }
      setDetail(response.data);
      setDetailAvailability("available");
    });
    return () => controller.abort();
  }, [client, enabled, onUnauthorized, revision, selectedGoalId]);

  useEffect(() => {
    if (!enabled) return;
    const timer = window.setInterval(refresh, 5_000);
    return () => window.clearInterval(timer);
  }, [enabled, refresh]);

  return {
    availability,
    detailAvailability,
    goals,
    total,
    truncated,
    selectedGoalId,
    detail,
    selectGoal: setSelectedGoalId,
    refresh,
  };
}
