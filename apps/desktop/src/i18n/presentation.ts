import type {
  ActivityEntry,
  DesktopState,
  DesktopOperationKind,
  DesktopError,
  ProjectReadiness,
  ReadinessNextActionKind,
  ReadinessSummaryKind,
  RunnerReadiness,
  ServerReadiness,
} from "../models/topology";
import type { MessageKey } from "./locale";

type Translate = (key: MessageKey, params?: Record<string, string | number>) => string;

const summaryKeys: Record<ReadinessSummaryKind, MessageKey> = {
  ready_for_chat_gpt: "readiness.readyForChatGpt",
  runtime_stopped: "common.stopped",
  runtime_starting: "common.starting",
  service_needs_attention: "readiness.serviceNeedsAttention",
  runner_disconnected: "readiness.runnerDisconnected",
  project_not_ready: "readiness.projectNotReady",
  runtime_ready_local_only: "readiness.runtimeReadyLocalOnly",
  tunnel_ready_waiting_for_chat_gpt: "readiness.tunnelReadyWaitingForChatGpt",
  connection_unverified: "readiness.connectionUnverified",
  quick_share_stopped: "readiness.quickShareStopped",
};

const nextActionKeys: Record<ReadinessNextActionKind, MessageKey> = {
  start_or_reconnect_service: "action.startOrReconnectService",
  start_runner: "action.startRunner",
  add_or_reload_project: "action.addOrReloadProject",
  choose_connection: "action.chooseConnection",
  check_connection: "action.checkConnection",
  restart_quick_share: "action.restartQuickShare",
  restore_clipboard_handoff: "action.restoreClipboard",
  restart_secure_tunnel: "action.restartSecureTunnel",
};

export function readinessSummary(
  kind: ReadinessSummaryKind,
  fallback: string,
  t: Translate,
) {
  return summaryKeys[kind] ? t(summaryKeys[kind]) : fallback;
}

export function readinessNextAction(
  kind: ReadinessNextActionKind | null | undefined,
  fallback: string | null | undefined,
  t: Translate,
) {
  return kind ? t(nextActionKeys[kind]) : fallback ?? null;
}

const activityKeys: Record<ActivityEntry["event_kind"], MessageKey> = {
  process_started: "activity.processStarted",
  process_exited: "activity.processExited",
  process_observation_failed: "activity.processObservationFailed",
  process_stopping: "activity.processStopping",
  process_stopped: "activity.processStopped",
  local_setup_preparing: "activity.localSetupPreparing",
  local_runtime_ready: "activity.localRuntimeReady",
  remote_connecting: "activity.remoteConnecting",
  remote_connected: "activity.remoteConnected",
  quick_share_starting: "activity.quickShareStarting",
  quick_share_ready: "activity.quickShareReady",
  quick_share_stopped: "activity.quickShareStopped",
  regular_tunnel_starting: "activity.regularTunnelStarting",
  regular_tunnel_ready: "activity.regularTunnelReady",
  regular_tunnel_stopped: "activity.regularTunnelStopped",
  runtime_stopped: "activity.runtimeStopped",
  project_activated: "activity.projectActivated",
  state_recovered: "activity.stateRecovered",
  operation_started: "activity.operationStarted",
  operation_cancel_requested: "activity.operationCancelRequested",
  operation_cancelled: "activity.operationCancelled",
  operation_failed: "activity.operationFailed",
};

export function activityMessage(entry: ActivityEntry, t: Translate) {
  if (entry.event_kind === "project_activated") return t("activity.projectActivated", { project: entry.message });
  if (entry.event_kind === "operation_started") {
    const kind = entry.message.split(": ")[1];
    if (Object.hasOwn(operationKeys, kind)) return `${t("activity.operationStarted")} · ${operationLabel(kind as DesktopOperationKind, t)}`;
  }
  return activityKeys[entry.event_kind] ? t(activityKeys[entry.event_kind]) : entry.message;
}

export function activitySource(source: string, t: Translate) {
  const key = `activity.source.${source}` as MessageKey;
  return key in {
    "activity.source.desktop": true,
    "activity.source.service": true,
    "activity.source.runner": true,
    "activity.source.quick_share": true,
    "activity.source.regular_tunnel": true,
  } ? t(key) : source.replaceAll("_", " ");
}

type ErrorPresentation = { title: string; action: string };

export type DesktopCommandDiagnostics = {
  phase?: string;
  logicalCommand?: string;
  executable?: string;
  exitCode?: number;
  reasonCode?: string;
};

