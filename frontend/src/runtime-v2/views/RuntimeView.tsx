import {
  Activity,
  ArrowUpRight,
  Bot,
  Check,
  HardDrive,
  Monitor,
  Play,
  Server,
  TerminalSquare,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { absoluteTime, relativeTime, shortId } from "../model/format.js";
import type { Availability, ProjectRow, RuntimeOverview } from "../model/types.js";
import { useAgentInventory } from "../state/useAgentInventory.js";
import { WindowActivityFeed } from "../components/WindowActivityFeed.js";
import { AgentsPanel } from "../components/AgentsPanel.js";
import { PageHeader } from "../components/ui/PageHeader.js";
import { useWindowWorkspace } from "../state/useWindowWorkspace.js";

type RuntimeMode = "overview" | "windows" | "agents";

function buildAlignmentLabel(value: string | undefined, t: (source: string) => string): string {
  switch (value) {
    case "exact":
    case "aligned":
      return t("Build matches");
    case "different_version":
      return t("Different version");
    case "different_commit":
    case "different":
      return t("Different revision");
    case "dirty":
      return t("Local development build");
    default:
      return t("unknown");
  }
}

type Props = {
  client: RuntimeV2Client;
  language: RuntimeLanguage;
  overview: RuntimeOverview | null;
  overviewAvailability: Availability;
  projects: ProjectRow[];
  target?: { mode: "windows"; windowKey: string } | { mode: "agents"; agentId: string } | null;
  onTargetConsumed?: () => void;
  onUnauthorized: () => void;
};

export function RuntimeView({
  client,
  language,
  overview,
  overviewAvailability,
  projects,
  target,
  onTargetConsumed,
  onUnauthorized,
}: Props) {
  const t = (value: string) => translate(value, language);
  const [mode, setMode] = useState<RuntimeMode>("overview");
  const [requestedWindowKey, setRequestedWindowKey] = useState("");
  const [requestedAgentId, setRequestedAgentId] = useState("");
  const windows = useWindowWorkspace(client, true, onUnauthorized, {
    refreshMs: mode === "windows" ? 3_000 : 30_000,
    loadDetail: mode === "windows",
  });
  const agents = useAgentInventory(client, mode === "overview");
  const latestWindowActivityAt = windows.detail
    ? (windows.detail.last_meaningful_activity_at_ms || windows.detail.last_tool_call_at_ms || windows.detail.last_seen_at_ms)
    : undefined;
  useEffect(() => {
    if (!target) return;
    if (target.mode === "windows") {
      setRequestedWindowKey(target.windowKey);
      setMode("windows");
    } else {
      setRequestedAgentId(target.agentId);
      setMode("agents");
    }
    onTargetConsumed?.();
  }, [onTargetConsumed, target]);

  useEffect(() => {
    if (!requestedWindowKey || mode !== "windows") return;
    if (windows.windows.some((window) => window.client_window_key === requestedWindowKey)) {
      windows.select(requestedWindowKey);
      setRequestedWindowKey("");
    }
  }, [mode, requestedWindowKey, windows.windows]);
  const overviewStatus = overviewAvailability === "available"
    ? { className: "good", label: "connected" }
    : overviewAvailability === "stale"
      ? { className: "warn", label: "stale" }
      : overviewAvailability === "loading" || overviewAvailability === "idle"
        ? { className: "", label: "Loading…" }
        : { className: "warn", label: "Runtime overview unavailable" };

  const projectFor = (projectId: string | undefined) =>
    projectId ? projects.find((project) => project.id === projectId) : undefined;

  return (
    <main className="page runtime-page ui-workbench-surface">
      <PageHeader title={t("Runtime")} className="runtime-heading" actions={<span className="quiet-pill">
          <span className={"status-dot " + overviewStatus.className} />
          {t(overviewStatus.label)}
        </span>} />

      <div className="runtime-tabs" role="tablist">
        <button className={mode === "overview" ? "active" : ""} role="tab" aria-selected={mode === "overview"} onClick={() => setMode("overview")}>
          <Server size={15} /> {t("Overview")}
        </button>
        <button className={mode === "windows" ? "active" : ""} role="tab" aria-selected={mode === "windows"} onClick={() => setMode("windows")}>
          <Monitor size={15} /> {t("Window Activity")} <span>{windows.total || windows.windows.length}</span>
        </button>
        <button className={mode === "agents" ? "active" : ""} role="tab" aria-selected={mode === "agents"} onClick={() => setMode("agents")}>
          <Bot size={15} /> {t("Agents")} {agents.count !== null && <span>{agents.count}</span>}
        </button>
      </div>

      {mode === "overview" ? (
        <>
          <div className="runtime-metrics">
            <div>
              <span><Server size={17} /> {t("Runners")}</span>
              <strong>{overview?.runner_count ?? "—"}</strong>
              <small>{overview ? String(overview.runners_online) + " " + t("online") : t("Loading…")}</small>
            </div>
            <div>
              <span><Play size={17} /> {t("Active jobs")}</span>
              <strong>{overview?.active_jobs ?? "—"}</strong>
              <small>{overview ? String(overview.workflow_sessions.running) + " " + t("running Sessions") : "—"}</small>
            </div>
            <div>
              <span><Monitor size={17} /> {t("Observed windows")}</span>
              <strong>{windows.availability === "denied" ? "—" : windows.total || windows.windows.length}</strong>
              <small>{t("many-to-many Session evidence")}</small>
            </div>
            <div>
              <span><Bot size={17} /> {t("Durable agents")}</span>
              <strong>{agents.available === false ? "—" : agents.count ?? "…"}</strong>
              <small>{agents.available === false ? t("communication:read required") : t("runtime inventory")}</small>
            </div>
          </div>

          <section className="runtime-section">
            <div className="section-heading">
              <div><h2>{t("Runner fleet")}</h2></div>
            </div>
            {overview?.runners.map((runner) => (
              <div className="runtime-row" key={runner.client_id}>
                <span className="runner-icon"><Monitor size={17} /></span>
                <span className="runtime-row-primary">
                  <strong>{runner.client_id}</strong>
                  <small>
                    {runner.connected ? t("Runner online") : t("Runner unavailable")}
                    {runner.version ? " · " + runner.version : ""}
                  </small>
                </span>
                <span className="runtime-row-meta runtime-row-jobs">
                  {runner.jobs_running} {t("jobs running")} · {runner.projects_scanned} {t("projects")}
                </span>
                <span className={"status-pill runtime-row-compat " + (runner.protocol_compatibility === "compatible" ? "good" : "warn")} title={t("Protocol compatibility")}>
                  {runner.protocol_compatibility === "compatible" ? <Check size={12} /> : <HardDrive size={12} />}
                  {t(runner.protocol_compatibility || "unknown")}
                </span>
                <span className="runtime-row-meta runtime-row-build" title={`${t("Build revisions are diagnostic identity, not compatibility gates.")}${runner.build_git_commit ? ` · ${runner.build_git_commit}` : ""}`}>
                  {t("Build alignment")}: {buildAlignmentLabel(runner.build_alignment || runner.source_alignment, t)}
                  {runner.build_git_commit ? ` · ${shortId(runner.build_git_commit, 8, 4)}` : ""}
                </span>
              </div>
            ))}
            {!overview?.runners.length && <div className="empty-inline">{t("Runtime overview unavailable")}</div>}
          </section>

          <section className="runtime-section">
            <div className="section-heading">
              <div><h2>{t("Meaningful runtime status")}</h2></div>
              <button className="text-button" type="button" onClick={() => setMode("windows")}>
                {t("Open Window activity")} <ArrowUpRight size={13} />
              </button>
            </div>
            <div className="event-log">
              <div>
                <Activity size={15} />
                <span>
                  <strong>{t("Workflow Sessions")}</strong>
                  <small>{overview ? String(overview.workflow_sessions.active) + " " + t("active") : "—"}</small>
                </span>
                <time>{overview?.recent_sessions.sessions[0] ? relativeTime(overview.recent_sessions.sessions[0].updated_at) : "—"}</time>
              </div>
              <div>
                <TerminalSquare size={15} />
                <span><strong>{t("Active jobs")}</strong><small>{overview ? String(overview.active_jobs) : "—"}</small></span>
                <time>{overview?.mixed_builds_present ? t("mixed builds") : t("builds observed")}</time>
              </div>
              <div>
                <HardDrive size={15} />
                <span>
                  <strong>{t("Source alignment")}</strong>
                  <small>{overview ? String(overview.source_mismatched_runners) + " " + t("mismatched runners") : "—"}</small>
                </span>
                <time>{overview?.build_git_commit ? shortId(overview.build_git_commit) : "—"}</time>
              </div>
            </div>
          </section>
        </>
      ) : mode === "agents" ? (
        <AgentsPanel client={client} language={language} onUnauthorized={onUnauthorized} selectedAgentId={requestedAgentId} onSelectedAgentConsumed={() => setRequestedAgentId("")} />
      ) : (
        <div className="windows-workbench" data-testid="window-workbench">
          <aside className="window-list">
            <div className="window-list-head">
              <div>
                <strong>{t("Observed Windows")}</strong>
                <small>{t("Observation evidence; Windows do not own Sessions.")}</small>
              </div>
              <span className="count-badge">{windows.windows.length}</span>
            </div>
            {(windows.availability === "available" || windows.availability === "stale") && (
              <details className={"window-scope-note " + windows.scope} data-testid="window-scope-note">
                <summary>{t("Observation scope")}</summary>
                {windows.scope === "global"
                  ? t("Global Runtime scope. Only observed WebCodex requests appear here; no Project selection is required.")
                  : t("This credential sees only its observation principal's Windows within currently authorized Projects. Global Window observation requires an administrator Runtime credential.")}
              </details>
            )}
            {(windows.availability === "available" || windows.availability === "stale") && windows.truncated && (
              <div className="inventory-note">{t("Window inventory is bounded; not all observed Windows are loaded.")}</div>
            )}
            {windows.windows.slice().sort((a, b) => Number(b.active_count > 0) - Number(a.active_count > 0) || (b.last_meaningful_activity_at_ms || b.last_seen_at_ms) - (a.last_meaningful_activity_at_ms || a.last_seen_at_ms)).map((window) => {
              const project = projectFor(window.last_project);
              return (
                <button
                  type="button"
                  className={"window-row" + (windows.selectedKey === window.client_window_key ? " selected" : "")}
                  onClick={() => windows.select(window.client_window_key)}
                  key={window.client_window_key}
                  data-testid={"window-row-" + window.client_window_key}
                >
                  <span className="window-icon"><Monitor size={16} /></span>
                  <span className="window-row-main">
                    <strong title={window.client_window_key}>Window {shortId(window.client_window_key)}</strong>
                    <small>{window.source} · {project?.client_id || t("Runner not observed")}</small>
                    <small title={window.last_project}>{t("Last Project")}: {project?.name || window.last_project || t("No current Project evidence")}</small>
                    <small>{window.active_count} {t("active requests")}</small>
                  </span>
                  <time title={absoluteTime(window.last_meaningful_activity_at_ms || window.last_seen_at_ms)}>{relativeTime(window.last_meaningful_activity_at_ms || window.last_seen_at_ms)}</time>
                </button>
              );
            })}
            {windows.availability === "stale" && <div className="inventory-note" role="status">{t("Window activity refresh failed; showing previous observations.")}</div>}
            {windows.availability === "error" && <div className="empty-inline" role="status">{t("Window activity unavailable")}</div>}
            {windows.availability === "loading" && <div className="empty-inline">{t("Loading Window activity…")}</div>}
            {windows.availability === "denied" && <div className="empty-inline">{t("Window activity unavailable")}</div>}
            {windows.availability === "available" && !windows.windows.length && <div className="empty-inline">{t("No Window activity observed yet.")}</div>}
          </aside>

          <section className="window-detail">
            {windows.detailAvailability === "stale" && <div className="inventory-note" role="status">{t("Window activity refresh failed; showing previous observations.")}</div>}
            {windows.detailAvailability === "error" && <div className="inventory-note" role="status">{t("Window activity unavailable")}</div>}
            {windows.detail ? (
              <>
                <header className="window-detail-head">
                  <div>
                    <span className="eyebrow">{t("Window evidence")}</span>
                    <h2>Window {shortId(windows.detail.client_window_key)}</h2>
                    <p>{windows.detail.source} · {t("last observed")} {relativeTime(windows.detail.last_seen_at_ms)}</p>
                  </div>
                  <span className="quiet-pill">{windows.detail.active_count} {t("active requests")}</span>
                </header>

                <section className="window-activity-semantics" aria-label={t("Activity signals")}>
                  <div data-testid="window-signal-window">
                    <span className="activity-source-badge window">{t("Window")}</span>
                    <strong>{windows.detail.active_count > 0 ? t("Active") : latestWindowActivityAt ? t("Last WebCodex call") : t("Not observed")}</strong>
                    <small>{latestWindowActivityAt ? absoluteTime(latestWindowActivityAt) : t("No Window-scoped WebCodex activity is loaded.")}</small>
                  </div>
                </section>

                <WindowActivityFeed key={windows.detail.client_window_key} detail={windows.detail} projects={projects} language={language} />
              </>
            ) : (
              <div className="empty-work">
                <Monitor size={22} />
                <h2>{t("Select an observed Window")}</h2>
                <p>{windows.detailAvailability === "denied" ? t("This Window is no longer visible to the current credential. Refresh to check available activity.") : t("Choose a window to see its tool calls.")}</p>
              </div>
            )}
          </section>
        </div>
      )}
    </main>
  );
}
