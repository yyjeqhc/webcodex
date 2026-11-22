import type { ReactNode } from "react";
import { useLocale } from "../i18n/locale";
import { useProduct } from "../i18n/product";
import { desktopCommandDiagnostics } from "../i18n/presentation";
import {
  isSafeDisplayMessage,
  resolveDesktopError,
  SAFE_ACTION_LABEL_KEYS,
} from "../lib/desktop-errors";
import type { DesktopError, SafeRecoveryAction } from "../models/topology";

export interface DesktopErrorActions {
  onRetry?: () => void;
  onRefresh?: () => void;
  onStartRuntime?: () => void;
  onStartServer?: () => void;
  onRestartServer?: () => void;
  onStartRunner?: () => void;
  onRestartRunner?: () => void;
  onRepairRunnerCredential?: () => void;
  onRepairUserCredential?: () => void;
  onCheckConnection?: () => void;
  onOpenNetwork?: () => void;
  onOpenRuntime?: () => void;
  onOpenDiagnostics?: () => void;
  onOpenActivity?: () => void;
  onOpenProjects?: () => void;
  onOpenSetup?: () => void;
  onRestoreConfiguration?: () => void;
  onManageProviders?: () => void;
}

export interface DesktopErrorAlertProps {
  error: DesktopError;
  actions?: DesktopErrorActions;
  disabled?: boolean;
  showSafeMessage?: boolean;
  className?: string;
  id?: string;
  extraActions?: ReactNode;
}

type ResolvedButton = {
  action: Exclude<SafeRecoveryAction, "none">;
  handler: () => void;
  primary: boolean;
};

/**
 * Strictly allowlisted mapping from semantic SafeRecoveryAction to host callbacks.
 * Never executes arbitrary Server-supplied strings and never maps retry-blocked
 * errors to blind retry handlers.
 */
function resolveActionHandler(
  action: SafeRecoveryAction,
  retryAllowed: boolean,
  actions?: DesktopErrorActions,
): (() => void) | undefined {
  if (!actions || action === "none") return undefined;
  switch (action) {
    case "retry":
      return retryAllowed ? actions.onRetry : actions.onRefresh ?? actions.onOpenDiagnostics;
    case "wait_auto_recovery":
    case "refresh_state":
    case "observe_before_retry":
      return actions.onRefresh ?? actions.onOpenDiagnostics;
    case "start_runtime":
      return actions.onStartRuntime ?? actions.onOpenRuntime;
    case "start_server":
      return actions.onStartServer ?? actions.onStartRuntime ?? actions.onOpenRuntime;
    case "restart_server":
      return actions.onRestartServer ?? actions.onOpenRuntime;
    case "start_runner":
      return actions.onStartRunner ?? actions.onRestartRunner ?? actions.onStartRuntime ?? actions.onOpenRuntime;
    case "restart_runner":
      return actions.onRestartRunner ?? actions.onOpenRuntime;
    case "repair_runner_credential":
      return actions.onRepairRunnerCredential ?? actions.onOpenRuntime;
    case "repair_user_credential":
      return actions.onRepairUserCredential ?? actions.onOpenDiagnostics;
    case "check_connection":
      return actions.onCheckConnection ?? actions.onRefresh;
    case "open_network_settings":
      return actions.onOpenNetwork ?? actions.onCheckConnection;
    case "open_runtime_settings":
      return actions.onOpenRuntime;
    case "open_diagnostics":
    case "inspect_service_ownership":
      return actions.onOpenDiagnostics;
    case "open_activity":
      return actions.onOpenActivity;
    case "open_projects":
      return actions.onOpenProjects;
    case "open_setup":
    case "resolve_migration":
      return actions.onOpenSetup ?? actions.onOpenDiagnostics;
    case "restore_configuration":
      return actions.onRestoreConfiguration ?? actions.onOpenDiagnostics;
    case "manage_providers":
      return actions.onManageProviders;
    default:
      return undefined;
  }
}

