import { useCallback, useEffect, useRef, useState } from "react";
import { desktopApi } from "../../lib/desktop-api";
import { parseJobConcurrency, runnerCapacity } from "../../lib/runner-capacity";
import { RunnerCapacitySummary } from "../../components/RunnerCapacitySummary";
import { useShellText } from "../../i18n/runtime-shell";
import { normalizeDesktopError } from "../../i18n/presentation";
import type { DesktopError, DesktopState, RunnerSettings, SettingsTarget } from "../../models/topology";
import type { ServerRunnerSummary } from "../../models/workspace";
import { workspaceQuery } from "../workspace/WorkspaceContext";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

type Props = { state: DesktopState; onState: (state: DesktopState) => void; active: boolean };
const sameTarget = (a: SettingsTarget, b: SettingsTarget) => a.client_id === b.client_id && a.server_url === b.server_url && a.config_path === b.config_path;

export function RunnerCapacityPanel(props: Props) {
  // Retargeting dismisses confirmations and discards late observations/drafts.
  const key = JSON.stringify([props.state.workspace_runner, props.state.topology, props.state.persistent_environment]);
  return <RunnerCapacitySettings key={key} {...props} />;
}

function RunnerCapacitySettings({ state, onState, active }: Props) {
  const s = useShellText();
  const [settings, setSettings] = useState<RunnerSettings | null>(null);
  const [observation, setObservation] = useState<{ runner: ServerRunnerSummary; at: number | null } | null>(null);
  const [draft, setDraft] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [confirm, setConfirm] = useState<RunnerSettings | null>(null);
  const [settingsUnavailable, setSettingsUnavailable] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [now, setNow] = useState(Date.now);
  const alive = useRef(true), action = useRef(false), reading = useRef(0);
  const generation = useRef(0);
  const editBase = useRef<number | null | undefined>(undefined);
  const operationBusy = Boolean(state.current_operation);
  const serverReady = state.readiness.server === "ready";
  const selected = useRef(state.workspace_runner).current;
  const expectsLocalSettings = Boolean(selected && state.topology?.runner?.kind === "local");
  useEffect(() => { alive.current = true; return () => { alive.current = false; generation.current++; }; }, []);
  const refresh = useCallback(async (force = false, reloadDraft = false) => {
    if (reading.current && !force) return;
    const request = ++generation.current;
    reading.current = request;
    const [saved, live] = await Promise.allSettled([
      desktopApi.runnerSettings(), workspaceQuery<ServerRunnerSummary>({ kind: "runner_details" }),
    ]);
    if (reading.current === request) reading.current = 0;
    if (!alive.current || request !== generation.current) return;
    const next = saved.status === "fulfilled" && saved.value && selected && sameTarget(saved.value.target, selected) ? saved.value : null;
    setSettings(next);
    setSettingsUnavailable(expectsLocalSettings && !next);
    if (next?.max_concurrent_jobs !== undefined) {
      // Only an explicit, successful reload discards the draft and its old
      // compare-and-save fence. Background reads must not overwrite an edit.
      if (reloadDraft) { editBase.current = undefined; setError(null); }
      setDraft(current => editBase.current === undefined ? String(next.max_concurrent_jobs ?? 4) : current);
    }
    const runner = live.status === "fulfilled" && selected && live.value?.client_id === selected.client_id ? live.value : null;
    setObservation(runner ? { runner, at: Date.now() } : null);
    setNow(Date.now()); setLoading(false);
  }, [selected, expectsLocalSettings]);
  useEffect(() => {
    // Keep pre-operation counts stale until a new observation, and discard
    // responses from reads that crossed the service boundary.
    generation.current++;
    reading.current = 0;
    setObservation(current => current && ({ ...current, at: null }));
  }, [operationBusy, serverReady]);
  useEffect(() => {
    if (!active || operationBusy) return;
    if (!action.current) void refresh();
    const timer = window.setInterval(() => {
      setNow(Date.now());
      if (!action.current && document.visibilityState === "visible") void refresh();
    }, 15000);
    const visible = () => { if (document.visibilityState === "visible" && !action.current) { setNow(Date.now()); void refresh(); } };
    document.addEventListener("visibilitychange", visible);
    return () => { window.clearInterval(timer); document.removeEventListener("visibilitychange", visible); };
  }, [active, operationBusy, serverReady, refresh]);
  const stale = !active || !serverReady || operationBusy || observation?.at == null || Math.max(now, Date.now()) - observation.at > 30000;
  const capacity = runnerCapacity(observation?.runner, stale);
  const saved = settings?.max_concurrent_jobs;
  const savedLimit = saved === undefined ? null : saved ?? 4;
  const savedDisplay = saved == null ? "—" : saved;
  const limit = parseJobConcurrency(draft ?? "");
  const canEdit = state.topology?.runner?.kind === "local" && Boolean(settings?.can_restart) && saved !== undefined;
  const disabled = busy || operationBusy || !canEdit;
  const changed = limit !== null && limit !== savedLimit;
  const run = async (work: () => Promise<void>) => {
    if (action.current || disabled) return;
    action.current = true; setBusy(true); setError(null);
    // A pre-action read must never overwrite the post-action observation.
    generation.current++;
    try { await work(); }
    catch (value) { if (alive.current) setError(normalizeDesktopError(value)); }
    finally {
      if (alive.current) { setObservation(null); await refresh(true); setBusy(false); }
      action.current = false;
    }
  };
  const save = () => run(async () => {
    if (!settings || saved === undefined || limit === null || !changed) return;
    const next = await desktopApi.saveRunnerJobConcurrency(settings.target, editBase.current === undefined ? saved : editBase.current, limit);
    editBase.current = undefined;
    if (alive.current) onState(next);
  });
  const restart = () => run(async () => {
    if (!confirm) return;
    const current = await desktopApi.runnerSettings();
    if (!sameTarget(current.target, confirm.target) || current.max_concurrent_jobs !== confirm.max_concurrent_jobs || !current.can_restart) {
      setConfirm(null);
      throw { code: "runner_settings_changed", message: s("Runner settings changed. Review them before restarting."), next_action: s("Refresh") };
    }
    if (!alive.current) return;
    const next = await desktopApi.restartOwnedRunner(confirm.target);
    if (alive.current) { setConfirm(null); onState(next); }
  });
  const errorCard = settingsUnavailable
    ? <div className="error-card" role="alert">{s("Saved Runner settings could not be read or verified. Refresh before changing concurrency.")}</div>
    : error && <div className="error-card" role="alert"><strong>{error.message}</strong><span>{error.next_action}</span></div>;
  return <section className="settings-section" aria-labelledby="runner-capacity-title">
    <h2 id="runner-capacity-title">{s("Runner Job capacity")}</h2>
    <p className="field-help">{s("Server-reported durable Job capacity. Stop-requested Jobs remain in the running count until termination because they still occupy execution slots. Other request queues are separate.")}</p>
    <RunnerCapacitySummary capacity={capacity} />
    {!stale && observation?.runner.jobs && <section className="workspace-section" aria-label={s("Runner Jobs")}>
      <h3>{s("Runner Jobs")}</h3>
      <p className="field-help">{s("Read-only Job inventory. Stop controls require a separately authorized action.")}</p>
      {observation.runner.jobs.length === 0
        ? <p>{s("No retained Jobs in this inventory.")}</p>
        : <ul className="changed-files" data-testid="runner-job-inventory">{observation.runner.jobs.map(job => <li key={job.job_id}>
            <span><strong>{job.kind}</strong> · <code>{job.job_id}</code> · {job.status}
              {job.elapsed_secs !== undefined && ` · ${job.elapsed_secs}s`}
              {job.project_id && <small> · {job.project_id}</small>}
              {job.session_id && <small> · {job.session_id}</small>}
            </span>
          </li>)}</ul>}
      {observation.runner.jobs_truncated && <p className="field-help">{s("Inventory incomplete; some Jobs are not shown.")}</p>}
    </section>}
    {loading ? <p role="status">{s("Loading…")}</p> : <>
      <p>{s("Saved limit")}: {savedDisplay} · {s("Default")}: 4</p>
      {saved != null && savedLimit !== null && <p role="status">{s(capacity.state !== "available" ? "Saved for the next Runner start; the effective limit is unknown." : capacity.limit !== savedLimit ? "Restart required to apply the saved limit." : "Saved limit is in effect.")}</p>}
      {saved !== undefined && <div className="field-group"><label htmlFor="runner-job-concurrency">{s("Maximum concurrent Jobs")}</label>
        <input id="runner-job-concurrency" type="number" min={1} max={64} step={1} value={draft ?? ""} onChange={event => {
          editBase.current = event.target.value === String(savedLimit) ? undefined : editBase.current === undefined ? saved : editBase.current;
          setDraft(event.target.value);
        }} disabled={disabled} aria-describedby="runner-job-concurrency-help" />
        <p id="runner-job-concurrency-help" className="field-help">{s("Choose a whole number from 1 to 64. Saving does not restart the Runner.")}</p>
      </div>}
      {!canEdit && !settingsUnavailable && <p className="field-help">{s("Read-only. Change and restart this Runner through its actual process owner.")}</p>}
      <div className="connection-actions">
        {saved !== undefined && <button type="button" className="secondary-button" disabled={disabled || !changed} onClick={() => void save()}>{s("Save for next restart")}</button>}
        {canEdit && <button type="button" className="secondary-button" disabled={disabled || changed || limit === null} onClick={() => setConfirm(settings)}>{s("Restart Runner…")}</button>}
        <button type="button" className="text-button" disabled={busy || operationBusy} onClick={() => void refresh(true, true)}>{s("Refresh")}</button>
      </div>
      <p className="field-help">{s("Refresh reloads the saved limit and discards unsaved edits.")}</p>
    </>}
    {!confirm && errorCard}
    {confirm && <WorkspaceDialog title={s("Restart Runner?")} onClose={() => setConfirm(null)} busy={busy}>
      <p>{s("Restarting applies the saved Runner configuration and may interrupt running or queued Jobs, browser sessions and handoffs. Save your work before continuing.")}</p>
      <RunnerCapacitySummary capacity={capacity} />
      {errorCard}
      <div className="connection-actions"><button type="button" className="secondary-button" disabled={busy} onClick={() => setConfirm(null)}>{s("Cancel")}</button>
        <button type="button" className="primary-button" disabled={disabled} onClick={() => void restart()}>{s("Restart Runner now")}</button></div>
    </WorkspaceDialog>}
  </section>;
}
