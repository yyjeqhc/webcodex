import { useEffect, useRef, useState } from "react";
import { PasswordInput, TextInput } from "@mantine/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { desktopApi } from "../../lib/desktop-api";
import { verifiedPublicOrigin } from "./gemini-status";
import { safeIdentifier } from "../../lib/desktop-errors";
import { useCloudflareControl } from "./useCloudflareControl";
import type { CloudflareConnectionStatus, CloudflareOAuthHandoff, TunnelConnection } from "../../models/connections-tools";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { cloudflarePreparationMessages } from "../../i18n/cloudflare";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { DesktopErrorAlert } from "../../components/DesktopErrorAlert";

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

interface CardProps {
  profile: TunnelConnection;
  canStart: boolean;
  busy: boolean;
  onEdit: () => void;
  onDelete: () => void;
  onRepair?: () => void;
  onRestartServer?: () => void;
  onEnableRegistration?: () => void;
}

export function CloudflareConnectionCard(props: CardProps) {
  const {profile, canStart} = props;
  return <CloudflareProfile key={JSON.stringify([profile.id, profile.configuration_id, profile.revision, canStart])} {...props} />;
}

function CloudflareProfile({ profile, canStart, busy, onEdit, onDelete, onRepair, onRestartServer, onEnableRegistration }: CardProps) {
  const c = useConnectionsTools(); const p = useProduct();
  const { t } = useLocale();
  const { status, failure, failureCode, actionPending, run, refresh } = useCloudflareControl(profile, canStart);
  const [oauth, setOauth] = useState<CloudflareConnectionStatus | null>(null);
  const [handoffFeedback, setHandoffFeedback] = useState<{origin: string; generation: number; instance: string; kind: "copied" | "clipboardRecovery" | "openGeminiFailed"} | null>(null);
  const alive = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const quick = profile.provider?.kind === "cloudflare_quick";
  const active = status?.lifecycle === "running" || status?.lifecycle === "starting" || status?.lifecycle === "disconnected";
  const failed = failureCode !== null;
  const ingressNotApplied = failureCode === "cloudflare_ingress_not_applied" || status?.reason_code === "cloudflare_restart_server_required";
  const disabled = busy || actionPending !== null;
  const unselectedService = profile.host_mode === "standalone" && !profile.autostart;
  const dcr = Boolean(profile.oauth_dynamic_registration_enabled);
  const matchesProfile = canStart && !ingressNotApplied && !profile.server_restart_required && status?.configured_revision === profile.revision;
  const running = matchesProfile && status?.lifecycle === "running";
  const observedOrigin = status?.public_origin;
  const origin = running ? verifiedPublicOrigin(profile, status, dcr) : null;
  const feedback = handoffFeedback?.origin === origin && handoffFeedback?.generation === status?.process_generation &&
    handoffFeedback?.instance === status?.server_instance_id ? handoffFeedback?.kind : null;
  const handoff = async (action: "copy" | "open") => {
    if (!origin || !status) return;
    let kind: "copied" | "clipboardRecovery" | "openGeminiFailed";
    try {
      const now = await desktopApi.cloudflareConnection({action: "status", profile_id: profile.id});
      if (!alive.current || now.profile_id !== profile.id || now.configuration_id !== profile.configuration_id || now.lifecycle !== "running" ||
          now.server_instance_id !== status.server_instance_id || now.process_generation !== status.process_generation ||
          verifiedPublicOrigin(profile, now, dcr) !== origin) {
        if (alive.current) void refresh();
        return;
      }
      if (action === "copy") { await writeText(origin + "/mcp"); kind = "copied"; }
      else { await desktopApi.openDiagnosticResource("gemini_apps"); return; }
    } catch { kind = action === "copy" ? "clipboardRecovery" : "openGeminiFailed"; }
    if (alive.current) setHandoffFeedback({origin, instance: status.server_instance_id, generation: status.process_generation, kind});
  };
  const reason = status?.reason_code ?? failureCode;
  const safeReason = safeIdentifier(reason);
  const identityRepair = reason === "cloudflare_identity_repair_required" || reason === "cloudflare_owner_repair_required" || reason === "cloudflare_runner_unavailable";
  const routeMismatch = reason === "cloudflare_forwarding_mismatch" || reason === "cloudflare_forwarding_timeout";
  const discoveryRetry = reason === "cloudflare_discovery_unavailable";
  const verifying = reason === "cloudflare_discovery_pending";
  const registrationUnavailable = dcr && running && !verifying && !status.registration_available;
  const manualButton = <button type="button" className="secondary-button" disabled={disabled || !running || !observedOrigin} onClick={() => setOauth(status)}>{c("configureOauth")}</button>;
  const address = origin && <div className="connection-id gemini-mcp-address" role="status">
      <strong>{dcr ? c("publicUrlReadyLabel") : "MCP"}</strong>
      <code>{origin}/mcp</code>
      <button type="button" className="secondary-button" onClick={() => void handoff("copy")}>{feedback === "copied" ? p("copied") : c("copyMcpAddress")}</button>
      {dcr && <button type="button" className="primary-button" onClick={() => void handoff("open")}>{c("openGemini")}</button>}
    </div>;
  const advanced = dcr || Boolean(onEnableRegistration);
  const awaitingGeminiUrl = Boolean(onEnableRegistration || dcr) && !origin;
  const missingUrlReason = reason && cloudflarePreparationMessages[reason] ? t(cloudflarePreparationMessages[reason])
    : reason === "cloudflare_revision_conflict" ? t("gemini.applyConnection")
    : onEnableRegistration && !dcr ? c("publicUrlRegistration")
    : !canStart ? c("publicUrlServer")
    : ingressNotApplied || profile.server_restart_required ? c("serverRestartHelp")
    : routeMismatch ? t("gemini.forwardFailure")
    : reason === "cloudflare_runner_unavailable" ? t("gemini.runnerOffline")
    : identityRepair ? c("cloudflareIdentityRepair")
    : reason === "cloudflare_binary_unavailable" ? c("installCloudflared")
    : reason === "cloudflare_discovery_unavailable" || verifying || status?.lifecycle === "starting" || actionPending ? c("publicUrlPending")
    : registrationUnavailable ? c("registrationUnavailable")
    : status?.lifecycle === "disconnected" ? c("publicUrlDisconnected")
    : status?.lifecycle === "stopped" ? c("publicUrlStopped")
    : c("publicUrlWaiting");
  const stateText = actionPending === "stop" ? c("stopping")
    : actionPending || verifying || status?.lifecycle === "starting" ? p("starting")
    : profile.server_restart_required || ingressNotApplied ? c("pendingServerRestart")
    : !canStart || failed || status?.lifecycle === "error" || registrationUnavailable || (status && !matchesProfile) ? c("connectionUnavailable")
    : origin && dcr ? c("endpointReady")
    : onEnableRegistration && !dcr && running ? c("registrationOff")
    : status?.lifecycle === "running" ? p("running")
    : status?.lifecycle === "disconnected" ? c("cloudflareDisconnected")
    : status?.lifecycle === "stopped" ? p("stopped") : c("awaitingStatus");
  return <article className="connection-profile" aria-labelledby={`connection-${profile.id}`} data-tunnel-profile-id={profile.id}>
    <header className="workspace-section-heading"><div><h2 id={`connection-${profile.id}`}>{profile.name}</h2><span className="workspace-observation">{quick ? c("cloudflareQuick") : c("cloudflareNamed")}</span></div><span role="status" className="connection-state">{stateText}</span></header>
    {onEnableRegistration && !dcr && <div><button type="button" className="primary-button" disabled={disabled} onClick={onEnableRegistration}>{c("connectGemini")}</button></div>}
    {onEnableRegistration && dcr && !active && !ingressNotApplied && !identityRepair && <button className="primary-button"
      disabled={disabled || unselectedService || (canStart && !status)} onClick={onEnableRegistration}>{c("connectGemini")}</button>}
    {(!onEnableRegistration || dcr) && address}
    {awaitingGeminiUrl && <div className="gemini-endpoint-pending" role="status">
      <strong>{c("publicUrlMissing")}</strong>
      <p>{missingUrlReason}</p>
    </div>}
    {origin && dcr && !status?.observed_authorization && <p>{c("addInGemini")}</p>}
    {feedback && feedback !== "copied" && <p role="alert">{c(feedback)}</p>}
    {quick && <p className="workspace-notice">{c("quickReauthorize")}</p>}
    {unselectedService && <p className="workspace-notice">{c("cloudflareSelectService")}</p>}
    {!advanced && <p>{status?.oauth_configured ? c("oauthConfigured") : c("oauthNotConfigured")}</p>}
    {((origin && dcr) || !advanced) && <p>{status?.observed_authorization ? c("oauthObserved") : c(dcr ? "awaitAuthorization" : "oauthNotObserved")}</p>}
    {origin && status?.observed_tool_call && <p>{c("toolObserved")}</p>}
    {registrationUnavailable && <p role="status">{c(discoveryRetry ? "discoveryRetry" : "registrationUnavailable")}</p>}
    {!awaitingGeminiUrl && reason === "cloudflare_binary_unavailable" && <p role="alert">{c("installCloudflared")}</p>}
    {routeMismatch && !quick && <p role="alert">{c("namedRouteHelp")}</p>}
    {reason === "cloudflare_already_active" && <p role="alert">{c("otherCloudflareActive")}</p>}
    {profile.server_restart_required && <p role="status">{c("profileRestartHelp")}</p>}
    {ingressNotApplied && <div className="server-owned-apply shell-notice warning" role="status"><strong>{c("serverRestartRequired")}</strong><p>{c("serverRestartHelp")}</p>{onRestartServer && <button type="button" className="primary-button" disabled={disabled} onClick={onRestartServer}>{c("restartServer")}</button>}</div>}
    {failure && !ingressNotApplied && !identityRepair && reason !== "cloudflare_already_active" && (
      <DesktopErrorAlert
        error={failure}
        disabled={disabled}
        actions={{ onRefresh: () => void refresh(), onCheckConnection: () => void refresh() }}
      />
    )}
    {identityRepair && !awaitingGeminiUrl && <p role="alert">{c("cloudflareIdentityRepair")}</p>}
    {identityRepair && onRepair && <button type="button" className="secondary-button" disabled={disabled} onClick={onRepair}>{c("openRuntimeSetup")}</button>}
    <div className="connection-actions">
      {(!onEnableRegistration || active) && <button type="button" className={active ? "secondary-button" : "primary-button"} disabled={disabled || !status || !canStart || unselectedService || profile.server_restart_required || status.configured_revision !== profile.revision || (!quick && !profile.credential_present)} onClick={() => void run(active ? "restart" : "start")}>{active ? c("reconnect") : dcr ? c("startPublicTunnel") : p("start")}</button>}
      {active && <button type="button" className="secondary-button" disabled={disabled} onClick={() => void run("stop")}>{p("stop")}</button>}
      {!advanced && manualButton}
    </div>
    <details className="connection-details"><summary>{c("profileDetails")}</summary>
      <p>{profile.host_mode === "embedded" ? c("serverOwned") : c("separateService")}</p>
      {status && <p>{c("localForwardingTarget")} <code>{status.local_target}</code></p>}
      {advanced && <>{!dcr && address}
        <details className="connection-details"><summary>{c("manualOauthOnly")}</summary>
          <p>{status?.oauth_configured ? c("oauthConfigured") : c("oauthNotConfigured")}</p>
          {manualButton}
        </details>
      </>}
      {safeReason && <p className="field-help">{c("technicalReason")}: <code>{safeReason}</code></p>}
      <div className="connection-actions">
        <button type="button" className="secondary-button" disabled={disabled} onClick={onEdit}>{p("edit")}</button>
        <button type="button" className="text-button" disabled={disabled} onClick={onDelete}>{c("delete")}</button>
      </div>
    </details>
    {oauth && running && status && oauth.server_instance_id === status.server_instance_id && oauth.process_generation === status.process_generation && oauth.public_origin === observedOrigin && <OAuthHandoff key={`${status.server_instance_id}:${status.process_generation}:${status.public_origin}`} profile={profile} status={status} onClose={() => { setOauth(null); void refresh(); }} />}
  </article>;
}
