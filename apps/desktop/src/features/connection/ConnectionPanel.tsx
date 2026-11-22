import { useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";
import { normalizeDesktopError } from "../../i18n/presentation";
import { DesktopErrorAlert } from "../../components/DesktopErrorAlert";
import { useLocale } from "../../i18n/locale";
import { EMPTY_CONNECTIONS, type TunnelConnection, type TunnelProfileAction } from "../../models/connections-tools";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { ChatgptObservation, desktopStatusPresentation } from "../workspace/WorkspaceStatus";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { desktopIdentity } from "../../lib/desktop-observation";
import { FirstRun } from "../onboarding/FirstRun";
import { LocalMigrationSetup } from "../onboarding/LocalMigrationSetup";
import { FirstReadGuide } from "../onboarding/FirstRunCompletion";
import { ConnectionEditor } from "./ConnectionEditor";
import { GeminiConnections } from "./GeminiConnections";
import { ConnectionCard } from "./ConnectionCard";

export function ConnectionPanel({ state, onState, onSettings, initialClient }: {
  initialClient?: "chatgpt" | "gemini";
  state: DesktopState;
  onState: (state: DesktopState) => void;
  onSettings: (section: "network" | "runtime" | "diagnostics") => void;
}) {
  useEffect(() => { if (initialClient) document.getElementById(initialClient + "-heading")?.focus(); }, [initialClient]);
  const p = useProduct();
  const c = useConnectionsTools();
  const { t } = useLocale();
  const profiles = state.connections ?? EMPTY_CONNECTIONS;
  const [editor, setEditor] = useState<{ profile: TunnelConnection | null; geminiNamed?: boolean; chatgptPreset?: boolean } | null>(null);
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
  const serverOwnedProfiles = profiles.profiles.filter(profile => (!profile.provider || profile.provider.kind === "openai") && profile.host_mode === "embedded" && profile.autostart);
  const serverRestartRequired = persistentLocal && profiles.profiles.some(profile =>
    (!profile.provider || profile.provider.kind === "openai") && profile.server_restart_required);
  const serverStartRequired = persistentLocal
    && !serverRestartRequired
    && serverOwnedProfiles.length > 0
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
  useEffect(() => {
    if (!copiedId) return;
    const timer = window.setTimeout(() => setCopiedId(null), 2500);
    return () => window.clearTimeout(timer);
  }, [copiedId]);
  const cloudflareProfiles = profiles.profiles.filter(profile => profile.provider && profile.provider.kind !== "openai");
  const openaiProfiles = profiles.profiles.filter(profile => !profile.provider || profile.provider.kind === "openai");
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
      if (profile && !profile.configuration_id) throw {code: "cloudflare_revision_stale"};
      const next = await desktopApi.connectGemini({
        environment_id: environmentId, profile: profile ? {
          id: profile.id, configuration_id: profile.configuration_id!, revision: profile.revision,
        } : null,
      });
      if (currentEnvironment() && next.persistent_environment === environmentId &&
          (next.state_generation ?? 0) >= (latest.current.state_generation ?? 0)) onState(next);
    } catch (value) {
      if (currentEnvironment()) {
        setFailed(normalizeDesktopError(value));
        // A save or start may have taken effect before its reply was lost.
        try {
          const observed = await desktopApi.getState();
          if (currentEnvironment() && observed.persistent_environment === environmentId &&
              (observed.state_generation ?? 0) >= (latest.current.state_generation ?? 0)) onState(observed);
        } catch { /* Keep the original diagnostic. */ }
      }
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
    preparation = <LocalMigrationSetup key={desktopIdentity(state)} state={state} onComplete={onState} onReset={onState} onDiagnostics={diagnostics} />;
  } else if (!persistentLocal || state.environment_setup?.configured === false) {
    preparation = localSetup
      ? <FirstRun state={state} onState={onState} onComplete={() => setLocalSetup(false)} onCancelLocal={() => setLocalSetup(false)} />
      : <div><p>{t("setup.geminiPrepare")}</p><button className="primary-button" disabled={disabled} onClick={() => setLocalSetup(true)}>{t(state.environment_setup?.configured === false ? "setup.resumeLocal" : "setup.prepareComputer")}</button></div>;
  }
  return <section className="page-section workspace-page" aria-labelledby="connection-title" data-webcodex-page="connection">
    <header className="page-heading-row"><h1 id="connection-title">{c("connections")}</h1></header>
    {state.topology?.server.kind === "remote" && <p className="workspace-notice"><code>{state.topology.server.url}</code></p>}
    {presentation.externalConnectionUnobserved && <p className="workspace-notice">{c("externalConnectionUnobserved")}</p>}
    {serverRestartRequired && <div className="server-owned-apply shell-notice warning" role="status"><div><strong>{c("serverRestartRequired")}</strong><p>{c("serverRestartHelp")}</p></div><button type="button" className="primary-button" disabled={disabled} onClick={() => void runServer("restart")}>{c("restartServer")}</button></div>}
    {serverStartRequired && <div className="server-owned-apply shell-notice" role="status"><div><strong>{c("serverStartRequired")}</strong><p>{c("serverStartHelp")}</p></div><button type="button" className="primary-button" disabled={disabled} onClick={() => void runServer("start")}>{c("startServer")}</button></div>}
    {profiles.config_error && <p role="alert" className="workspace-notice">{c("configError")}</p>}
    {failed && !deleting && <div className="connection-error">
      <DesktopErrorAlert
        error={failed}
        disabled={disabled}
        actions={{
          onOpenDiagnostics: diagnostics,
          onRestartServer: environmentId ? () => void runServer("restart") : undefined,
          onOpenRuntime: () => onSettings("runtime"),
          onOpenNetwork: () => onSettings("network"),
          onRefresh: () => void desktopApi.refresh().then(onState).catch(value => setFailed(normalizeDesktopError(value))),
          onCheckConnection: () => void desktopApi.refresh().then(onState).catch(value => setFailed(normalizeDesktopError(value))),
        }}
        extraActions={autoProxyRecovery ? <p className="field-help">{c("currentProxyHint")}</p> : undefined}
      />
      {failed.code === "tunnel_revision_stale" && <p className="field-help">{c("staleConnection")}</p>}
      {failed.code === "cloudflare_selection_conflict" && <p className="field-help">{c("otherCloudflareActive")}</p>}
    </div>}
    {saved && <p role="status" className="workspace-notice">{c("saved")}</p>}
    <GeminiConnections key={environmentId} profiles={cloudflareProfiles} preparation={preparation} preparing={busyId === "__gemini__"}
      busy={disabled || !state.connections || profiles.config_error} canStart={persistentLocal && state.readiness.server === "ready" && busyId !== "__server__" && state.current_operation?.kind !== "environment_service"}
      onPrepare={profile => void prepareGemini(profile)} onNamed={() => setEditor({profile: null, geminiNamed: true})}
      onRepair={() => onSettings("runtime")} onRestartServer={persistentLocal && !serverRestartRequired ? () => void runServer("restart") : undefined}
      onEdit={profile => setEditor({profile})} onDelete={profile => { setFailed(null); setDeleting(profile); }} />
    {state.topology?.experience !== "quick_share" && <section className="chatgpt-connection" aria-labelledby="chatgpt-heading">
      <header className="workspace-section-heading">
        <div>
          <h2 id="chatgpt-heading" tabIndex={-1}>{c("connectChatgpt")}</h2>
          <p className="field-help">{c("chatgptSectionHelp")}</p>
        </div>
        {canManageConnections && !profiles.config_error && <button type="button" className={openaiProfiles.length ? "secondary-button" : "primary-button"} disabled={disabled} onClick={() => { setSaved(false); setEditor({ profile: null, chatgptPreset: true }); }}>{c("connectChatgpt")}</button>}
      </header>
      {local && !state.readiness.runtime_ready && !serverStartRequired && !serverRestartRequired && openaiProfiles.length > 0 &&
        <div className="shell-actions"><p>{c("localRuntimeNeeded")}</p>
          <button type="button" className="secondary-button" disabled={disabled} onClick={() => void desktopApi.resumeSavedRuntime().then(onState).catch(value => setFailed(normalizeDesktopError(value)))}>{p("start")} WebCodex</button></div>}
      {(openaiProfiles.length > 0 || state.chatgpt_activity?.observed) && <ChatgptObservation state={state} />}
      <div className="connection-list">{openaiProfiles.map(profile => <ConnectionCard key={profile.id} profile={profile} canStart={local && state.readiness.runtime_ready} busy={disabled || !canManageConnections} copied={copiedId === profile.id}
        onAction={action => void run(profile.id, action)} onSettings={onSettings}
        onEdit={() => { setSaved(false); setEditor({ profile }); }} onDelete={() => { setFailed(null); setDeleting(profile); }}
        onCopy={() => { if (profile.tunnel_id) void writeText(profile.tunnel_id).then(() => setCopiedId(profile.id)).catch(value => setFailed(normalizeDesktopError(value))); }} />)}</div>
      {state.connections && !profiles.profiles.length && !profiles.config_error && <p className="workspace-empty">{presentation.connectionText}</p>}
    </section>}
    {state.topology?.experience === "quick_share" && state.quick_share && <button className="secondary-button" disabled={disabled} onClick={() => { void desktopApi.stopQuickShare().then(onState).catch(value => setFailed(normalizeDesktopError(value))); }}>{p("stop")} Quick Share</button>}
    <details className="connection-details" onToggle={event => setShowReadGuide(event.currentTarget.open)}>
      <summary>{t("completion.chatgptGuide")}</summary>
      {showReadGuide && <FirstReadGuide state={state} />}
    </details>
    <details className="connection-details"><summary>{t("first.advanced")}</summary>
    <div className="connection-context"><span>{presentation.connectionText}</span><div className="connection-route"><span>{c("networkRoute")}</span><strong>{route}</strong></div><button type="button" className="text-button" onClick={() => onSettings("network")}>{c("proxySettings")}</button></div>
      <button type="button" className="secondary-button" disabled={disabled || profiles.config_error || !canManageConnections} onClick={() => { setSaved(false); setEditor({ profile: null }); }}>{c("addConnection")}</button>
    </details>
    {editor && (!editor.profile || profiles.profiles.some(item => item.id === editor.profile?.id && item.revision === editor.profile.revision && item.configuration_id === editor.profile.configuration_id)) && <ConnectionEditor environmentId={environmentId} profile={editor.profile} geminiNamed={editor.geminiNamed} chatgptPreset={editor.chatgptPreset} initialAutostart={!editor.geminiNamed || !cloudflareProfiles.some(item => item.autostart)} persistentLocal={persistentLocal} onSetup={() => requestAnimationFrame(() => document.getElementById("gemini-heading")?.focus())} onState={next => { onState(next); setSaved(true); }} onClose={() => setEditor(null)} />}
    {deleting && <WorkspaceDialog title={c("delete") + " " + deleting.name} onClose={() => setDeleting(null)} busy={busyId !== null}>
      <p>{deleting.host_mode === "embedded" ? c("deleteServerOwnedHelp") : c("deleteConnectionHelp")}</p>
      {failed && <DesktopErrorAlert error={failed} disabled={disabled} actions={{ onOpenDiagnostics: diagnostics }} />}
      <div className="connection-actions"><button type="button" className="danger-button" disabled={disabled} onClick={() => void run(deleting.id, "delete", deleting.provider && deleting.provider.kind !== "openai" ? deleting.revision : undefined, deleting.configuration_id)}>{c("delete")}</button><button type="button" className="secondary-button" disabled={disabled} onClick={() => setDeleting(null)}>{p("cancel")}</button></div>
    </WorkspaceDialog>}
  </section>;
}
