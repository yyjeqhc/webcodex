import { translate, localizedCountLabel, type RuntimeLanguage } from "./runtime_i18n.js";
import { runtimeWindowActivityLabel, runtimeWindowShortKey } from "./runtime_console_state.js";

export function windowDateTimeLabel(timestampMs: unknown, language?: RuntimeLanguage): string {
  const value = Number(timestampMs);
  if (!Number.isFinite(value) || value <= 0) return translate("time unavailable", language);
  return new Date(value).toLocaleString(language === "zh-CN" ? "zh-CN" : "en");
}

export function windowAgeLabel(
  timestampMs: unknown,
  now: number = Date.now(),
  language?: RuntimeLanguage,
): string {
  return runtimeWindowActivityLabel(timestampMs, now, language);
}

export function runtimeProjectClientId(project: unknown): string {
  const value = String(project || "");
  const parts = value.split(":");
  return parts.length >= 3 && parts[0] === "agent" ? parts[1] : "";
}

function appendChipElement(parent: HTMLElement, text: string, extraClass = ""): HTMLElement {
  const chip = document.createElement("span");
  chip.className = "chip" + (extraClass ? " " + extraClass : "");
  chip.textContent = text;
  parent.appendChild(chip);
  return chip;
}

export function renderWindowActivityRows(
  node: HTMLElement | null,
  activities: any[],
  options: {
    compact?: boolean;
    language?: RuntimeLanguage;
    onCopyTrace?: (traceId: string) => void;
  } = {},
): void {
  if (!node) return;
  while (node.firstChild) node.removeChild(node.firstChild);
  const compact = options.compact ?? false;
  const language = options.language;
  for (const activity of activities) {
    const item = document.createElement("article");
    item.className = "window-activity-item" + (compact ? " compact" : "")
      + (activity?.recorder_gap_session_id ? " recorder-gap" : "");
    const head = document.createElement("div");
    head.className = "window-activity-head";
    const title = document.createElement("strong");
    title.textContent = String(activity?.tool_name || activity?.method || "WebPi call");
    const time = document.createElement("span");
    time.className = "muted small";
    time.textContent = windowDateTimeLabel(activity?.started_at_ms, language);
    head.appendChild(title);
    head.appendChild(time);
    item.appendChild(head);

    const facts = document.createElement("div");
    facts.className = "chips window-activity-facts";
    appendChipElement(facts, translate(String(activity?.status || "unknown"), language));
    if (activity?.project) appendChipElement(facts, String(activity.project));
    if (activity?.activity_presentation) {
      appendChipElement(facts, String(activity.activity_presentation), "tone-runtime");
    }
    if (activity?.activity_kind) appendChipElement(facts, String(activity.activity_kind));
    if (activity?.meaningful) appendChipElement(facts, translate("meaningful", language), "tone-runtime");
    if (activity?.recorder_gap_session_id) appendChipElement(facts, translate("recorder gap", language), "tone-warn");
    if (activity?.response_streaming === true) {
      appendChipElement(facts, translate("streaming timing unavailable", language), "tone-warn");
    } else if (typeof activity?.service_ms === "number") {
      appendChipElement(facts, (language === "zh-CN" ? "服务耗时 " : "service ") + String(activity.service_ms) + " ms");
    } else if (activity?.meaningful) {
      appendChipElement(facts, translate("service unavailable", language));
    }
    if (activity?.meaningful) {
      if (typeof activity?.next_call_gap_ms === "number") {
        appendChipElement(facts, (language === "zh-CN" ? "下次间隔 " : "next gap ") + String(activity.next_call_gap_ms) + " ms");
      } else {
        appendChipElement(facts, translate("next gap unavailable", language));
      }
      if (typeof activity?.cycle_ms === "number") {
        appendChipElement(facts, (language === "zh-CN" ? "周期 " : "cycle ") + String(activity.cycle_ms) + " ms");
      }
    }
    if (activity?.window_transition_kind === "overlap") {
      appendChipElement(facts, translate("overlap from previous", language), "tone-warn");
    }
    item.appendChild(facts);

    const links = Array.isArray(activity?.workflow_sessions) ? activity.workflow_sessions : [];
    if (links.length) {
      const relation = document.createElement("div");
      relation.className = "muted small";
      relation.textContent = links
        .map((link: any) => String(link.workflow_session_id || "") + " · " + String(link.relation || "linked"))
        .join(" · ");
      item.appendChild(relation);
    }
    if (activity?.recorder_gap_session_id) {
      const gap = document.createElement("div");
      gap.className = "window-gap-note small";
      gap.textContent = (language === "zh-CN" ? "记录未继续于会话 " : "Recording was not continued for ") + String(activity.recorder_gap_session_id) + ".";
      item.appendChild(gap);
    }
    if (activity?.server_trace_id) {
      const trace = document.createElement("button");
      trace.type = "button";
      trace.className = "window-trace-copy";
      trace.textContent = "trace " + String(activity.server_trace_id);
      trace.title = translate("Copy trace id", language);
      if (options.onCopyTrace) {
        trace.addEventListener("click", () => options.onCopyTrace!(String(activity.server_trace_id)));
      }
      item.appendChild(trace);
    }
    node.appendChild(item);
  }
}

