import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";
import type { BinaryProbe, RuntimeCandidate, RuntimeSettings, RuntimeSource, RuntimeSwitchResult } from "../../models/runtime-shell";
import { useShellText } from "../../i18n/runtime-shell";
import { useProduct } from "../../i18n/product";
import { useLocale } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

export function RuntimePanel({ state, onState, onActivity, onUpdates }: { state: DesktopState; onState: (state: DesktopState) => void; onActivity?: () => void; onUpdates?: () => void }) {
  const s = useShellText(); const p = useProduct(); const { t } = useLocale();
  const [settings, setSettings] = useState<RuntimeSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [confirm, setConfirm] = useState<RuntimeCandidate | null>(null);
  const [result, setResult] = useState<RuntimeSwitchResult | null>(null);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    void Promise.resolve().then(() => desktopApi.runtimeSettings()).then(value => { if (alive.current) { setSettings(value); setResult(value.last_switch); } })
      .catch(value => { if (alive.current) setError(normalizeDesktopError(value)); });
    return () => { alive.current = false; };
  }, []);
  const disabled = busy || Boolean(state.current_operation);
  const run = async (action: () => Promise<void>) => {
    if (disabled) return;
    setBusy(true); setError(null);
    try { await action(); } catch (value) { if (alive.current) setError(normalizeDesktopError(value)); }
    finally { if (alive.current) setBusy(false); }
  };
  const probe = (source: RuntimeSource) => run(async () => {
    const next = await desktopApi.probeRuntime(source);
    if (alive.current) { setSettings(next); setResult(null); }
  });
  const choose = () => run(async () => {
    const directory = await open({ title: s("Select Runtime folder…"), directory: true, multiple: false });
    if (typeof directory !== "string" || !alive.current) return;
    const next = await desktopApi.probeRuntime({ kind: "custom", directory });
    if (alive.current) { setSettings(next); setResult(null); }
  });
  const activate = (candidate: RuntimeCandidate, confirmInterrupt: boolean) => run(async () => {
    const next = await desktopApi.switchRuntime({ candidate_id: candidate.candidate_id, expected_selection_revision: candidate.selection_revision, confirm_interrupt: confirmInterrupt });
    if (alive.current) { setResult(next); setConfirm(null); }
    onState(await desktopApi.getState());
    const current = await desktopApi.runtimeSettings();
    if (alive.current) setSettings(current);
  });
  const stageSwitch = () => {
    const candidate = settings?.candidate;
    if (!candidate || candidate.compatibility !== "compatible" || !settings.can_switch || disabled) return;
    // The native side rechecks Jobs and both identity/byte fences immediately
    // before stopping anything. A stale zero here never grants that authority.
    if (settings.active_jobs !== 0) setConfirm(candidate);
    else void activate(candidate, false);
  };
  const resultTitle = result?.outcome === "activated" ? "Runtime activated" : result?.outcome === "selected" ? "Runtime selected; start it when ready" : result?.outcome === "rolled_back" ? "Previous Runtime restored" : "Runtime recovery required";
  const switchHelp = settings?.switch_unavailable_reason === "persistent_runtime_upgrade_required" ? "runtimeManagedHelp" : settings?.switch_unavailable_reason === "quick_share_must_be_stopped" ? "runtimeQuickShareHelp" : settings?.switch_unavailable_reason === "configuration_migration_failed" ? "runtimeConfigurationHelp" : "runtimeOwnershipHelp";
  const presentation = error && desktopErrorPresentation(error, t);
  const errorCard = presentation && <div role="alert" className="error-card"><strong>{presentation.title}</strong><span>{presentation.action}</span><details><summary>{p("details")}</summary><code>{error?.code}</code></details></div>;
  return <div className="runtime-settings-panel" data-webcodex-panel="runtime">
    <h2>{s("Current Runtime")}</h2>
    <p className="field-help">{p("runtimeFilesHelp")}</p>
    {settings ? <>
      <dl className="runtime-facts"><div><dt>{s("Source")}</dt><dd>{s(settings.source.kind === "bundled" ? "Bundled" : "Custom")}</dd></div></dl>
      {settings.source.kind === "custom" && <code className="runtime-directory">{settings.source.directory}</code>}
      {settings.selected && <BinaryFacts candidate={settings.selected} checkedDesktopContract={settings.desktop_contract} />}
      {settings.unavailable_code && !settings.selected?.error_code && <div className="shell-notice warning" role="alert"><strong>{s("Selected Runtime is unavailable")}</strong><p>{p("runtimeUnavailableHelp")}</p><code>{settings.unavailable_code}</code></div>}
      {!settings.can_switch && <div className="shell-notice"><p>{p(switchHelp)}</p>{switchHelp === "runtimeManagedHelp" && onUpdates && <button type="button" className="secondary-button" onClick={onUpdates}>{p("aboutAndUpdates")}</button>}<details><summary>{p("details")}</summary><code>{settings.switch_unavailable_reason}</code></details></div>}
    </> : !error && <p role="status">{s("Loading…")}</p>}
    <div className="shell-actions">
      <button type="button" className="secondary-button" disabled={disabled} onClick={() => void run(async () => { const next = await desktopApi.recheckRuntime(); if (alive.current) setSettings(next); })}>{s("Recheck Runtime")}</button>
      {settings?.can_switch && <><button type="button" className="secondary-button" disabled={disabled} onClick={() => void probe({ kind: "bundled" })}>{s("Use bundled Runtime")}</button>
      <button type="button" className="secondary-button" disabled={disabled} onClick={() => void choose()}>{s("Select Runtime folder…")}</button></>}
    </div>
    {settings?.candidate && <div className="runtime-candidate" data-webcodex-panel="runtime-candidate">
      <h3>{s("Candidate Runtime")}</h3><p className="field-help">{p("runtimeCandidateHelp")}</p>
      {settings.candidate.directory && <code className="runtime-directory">{settings.candidate.directory}</code>}
      <BinaryFacts candidate={settings.candidate} />
      <button type="button" className="primary-button" disabled={disabled || !settings.can_switch || settings.candidate.compatibility !== "compatible"} onClick={stageSwitch}>{s("Use this Runtime")}</button>
    </div>}
    {result && <div className={`shell-notice ${result.outcome === "recovery_required" ? "warning" : ""}`} role={result.outcome === "recovery_required" ? "alert" : "status"}>
      <strong>{s(resultTitle)}</strong>{result.reason_code && <code>{result.reason_code}</code>}{result.rollback_reason_code && <code>{result.rollback_reason_code}</code>}
    </div>}
    {!confirm && errorCard}
    {busy && <p role="status">{s("Loading…")}</p>}
    {confirm && <WorkspaceDialog title={s("Runtime restart warning")} onClose={() => setConfirm(null)} busy={disabled}>
      <p>{s("Restarting the Desktop-owned Runner may interrupt active Jobs.")}</p>
      <p>{settings?.active_jobs == null ? s("Job count is not confirmed.") : `${s("Active Jobs")}: ${settings.active_jobs}`}</p>
      {errorCard}
      <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => setConfirm(null)}>{s("Cancel")}</button>
        {onActivity && <button type="button" className="text-button" disabled={disabled} onClick={onActivity}>{s("View activity")}</button>}
        <button type="button" className="primary-button" disabled={disabled} onClick={() => void activate(confirm, true)}>{s("Switch anyway")}</button></div>
    </WorkspaceDialog>}
  </div>;
}

