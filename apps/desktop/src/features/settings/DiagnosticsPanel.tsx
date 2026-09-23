import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState } from "../../models/topology";
import type { DiagnosticSnapshot, DiagnosticResource, TraceMode } from "../../models/runtime-shell";
import { useShellText } from "../../i18n/runtime-shell";
import { normalizeDesktopError } from "../../i18n/presentation";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { ContinuationFacts } from "../activity/ContinuationFacts";

export function DiagnosticsPanel({ state, onState }: { state: DesktopState; onState: (state: DesktopState) => void }) {
  const s = useShellText();
  const [data, setData] = useState<DiagnosticSnapshot | null>(null);
  const [mode, setMode] = useState<TraceMode>("off");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const [confirmation, setConfirmation] = useState<{ kind: "trace"; restart: boolean; jobs: number | null } | { kind: "credential" } | { kind: "restore" } | null>(null);
  const alive = useRef(true);
  const install = (next: DiagnosticSnapshot) => { if (alive.current) { setData(next); setMode(next.trace.mode); } };
  useEffect(() => {
    alive.current = true;
    void Promise.resolve().then(() => desktopApi.diagnostics()).then(install).catch(value => { if (alive.current) setError(normalizeDesktopError(value).code); });
    return () => { alive.current = false; };
  }, []);
  const disabled = busy || Boolean(state.current_operation);
  const act = async (action: () => Promise<void>) => {
    if (disabled) return;
    setBusy(true); setError(null); setNotice("");
    try { await action(); } catch (value) { if (alive.current) setError(normalizeDesktopError(value).code); }
    finally { if (alive.current) setBusy(false); }
  };
  const saveTrace = async (restart: boolean, confirmed: boolean) => {
    if (!data) return;
    const next = await desktopApi.setToolRequestTracing({ mode, expected_revision: data.trace.revision, confirm_full: mode === "full" && confirmed, restart, confirm_interrupt: restart && confirmed });
    if (alive.current) { setData({ ...data, trace: next }); setNotice(next.restart_required ? "Restart required" : "Saved"); setConfirmation(null); }
    onState(await desktopApi.getState());
  };
  const prepareTrace = (restart: boolean) => act(async () => {
    if (restart) {
      const runtime = await desktopApi.runtimeSettings();
      if (alive.current) setConfirmation({ kind: "trace", restart, jobs: runtime.active_jobs });
    } else if (mode === "full") setConfirmation({ kind: "trace", restart, jobs: 0 });
    else await saveTrace(false, false);
  });
  const confirmAction = () => act(async () => {
    if (!confirmation || !data) return;
    if (confirmation.kind === "trace") await saveTrace(confirmation.restart, true);
    else if (confirmation.kind === "credential" && data.credential_copy_fence) {
      await desktopApi.copyRuntimeConsoleCredential(data.credential_copy_fence);
      if (alive.current) { setNotice("Copied"); setConfirmation(null); }
    } else if (confirmation.kind === "restore" && data.configuration.primary_fingerprint) {
      onState(await desktopApi.restorePreviousConfiguration(data.configuration.primary_fingerprint));
      if (alive.current) setConfirmation(null);
      install(await desktopApi.diagnostics());
    }
  });
  const resourceNames: [DiagnosticResource, string][] = [["app_data", "Open app data directory"], ["server_configuration", "Open Server configuration location"], ["trace_directory", "Open trace directory"], ["runtime_directory", "Open Runtime folder"]];
  return <div className="diagnostics-panel" data-webcodex-panel="diagnostics">
    <div className="shell-section-heading"><h2>{s("Diagnostics Center")}</h2><button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async () => install(await desktopApi.diagnostics()))}>{s("Refresh")}</button></div>
    {!data && !error && <p role="status">{s("Loading…")}</p>}
    {data && <>
      {data.configuration.reason_code && <section className="shell-notice warning" role="alert">
        <h3>{s("Configuration could not be migrated")}</h3><code>{data.configuration.reason_code}</code>
        <button type="button" className="secondary-button" disabled={disabled || !data.configuration.backup_available || !data.configuration.primary_fingerprint} onClick={() => setConfirmation({ kind: "restore" })}>{s("Restore previous configuration")}</button>
      </section>}
      <div className="shell-subsection">
        <h3>{s("Tool Request Tracing")}</h3>
        <label htmlFor="desktop-trace-mode">{s("Tool Request Tracing")}</label>
        <select id="desktop-trace-mode" value={mode} onChange={event => setMode(event.target.value as TraceMode)} disabled={disabled || !data.trace.available}>
          <option value="off">{s("Off")}</option><option value="metadata">{s("Metadata")}</option><option value="full">{s("Full")}</option>
        </select>
        <p className="field-help">{s(mode === "full" ? "Full tracing may contain sensitive tool inputs and results. Enable it only temporarily." : "Metadata records lifecycle and correlation, not full tool arguments or results.")}</p>
        <p>{s("Effective mode")}: {s(data.trace.effective_mode === "metadata" ? "Metadata" : data.trace.effective_mode === "full" ? "Full" : data.trace.effective_mode === "off" ? "Off" : "Unknown")}</p>
        {data.trace.restart_required && <p role="status">{s("Restart required")}</p>}
        {data.trace.error_code && <code>{data.trace.error_code}</code>}
        <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled || !data.trace.available} onClick={() => void prepareTrace(false)}>{s("Save")}</button>
          <button type="button" className="primary-button" disabled={disabled || !data.trace.available || !data.trace.can_restart} onClick={() => void prepareTrace(true)}>{s("Save & Restart Runtime")}</button></div>
      </div>
      <div className="shell-subsection">
        <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled || !data.resources.includes("runtime_console")} onClick={() => void act(() => desktopApi.openDiagnosticResource("runtime_console"))}>{s("Open Runtime Console")}</button>
          <button type="button" className="text-button" disabled={disabled || !data.can_copy_console_credential} onClick={() => setConfirmation({ kind: "credential" })}>{s("Copy Runtime Console credential")}</button></div>
      </div>
      <div className="shell-subsection">
        <p className="field-help">{s("The report and support bundle exclude project code, credentials, configuration contents and full request/result payloads.")}</p>
        <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async () => { await desktopApi.copyDiagnosticReport(); if (alive.current) setNotice("Copied"); })}>{s("Copy Diagnostic Report")}</button>
          <button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async () => {
            const path = await save({ title: s("Export Support Bundle"), defaultPath: "webcodex-support.zip", filters: [{ name: "ZIP", extensions: ["zip"] }] });
            if (!path || !alive.current) return;
            await desktopApi.exportSupportBundle(path); if (alive.current) setNotice("Support bundle exported");
          })}>{s("Export Support Bundle")}</button></div>
      </div>
      <div className="shell-subsection"><h3>{s("Latest observed Window")}</h3><ContinuationFacts value={data.report.last_webcodex_call} observedAt={data.observed_at_ms} /></div>
      <div className="shell-actions">{resourceNames.filter(([kind]) => data.resources.includes(kind)).map(([kind, label]) => <button type="button" key={kind} className="text-button" disabled={disabled} onClick={() => void act(() => desktopApi.openDiagnosticResource(kind))}>{s(label)}</button>)}</div>
    </>}
    {notice && <p role="status">{s(notice)}</p>}{busy && <p role="status">{s("Loading…")}</p>}
    {error && <p className="shell-notice warning" role="alert"><code>{error}</code><span>{s("Refresh")}</span></p>}
    {confirmation && <WorkspaceDialog title={s(confirmation.kind === "trace" ? mode === "full" ? "Confirm full tracing" : "Runtime restart warning" : confirmation.kind === "credential" ? "Sensitive clipboard action" : "Restore configuration")} onClose={() => setConfirmation(null)} busy={disabled}>
      {confirmation.kind === "trace" ? <>
        {mode === "full" && <p>{s("Full tracing may contain sensitive tool inputs and results. Enable it only temporarily.")}</p>}
        {confirmation.restart && <><p>{s("Server restart may interrupt in-flight requests; the existing Runner will reconnect.")}</p><p>{confirmation.jobs == null ? s("Job count is not confirmed.") : `${s("Active Jobs")}: ${confirmation.jobs}`}</p></>}
      </> : <p>{s(confirmation.kind === "credential" ? "This copies the managed user credential to your clipboard. Do not share it; clipboard managers may retain it." : "Restore the previous known-good configuration without deleting Projects or credentials. Runtime will remain stopped until explicitly started.")}</p>}
      <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => setConfirmation(null)}>{s("Cancel")}</button><button type="button" className="primary-button" disabled={disabled} onClick={() => void confirmAction()}>{s(confirmation.kind === "credential" ? "Copy sensitive credential" : confirmation.kind === "restore" ? "Restore previous configuration" : confirmation.restart ? "Save & Restart Runtime" : "Confirm full tracing")}</button></div>
    </WorkspaceDialog>}
  </div>;
}
