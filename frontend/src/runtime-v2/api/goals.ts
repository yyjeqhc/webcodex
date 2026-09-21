import type { RuntimeV2Client } from "./client.js";
import type { GoalDetailResponse, GoalsResponse } from "../model/goals.js";

export function fetchGoals(client: RuntimeV2Client, project?: string, signal?: AbortSignal) {
  return client.post<GoalsResponse>("goals", project ? { project } : {}, signal);
}

export function fetchGoal(client: RuntimeV2Client, goalId: string, signal?: AbortSignal) {
  return client.post<GoalDetailResponse>("goal", { goal_id: goalId }, signal);
}
