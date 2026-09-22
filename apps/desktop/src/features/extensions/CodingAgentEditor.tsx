import { useRef, useState, type FormEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState, SettingsTarget } from "../../models/topology";
import type { AcpGlobalSettings, CodingAgentProfile, CodingAgentRequest } from "../../models/runner-capabilities";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { useRunnerCapabilitiesText } from "../../i18n/runner-capabilities";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

type Mapping = { row: number; child: string; runner: string };
function stringArray(input: string, limit: number): string[] {
  const parsed: unknown = JSON.parse(input);
  if (!Array.isArray(parsed) || parsed.length > 64 || parsed.some(value => typeof value !== "string" || value.length > limit || value.includes("\0"))) throw new Error("invalid_fields");
  return parsed as string[];
}
export function CodingAgentEditor({ profile, revision, target, globals, onState, onClose }: {
  profile: CodingAgentProfile | null; revision: number; target: SettingsTarget; globals: AcpGlobalSettings | null;
  onState: (state: DesktopState) => void; onClose: () => void;
}) {
  const p = useProduct(); const c = useConnectionsTools(); const r = useRunnerCapabilitiesText();
  const [name, setName] = useState(profile?.name ?? "");
  const [id, setId] = useState(profile?.provider_id ?? "");
  const [executable, setExecutable] = useState(profile?.executable ?? "");
  const [args, setArgs] = useState(JSON.stringify(profile?.args ?? []));
  const [enabled, setEnabled] = useState(profile?.enabled ?? true);
  const [options, setOptions] = useState(JSON.stringify(profile?.allowed_config_options ?? []));
  const [mapping, setMapping] = useState<Mapping[]>(() => Object.entries(profile?.env_from_env ?? {}).map(([child, runner], row) => ({ row, child, runner })));
  const [manageGlobals, setManageGlobals] = useState(globals !== null);
  const [concurrency, setConcurrency] = useState(globals?.max_concurrent_runs ?? 1);
  const [timeout, setTimeout] = useState(globals?.permission_timeout_secs ?? 5);
  const [busy, setBusy] = useState(false); const [error, setError] = useState<string | null>(null);
  const submitting = useRef(false); const nextRow = useRef(mapping.length);
  const updateMapping = (row: number, patch: Partial<Mapping>) => setMapping(fields => fields.map(field => field.row === row ? { ...field, ...patch } : field));
  const submit = async (event: FormEvent) => {
    event.preventDefault(); if (submitting.current) return;
    let request: CodingAgentRequest;
    try {
      const parsedArgs = stringArray(args, 4096); const allowed = stringArray(options, 128);
      if (!/^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$/.test(id.trim()) || new Set(allowed).size !== allowed.length || allowed.some(value => !value.trim())) throw new Error("invalid_fields");
      const seen = new Set<string>();
      const env = mapping.map(field => {
        const child = field.child.trim(); const runner = field.runner.trim();
        for (const variable of [child, runner]) {
          const upper = variable.toUpperCase();
          if (!/^[A-Za-z_][A-Za-z0-9_]{0,255}$/.test(variable) || upper.startsWith("WEBCODEX_") || upper === "AUTHORIZATION") throw new Error("invalid_fields");
        }
        if (seen.has(child.toUpperCase())) throw new Error("invalid_fields");
        seen.add(child.toUpperCase());
        return [child, runner];
      });
      request = { target, expected_revision: revision, previous_id: profile?.provider_id ?? null,
        profile: { provider_id: id.trim(), name: name.trim(), executable: executable.trim(), args: parsedArgs, enabled, env_from_env: Object.fromEntries(env), allowed_config_options: allowed },
        global_settings: manageGlobals ? { max_concurrent_runs: concurrency, permission_timeout_secs: timeout } : null };
    } catch { setError("invalid_fields"); return; }
    submitting.current = true; setBusy(true); setError(null);
    try { onState(await desktopApi.saveCodingAgent(request)); onClose(); }
    catch (failure) { setError(typeof failure === "object" && failure !== null && "code" in failure ? String(failure.code) : "operation_failed"); }
    finally { submitting.current = false; setBusy(false); }
  };
  const chooseExecutable = async () => {
    if (busy) return;
    try { const path = await open({ title: r("executable"), multiple: false, directory: false }); if (typeof path === "string") setExecutable(path); }
    catch { setError("operation_failed"); }
  };
  return <WorkspaceDialog title={profile ? `${p("edit")} ${profile.name}` : r("addCodingAgent")} onClose={onClose} busy={busy}>
    <form className="profile-editor" onSubmit={event => void submit(event)} aria-busy={busy}>
      <div className="field-group"><label htmlFor="coding-agent-name">{c("name")}</label><input id="coding-agent-name" aria-label="Name" value={name} onChange={event => setName(event.target.value)} maxLength={128} required disabled={busy} autoFocus /></div>
      <div className="field-group"><label htmlFor="coding-agent-id">{r("providerId")}</label><input id="coding-agent-id" aria-label="Provider ID" value={id} onChange={event => setId(event.target.value)} maxLength={64} required disabled={busy} spellCheck={false} placeholder="pi" /></div>
      <div className="field-group"><label htmlFor="coding-agent-executable">{r("executable")}</label><input id="coding-agent-executable" aria-label="Executable" value={executable} onChange={event => setExecutable(event.target.value)} maxLength={1024} required disabled={busy} spellCheck={false} /><button type="button" className="text-button" aria-label="Choose Executable" disabled={busy} onClick={() => void chooseExecutable()}>{p("open")}</button></div>
      <div className="field-group"><label htmlFor="coding-agent-arguments">{c("arguments")}</label><textarea id="coding-agent-arguments" aria-label="Arguments" value={args} onChange={event => setArgs(event.target.value)} maxLength={65536} rows={3} required disabled={busy} spellCheck={false} aria-describedby="coding-arguments-help" /><small id="coding-arguments-help">{r("agentArgsHelp")}</small></div>
      <label className="profile-checkbox" htmlFor="coding-agent-enabled"><input id="coding-agent-enabled" type="checkbox" checked={enabled} onChange={event => setEnabled(event.target.checked)} disabled={busy} />{c("enabled")}</label>
      <details><summary>{p("advanced")}</summary>
        <fieldset className="mcp-environment" disabled={busy}><legend>{r("envMapping")}</legend><p id="coding-env-help">{r("envHelp")}</p>
          {mapping.map(field => <div className="mcp-environment-row" key={field.row}>
            <div className="field-group"><label htmlFor={`coding-env-child-${field.row}`}>{r("childVariable")} {field.row + 1}</label><input id={`coding-env-child-${field.row}`} aria-label={`Child Environment Variable ${field.row + 1}`} value={field.child} onChange={event => updateMapping(field.row, { child: event.target.value })} maxLength={256} required spellCheck={false} aria-describedby="coding-env-help" /></div>
            <div className="field-group"><label htmlFor={`coding-env-runner-${field.row}`}>{r("runnerVariable")} {field.row + 1}</label><input id={`coding-env-runner-${field.row}`} aria-label={`Runner Environment Variable ${field.row + 1}`} value={field.runner} onChange={event => updateMapping(field.row, { runner: event.target.value })} maxLength={256} required spellCheck={false} autoComplete="off" /></div>
            <button type="button" className="text-button" aria-label={`Remove Environment Mapping ${field.row + 1}`} onClick={() => setMapping(fields => fields.filter(value => value.row !== field.row))}>{c("remove")}</button>
          </div>)}
          <button type="button" className="secondary-button" aria-label="Add Environment Mapping" disabled={mapping.length >= 64} onClick={() => { const row = nextRow.current++; setMapping(fields => [...fields, { row, child: "", runner: "" }]); }}>{r("addMapping")}</button>
        </fieldset>
        <div className="field-group"><label htmlFor="coding-agent-options">{r("allowedOptions")}</label><textarea id="coding-agent-options" value={options} onChange={event => setOptions(event.target.value)} rows={2} maxLength={16384} required disabled={busy} spellCheck={false} /></div>
        <label className="profile-checkbox" htmlFor="coding-global-settings"><input id="coding-global-settings" type="checkbox" checked={manageGlobals} onChange={event => setManageGlobals(event.target.checked)} disabled={busy} />{r("globalSettings")}</label>
        <p>{r("preserveGlobals")}</p>
        {manageGlobals && <div className="mcp-environment-row"><div className="field-group"><label htmlFor="coding-concurrency">{r("maxConcurrent")}</label><input id="coding-concurrency" type="number" min={1} max={8} value={concurrency} onChange={event => setConcurrency(event.target.valueAsNumber)} required disabled={busy} /></div><div className="field-group"><label htmlFor="coding-timeout">{r("permissionTimeout")}</label><input id="coding-timeout" type="number" min={1} max={60} value={timeout} onChange={event => setTimeout(event.target.valueAsNumber)} required disabled={busy} /></div></div>}
      </details>
      {error && <p role="alert" className="workspace-notice">{error === "coding_agent_ownership_conflict" ? r("ownershipConflict") : error === "invalid_fields" ? c("invalidFields") : c("operationFailed")}</p>}
      <div className="connection-actions"><button className="primary-button" type="submit" aria-label="Save" disabled={busy}>{p("save")}</button><button className="secondary-button" type="button" disabled={busy} onClick={onClose}>{p("cancel")}</button></div>
    </form>
  </WorkspaceDialog>;
}
