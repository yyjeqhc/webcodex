import { useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useDeviceInvitationText } from "../../i18n/device-invitation";
import { normalizeDesktopError } from "../../i18n/presentation";
import { desktopApi } from "../../lib/desktop-api";
import { runnerServerAddress } from "../../lib/runner-server-address";
import type { DesktopState } from "../../models/topology";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

type InvitationView = { phase: "idle" | "creating" | "failed" | "denied" | "expired" }
  | { phase: "created"; code: string; requestedAt: number };
const DISPLAY_PERIOD_MS = 10 * 60 * 1000;

/** Discoverability only; NativeEnvironment and the Server decide permission. */
export function AddDevice({ state, onViewDevices }: { state: DesktopState; onViewDevices: () => void }) {
  const text = useDeviceInvitationText();
  const [openedFor, setOpenedFor] = useState<string | null>(null);
  const localServer = state.environment_setup
    ? state.environment_setup.configured && state.environment_setup.mode === "create"
    : state.topology?.experience === "full" && state.topology.server.kind === "local";
  const target = JSON.stringify([state.persistent_environment, state.environment_setup?.server_url, state.topology]);
  useEffect(() => { setOpenedFor(null); }, [target]);
  if (!localServer || !state.persistent_environment) return null;
  const savedUrl = state.environment_setup?.server_url ?? state.workspace_runner?.server_url ?? "";
  return <div className="workspace-notice">
    <p>{text("intro")}</p>
    <button type="button" className="secondary-button" onClick={() => setOpenedFor(target)}>{text("add")}</button>
    {openedFor === target && <DeviceInvitationDialog key={target} environmentId={state.persistent_environment}
      savedUrl={savedUrl} operationBusy={Boolean(state.current_operation)} onClose={() => setOpenedFor(null)}
      onViewDevices={() => { setOpenedFor(null); onViewDevices(); }} />}
  </div>;
}

function DeviceInvitationDialog({ environmentId, savedUrl, operationBusy, onClose, onViewDevices }: {
  environmentId: string; savedUrl: string; operationBusy: boolean; onClose: () => void; onViewDevices: () => void;
}) {
  const text = useDeviceInvitationText();
  const [url, setUrl] = useState(savedUrl);
  const [view, setView] = useState<InvitationView>({ phase: "idle" });
  const [visible, setVisible] = useState(false);
  const [copyStatus, setCopyStatus] = useState<"copied" | "copyFailed" | null>(null);
  const alive = useRef(true);
  const inFlight = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => {
    if (view.phase !== "created") return;
    // Core returns no expiry instant. Conservatively stop displaying ten minutes
    // after requesting; do not invent a Server countdown or redemption status.
    const expire = () => {
      if (performance.now() - view.requestedAt >= DISPLAY_PERIOD_MS) {
        setView({ phase: "expired" }); setVisible(false); setCopyStatus(null);
      }
    };
    const timer = window.setTimeout(expire, Math.max(0, DISPLAY_PERIOD_MS - (performance.now() - view.requestedAt)));
    document.addEventListener("visibilitychange", expire);
    return () => { window.clearTimeout(timer); document.removeEventListener("visibilitychange", expire); };
  }, [view]);
  const address = runnerServerAddress(url);
  const creating = view.phase === "creating";
  const create = async () => {
    if (inFlight.current || operationBusy || address.issue || view.phase === "created") return;
    inFlight.current = true; setView({ phase: "creating" }); setVisible(false); setCopyStatus(null);
    const requestedAt = performance.now();
    try {
      // The advertised URL is never an authority selector or credential target.
      const invitation = await desktopApi.createEnvironmentInvitation(environmentId);
      if (!alive.current) return;
      if (invitation.environmentId !== environmentId || !invitation.pairingCode) {
        setView({ phase: "failed" });
      } else if (performance.now() - requestedAt >= DISPLAY_PERIOD_MS) {
        setView({ phase: "expired" });
      } else {
        setView({ phase: "created", code: invitation.pairingCode, requestedAt });
      }
    } catch (value) {
      if (alive.current) setView({ phase: ["permission_denied", "authentication_required", "server_admin_required"]
        .includes(normalizeDesktopError(value).code) ? "denied" : "failed" });
    } finally { inFlight.current = false; }
  };
  const validCode = () => {
    if (view.phase !== "created") return null;
    if (performance.now() - view.requestedAt >= DISPLAY_PERIOD_MS) {
      setView({ phase: "expired" }); setVisible(false); setCopyStatus(null); return null;
    }
    return view.code;
  };
  const copy = async (value: string | null) => {
    if (value === null) return;
    try { await writeText(value); if (alive.current) setCopyStatus("copied"); }
    catch { if (alive.current) setCopyStatus("copyFailed"); }
  };
  return <WorkspaceDialog title={text("add")} onClose={onClose}>
    <form className="device-invitation" onSubmit={event => { event.preventDefault(); void create(); }}>
      <p>{text("intro")}</p>
      <div className="field-group">
        <label htmlFor="device-server-url">{text("address")}</label>
        <input id="device-server-url" type="url" value={url} required placeholder="https://webcodex.example.com"
          readOnly={creating} onChange={event => { setUrl(event.currentTarget.value); setCopyStatus(null); }}
          aria-describedby="device-network-help device-address-help" data-autofocus />
        <p id="device-network-help" className="field-help">{text("network")}</p>
        {address.issue && <p id="device-address-help" className="field-help" role="alert">{text(address.issue)}</p>}
      </div>
      <p role={view.phase === "failed" || view.phase === "denied" ? "alert" : "status"} aria-live="polite">{text(view.phase)}</p>
      {view.phase === "created" && <>
        <div className="field-group">
          <label htmlFor="device-pairing-code">{text("code")}</label>
          <input id="device-pairing-code" type={visible ? "text" : "password"} value={view.code} readOnly autoComplete="off" spellCheck={false} />
        </div>
        <p className="field-help">{text("validity")}</p>
        <div className="shell-actions">
          <button type="button" className="secondary-button" onClick={() => { if (validCode()) setVisible(!visible); }}>{text(visible ? "hide" : "show")}</button>
          <button type="button" className="secondary-button" onClick={() => void copy(validCode())}>{text("copyCode")}</button>
          <button type="button" className="secondary-button" disabled={Boolean(address.issue)} onClick={() => void copy(address.origin)}>{text("copyAddress")}</button>
        </div>
      </>}
      {copyStatus && <p role="status">{text(copyStatus)}</p>}
      <ol><li>{text("install")}</li><li>{text("join")}</li><li>{text("verify")}</li></ol>
      <div className="shell-actions">
        {view.phase !== "created" && <button type="submit" className="primary-button" disabled={creating || operationBusy || Boolean(address.issue)}>{text("create")}</button>}
        <button type="button" className="secondary-button" onClick={onViewDevices}>{text("devices")}</button>
        <button type="button" className="secondary-button" onClick={onClose}>{text("close")}</button>
      </div>
    </form>
  </WorkspaceDialog>;
}
