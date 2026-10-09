import {
  ArrowUpRight,
  Bot,
  Check,
  HardDrive,
  FolderOpen,
  Monitor,
  Play,
  Server,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, RuntimeOverview } from "../model/types.js";
import { AgentsPanel } from "../components/AgentsPanel.js";
import { CopyIdentity } from "../components/ui/CopyIdentity.js";
import { PageHeader } from "../components/ui/PageHeader.js";

const CONFIG_LABELS: Record<string, string> = {
  "shared_key_enabled": "Shared key authentication",
  "anonymous_enabled": "Anonymous access",
  "oauth2_enabled": "OAuth2 authentication",
  "oauth2_shared_key_bridge_enabled": "OAuth2 shared key bridge",
  "profile": "MCP host profile",
  "host_budget_secs": "Host request budget",
  "initial_job_handoff_secs": "Legacy handoff hint (unused)",
  "max_sync_wait_secs": "Maximum synchronous wait",
  "continuation_wait_secs": "Continuation wait"
};

type RuntimeMode = "overview" | "agents";

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
  onRefresh?: () => void;
  updatedAt?: number | null;
  refreshing?: boolean;
  onOpenWork: () => void;
  target?: { mode: "agents"; agentId: string } | null;
  onTargetConsumed?: () => void;
  onUnauthorized: () => void;
};

