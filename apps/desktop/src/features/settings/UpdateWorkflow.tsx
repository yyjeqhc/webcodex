import { useEffect, useId, useRef, useState } from "react";
import type { RuntimeUpdates } from "../../hooks/useRuntimeUpdates";
import type { UpdateConfirmation, UpdateErrorKind } from "../../models/runtime-shell";
import { desktopApi } from "../../lib/desktop-api";
import { useUpdateText } from "../../i18n/update-text";

const errors: Record<UpdateErrorKind, string> = {
  network_unavailable: "The update service is unavailable. Retry later; normal work is unaffected.",
  manifest_missing: "This release has no automatic installer. View the release to update manually.",
  manifest_invalid: "Update metadata could not be verified. Nothing will be installed.",
  unsupported_platform: "A verified installer is unavailable for this platform or package manager.",
  download_failed: "The update download was interrupted. Retry to download it again.",
  download_too_large: "The download exceeded the safety limit. View the release for help.",
  checksum_mismatch: "Update download could not be verified. Nothing will be installed.",
  source_manifest_invalid: "Update download could not be verified. Nothing will be installed.",
  provenance_failed: "The published build could not be verified. Automatic replacement is blocked.",
  cache_unavailable: "The private update cache is unavailable. Check disk space and app data permissions.",
  cancelled: "Download paused until you choose Retry.",
  upgrade_preflight_failed: "Installation could not be prepared safely. Finish active work and check the local Environment before retrying.",
  authorization_required: "System authorization was not completed. The update has not been installed.",
  guarded_handoff_unavailable: "This release requires manual installation. Use the release instructions; automatic installation is unavailable.",
  installer_launch_failed: "The system installer could not be started. Review the update status before retrying.",
  upgrade_rolled_back: "The installation did not complete. The previous version was restored. Retry only after reviewing the Environment.",
  recovery_required: "The previous installation needs attention. Do not start another installer. Check Environment recovery before continuing.",
};

