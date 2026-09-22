import { useEffect, useRef, useState, type FormEvent } from "react";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState, RunnerSettings, SettingsTarget } from "../../models/topology";
import type { SshMutationResult, SshRegisterRequest, SshResource, SshResourceError, SshResourcesSnapshot } from "../../models/runner-capabilities";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { useRunnerCapabilitiesText, type RunnerCapabilitiesKey } from "../../i18n/runner-capabilities";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

function errorText(error: SshResourceError): RunnerCapabilitiesKey {
  switch (error) {
    case "ssh_resource_outcome_unknown": return "uncertain";
    case "runner_replaced": case "ssh_resource_registry_stale": case "ssh_resource_not_found": return "refreshNeeded";
    case "ssh_resource_name_conflict": case "ssh_resource_static_conflict": return "nameConflict";
    case "ssh_resource_static_read_only": return "readOnly";
    case "insufficient_scope": return "permissionNeeded";
    default: return "sshUnavailable";
  }
}
type ObservedTarget = { expected: SettingsTarget; observationId: string };

export function SshResourcesPanel({ state, onState, settings, onRestarted }: {
  state: DesktopState; onState: (state: DesktopState) => void; settings: RunnerSettings | null; onRestarted: () => void;
}) {
  const p = useProduct(); const c = useConnectionsTools(); const r = useRunnerCapabilitiesText();
  const [inventory, setInventory] = useState<SshResourcesSnapshot | null>(null);
  const [busy, setBusy] = useState(false); const [error, setError] = useState<SshResourceError | null>(null);
  const [restartRequired, setRestartRequired] = useState(false);
  const [grantTarget, setGrantTarget] = useState<SettingsTarget | null>(null);
  const [grantFailed, setGrantFailed] = useState(false);
  const [editor, setEditor] = useState<ObservedTarget | null>(null);
  const [deleting, setDeleting] = useState<(ObservedTarget & { resource: SshResource }) | null>(null);
  const submitting = useRef(false); const sequence = useRef(0); const mounted = useRef(true);
  const disabled = busy || Boolean(state.current_operation);

  useEffect(() => {
    mounted.current = true;
    const current = ++sequence.current;
    setInventory(null); setError(null); setGrantTarget(null); setGrantFailed(false);
    void desktopApi.sshResources().then(value => { if (mounted.current && current === sequence.current) setInventory(value); })
      .catch(() => { if (mounted.current && current === sequence.current) setError("ssh_resource_registry_unavailable"); });
    return () => { mounted.current = false; ++sequence.current; };
  }, [settings?.target.client_id, settings?.target.config_path, settings?.target.server_url]);

  const observe = async (action?: "add" | SshResource) => {
    if (submitting.current || state.current_operation) return;
    submitting.current = true; setBusy(true); setError(null); setGrantFailed(false);
    const current = ++sequence.current;
    try {
      const target = action ? (await desktopApi.runnerSettings()).target : null;
      const value = await desktopApi.sshResources();
      if (!mounted.current || current !== sequence.current) return;
      setInventory(value);
      if (action && target && value.available && value.observation_id && value.runner === target.client_id) {
        const observed = { expected: target, observationId: value.observation_id };
        if (action === "add") setEditor(observed);
        else {
          const resource = value.resources.find(resource => resource.name === action.name);
          if (resource?.source === "managed") setDeleting({ ...observed, resource });
          else setError(resource ? "ssh_resource_static_read_only" : "ssh_resource_not_found");
        }
      }
    } catch { if (mounted.current) setError("ssh_resource_registry_unavailable"); }
    finally { submitting.current = false; if (mounted.current) setBusy(false); }
  };
  const mutate = async (operation: () => Promise<SshMutationResult>) => {
    if (submitting.current || state.current_operation) return;
    submitting.current = true; ++sequence.current; setBusy(true); setError(null);
    // Targets exist only in this form's transient request, never inventory/history.
    setEditor(null); setDeleting(null);
    try {
      const result = await operation();
      if (!mounted.current) return;
      setInventory(result.inventory); setError(result.error_kind);
      if (result.restart_required) setRestartRequired(true);
    } catch {
      // Lost native IPC may follow a committed operation. Read once, never resend.
      if (mounted.current) { setError("ssh_resource_outcome_unknown"); setInventory(null); }
      try { const value = await desktopApi.sshResources(); if (mounted.current) setInventory(value); } catch { /* explicit Refresh remains available */ }
    } finally { submitting.current = false; if (mounted.current) setBusy(false); }
  };
  const restart = async () => {
    if (submitting.current || state.current_operation) return;
    submitting.current = true; ++sequence.current; setBusy(true); setError(null); setInventory(null);
    try {
      const current = await desktopApi.runnerSettings();
      if (!current.can_restart || current.target.client_id !== settings?.target.client_id) throw new Error("runner_changed");
      onState(await desktopApi.restartOwnedRunner(current.target)); onRestarted();
      const value = await desktopApi.sshResources();
      if (mounted.current) { setInventory(value); setRestartRequired(false); }
    } catch { if (mounted.current) setError("ssh_resource_registry_unavailable"); }
    finally { submitting.current = false; if (mounted.current) setBusy(false); }
  };
  const authorize = async () => {
    if (!grantTarget || submitting.current || state.current_operation) return;
    const expected = grantTarget;
    submitting.current = true; ++sequence.current; setBusy(true); setGrantFailed(false); setGrantTarget(null);
    try {
      const value = await desktopApi.authorizeRunnerCapabilities(expected);
      if (mounted.current) { setInventory(value); setError(null); }
    } catch { if (mounted.current) setGrantFailed(true); }
    finally { submitting.current = false; if (mounted.current) setBusy(false); }
  };
  const needsRestart = restartRequired || Boolean(inventory?.resources.some(resource => resource.pending_restart));
  const visibleError = error ?? inventory?.error_kind;
  return <div role="presentation" aria-busy={busy}>
    <div className="extension-toolbar"><button type="button" className="primary-button" aria-label="Add SSH Resource" disabled={disabled || !settings || !inventory?.available} onClick={() => void observe("add")}>{r("addSshResource")}</button><button type="button" className="secondary-button" aria-label="Refresh SSH Resources" disabled={disabled} onClick={() => void observe()}>{p("refresh")}</button></div>
    <p className="workspace-notice">{r("sshPrivacy")}</p>
    {visibleError && <p role="alert" className="workspace-notice">{r(errorText(visibleError))}</p>}
    {grantFailed && <p role="alert" className="workspace-notice">{r("authorizeFailed")}</p>}
    {inventory?.error_kind === "insufficient_scope" && inventory.can_authorize && settings && <button type="button" className="secondary-button" aria-label="Authorize Runner Capabilities" disabled={disabled} onClick={() => setGrantTarget(settings.target)}>{r("authorize")}</button>}
    {!inventory && !visibleError && <p role="status">{p("loading")}</p>}
    {needsRestart && <div className="extension-apply-bar" role="status"><span>{r("restartRequired")}</span><button type="button" className="secondary-button" aria-label="Restart Runner" disabled={disabled || !settings?.can_restart} onClick={() => void restart()}>{p("restartRunner")}</button></div>}
    {inventory?.available && inventory.resources.map(resource => <article className="extension-row" key={resource.name} aria-labelledby={`ssh-resource-${resource.name}`} data-ssh-resource-name={resource.name}>
      <div><h3 id={`ssh-resource-${resource.name}`}>{resource.name}</h3><p>{r(resource.source)} · {resource.active ? r("active") : c("configured")}{resource.pending_restart ? ` · ${r("restartRequired")}` : ""}{resource.source === "static" ? ` · ${r("readOnly")}` : ""}</p></div>
      {resource.source === "managed" && <button type="button" className="text-button" aria-label={`Remove ${resource.name}`} disabled={disabled || !inventory.observation_id} onClick={() => void observe(resource)}>{c("remove")}</button>}
    </article>)}
    {inventory?.available && !inventory.resources.length && <p className="workspace-empty">{r("noResources")}</p>}
    {editor && <SshResourceEditor observed={editor} busy={disabled} onClose={() => setEditor(null)} onSubmit={request => void mutate(() => desktopApi.registerSshResource(request))} />}
    {grantTarget && <WorkspaceDialog title={r("authorize")} onClose={() => setGrantTarget(null)} busy={busy}><p>{r("authorizeHelp")}</p><div className="connection-actions"><button type="button" className="primary-button" aria-label="Confirm Authorize Runner Capabilities" disabled={disabled} onClick={() => void authorize()}>{r("authorize")}</button><button type="button" className="secondary-button" disabled={disabled} onClick={() => setGrantTarget(null)}>{p("cancel")}</button></div></WorkspaceDialog>}
    {deleting && <WorkspaceDialog title={`${c("remove")} ${deleting.resource.name}`} onClose={() => setDeleting(null)} busy={busy}><p>{r("removeResourceHelp")}</p><div className="connection-actions"><button type="button" className="primary-button" aria-label={`Confirm Remove ${deleting.resource.name}`} disabled={disabled} onClick={() => void mutate(() => desktopApi.removeSshResource(deleting.expected, deleting.observationId, deleting.resource.name))}>{c("remove")}</button><button type="button" className="secondary-button" disabled={disabled} onClick={() => setDeleting(null)}>{p("cancel")}</button></div></WorkspaceDialog>}
  </div>;
}

