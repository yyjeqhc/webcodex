import type {
  ActivityPreview,
  AttentionOverview,
  RecentSession,
  SessionActivity,
  SessionDetail,
  SessionListItem,
  ValidationOverview,
} from "./types.js";

export type WorkBucket = "running" | "attention" | "active" | "recent";
export type ProgressIntent = "explored" | "edited" | "ran" | "tested" | "reviewed" | "delegated" | "waiting" | "activity";
export type ActivitySource = "session" | "window" | "workspace" | "job";
export type ActivitySignalTone = "live" | "observed" | "sparse" | "quiet" | "unavailable";

export type WorkItem = {
  key: string;
  sessionId: string;
  projectId: string;
  projectName: string;
  runner: string;
  title: string;
  lifecycle: string;
  mode: string;
  updatedAt: number;
  bucket: WorkBucket;
  phase: string;
  runningCall: boolean;
  runningJobs: number;
  attentionCount: number;
  validation: ValidationOverview;
  currentActivity?: ActivityPreview;
  lastActivity?: ActivityPreview;
  reportedProgress?: { reported_at: number; text: string };
};

export type ActivitySignal = {
  source: ActivitySource;
  label: string;
  status: string;
  detail: string;
  observedAt?: number;
  tone: ActivitySignalTone;
};

export type ProgressGroup = {
  source: ActivitySource;
  intent: ProgressIntent;
  label: string;
  count: number;
  tools: string[];
  paths: string[];
  latestAt: number;
  latestSummary?: string;
  state: string;
  actor?: { kind: string; name: string; id?: string };
  provenance?: string[];
};

function maxFinite(values: Array<number | undefined>): number | undefined {
  const finite = values.filter((value): value is number => Number.isFinite(value));
  return finite.length ? Math.max(...finite) : undefined;
}

function jobStatus(job: NonNullable<SessionDetail["jobs"]>[number]): { status: string; tone: ActivitySignalTone } {
  const status = job.status.toLowerCase();
  const activity = `${job.activity_state || ""} ${job.activity_phase || ""}`.toLowerCase();
  if (/block/.test(activity)) return { status: "Blocked", tone: "live" };
  if (/wait/.test(activity)) return { status: "Waiting", tone: "live" };
  if (/queued/.test(status)) return { status: "Queued", tone: "live" };
  if (/running|started/.test(status)) return { status: "Running", tone: "live" };
  if (job.terminal) return { status: "Terminal", tone: "quiet" };
  return { status: job.status || "Observed", tone: "observed" };
}