export function formatUpdateBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  if (bytes < 1024) return `${Math.floor(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function UpdateWorkflow({ updates, banner = false }: { updates: RuntimeUpdates; banner?: boolean }) {
  const u = useUpdateText(); const s = u; const status = updates.status; const update = updates.local?.view.download ?? status?.download;
  const [reviewedOperation, setReviewedOperation] = useState<string | null>(null);
  const [openError, setOpenError] = useState(false); const [confirmation, setConfirmation] = useState<UpdateConfirmation | null>(null);
  const opener = useRef<HTMLElement | null>(null); const dialog = useRef<HTMLElement | null>(null);
  const close = () => { setConfirmation(null); opener.current?.focus(); };
  useEffect(() => { if (confirmation) dialog.current?.querySelector<HTMLButtonElement>("button")?.focus(); }, [confirmation]);
  const confirmVersion = confirmation?.candidate.version;
  const title = useId();
  const recovery = update?.error_kind === "recovery_required";
  const view = updates.local?.view; const upgrade = view?.upgrade;
  if (!status?.latest && !update?.pending_install && !recovery && !upgrade && !view?.restart_required) return null;
  const version = update?.pending_install ? update.version : status?.latest?.version ?? upgrade?.version;
  const sameTarget = Boolean(update?.version && update.version === version);
  const phase = sameTarget ? update?.phase : "available";
  const waitingTasks = view?.blockers.includes("active_tasks") ?? false;
  const pendingUpgrade = Boolean(upgrade && !["committed", "rolled_back"].includes(upgrade.phase));
  const newerCandidate = Boolean(upgrade && ((view?.candidate && view.candidate.manifest_sha256 !== upgrade.manifest_sha256) || (status?.update_available && status.latest?.version !== upgrade.version)));
  const durable = Boolean(view?.restart_required || pendingUpgrade || (upgrade && !newerCandidate && !(upgrade.phase === "rolled_back" && reviewedOperation === upgrade.operation_id)));
  const pending = Boolean(update?.pending_install);
  const active = phase === "checking" || phase === "downloading" || phase === "verifying";
  const installing = phase === "preparing" || phase === "installing_or_handed_off";
  const knownTotal = sameTarget && update?.total_bytes != null && update.total_bytes > 0 && update.downloaded_bytes <= update.total_bytes;
  const percent = knownTotal ? Math.floor((update!.downloaded_bytes / update!.total_bytes!) * 100) : null;
  const handoffUnavailable = update?.error_kind === "guarded_handoff_unavailable" || Boolean(view?.blockers.includes("guarded_handoff_unavailable"));
  const failure = update?.error_kind ? errors[update.error_kind] ?? "The update action could not be completed. Review the update status." : handoffUnavailable ? errors.guarded_handoff_unavailable : null;
  const blocked = update?.installation === "source_build" || update?.installation === "unmanaged_installation";
  const unsupported = update?.installation === "unsupported_platform";
  const ready = sameTarget && phase === "ready_to_install";
  const canDownload = !durable && status?.update_available && update && !unsupported && !update.legacy_release && !pending && !recovery && !active && !installing && !ready;
  const durableText = view?.restart_required ? "Update completed. Restart Desktop to use the installed version."
    : upgrade?.phase === "committed" ? "Update completed and verified."
    : upgrade?.phase === "rolled_back" ? "Previous version restored. Review local status before trying again."
    : upgrade?.phase === "restoring" ? "Restoration is recorded. Its executor is not confirmed. Review recovery status."
    : upgrade?.phase === "recovery_required" ? "Manual recovery is required. Do not start another installer."
    : phase === "installing_or_handed_off" ? "Installer handoff is recorded. Completion and executor activity are unconfirmed."
    : upgrade?.phase === "prepared" ? "Upgrade preparation is recorded. Executor activity is unconfirmed."
    : upgrade?.phase === "stopping" ? "Service shutdown is recorded. Executor activity is unconfirmed."
    : upgrade?.phase === "stopped" ? "Services are recorded as stopped. Upgrade completion is unconfirmed."
    : upgrade?.phase === "snapshot_ready" ? "Upgrade snapshot is recorded. Installer handoff is unconfirmed."
    : upgrade?.phase === "verifying" ? "Installation verification is recorded. Executor activity is unconfirmed."
    : upgrade ? "Installation is pending. Do not start another installer."
    : waitingTasks ? "Waiting for active tasks to finish. Refresh status when ready."
    : view?.blockers.includes("task_observation_unavailable") ? "Active task count is unconfirmed. Refresh local status before installing."
    : null;
  const stateText = phase === "checking" ? "Checking installer metadata…"
    : phase === "downloading" ? "Downloading update…"
    : phase === "verifying" ? "Verifying update…"
    : ready ? "Downloaded and verified"
    : phase === "preparing" ? "Upgrade preparation is recorded. Executor activity is unconfirmed."
    : phase === "installing_or_handed_off" ? "Installer handoff is recorded. Completion and executor activity are unconfirmed."
    : update?.cancelled && phase !== "failed" ? "Download paused until you choose Retry."
    : "A new stable WebCodex release is available.";
  return <div className="update-workflow" aria-label={s("Stable update")}>
    {!banner && version && <strong>WebCodex {version}</strong>}
    {upgrade && upgrade.version !== version && <p>{u("Recorded upgrade")} · WebCodex {upgrade.version}</p>}
    <p role="status">{durableText ? u(durableText) : s(stateText)}</p>
    {view?.candidate && <p className="field-help">{u("Candidate revision")} <code>{view.candidate.source_sha}</code></p>}
    {(durable || waitingTasks || view?.blockers.includes("task_observation_unavailable") || updates.localError) && <button type="button" className="secondary-button" disabled={updates.actionBusy} onClick={() => { void updates.refreshLocal().then(next => { if (next?.view.upgrade?.phase === "rolled_back") setReviewedOperation(next.view.upgrade.operation_id); }); }}>{u("Refresh local update status")}</button>}
    {phase === "downloading" && <div className="update-progress">
      <progress aria-label={s("Update download progress")} value={percent ?? undefined} max={100} />
      <span>{percent !== null ? `${percent}% · ` : ""}{formatUpdateBytes(update?.downloaded_bytes ?? 0)}{knownTotal ? ` / ${formatUpdateBytes(update!.total_bytes!)}` : ""}</span>
    </div>}
    {blocked && <p className="field-help">{s("This is a source, development, or standalone installation. Downloading is optional; automatic replacement is disabled.")}</p>}
    {update?.installation === "environment_not_configured" && <p className="field-help">{s("The update can be downloaded, but one-click installation requires a configured local Environment owned by this account.")}</p>}
    {unsupported && <p className="field-help">{s("A verified installer is unavailable for this platform or package manager.")}</p>}
    {update?.legacy_release && <p className="field-help">{s("This release has no automatic installer. View the release to update manually.")}</p>}
    {failure && <p role="status" className="update-error">{s(failure)}</p>}
    {updates.actionError && !failure && <p role="status">{s("The update action could not be completed. Review the update status.")}</p>}
    <div className="shell-actions">
      {ready && update?.can_install && !handoffUnavailable && !pending && !recovery && !durable && !waitingTasks && updates.local?.confirmation && <button type="button" className="primary-button" disabled={updates.actionBusy} onClick={event => { opener.current = event.currentTarget; setConfirmation(updates.local?.confirmation ?? null); }}>{s("Install update")}</button>}
      {canDownload && <button type="button" className="secondary-button" disabled={updates.actionBusy} onClick={() => void updates.download()}>{s(phase === "failed" || update?.cancelled ? "Retry" : "Download update")}</button>}
      {active && <button type="button" className="secondary-button" disabled={updates.actionBusy} onClick={() => void updates.cancelDownload()}>{s("Cancel download")}</button>}
      {status?.latest && <button type="button" className="secondary-button" onClick={() => {
        setOpenError(false); void desktopApi.openLatestRelease().catch(() => setOpenError(true));
      }}>{s(status.latest.compatibility === "desktop_required" ? "View Desktop release" : "View release")}</button>}
      {!pending && !recovery && !installing && !durable && <button type="button" className="text-button" disabled={updates.actionBusy} onClick={() => { close(); void updates.remindLater(); }}>{s(ready ? "Later" : "Remind me later")}</button>}
    </div>
    {openError && <p role="status">{s("Unable to open this location.")}</p>}
    {confirmation && <section ref={dialog} className="update-confirmation" role="alertdialog" aria-modal="true" aria-labelledby={title} onKeyDown={event => {
      if (event.key === "Escape") { event.preventDefault(); close(); }
      if (event.key === "Tab") {
        const buttons = [...(dialog.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
        const first = buttons[0]; const last = buttons[buttons.length - 1];
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }
    }}>
      <h3 id={title}>{s("Install unified update?")} · {confirmVersion}</h3>
      <dl className="runtime-facts"><div><dt>{u("Local Environment")}</dt><dd><code>{confirmation.target.environment_id}</code></dd></div><div><dt>{u("Candidate revision")}</dt><dd><code>{confirmation.candidate.source_sha}</code></dd></div></dl>
      <div><strong>{u("Local services that may restart")}</strong>{confirmation.service_inventory_complete ? confirmation.services.length > 0 ? <ul>{confirmation.services.map(service => <li key={`${service.component}:${service.scope}`}>{service.component === "server" ? "Server" : service.component === "runner" ? "Runner" : "Tunnel"} · {u(service.scope === "user" ? "User service scope" : "System service scope")}</li>)}</ul> : <p>{u("No local services are included.")}</p> : <p>{u("Local service scope is unconfirmed.")}</p>}</div>
      <p>{u("This replaces local Desktop, CLI, Server and Runner files. Only services in this saved local Environment may restart; remote services are unaffected.")}</p>
      <p>{s("This updates Desktop, CLI, Server and Runner together. Finish active work first. Local services may stop, and Desktop will close after the system installer starts. Your operating system may request administrator authorization.")}</p>
      <div className="shell-actions">
        <button type="button" className="primary-button" disabled={updates.actionBusy || recovery || handoffUnavailable || durable || !ready || !update?.can_install || !confirmation.service_inventory_complete || version !== confirmVersion || JSON.stringify(updates.local?.confirmation) !== JSON.stringify(confirmation)} onClick={() => {
          const target = confirmation; close(); void updates.install(target);
        }}>{s("Install and close WebCodex")}</button>
        <button type="button" className="secondary-button" onClick={() => close()}>{s("Not now")}</button>
      </div>
    </section>}
  </div>;
}
