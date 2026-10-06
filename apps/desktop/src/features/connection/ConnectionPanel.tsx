import { useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState } from "../../models/topology";
import type { DesktopError } from "../../models/topology";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { useLocale } from "../../i18n/locale";
import { useShellText } from "../../i18n/runtime-shell";
import { EMPTY_CONNECTIONS, type TunnelConnection, type TunnelProfileAction } from "../../models/connections-tools";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { desktopStatusPresentation } from "../workspace/WorkspaceStatus";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { FirstReadGuide } from "../onboarding/FirstRunCompletion";
import { ConnectionEditor } from "./ConnectionEditor";
import { ConnectionCard } from "./ConnectionCard";

export function ConnectionPanel({ state, onState, onSettings }: { state: DesktopState; onState: (state: DesktopState) => void; onSettings: (section: "network" | "runtime" | "diagnostics") => void }) {
  const p = useProduct(); const c = useConnectionsTools(); const { t } = useLocale(); const s = useShellText();
  const profiles = state.connections ?? EMPTY_CONNECTIONS;
  const [editor, setEditor] = useState<{ profile: TunnelConnection | null } | null>(null);
  const [deleting, setDeleting] = useState<TunnelConnection | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [failed, setFailed] = useState<DesktopError | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const submitting = useRef(false);
  const disabled = busyId !== null || Boolean(state.current_operation);
  const local = state.topology?.server.kind === "local" && state.topology.experience === "full";
  const presentation = desktopStatusPresentation(state, p);
  const autoProxyRecovery = failed?.code === "tunnel_unavailable" && state.tunnel_proxy.mode === "auto" && state.tunnel_proxy.effective_proxy_present;
  const route = state.tunnel_proxy.effective_source === "system" ? p("systemProxy") : state.tunnel_proxy.effective_source === "environment" ? p("environmentProxy") : state.tunnel_proxy.effective_source === "custom" ? p("customProxy") : state.tunnel_proxy.effective_source === "invalid_custom" ? `${t("settings.tunnelProxyCustom")} · ${p("unavailable")}` : t("settings.tunnelProxyDirectValue");
  const failure = failed ? desktopErrorPresentation(failed, t) : null;
  useEffect(() => { if (copiedId) { const timer = window.setTimeout(() => setCopiedId(null), 2500); return () => window.clearTimeout(timer); } }, [copiedId]);
  const run = async (id: string, action: TunnelProfileAction) => {
    if (submitting.current || state.current_operation) return;
    submitting.current = true; setBusyId(id); setFailed(null); setSaved(false);
    try { onState(await desktopApi.tunnelProfileAction(id, action)); if (action === "delete") setDeleting(null); }
    catch (value) { setFailed(normalizeDesktopError(value)); }
    finally { submitting.current = false; setBusyId(null); }
  };
  return <section className="page-section workspace-page" aria-labelledby="connection-title" data-webcodex-page="connection">
    <header className="page-heading-row"><h1 id="connection-title">{c("connections")}</h1><button type="button" className="primary-button" disabled={disabled || profiles.config_error} onClick={() => { setSaved(false); setEditor({ profile: null }); }}>{c("addConnection")}</button></header>
    <div className="connection-context"><span>{presentation.connectionText}</span><span>{c("networkRoute")} · <strong>{route}</strong></span><button type="button" className="text-button" onClick={() => onSettings("network")}>{c("proxySettings")}</button></div>
    {state.topology?.server.kind === "remote" && <p className="workspace-notice"><code>{state.topology.server.url}</code></p>}
    {presentation.externalConnectionUnobserved && <p className="workspace-notice">{c("externalConnectionUnobserved")}</p>}
    {local && !state.readiness.runtime_ready && <div className="workspace-notice"><p>{c("localRuntimeNeeded")}</p><button type="button" className="secondary-button" onClick={() => onSettings("runtime")}>{s("Runtime")}</button></div>}
    {profiles.config_error && <p role="alert" className="workspace-notice">{c("configError")}</p>}
    {failure && !deleting && <div role="alert" className="workspace-notice"><strong>{failure.title}</strong><p>{failure.action}</p><p>{c("operationFailed")}</p>{autoProxyRecovery && <p>{c("currentProxyHint")}</p>}</div>}
    {saved && <p role="status" className="workspace-notice">{c("saved")}</p>}
    <div className="connection-list">{profiles.profiles.map(profile => <ConnectionCard key={profile.id} profile={profile} canStart={local && state.readiness.runtime_ready} busy={disabled} copied={copiedId === profile.id}
      onAction={action => void run(profile.id, action)} onSettings={onSettings}
      onEdit={() => { setSaved(false); setEditor({ profile }); }} onDelete={() => { setFailed(null); setDeleting(profile); }}
      onCopy={() => { if (profile.tunnel_id) void writeText(profile.tunnel_id).then(() => setCopiedId(profile.id)).catch(value => setFailed(normalizeDesktopError(value))); }} />)}</div>
    {state.connections && !profiles.profiles.length && !profiles.config_error && state.topology?.experience !== "quick_share" && <p className="workspace-empty">{presentation.connectionText}</p>}
    {state.topology?.experience === "quick_share" && state.quick_share && <button className="secondary-button" disabled={disabled} onClick={() => { void desktopApi.stopQuickShare().then(onState).catch(value => setFailed(normalizeDesktopError(value))); }}>{p("stop")} Quick Share</button>}
    <FirstReadGuide state={state} />
    {editor && <ConnectionEditor profile={editor.profile} onState={next => { onState(next); setSaved(true); }} onClose={() => setEditor(null)} />}
    {deleting && <WorkspaceDialog title={c("delete") + " " + deleting.name} onClose={() => setDeleting(null)} busy={busyId !== null}>
      <p>{c("deleteConnectionHelp")}</p>
      {failed && <p role="alert" className="workspace-notice">{c("operationFailed")}</p>}
      <div className="connection-actions"><button type="button" className="primary-button" disabled={disabled} onClick={() => void run(deleting.id, "delete")}>{c("delete")}</button><button type="button" className="secondary-button" disabled={disabled} onClick={() => setDeleting(null)}>{p("cancel")}</button></div>
    </WorkspaceDialog>}
  </section>;
}
