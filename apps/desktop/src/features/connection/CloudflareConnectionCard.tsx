import { useCallback, useEffect, useRef, useState } from "react";
import { PasswordInput, TextInput } from "@mantine/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { desktopApi } from "../../lib/desktop-api";
import { normalizeDesktopError } from "../../i18n/presentation";
import type { CloudflareConnectionRequest, CloudflareConnectionStatus, CloudflareOAuthHandoff, TunnelConnection } from "../../models/connections-tools";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { useProduct } from "../../i18n/product";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

// Matches webcodex_core::authority::profiles::LOCAL_USER.
const DEFAULT_OAUTH_SCOPES = "runtime:read runner:manage session:collaborate project:read project:write job:run";
const OPTIONAL_OAUTH_SCOPES = [
  "browser:read browser:control browser:launch",
  "computer:read computer:control computer:launch computer:display_read computer:pointer_control computer:clipboard_read computer:clipboard_write",
  "mcp:local",
  "ssh:local",
];

function OAuthHandoff({ profile, status, onClose }: { profile: TunnelConnection; status: CloudflareConnectionStatus; onClose: () => void }) {
  const c = useConnectionsTools(); const p = useProduct();
  const [callback, setCallback] = useState("");
  const [scopes, setScopes] = useState(DEFAULT_OAUTH_SCOPES);
  const [handoff, setHandoff] = useState<CloudflareOAuthHandoff | null>(null);
  const [busy, setBusy] = useState(false); const [failed, setFailed] = useState(false);
  const [copied, setCopied] = useState(false);
  const alive = useRef(true); const submitting = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const issue = async (replace: boolean) => {
    if (submitting.current) return;
    submitting.current = true; setBusy(true); setFailed(false); setHandoff(null);
    try {
      const result = await desktopApi.cloudflareConnection<CloudflareOAuthHandoff>({ action: "configure_oauth", profile_id: profile.id, server_instance_id: status.server_instance_id, process_generation: status.process_generation, redirect_uri: callback.trim(), scopes: scopes.trim().split(/\s+/).filter(Boolean), replace });
      if (alive.current) setHandoff(result);
    } catch { if (alive.current) setFailed(true); }
    finally { submitting.current = false; if (alive.current) setBusy(false); }
  };
  return <WorkspaceDialog title={c("configureOauth")} onClose={onClose} busy={busy}>
    <form className="profile-editor" onSubmit={event => { event.preventDefault(); void issue(false); }}>
      <p>{c("oauthHandoffHelp")}</p>
      <TextInput label={c("oauthCallback")} aria-label={c("oauthCallback")} type="url" required value={callback} maxLength={2048} onChange={event => setCallback(event.currentTarget.value)} disabled={busy || Boolean(handoff?.client_secret)} autoFocus />
      <TextInput label={c("oauthScopes")} aria-label={c("oauthScopes")} required value={scopes} maxLength={1024} onChange={event => setScopes(event.currentTarget.value)} disabled={busy || Boolean(handoff?.client_secret)} />
      <details><summary>{c("oauthOptionalScopes")}</summary>{OPTIONAL_OAUTH_SCOPES.map(group => <p key={group}><code>{group}</code></p>)}</details>
      {failed && <p role="alert">{c("operationFailed")}</p>}
      {handoff?.client_secret ? <div className="oauth-handoff" role="region" aria-label={c("configureOauth")}>
        <TextInput label={c("clientId")} aria-label={c("clientId")} readOnly value={handoff.client_id} />
        <PasswordInput label={c("clientSecret")} aria-label={c("clientSecret")} readOnly value={handoff.client_secret} autoComplete="off" />
        <button type="button" className="secondary-button" onClick={() => { void writeText(JSON.stringify({client_id: handoff.client_id, client_secret: handoff.client_secret})).then(() => setCopied(true)).catch(() => setFailed(true)); }}>{copied ? p("copied") : c("copyCredentials")}</button>
      </div> : <>
        {(status.oauth_configured || handoff?.already_configured) && <p role="status">{c("oauthSecretLost")}</p>}
        <div className="connection-actions">
          {!status.oauth_configured && !handoff?.already_configured && <button type="submit" className="primary-button" disabled={busy || !callback.trim() || !scopes.trim()}>{c("issueOauth")}</button>}
          {(status.oauth_configured || handoff?.already_configured) && <button type="button" className="primary-button" disabled={busy || !callback.trim() || !scopes.trim()} onClick={() => void issue(true)}>{c("replaceOauth")}</button>}
        </div>
      </>}
      <button type="button" className="secondary-button" disabled={busy} onClick={onClose}>{p("close")}</button>
    </form>
  </WorkspaceDialog>;
}