export function activitySignals(detail: SessionDetail | null, item?: WorkItem): ActivitySignal[] {
  const sessionUpdatedAt = detail?.updated_at || item?.updatedAt;
  const gapActivity = detail?.window_activity_after_last_session_record || [];
  const linkedWindows = detail?.linked_windows || [];
  const latestWindowAtMs = maxFinite([
    ...linkedWindows.map((window) => window.last_meaningful_activity_at_ms || window.last_seen_at_ms),
    ...gapActivity.map((activity) => activity.ended_at_ms || activity.started_at_ms),
  ]);
  const activeWindowRequests = linkedWindows.reduce((total, window) => total + (window.active_count || 0), 0);
  const sparseSessionRelation = gapActivity.length > 0 || linkedWindows.some((window) =>
    window.last_meaningful_activity_at_ms !== undefined && window.last_meaningful_activity_at_ms > window.last_linked_at_ms
  );

  let windowSignal: ActivitySignal;
  if (detail?.window_activity_available === false) {
    windowSignal = { source: "window", label: "Window / Model", status: "Unavailable", detail: "Window activity is not available to this credential.", tone: "unavailable" };
  } else if (activeWindowRequests > 0) {
    windowSignal = { source: "window", label: "Window / Model", status: "Active", detail: "WebCodex request currently in flight.", observedAt: latestWindowAtMs ? Math.floor(latestWindowAtMs / 1000) : undefined, tone: "live" };
  } else if (latestWindowAtMs !== undefined) {
    windowSignal = { source: "window", label: "Window / Model", status: "Last WebCodex call", detail: sparseSessionRelation ? "Window activity continues beyond the latest Session-linked record." : "Observed from Window-scoped WebCodex activity.", observedAt: Math.floor(latestWindowAtMs / 1000), tone: "observed" };
  } else {
    windowSignal = { source: "window", label: "Window / Model", status: "Not observed", detail: "No Window-scoped WebCodex activity is loaded.", tone: "quiet" };
  }

  const latestSessionActivityAt = detail
    ? maxFinite(detail.activity.map((activity) => activity.finished_at ?? activity.started_at))
    : undefined;
  const sessionSignal: ActivitySignal = sparseSessionRelation
    ? {
        source: "session",
        label: "Workflow Session",
        status: "Sparse activity",
        detail: "Window activity is newer than explicit Session-linked work; this is provenance sparsity, not model idleness.",
        observedAt: latestSessionActivityAt ?? sessionUpdatedAt,
        tone: "sparse",
      }
    : latestSessionActivityAt !== undefined || sessionUpdatedAt !== undefined
      ? {
          source: "session",
          label: "Workflow Session",
          status: "Last linked activity",
          detail: "Exact retained Session progress / collaboration evidence.",
          observedAt: latestSessionActivityAt ?? sessionUpdatedAt,
          tone: "observed",
        }
      : {
          source: "session",
          label: "Workflow Session",
          status: "Not observed",
          detail: "No explicit Session-linked activity is loaded.",
          tone: "quiet",
        };

  let workspaceSignal: ActivitySignal;
  if (detail?.workspace_activity_available === false || detail?.workspace_activity_available === undefined) {
    workspaceSignal = { source: "workspace", label: "Workspace", status: "Unavailable", detail: "Workspace activity projection is not available.", tone: "unavailable" };
  } else if (detail.workspace_last_activity) {
    workspaceSignal = {
      source: "workspace",
      label: "Workspace",
      status: "Last action",
      detail: `${detail.workspace_last_activity.tool}${detail.workspace_last_activity.success ? "" : " · failed"}`,
      observedAt: detail.workspace_last_activity.created_at,
      tone: "observed",
    };
  } else {
    workspaceSignal = { source: "workspace", label: "Workspace", status: "Not observed", detail: "No workspace ledger action is loaded for this Project.", tone: "quiet" };
  }

  let jobSignal: ActivitySignal;
  if (detail?.job_activity_available === false || detail?.job_activity_available === undefined) {
    jobSignal = { source: "job", label: "Job", status: "Unavailable", detail: "Job lifecycle projection is not available.", tone: "unavailable" };
  } else if (detail.jobs?.length) {
    const ordered = [...detail.jobs].sort((a, b) => {
      const aActive = Number(!a.terminal);
      const bActive = Number(!b.terminal);
      if (aActive !== bActive) return bActive - aActive;
      return (b.ended_at || b.started_at || b.created_at) - (a.ended_at || a.started_at || a.created_at);
    });
    const job = ordered[0];
    const projected = jobStatus(job);
    jobSignal = {
      source: "job",
      label: "Job",
      status: projected.status,
      detail: [job.kind, job.activity_phase].filter(Boolean).join(" · ") || job.status,
      observedAt: job.ended_at || job.started_at || job.created_at,
      tone: projected.tone,
    };
  } else {
    jobSignal = { source: "job", label: "Job", status: "Not observed", detail: "No Job lifecycle exists for this Session.", tone: "quiet" };
  }

  return [windowSignal, sessionSignal, workspaceSignal, jobSignal];
}

export function attentionCount(attention: AttentionOverview): number {
  return attention.open_guidance + attention.open_questions + attention.open_risks + attention.open_todos;
}

export function workBucket(session: SessionListItem): WorkBucket {
  // Only Runner-owned Jobs are authoritative live execution here. An unfinished
  // Session call is retained ledger evidence and can outlive the originating Window.
  if (session.running_jobs > 0) return "running";
  if (attentionCount(session.overview.attention) > 0) return "attention";
  return session.lifecycle === "active" ? "active" : "recent";
}

