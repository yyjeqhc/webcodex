import { useState } from "react";
import type { DesktopState } from "../../models/topology";
import { desktopApi } from "../../lib/desktop-api";
import { useShellText } from "../../i18n/runtime-shell";
import { normalizeDesktopError } from "../../i18n/presentation";

export function ReadinessBanner({ state, onState, onDiagnostics, onRuntime, onConnection }: { state: DesktopState; onState: (state: DesktopState) => void; onDiagnostics: () => void; onRuntime: () => void; onConnection: () => void }) {
  const s = useShellText(); const [busy, setBusy] = useState(false); const [error, setError] = useState<string | null>(null);
  const starting = state.readiness.server === "starting" || state.readiness.runner === "connecting";
  const stopped = state.readiness.server === "stopped" && state.readiness.runner === "stopped";
  const runtimeReady = state.readiness.runtime_ready;
  const projectReady = state.readiness.project === "ready";
  const noProject = state.readiness.project === "none";
  const connectionProblem = (state.connections?.needs_attention ?? 0) > 0 || ["error", "degraded"].includes(state.readiness.exposure);
  const healthy = runtimeReady && (projectReady || noProject) && !connectionProblem && !state.configuration_issue;
  const title = state.configuration_issue ? "Configuration could not be migrated" : healthy ? "WebCodex Ready" : starting ? "Runtime is starting" : stopped ? "Runtime is stopped" : "Needs attention";
  const run = async (action: () => Promise<DesktopState>) => {
    if (busy || state.current_operation) return; setBusy(true); setError(null);
    try { onState(await action()); } catch (value) { setError(normalizeDesktopError(value).code); } finally { setBusy(false); }
  };
  const disabled = busy || Boolean(state.current_operation);
  const runnerProblem = state.readiness.server === "ready" && ["offline", "error"].includes(state.readiness.runner);
  const failedConnections = state.connections?.profiles.filter(profile => profile.enabled && (profile.lifecycle === "error" || profile.health === "degraded")) ?? [];
  const restartRunner = () => run(async () => {
    const current = await desktopApi.runnerSettings();
    if (!current.can_restart) throw { code: "runner_not_owned", message: "Runner is not managed by Desktop", next_action: "Open Diagnostics" };
    return desktopApi.restartOwnedRunner(current.target);
  });
  return <section className={`readiness-banner ${healthy ? "ready" : "attention"}`} aria-label={s("WebCodex Ready")}>
    <div><h2>{s(title)}</h2><dl className="runtime-facts"><div><dt>{s("Runtime")}</dt><dd>{runtimeReady ? s("Compatible") : s(starting ? "Runtime is starting" : stopped ? "Runtime is stopped" : "Runtime unavailable")}</dd></div>
      <div><dt>{s("Project")}</dt><dd>{noProject ? s("No default project") : projectReady ? s("Ready") : s("Needs attention")}</dd></div>
      <div><dt>{s("ChatGPT connection")}</dt><dd>{state.chatgpt_activity?.observed ? s("Observed") : s("Not observed")}</dd></div></dl></div>
    {!healthy && <div className="shell-actions">
      {!runtimeReady && stopped && !state.configuration_issue && <button type="button" className="primary-button" disabled={disabled} onClick={() => void run(desktopApi.resumeSavedRuntime)}>{s("Start Runtime")}</button>}
      {runnerProblem && <button type="button" className="primary-button" disabled={disabled} onClick={() => void restartRunner()}>{s("Restart Runner")}</button>}
      {failedConnections.length === 1 && <button type="button" className="secondary-button" disabled={disabled} onClick={() => void run(() => desktopApi.tunnelProfileAction(failedConnections[0].id, "restart"))}>{s("Restart Tunnel")}</button>}
      {runtimeReady && !projectReady && state.project?.path && <button type="button" className="primary-button" disabled={disabled} onClick={() => void run(() => desktopApi.activateLocalProject(state.project!.path))}>{s("Reactivate project")}</button>}
      {connectionProblem && <button type="button" className="secondary-button" disabled={disabled} onClick={onConnection}>{s("Check connection")}</button>}
      {!runtimeReady && <button type="button" className="secondary-button" onClick={onRuntime}>{s("Select Runtime folder…")}</button>}
      <button type="button" className="secondary-button" onClick={onDiagnostics}>{s("Diagnostics")}</button>
    </div>}
    {error && <p role="alert"><code>{error}</code> <button type="button" className="text-button" onClick={onDiagnostics}>{s("Diagnostics")}</button></p>}
  </section>;
}