export function DesktopErrorAlert({
  error,
  actions,
  disabled = false,
  showSafeMessage = false,
  className,
  id,
  extraActions,
}: DesktopErrorAlertProps) {
  const { t } = useLocale();
  const p = useProduct();
  const resolved = resolveDesktopError(error, t);
  const { diagnostics } = resolved;
  const commandDiag = desktopCommandDiagnostics(error);

  const buttons: ResolvedButton[] = [];
  const addedHandlers = new Set<() => void>();

  const tryPushButton = (action: SafeRecoveryAction | undefined, primary: boolean) => {
    if (!action || action === "none") return;
    if (action === "retry" && !diagnostics.retryAllowed) return;
    const handler = resolveActionHandler(action, diagnostics.retryAllowed, actions);
    if (!handler || addedHandlers.has(handler)) return;
    addedHandlers.add(handler);
    buttons.push({ action, handler, primary });
  };

  tryPushButton(diagnostics.safeAction, diagnostics.recoveryKind !== "auto_recovering");
  tryPushButton(resolved.secondaryAction, false);

  const tone = diagnostics.recoveryKind === "auto_recovering"
    ? "info"
    : diagnostics.recoveryKind === "confirmation_required"
      ? "warning"
      : "error";

  const originLabel = t(
    diagnostics.origin === "trusted_structured"
      ? "error.diag.originTrusted"
      : diagnostics.origin === "trusted_partial"
        ? "error.diag.originPartial"
        : diagnostics.origin === "ipc_transport"
          ? "error.diag.originIpc"
          : "error.diag.originUnknown",
  );

  const stageValue = diagnostics.stage ?? commandDiag?.phase;
  const exitCodeValue = diagnostics.exitCode ?? commandDiag?.exitCode;
  const reasonValue = diagnostics.reasonCode ?? commandDiag?.reasonCode;

  const canShowMessage = showSafeMessage
    && diagnostics.origin === "trusted_structured"
    && isSafeDisplayMessage(error.message);

  return (
    <div
      id={id}
      className={`error-card desktop-error-alert tone-${tone}${className ? ` ${className}` : ""}`}
      role="alert"
      data-error-code={diagnostics.code}
      data-error-component={diagnostics.component}
      data-error-recovery={diagnostics.recoveryKind}
    >
      {/* Layer 1: Concise localized cause, component & status */}
      <div className="desktop-error-header">
        <span className="desktop-error-badge component">{resolved.componentLabel}</span>
        <span className={`desktop-error-badge recovery ${tone}`}>{resolved.recoveryLabel}</span>
      </div>
      <strong>{resolved.title}</strong>
      {resolved.stageLabel && (
        <span className="desktop-error-stage">
          {t("error.stageSummary", { stage: resolved.stageLabel })}
        </span>
      )}
      <span>{resolved.action}</span>
      {!diagnostics.retryAllowed && (diagnostics.recoveryKind === "manual_check" || diagnostics.recoveryKind === "confirmation_required") && (
        <span className="desktop-error-guard">{t("error.diag.retryBlocked")}</span>
      )}

      {/* Layer 2: Allowlisted real recovery actions */}
      {(buttons.length > 0 || extraActions) && (
        <div className="shell-actions desktop-error-actions">
          {buttons.map((button) => (
            <button
              key={button.action}
              type="button"
              className={button.primary ? "primary-button" : "secondary-button"}
              disabled={disabled}
              onClick={button.handler}
              data-webcodex-recovery-action={button.action}
            >
              {t(SAFE_ACTION_LABEL_KEYS[button.action])}
            </button>
          ))}
          {extraActions}
        </div>
      )}

      {/* Layer 3: Expandable safe technical diagnostics */}
      <details className="desktop-error-details">
        <summary>{t("common.details")}</summary>
        <code>{diagnostics.code}</code>
        {canShowMessage && <p>{error.message}</p>}
        <dl className="error-diagnostics">
          <dt>{t("error.diag.component")}</dt>
          <dd><code>{diagnostics.component}</code></dd>
          {stageValue && (
            <>
              <dt>{p("errorPhase")}</dt>
              <dd><code>{stageValue}</code></dd>
            </>
          )}
          {diagnostics.httpStatus !== undefined && (
            <>
              <dt>{t("error.diag.httpStatus")}</dt>
              <dd><code>HTTP {diagnostics.httpStatus}</code></dd>
            </>
          )}
          {exitCodeValue !== undefined && (
            <>
              <dt>{p("runtimeExitCode")}</dt>
              <dd><code>{exitCodeValue}</code></dd>
            </>
          )}
          {reasonValue && (
            <>
              <dt>{p("errorReason")}</dt>
              <dd><code>{reasonValue}</code></dd>
            </>
          )}
          {commandDiag?.logicalCommand && (
            <>
              <dt>{p("errorCommand")}</dt>
              <dd><code>{commandDiag.logicalCommand}</code></dd>
            </>
          )}
          {commandDiag?.executable && (
            <>
              <dt>{p("errorExecutable")}</dt>
              <dd><code>{commandDiag.executable}</code></dd>
            </>
          )}
          {diagnostics.operationId && (
            <>
              <dt>{t("error.diag.operationId")}</dt>
              <dd><code>{diagnostics.operationId}</code></dd>
            </>
          )}
          {diagnostics.activitySequence !== undefined && (
            <>
              <dt>{t("error.diag.activityRef")}</dt>
              <dd><code>#{diagnostics.activitySequence}</code></dd>
            </>
          )}
          {diagnostics.resourceId && (
            <>
              <dt>{t("error.diag.resourceId")}</dt>
              <dd><code>{diagnostics.resourceId}</code></dd>
            </>
          )}
          <dt>{t("error.diag.origin")}</dt>
          <dd>{originLabel}</dd>
        </dl>
      </details>
    </div>
  );
}
