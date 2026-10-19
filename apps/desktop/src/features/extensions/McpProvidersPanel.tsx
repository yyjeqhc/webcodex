import { useRef, useState } from "react";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState, RunnerSettings } from "../../models/topology";
import { EMPTY_MCP_PROVIDERS, type McpProviderProfile } from "../../models/connections-tools";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { McpProviderEditor } from "./McpProviderEditor";

export function McpProvidersPanel({ state, onState, settings, onRestarted }: { state: DesktopState; onState: (state: DesktopState) => void; settings: RunnerSettings | null; onRestarted: () => void }) {
  const p = useProduct(); const c = useConnectionsTools(); const providers = state.mcp_providers ?? EMPTY_MCP_PROVIDERS;
  const [editor, setEditor] = useState<{ profile: McpProviderProfile | null; revision: number } | null>(null);
  const [deleting, setDeleting] = useState<{ profile: McpProviderProfile; revision: number } | null>(null);
  const [busy, setBusy] = useState(false); const [failed, setFailed] = useState(false); const [saved, setSaved] = useState(false);
  const submitting = useRef(false); const disabled = busy || Boolean(state.current_operation);
  const restart = async () => {
    if (submitting.current || state.current_operation) return;
    submitting.current = true; setBusy(true); setFailed(false);
    try {
      const current = await desktopApi.runnerSettings();
      if (!current.can_restart) throw new Error("runner_unavailable");
      onState(await desktopApi.restartOwnedRunner(current.target)); onRestarted(); setSaved(false);
    } catch { setFailed(true); }
    finally { submitting.current = false; setBusy(false); }
  };
  const remove = async () => {
    if (!deleting || submitting.current || state.current_operation) return;
    submitting.current = true; setBusy(true); setFailed(false);
    try { onState(await desktopApi.removeMcpProvider(deleting.profile.id, deleting.revision)); setDeleting(null); setSaved(true); }
    catch { setFailed(true); }
    finally { submitting.current = false; setBusy(false); }
  };
  // The enclosing tabpanel already names this group. Avoid redundant native AX
  // landmarks that bury launch controls below the bounded Computer Use tree.
  return <div className="mcp-providers" role="presentation">
    <div className="extension-toolbar"><button type="button" className="primary-button" disabled={disabled || providers.config_error} onClick={() => setEditor({ profile: null, revision: providers.revision })}>{c("addMcpProvider")}</button></div>
    <p className="workspace-notice">{c("sharedProviders")}</p>
    {(saved || providers.restart_required) && <div className="extension-apply-bar" role="status"><span>{c("saved")}{providers.restart_required ? ` · ${p("needsRestart")}` : ""}</span>{providers.restart_required && <button type="button" className="secondary-button" disabled={disabled || !settings?.can_restart} onClick={() => void restart()}>{p("restartRunner")}</button>}</div>}
    {providers.config_error && <p className="workspace-notice" role="alert">{c("configError")}</p>}
    {failed && !deleting && <p className="workspace-notice" role="alert">{c("operationFailed")}</p>}
    {providers.profiles.map(profile => <article key={profile.id} className="extension-row mcp-provider-row" aria-labelledby={`mcp-provider-${profile.id}`} data-mcp-provider-id={profile.id}>
      <div><h3 id={`mcp-provider-${profile.id}`}>{profile.name}</h3><span>Local MCP · {c(profile.enabled ? "configured" : "disabled")}</span><p className="mcp-command">{c("command")}: <code>{profile.command}</code></p>{profile.env_keys.length > 0 && <span>{c("credentials")}</span>}</div>
      <div className="connection-actions"><button type="button" className="secondary-button" disabled={disabled} aria-label={`${p("edit")} ${profile.name}`} onClick={() => setEditor({ profile, revision: providers.revision })}>{p("edit")}</button><button type="button" className="text-button" disabled={disabled} aria-label={`${c("delete")} ${profile.name}`} onClick={() => { setFailed(false); setDeleting({ profile, revision: providers.revision }); }}>{c("delete")}</button></div>
    </article>)}
    {!providers.profiles.length && !providers.config_error && <p className="workspace-empty">{c("noProviders")}</p>}
    <details className="workspace-technical"><summary>{p("advanced")}</summary><p>{c("capacity")}: {providers.profiles.filter(profile => profile.enabled).length} / {providers.max_enabled}</p>{settings && <code>{settings.target.config_path}</code>}</details>
    {editor && <McpProviderEditor profile={editor.profile} revision={editor.revision} onState={next => { onState(next); setSaved(true); }} onClose={() => setEditor(null)} />}
    {deleting && <WorkspaceDialog title={`${c("delete")} ${deleting.profile.name}`} onClose={() => setDeleting(null)} busy={busy}><p>{c("deleteProviderHelp")}</p>{failed && <p role="alert" className="workspace-notice">{c("operationFailed")}</p>}<div className="connection-actions"><button type="button" className="danger-button" disabled={disabled} onClick={() => void remove()}>{c("delete")}</button><button type="button" className="secondary-button" disabled={disabled} onClick={() => setDeleting(null)}>{p("cancel")}</button></div></WorkspaceDialog>}
  </div>;
}