const binaryErrors = new Set([
  "binaries_not_checked",
  "binary_directory_invalid",
  "binary_directory_missing",
  "binary_missing",
  "binary_version_mismatch",
  "binary_version_unverifiable",
  "binary_probe_failed",
]);
const serverErrors = new Set([
  "server_unreachable",
  "server_start_failed",
  "server_unavailable",
  "local_port_unavailable",
]);
const runnerErrors = new Set(["runner_offline"]);
const projectErrors = new Set([
  "project_unavailable",
  "project_not_directory",
  "project_not_loaded",
  "project_not_ready",
]);
const enrollmentErrors = new Set([
  "webcodex_command_failed",
  "webcodex_command_start_failed",
  "webcodex_command_input_failed",
  "webcodex_command_wait_failed",
  "webcodex_command_timeout",
]);
const commandPhasePresentation: Record<string, { title: MessageKey; action: MessageKey }> = {
  server_init: { title: "error.serverTitle", action: "error.serverAction" },
  server_status: { title: "error.serverTitle", action: "error.serverAction" },
  pairing_create: { title: "error.enrollmentTitle", action: "error.enrollmentAction" },
  login: { title: "error.enrollmentTitle", action: "error.enrollmentAction" },
  runner_status: { title: "error.runnerTitle", action: "error.runnerAction" },
  project_activation: { title: "error.projectTitle", action: "error.projectAction" },
  project_register: { title: "error.projectTitle", action: "error.projectAction" },
  project_readiness: { title: "error.projectTitle", action: "error.projectAction" },
};
const tunnelErrors = new Set([
  "tunnel_unavailable",
  "tunnel_auth_invalid",
  "regular_tunnel_not_ready",
  "regular_tunnel_already_running",
  "quick_share_not_ready",
]);
const processErrors = new Set([
  "process_start_failed",
  "process_already_running",
  "quick_share_already_running",
]);

export function desktopErrorPresentation(error: DesktopError, t: Translate): ErrorPresentation {
  if (error.code === "tunnel_config_apply_failed") return { title: t("error.tunnelTitle"), action: t("tunnelConfig.applyFailed") };
  if (error.code === "tunnel_config_invalid") return { title: t("error.tunnelTitle"), action: t("tunnelConfig.invalidInput") };
  if (error.code === "tunnel_config_save_failed") return { title: t("error.fallbackTitle"), action: t("tunnelConfig.saveFailed") };
  if (binaryErrors.has(error.code)) return { title: t("error.binaryTitle"), action: t("error.binaryAction") };
  if (serverErrors.has(error.code) || error.code === "server_url_invalid") return { title: t("error.serverTitle"), action: t("error.serverAction") };
  if (error.code === "runtime_not_ready") return { title: t("error.runtimeTitle"), action: t("error.runtimeAction") };
  if (runnerErrors.has(error.code)) return { title: t("error.runnerTitle"), action: t("error.runnerAction") };
  if (projectErrors.has(error.code)) return { title: t("error.projectTitle"), action: t("error.projectAction") };
  if (error.code === "pairing_code_invalid") return { title: t("error.pairingTitle"), action: t("error.pairingAction") };
  if (enrollmentErrors.has(error.code)) {
    const phase = desktopCommandDiagnostics(error)?.phase;
    const phasePresentation = phase ? commandPhasePresentation[phase] : undefined;
    if (phasePresentation) {
      return { title: t(phasePresentation.title), action: t(phasePresentation.action) };
    }
    return { title: t("error.enrollmentTitle"), action: t("error.enrollmentAction") };
  }
  if (tunnelErrors.has(error.code)) return { title: t("error.tunnelTitle"), action: t("error.tunnelAction") };
  if (processErrors.has(error.code)) return { title: t("error.processTitle"), action: t("error.processAction") };
  if (error.code === "webcodex_contract_invalid") return { title: t("error.contractTitle"), action: t("error.contractAction") };
  if (error.code === "unsupported_topology" || error.code === "quick_share_provider_invalid") return { title: t("error.topologyTitle"), action: t("error.topologyAction") };
  if (error.code === "desktop_operation_busy" || error.code === "desktop_operation_not_cancellable") return { title: t("error.operationBusyTitle"), action: t("error.operationBusyAction") };
  if (error.code === "desktop_operation_not_current") return { title: t("error.operationStaleTitle"), action: t("error.operationStaleAction") };
  if (error.code === "desktop_operation_cancelled") return { title: t("error.operationCancelledTitle"), action: t("error.operationCancelledAction") };
  return { title: t("error.fallbackTitle"), action: t("error.fallbackAction") };
}

