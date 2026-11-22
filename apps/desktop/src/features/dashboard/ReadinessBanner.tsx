import { useEffect, useRef, useState } from "react";
import type { DesktopError, DesktopState } from "../../models/topology";
import { desktopApi } from "../../lib/desktop-api";
import { desktopIdentity } from "../../lib/desktop-observation";
import { extractSafeErrorDiagnostics, isErrorStaleForState } from "../../lib/desktop-errors";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { useShellText } from "../../i18n/runtime-shell";
import { normalizeDesktopError } from "../../i18n/presentation";
import { DesktopErrorAlert } from "../../components/DesktopErrorAlert";

export function ReadinessBanner({
  state,
  onState,
  onDiagnostics,
  onRuntime,
  onConnection,
  onProviders,
  onActivity,
  onProjects,
  onSetup,
  onRefresh,
}: {
  state: DesktopState;
  onState: (state: DesktopState) => void;
  onDiagnostics: () => void;
  onRuntime: () => void;
  onConnection: () => void;
  onProviders?: (kind: "mcp" | "acp") => void;
  onActivity?: () => void;
  onProjects?: () => void;
  onSetup?: () => void;
  onRefresh?: () => void;
}) {
  const p = useProduct();
  const { t } = useLocale();
  const submitting = useRef(false);
  const s = useShellText();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const identity = desktopIdentity(state);
  const errorIdentityRef = useRef<string | null>(null);

  useEffect(() => {
    setError((current) => {
      if (!current) return null;
      if (!state.runtime_error && state.readiness.runtime_ready && extractSafeErrorDiagnostics(current).retryAllowed) {
        return null;
      }
      return isErrorStaleForState(current, state, errorIdentityRef.current, identity) ? null : current;
    });
  }, [state, identity]);

  const failure = error ?? state.runtime_error;
  const provider = failure?.details as { provider_kind?: string; provider_name?: string; provider_config_path?: string } | undefined;
  const providerKind = provider?.provider_kind === "mcp" ? "mcp" : provider?.provider_kind === "acp" ? "acp" : null;
  const starting = state.readiness.server === "starting" || state.readiness.runner === "connecting";
  const stopped = state.readiness.server === "stopped";
  const runtimeReady = state.readiness.runtime_ready;
  const observing = state.readiness.server === "unknown" || (state.readiness.server === "ready" && state.readiness.runner === "unknown");
  const title = state.configuration_issue ? s("Configuration could not be migrated") : failure ? s("Needs attention")
    : starting ? t("first.preparing") : observing ? t("common.checking") : stopped ? "WebCodex · " + p("stopped") : s("Needs attention");

  const run = async (action: () => Promise<DesktopState>) => {
    if (submitting.current || state.current_operation) return;
    submitting.current = true;
    const expectedIdentity = identity;
    setBusy(true);
    setError(null);
    try {
      onState(await action());
    } catch (value) {
      errorIdentityRef.current = expectedIdentity;
      setError(normalizeDesktopError(value));
      try {
        onState(await desktopApi.getState());
      } catch {
        /* Keep the precise start error if observation also fails. */
      }
    } finally {
      submitting.current = false;
      setBusy(false);
    }
  };

  const disabled = busy || Boolean(state.current_operation);
  const runnerProblem = state.readiness.server === "ready" && state.topology?.runner.kind === "local" && ["stopped", "error"].includes(state.readiness.runner);
  const restartRunner = () => run(async () => {
    if (state.persistent_environment) return desktopApi.environmentServiceAction({
      environmentId: state.persistent_environment, component: "runner", action: state.readiness.runner === "stopped" ? "start" : "restart",
    });
    const current = await desktopApi.runnerSettings();
    if (!current.can_restart) throw { code: "runner_not_owned", message: "Runner is not managed by Desktop", next_action: "Open Diagnostics" };
    return desktopApi.restartOwnedRunner(current.target);
  });

  if (runtimeReady && !failure && !state.configuration_issue) return null;
  return <section className="readiness-banner attention" aria-label="WebCodex">
    <h2 role="status">{title}</h2>
    {!failure && !starting && !state.current_operation && (
      state.configuration_issue ? <button type="button" className="primary-button" onClick={onDiagnostics}>{p("troubleshooting")}</button>
      : state.readiness.next_action_kind === "restart_quick_share" ? <button type="button" className="primary-button" onClick={onSetup}>{p("restart")} Quick Share</button>
      : runnerProblem ? <button type="button" className="primary-button" disabled={disabled} onClick={() => void restartRunner()}>{s("Restart Runner")}</button>
      : stopped && state.topology?.server.kind !== "remote" ? <button type="button" className="primary-button" disabled={disabled} onClick={() => void run(desktopApi.resumeSavedRuntime)}>{p("start")} WebCodex</button>
      : <button type="button" className="secondary-button" disabled={disabled} onClick={observing ? onRefresh ?? (() => void run(desktopApi.refresh)) : onDiagnostics}>{observing ? p("refresh") : p("troubleshooting")}</button>
    )}
    {failure && (providerKind ? <div role="alert" className="runtime-start-error">
      <strong>{providerKind.toUpperCase()} · {provider?.provider_name}</strong><p>{p("providerProgramUnavailable")}</p>{provider?.provider_config_path ? <><p>{p("providerConfigRecoveryHelp")}</p><code>{provider.provider_config_path}</code></> : <p>{p("providerRecoveryHelp")}</p>}{onProviders && !provider?.provider_config_path && <button type="button" className="secondary-button" onClick={() => onProviders(providerKind)}>{p("manageFailedProvider")}</button>}
      <code>{failure.code}</code>
    </div> : <DesktopErrorAlert
      error={failure}
      showSafeMessage
      disabled={disabled}
      className="runtime-start-error"
      actions={{
        onRefresh: onRefresh ?? (() => void run(desktopApi.refresh)),
        onStartRuntime: () => void run(desktopApi.resumeSavedRuntime),
        onStartServer: () => void run(desktopApi.resumeSavedRuntime),
        onRestartServer: onRuntime,
        onStartRunner: () => void run(desktopApi.resumeSavedRuntime),
        onRestartRunner: () => void restartRunner(),
        onRepairRunnerCredential: onRuntime,
        onRepairUserCredential: onDiagnostics,
        onCheckConnection: onConnection,
        onOpenRuntime: onRuntime,
        onOpenDiagnostics: onDiagnostics,
        onOpenActivity: onActivity,
        onOpenProjects: onProjects,
        onOpenSetup: onSetup,
        onRestoreConfiguration: onDiagnostics,
      }}
    />)}
  </section>;
}