export function createWindowCard(
  row: any,
  selectedWindowKey: string,
  onSelect: (key: string) => void,
  now: number = Date.now(),
  language?: RuntimeLanguage,
): HTMLElement | null {
  const key = String(row?.client_window_key || "");
  if (!key) return null;
  const button = document.createElement("button");
  button.type = "button";
  button.dataset.action = "open-window";
  button.className = "runtime-window-card" + (key === selectedWindowKey ? " selected" : "");
  if (key === selectedWindowKey) button.setAttribute("aria-current", "true");
  const head = document.createElement("div");
  head.className = "runtime-window-card-head";
  const title = document.createElement("strong");
  title.textContent = "Window " + runtimeWindowShortKey(key);
  const active = document.createElement("span");
  active.className = "chip" + (Number(row?.active_count || 0) > 0 ? " tone-runtime" : "");
  active.textContent = Number(row?.active_count || 0) > 0
    ? (language === "zh-CN" ? String(row.active_count) + " 个活跃" : String(row.active_count) + " active")
    : String(row?.source || "window");
  head.appendChild(title);
  head.appendChild(active);
  button.appendChild(head);
  if (row.last_project_name && row.last_project_name !== "—") {
    const project = document.createElement("p"); project.className = "window-project-label";
    project.textContent = String(row.last_project_name); button.appendChild(project);
  }
  const call = document.createElement("span");
  call.className = "muted small";
  call.textContent = row?.last_tool_call_at_ms
    ? (language === "zh-CN" ? "最后调用 " : "Last WebPi call ") + windowAgeLabel(row.last_tool_call_at_ms, now, language)
    : (language === "zh-CN" ? "最后活动 " : "Last WebPi activity ") + windowAgeLabel(row?.last_seen_at_ms, now, language);
  button.appendChild(call);
  const meaningful = document.createElement("span");
  meaningful.className = "muted small";
  meaningful.textContent = row?.last_meaningful_activity_at_ms
    ? (language === "zh-CN" ? "最后有效工作 " : "Last meaningful work ") + windowAgeLabel(row.last_meaningful_activity_at_ms, now, language)
    : (language === "zh-CN" ? "未记录到有效 WebPi 工作" : "No meaningful WebPi work recorded");
  button.appendChild(meaningful);
  const links = document.createElement("span");
  links.className = "muted small";
  links.textContent = localizedCountLabel(Number(row?.linked_session_count || 0), "linked Session", "linked Sessions", language)
    + (Number(row?.recorder_gap_count || 0) ? " · " + String(row.recorder_gap_count) + (language === "zh-CN" ? " 个记录断层" : " recorder gap") : "");
  button.appendChild(links);
  button.addEventListener("click", () => onSelect(key));
  return button;
}

