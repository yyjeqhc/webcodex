import { useCallback, useEffect, useRef, useState } from "react";
import { desktopApi } from "../../lib/desktop-api";
import type { ComputerPermissions as Permissions, PermissionStatus } from "../../models/topology";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { useShellText } from "../../i18n/runtime-shell";

const EXPLAINED_KEY = "desktop-permissions-explained";
type PermissionAction = "accessibility" | "screen_recording" | "open_settings" | "open_accessibility_settings" | "open_screen_recording_settings" | "show_runner";
function wasExplained() {
  try { return localStorage.getItem(EXPLAINED_KEY) === "1"; } catch { return false; }
}

export function ComputerPermissions({ welcome = false }: { welcome?: boolean }) {
  const { t } = useLocale();
  const p = useProduct(); const s = useShellText();
  const [permissions, setPermissions] = useState<Permissions | null>(null);
  const [dismissed, setDismissed] = useState(() => welcome && wasExplained());
  const [foreground, setForeground] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const generation = useRef(0);
  const requestInFlight = useRef(false);
  const observe = useCallback(async () => {
    if (requestInFlight.current) return;
    const current = ++generation.current;
    try {
      const next = await desktopApi.computerPermissions();
      if (generation.current !== current) return;
      setPermissions(next); setFailed(false);
      if (next.foreground) setForeground(true);
    } catch { if (generation.current === current) setFailed(true); }
  }, []);
  useEffect(() => {
    if (dismissed) return;
    void observe();
    const onFocus = () => { void observe(); };
    window.addEventListener("focus", onFocus);
    return () => { generation.current++; window.removeEventListener("focus", onFocus); };
  }, [dismissed, observe]);
  const showWelcome = welcome && !dismissed && foreground && permissions?.supported;
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!showWelcome || !dialog) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (!dialog.open) dialog.showModal?.();
    return () => { dialog.close?.(); if (previous?.isConnected) previous.focus(); };
  }, [showWelcome]);
  const dismiss = () => {
    try { localStorage.setItem(EXPLAINED_KEY, "1"); } catch { /* Session-local dismissal still works. */ }
    setDismissed(true);
  };
  const request = async (action?: PermissionAction) => {
    if (requestInFlight.current) return;
    requestInFlight.current = true;
    setBusy(true); setFailed(false);
    const current = ++generation.current;
    try {
      const next = await (action ? desktopApi.requestComputerPermission(action) : desktopApi.computerPermissions());
      if (generation.current === current) setPermissions(next);
    } catch { if (generation.current === current) setFailed(true); }
    finally { requestInFlight.current = false; if (generation.current === current) setBusy(false); }
  };
  if (welcome && !showWelcome) return null;
  if (!welcome && permissions && !permissions.supported) return null;
  const content = <>
    <h2 id={welcome ? "permission-welcome-title" : "permission-settings-title"}>Computer Use</h2>
    {permissions?.supported && <>
      <dl className="detail-list permission-execution-owner">
        <div><dt>{p("executionProcess")}</dt><dd>{permissions.execution_process ?? "WebCodex Runner"}</dd></div>
        {permissions.execution_path && <div><dt>{p("source")}</dt><dd><code>{permissions.execution_path}</code></dd></div>}
      </dl>
      <p>{t("permissions.owner")}</p>
      <div className="permission-rows" data-webcodex-permission-owner="runner">
        <RunnerPermissionRow label={p("screenRecording")} status={permissions.runner_screen_recording ?? "unknown"} busy={busy} onOpen={() => void request("open_screen_recording_settings")} />
        <RunnerPermissionRow label={p("accessibility")} status={permissions.runner_accessibility ?? "unknown"} busy={busy} onOpen={() => void request("open_accessibility_settings")} />
      </div>
      <div className="permission-actions">
        {permissions.execution_path && <button type="button" className="secondary-button" disabled={busy} onClick={() => void request("show_runner")} data-webcodex-action="show-runner">{p("showRunnerInFinder")}</button>}
        <button type="button" className="secondary-button" data-webcodex-action="recheck-permissions" disabled={busy} onClick={() => void request()}>{t("permissions.recheck")}</button>
      </div>
    </>}
    {failed && <><p role="alert">{t("permissions.error")}</p>{!permissions?.supported && <button type="button" className="secondary-button" data-webcodex-action="recheck-permissions" disabled={busy} onClick={() => void request()}>{t("permissions.recheck")}</button>}</>}
    {!permissions && !failed && <p role="status">{t("common.checking")}</p>}
    <details className="workspace-technical permission-troubleshooting"><summary>{s("Computer Use permission troubleshooting")}</summary>
      <strong>{p("desktopAppProbe")}</strong><p>{t("permissions.restartHelp")}</p>
      {permissions?.supported && <div className="permission-rows" data-webcodex-permission-owner="desktop">
        {([ ["screen_recording", p("screenRecording"), permissions.desktop_screen_recording], ["accessibility", p("accessibility"), permissions.desktop_accessibility] ] as const).map(([action, label, allowed]) => <div className="permission-row" key={action}>
          <span>Desktop · {label}</span><strong className={allowed ? "permission-allowed" : "permission-needed"}>{allowed ? `✓ ${t("permissions.granted")}` : t("permissions.notGranted")}</strong>
          {!allowed && <button type="button" className="secondary-button" aria-label={`${p("grant")} · Desktop · ${label}`} disabled={busy} onClick={() => void request(action)} data-webcodex-action={`request-desktop-${action.replace("_", "-")}`}>{p("grant")}</button>}
        </div>)}
      </div>}
      {permissions?.supported && <button type="button" className="secondary-button" disabled={busy} onClick={() => void request("open_settings")}>{t("permissions.openSettings")}</button>}
    </details>
    {welcome && <button type="button" className="secondary-button permission-continue" data-webcodex-action="dismiss-permission-welcome" onClick={dismiss}>{t("permissions.later")}</button>}
  </>;
  return welcome
    ? <dialog ref={dialogRef} className="permission-dialog" aria-labelledby="permission-welcome-title" onCancel={event => { event.preventDefault(); dismiss(); }}>{content}</dialog>
    : <section className="settings-section permission-panel" aria-labelledby="permission-settings-title">{content}</section>;
}

function RunnerPermissionRow({ label, status, busy, onOpen }: { label: string; status: PermissionStatus; busy: boolean; onOpen: () => void }) {
  const { t } = useLocale();
  const p = useProduct();
  const statusText = status === "granted" ? `✓ ${t("permissions.granted")}` : status === "denied" ? t("permissions.notGranted") : `${t("permissions.runnerUnknown")} · ${p("requiresSystemCheck")}`;
  return <div className="permission-row"><span>{label}</span><strong className={status === "granted" ? "permission-allowed" : status === "denied" ? "permission-needed" : undefined}>{statusText}</strong><button type="button" className="secondary-button" aria-label={`${t("permissions.openSettings")} · ${label}`} disabled={busy} onClick={onOpen}>{t("permissions.openSettings")}</button></div>;
}