function boundedText(value: string | undefined, max = 140): string {
  const text = (value || "").trim().replace(/\s+/g, " ");
  return text.length <= max ? text : text.slice(0, max - 1) + "…";
}

export function phaseFromSession(session: SessionListItem): string {
  if (session.running_jobs > 0) {
    return session.running_jobs === 1 ? "1 running Job" : `${session.running_jobs} running Jobs`;
  }
  const attention = attentionCount(session.overview.attention);
  if (attention > 0) return attention === 1 ? "Needs attention" : `${attention} attention items`;
  if (session.overview.reported_progress?.text) return boundedText(session.overview.reported_progress.text);
  if (session.last_activity) {
    return boundedText(session.last_activity.summary) || session.last_activity.tool || session.last_activity.kind;
  }
  return session.lifecycle || "Retained";
}

export function workItemFromRecent(session: RecentSession): WorkItem {
  return {
    key: `${session.project_id}:${session.session_id}`,
    sessionId: session.session_id,
    projectId: session.project_id,
    projectName: session.project_name || session.project_id,
    runner: session.client_id,
    title: session.title,
    lifecycle: session.lifecycle,
    mode: session.mode,
    updatedAt: session.updated_at,
    bucket: workBucket(session),
    phase: phaseFromSession(session),
    runningCall: session.running_call,
    runningJobs: session.running_jobs,
    attentionCount: attentionCount(session.overview.attention),
    validation: session.overview.validation,
    currentActivity: session.current_activity,
    lastActivity: session.last_activity,
    reportedProgress: session.overview.reported_progress,
  };
}

export function workItemFromProjectSession(
  session: SessionListItem,
  projectId: string,
  projectName: string,
  runner: string,
): WorkItem {
  return workItemFromRecent({ ...session, project_id: projectId, project_name: projectName, client_id: runner });
}

export function intentForActivity(activity: Pick<SessionActivity, "kind" | "tool">): ProgressIntent {
  const kind = (activity.kind || "").toLowerCase();
  const tool = (activity.tool || "").toLowerCase();
  const explorationTool = tool === "rg" || tool === "read_files" || tool === "search_and_read" || tool === "search_project_texts" || tool === "find" || tool.startsWith("list_");
  if (/explor|read|search|inspect/.test(kind) || explorationTool) return "explored";
  if (/edit|write|patch|mutat/.test(kind) || /apply|edit|write|create|delete|rename/.test(tool)) return "edited";
  if (/valid|test|check|build|format/.test(kind) || /test|check|build|fmt|clippy/.test(tool)) return "tested";
  if (/review|diff/.test(kind) || /review|diff|show_changes|git_status/.test(tool)) return "reviewed";
  if (/delegat|agent_task|handoff/.test(kind) || /delegate|agent_task/.test(tool)) return "delegated";
  if (/wait|block/.test(kind) || /wait_for|observe_jobs/.test(tool)) return "waiting";
  if (/run|shell|process|job|exec/.test(kind) || /run_|cargo|shell|process/.test(tool)) return "ran";
  return "activity";
}

const INTENT_LABELS: Record<ProgressIntent, string> = {
  explored: "Explored",
  edited: "Edited",
  ran: "Ran",
  tested: "Tested",
  reviewed: "Reviewed",
  delegated: "Delegated",
  waiting: "Waiting",
  activity: "Activity",
};