export function renderWindowActiveRequests(
  activeNode: HTMLElement | null,
  activeRequests: any[],
  options: {
    now?: number;
    language?: RuntimeLanguage;
    onCopyTrace?: (traceId: string) => void;
  } = {},
): void {
  if (!activeNode) return;
  while (activeNode.firstChild) activeNode.removeChild(activeNode.firstChild);
  const now = options.now ?? Date.now();
  const language = options.language;
  if (!activeRequests.length) {
    const empty = document.createElement("p");
    empty.className = "muted small";
    empty.textContent = translate("No WebPi request is currently active.", language);
    activeNode.appendChild(empty);
    return;
  }
  for (const request of activeRequests) {
    const item = document.createElement("article");
    item.className = "window-request-item";
    const title = document.createElement("strong");
    title.textContent = String(request?.tool_name || request?.method || "WebPi request");
    item.appendChild(title);
    const meta = document.createElement("div");
    meta.className = "muted small";
    const facts = [
      request?.project,
      request?.started_at_ms ? (language === "zh-CN" ? "开始于 " : "started ") + windowAgeLabel(request.started_at_ms, now, language) : null,
      typeof request?.elapsed_ms === "number" ? String(request.elapsed_ms) + (language === "zh-CN" ? " 毫秒已耗时" : " ms elapsed") : null,
    ].filter(Boolean).map(String);
    meta.textContent = facts.join(" · ");
    item.appendChild(meta);
    if (request?.server_trace_id) {
      const trace = document.createElement("button");
      trace.type = "button";
      trace.className = "window-trace-copy";
      trace.textContent = "trace " + String(request.server_trace_id);
      trace.title = translate("Copy trace id", language);
      if (options.onCopyTrace) {
        trace.addEventListener("click", () => options.onCopyTrace!(String(request.server_trace_id)));
      }
      item.appendChild(trace);
    }
    activeNode.appendChild(item);
  }
}

export function renderWindowLinkedSessions(
  sessionsNode: HTMLElement | null,
  linkedSessions: any[],
  onOpenSession: (session: any) => void,
  language?: RuntimeLanguage,
): void {
  if (!sessionsNode) return;
  while (sessionsNode.firstChild) sessionsNode.removeChild(sessionsNode.firstChild);
  for (const session of linkedSessions) {
    const button = document.createElement("button");
    button.type = "button";
    button.dataset.action = "open-linked-session";
    button.className = "window-session-card";
    const title = document.createElement("strong");
    title.textContent = String(session?.title || session?.workflow_session_id || translate("Workflow Session", language));
    const meta = document.createElement("span");
    meta.className = "muted small";
    meta.textContent = [
      session?.workflow_session_id,
      session?.lifecycle,
      session?.project,
      ...(Array.isArray(session?.relations) ? session.relations : []),
    ].filter(Boolean).map(String).join(" · ");
    button.appendChild(title);
    button.appendChild(meta);
    button.addEventListener("click", () => onOpenSession(session));
    sessionsNode.appendChild(button);
  }
  if (!sessionsNode.childElementCount) {
    const empty = document.createElement("p");
    empty.className = "muted small";
    empty.textContent = translate("No authorized Workflow Session links.", language);
    sessionsNode.appendChild(empty);
  }
}

export function renderSessionWindowCorrelationLinks(
  linkedNode: HTMLElement | null,
  links: any[],
  onSelectWindow: (key: string) => void,
  now: number = Date.now(),
  language?: RuntimeLanguage,
): void {
  if (!linkedNode) return;
  while (linkedNode.firstChild) linkedNode.removeChild(linkedNode.firstChild);
  for (const link of links) {
    const key = String(link?.client_window_key || "");
    if (!key) continue;
    const button = document.createElement("button");
    button.type = "button";
    button.className = "window-session-card" + (Number(link?.recorder_gap_count || 0) ? " recorder-gap" : "");
    const title = document.createElement("strong");
    title.textContent = "Window " + runtimeWindowShortKey(key);
    const meta = document.createElement("span");
    meta.className = "muted small";
    meta.textContent = [
      link?.source,
      link?.last_seen_at_ms ? (language === "zh-CN" ? "最后活动 " : "last WebPi activity ") + windowAgeLabel(link.last_seen_at_ms, now, language) : null,
      Number(link?.recorder_gap_count || 0) ? String(link.recorder_gap_count) + (language === "zh-CN" ? " 个记录断层" : " recorder gap") : null,
    ].filter(Boolean).map(String).join(" · ");
    button.appendChild(title);
    button.appendChild(meta);
    button.addEventListener("click", () => onSelectWindow(key));
    linkedNode.appendChild(button);
  }
  if (!links.length) {
    const empty = document.createElement("p");
    empty.className = "muted small";
    empty.textContent = translate("No linked Window evidence.", language);
    linkedNode.appendChild(empty);
  }
}

