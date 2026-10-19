import { useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { save } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import { desktopIdentity, readDesktopObservation } from "../../lib/desktop-observation";
import type { DesktopError, DesktopState } from "../../models/topology";
import type { DiagnosticSnapshot, DiagnosticResource, TraceMode } from "../../models/runtime-shell";
import { useProduct } from "../../i18n/product";
import { useLocale } from "../../i18n/locale";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { useShellText } from "../../i18n/runtime-shell";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { ContinuationFacts } from "../activity/ContinuationFacts";

type DiagnosticsProps = { state: DesktopState; onState: (state: DesktopState) => void; onActivity?: () => void; onRuntime?: () => void; onConnection?: () => void };

export function DiagnosticsPanel(props: DiagnosticsProps) {
  return <DiagnosticsContents key={desktopIdentity(props.state)} {...props} />;
}

function DiagnosticsContents({ state, onState, onActivity, onRuntime, onConnection }: DiagnosticsProps) {
  const s = useShellText(); const p = useProduct(); const c = useConnectionsTools(); const { t } = useLocale();
  const [data, setData] = useState<DiagnosticSnapshot | null>(null);
  const [mode, setMode] = useState<TraceMode>("off");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [notice, setNotice] = useState("");
  const [credentialRecoveryOpen, setCredentialRecoveryOpen] = useState(false);
  const [userToken, setUserToken] = useState("");
  const userTokenInput = useRef<HTMLInputElement>(null);
  const [confirmation, setConfirmation] = useState<{ kind: "trace"; restart: boolean; jobs: number | null; mode: TraceMode; revision: string } | { kind: "restore"; fingerprint: string } | null>(null);
  const mounted = useRef(false);
  const readRequest = useRef<AbortController | null>(null);
  const actionRequest = useRef<AbortController | null>(null);
  const dirtyMode = useRef(false);
  const identity = desktopIdentity(state);
  const refresh = async () => {
    readRequest.current?.abort();
    const request = new AbortController();
    readRequest.current = request;
    setError(null); setConfirmation(null);
    try {
      const next = await readDesktopObservation(identity, request.signal, desktopApi.diagnostics);
      if (mounted.current && !request.signal.aborted) {
        setData(next);
        if (!dirtyMode.current) setMode(next.trace.configured_mode ?? "off");
      }
    } catch (value) {
      if (mounted.current && !request.signal.aborted) setError(normalizeDesktopError(value));
    } finally {
      if (readRequest.current === request) readRequest.current = null;
    }
  };
  useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => {
      mounted.current = false;
      readRequest.current?.abort();
      actionRequest.current?.abort();
    };
  }, []);
  const disabled = busy || Boolean(state.current_operation);
  const persistentEnvironment = state.persistent_environment?.trim() || null;
  const canTrace = Boolean(data?.trace.can_edit);
  const displayedMode = canTrace ? mode : data?.trace.effective_mode;
  const traceLabel = (value: TraceMode) => value === "metadata" ? p("traceMetadata") : value === "full" ? p("traceFull") : s("Off");
  const unavailableHelp = state.topology?.server.kind === "remote" ? "traceRemoteHelp"
    : data?.trace.error_code === "trace_system_service_read_only" ? "traceSystemHelp"
    : !state.topology ? "traceSetupHelp" : "traceUnavailableHelp";
  const act = async (action: (valid: () => boolean, signal: AbortSignal) => Promise<void>) => {
    if (disabled || !mounted.current || actionRequest.current) return;
    const request = new AbortController();
    actionRequest.current = request;
    // A preceding observation cannot overwrite a later explicit action.
    readRequest.current?.abort();
    const valid = () => mounted.current && !request.signal.aborted;
    setBusy(true); setError(null); setNotice("");
    try { await action(valid, request.signal); }
    catch (value) { if (valid()) setError(normalizeDesktopError(value)); }
    finally {
      if (actionRequest.current === request) actionRequest.current = null;
      if (valid()) setBusy(false);
    }
  };
  const saveTrace = async (restart: boolean, confirmed: boolean, valid: () => boolean, selection = { mode, revision: data?.trace.revision }) => {
    if (!data || !canTrace || !selection.revision || !valid()) return;
    const next = await desktopApi.setToolRequestTracing({ mode: selection.mode, expected_revision: selection.revision, confirm_full: selection.mode === "full" && confirmed, restart, confirm_interrupt: restart && confirmed });
    if (!valid()) return;
    dirtyMode.current = false;
    setData({ ...data, trace: next }); setMode(next.configured_mode ?? "off");
    setNotice(next.restart_required ? "Restart required" : "Saved"); setConfirmation(null);
    const nextState = await desktopApi.getState();
    if (valid()) onState(nextState);
  };
  const restoreServerUserCredential = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const token = userToken;
    const environmentId = persistentEnvironment;
    if (!environmentId || token.length === 0 || disabled || actionRequest.current) return;
    setUserToken("");
    if (userTokenInput.current) userTokenInput.current.value = "";
    void act(async valid => {
      const next = await desktopApi.repairEnvironmentUserCredential({ environmentId, userToken: token });
      if (!valid()) return;
      setCredentialRecoveryOpen(false);
      onState(next);
    });
  };
  const prepareTrace = (restart: boolean) => act(async (valid, signal) => {
    if (!data || !canTrace) return;
    const selection = { mode, revision: data.trace.revision };
    if (restart) {
      const runtime = await readDesktopObservation(identity, signal, desktopApi.runtimeSettings);
      if (valid()) setConfirmation({ kind: "trace", restart, jobs: runtime.active_jobs, ...selection });
    } else if (mode === "full") setConfirmation({ kind: "trace", restart, jobs: 0, ...selection });
    else await saveTrace(false, false, valid, selection);
  });
  const confirmAction = () => act(async valid => {
    if (!confirmation || !data) return;
    if (confirmation.kind === "trace") await saveTrace(confirmation.restart, true, valid, confirmation);
    else {
      const next = await desktopApi.restorePreviousConfiguration(confirmation.fingerprint);
      if (!valid()) return;
      setConfirmation(null);
      onState(next);
      if (valid()) await refresh();
    }
  });
  const resourceNames: [DiagnosticResource, string][] = [["app_data", "Open app data directory"], ["server_configuration", "Open Server configuration location"], ["trace_directory", "Open trace directory"], ["runtime_directory", "Open Runtime folder"]];
  const presentation = error && desktopErrorPresentation(error, t);
  const errorCard = presentation && <div className="error-card" role="alert"><strong>{presentation.title}</strong><span>{presentation.action}</span><details><summary>{p("details")}</summary><code>{error?.code}</code></details></div>;
  return <div className="diagnostics-panel" data-webcodex-panel="diagnostics">
    <div className="shell-section-heading"><h2>{p("resolveProblems")}</h2><button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(() => refresh())}>{s("Refresh")}</button></div>
    <div className="diagnostics-routes">
      {onConnection && <div><p>{p("tunnelTroubleshootingHelp")}</p><button type="button" className="secondary-button" onClick={onConnection}>{c("connections")}</button></div>}
      {onActivity && <div><p>{p("activityTroubleshootingHelp")}</p><button type="button" className="secondary-button" onClick={onActivity}>{p("activity")}</button></div>}
      {onRuntime && <div><p>{p("serviceRecoveryHelp")}</p><button type="button" className="secondary-button" onClick={onRuntime}>{p("runtimeAndServices")}</button></div>}
    </div>
    {!data && !error && <p role="status">{s("Loading…")}</p>}
    {data && <>
      {data.configuration.reason_code && <section className="shell-notice warning" role="alert">
        <h3>{s("Configuration could not be migrated")}</h3><code>{data.configuration.reason_code}</code>
        <button type="button" className="secondary-button" disabled={disabled || !data.configuration.backup_available || !data.configuration.primary_fingerprint} onClick={() => { if (data.configuration.primary_fingerprint) setConfirmation({ kind: "restore", fingerprint: data.configuration.primary_fingerprint }); }}>{s("Restore previous configuration")}</button>
      </section>}
      <div className="shell-subsection diagnostics-support"><h3>{p("supportHelp")}</h3><p>{p("supportPurpose")}</p>
        <p className="field-help">{s("The copied report excludes configuration contents. The support bundle adds an allowlisted nonsecret diagnostic configuration snapshot that may include private local paths; review it before sharing. Credentials, raw configuration and project code remain excluded.")}</p>
        <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async valid => { await desktopApi.copyDiagnosticReport(); if (valid()) setNotice("Copied"); })}>{s("Copy diagnostic report")}</button>
          <button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async valid => {
            const path = await save({ title: s("Export support bundle"), defaultPath: "webcodex-support.zip", filters: [{ name: "ZIP", extensions: ["zip"] }] });
            if (!path || !valid()) return;
            await desktopApi.exportSupportBundle(path); if (valid()) setNotice("Support bundle exported");
          })}>{s("Export support bundle")}</button></div>
      </div>
      <section className="shell-subsection" aria-labelledby="desktop-tracing-title">
        <h3 id="desktop-tracing-title">{s("Tool request tracing")}</h3>
        <p className="field-help">{p("tracePurpose")}</p>
        <dl className="runtime-facts">
          <div><dt>{p("traceCurrentMode")}</dt><dd>{data.trace.effective_mode ? traceLabel(data.trace.effective_mode) : p("traceUnconfirmed")}</dd></div>
          <div><dt>{p("traceSavedMode")}</dt><dd>{data.trace.configured_mode ? traceLabel(data.trace.configured_mode) : p("traceUnconfirmed")}</dd></div>
        </dl>
        <p className="field-help">{data.trace.effective_mode ? p("traceCurrentHelp") : p("traceUnconfirmedHelp")}</p>
        <label htmlFor="desktop-trace-mode">{s("Tool request tracing")}</label>
        <select id="desktop-trace-mode" value={displayedMode ?? "unknown"} onChange={event => { dirtyMode.current = true; setMode(event.target.value as TraceMode); setConfirmation(null); }} disabled={disabled || !canTrace} aria-describedby={canTrace ? "desktop-trace-mode-help" : "desktop-trace-mode-help desktop-trace-authority-help"}>
          {!canTrace && !data.trace.effective_mode && <option value="unknown">{p("traceUnconfirmed")}</option>}
          <option value="off">{s("Off")}</option><option value="metadata">{p("traceMetadata")}</option><option value="full">{p("traceFull")}</option>
        </select>
        <p id="desktop-trace-mode-help" className="field-help">{!displayedMode ? p("traceUnconfirmedHelp") : displayedMode === "off" ? p("traceOffHelp") : s(displayedMode === "full" ? "Full tracing may contain sensitive tool inputs and results. Enable it only temporarily." : "Metadata records lifecycle and correlation, not full tool arguments or results.")}</p>
        {data.trace.restart_required && data.trace.effective_mode && <p role="status">{p("tracePendingHelp")}</p>}
        {canTrace ? <>
        <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => void prepareTrace(false)}>{s("Save")}</button>
          {data.trace.can_restart && <button type="button" className="primary-button" disabled={disabled} onClick={() => void prepareTrace(true)}>{p("traceSaveRestart")}</button>}</div>
        <p className="field-help">{p("traceRestartHelp")}</p>
        </> : <div className="workspace-notice" id="desktop-trace-authority-help"><p>{p(unavailableHelp)}</p>{onRuntime && state.topology?.server.kind !== "remote" && <button type="button" className="secondary-button" onClick={onRuntime}>{p("runtimeAndServices")}</button>}</div>}
        {!canTrace && <div className="shell-actions">
          {data.resources.includes("server_configuration") && <button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(() => desktopApi.openDiagnosticResource("server_configuration"))}>{s("Open Server configuration location")}</button>}
          <code>WEBCODEX_TOOL_REQUEST_TRACE=metadata</code>
          <button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async valid => { await writeText("WEBCODEX_TOOL_REQUEST_TRACE=metadata"); if (valid()) setNotice("Copied"); })}>{p("traceCopyMetadata")}</button>
        </div>}
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
      <div className="shell-subsection"><h3>{p("diagnosticFiles")}</h3><p className="field-help">{p("diagnosticFilesHelp")}</p><div className="shell-actions">{resourceNames.filter(([kind]) => data.resources.includes(kind) && (kind !== "server_configuration" || canTrace)).map(([kind, label]) => <button type="button" key={kind} className="text-button" disabled={disabled} onClick={() => void act(() => desktopApi.openDiagnosticResource(kind))}>{s(label)}</button>)}</div></div>
    </>}
    {notice && <p role="status">{s(notice)}</p>}{busy && <p role="status">{s("Loading…")}</p>}
    {!confirmation && errorCard}
    {confirmation && <WorkspaceDialog title={s(confirmation.kind === "trace" ? confirmation.mode === "full" ? "Confirm full tracing" : "Runtime restart warning" : "Restore configuration")} onClose={() => setConfirmation(null)} busy={disabled}>
      {confirmation.kind === "trace" ? <>
        {confirmation.mode === "full" && <p>{s("Full tracing may contain sensitive tool inputs and results. Enable it only temporarily.")}</p>}
        {confirmation.restart && <><p>{s("Server restart may interrupt in-flight requests; the existing Runner will reconnect.")}</p><p>{confirmation.jobs == null ? s("Job count is not confirmed.") : s("Active jobs: {count}").replace("{count}", String(confirmation.jobs))}</p></>}
      </> : <p>{s("Restore the previous known-good configuration without deleting Projects or credentials. Runtime will remain stopped until explicitly started.")}</p>}
      {errorCard}
      <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => setConfirmation(null)}>{s("Cancel")}</button><button type="button" className="primary-button" disabled={disabled} onClick={() => void confirmAction()}>{confirmation.kind === "trace" && confirmation.restart ? p("traceSaveRestart") : s(confirmation.kind === "restore" ? "Restore previous configuration" : "Confirm full tracing")}</button></div>
    </WorkspaceDialog>}
  </div>;
}