function BinaryFacts({ candidate, checkedDesktopContract }: { candidate: RuntimeCandidate; checkedDesktopContract?: RuntimeSettings["desktop_contract"] }) {
  const s = useShellText(); const p = useProduct();
  return <>
    <dl className="runtime-facts"><div><dt>{s("Compatibility")}</dt><dd>{s(candidate.compatibility === "compatible" ? "Compatible" : candidate.compatibility === "incompatible" ? "Incompatible" : "Unknown")}</dd></div></dl>
    {candidate.compatibility === "compatible" && !candidate.error_code && <p className="field-help">{p("runtimeReadyHelp")}</p>}
    {candidate.compatibility !== "compatible" && !candidate.error_code && <p className="field-help">{p("runtimeUnavailableHelp")}</p>}
    {candidate.error_code && <div className="shell-notice warning" role="alert"><strong>{p("runtimeVerificationFailed")}</strong><p>{p(candidate.compatibility === "incompatible" ? "runtimeMismatchHelp" : candidate.compatibility === "unknown" && candidate.binaries.length ? "runtimeCheckUnconfirmedHelp" : "runtimeUnavailableHelp")}</p><details><summary>{p("details")}</summary><code>{candidate.error_code}</code></details></div>}
    {candidate.advisories.includes("runtime_files_changed_restart_required") && <p className="workspace-notice" role="status">{p("needsRestart")}</p>}
    {!!candidate.binaries.length && <div className="runtime-binary-list" aria-label={s("Required binaries")}>{candidate.binaries.map(binary => <article key={binary.name}>
      <h4>{binary.name}</h4><dl className="runtime-facts"><div><dt>{s("File")}</dt><dd>{binary.present === null ? p("runtimeFileUnconfirmed") : s(binary.present ? "Present" : "Missing")}</dd></div><div><dt>{p("runtimeStartupCheck")}</dt><dd>{p(binary.startup_check === "passed" ? "runtimeStartupPassed" : binary.startup_check === "failed" ? "runtimeStartupFailed" : "runtimeStartupNotChecked")}</dd></div></dl>
      {binary.error_code && <p className="field-help">{p(binaryFailureHelp(binary))}</p>}
      {(binary.metadata || binary.error_code) && <details className="workspace-technical"><summary>{p("details")}</summary>
        {binary.metadata && <dl className="runtime-facts"><div><dt>{s("Version")}</dt><dd>{binary.metadata.version}</dd></div><div><dt>{s("Revision")}</dt><dd><code>{binary.metadata.git_commit ?? s("Unknown")}</code>{binary.metadata.git_dirty && <span className="workspace-badge">{s("Dirty build")}</span>}</dd></div>
          <div><dt>{s("Target / architecture")}</dt><dd>{binary.metadata.target} / {binary.metadata.architecture}</dd></div><div><dt>{s("Desktop contract")}</dt><dd>[{binary.metadata.desktop_runtime_contract.min_generation}, {binary.metadata.desktop_runtime_contract.max_generation}]</dd></div></dl>}
        {binary.error_code && <code className="runtime-probe-error">{binary.error_code}</code>}
        {binary.diagnostics?.exit_code != null && <p>{p("runtimeExitCode")}: <code>{binary.diagnostics.exit_code} / 0x{(binary.diagnostics.exit_code >>> 0).toString(16).padStart(8, "0").toUpperCase()}</code></p>}
        {binary.diagnostics?.io_kind && <p><code>{binary.diagnostics.io_kind}</code></p>}
      </details>}
    </article>)}</div>}
    <details className="workspace-technical runtime-build-details"><summary>{p("buildDetails")}</summary>
    <dl className="runtime-facts">
      {checkedDesktopContract && <div><dt>{s("Desktop contract")}</dt><dd>[{checkedDesktopContract.min_generation}, {checkedDesktopContract.max_generation}]</dd></div>}
      <div><dt>{s("Build alignment")}</dt><dd>{s(({ exact: "Exact", different_commit: "Different revisions", different_version: "Different versions", dirty: "Dirty build", unknown: "Unknown" })[candidate.build_alignment])}</dd></div></dl>
    {candidate.advisories.length > 0 && <p className="field-help">{s("Build revisions are diagnostic identity, not compatibility gates.")} {s("Operator responsibility")}</p>}
    </details>
  </>;
}

function binaryFailureHelp(binary: BinaryProbe) {
  if (binary.diagnostics?.io_kind === "PermissionDenied" || (binary.diagnostics?.exit_code != null && (binary.diagnostics.exit_code >>> 0) === 0xC0000022)) return "runtimeAccessDeniedHelp";
  switch (binary.error_code) {
    case "binary_missing": return "runtimeFileMissingHelp";
    case "runtime_file_unreadable": return "runtimeFileUnreadableHelp";
    case "binary_not_executable":
    case "webcodex_command_start_failed": return "runtimeStartFailedHelp";
    case "webcodex_command_timeout": return "runtimeProbeTimeoutHelp";
    case "binary_architecture_mismatch": return "runtimeArchitectureMismatchHelp";
    case "runtime_contract_incompatible": return "runtimeMismatchHelp";
    default: return "runtimeMetadataFailedHelp";
  }
}