export function groupRecentProgress(detail: SessionDetail | null): ProgressGroup[] {
  if (!detail) return [];
  const groups: ProgressGroup[] = [];
  const append = (group: ProgressGroup) => {
    const previous = groups.at(-1);
    if (
      previous &&
      previous.source === group.source &&
      previous.intent === group.intent &&
      previous.state === group.state &&
      previous.tools.join("\u0000") === group.tools.join("\u0000") &&
      (previous.provenance || []).join("\u0000") === (group.provenance || []).join("\u0000")
    ) {
      previous.count += group.count;
      previous.latestAt = Math.max(previous.latestAt, group.latestAt);
      previous.latestSummary = group.latestSummary || previous.latestSummary;
      previous.paths = Array.from(new Set([...previous.paths, ...group.paths]));
      previous.provenance = Array.from(new Set([...(previous.provenance || []), ...(group.provenance || [])]));
      return;
    }
    groups.push(group);
  };

  const sourceGroups: ProgressGroup[] = [];
  for (const activity of detail.activity) {
    const intent = intentForActivity(activity);
    const tools = [activity.tool, ...(activity.group_tools || [])].filter((value): value is string => Boolean(value));
    sourceGroups.push({
      source: "session",
      intent,
      label: INTENT_LABELS[intent],
      count: Math.max(1, activity.group_count || 1),
      tools: Array.from(new Set(tools)),
      paths: Array.from(new Set(activity.paths || [])),
      latestAt: activity.finished_at ?? activity.started_at,
      latestSummary: boundedText(activity.summary) || undefined,
      state: activity.state,
      provenance: ["exact Session ledger"],
    });
  }
  for (const activity of detail.window_activity_after_last_session_record || []) {
    const intent = intentForActivity({ kind: activity.activity_kind || "activity", tool: activity.tool_name });
    sourceGroups.push({
      source: "window",
      intent,
      label: INTENT_LABELS[intent],
      count: 1,
      tools: activity.tool_name ? [activity.tool_name] : [],
      paths: [],
      latestAt: Math.floor((activity.ended_at_ms || activity.started_at_ms) / 1000),
      latestSummary: boundedText(activity.activity_presentation || activity.tool_name || activity.method) || undefined,
      state: activity.status,
      provenance: [
        "Window observation",
        ...(activity.project ? [activity.project] : []),
        ...activity.workflow_sessions.map((relation) => `${relation.relation} · ${relation.workflow_session_id}`),
      ],
    });
  }
  if (detail.workspace_activity_available && detail.workspace_last_activity) {
    sourceGroups.push({
      source: "workspace",
      intent: intentForActivity({ kind: "workspace", tool: detail.workspace_last_activity.tool }),
      label: "Workspace action",
      count: 1,
      tools: [detail.workspace_last_activity.tool],
      paths: [],
      latestAt: detail.workspace_last_activity.created_at,
      latestSummary: detail.workspace_last_activity.success ? "Workspace action completed" : "Workspace action failed",
      state: detail.workspace_last_activity.success ? "success" : "failed",
      provenance: ["Project workspace ledger"],
    });
  }
  for (const job of detail.jobs || []) {
    const projected = jobStatus(job);
    sourceGroups.push({
      source: "job",
      intent: /wait|block|queue/i.test(`${projected.status} ${job.activity_phase || ""}`) ? "waiting" : "ran",
      label: "Job",
      count: 1,
      tools: [],
      paths: [],
      latestAt: job.ended_at || job.started_at || job.created_at,
      latestSummary: [projected.status, job.kind, job.activity_phase].filter(Boolean).join(" · "),
      state: job.status,
      provenance: [`Job ${job.job_id}`],
    });
  }

  sourceGroups.sort((a, b) => a.latestAt - b.latestAt || a.source.localeCompare(b.source));
  for (const group of sourceGroups) append(group);
  return groups;
}

export function selectedWorkFromDetail(item: WorkItem, detail: SessionDetail | null): WorkItem {
  if (!detail) return item;
  const phase = phaseFromSession(detail);
  return {
    ...item,
    title: detail.title,
    lifecycle: detail.lifecycle,
    mode: detail.mode,
    updatedAt: detail.updated_at,
    bucket: workBucket(detail),
    phase,
    runningCall: detail.running_call,
    runningJobs: detail.running_jobs,
    attentionCount: attentionCount(detail.overview.attention),
    validation: detail.overview.validation,
    currentActivity: detail.current_activity || item.currentActivity,
    lastActivity: detail.last_activity || item.lastActivity,
    reportedProgress: detail.overview.reported_progress || item.reportedProgress,
  };
}