export interface RuntimeWindowDetailFields {
  title: string;
  key: string;
  source: string;
  activeCount: string;
  lastCall: string;
  lastMeaningful: string;
  activeStatus: string;
  linkedStatus: string;
  activityStatus: string;
}

export function formatWindowDetailFields(
  detail: any,
  fallbackKey = "",
  now: number = Date.now(),
  language?: RuntimeLanguage,
): RuntimeWindowDetailFields | null {
  if (!detail) return null;
  const key = String(detail.client_window_key || fallbackKey || "");
  return {
    title: "Window " + runtimeWindowShortKey(key),
    key: key || "—",
    source: String(detail.source || "—"),
    activeCount: String(Number(detail.active_count || 0)),
    lastCall: detail.last_tool_call_at_ms
      ? windowAgeLabel(detail.last_tool_call_at_ms, now, language)
      : translate("No completed tools/call activity", language),
    lastMeaningful: detail.last_meaningful_activity_at_ms
      ? windowAgeLabel(detail.last_meaningful_activity_at_ms, now, language)
      : translate("No meaningful WebPi work recorded", language),
    activeStatus: translate(Number(detail.active_count || 0) ? "Active request" : "No active request", language),
    linkedStatus:
      localizedCountLabel(Number(detail.sessions_returned || 0), "Session", "Sessions", language) +
      (detail.sessions_truncated ? " · " + translate("bounded", language) : ""),
    activityStatus:
      localizedCountLabel(Number(detail.activity_returned || 0), "event", "events", language) +
      (detail.activity_truncated ? " · " + translate("bounded", language) : ""),
  };
}

export function formatWindowEmptyState(
  availability: "idle" | "loading" | "available" | "stale" | "unavailable",
  visibilityScope: "global" | "principal" = "principal",
  isProjectScoped = false,
  language?: RuntimeLanguage,
): string {
  if (availability === "unavailable") {
    return translate("Window activity requires runtime:read.", language);
  }
  if (availability === "stale") {
    return translate("Window activity could not be refreshed.", language);
  }
  if (availability === "available") {
    if (isProjectScoped) {
      if (visibilityScope === "principal") {
        return translate(
          "No Window activity is visible for this Project to this credential.",
          language,
        );
      }
      return translate("No Window activity has been observed for this Project.", language);
    }
    if (visibilityScope === "principal") {
      return translate("No Window activity is visible to this credential.", language);
    }
    return translate("No Window activity is visible.", language);
  }
  return translate("Loading Window activity…", language);
}

export function formatWindowListStatusText(
  availability: "idle" | "loading" | "available" | "stale" | "unavailable",
  count: number,
  visibilityScope: "global" | "principal" = "principal",
  language?: RuntimeLanguage,
): string {
  if (availability === "unavailable") {
    return translate("runtime:read required", language);
  }
  if (availability === "stale") {
    if (count > 0) {
      const countPart = localizedCountLabel(count, "Window", "Windows", language);
      const stalePart = translate("refresh failed, showing previous data", language);
      return countPart + " · " + stalePart;
    }
    return translate("Window activity could not be refreshed.", language);
  }
  if (count > 0) {
    return localizedCountLabel(count, "Window", "Windows", language);
  }
  return formatWindowEmptyState(availability, visibilityScope, false, language);
}

export function renderWindowCards(
  node: HTMLElement | null,
  windowRows: any[],
  selectedWindowKey: string,
  onSelect: (key: string) => void,
  now: number = Date.now(),
  language?: RuntimeLanguage,
): void {
  if (!node) return;
  while (node.firstChild) node.removeChild(node.firstChild);
  for (const row of windowRows) {
    const card = createWindowCard(row, selectedWindowKey, onSelect, now, language);
    if (card) node.appendChild(card);
  }
}

export function renderProjectWindowCards(
  node: HTMLElement | null,
  windowRows: any[],
  onSelect: (key: string) => void,
  now: number = Date.now(),
  language?: RuntimeLanguage,
): void {
  if (!node) return;
  while (node.firstChild) node.removeChild(node.firstChild);
  for (const row of windowRows) {
    const card = createWindowCard(row, "", onSelect, now, language);
    if (card) {
      card.title = translate("Open Window inspector", language);
      node.appendChild(card);
    }
  }
}