function SshResourceEditor({ observed, busy, onSubmit, onClose }: {
  observed: ObservedTarget; busy: boolean; onSubmit: (request: SshRegisterRequest) => void; onClose: () => void;
}) {
  const p = useProduct(); const r = useRunnerCapabilitiesText();
  const [name, setName] = useState(""); const [target, setTarget] = useState(""); const [cwd, setCwd] = useState("");
  const submit = (event: FormEvent) => {
    event.preventDefault(); if (busy) return;
    onSubmit({ expected: observed.expected, observation_id: observed.observationId, name: name.trim(), target: target.trim(), default_cwd: cwd.trim() || null });
  };
  return <WorkspaceDialog title={r("addSshResource")} onClose={onClose} busy={busy}>
    <form className="profile-editor" onSubmit={submit}>
      <div className="field-group"><label htmlFor="ssh-resource-name">{r("resourceName")}</label><input id="ssh-resource-name" aria-label="Resource Name" value={name} onChange={event => setName(event.target.value)} required maxLength={80} disabled={busy} autoFocus spellCheck={false} /></div>
      <div className="field-group"><label htmlFor="ssh-resource-target">{r("target")}</label><input id="ssh-resource-target" aria-label="Target" value={target} onChange={event => setTarget(event.target.value)} required maxLength={512} disabled={busy} autoComplete="off" spellCheck={false} placeholder="special" aria-describedby="ssh-target-help" /><small id="ssh-target-help">{r("targetHelp")}</small></div>
      <div className="field-group"><label htmlFor="ssh-resource-cwd">{r("defaultCwd")}</label><input id="ssh-resource-cwd" aria-label="Default Working Directory" value={cwd} onChange={event => setCwd(event.target.value)} maxLength={4096} disabled={busy} autoComplete="off" spellCheck={false} /></div>
      <div className="connection-actions"><button type="submit" className="primary-button" aria-label="Save" disabled={busy}>{p("save")}</button><button type="button" className="secondary-button" disabled={busy} onClick={onClose}>{p("cancel")}</button></div>
    </form>
  </WorkspaceDialog>;
}
