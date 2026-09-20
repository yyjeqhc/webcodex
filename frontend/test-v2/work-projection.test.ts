import { describe, expect, it } from "vitest";
import {
  groupRecentProgress,
  phaseFromSession,
  workBucket,
  workItemFromRecent,
} from "../src/runtime-v2/model/work.js";
import { recentSession, sessionDetail, sessionItem } from "./fixtures.js";

describe("Work projection", () => {
  it("prioritizes current calls and active Jobs without inventing a phase", () => {
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
    });
    expect(workBucket(call)).toBe("running");
    expect(phaseFromSession(call)).toBe("Run focused Runtime tests");
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

  it("groups low-level activity by user-facing intent and remains bounded", () => {
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
    const groups = groupRecentProgress(detail);
    expect(groups.map((group) => group.intent)).toEqual(["tested", "edited", "explored"]);
    expect(groups[0].tools).toContain("cargo_test");
  });
});