export function CloudflareConnectionCard({ profile, canStart, busy, onEdit, onDelete, onRepair, onRestartServer }: { profile: TunnelConnection; canStart: boolean; busy: boolean; onEdit: () => void; onDelete: () => void; onRepair?: () => void; onRestartServer?: () => void }) {
  const c = useConnectionsTools(); const p = useProduct();
  const [status, setStatus] = useState<CloudflareConnectionStatus | null>(null);
  const [failureCode, setFailureCode] = useState<string | null>(null); const [working, setWorking] = useState(false);
  const [oauth, setOauth] = useState(false); const [copied, setCopied] = useState(false);
  const alive = useRef(true); const sequence = useRef(0); const inFlight = useRef(false);
  const current = useRef<CloudflareConnectionStatus | null>(null);
  const quick = profile.provider?.kind === "cloudflare_quick";
  const refresh = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true; const attempt = ++sequence.current;
    try {
      const next = await desktopApi.cloudflareConnection({ action: "status", profile_id: profile.id });
      if (alive.current && attempt === sequence.current) { current.current = next; setStatus(next); setFailureCode(null); }
    } catch (error) { if (alive.current && attempt === sequence.current) { current.current = null; setStatus(null); setFailureCode(normalizeDesktopError(error).code); } }
    finally { inFlight.current = false; }
  }, [profile.id, profile.revision]);
  useEffect(() => { alive.current = true; setStatus(null); setFailureCode(null); setOauth(false); setCopied(false); void refresh(); const timer = window.setInterval(() => void refresh(), 3000); return () => { alive.current = false; ++sequence.current; current.current = null; window.clearInterval(timer); }; }, [refresh]);
  const run = async (action: "start" | "stop" | "restart") => {
    const observed = current.current;
    if (!observed || inFlight.current || busy) return;
    inFlight.current = true; const attempt = ++sequence.current; setWorking(true); setFailureCode(null); setOauth(false); setCopied(false);
    current.current = null; setStatus(null);
    const stop: CloudflareConnectionRequest = { action: "stop", profile_id: profile.id, server_instance_id: observed.server_instance_id, process_generation: observed.process_generation };
    try {
      if (action === "restart") await desktopApi.cloudflareConnection(stop);
      const next = await desktopApi.cloudflareConnection(action === "stop" ? stop : { action: "start", profile_id: profile.id, server_instance_id: observed.server_instance_id, expected_revision: profile.revision });
      if (alive.current && attempt === sequence.current) { current.current = next; setStatus(next); }
    } catch (error) { if (alive.current && attempt === sequence.current) { current.current = null; setStatus(null); setFailureCode(normalizeDesktopError(error).code); } }
    finally { inFlight.current = false; if (alive.current) setWorking(false); }
  };
  const active = status?.lifecycle === "running" || status?.lifecycle === "starting" || status?.lifecycle === "disconnected";
  const failed = failureCode !== null;
  const ingressNotApplied = failureCode === "cloudflare_ingress_not_applied";
  const disabled = busy || working;
  const unselectedService = profile.host_mode === "standalone" && !profile.autostart;
  const origin = status?.public_origin;
  const identityRepair = status?.reason_code === "cloudflare_identity_repair_required" || status?.reason_code === "cloudflare_owner_repair_required";
  return <article className="connection-profile" aria-labelledby={`connection-${profile.id}`} data-tunnel-profile-id={profile.id}>
    <header className="workspace-section-heading"><div><h2 id={`connection-${profile.id}`}>{profile.name}</h2><span className="workspace-observation">{quick ? c("cloudflareQuick") : c("cloudflareNamed")} · {profile.host_mode === "embedded" ? c("serverOwned") : c("separateService")}</span></div><span role="status" className="connection-state">{status?.lifecycle === "running" ? p("running") : status?.lifecycle === "starting" ? p("starting") : status?.lifecycle === "stopped" ? p("stopped") : status?.lifecycle === "disconnected" ? c("cloudflareDisconnected") : status?.lifecycle === "error" ? c("connectionUnavailable") : c("awaitingStatus")}</span></header>
    {origin && <div className="connection-id"><span>MCP</span><code>{origin}/mcp</code><button type="button" className="text-button" onClick={() => { void writeText(`${origin}/mcp`).then(() => setCopied(true)).catch(error => setFailureCode(normalizeDesktopError(error).code)); }}>{copied ? p("copied") : c("copyMcpAddress")}</button></div>}
    {status && <p>{c("localForwardingTarget")}: <code>{status.local_target}</code></p>}
    {quick && <p className="workspace-notice">{c("quickReauthorize")}</p>}
    {unselectedService && <p className="workspace-notice">{c("cloudflareSelectService")}</p>}
    <p>{status?.oauth_configured ? c("oauthConfigured") : c("oauthNotConfigured")}</p>
    <p>{status?.observed_authorization ? c("oauthObserved") : c("oauthNotObserved")}</p>
    {profile.server_restart_required && <p role="status">{c("profileRestartHelp")}</p>}
    {ingressNotApplied && <div className="server-owned-apply shell-notice warning" role="status"><strong>{c("serverRestartRequired")}</strong><p>{c("serverRestartHelp")}</p>{onRestartServer && <button type="button" className="primary-button" disabled={disabled} onClick={onRestartServer}>{c("restartServer")}</button>}</div>}
    {failed && !ingressNotApplied && <p role="alert">{c("operationFailed")}</p>}
    {identityRepair && <p role="alert">{c("cloudflareIdentityRepair")}</p>}
    {(failed || identityRepair) && onRepair && <button type="button" className="secondary-button" disabled={disabled} onClick={onRepair}>{c("openRuntimeSetup")}</button>}
    <div className="connection-actions">
      <button type="button" className="secondary-button" disabled={disabled} onClick={onEdit}>{p("edit")}</button>
      <button type="button" className="primary-button" disabled={disabled || !status || !canStart || unselectedService || profile.server_restart_required || status.configured_revision !== profile.revision || (!quick && !profile.credential_present)} onClick={() => void run(active ? "restart" : "start")}>{active ? c("reconnect") : p("start")}</button>
      {active && <button type="button" className="secondary-button" disabled={disabled} onClick={() => void run("stop")}>{p("stop")}</button>}
      <button type="button" className="secondary-button" disabled={disabled || status?.lifecycle !== "running" || !origin} onClick={() => setOauth(true)}>{c("configureOauth")}</button>
      <button type="button" className="text-button" disabled={disabled} onClick={onDelete}>{c("delete")}</button>
    </div>
    {oauth && status && <OAuthHandoff key={`${status.server_instance_id}:${status.process_generation}:${status.public_origin}`} profile={profile} status={status} onClose={() => { setOauth(false); void refresh(); }} />}
  </article>;
}
