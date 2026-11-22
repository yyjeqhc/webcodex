import { useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { useLocale } from "../../i18n/locale";
import { useShellText } from "../../i18n/runtime-shell";
import { EMPTY_CONNECTIONS, type TunnelConnection, type TunnelProfileAction } from "../../models/connections-tools";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { desktopStatusPresentation } from "../workspace/WorkspaceStatus";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { desktopIdentity } from "../../lib/desktop-observation";
import { FirstRun } from "../onboarding/FirstRun";
import { LocalMigrationSetup } from "../onboarding/LocalMigrationSetup";
import { FirstReadGuide } from "../onboarding/FirstRunCompletion";
import { ConnectionEditor } from "./ConnectionEditor";
import { GeminiConnections } from "./GeminiConnections";
import { ConnectionCard } from "./ConnectionCard";

export function ConnectionPanel({ state, onState, onSettings }: {
  state: DesktopState;
  onState: (state: DesktopState) => void;
  onSettings: (section: "network" | "runtime" | "diagnostics") => void;
}) {
  const p = useProduct();
  const c = useConnectionsTools();
  const { t } = useLocale();
  const s = useShellText();
  const profiles = state.connections ?? EMPTY_CONNECTIONS;
  const [editor, setEditor] = useState<{ profile: TunnelConnection | null; geminiNamed?: boolean } | null>(null);
  const [deleting, setDeleting] = useState<TunnelConnection | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [failed, setFailed] = useState<DesktopError | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [showReadGuide, setShowReadGuide] = useState(false);
  const [localSetup, setLocalSetup] = useState(false);
  const [saved, setSaved] = useState(false);
  const submitting = useRef(false);
  const disabled = busyId !== null || Boolean(state.current_operation);
  const local = state.topology?.server.kind === "local" && state.topology.experience === "full";
  const environmentId = state.persistent_environment?.trim() || null;
  const persistentLocal = Boolean(environmentId && local);
  const canManageConnections = !environmentId || local;
  const serverOwnedProfiles = profiles.profiles.filter(profile => profile.host_mode === "embedded" && profile.autostart);
  const serverRestartRequired = persistentLocal && profiles.server_restart_required;
  const serverStartRequired = persistentLocal
    && !serverRestartRequired
    && (serverOwnedProfiles.length > 0 || profiles.profiles.some(profile => profile.provider && profile.provider.kind !== "openai"))
    && !["ready", "starting"].includes(state.readiness.server);
  const presentation = desktopStatusPresentation(state, p);
  const autoProxyRecovery = failed?.code === "tunnel_unavailable" && state.tunnel_proxy.mode === "auto" && state.tunnel_proxy.effective_proxy_present;
  const route = state.tunnel_proxy.effective_source === "system"
    ? p("systemProxy")
    : state.tunnel_proxy.effective_source === "environment"
      ? p("environmentProxy")
      : state.tunnel_proxy.effective_source === "custom"
        ? p("customProxy")
        : state.tunnel_proxy.effective_source === "invalid_custom"
          ? `${t("settings.tunnelProxyCustom")} · ${p("unavailable")}`
          : t("settings.tunnelProxyDirectValue");
  const failure = failed ? desktopErrorPresentation(failed, t) : null;
  useEffect(() => {
    if (!copiedId) return;
    const timer = window.setTimeout(() => setCopiedId(null), 2500);
    return () => window.clearTimeout(timer);
  }, [copiedId]);
  const cloudflareProfiles = profiles.profiles.filter(profile => profile.provider && profile.provider.kind !== "openai");
  const latest = useRef(state);
  latest.current = state;
  const mounted = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const prepareGemini = async (profile: TunnelConnection | null) => {
    if (!environmentId || !persistentLocal || !state.connections || profiles.config_error || submitting.current || state.current_operation) return;
    submitting.current = true;
    setBusyId("__gemini__");
    setFailed(null);
    setSaved(false);
    const currentEnvironment = () => mounted.current && latest.current.persistent_environment === environmentId;
    try {
      const autostart = profile?.autostart ?? !cloudflareProfiles.some(item => item.autostart);
      const savedState = await desktopApi.saveTunnelProfile({
        id: profile?.id ?? null, name: profile?.name ?? "Gemini", provider: profile?.provider ?? {kind: "cloudflare_quick"},
        host_mode: profile?.host_mode ?? "embedded", tunnel_id: profile?.tunnel_id ?? "", api_key: null,
        expected_target: {environment_id: environmentId, configuration_id: profile?.configuration_id ?? null},
        autostart, expected_revision: profile?.revision ?? null, oauth_dynamic_registration_enabled: true,
      });
      if (!currentEnvironment() || savedState.persistent_environment !== environmentId) return;
      const lastSeen = latest.current;
      // An intervening observation may already include deletion, stop or another
      // edit. Read the canonical snapshot and leave any next action to the user.
      const next = lastSeen === state ? savedState : await desktopApi.getState();
      if (!currentEnvironment() || lastSeen !== latest.current || next.persistent_environment !== environmentId) return;
      onState(next);
      if (lastSeen === state && !profile && autostart && state.readiness.server === "stopped" && next.readiness.server === "stopped") {
        const started = await desktopApi.environmentServiceAction({environmentId, component: "server", action: "start"});
        if (currentEnvironment() && started.persistent_environment === environmentId) onState(started);
      }
    } catch (value) {
      if (currentEnvironment()) setFailed(normalizeDesktopError(value));
    } finally {
      submitting.current = false;
      if (mounted.current) setBusyId(null);
    }
  };
  const run = async (id: string, action: TunnelProfileAction, expectedRevision?: number, expectedConfigurationId?: string | null) => {
    if (submitting.current || state.current_operation) return;
    submitting.current = true;
    setBusyId(id);
    setFailed(null);
    setSaved(false);
    try {
      onState(await (expectedRevision === undefined ? desktopApi.tunnelProfileAction(id, action) : desktopApi.tunnelProfileAction(id, action, expectedRevision, expectedConfigurationId)));
      if (action === "delete") setDeleting(null);
    } catch (value) {
      setFailed(normalizeDesktopError(value));
    } finally {
      submitting.current = false;
      setBusyId(null);
    }
  };
  const runServer = async (action: "start" | "restart") => {
    if (!environmentId || submitting.current || state.current_operation) return;
    submitting.current = true;
    setBusyId("__server__");
    setFailed(null);
    setSaved(false);
    try {
      onState(await desktopApi.environmentServiceAction({ environmentId, component: "server", action }));
    } catch (value) {
      setFailed(normalizeDesktopError(value));
    } finally {
      submitting.current = false;
      setBusyId(null);
    }
  };
  const diagnostics = () => onSettings("diagnostics");
  const remote = state.topology?.server.kind === "remote" || state.environment_setup?.mode === "join";
  let preparation;
  if (state.configuration_issue || profiles.config_error || (!local && environmentId && !remote)) {
    preparation = <div><p>{t("migration.recovery")}</p><button className="secondary-button" onClick={diagnostics}>{p("troubleshooting")}</button></div>;
  } else if (state.quick_share || state.topology?.experience === "quick_share") {
    preparation = <p>{t("setup.stopSharing")}</p>;
  } else if (remote) {
    preparation = <p>{t("setup.geminiRemote")}</p>;
  } else if (persistentLocal && state.topology?.runner.kind === "none" && state.environment_setup?.configured !== false) {
    preparation = <div><p>{t("setup.geminiRunner")}</p><button className="secondary-button" onClick={() => onSettings("runtime")}>{c("openRuntimeSetup")}</button></div>;
  } else if (!persistentLocal && local && !state.environment_setup) {
    preparation = <LocalMigrationSetup key={desktopIdentity(state)} state={state} onComplete={onState} onDiagnostics={diagnostics} />;
  } else if (!persistentLocal || state.environment_setup?.configured === false) {
    preparation = localSetup
      ? <FirstRun state={state} onState={onState} onComplete={() => setLocalSetup(false)} onCancelLocal={() => setLocalSetup(false)} />
      : <div><p>{t("setup.geminiPrepare")}</p><button className="primary-button" disabled={disabled} onClick={() => setLocalSetup(true)}>{t(state.environment_setup?.configured === false ? "setup.resumeLocal" : "setup.prepareComputer")}</button></div>;
  }
  return <section className="page-section workspace-page" aria-labelledby="connection-title" data-webcodex-page="connection">
    <header className="page-heading-row"><h1 id="connection-title">{c("connections")}</h1><button type="button" className="secondary-button" disabled={disabled || profiles.config_error || !canManageConnections} onClick={() => { setSaved(false); setEditor({ profile: null }); }}>{c("addConnection")}</button></header>
    <div className="connection-context"><span>{presentation.connectionText}</span><div className="connection-route"><span>{c("networkRoute")}</span><strong>{route}</strong></div><button type="button" className="text-button" onClick={() => onSettings("network")}>{c("proxySettings")}</button></div>
    {state.topology?.server.kind === "remote" && <p className="workspace-notice"><code>{state.topology.server.url}</code></p>}
    {presentation.externalConnectionUnobserved && <p className="workspace-notice">{c("externalConnectionUnobserved")}</p>}
    {local && !state.readiness.runtime_ready && <div className="workspace-notice"><p>{c("localRuntimeNeeded")}</p><button type="button" className="secondary-button" onClick={() => onSettings("runtime")}>{s("Runtime")}</button></div>}
    {serverRestartRequired && <div className="server-owned-apply shell-notice warning" role="status"><div><strong>{c("serverRestartRequired")}</strong><p>{c("serverRestartHelp")}</p></div><button type="button" className="primary-button" disabled={disabled} onClick={() => void runServer("restart")}>{c("restartServer")}</button></div>}
    {serverStartRequired && <div className="server-owned-apply shell-notice" role="status"><div><strong>{c("serverStartRequired")}</strong><p>{c("serverStartHelp")}</p></div><button type="button" className="primary-button" disabled={disabled} onClick={() => void runServer("start")}>{c("startServer")}</button></div>}
    {profiles.config_error && <p role="alert" className="workspace-notice">{c("configError")}</p>}
    {failure && !deleting && <div role="alert" className="workspace-notice"><strong>{failure.title}</strong><p>{failed?.code === "tunnel_revision_stale" ? c("staleConnection") : failed?.code === "cloudflare_selection_conflict" ? c("otherCloudflareActive") : failure.action}</p><p>{c("operationFailed")}</p>{autoProxyRecovery && <p>{c("currentProxyHint")}</p>}</div>}
    {saved && <p role="status" className="workspace-notice">{c("saved")}</p>}
    <GeminiConnections key={environmentId} profiles={cloudflareProfiles} preparation={preparation} preparing={busyId === "__gemini__"}
      busy={disabled || !state.connections || profiles.config_error} canStart={persistentLocal && state.readiness.server === "ready" && busyId !== "__server__" && state.current_operation?.kind !== "environment_service"}
      onPrepare={profile => void prepareGemini(profile)} onNamed={() => setEditor({profile: null, geminiNamed: true})}
      onRepair={() => onSettings("runtime")} onRestartServer={persistentLocal && !serverRestartRequired ? () => void runServer("restart") : undefined}
      onEdit={profile => setEditor({profile})} onDelete={profile => { setFailed(null); setDeleting(profile); }} />
    <div className="connection-list">{profiles.profiles.filter(profile => !profile.provider || profile.provider.kind === "openai").map(profile => <ConnectionCard key={profile.id} profile={profile} canStart={local && state.readiness.runtime_ready} busy={disabled || !canManageConnections} copied={copiedId === profile.id}
      onAction={action => void run(profile.id, action)} onSettings={onSettings}
      onEdit={() => { setSaved(false); setEditor({ profile }); }} onDelete={() => { setFailed(null); setDeleting(profile); }}
      onCopy={() => { if (profile.tunnel_id) void writeText(profile.tunnel_id).then(() => setCopiedId(profile.id)).catch(value => setFailed(normalizeDesktopError(value))); }} />)}</div>
    {state.connections && !profiles.profiles.length && !profiles.config_error && state.topology?.experience !== "quick_share" && <p className="workspace-empty">{presentation.connectionText}</p>}
    {state.topology?.experience === "quick_share" && state.quick_share && <button className="secondary-button" disabled={disabled} onClick={() => { void desktopApi.stopQuickShare().then(onState).catch(value => setFailed(normalizeDesktopError(value))); }}>{p("stop")} Quick Share</button>}
    <details className="connection-details" onToggle={event => setShowReadGuide(event.currentTarget.open)}>
      <summary>{t("completion.chatgptGuide")}</summary>
      {showReadGuide && <FirstReadGuide state={state} />}
    </details>
    {editor && (!editor.profile || profiles.profiles.some(item => item.id === editor.profile?.id && item.revision === editor.profile.revision && item.configuration_id === editor.profile.configuration_id)) && <ConnectionEditor environmentId={environmentId} profile={editor.profile} geminiNamed={editor.geminiNamed} initialAutostart={!editor.geminiNamed || !cloudflareProfiles.some(item => item.autostart)} persistentLocal={persistentLocal} onSetup={() => requestAnimationFrame(() => document.getElementById("gemini-heading")?.focus())} onState={next => { onState(next); setSaved(true); }} onClose={() => setEditor(null)} />}
    {deleting && <WorkspaceDialog title={c("delete") + " " + deleting.name} onClose={() => setDeleting(null)} busy={busyId !== null}>
      <p>{deleting.host_mode === "embedded" ? c("deleteServerOwnedHelp") : c("deleteConnectionHelp")}</p>
      {failed && <p role="alert" className="workspace-notice">{c("operationFailed")}</p>}
      <div className="connection-actions"><button type="button" className="danger-button" disabled={disabled} onClick={() => void run(deleting.id, "delete", deleting.provider && deleting.provider.kind !== "openai" ? deleting.revision : undefined, deleting.configuration_id)}>{c("delete")}</button><button type="button" className="secondary-button" disabled={disabled} onClick={() => setDeleting(null)}>{p("cancel")}</button></div>
    </WorkspaceDialog>}
  </section>;
}
