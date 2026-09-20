import { describe, expect, it } from "vitest";
import {
  groupRecentProgress,
  activitySignals,
  phaseFromSession,
  workBucket,
  workItemFromRecent,
} from "../src/runtime-v2/model/work.js";
import { recentSession, sessionDetail, sessionItem, sparseActivitySessionDetail } from "./fixtures.js";

describe("Work projection", () => {
  it("treats only Runner Jobs as authoritative live execution", () => {
    const job = sessionItem({
      running_jobs: 1,
      running_jobs_complete: true,
      last_activity: undefined,
      overview: {
        ...sessionItem().overview,
        reported_progress: undefined,
      },
    });
    expect(workBucket(job)).toBe("running");
    expect(phaseFromSession(job)).toBe("1 running Job");

    const call = sessionItem({
      running_call: true,
      current_activity: {
        kind: "validation",
        tool: "cargo_test",
        state: "running",
        execution_state: "running",
        job_handoff: false,
        summary: "Run focused Runtime tests",
        paths: [],
      },
      last_activity: undefined,
      overview: {
        ...sessionItem().overview,
        reported_progress: undefined,
      },
    });
    expect(workBucket(call)).toBe("active");
    expect(phaseFromSession(call)).toBe("active");
  });

  it("keeps inactive/no-Job Sessions out of Running", () => {
    expect(workBucket(sessionItem({ running_jobs: 0, running_call: false }))).toBe("active");
    expect(workBucket(sessionItem({ lifecycle: "closed", running_jobs: 0, running_call: false }))).toBe("recent");
  });

  it("surfaces attention before ordinary active work", () => {
    const session = sessionItem({
      overview: {
        ...sessionItem().overview,
        attention: {
          open_guidance: 0,
          open_questions: 1,
          open_risks: 0,
          open_todos: 1,
        },
      },
    });
    expect(workBucket(session)).toBe("attention");
    expect(phaseFromSession(session)).toBe("2 attention items");
  });

  it("presents canonical current validation, including pass after an older failure", () => {
    const freshPass = recentSession({
      overview: {
        ...recentSession().overview,
        validation: {
          state: "pass",
          latest_kind: "test",
          latest_at: 1_790_000_100,
          unresolved_failure_count: 0,
          tests_run_count: 42,
          history_complete: true,
          history_truncated: false,
        },
      },
    });
    expect(workItemFromRecent(freshPass).validation.state).toBe("pass");
    expect(workItemFromRecent(freshPass).validation.unresolved_failure_count).toBe(0);

    const failure = recentSession({
      overview: {
        ...recentSession().overview,
        validation: {
          state: "failure",
          latest_kind: "test",
          latest_at: 1_790_000_100,
          unresolved_failure_count: 2,
          history_complete: true,
          history_truncated: false,
        },
      },
    });
    expect(workItemFromRecent(failure).validation.state).toBe("failure");
    expect(workItemFromRecent(failure).validation.unresolved_failure_count).toBe(2);
  });

  it("accepts server activity rows with serde-omitted empty arrays", () => {
    const detail = sessionDetail({
      activity: [{
        kind: "run",
        tool: "run_process",
        state: "success",
        job_handoff: false,
        started_at: 1_789_999_250,
        finished_at: 1_789_999_251,
        summary: "Executed command",
      }],
      activity_total: 1,
      activity_returned: 1,
    });

    expect(groupRecentProgress(detail)).toEqual([
      expect.objectContaining({
        intent: "ran",
        tools: ["run_process"],
        paths: [],
      }),
    ]);
  });

  it("keeps Window, Session, Workspace, and Job activity semantically independent", () => {
    const detail = sparseActivitySessionDetail();
    const signals = activitySignals(detail);
    expect(signals.find((signal) => signal.source === "window")).toEqual(expect.objectContaining({
      status: "Last WebCodex call",
      tone: "observed",
    }));
    expect(signals.find((signal) => signal.source === "session")).toEqual(expect.objectContaining({
      status: "Sparse activity",
      tone: "sparse",
    }));
    expect(signals.find((signal) => signal.source === "workspace")).toEqual(expect.objectContaining({
      status: "Last action",
      detail: "run_shell",
    }));
    expect(signals.find((signal) => signal.source === "job")).toEqual(expect.objectContaining({
      status: "Running",
    }));

    const noJob = activitySignals({ ...detail, jobs: [] });
    expect(noJob.find((signal) => signal.source === "job")?.status).toBe("Not observed");
    expect(noJob.find((signal) => signal.source === "window")?.status).toBe("Last WebCodex call");
  });

  it("groups low-level activity by user-facing intent without a second UI history cap", () => {
    const detail = sessionDetail({
      activity: [
        ...sessionDetail().activity,
        {
          kind: "validation",
          tool: "cargo_test",
          state: "success",
          execution_state: "completed",
          job_handoff: false,
          started_at: 1_789_999_300,
          finished_at: 1_789_999_301,
          duration_ms: 1000,
          summary: "Focused tests passed",
          paths: [],
          group_kinds: [],
          group_tools: ["cargo_check"],
        },
      ],
      activity_total: 3,
      activity_returned: 3,
    });
    const manyActivities = Array.from({ length: 20 }, (_, index) => ({
      kind: index % 2 ? "run" : "exploration",
      tool: index % 2 ? `run_tool_${index}` : `read_tool_${index}`,
      state: "success",
      job_handoff: false,
      started_at: 1_790_000_400 + index,
      finished_at: 1_790_000_400 + index,
      summary: `activity ${index}`,
      paths: [],
      group_kinds: [],
      group_tools: [],
    }));
    detail.activity.push(...manyActivities);
    const groups = groupRecentProgress(detail);
    expect(groups.slice(0, 3).map((group) => group.intent)).toEqual(["explored", "edited", "tested"]);
    expect(groups[2]?.tools).toContain("cargo_test");
    expect(groups.map((group) => group.latestAt)).toEqual([...groups.map((group) => group.latestAt)].sort((a, b) => a - b));
    expect(groups.length).toBeGreaterThan(12);
    expect(groups.some((group) => group.latestSummary === "activity 0")).toBe(true);
    expect(groups.some((group) => group.latestSummary === "activity 19")).toBe(true);
  });
});
