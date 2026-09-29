import { useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState } from "../../models/topology";
import type { DesktopError } from "../../models/topology";
import { normalizeDesktopError } from "../../i18n/presentation";
import { EMPTY_CONNECTIONS, type TunnelConnection, type TunnelProfileAction } from "../../models/connections-tools";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { ChatgptObservation, WorkspaceStatus } from "../workspace/WorkspaceStatus";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { ConnectionEditor } from "./ConnectionEditor";
import { ConnectionCard } from "./ConnectionCard";

export function ConnectionPanel({ state, onState }: { state: DesktopState; onState: (state: DesktopState) => void }) {
  const p = useProduct(); const c = useConnectionsTools();
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
  const autoProxyRecovery = failed?.code === "tunnel_unavailable" && state.tunnel_proxy.mode === "auto" && state.tunnel_proxy.effective_proxy_present;
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
    <WorkspaceStatus state={state} />
    <p className="workspace-notice">{c("sharedRuntime")}</p>
    {state.topology?.server.kind === "remote" && <p className="workspace-notice"><code>{state.topology.server.url}</code></p>}
    {(!local || !state.readiness.runtime_ready) && <p className="workspace-notice">{c("localRuntimeNeeded")}</p>}
    {profiles.config_error && <p role="alert" className="workspace-notice">{c("configError")}</p>}
    {failed && !deleting && <div role="alert" className="workspace-notice"><p>{c("operationFailed")}</p>{autoProxyRecovery && <p>{p("tunnelAutoProxyRecovery")}</p>}</div>}
    {saved && <p role="status" className="workspace-notice">{c("saved")}</p>}
    <div className="connection-list">{profiles.profiles.map(profile => <ConnectionCard key={profile.id} profile={profile} canStart={local && state.readiness.runtime_ready} busy={disabled} copied={copiedId === profile.id} autoProxyDetected={state.tunnel_proxy.mode === "auto" && state.tunnel_proxy.effective_proxy_present}
      onAction={action => void run(profile.id, action)}
      onEdit={() => { setSaved(false); setEditor({ profile }); }} onDelete={() => { setFailed(null); setDeleting(profile); }}
      onCopy={() => { if (profile.tunnel_id) void writeText(profile.tunnel_id).then(() => setCopiedId(profile.id)).catch(value => setFailed(normalizeDesktopError(value))); }} />)}</div>
    {!profiles.profiles.length && !profiles.config_error && <p className="workspace-empty">{c("noConnections")}</p>}
    {state.topology?.experience === "quick_share" && state.quick_share && <button className="secondary-button" disabled={disabled} onClick={() => { void desktopApi.stopQuickShare().then(onState).catch(value => setFailed(normalizeDesktopError(value))); }}>{p("stop")} Quick Share</button>}
    <section className="workspace-section connection-chatgpt"><h2>ChatGPT</h2><ChatgptObservation state={state} /></section>
    {editor && <ConnectionEditor profile={editor.profile} onState={next => { onState(next); setSaved(true); }} onClose={() => setEditor(null)} />}
    {deleting && <WorkspaceDialog title={c("delete") + " " + deleting.name} onClose={() => setDeleting(null)} busy={busyId !== null}>
      <p>{c("deleteConnectionHelp")}</p>
      {failed && <p role="alert" className="workspace-notice">{c("operationFailed")}</p>}
      <div className="connection-actions"><button type="button" className="primary-button" disabled={disabled} onClick={() => void run(deleting.id, "delete")}>{c("delete")}</button><button type="button" className="secondary-button" disabled={disabled} onClick={() => setDeleting(null)}>{p("cancel")}</button></div>
    </WorkspaceDialog>}
  </section>;
}