export function RuntimeView({
  client,
  language,
  overview,
  overviewAvailability,
  onRefresh,
  updatedAt,
  refreshing,
  onOpenWork,
  target,
  onTargetConsumed,
  onUnauthorized,
}: Props) {
  const t = (value: string) => translate(value, language);
  const [mode, setMode] = useState<RuntimeMode>("overview");
  const [requestedAgentId, setRequestedAgentId] = useState("");
  useEffect(() => {
    if (!target) return;
    setRequestedAgentId(target.agentId);
    setMode("agents");
    onTargetConsumed?.();
  }, [onTargetConsumed, target]);
  const overviewStatus = overviewAvailability === "available"
    ? { className: "good", label: "connected" }
    : overviewAvailability === "stale"
      ? { className: "warn", label: "stale" }
      : overviewAvailability === "loading" || overviewAvailability === "idle"
        ? { className: "", label: "Loading…" }
        : { className: "warn", label: "Runtime overview unavailable" };

  return (
    <main className="page runtime-page ui-workbench-surface">
      <PageHeader title={t("Runtime")} context={t("Monitor connected machines, current work, and server settings.")} className="runtime-heading" actions={<span className="quiet-pill">
          <span className={"status-dot " + overviewStatus.className} />
          {t(overviewStatus.label)}
        </span>} />

      <div className="runtime-sync-status">
        <span>{updatedAt ? t("Last synced") + " · " + new Date(updatedAt).toLocaleTimeString() : t(overviewAvailability === "loading" || overviewAvailability === "idle" ? "Loading…" : "Not synced yet")}</span>
        <button type="button" className="text-button" onClick={onRefresh} disabled={refreshing}>{t(refreshing ? "Refreshing…" : "Refresh")}</button>
        {overviewAvailability === "stale" && <span role="status">{t("Refresh failed · showing previous data")}</span>}
      </div>

      <div className="runtime-tabs" role="tablist">
        <button className={mode === "overview" ? "active" : ""} role="tab" aria-selected={mode === "overview"} onClick={() => setMode("overview")}>
          <Server size={15} /> {t("Overview")}
        </button>
        <button className={mode === "agents" ? "active" : ""} role="tab" aria-selected={mode === "agents"} onClick={() => setMode("agents")}>
          <Bot size={15} /> {t("Agents")}
        </button>
      </div>

      {mode === "overview" ? (
        <>
          <div className="runtime-metrics">
            <div>
              <span><Server size={17} /> {t("Runners")}</span>
              <strong>{overview?.runner_count ?? "—"}</strong>
              <small>{t("Machines visible with your current access")}</small>
              {overview && <span className="runtime-metric-detail">{overview.runners_online} {t("online")} · {overview.runners_stale} {t("stale")} · {overview.runners_unavailable} {t("offline")}</span>}
            </div>
            <div>
              <span><Play size={17} /> {t("Active jobs")}</span>
              <strong>{overview?.active_jobs ?? "—"}</strong>
              <small>{t("Jobs currently tracked by the Runtime")}</small>
              <span className="runtime-metric-detail">{overview?.detail_level === "primary" ? t("Session summary is loading…") : overview ? String(overview.workflow_sessions.running) + " " + t("running Sessions") + (overview.workflow_sessions.truncated ? " · " + t("Partial inventory") : "") : "—"}</span>
            </div>
            <div>
              <span><Monitor size={17} /> {t("Active Windows")}</span>
              <strong>{overview?.active_windows ?? "—"}</strong>
              <small>{t("Windows with work in progress")}</small>
              <button className="text-button" type="button" onClick={onOpenWork}>{t("View activity")} <ArrowUpRight size={13} /></button>
            </div>
            <div>
              <span><FolderOpen size={17} /> {t("Projects")}</span>
              <strong>{overview?.projects_available ? overview.visible_projects : "—"}</strong>
              <small>{t("Workspaces visible with your current access")}</small>
              {overview && <span className="runtime-metric-detail">{t(!overview.projects_available ? "Project inventory unavailable" : overview.projects_truncated ? "Partial inventory" : "Inventory loaded")}</span>}
            </div>
          </div>

          <section className="runtime-section">
            <div className="section-heading">
              <div><h2>{t("Runner fleet")}</h2><p>{t("Execution machines and their reported workload. Unavailable machines appear first.")}</p></div>
            </div>
            {overview?.runners.slice().sort((a, b) => Number(a.connected) - Number(b.connected) || Number(a.protocol_compatibility === "compatible") - Number(b.protocol_compatibility === "compatible") || a.client_id.localeCompare(b.client_id)).map((runner) => (
              <div className="runtime-row" key={runner.client_id}>
                <span className="runner-icon"><Monitor size={17} /></span>
                <span className="runtime-row-primary">
                  <CopyIdentity value={runner.client_id} label={t("Runner")} language={language} />
                  <small>
                    {t(runner.status === "stale" ? "stale" : runner.connected ? "Runner online" : "Runner unavailable")}
                    {runner.version ? " · " + runner.version : ""}
                    {runner.computer_session_availability !== undefined && (
                      " · " + t(overviewAvailability === "available" && runner.connected && runner.status !== "stale" && runner.computer_session_availability
                        ? "GUI session available" : "GUI session unavailable")
                    )}
                  </small>
                </span>
                <span className="runtime-row-meta runtime-row-jobs">
                  <span>{runner.jobs_running} {t("jobs running")} · {runner.jobs_queued} {t("Queued jobs")}</span>
                  <span>{t("Concurrency limit")}: {runner.job_concurrency_limit ?? "—"} · {runner.projects_scanned} {t("projects")}{runner.projects_scan_partial ? " · " + t("Partial inventory") : ""}</span>
                </span>
                <span className={"status-pill runtime-row-compat " + (runner.protocol_compatibility === "compatible" ? "good" : "warn")} title={t("Protocol compatibility")}>
                  {runner.protocol_compatibility === "compatible" ? <Check size={12} /> : <HardDrive size={12} />}
                  {t(runner.protocol_compatibility || "unknown")}
                </span>
              </div>
            ))}
            {!overview?.runners.length && <div className="empty-inline">{t(overviewAvailability === "loading" || overviewAvailability === "idle" ? "Loading…" : overview ? "No authorized Runners yet" : "Runtime overview unavailable")}</div>}
          </section>

          {overview && <section className="runtime-section">
            <div className="section-heading"><h2>{t("Server configuration")}</h2></div>
            <p>{t("Effective server parameters. Credentials are never displayed.")}</p>
            <div className="runtime-server-version"><span>{t("Version")}</span><code>{overview.version || "—"}</code></div>
            {overview.effective_config ? <div className="runtime-config-grid">
              <section className="runtime-config-group">
                <h3>{t("Authentication")}</h3>
                <p>{t("Sign-in methods accepted by this server.")}</p>
                <dl>
                  {Object.entries(overview.effective_config.auth).map(([name, enabled]) => <div key={name}>
                    <dt title={"auth." + name}>{t(CONFIG_LABELS[name] || name)}</dt>
                    <dd><span className="quiet-pill">{t(enabled ? "Enabled" : "Disabled")}</span></dd>
                  </div>)}
                </dl>
              </section>
              <section className="runtime-config-group">
                <h3>{t("Requests and waits")}</h3>
                <p>{t("Host profile and configured waits for tool requests.")}</p>
                <dl>
                  {Object.entries(overview.effective_config.mcp_host).map(([name, value]) => <div key={name}>
                    <dt title={"mcp_host." + name}>{t(CONFIG_LABELS[name] || name)}</dt>
                    <dd>{value}{name.endsWith("_secs") ? " " + t("seconds") : ""}</dd>
                  </div>)}
                  <div><dt title="tool_request_trace_mode">{t("Request tracing")}</dt><dd>{overview.effective_config.tool_request_trace_mode}</dd></div>
                </dl>
              </section>
            </div> : <p>{t("Configuration is unavailable from this server version.")}</p>}
          </section>}

          {overview && <details className="runtime-section runtime-diagnostics">
            <summary>{t("Build diagnostics")}</summary>
            <p>{t("Build revisions are diagnostic identity, not compatibility gates.")}</p>
            <div className="runtime-diagnostic-row"><strong>{t("Current Runtime")}</strong><code>{overview.build_git_commit || "—"}</code></div>
            {overview.runners.map((runner) => <div className="runtime-diagnostic-row" key={runner.client_id}>
              <strong>{runner.client_id}</strong>
              <span className="runtime-row-build">{t("Build alignment")}: {buildAlignmentLabel(runner.build_alignment || runner.source_alignment, t)}</span>
              <code>{runner.build_git_commit || "—"}</code>
            </div>)}
          </details>}
        </>
      ) : (
        <AgentsPanel client={client} language={language} onUnauthorized={onUnauthorized} selectedAgentId={requestedAgentId} onSelectedAgentConsumed={() => setRequestedAgentId("")} />
      )}
    </main>
  );
}
