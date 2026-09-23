import { useConnectionsTools } from "../../i18n/connections-tools";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { connectionActive, type TunnelConnection, type TunnelProfileAction } from "../../models/connections-tools";

export function ConnectionCard({ profile, canStart, busy, copied, onAction, onEdit, onDelete, onCopy }: {
  profile: TunnelConnection; canStart: boolean; busy: boolean; copied: boolean;
  onAction: (action: TunnelProfileAction) => void; onEdit: () => void; onDelete: () => void; onCopy: () => void;
}) {
  const p = useProduct(); const c = useConnectionsTools(); const { formatTime } = useLocale();
  const active = connectionActive(profile);
  // A failed child may already be reaped, but the enabled desired connection
  // still needs explicit Restart and Stop controls (Stop persists that intent).
  const canStop = active || (profile.enabled && profile.lifecycle === "error");
  const startAction = canStop ? "restart" : "start";
  const status = profile.ready ? p("running") : profile.last_error || profile.lifecycle === "error" ? p("failed") : profile.lifecycle === "starting" ? p("starting") : profile.lifecycle === "stopping" ? c("stopping") : profile.lifecycle === "running" ? p("starting") : p("stopped");
  return <article className="connection-profile" aria-labelledby={`connection-${profile.id}`} data-tunnel-profile-id={profile.id}>
    <header className="workspace-section-heading"><div><h2 id={`connection-${profile.id}`}>{profile.name}</h2><span className="workspace-observation">{c("secureTunnel")}</span></div><span className={`connection-state ${profile.ready ? "ready" : ""}`} role="status"><i className={`status-dot ${profile.ready ? "ready" : profile.last_error ? "error" : "unknown"}`} aria-hidden="true" />{status}</span></header>
    <div className="connection-id"><span>Tunnel ID</span><code>{profile.tunnel_id || "—"}</code>{profile.tunnel_id && <button type="button" className="text-button" aria-label={`${c("copyId")} ${profile.name}`} onClick={onCopy}>{copied ? p("copied") : c("copyId")}</button>}</div>
    {profile.last_error && <p className="workspace-notice" role="status">{c(profile.last_error === "local_mcp_unavailable" ? "localUnavailable" : "tunnelUnavailable")}</p>}
    <div className="connection-actions">
      <button type="button" className="secondary-button" disabled={busy} aria-label={`${p("edit")} ${profile.name}`} onClick={onEdit}>{p("edit")}</button>
      <button type="button" className={active ? "secondary-button" : "primary-button"} disabled={busy || !canStart || !profile.credential_present} aria-label={`${p(startAction)} ${profile.name}`} onClick={() => onAction(startAction)}>{p(startAction)}</button>
      {canStop && <button type="button" className="secondary-button" disabled={busy} aria-label={`${p("stop")} ${profile.name}`} onClick={() => onAction("stop")}>{p("stop")}</button>}
      <button type="button" className="text-button" disabled={busy} aria-label={`${c("delete")} ${profile.name}`} onClick={onDelete}>{c("delete")}</button>
    </div>
    <details className="connection-details"><summary>{p("advanced")} · {profile.name}</summary>
      {profile.credential_present && <p>{c("credentials")}</p>}
      <dl><dt>Profile ID</dt><dd><code>{profile.id}</code></dd><dt>PID</dt><dd>{profile.pid ?? "—"}</dd><dt>Process started</dt><dd>{String(profile.process_started)}</dd><dt>Process ready</dt><dd>{String(profile.process_ready)}</dd><dt>Tunnel ready</dt><dd>{profile.tunnel_ready === null ? "—" : String(profile.tunnel_ready)}</dd><dt>Local MCP ready</dt><dd>{profile.local_mcp_ready === null ? "—" : String(profile.local_mcp_ready)}</dd>{profile.failure_stage && <><dt>Failure stage</dt><dd><code>{profile.failure_stage}</code></dd></>}{profile.reason_code && <><dt>Reason code</dt><dd><code>{profile.reason_code}</code></dd></>}{profile.local_mcp_url && <><dt>MCP</dt><dd><code>{profile.local_mcp_url}</code></dd></>}{profile.runtime_directory && <><dt>Runtime</dt><dd><code>{profile.runtime_directory}</code></dd></>}{profile.health_url && <><dt>Health</dt><dd><code>{profile.health_url}</code></dd></>}{profile.log_file && <><dt>Log</dt><dd><code>{profile.log_file}</code></dd></>}</dl>
      {profile.logs.length > 0 && <><h3>{c("logs")}</h3><ol className="connection-events">{profile.logs.map((entry, index) => <li key={`${entry.timestamp_ms}-${index}`}><time>{formatTime(entry.timestamp_ms)}</time><code>{entry.event}</code></li>)}</ol></>}
    </details>
  </article>;
}
