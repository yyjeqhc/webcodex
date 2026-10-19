import { Fragment, type ReactNode } from "react";
import { useConnectionsTools, type ConnectionsToolsKey } from "../../i18n/connections-tools";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { useShellText } from "../../i18n/runtime-shell";
import { connectionActive, type TunnelConnection, type TunnelProfileAction } from "../../models/connections-tools";

type Recovery = { title: ConnectionsToolsKey; action: ConnectionsToolsKey; settings: "network" | "runtime" | "diagnostics" };
function recoveryFor(profile: TunnelConnection): Recovery {
  // A Stop error describes the latest control action even when the earlier
  // connection failure's reason code is still retained for diagnostics.
  if (profile.last_error === "stop_failed") return { title: "stopFailed", action: "stopRecovery", settings: "diagnostics" };
  switch (profile.reason_code) {
    case "tunnel_restart_uncertain": return { title: "restartUncertain", action: "restartUncertainRecovery", settings: "diagnostics" };
    case "tunnel_auth_rejected": return { title: "connectionUnavailable", action: "credentialRecovery", settings: "diagnostics" };
    case "tunnel_capacity_exhausted":
    case "tunnel_protocol_failed": return { title: "protocolFailed", action: "diagnosticsRecovery", settings: "diagnostics" };
    case "tunnel_control_plane_unreachable": return { title: "controlPlaneFailed", action: "networkRecovery", settings: "network" };
    case "local_mcp_unavailable": return { title: "localMcpFailed", action: "localMcpRecovery", settings: "runtime" };
    case "tunnel_startup_failed": return { title: "startupFailed", action: "networkRecovery", settings: "network" };
  }
  switch (profile.last_error) {
    case "local_mcp_unavailable": return { title: "localMcpFailed", action: "localMcpRecovery", settings: "runtime" };
    case "health_stale": return { title: "healthStale", action: "diagnosticsRecovery", settings: "diagnostics" };
    case "protocol_invalid": return { title: "protocolFailed", action: "diagnosticsRecovery", settings: "diagnostics" };
    case "process_exited": return { title: "processExited", action: "diagnosticsRecovery", settings: "diagnostics" };
    case "start_failed": return { title: "daemonFailed", action: "diagnosticsRecovery", settings: "diagnostics" };
    case "startup_timeout": return { title: "startupFailed", action: "networkRecovery", settings: "network" };
    case "tunnel_unavailable": return { title: "connectionUnavailable", action: "networkRecovery", settings: "network" };
    default: return { title: "connectionUnavailable", action: "diagnosticsRecovery", settings: "diagnostics" };
  }
}

