import { useEffect, useRef, useState } from "react";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState, RunnerSettings, SettingsTarget } from "../../models/topology";
import { codingAgentIsActive, EMPTY_CODING_AGENTS, type CodingAgentInventory, type CodingAgentProfile } from "../../models/runner-capabilities";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { useRunnerCapabilitiesText } from "../../i18n/runner-capabilities";
import { workspaceQuery } from "../workspace/WorkspaceContext";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { CodingAgentEditor } from "./CodingAgentEditor";
import { RunnerCapabilityAuthorization } from "./RunnerCapabilityAuthorization";

export function CodingAgentsPanel({ state, onState, settings, onRestarted }: {
  state: DesktopState; onState: (state: DesktopState) => void; settings: RunnerSettings | null; onRestarted: () => void;
}) {
  const p = useProduct(); const c = useConnectionsTools(); const r = useRunnerCapabilitiesText();
  const configured = state.coding_agents ?? EMPTY_CODING_AGENTS;
  const [inventory, setInventory] = useState<CodingAgentInventory | null>(null);
  const [revision, setRevision] = useState(0); const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false); const [busy, setBusy] = useState(false); const submitting = useRef(false);
  const [editor, setEditor] = useState<{ profile: CodingAgentProfile | null; revision: number; target: SettingsTarget } | null>(null);
  const [deleting, setDeleting] = useState<{ profile: CodingAgentProfile; revision: number; target: SettingsTarget } | null>(null);
  useEffect(() => {
    let cancelled = false; setInventory(null); setLoading(true);
    void workspaceQuery<CodingAgentInventory>({ kind: "overview" }).then(value => {
      if (!cancelled && value.client_id === settings?.target.client_id) setInventory(value);
    }).catch(() => { if (!cancelled) setInventory(null); }).finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [settings?.target.client_id, settings?.target.config_path, settings?.target.server_url, revision, configured.restart_required]);
  const disabled = busy || Boolean(state.current_operation);
  const refresh = () => { setFailed(false); setRevision(value => value + 1); };
  const restart = async () => {
    if (submitting.current || state.current_operation) return;
    submitting.current = true; setBusy(true); setFailed(false); setInventory(null);
    try {
      const current = await desktopApi.runnerSettings();
      if (!current.can_restart || current.target.client_id !== settings?.target.client_id) throw new Error("runner_changed");
      onState(await desktopApi.restartOwnedRunner(current.target)); onRestarted(); refresh();
    } catch { setFailed(true); }
    finally { submitting.current = false; setBusy(false); }
  };
  const remove = async () => {
    if (!deleting || submitting.current || state.current_operation) return;
    submitting.current = true; setBusy(true); setFailed(false);
    try { onState(await desktopApi.removeCodingAgent(deleting.target, deleting.profile.provider_id, deleting.revision)); setDeleting(null); refresh(); }
    catch { setFailed(true); }
    finally { submitting.current = false; setBusy(false); }
  };
  const advertised = inventory?.connected ? inventory.coding_agent_providers ?? [] : [];
  return <div role="presentation">
    <div className="extension-toolbar"><button type="button" className="primary-button" aria-label="Add Coding Agent" disabled={disabled || !settings || configured.config_error} onClick={() => settings && setEditor({ profile: null, revision: configured.revision, target: settings.target })}>{r("addCodingAgent")}</button><button type="button" className="secondary-button" aria-label="Refresh Coding Agents" disabled={disabled || loading} onClick={refresh}>{p("refresh")}</button></div>
    <p className="workspace-notice">{r("agentHelp")}</p>
    <RunnerCapabilityAuthorization settings={settings} capability="coding_agents" disabled={disabled} refreshKey={revision} onAuthorized={refresh} />
    {configured.restart_required && <div className="extension-apply-bar" role="status"><span>{r("savedRestart")}</span><button type="button" className="secondary-button" aria-label="Restart Runner" disabled={disabled || !settings?.can_restart} onClick={() => void restart()}>{p("restartRunner")}</button></div>}
    {configured.config_error && <p role="alert" className="workspace-notice">{c("configError")}</p>}
    {failed && <p role="alert" className="workspace-notice">{c("operationFailed")}</p>}
    {loading && <p role="status">{p("loading")}</p>}
    {configured.profiles.map(profile => {
      const active = codingAgentIsActive(profile, inventory);
      return <article key={profile.provider_id} className="extension-row" aria-labelledby={`coding-provider-${profile.provider_id}`} data-coding-provider-id={profile.provider_id}>
        <div><h3 id={`coding-provider-${profile.provider_id}`}>{profile.name}</h3><code>{profile.provider_id}</code><p>{c("configured")} · {active ? r("active") : profile.enabled ? p("unavailable") : c("disabled")}{configured.restart_required ? ` · ${r("restartRequired")}` : ""}</p><details><summary>{p("details")}</summary><code>{profile.executable}</code>{Object.entries(profile.env_from_env).map(([child, runner]) => <p key={child}><code>{child} ← {runner}</code></p>)}</details></div>
        <div className="connection-actions"><button type="button" className="secondary-button" aria-label={`Edit ${profile.name}`} disabled={disabled || !settings} onClick={() => settings && setEditor({ profile, revision: configured.revision, target: settings.target })}>{p("edit")}</button><button type="button" className="text-button" aria-label={`Remove ${profile.name}`} disabled={disabled || !settings} onClick={() => settings && setDeleting({ profile, revision: configured.revision, target: settings.target })}>{c("remove")}</button></div>
      </article>;
    })}
    {advertised.filter(provider => !configured.profiles.some(profile => profile.provider_id === provider.provider_id)).map(provider => <article className="extension-row" key={provider.provider_id}><div><h3>{provider.name}</h3><code>{provider.provider_id}</code><p>{r("observed")} · {r("active")} · {r("readOnly")}</p></div></article>)}
    {!configured.profiles.length && !advertised.length && !configured.config_error && <p className="workspace-empty">{r("noCodingAgents")}</p>}
    {editor && <CodingAgentEditor profile={editor.profile} revision={editor.revision} target={editor.target} globals={configured.global_settings} onState={onState} onClose={() => setEditor(null)} />}
    {deleting && <WorkspaceDialog title={`${c("remove")} ${deleting.profile.name}`} onClose={() => setDeleting(null)} busy={busy}><p>{r("removeAgentHelp")}</p><div className="connection-actions"><button type="button" className="primary-button" aria-label={`Confirm Remove ${deleting.profile.name}`} disabled={disabled} onClick={() => void remove()}>{c("remove")}</button><button type="button" className="secondary-button" disabled={disabled} onClick={() => setDeleting(null)}>{p("cancel")}</button></div></WorkspaceDialog>}
  </div>;
}
