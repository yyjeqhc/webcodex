import { useEffect, useRef, useState } from "react";
import { desktopApi } from "../lib/desktop-api";
import { desktopIdentity, readDesktopObservation } from "../lib/desktop-observation";
import { normalizeDesktopError } from "../i18n/presentation";
import { useProduct } from "../i18n/product";
import { useShellText } from "../i18n/runtime-shell";
import type { DesktopError, DesktopState } from "../models/topology";
import type { DiagnosticSnapshot } from "../models/runtime-shell";
import { WorkspaceDialog } from "../features/workspace/WorkspaceDialog";
import { DesktopErrorAlert } from "./DesktopErrorAlert";

interface Props {
  state: DesktopState;
  snapshot?: DiagnosticSnapshot;
  label?: string;
  copiedLabel?: string;
  disabled?: boolean;
}

// A new Runtime identity or credential fence always closes an old confirmation.
export function RuntimeCredentialCopy(props: Props) {
  const key = JSON.stringify([desktopIdentity(props.state), props.state.readiness.server, props.snapshot?.credential_copy_fence]);
  return <CredentialCopy key={key} {...props} />;
}

function CredentialCopy({ state, snapshot, label, copiedLabel, disabled = false }: Props) {
  const p = useProduct(); const s = useShellText();
  const [data, setData] = useState(snapshot ?? null);
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const alive = useRef(false);
  const submitting = useRef(false);
  const observation = useRef<AbortController | null>(null);
  const identity = desktopIdentity(state);
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; observation.current?.abort(); };
  }, []);
  const blocked = disabled || busy || Boolean(state.current_operation);
  const canCopy = Boolean(data?.resources.includes("runtime_console") && data.can_copy_console_credential && data.credential_copy_fence);
  const observe = async () => {
    observation.current?.abort();
    const request = new AbortController();
    observation.current = request;
    setBusy(true); setError(null); setData(null);
    try {
      const next = await readDesktopObservation(identity, request.signal, desktopApi.diagnostics);
      if (!request.signal.aborted && alive.current) setData(next);
    } catch (value) {
      if (!request.signal.aborted && alive.current) setError(normalizeDesktopError(value));
    } finally {
      if (!request.signal.aborted && alive.current) setBusy(false);
    }
  };
  const copy = async () => {
    if (blocked || submitting.current || !canCopy || !data?.credential_copy_fence) return;
    submitting.current = true; setBusy(true); setError(null);
    try {
      // The native command validates the exact Runtime/credential identity and
      // writes directly to the OS clipboard. Credential bytes never enter React.
      await desktopApi.copyRuntimeConsoleCredential(data.credential_copy_fence);
      if (alive.current) { setOpen(false); setCopied(true); }
    } catch (value) {
      if (alive.current) setError(normalizeDesktopError(value));
    } finally {
      submitting.current = false;
      if (alive.current) setBusy(false);
    }
  };
  return <>
    <button type="button" className="secondary-button" disabled={blocked} onClick={() => {
      setCopied(false); setError(null); setOpen(true);
      if (!snapshot) void observe();
    }}>{label ?? s("Copy Runtime Console credential")}</button>
    {copied && <p role="status">{copiedLabel ?? s("Copied")}</p>}
    {open && <WorkspaceDialog title={s("Sensitive clipboard action")} onClose={() => setOpen(false)} busy={blocked}>
      <p>{s("This copies the managed user credential to your clipboard. Do not share it; clipboard managers may retain it.")}</p>
      {!data && !error && <p role="status">{p("loading")}</p>}
      {data && !canCopy && <p role="status">{p("consoleCredentialUnavailable")}</p>}
      {error && <DesktopErrorAlert error={error} disabled={blocked} actions={{onRefresh: () => void observe(), onRetry: () => void observe()}} />}
      <div className="shell-actions">
        <button type="button" className="secondary-button" disabled={blocked} onClick={() => setOpen(false)}>{s("Cancel")}</button>
        <button type="button" className="primary-button" disabled={blocked || !canCopy} onClick={() => void copy()}>{s("Copy sensitive credential")}</button>
      </div>
    </WorkspaceDialog>}
  </>;
}
