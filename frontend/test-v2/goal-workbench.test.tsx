import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { GoalWorkbench } from "../src/runtime-v2/components/GoalWorkbench.js";
import { UiProvider } from "../src/ui/UiProvider.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";

const goalId = "wc_goal_1234567890abcdef";
const sessionId = "wc_sess_1234567890abcdef";
const controllerId = "wc_dagent_controller1234";
const workerId = "wc_dagent_worker123456";
const windowKey = "a".repeat(64);

function client(): RuntimeV2Client {
  return {
    post: vi.fn(async (path: string) => {
      if (path === "goals") return {
        ok: true,
        status: 200,
        data: {
          goals: [{ goal_id: goalId, title: "Runtime V2 Goal Workbench", lifecycle: "active", revision: 5, updated_at_unix_ms: 1_790_000_500_000, agent_task_count: 1, workflow_session_count: 1, total_step_count: 5, completed_step_count: 2, current_step_id: "implement", current_step_title: "Implement read-only Goal Workbench", progress_summary: "Survey and design complete", checkpoint_at_unix_ms: 1_790_000_490_000, project_ids: ["agent:special:webcodex"] }],
          total: 1,
          source_total: 1,
          truncated: false,
        },
      };
      if (path === "goal") return {
        ok: true,
        status: 200,
        data: {
          goal: {
            summary: { goal_id: goalId, title: "Runtime V2 Goal Workbench", lifecycle: "active", revision: 5, created_at_unix_ms: 1_790_000_000_000, updated_at_unix_ms: 1_790_000_500_000, terminal_at_unix_ms: null, agent_task_count: 1, workflow_session_count: 1 },
            plan: { completion_conditions: ["UI dogfood passes", "Final review complete"], steps: [
              { id: "survey", title: "Survey", status: "completed", updated_at_unix_ms: 1 },
              { id: "design", title: "Design", status: "completed", updated_at_unix_ms: 2 },
              { id: "implement", title: "Implement read-only Goal Workbench", status: "in_progress", updated_at_unix_ms: 3 },
              { id: "validate", title: "Validate", status: "pending", updated_at_unix_ms: 4 },
              { id: "review", title: "Review", status: "pending", updated_at_unix_ms: 5 },
            ], progress_summary: "Survey and design complete", checkpoint_at_unix_ms: 1_790_000_490_000 },
            objective: "Make durable Goals the highest-level work truth.",
            controller_agent_id: controllerId,
            terminal_reason: null,
            correlations: [],
          },
          goal_plan: {
            version: 3, goal_id: goalId, title: "Runtime V2 Goal Workbench", total_step_count: 5, completed_step_count: 2, current_step_id: "implement",
            steps: [
              { id: "survey", title: "Survey", status: "completed", updated_at_unix_ms: 1 },
              { id: "design", title: "Design", status: "completed", updated_at_unix_ms: 2 },
              { id: "implement", title: "Implement read-only Goal Workbench", status: "in_progress", updated_at_unix_ms: 3 },
              { id: "validate", title: "Validate", status: "pending", updated_at_unix_ms: 4 },
              { id: "review", title: "Review", status: "pending", updated_at_unix_ms: 5 },
            ],
            progress_summary: "Survey and design complete", checkpoint_at_unix_ms: 1_790_000_490_000, controller_agent_id: controllerId, lifecycle: "active", revision: 5, updated_at_unix_ms: 1_790_000_500_000, terminal_at_unix_ms: null, agent_task_count: 1, workflow_session_count: 1,
            activity: { available: true, state: "active", idle_threshold_ms: 300000, observation_lease_ms: 60000, last_seen_at_ms: 1_790_000_499_000, last_meaningful_activity_at_ms: 1_790_000_498_000, quiet_for_ms: 2000, linked_window_count: 1, active_meaningful_request_count: 1, coverage_partial: false },
            continuity: { available: true, state: "ready", production_auto_resume_available: true, wake_state: null, host_delivery: "not_started", fresh_turn: "not_confirmed", last_resume_at_unix_ms: 1_790_000_400_000 },
          },
          projects: [{ id: "agent:special:webcodex", client_id: "special", name: "WebCodex", path: "/root/git/webcodex", connected: true }],
          sessions: [{ session_id: sessionId, title: "Goal implementation Session", lifecycle: "active", mode: "normal", updated_at: 1_790_000_498, running_jobs: 1, project_id: "agent:special:webcodex", project_name: "WebCodex", client_id: "special" }],
          tasks: [{ summary: { task_id: "wc_task_worker123456", assignee_agent_id: workerId, title: "Validate Goal Workbench", referenced_project_id: "agent:special:webcodex", state: "active", created_at_unix_ms: 1, updated_at_unix_ms: 1_790_000_480_000, terminal_at_unix_ms: null, latest_attempt: null, execution_bound: true, execution_kind: "coding_agent_run", execution_status: "running", recovery_kind: "none" }, instruction: "Run focused frontend validation." }],
          waits: [{ wait_id: "wc_agent_wait_1234567890abcdef", target_agent_id: controllerId, goal_id: goalId, state: "waiting", mode: "all", revision: 1, created_at_unix_ms: 1, updated_at_unix_ms: 2, source_count: 2, match_count: 1, match_sequence: 1, sources: [{ ordinal: 0, kind: "agent_task_terminal", task_id: "wc_task_worker123456" }, { ordinal: 1, kind: "agent_task_terminal", task_id: "wc_task_worker654321" }], matches: [{ sequence: 1, kind: "agent_task_terminal", task_id: "wc_task_worker123456", task_attempt_id: "wc_attempt_123456789", terminal_task_state: "succeeded", occurred_at_unix_ms: 2 }] }],
          waits_truncated: false,
          agents: [{ agent_id: controllerId, handle: "goal-controller", display_name: "Goal Controller", profile_revision: 2, current_controller_generation: 3, active_endpoint_count: 1, queued_delivery_count: 0, unresolved_wake_count: 0 }, { agent_id: workerId, handle: "validator", display_name: "Validator Worker", profile_revision: 1, active_endpoint_count: 1 }],
          windows: [{ client_window_key: windowKey, source: "openai-session", last_seen_at_ms: 1_790_000_499_000, last_meaningful_activity_at_ms: 1_790_000_498_000, active_count: 1, session_ids: [sessionId] }],
        },
      };
      throw new Error("unexpected path " + path);
    }),
  } as unknown as RuntimeV2Client;
}

