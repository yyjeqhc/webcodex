import { useCallback, useEffect, useRef, useState } from "react";
import { desktopApi } from "../../lib/desktop-api";
import type { ComputerPermissions as Permissions } from "../../models/topology";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { useShellText } from "../../i18n/runtime-shell";

const EXPLAINED_KEY = "desktop-permissions-explained";
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
  const showWelcome = welcome && !dismissed && foreground && permissions?.supported
    && !(permissions.desktop_accessibility && permissions.desktop_screen_recording);
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
  const request = async (action?: "accessibility" | "screen_recording" | "open_settings") => {
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
    {permissions?.supported && <div className="permission-rows">
      {([ ["screen_recording", p("screenRecording"), permissions.desktop_screen_recording], ["accessibility", p("accessibility"), permissions.desktop_accessibility] ] as const).map(([action, label, allowed]) => <div className="permission-row" key={action}>
        <span>{label}</span><strong className={allowed ? "permission-allowed" : "permission-needed"}>{allowed ? `✓ ${p("allowed")}` : p("needed")}</strong>
        {!allowed && <button type="button" className="secondary-button" aria-label={`${p("grant")} · ${label}`} disabled={busy} onClick={() => void request(action)} data-webcodex-action={`request-${action.replace("_", "-")}`}>{p("grant")}</button>}
      </div>)}
    </div>}
    {failed && <p role="alert">{t("permissions.error")}</p>}
    {!permissions && !failed && <p role="status">{t("common.checking")}</p>}
    <details className="workspace-technical permission-troubleshooting"><summary>{s("Computer Use permission troubleshooting")}</summary>
      <p>{t("permissions.owner")}</p><p>{t("permissions.restartHelp")}</p>
      <div className="permission-actions"><button type="button" className="secondary-button" data-webcodex-action="recheck-permissions" disabled={busy} onClick={() => void request()}>{t("permissions.recheck")}</button>
      {permissions?.supported && <button type="button" className="secondary-button" disabled={busy} onClick={() => void request("open_settings")}>{t("permissions.openSettings")}</button>}</div>
    </details>
    {welcome && <button type="button" className="secondary-button permission-continue" data-webcodex-action="dismiss-permission-welcome" onClick={dismiss}>{t("permissions.later")}</button>}
  </>;
  return welcome
    ? <dialog ref={dialogRef} className="permission-dialog" aria-labelledby="permission-welcome-title" onCancel={event => { event.preventDefault(); dismiss(); }}>{content}</dialog>
    : <section className="settings-section permission-panel" aria-labelledby="permission-settings-title">{content}</section>;
}
