import type { DurableAgent } from "./agents.js";
import type { ProjectRow } from "./types.js";

export type GoalLifecycle = "active" | "completed" | "cancelled" | string;
export type GoalStepStatus = "pending" | "in_progress" | "completed" | string;

export type GoalStep = {
  id: string;
  title: string;
  status: GoalStepStatus;
  updated_at_unix_ms: number;
};

export type GoalListItem = {
  goal_id: string;
  title: string;
  lifecycle: GoalLifecycle;
  revision: number;
  updated_at_unix_ms: number;
  agent_task_count: number;
  workflow_session_count: number;
  total_step_count: number;
  completed_step_count: number;
  current_step_id?: string | null;
  current_step_title?: string | null;
  progress_summary?: string | null;
  checkpoint_at_unix_ms?: number | null;
  project_ids: string[];
};

export type GoalsResponse = {
  goals: GoalListItem[];
  total: number;
  source_total: number;
  truncated: boolean;
};

export type GoalActivityObservation = {
  available: boolean;
  state: string;
  idle_threshold_ms: number;
  observation_lease_ms: number;
  last_seen_at_ms?: number | null;
  last_meaningful_activity_at_ms?: number | null;
  quiet_for_ms?: number | null;
  linked_window_count?: number | null;
  active_meaningful_request_count?: number | null;
  coverage_partial: boolean;
};

export type GoalContinuityObservation = {
  available: boolean;
  state: string;
  production_auto_resume_available: boolean;
  wake_state?: string | null;
  host_delivery: string;
  fresh_turn: string;
  attention_candidate_at_unix_ms?: number | null;
  attention_created_at_unix_ms?: number | null;
  wake_created_at_unix_ms?: number | null;
  host_dispatch_prepared_at_unix_ms?: number | null;
  host_dispatch_accepted_at_unix_ms?: number | null;
  host_dispatch_unknown_at_unix_ms?: number | null;
  wake_consumed_at_unix_ms?: number | null;
  first_post_resume_meaningful_at_unix_ms?: number | null;
  last_post_resume_meaningful_at_unix_ms?: number | null;
  last_resume_at_unix_ms?: number | null;
};

export type GoalPlanProjection = {
  version: number;
  goal_id: string;
  title: string;
  total_step_count: number;
  completed_step_count: number;
  current_step_id?: string | null;
  steps: GoalStep[];
  progress_summary?: string | null;
  checkpoint_at_unix_ms?: number | null;
  controller_agent_id?: string | null;
  lifecycle: GoalLifecycle;
  revision: number;
  updated_at_unix_ms: number;
  terminal_at_unix_ms?: number | null;
  agent_task_count: number;
  workflow_session_count: number;
  activity: GoalActivityObservation;
  continuity: GoalContinuityObservation;
};

export type GoalRaw = {
  summary: {
    goal_id: string;
    title: string;
    lifecycle: GoalLifecycle;
    revision: number;
    created_at_unix_ms: number;
    updated_at_unix_ms: number;
    terminal_at_unix_ms?: number | null;
    agent_task_count: number;
    workflow_session_count: number;
  };
  plan: {
    completion_conditions: string[];
    steps: GoalStep[];
    progress_summary?: string | null;
    checkpoint_at_unix_ms?: number | null;
  };
  objective: string;
  controller_agent_id?: string | null;
  terminal_reason?: string | null;
  correlations: Array<{
    kind: string;
    reference_id: string;
    created_at_unix_ms: number;
  }>;
};

export type GoalSession = {
  session_id: string;
  title: string;
  lifecycle: string;
  mode: string;
  updated_at: number;
  running_jobs: number;
  project_id: string;
  project_name?: string | null;
  client_id: string;
};

export type GoalTask = {
  summary: {
    task_id: string;
    assignee_agent_id?: string | null;
    title: string;
    referenced_project_id?: string | null;
    state: string;
    created_at_unix_ms: number;
    updated_at_unix_ms: number;
    terminal_at_unix_ms?: number | null;
    latest_attempt?: {
      attempt_id: string;
      state: string;
      attempt_number: number;
      terminal_result?: string | null;
      terminal_reason?: string | null;
    } | null;
    execution_bound: boolean;
    execution_kind?: string | null;
    execution_status?: string | null;
    recovery_kind: string;
  };
  instruction: string;
};

export type GoalWait = {
  wait_id: string;
  target_agent_id: string;
  goal_id?: string | null;
  state: string;
  mode: "any" | "all" | string;
  revision: number;
  created_at_unix_ms: number;
  updated_at_unix_ms: number;
  triggered_at_unix_ms?: number | null;
  resumed_at_unix_ms?: number | null;
  cancelled_at_unix_ms?: number | null;
  source_count: number;
  match_count: number;
  match_sequence: number;
  sources: Array<{ ordinal: number; kind: string; task_id: string }>;
  matches: Array<{
    sequence: number;
    kind: string;
    task_id: string;
    task_attempt_id: string;
    terminal_task_state: string;
    occurred_at_unix_ms: number;
  }>;
};

export type GoalWindow = {
  client_window_key: string;
  source: string;
  last_seen_at_ms: number;
  last_meaningful_activity_at_ms?: number | null;
  active_count: number;
  session_ids: string[];
};

export type GoalDetailResponse = {
  goal: GoalRaw;
  goal_plan: GoalPlanProjection;
  projects: ProjectRow[];
  sessions: GoalSession[];
  tasks: GoalTask[];
  waits: GoalWait[];
  waits_truncated: boolean;
  agents: DurableAgent[];
  windows: GoalWindow[];
};
