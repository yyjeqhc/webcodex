import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";
import type { DiagnosticSnapshot, DiagnosticResource, TraceMode } from "../../models/runtime-shell";
import { useProduct } from "../../i18n/product";
import { useLocale } from "../../i18n/locale";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { useShellText } from "../../i18n/runtime-shell";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { ContinuationFacts } from "../activity/ContinuationFacts";

export function DiagnosticsPanel({ state, onState, onActivity, onRuntime, onConnection }: { state: DesktopState; onState: (state: DesktopState) => void; onActivity?: () => void; onRuntime?: () => void; onConnection?: () => void }) {
  const s = useShellText(); const p = useProduct(); const c = useConnectionsTools(); const { t } = useLocale();
  const [data, setData] = useState<DiagnosticSnapshot | null>(null);
  const [mode, setMode] = useState<TraceMode>("off");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [notice, setNotice] = useState("");
  const [credentialRecoveryOpen, setCredentialRecoveryOpen] = useState(false);
  const [userToken, setUserToken] = useState("");
  const userTokenInput = useRef<HTMLInputElement>(null);
  const [confirmation, setConfirmation] = useState<{ kind: "trace"; restart: boolean; jobs: number | null } | { kind: "restore" } | null>(null);
  const alive = useRef(true);
  const install = (next: DiagnosticSnapshot) => { if (alive.current) { setData(next); setMode(next.trace.mode); } };
  useEffect(() => {
    alive.current = true;
    void Promise.resolve().then(() => desktopApi.diagnostics()).then(install).catch(value => { if (alive.current) setError(normalizeDesktopError(value)); });
    return () => { alive.current = false; };
  }, []);
  const disabled = busy || Boolean(state.current_operation);
  const persistentEnvironment = state.persistent_environment?.trim() || null;
  const canTrace = Boolean(data?.trace.available);
  const traceLabel = (value: TraceMode) => value === "metadata" ? p("traceMetadata") : value === "full" ? p("traceFull") : s("Off");
  const unavailableHelp = state.topology?.server.kind === "remote" ? "traceRemoteHelp"
    : data?.trace.error_code === "trace_system_service_read_only" ? "traceSystemHelp"
    : !state.topology ? "traceSetupHelp" : "traceUnavailableHelp";
  const act = async (action: () => Promise<void>) => {
    if (disabled) return;
    setBusy(true); setError(null); setNotice("");
    try { await action(); } catch (value) { if (alive.current) setError(normalizeDesktopError(value)); }
    finally { if (alive.current) setBusy(false); }
  };
  const saveTrace = async (restart: boolean, confirmed: boolean) => {
    if (!data || !canTrace) return;
    const next = await desktopApi.setToolRequestTracing({ mode, expected_revision: data.trace.revision, confirm_full: mode === "full" && confirmed, restart, confirm_interrupt: restart && confirmed });
    if (alive.current) { setData({ ...data, trace: next }); setNotice(next.restart_required ? "Restart required" : "Saved"); setConfirmation(null); }
    onState(await desktopApi.getState());
  };
  const restoreServerUserCredential = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const token = userToken;
    setUserToken("");
    if (userTokenInput.current) userTokenInput.current.value = "";
    void act(async () => {
      if (!persistentEnvironment || token.length === 0) return;
      const next = await desktopApi.repairEnvironmentUserCredential({ environmentId: persistentEnvironment, userToken: token });
      onState(next);
      if (alive.current) setCredentialRecoveryOpen(false);
    });
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
    else if (confirmation.kind === "restore" && data.configuration.primary_fingerprint) {
      onState(await desktopApi.restorePreviousConfiguration(data.configuration.primary_fingerprint));
      if (alive.current) setConfirmation(null);
      install(await desktopApi.diagnostics());
    }
  });
  const resourceNames: [DiagnosticResource, string][] = [["app_data", "Open app data directory"], ["server_configuration", "Open Server configuration location"], ["trace_directory", "Open trace directory"], ["runtime_directory", "Open Runtime folder"]];
  const presentation = error && desktopErrorPresentation(error, t);
  const errorCard = presentation && <div className="error-card" role="alert"><strong>{presentation.title}</strong><span>{presentation.action}</span><details><summary>{p("details")}</summary><code>{error?.code}</code></details></div>;
  return <div className="diagnostics-panel" data-webcodex-panel="diagnostics">
    <div className="shell-section-heading"><h2>{p("resolveProblems")}</h2><button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async () => install(await desktopApi.diagnostics()))}>{s("Refresh")}</button></div>
    <div className="diagnostics-routes">
      {onConnection && <div><p>{p("tunnelTroubleshootingHelp")}</p><button type="button" className="secondary-button" onClick={onConnection}>{c("connections")}</button></div>}
      {onActivity && <div><p>{p("activityTroubleshootingHelp")}</p><button type="button" className="secondary-button" onClick={onActivity}>{p("activity")}</button></div>}
      {onRuntime && <div><p>{p("serviceRecoveryHelp")}</p><button type="button" className="secondary-button" onClick={onRuntime}>{p("runtimeAndServices")}</button></div>}
    </div>
    {!data && !error && <p role="status">{s("Loading…")}</p>}
    {data && <>
      {data.configuration.reason_code && <section className="shell-notice warning" role="alert">
        <h3>{s("Configuration could not be migrated")}</h3><code>{data.configuration.reason_code}</code>
        <button type="button" className="secondary-button" disabled={disabled || !data.configuration.backup_available || !data.configuration.primary_fingerprint} onClick={() => setConfirmation({ kind: "restore" })}>{s("Restore previous configuration")}</button>
      </section>}
      <div className="shell-subsection diagnostics-support"><h3>{p("supportHelp")}</h3><p>{p("supportPurpose")}</p>
        <p className="field-help">{s("The report and support bundle exclude project code, credentials, configuration contents and full request/result payloads.")}</p>
        <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async () => { await desktopApi.copyDiagnosticReport(); if (alive.current) setNotice("Copied"); })}>{s("Copy Diagnostic Report")}</button>
          <button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async () => {
            const path = await save({ title: s("Export Support Bundle"), defaultPath: "webcodex-support.zip", filters: [{ name: "ZIP", extensions: ["zip"] }] });
            if (!path || !alive.current) return;
            await desktopApi.exportSupportBundle(path); if (alive.current) setNotice("Support bundle exported");
          })}>{s("Export Support Bundle")}</button></div>
      </div>
      <section className="shell-subsection" aria-labelledby="desktop-tracing-title">
        <h3 id="desktop-tracing-title">{s("Tool Request Tracing")}</h3>
        <p className="field-help">{p("tracePurpose")}</p>
        <dl className="runtime-facts">
          <div><dt>{p("traceCurrentMode")}</dt><dd>{data.trace.effective_mode ? traceLabel(data.trace.effective_mode) : p("traceUnconfirmed")}</dd></div>
          {canTrace && <div><dt>{p("traceSavedMode")}</dt><dd>{traceLabel(data.trace.mode)}</dd></div>}
        </dl>
        <p className="field-help">{data.trace.effective_mode ? p("traceCurrentHelp") : p("traceUnconfirmedHelp")}</p>
        {canTrace ? <>
        <label htmlFor="desktop-trace-mode">{s("Tool Request Tracing")}</label>
        <select id="desktop-trace-mode" value={mode} onChange={event => setMode(event.target.value as TraceMode)} disabled={disabled} aria-describedby="desktop-trace-mode-help">
          <option value="off">{s("Off")}</option><option value="metadata">{p("traceMetadata")}</option><option value="full">{p("traceFull")}</option>
        </select>
        <p id="desktop-trace-mode-help" className="field-help">{mode === "off" ? p("traceOffHelp") : s(mode === "full" ? "Full tracing may contain sensitive tool inputs and results. Enable it only temporarily." : "Metadata records lifecycle and correlation, not full tool arguments or results.")}</p>
        {data.trace.restart_required && data.trace.effective_mode && <p role="status">{p("tracePendingHelp")}</p>}
        <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => void prepareTrace(false)}>{s("Save")}</button>
          {data.trace.can_restart && <button type="button" className="primary-button" disabled={disabled} onClick={() => void prepareTrace(true)}>{p("traceSaveRestart")}</button>}</div>
        <p className="field-help">{p("traceRestartHelp")}</p>
        </> : <div className="workspace-notice"><p>{p(unavailableHelp)}</p>{onRuntime && state.topology?.server.kind !== "remote" && <button type="button" className="secondary-button" onClick={onRuntime}>{p("runtimeAndServices")}</button>}</div>}
        {data.trace.error_code && <details className="workspace-technical"><summary>{p("details")}</summary><code>{data.trace.error_code}</code></details>}
      </section>
      {persistentEnvironment && <div className="shell-subsection">
        <h3>{p("accountRecovery")}</h3><p className="field-help">{p("accountRecoveryHelp")}</p>
        <div className="credential-recovery">
          {!credentialRecoveryOpen ? <button type="button" className="secondary-button" disabled={disabled} onClick={() => setCredentialRecoveryOpen(true)}>{s("Restore Server user credential")}</button> : <form onSubmit={restoreServerUserCredential}>
            <label htmlFor="server-user-api-token">{s("Existing Server user API token")}</label>
            <input ref={userTokenInput} id="server-user-api-token" type="password" autoComplete="current-password" value={userToken} onChange={event => setUserToken(event.target.value)} disabled={disabled} />
            <div className="shell-actions">
              <button type="submit" className="primary-button" disabled={disabled || userToken.length === 0}>{s("Save credential")}</button>
              <button type="button" className="secondary-button" disabled={disabled} onClick={() => { setUserToken(""); if (userTokenInput.current) userTokenInput.current.value = ""; setCredentialRecoveryOpen(false); }}>{s("Cancel")}</button>
            </div>
          </form>}
        </div>
      </div>}
      <details className="workspace-technical"><summary>{p("callDetails")}</summary><ContinuationFacts value={data.report.last_webcodex_call} observedAt={data.observed_at_ms} /></details>
      <div className="shell-subsection"><h3>{p("diagnosticFiles")}</h3><p className="field-help">{p("diagnosticFilesHelp")}</p><div className="shell-actions">{resourceNames.filter(([kind]) => data.resources.includes(kind)).map(([kind, label]) => <button type="button" key={kind} className="text-button" disabled={disabled} onClick={() => void act(() => desktopApi.openDiagnosticResource(kind))}>{s(label)}</button>)}</div></div>
    </>}
    {notice && <p role="status">{s(notice)}</p>}{busy && <p role="status">{s("Loading…")}</p>}
    {!confirmation && errorCard}
    {confirmation && <WorkspaceDialog title={s(confirmation.kind === "trace" ? mode === "full" ? "Confirm full tracing" : "Runtime restart warning" : "Restore configuration")} onClose={() => setConfirmation(null)} busy={disabled}>
      {confirmation.kind === "trace" ? <>
        {mode === "full" && <p>{s("Full tracing may contain sensitive tool inputs and results. Enable it only temporarily.")}</p>}
        {confirmation.restart && <><p>{s("Server restart may interrupt in-flight requests; the existing Runner will reconnect.")}</p><p>{confirmation.jobs == null ? s("Job count is not confirmed.") : `${s("Active Jobs")}: ${confirmation.jobs}`}</p></>}
      </> : <p>{s("Restore the previous known-good configuration without deleting Projects or credentials. Runtime will remain stopped until explicitly started.")}</p>}
      {errorCard}
      <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => setConfirmation(null)}>{s("Cancel")}</button><button type="button" className="primary-button" disabled={disabled} onClick={() => void confirmAction()}>{confirmation.kind === "trace" && confirmation.restart ? p("traceSaveRestart") : s(confirmation.kind === "restore" ? "Restore previous configuration" : "Confirm full tracing")}</button></div>
    </WorkspaceDialog>}
  </div>;
}
