import { useEffect, useRef, useState } from "react";
import { ExternalLink, Monitor } from "lucide-react";
import { desktopApi } from "../../lib/desktop-api";
import { useProduct } from "../../i18n/product";
import { normalizeDesktopError } from "../../i18n/presentation";
import type { DesktopError, DesktopState } from "../../models/topology";
import type { DiagnosticSnapshot } from "../../models/runtime-shell";
import { RuntimeCredentialCopy } from "../../components/RuntimeCredentialCopy";
import { desktopIdentity, readDesktopObservation } from "../../lib/desktop-observation";
import { DesktopErrorAlert } from "../../components/DesktopErrorAlert";

export function RuntimeConsolePanel({ state, onSettings }: { state: DesktopState; onSettings: () => void }) {
  const p = useProduct();
  const [data, setData] = useState<DiagnosticSnapshot | null>(null);
  const [error, setError] = useState<DesktopError | null>(null);
  const [busy, setBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  const alive = useRef(true);
  const observation = useRef(0);
  const identity = desktopIdentity(state);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => {
    const request = new AbortController();
    observation.current += 1;
    setData(null); setError(null);
    void readDesktopObservation(identity, request.signal, desktopApi.diagnostics).then(next => { if (!request.signal.aborted) setData(next); })
      .catch(value => { if (!request.signal.aborted) setError(normalizeDesktopError(value)); });
    return () => { request.abort(); };
  }, [revision, identity, state.readiness.server]);
  const disabled = busy || Boolean(state.current_operation);
  const available = Boolean(data?.resources.includes("runtime_console"));
  const canCopy = available && Boolean(data?.can_copy_console_credential && data.credential_copy_fence);
  const act = async (action: () => Promise<void>, onSuccess?: () => void) => {
    if (disabled) return;
    const observed = observation.current;
    setBusy(true); setError(null);
    try { await action(); if (alive.current && observation.current === observed) onSuccess?.(); }
    catch (value) { if (alive.current && observation.current === observed) setError(normalizeDesktopError(value)); }
    finally { if (alive.current) setBusy(false); }
  };
  const errorCard = error && (
    <DesktopErrorAlert
      error={error}
      disabled={disabled}
      actions={{
        onRefresh: () => setRevision(value => value + 1),
        onRetry: () => setRevision(value => value + 1),
        onOpenRuntime: onSettings,
      }}
    />
  );
  return <section className="page-section workspace-page console-page" data-webcodex-page="console" aria-labelledby="console-title">
    <header className="page-heading-row"><h1 id="console-title">{p("runtimeConsole")}</h1><button type="button" className="secondary-button" disabled={disabled} onClick={() => setRevision(value => value + 1)}>{p("refresh")}</button></header>
    <div className="console-launch">
      <Monitor size={32} aria-hidden="true" />
      <p>{p("consolePurpose")}</p>
      <button type="button" className="primary-button" disabled={disabled || !available || state.readiness.server !== "ready"} onClick={() => void act(() => desktopApi.openDiagnosticResource("runtime_console"))}>{p("consoleBrowser")}<ExternalLink size={16} aria-hidden="true" /></button>
    </div>
    {!data && !error && <p role="status">{p("loading")}</p>}
    {data && !available && <p className="workspace-notice">{p("consoleRemoteHelp")}</p>}
    {data && available && state.readiness.server !== "ready" && <div className="shell-notice warning"><p>{p("consoleServerHelp")}</p><button type="button" className="secondary-button" onClick={onSettings}>{p("runtimeAndServices")}</button></div>}
    {data && available && <div className="console-account">
      <h2>{p("consoleAccount")}</h2><p className="field-help">{p(canCopy ? "consoleAccountHelp" : "consoleCredentialUnavailable")}</p>
      {canCopy && <RuntimeCredentialCopy state={state} snapshot={data} disabled={disabled} />}
    </div>}
    {errorCard}
  </section>;
}