describe("Goal Workbench", () => {
  it("renders durable Goal truth and navigates to exact existing resources", async () => {
    const onSurfaceChange = vi.fn();
    const onOpenSession = vi.fn();
    const onOpenAgent = vi.fn();
    const onOpenWindow = vi.fn();
    render(
      <UiProvider><GoalWorkbench
        client={client()}
        language="en"
        projects={[{ id: "agent:special:webcodex", client_id: "special", name: "WebCodex", path: "/root/git/webcodex", connected: true }]}
        surface="goals"
        onSurfaceChange={onSurfaceChange}
        onOpenSession={onOpenSession}
        onOpenAgent={onOpenAgent}
        onOpenWindow={onOpenWindow}
        onUnauthorized={vi.fn()}
      /></UiProvider>,
    );

    expect(await screen.findByRole("heading", { name: "Runtime V2 Goal Workbench" })).toBeTruthy();
    expect(screen.getByText("2 / 5 steps completed")).toBeTruthy();
    expect(screen.getByTestId("goal-step-implement").textContent).toContain("Implement read-only Goal Workbench");
    expect(screen.getByTestId("goal-step-survey").hasAttribute("data-completed")).toBe(true);
    expect(screen.getByTestId("goal-step-implement").hasAttribute("data-progress")).toBe(true);
    expect(screen.getByTestId("goal-step-validate").hasAttribute("data-progress")).toBe(false);
    expect(screen.getByTestId("goal-step-survey").querySelector(".mantine-Stepper-stepCompletedIcon svg")).toBeTruthy();
    expect(screen.getAllByText("Goal Controller").length).toBeGreaterThan(0);
    expect(screen.getByText("ALL 1 / 2")).toBeTruthy();
    expect(screen.getByText("Goal implementation Session")).toBeTruthy();
    expect(screen.getByText("Validate Goal Workbench")).toBeTruthy();
    expect(screen.getByText(/Window aaaaaaaaaa/)).toBeTruthy();

    fireEvent.click(screen.getByText("Goal implementation Session"));
    expect(onOpenSession).toHaveBeenCalledWith(expect.objectContaining({ sessionId }));
    fireEvent.click(screen.getAllByText("Goal Controller")[0]);
    expect(onOpenAgent).toHaveBeenCalledWith(controllerId);
    fireEvent.click(screen.getByText(/Window aaaaaaaaaa/));
    expect(onOpenWindow).toHaveBeenCalledWith(windowKey);
    fireEvent.click(screen.getByRole("radio", { name: /Activity/ }));
    expect(onSurfaceChange).toHaveBeenCalledWith("windows");
  });

  it("keeps Goal list and detail read-only", async () => {
    const rendered = render(
      <UiProvider><GoalWorkbench
        client={client()}
        language="en"
        projects={[]}
        surface="goals"
        onSurfaceChange={vi.fn()}
        onOpenSession={vi.fn()}
        onOpenAgent={vi.fn()}
        onOpenWindow={vi.fn()}
        onUnauthorized={vi.fn()}
      /></UiProvider>,
    );
    await screen.findByRole("heading", { name: "Runtime V2 Goal Workbench" });
    expect(rendered.container.querySelector("input[aria-label='Search Goals']")).toBeTruthy();
    expect(rendered.container.querySelector("textarea")).toBeNull();
    expect(screen.queryByRole("button", { name: /complete goal/i })).toBeNull();
    await waitFor(() => expect(rendered.container.querySelectorAll(".goal-resource-row").length).toBeGreaterThan(0));
  });
});
