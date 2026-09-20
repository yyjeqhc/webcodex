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

export type ProgressGroup = {
  intent: ProgressIntent;
  label: string;
  count: number;
  tools: string[];
  paths: string[];
  latestAt: number;
  latestSummary?: string;
  state: string;
  actor?: { kind: string; name: string; id?: string };
};

export function attentionCount(attention: AttentionOverview): number {
  return attention.open_guidance + attention.open_questions + attention.open_risks + attention.open_todos;
}

export function workBucket(session: SessionListItem): WorkBucket {
  if (session.running_call || session.running_jobs > 0) return "running";
  if (attentionCount(session.overview.attention) > 0) return "attention";
  return session.lifecycle === "active" ? "active" : "recent";
}

function boundedText(value: string | undefined, max = 140): string {
  const text = (value || "").trim().replace(/\s+/g, " ");
  return text.length <= max ? text : text.slice(0, max - 1) + "…";
}

export function phaseFromSession(session: SessionListItem): string {
  const current = session.current_activity;
  if (session.running_call && current) {
    return boundedText(current.summary) || current.tool || current.kind || "Working";
  }
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

export function groupRecentProgress(detail: SessionDetail | null, limit = 80): ProgressGroup[] {
  if (!detail) return [];
  const bounded = detail.activity.slice(-Math.max(1, limit));
  const groups: ProgressGroup[] = [];
  for (const activity of bounded) {
    const intent = intentForActivity(activity);
    const latestAt = activity.finished_at ?? activity.started_at;
    const tools = [activity.tool, ...activity.group_tools].filter((value): value is string => Boolean(value));
    const paths = activity.paths || [];
    const previous = groups.at(-1);
    if (previous && previous.intent === intent && previous.state === activity.state) {
      previous.count += Math.max(1, activity.group_count || 1);
      previous.latestAt = Math.max(previous.latestAt, latestAt);
      previous.latestSummary = boundedText(activity.summary) || previous.latestSummary;
      previous.tools = Array.from(new Set([...previous.tools, ...tools])).slice(0, 8);
      previous.paths = Array.from(new Set([...previous.paths, ...paths])).slice(0, 12);
      continue;
    }
    groups.push({
      intent,
      label: INTENT_LABELS[intent],
      count: Math.max(1, activity.group_count || 1),
      tools: Array.from(new Set(tools)).slice(0, 8),
      paths: Array.from(new Set(paths)).slice(0, 12),
      latestAt,
      latestSummary: boundedText(activity.summary) || undefined,
      state: activity.state,
    });
  }
  return groups.reverse().slice(0, 12);
}

export function selectedWorkFromDetail(item: WorkItem, detail: SessionDetail | null): WorkItem {
  if (!detail) return item;
  const detailPhase = phaseFromSession(detail);
  const phase = detail.running_call && !detail.current_activity && item.currentActivity
    ? item.phase
    : detailPhase;
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