export function desktopCommandDiagnostics(error: DesktopError): DesktopCommandDiagnostics | null {
  if (!error.details || typeof error.details !== "object" || Array.isArray(error.details)) return null;
  const details = error.details as Record<string, unknown>;
  const phase = safeDiagnosticString(details.phase, /^[a-z0-9_-]+$/);
  const logicalCommand = safeDiagnosticString(details.logical_command, /^[a-z0-9 _-]+$/);
  const executable = safeDiagnosticString(details.executable, /^[A-Za-z0-9._-]+$/);
  const reasonCode = safeDiagnosticString(details.reason_code, /^[a-z0-9_-]+$/);
  const exitCode = typeof details.exit_code === "number" && Number.isSafeInteger(details.exit_code)
    ? details.exit_code
    : undefined;
  if (!phase && !logicalCommand && !executable && exitCode === undefined && !reasonCode) return null;
  return { phase, logicalCommand, executable, exitCode, reasonCode };
}

function safeDiagnosticString(value: unknown, pattern: RegExp): string | undefined {
  if (typeof value !== "string" || value.length === 0 || value.length > 96 || !pattern.test(value)) return undefined;
  const lowered = value.toLowerCase();
  if (["token", "secret", "password", "credential", "api_key", "authorization"].some(sensitive => lowered.includes(sensitive))) {
    return undefined;
  }
  return value;
}

export function normalizeDesktopError(value: unknown): DesktopError {
  if (value && typeof value === "object") {
    const candidate = value as Partial<DesktopError>;
    if (candidate.code && candidate.message && candidate.next_action) {
      return candidate as DesktopError;
    }
  }
  return {
    code: "desktop_operation_failed",
    message: "Desktop could not complete the operation.",
    next_action: "Retry the operation or open Activity for safe diagnostics.",
  };
}

export function serverReadinessLabel(value: ServerReadiness, t: Translate) {
  if (value === "ready") return t("common.running");
  if (value === "stopped") return t("common.stopped");
  if (value === "starting") return t("common.starting");
  if (value === "error") return t("common.error");
  return t("common.unknown");
}

export function runnerReadinessLabel(value: RunnerReadiness, t: Translate) {
  if (value === "ready") return t("common.connected");
  if (value === "stopped") return t("common.stopped");
  if (value === "connecting") return t("common.starting");
  if (value === "error") return t("common.error");
  return t("common.unknown");
}

export function projectReadinessLabel(value: ProjectReadiness, t: Translate) {
  if (value === "ready") return t("common.ready");
  if (value === "configured") return t("common.configured");
  if (value === "reload_required") return t("readiness.projectNotReady");
  if (value === "error") return t("common.error");
  if (value === "none") return t("project.none");
  return t("common.unknown");
}

const operationKeys: Record<DesktopOperationKind, MessageKey> = {
  runtime_probe: "operation.runtimeProbe",
  runtime_switch: "operation.runtimeSwitch",
  trace_update: "operation.traceUpdate",
  configuration_restore: "operation.configurationRestore",
  local_setup: "operation.localSetup",
  local_project_activate: "operation.localProjectActivate",
  project_unregister: "operation.projectUnregister",
  remote_setup: "operation.remoteSetup",
  quick_share_start: "operation.quickShareStart",
  quick_share_stop: "operation.quickShareStop",
  regular_tunnel_start: "operation.regularTunnelStart",
  regular_tunnel_stop: "operation.regularTunnelStop",
  local_runtime_stop: "operation.localRuntimeStop",
  runtime_refresh: "operation.runtimeRefresh",
  runtime_resume: "operation.runtimeResume",
  runner_settings_update: "operation.runnerSettingsUpdate",
  runner_restart: "operation.runnerRestart",
  tunnel_config_update: "operation.tunnelConfigUpdate",
  tunnel_proxy_update: "operation.tunnelProxyUpdate",
};

export function operationLabel(kind: DesktopOperationKind, t: Translate) {
  return t(operationKeys[kind]);
}

export function runtimeLabel(state: DesktopState, t: Translate) {
  if (state.readiness.runtime_ready) return t("sidebar.runtimeReady");
  if (!state.topology) return t("sidebar.needsSetup");
  if (["runtime_resume", "local_setup", "remote_setup"].includes(state.current_operation?.kind ?? "") || state.readiness.server === "starting") return t("common.starting");
  if (state.readiness.server === "stopped" && state.readiness.runner === "stopped") return t("common.stopped");
  return readinessSummary(state.readiness.summary_kind, state.readiness.summary, t);
}