export function ConnectionCard({ profile, canStart, busy, copied, onAction, onEdit, onDelete, onCopy, onSettings }: {
  profile: TunnelConnection;
  canStart: boolean;
  busy: boolean;
  copied: boolean;
  onAction: (action: TunnelProfileAction) => void;
  onEdit: () => void;
  onDelete: () => void;
  onCopy: () => void;
  onSettings: (section: "network" | "runtime" | "diagnostics") => void;
}) {
  const p = useProduct();
  const c = useConnectionsTools();
  const s = useShellText();
  const { formatTime } = useLocale();
  const serverOwned = profile.host_mode === "embedded";
  const active = connectionActive(profile);
  // A failed child may already be reaped, but the enabled desired connection
  // still needs explicit Restart and Stop controls (Stop persists that intent).
  const canStop = !serverOwned && (active || (profile.enabled && profile.lifecycle === "error"));
  const startAction = canStop ? "restart" : "start";
  const failed = !profile.ready && Boolean(profile.last_error || profile.lifecycle === "error");
  const recovery = recoveryFor(profile);
  const healthStale = profile.last_error === "health_stale";
  const status = profile.server_restart_required
    ? c("pendingServerRestart")
    : profile.ready
      ? p("running")
      : failed
        ? c("connectionUnavailable")
        : profile.lifecycle === "starting"
          ? p("starting")
          : profile.lifecycle === "stopping"
            ? c("stopping")
            : profile.lifecycle === "running"
              ? p("starting")
              : p("stopped");
  // Readiness is asynchronous: use the observed per-profile failure, not only a
  // rejected start command. Never infer a proxy cause or silently switch routes.
  const autoProxyRecovery = profile.auto_proxy_used === true && profile.last_error === "tunnel_unavailable" && (
    !profile.reason_code || ["tunnel_control_plane_unreachable", "tunnel_unavailable"].includes(profile.reason_code)
  );
  const owner = serverOwned ? c("serverOwned") : c("separateService");
  const yesNo = (value: boolean | null) => value === null ? "—" : c(value ? "yes" : "no");
  const startup = profile.startup_timings_ms;
  const detailRows: { label: ConnectionsToolsKey; value: ReactNode; path?: boolean }[] = [
    { label: "connectionLifecycle", value: owner },
    { label: "autostart", value: profile.autostart ? c("enabled") : c("disabled") },
    { label: "profileId", value: <code>{profile.id}</code> },
    { label: "pid", value: profile.pid ?? "—" },
    { label: "processStarted", value: yesNo(profile.process_started) },
    { label: "processReady", value: yesNo(profile.process_ready) },
  ];
  if (startup) {
    detailRows.push(
      { label: "cliStartup", value: `${startup.cli_total} ms` },
      { label: "firstControlPlanePoll", value: `${startup.initial_poll} ms` },
    );
    if (startup.poll_attempts != null && startup.poll_request_ms != null) {
      detailRows.push(
        { label: "initialPollRequests", value: startup.poll_attempts },
        { label: "pollHttpTime", value: `${startup.poll_request_ms} ms` },
        { label: "pollRetryWait", value: `${Math.max(0, startup.initial_poll - startup.poll_request_ms)} ms` },
      );
    }
    detailRows.push({ label: "otherStartupWork", value: `${startup.cli_total - startup.initial_poll} ms` });
  }
  detailRows.push(
    { label: "tunnelReady", value: yesNo(profile.tunnel_ready) },
    { label: "localMcpReady", value: yesNo(profile.local_mcp_ready) },
  );
  if (profile.failure_stage) detailRows.push({ label: "failureStage", value: <code>{profile.failure_stage}</code> });
  if (profile.reason_code) detailRows.push({ label: "reasonCode", value: <code>{profile.reason_code}</code> });
  if (profile.local_mcp_url) detailRows.push({ label: "mcpAddress", value: <code>{profile.local_mcp_url}</code>, path: true });
  if (profile.runtime_directory) detailRows.push({ label: "runtimeDirectory", value: <code>{profile.runtime_directory}</code>, path: true });
  return <article className="connection-profile" aria-labelledby={`connection-${profile.id}`} data-tunnel-profile-id={profile.id}>
    <header className="workspace-section-heading"><div><h2 id={`connection-${profile.id}`}>{profile.name}</h2><span className="workspace-observation">{owner}</span></div><span className={`connection-state ${profile.ready ? "ready" : ""}`} role="status"><i className={`status-dot ${profile.ready ? "ready" : failed ? "error" : "unknown"}`} aria-hidden="true" />{status}</span></header>
    {(active || failed) && <dl className="connection-checks">{([[c("secureTunnel"), healthStale ? null : profile.tunnel_ready], [c("localMcp"), healthStale ? null : profile.local_mcp_ready]] as const).map(([label, ready]) => <div key={label}><dt>{label}</dt><dd><i className={`status-dot ${ready === true ? "ready" : ready === false ? "error" : "unknown"}`} aria-hidden="true" />{ready === true ? c("reachable") : ready === false ? c("unreachable") : healthStale ? c("awaitingStatus") : p("unknown")}</dd></div>)}</dl>}
    <div className="connection-id"><span>{c("tunnelId")}</span><code>{profile.tunnel_id || "—"}</code>{profile.tunnel_id && <button type="button" className="text-button" aria-label={`${c("copyId")} ${profile.name}`} onClick={onCopy}>{copied ? p("copied") : c("copyId")}</button>}</div>
    {profile.server_restart_required && <div className="connection-apply-note" role="status"><strong>{c("pendingServerRestart")}</strong><span>{c("profileRestartHelp")}</span></div>}
    {failed && <div className="connection-recovery" role="status"><strong>{c(recovery.title)}</strong><p>{c(recovery.action)}</p>{autoProxyRecovery && <p>{c("autoProxyHint")}</p>}
      <button type="button" className="secondary-button" onClick={() => onSettings(recovery.settings)}>{recovery.settings === "network" ? c("proxySettings") : s(recovery.settings === "runtime" ? "Runtime" : "Troubleshooting")}</button>
      <p className="connection-impact">{c("chatgptImpact")}</p>
    </div>}
    {!profile.credential_present && <p className="workspace-notice">{c("missingCredentials")}</p>}
    <div className="connection-actions">
      <button type="button" className="secondary-button" disabled={busy} aria-label={`${p("edit")} ${profile.name}`} onClick={onEdit}>{p("edit")}</button>
      {!serverOwned && <button type="button" className={active ? "secondary-button" : "primary-button"} disabled={busy || !canStart || !profile.credential_present} aria-label={`${p(startAction)} ${profile.name}`} onClick={() => onAction(startAction)}>{p(startAction)}</button>}
      {canStop && <button type="button" className="secondary-button" disabled={busy} aria-label={`${p("stop")} ${profile.name}`} onClick={() => onAction("stop")}>{p("stop")}</button>}
      <button type="button" className="text-button" disabled={busy} aria-label={`${c("delete")} ${profile.name}`} onClick={onDelete}>{c("delete")}</button>
    </div>
    <details className="connection-details"><summary aria-label={`${p("details")} ${profile.name}`}>{p("details")}</summary>
      {profile.credential_present && <p>{c("credentials")}</p>}
      <dl className="connection-meta-grid">
        {detailRows.map(({ label, value, path }) =>
          <Fragment key={label}>
            <dt>{c(label)}</dt>
            <dd className={path ? "connection-meta-path" : undefined}>{value}</dd>
          </Fragment>
        )}
      </dl>
      {profile.logs.length > 0 && <><h3>{c("logs")}</h3><ol className="connection-events">{profile.logs.map((entry, index) => <li key={`${entry.timestamp_ms}-${index}`}><time>{formatTime(entry.timestamp_ms)}</time><code>{entry.event}</code></li>)}</ol></>}
    </details>
  </article>;
}
