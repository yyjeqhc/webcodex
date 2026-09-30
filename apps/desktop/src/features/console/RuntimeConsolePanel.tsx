import { useEffect, useRef, useState } from "react";
import { ExternalLink, Monitor } from "lucide-react";
import { desktopApi } from "../../lib/desktop-api";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { useShellText } from "../../i18n/runtime-shell";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import type { DesktopError, DesktopState } from "../../models/topology";
import type { DiagnosticSnapshot } from "../../models/runtime-shell";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

export function RuntimeConsolePanel({ state, onSettings }: { state: DesktopState; onSettings: () => void }) {
  const p = useProduct(); const s = useShellText(); const { t } = useLocale();
  const [data, setData] = useState<DiagnosticSnapshot | null>(null);
  const [error, setError] = useState<DesktopError | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmation, setConfirmation] = useState(false);
  const [copied, setCopied] = useState(false);
  const [revision, setRevision] = useState(0);
  const alive = useRef(true);
  const observation = useRef(0);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => {
    let cancelled = false;
    observation.current += 1;
    setData(null); setError(null); setConfirmation(false); setCopied(false);
    void desktopApi.diagnostics().then(next => { if (!cancelled) setData(next); })
      .catch(value => { if (!cancelled) setError(normalizeDesktopError(value)); });
    return () => { cancelled = true; };
  }, [revision, state.workspace_runner?.server_url, state.persistent_environment, state.topology?.server.kind, state.topology?.server.kind === "remote" ? state.topology.server.url : null, state.readiness.server]);
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
  const presentation = error && desktopErrorPresentation(error, t);
  const errorCard = presentation && <div className="error-card" role="alert"><strong>{presentation.title}</strong><span>{presentation.action}</span><details><summary>{p("details")}</summary><code>{error?.code}</code></details></div>;
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
      {canCopy && <button type="button" className="secondary-button" disabled={disabled} onClick={() => setConfirmation(true)}>{s("Copy Runtime Console credential")}</button>}
      {copied && <p role="status">{s("Copied")}</p>}
    </div>}
    {!confirmation && errorCard}
    {confirmation && <WorkspaceDialog title={s("Sensitive clipboard action")} onClose={() => setConfirmation(false)} busy={disabled}>
      <p>{s("This copies the managed user credential to your clipboard. Do not share it; clipboard managers may retain it.")}</p>
      {errorCard}
      <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => setConfirmation(false)}>{s("Cancel")}</button><button type="button" className="primary-button" disabled={disabled || !canCopy} onClick={() => void act(async () => {
        if (!canCopy || !data?.credential_copy_fence) return;
        await desktopApi.copyRuntimeConsoleCredential(data.credential_copy_fence);
      }, () => { setConfirmation(false); setCopied(true); })}>{s("Copy sensitive credential")}</button></div>
    </WorkspaceDialog>}
  </section>;
}
