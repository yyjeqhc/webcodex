import { useId, useState } from "react";
import type { RuntimeUpdates } from "../../hooks/useRuntimeUpdates";
import type { UpdateErrorKind } from "../../models/runtime-shell";
import { desktopApi } from "../../lib/desktop-api";
import { useShellText } from "../../i18n/runtime-shell";

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
  const s = useShellText(); const status = updates.status; const update = status?.download;
  const [openError, setOpenError] = useState(false); const [confirmVersion, setConfirmVersion] = useState<string | null>(null);
  const title = useId();
  if (!status?.latest && !update?.pending_install) return null;
  const version = update?.pending_install ? update.version : status?.latest?.version;
  const sameTarget = Boolean(update?.version && update.version === version);
  const phase = sameTarget ? update?.phase : "available";
  const pending = Boolean(update?.pending_install);
  const active = phase === "checking" || phase === "downloading" || phase === "verifying";
  const installing = phase === "preparing" || phase === "installing_or_handed_off";
  const knownTotal = sameTarget && update?.total_bytes != null && update.total_bytes > 0 && update.downloaded_bytes <= update.total_bytes;
  const percent = knownTotal ? Math.floor((update!.downloaded_bytes / update!.total_bytes!) * 100) : null;
  const failure = update?.error_kind ? errors[update.error_kind] ?? "The update action could not be completed. Review the update status." : null;
  const blocked = update?.installation === "source_build" || update?.installation === "unmanaged_installation";
  const unsupported = update?.installation === "unsupported_platform";
  const ready = sameTarget && phase === "ready_to_install";
  const canDownload = status?.update_available && update && !unsupported && !update.legacy_release && !pending && !active && !installing && !ready;
  const stateText = phase === "checking" ? "Checking installer metadata…"
    : phase === "downloading" ? "Downloading update…"
    : phase === "verifying" ? "Verifying update…"
    : ready ? "Downloaded and verified"
    : phase === "preparing" ? "Preparing the unified upgrade…"
    : phase === "installing_or_handed_off" ? "The system installer is running. Installation is not yet confirmed."
    : update?.cancelled && phase !== "failed" ? "Download paused until you choose Retry."
    : "A new stable WebCodex release is available.";
  return <div className="update-workflow" aria-label={s("Stable update")}>
    {!banner && <strong>WebCodex {version}</strong>}
    <p role="status">{s(stateText)}</p>
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
      {ready && update?.can_install && !pending && <button type="button" className="primary-button" disabled={updates.actionBusy} onClick={() => setConfirmVersion(version ?? null)}>{s("Install update")}</button>}
      {canDownload && <button type="button" className="secondary-button" disabled={updates.actionBusy} onClick={() => void updates.download()}>{s(phase === "failed" || update?.cancelled ? "Retry" : "Download update")}</button>}
      {active && <button type="button" className="secondary-button" disabled={updates.actionBusy} onClick={() => void updates.cancelDownload()}>{s("Cancel download")}</button>}
      {status?.latest && <button type="button" className="secondary-button" onClick={() => {
        setOpenError(false); void desktopApi.openLatestRelease().catch(() => setOpenError(true));
      }}>{s(status.latest.compatibility === "desktop_required" ? "View Desktop release" : "View release")}</button>}
      {!pending && !installing && <button type="button" className="text-button" disabled={updates.actionBusy} onClick={() => { setConfirmVersion(null); void updates.remindLater(); }}>{s(ready ? "Later" : "Remind me later")}</button>}
    </div>
    {openError && <p role="status">{s("Unable to open this location.")}</p>}
    {confirmVersion && <section className="update-confirmation" role="alertdialog" aria-labelledby={title}>
      <h3 id={title}>{s("Install unified update?")} · {confirmVersion}</h3>
      <p>{s("This updates Desktop, CLI, Server and Runner together. Finish active work first. Local services may stop, and Desktop will close after the system installer starts. Your operating system may request administrator authorization.")}</p>
      <div className="shell-actions">
        <button type="button" className="primary-button" disabled={updates.actionBusy || !ready || !update?.can_install || version !== confirmVersion} onClick={() => {
          const target = confirmVersion; setConfirmVersion(null); void updates.install(target);
        }}>{s("Install and close WebCodex")}</button>
        <button type="button" className="secondary-button" onClick={() => setConfirmVersion(null)}>{s("Not now")}</button>
      </div>
    </section>}
  </div>;
}
