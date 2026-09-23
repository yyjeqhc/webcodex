import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState } from "../../models/topology";
import type { RuntimeCandidate, RuntimeSettings, RuntimeSource, RuntimeSwitchResult } from "../../models/runtime-shell";
import { useShellText } from "../../i18n/runtime-shell";
import { normalizeDesktopError } from "../../i18n/presentation";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

export function RuntimePanel({ state, onState, onActivity }: { state: DesktopState; onState: (state: DesktopState) => void; onActivity?: () => void }) {
  const s = useShellText();
  const [settings, setSettings] = useState<RuntimeSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<RuntimeCandidate | null>(null);
  const [result, setResult] = useState<RuntimeSwitchResult | null>(null);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    void Promise.resolve().then(() => desktopApi.runtimeSettings()).then(value => { if (alive.current) { setSettings(value); setResult(value.last_switch); } })
      .catch(value => { if (alive.current) setError(normalizeDesktopError(value).code); });
    return () => { alive.current = false; };
  }, []);
  const disabled = busy || Boolean(state.current_operation);
  const run = async (action: () => Promise<void>) => {
    if (disabled) return;
    setBusy(true); setError(null);
    try { await action(); } catch (value) { if (alive.current) setError(normalizeDesktopError(value).code); }
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
  return <div className="runtime-settings-panel" data-webcodex-panel="runtime">
    <h2>{s("Current Runtime")}</h2>
    <p className="field-help">{s("Build revisions are diagnostic identity, not compatibility gates.")}</p>
    {settings ? <>
      <dl className="runtime-facts"><div><dt>{s("Source")}</dt><dd>{s(settings.source.kind === "bundled" ? "Bundled" : "Custom")}</dd></div>
        <div><dt>{s("Desktop contract")}</dt><dd>[{settings.desktop_contract.min_generation}, {settings.desktop_contract.max_generation}]</dd></div></dl>
      {settings.source.kind === "custom" && <code className="runtime-directory">{settings.source.directory}</code>}
      {settings.selected && <BinaryFacts candidate={settings.selected} />}
      {(settings.unavailable_code || settings.selected?.error_code) && <div className="shell-notice warning" role="alert"><strong>{s("Selected Runtime is unavailable")}</strong><code>{settings.unavailable_code || settings.selected?.error_code}</code></div>}
      {!settings.can_switch && <p className="field-help"><code>{settings.switch_unavailable_reason}</code></p>}
    </> : !error && <p role="status">{s("Loading…")}</p>}
    <div className="shell-actions">
      <button type="button" className="secondary-button" disabled={disabled} onClick={() => void probe({ kind: "bundled" })}>{s("Use bundled Runtime")}</button>
      <button type="button" className="primary-button" disabled={disabled} onClick={() => void choose()}>{s("Select Runtime folder…")}</button>
      <button type="button" className="secondary-button" disabled={disabled} onClick={() => void run(async () => { const next = await desktopApi.recheckRuntime(); if (alive.current) setSettings(next); })}>{s("Recheck Runtime")}</button>
      <button type="button" className="text-button" disabled={disabled || !state.binaries} onClick={() => void run(() => desktopApi.openDiagnosticResource("runtime_directory"))}>{s("Open Runtime folder")}</button>
    </div>
    {settings?.candidate && <div className="runtime-candidate" data-webcodex-panel="runtime-candidate">
      <h3>{s("Candidate Runtime")}</h3><p className="field-help">{s("Candidate inspection does not change the active Runtime.")}</p>
      {settings.candidate.directory && <code className="runtime-directory">{settings.candidate.directory}</code>}
      <BinaryFacts candidate={settings.candidate} />
      <button type="button" className="primary-button" disabled={disabled || !settings.can_switch || settings.candidate.compatibility !== "compatible"} onClick={stageSwitch}>{s("Use this Runtime")}</button>
    </div>}
    {result && <div className={`shell-notice ${result.outcome === "recovery_required" ? "warning" : ""}`} role={result.outcome === "recovery_required" ? "alert" : "status"}>
      <strong>{s(resultTitle)}</strong>{result.reason_code && <code>{result.reason_code}</code>}{result.rollback_reason_code && <code>{result.rollback_reason_code}</code>}
    </div>}
    {error && <div role="alert" className="shell-notice warning"><strong>{s("Runtime unavailable")}</strong><code>{error}</code><span>{s("Recheck Runtime")}</span></div>}
    {busy && <p role="status">{s("Loading…")}</p>}
    {confirm && <WorkspaceDialog title={s("Runtime restart warning")} onClose={() => setConfirm(null)} busy={disabled}>
      <p>{s("Restarting the Desktop-owned Runner may interrupt active Jobs.")}</p>
      <p>{settings?.active_jobs == null ? s("Job count is not confirmed.") : `${s("Active Jobs")}: ${settings.active_jobs}`}</p>
      <div className="shell-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={() => setConfirm(null)}>{s("Cancel")}</button>
        {onActivity && <button type="button" className="text-button" disabled={disabled} onClick={onActivity}>{s("View activity")}</button>}
        <button type="button" className="primary-button" disabled={disabled} onClick={() => void activate(confirm, true)}>{s("Switch anyway")}</button></div>
    </WorkspaceDialog>}
  </div>;
}

export function BinaryFacts({ candidate }: { candidate: RuntimeCandidate }) {
  const s = useShellText();
  return <>
    <dl className="runtime-facts"><div><dt>{s("Compatibility")}</dt><dd>{s(candidate.compatibility === "compatible" ? "Compatible" : candidate.compatibility === "incompatible" ? "Incompatible" : "Unknown")}</dd></div>
      <div><dt>{s("Build alignment")}</dt><dd>{s(({ exact: "Exact", different_commit: "Different revisions", different_version: "Different versions", dirty: "Dirty build", unknown: "Unknown" })[candidate.build_alignment])}</dd></div></dl>
    {!!candidate.binaries.length && <div className="runtime-binary-list" aria-label={s("Required binaries")}>{candidate.binaries.map(binary => <article key={binary.name}>
      <h4>{binary.name}</h4><dl className="runtime-facts"><div><dt>{s("File")}</dt><dd>{s(binary.present ? "Present" : "Missing")}</dd></div><div><dt>{s("Executable")}</dt><dd>{binary.executable ? "✓" : "—"}</dd></div>
        {binary.metadata && <><div><dt>{s("Version")}</dt><dd>{binary.metadata.version}</dd></div><div><dt>{s("Revision")}</dt><dd><code>{binary.metadata.git_commit ?? s("Unknown")}</code>{binary.metadata.git_dirty && <span className="workspace-badge">{s("Dirty build")}</span>}</dd></div>
          <div><dt>{s("Target / architecture")}</dt><dd>{binary.metadata.target} / {binary.metadata.architecture}</dd></div><div><dt>{s("Desktop contract")}</dt><dd>[{binary.metadata.desktop_runtime_contract.min_generation}, {binary.metadata.desktop_runtime_contract.max_generation}]</dd></div></>}
      </dl>{binary.error_code && <code className="runtime-probe-error">{binary.error_code}</code>}</article>)}</div>}
    {candidate.advisories.length > 0 && <p className="field-help">{s("Build revisions are diagnostic identity, not compatibility gates.")} {s("Operator responsibility")}</p>}
    {candidate.error_code && <p role="alert"><code>{candidate.error_code}</code></p>}
  </>;
}
