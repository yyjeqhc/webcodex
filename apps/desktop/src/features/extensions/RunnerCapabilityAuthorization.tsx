import { useEffect, useRef, useState } from "react";
import { desktopApi } from "../../lib/desktop-api";
import type { RunnerSettings, SettingsTarget } from "../../models/topology";
import type { RunnerCapabilityAuthorizationSnapshot } from "../../models/runner-capabilities";
import { useProduct } from "../../i18n/product";
import { useRunnerCapabilitiesText } from "../../i18n/runner-capabilities";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

type Props = {
  settings: RunnerSettings | null;
  capability: "coding_agents" | "ssh_resources";
  disabled: boolean;
  refreshKey?: number;
  onAuthorized: () => void;
};

export function RunnerCapabilityAuthorization({ settings, ...props }: Props) {
  // Re-pair/connection changes discard a previously opened approval dialog.
  return settings ? <AuthorizationFlow key={JSON.stringify(settings.target)} target={settings.target} {...props} /> : null;
}

function AuthorizationFlow({ target, capability, disabled, refreshKey, onAuthorized }: Omit<Props, "settings"> & { target: SettingsTarget }) {
  const p = useProduct(); const r = useRunnerCapabilitiesText();
  const [status, setStatus] = useState<RunnerCapabilityAuthorizationSnapshot | null>(null);
  const [failed, setFailed] = useState(false); const [unavailable, setUnavailable] = useState(false);
  const [confirming, setConfirming] = useState(false); const [busy, setBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  const inFlight = useRef(false); const sequence = useRef(0); const mounted = useRef(true);
  const matches = (value: RunnerCapabilityAuthorizationSnapshot) => value.target.client_id === target.client_id
    && value.target.config_path === target.config_path && value.target.server_url === target.server_url;
  useEffect(() => {
    mounted.current = true; const current = ++sequence.current;
    setStatus(null); setUnavailable(false);
    // Read credential scopes directly. Do not list SSH resources or derive
    // authorization from advertised providers, saved profiles, or a restart.
    void Promise.resolve().then(() => desktopApi.runnerCapabilityAuthorization(target)).then(value => {
      if (!matches(value)) throw new Error("connection_changed");
      if (mounted.current && current === sequence.current) setStatus(value);
    }).catch(() => { if (mounted.current && current === sequence.current) setUnavailable(true); });
    return () => { mounted.current = false; ++sequence.current; };
  }, [target.client_id, target.config_path, target.server_url, revision, refreshKey]);

  const authorize = async () => {
    if (!confirming || !status?.can_authorize || inFlight.current || disabled) return;
    inFlight.current = true; setBusy(true); setConfirming(false); setFailed(false);
    const current = ++sequence.current;
    try {
      const value = await desktopApi.authorizeRunnerCapabilities(target);
      if (!matches(value)) throw new Error("connection_changed");
      if (!mounted.current || current !== sequence.current) return;
      setStatus(value);
      if (value[capability]) onAuthorized();
      else setFailed(true);
    } catch {
      if (!mounted.current || current !== sequence.current) return;
      setFailed(true); setStatus(null);
      // A lost IPC response can follow a committed grant. Observe only; never
      // repeat authorization automatically and never query SSH for this result.
      try {
        const value = await desktopApi.runnerCapabilityAuthorization(target);
        if (mounted.current && current === sequence.current && matches(value)) setStatus(value);
      } catch { /* explicit Refresh remains available */ }
    } finally { inFlight.current = false; if (mounted.current) setBusy(false); }
  };
  if (!failed && status?.[capability]) return null;
  return <div className="workspace-notice" aria-label="Runner Capability Authorization">
    {(unavailable || failed) && <p role="alert">{r(failed ? "authorizeFailed" : "authorizationUnavailable")}</p>}
    {status && !status[capability] && <>
      <p>{r(status.can_authorize ? "authorizationNeeded" : "operatorPairingNeeded")}</p>
      {status.can_authorize && <button type="button" className="secondary-button" aria-label="Authorize Runner Capabilities" disabled={disabled || busy} onClick={() => setConfirming(true)}>{r("authorize")}</button>}
    </>}
    {(unavailable || failed || status) && <button type="button" className="text-button" aria-label="Refresh Runner Authorization" disabled={disabled || busy} onClick={() => { setFailed(false); setRevision(value => value + 1); }}>{p("refresh")}</button>}
    {confirming && <WorkspaceDialog title={r("authorize")} onClose={() => setConfirming(false)} busy={busy}>
      <p>{r("authorizeHelp")}</p><div className="connection-actions">
        <button type="button" className="primary-button" aria-label="Confirm Authorize Runner Capabilities" disabled={disabled || busy} onClick={() => void authorize()}>{r("authorize")}</button>
        <button type="button" className="secondary-button" disabled={busy} onClick={() => setConfirming(false)}>{p("cancel")}</button>
      </div>
    </WorkspaceDialog>}
  </div>;
}
