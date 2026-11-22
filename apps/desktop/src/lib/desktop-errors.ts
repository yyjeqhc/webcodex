import type {
  DesktopError,
  DesktopState,
  ErrorComponent,
  ErrorDiagnostic,
  RecoveryKind,
  SafeRecoveryAction,
} from "../models/topology";
import { cloudflarePreparationMessages } from "../i18n/cloudflare";
import type { MessageKey } from "../i18n/locale";

export type Translate = (key: MessageKey, params?: Record<string, string | number>) => string;

export type ErrorOrigin =
  | "trusted_structured"
  | "trusted_partial"
  | "ipc_transport"
  | "unknown";

export type SafeErrorDiagnostics = {
  code: string;
  origin: ErrorOrigin;
  component: ErrorComponent;
  stage?: string;
  logicalCommand?: string;
  executable?: string;
  reasonCode?: string;
  httpStatus?: number;
  exitCode?: number;
  operationId?: string;
  generation?: number;
  resourceId?: string;
  activitySequence?: number;
  recoveryKind: RecoveryKind;
  safeAction: SafeRecoveryAction;
  retryAllowed: boolean;
};

export type ResolvedDesktopError = {
  title: string;
  action: string;
  stageLabel?: string;
  componentLabel: string;
  recoveryLabel: string;
  diagnostics: SafeErrorDiagnostics;
  secondaryAction?: SafeRecoveryAction;
};

const VALID_COMPONENTS = new Set<ErrorComponent>([
  "runtime",
  "server",
  "runner",
  "service",
  "tunnel",
  "auth",
  "project",
  "environment",
  "migration",
  "config",
  "desktop",
  "unknown",
]);

const VALID_RECOVERY_KINDS = new Set<RecoveryKind>([
  "auto_recovering",
  "user_action",
  "manual_check",
  "confirmation_required",
]);

const VALID_SAFE_ACTIONS = new Set<SafeRecoveryAction>([
  "wait_auto_recovery",
  "retry",
  "refresh_state",
  "start_runtime",
  "start_server",
  "restart_server",
  "start_runner",
  "restart_runner",
  "repair_runner_credential",
  "repair_user_credential",
  "check_connection",
  "open_network_settings",
  "open_runtime_settings",
  "open_diagnostics",
  "open_activity",
  "open_projects",
  "open_setup",
  "inspect_service_ownership",
  "resolve_migration",
  "observe_before_retry",
  "restore_configuration",
  "manage_providers",
  "none",
]);

const SENSITIVE_FRAGMENTS = [
  "token",
  "secret",
  "password",
  "credential",
  "bearer",
  "cookie",
  "api_key",
  "apikey",
  "authorization",
  "private_key",
  "session_id",
  "wc_pat_",
  "wc_agent_",
  "wc_boot_",
  "wc_acct_",
];

// The stricter reference-ID filter below must not be used for error codes:
// e.g. credential_repair_failed and tunnel_token_invalid are valid, public
// diagnostic identifiers and do not contain secret values.
const SENSITIVE_IDENTIFIER_FRAGMENTS = SENSITIVE_FRAGMENTS.filter(
  (fragment) => !["token", "credential", "authorization"].includes(fragment),
);

const errorOrigins = new WeakMap<DesktopError, ErrorOrigin>();

export function safeIdentifier(value: unknown, pattern = /^[a-z][a-z0-9_-]{1,95}$/): string | undefined {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  if (trimmed.length === 0 || trimmed.length > 96 || !pattern.test(trimmed)) return undefined;
  const lowered = trimmed.toLowerCase();
  if (SENSITIVE_IDENTIFIER_FRAGMENTS.some((fragment) => lowered.includes(fragment))) return undefined;
  return trimmed;
}

export function safeReferenceId(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  if (trimmed.length === 0 || trimmed.length > 96 || !/^[A-Za-z0-9_-]+$/.test(trimmed)) return undefined;
  const lowered = trimmed.toLowerCase();
  if (SENSITIVE_FRAGMENTS.some((fragment) => lowered.includes(fragment))) return undefined;
  return trimmed;
}

export function isSafeDisplayMessage(message: unknown): message is string {
  if (typeof message !== "string") return false;
  const trimmed = message.trim();
  if (!trimmed || trimmed.length > 600) return false;
  const lowered = trimmed.toLowerCase();
  if (
    /\b(wc_pat_|wc_agent_|wc_boot_|wc_acct_)/i.test(trimmed)
    || /\bbearer\s+\S+/i.test(trimmed)
    || /\b(authorization|cookie)\s*:/i.test(trimmed)
    || /\b(password|secret|api_key|token)\s*=\s*\S+/i.test(lowered)
  ) {
    return false;
  }
  return true;
}

function sanitizeIncomingDiagnostic(value: unknown): ErrorDiagnostic | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const raw = value as Record<string, unknown>;
  const component = typeof raw.component === "string" && VALID_COMPONENTS.has(raw.component as ErrorComponent)
    ? (raw.component as ErrorComponent)
    : undefined;
  const recoveryKind = typeof raw.recovery_kind === "string" && VALID_RECOVERY_KINDS.has(raw.recovery_kind as RecoveryKind)
    ? (raw.recovery_kind as RecoveryKind)
    : undefined;
  const safeAction = typeof raw.safe_action === "string" && VALID_SAFE_ACTIONS.has(raw.safe_action as SafeRecoveryAction)
    ? (raw.safe_action as SafeRecoveryAction)
    : undefined;
  if (!component || !recoveryKind || !safeAction) return undefined;
  const httpStatus = typeof raw.http_status === "number" && Number.isInteger(raw.http_status) && raw.http_status >= 100 && raw.http_status <= 599
    ? raw.http_status
    : undefined;
  const exitCode = typeof raw.exit_code === "number" && Number.isSafeInteger(raw.exit_code)
    ? raw.exit_code
    : undefined;
  const generation = typeof raw.generation === "number" && Number.isSafeInteger(raw.generation) && raw.generation >= 0
    ? raw.generation
    : undefined;
  const activitySequence = typeof raw.activity_sequence === "number" && Number.isSafeInteger(raw.activity_sequence) && raw.activity_sequence >= 0
    ? raw.activity_sequence
    : undefined;
  return {
    component,
    stage: safeIdentifier(raw.stage, /^[a-z0-9_-]{2,96}$/) ?? null,
    reason_code: safeIdentifier(raw.reason_code, /^[a-z0-9_-]{2,96}$/) ?? null,
    recovery_kind: recoveryKind,
    safe_action: safeAction,
    retry_allowed: raw.retry_allowed === true,
    http_status: httpStatus ?? null,
    exit_code: exitCode ?? null,
    operation_id: safeReferenceId(raw.operation_id) ?? null,
    generation: generation ?? null,
    resource_id: safeReferenceId(raw.resource_id) ?? null,
    activity_sequence: activitySequence ?? null,
  };
}

function isIpcTransportFailure(value: unknown): boolean {
  const text = typeof value === "string"
    ? value
    : value instanceof Error
      ? `${value.name}: ${value.message}`
      : null;
  if (!text) return false;
  return /(tauri|__tauri|ipc|invoke|command .+ not found|window not found|channel closed|failed to deserialize|invalid args|callback not found)/i.test(text);
}

function withOrigin(error: DesktopError, origin: ErrorOrigin): DesktopError {
  errorOrigins.set(error, origin);
  return error;
}

export function desktopErrorOrigin(error: DesktopError): ErrorOrigin {
  const known = errorOrigins.get(error);
  if (known) return known;
  if (error.code === "desktop_ipc_failed") return "ipc_transport";
  if (error.code === "desktop_unknown_error") return "unknown";
  if (error.code && error.message && error.next_action) return "trusted_structured";
  return "trusted_partial";
}

/**
 * Normalizes any thrown error into a canonical DesktopError while preserving
 * diagnostic fidelity across four distinct trust tiers:
 *  1. Trusted structured DesktopError (from Rust backend)
 *  2. Trusted partial internal error (valid safe code, missing fields)
 *  3. Tauri IPC / transport failure (never echoing raw exception strings)
 *  4. Unknown / unsafe exception (safe fail-closed fallback, zero leakage)
 */
export function normalizeDesktopError(value: unknown): DesktopError {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    const candidate = value as Record<string, unknown>;
    const safeCode = safeIdentifier(candidate.code, /^[a-z][a-z0-9_-]{1,95}$/);
    const diagnostic = sanitizeIncomingDiagnostic(candidate.diagnostic);
    if (
      safeCode
      && typeof candidate.message === "string" && candidate.message.trim().length > 0
      && typeof candidate.next_action === "string" && candidate.next_action.trim().length > 0
    ) {
      const normalized: DesktopError = {
        code: safeCode,
        message: candidate.message,
        next_action: candidate.next_action,
        ...(candidate.details !== undefined ? { details: candidate.details } : {}),
        ...(diagnostic ? { diagnostic } : {}),
      };
      return withOrigin(normalized, "trusted_structured");
    }
    if (safeCode) {
      const normalized: DesktopError = {
        code: safeCode,
        message: "Internal operation reported an incomplete error record.",
        next_action: "Open Diagnostics or Activity to inspect current status.",
        ...(candidate.details !== undefined ? { details: candidate.details } : {}),
        ...(diagnostic ? { diagnostic } : {}),
      };
      return withOrigin(normalized, "trusted_partial");
    }
  }

  if (isIpcTransportFailure(value)) {
    return withOrigin({
      code: "desktop_ipc_failed",
      message: "Desktop native bridge communication failed.",
      next_action: "Refresh Desktop state or check Activity if the issue persists.",
      diagnostic: {
        component: "desktop",
        stage: "ipc_bridge",
        reason_code: "ipc_transport_error",
        recovery_kind: "user_action",
        safe_action: "refresh_state",
        retry_allowed: true,
      },
    }, "ipc_transport");
  }

  return withOrigin({
    code: "desktop_unknown_error",
    message: "An unconfirmed error occurred.",
    next_action: "Open Diagnostics or Activity to inspect safe status details.",
    diagnostic: {
      component: "unknown",
      reason_code: "unrecognized_exception",
      recovery_kind: "manual_check",
      safe_action: "open_diagnostics",
      retry_allowed: false,
    },
  }, "unknown");
}

type CategoryRule = {
  titleKey: MessageKey;
  actionKey: MessageKey;
  component: ErrorComponent;
  recoveryKind: RecoveryKind;
  safeAction: SafeRecoveryAction;
  secondaryAction?: SafeRecoveryAction;
  retryAllowed: boolean;
};

const BINARY_CODES = new Set([
  "binaries_missing",
  "binaries_not_checked",
  "binary_directory_invalid",
  "binary_directory_missing",
  "binary_missing",
  "binary_version_mismatch",
  "binary_version_unverifiable",
  "binary_probe_failed",
  "runtime_unavailable",
  "runtime_incompatible",
  "runtime_missing",
  "runtime_binary_missing",
  "runtime_binary_invalid",
  "runtime_binary_unexecutable",
  "runtime_directory_invalid",
  "runtime_directory_missing",
  "runtime_probe_failed",
  "runtime_protocol_incompatible",
  "selected_runtime_not_active",
  "service_missingprerequisite",
  "upgrade_package_flavor",
  "installer_package_target",
  "upgrade_package_ownership",
]);

const PORT_CONFLICT_CODES = new Set([
  "server_port_in_use",
  "local_port_unavailable",
  "listen_in_use",
  "listen_address_in_use",
  "port_in_use",
]);

const SERVER_START_OR_REACH_CODES = new Set([
  "server_unreachable",
  "server_start_failed",
  "server_exited",
  "server_not_ready",
  "server_unavailable",
  "server_url_invalid",
  "server_identity_contract",
  "workspace_server_unreachable",
]);

const OWNERSHIP_CODES = new Set([
  "service_foreignservice",
  "service_ownershipunknown",
  "service_owner_conflict",
  "service_scope_conflict",
  "tunnel_owner",
  "tunnel_binding_conflict",
  "tunnel_catalog_conflict",
  "runner_not_owned",
  "runner_identity_mismatch",
  "runner_identity",
  "enrollment_binding_conflict",
  "server_not_owned",
  "server_binding_conflict",
  "server_admin_required",
]);

const UNCERTAIN_CODES = new Set([
  "operation_uncertain",
  "response_uncertain",
  "service_outcomeunknown",
  "process_stop_unconfirmed",
  "project_reconcile_required",
  "runner_config_reconcile_required",
  "pairing_recovery_required",
  "server_environment_write_unconfirmed",
  "support_bundle_write_unconfirmed",
  "inventory_write_unconfirmed",
  "inventory_export_path_unconfirmed",
  "update_outcome_uncertain",
  "legacy_stop_unconfirmed",
]);

const MIGRATION_CODES = new Set([
  "migration_required",
  "migration_conflict",
  "migration_target_conflict",
  "migration_recovery_required",
  "migration_restored",
  "legacy_owner_unknown",
  "legacy_owner_changed",
  "legacy_owner_active",
  "legacy_process_changed",
  "legacy_tunnel_owner",
  "legacy_tunnel_id",
  "legacy_tunnel_credentials",
  "legacy_runner_layout_unsupported",
  "legacy_runner_configuration",
]);

const REVISION_STALE_CODES = new Set([
  "cloudflare_revision_stale",
  "tunnel_revision_stale",
  "environment_changed",
  "environment_conflict",
  "runtime_selection_changed",
  "diagnostic_identity_changed",
  "server_environment_changed",
  "inventory_changed",
  "project_revision_changed",
  "project_operation_conflict",
  "mcp_provider_changed",
  "coding_agent_changed",
  "managed_instruction_conflict",
  "runner_settings_changed",
]);

const AUTH_CODES = new Set([
  "authentication_required",
  "workspace_authentication_required",
  "user_credential_required",
  "cloudflare_authentication_failed",
  "pairing_code_required",
]);

const PERMISSION_CODES = new Set([
  "permission_denied",
  "workspace_permission_denied",
  "user_identity_conflict",
  "pairing_user_conflict",
]);

const SERVICE_CREDENTIAL_CODES = new Set([
  "service_credentialinvalid",
  "service_credentialrequired",
  "service_logonfailure",
  "credential_repair_unavailable",
]);

const ELEVATION_CODES = new Set([
  "service_elevationrequired",
  "service_elevationcancelled",
  "service_permissiondenied",
  "elevation_required",
  "elevation_cancelled",
]);

const PROJECT_CODES = new Set([
  "project_unavailable",
  "project_not_directory",
  "project_not_loaded",
  "project_not_ready",
  "project_path",
  "project_path_invalid",
  "project_identity",
  "project_inventory",
  "local_project_required",
]);

const COMMAND_CODES = new Set([
  "webcodex_command_failed",
  "webcodex_command_start_failed",
  "webcodex_command_input_failed",
  "webcodex_command_wait_failed",
  "webcodex_command_timeout",
]);

const COMMAND_PHASE_RULES: Record<string, CategoryRule> = {
  runtime_probe: {
    titleKey: "error.binaryTitle",
    actionKey: "error.binaryAction",
    component: "runtime",
    recoveryKind: "user_action",
    safeAction: "open_runtime_settings",
    secondaryAction: "open_diagnostics",
    retryAllowed: false,
  },
  server_init: {
    titleKey: "error.serverTitle",
    actionKey: "error.serverAction",
    component: "server",
    recoveryKind: "user_action",
    safeAction: "open_diagnostics",
    secondaryAction: "open_activity",
    retryAllowed: true,
  },
  server_status: {
    titleKey: "error.serverTitle",
    actionKey: "error.serverAction",
    component: "server",
    recoveryKind: "user_action",
    safeAction: "start_server",
    secondaryAction: "open_diagnostics",
    retryAllowed: true,
  },
  pairing_create: {
    titleKey: "error.enrollmentTitle",
    actionKey: "error.enrollmentAction",
    component: "auth",
    recoveryKind: "user_action",
    safeAction: "open_diagnostics",
    retryAllowed: true,
  },
  login: {
    titleKey: "error.enrollmentTitle",
    actionKey: "error.enrollmentAction",
    component: "auth",
    recoveryKind: "user_action",
    safeAction: "repair_user_credential",
    secondaryAction: "open_diagnostics",
    retryAllowed: true,
  },
  runner_status: {
    titleKey: "error.runnerTitle",
    actionKey: "error.runnerAction",
    component: "runner",
    recoveryKind: "user_action",
    safeAction: "start_runner",
    secondaryAction: "open_diagnostics",
    retryAllowed: true,
  },
  project_activation: {
    titleKey: "error.projectTitle",
    actionKey: "error.projectAction",
    component: "project",
    recoveryKind: "user_action",
    safeAction: "open_projects",
    retryAllowed: true,
  },
  project_register: {
    titleKey: "error.projectTitle",
    actionKey: "error.projectAction",
    component: "project",
    recoveryKind: "user_action",
    safeAction: "open_projects",
    retryAllowed: true,
  },
  project_readiness: {
    titleKey: "error.projectTitle",
    actionKey: "error.projectAction",
    component: "project",
    recoveryKind: "user_action",
    safeAction: "open_projects",
    retryAllowed: true,
  },
};

function inferHttpStatusFromMessage(message: string): number | undefined {
  const match = /\(HTTP\s+(\d{3})\)/.exec(message);
  if (!match) return undefined;
  const parsed = Number(match[1]);
  return Number.isInteger(parsed) && parsed >= 100 && parsed <= 599 ? parsed : undefined;
}

function classifyRule(error: DesktopError, diag: {
  stage?: string;
  reasonCode?: string;
  httpStatus?: number;
}): CategoryRule {
  const code = error.code;

  if (Object.hasOwn(cloudflarePreparationMessages, code)) {
    return {titleKey: "error.tunnelTitle", actionKey: cloudflarePreparationMessages[code], component: "tunnel",
      recoveryKind: "user_action", safeAction: code === "cloudflare_proxy_invalid" ? "open_network_settings" : "check_connection",
      secondaryAction: "open_diagnostics", retryAllowed: ["cloudflare_network_unavailable", "cloudflare_download_failed", "cloudflare_start_failed"].includes(code)};
  }
  // Tunnel config specialized messages (preserving existing contract)
  if (code === "tunnel_config_apply_failed" || code === "tunnel_apply_failed") {
    return { titleKey: "error.tunnelTitle", actionKey: "tunnelConfig.applyFailed", component: "tunnel", recoveryKind: "user_action", safeAction: "check_connection", secondaryAction: "open_diagnostics", retryAllowed: true };
  }
  if (code === "tunnel_config_invalid") {
    return { titleKey: "error.tunnelTitle", actionKey: "tunnelConfig.invalidInput", component: "config", recoveryKind: "user_action", safeAction: "check_connection", retryAllowed: false };
  }
  if (code === "tunnel_config_save_failed") {
    return { titleKey: "error.fallbackTitle", actionKey: "tunnelConfig.saveFailed", component: "config", recoveryKind: "user_action", safeAction: "open_diagnostics", retryAllowed: true };
  }

  // Port occupied / listener bind failure (including structured reason_code on server_start_failed)
  if (PORT_CONFLICT_CODES.has(code) || diag.reasonCode === "local_port_unavailable") {
    return {
      titleKey: "error.serverPortTitle",
      actionKey: "error.serverPortAction",
      component: "server",
      recoveryKind: "user_action",
      safeAction: "open_diagnostics",
      secondaryAction: "open_activity",
      retryAllowed: false,
    };
  }

  // Ownership conflicts (never force takeover / never blind retry)
  if (OWNERSHIP_CODES.has(code)) {
    return {
      titleKey: "error.ownershipTitle",
      actionKey: "error.ownershipAction",
      component: code.startsWith("tunnel_") ? "tunnel" : code.startsWith("runner_") ? "runner" : code.startsWith("server_") ? "server" : "service",
      recoveryKind: "manual_check",
      safeAction: "inspect_service_ownership",
      secondaryAction: "open_diagnostics",
      retryAllowed: false,
    };
  }

  // Uncertain outcomes (never auto-retry side-effecting writes)
  if (UNCERTAIN_CODES.has(code)) {
    return {
      titleKey: "error.uncertainTitle",
      actionKey: "error.uncertainAction",
      component: code.startsWith("project_") ? "project" : code.startsWith("runner_") ? "runner" : code.startsWith("service_") ? "service" : "desktop",
      recoveryKind: "manual_check",
      safeAction: "observe_before_retry",
      secondaryAction: "open_diagnostics",
      retryAllowed: false,
    };
  }

  // Legacy migration conflicts
  if (MIGRATION_CODES.has(code) || code.startsWith("migration_") || code.startsWith("legacy_")) {
    return {
      titleKey: "error.migrationTitle",
      actionKey: "error.migrationAction",
      component: "migration",
      recoveryKind: "manual_check",
      safeAction: "resolve_migration",
      secondaryAction: "open_diagnostics",
      retryAllowed: false,
    };
  }

  // Configuration corruption / migration failure
  if (code === "configuration_migration_failed" || code === "desktop_state_corrupt" || code === "journal_missing" || code === "server_environment_invalid" || code === "server_environment_duplicate_key") {
    return {
      titleKey: "error.configCorruptTitle",
      actionKey: "error.configCorruptAction",
      component: "config",
      recoveryKind: "confirmation_required",
      safeAction: "restore_configuration",
      secondaryAction: "open_diagnostics",
      retryAllowed: false,
    };
  }

  // Revision / fence / identity mismatch
  if (REVISION_STALE_CODES.has(code)) {
    return {
      titleKey: "error.revisionStaleTitle",
      actionKey: "error.revisionStaleAction",
      component: code.startsWith("cloudflare_") || code.startsWith("tunnel_") ? "tunnel" : "config",
      recoveryKind: "confirmation_required",
      safeAction: code.startsWith("cloudflare_") || code.startsWith("tunnel_") ? "check_connection" : "refresh_state",
      retryAllowed: false,
    };
  }

  // Explicit confirmation required
  if (code === "runtime_switch_jobs_confirmation_required" || code === "full_trace_confirmation_required" || code === "project_confirmation_required") {
    return {
      titleKey: "error.confirmationTitle",
      actionKey: "error.confirmationAction",
      component: "desktop",
      recoveryKind: "confirmation_required",
      safeAction: "observe_before_retry",
      retryAllowed: false,
    };
  }

  // Server HTTP failures (differentiate 400 vs 409 vs 503 vs 5xx!)
  if (code === "server_request_failed" || code === "workspace_unavailable") {
    if (diag.httpStatus === 503 || diag.reasonCode === "http_service_unavailable") {
      return {
        titleKey: "error.serverBusyTitle",
        actionKey: "error.serverBusyAction",
        component: "server",
        recoveryKind: "auto_recovering",
        safeAction: "refresh_state",
        secondaryAction: "open_diagnostics",
        retryAllowed: false,
      };
    }
    if (diag.httpStatus === 409 || diag.reasonCode === "http_conflict") {
      return {
        titleKey: "error.serverConflictTitle",
        actionKey: "error.serverConflictAction",
        component: "server",
        recoveryKind: "confirmation_required",
        safeAction: "observe_before_retry",
        secondaryAction: "open_diagnostics",
        retryAllowed: false,
      };
    }
    if (diag.httpStatus !== undefined && diag.httpStatus >= 500) {
      return {
        titleKey: "error.serverInternalTitle",
        actionKey: "error.serverInternalAction",
        component: "server",
        recoveryKind: "manual_check",
        safeAction: "open_diagnostics",
        secondaryAction: "open_activity",
        retryAllowed: false,
      };
    }
    return {
      titleKey: "error.serverRequestTitle",
      actionKey: "error.serverRequestAction",
      component: "server",
      recoveryKind: "user_action",
      safeAction: "open_diagnostics",
      secondaryAction: "open_activity",
      retryAllowed: false,
    };
  }

  if (code === "server_conflict") {
    return {
      titleKey: "error.serverConflictTitle",
      actionKey: "error.serverConflictAction",
      component: "server",
      recoveryKind: "confirmation_required",
      safeAction: "observe_before_retry",
      secondaryAction: "open_diagnostics",
      retryAllowed: false,
    };
  }

  // Binary / Runtime dependency missing
  if (BINARY_CODES.has(code)) {
    return {
      titleKey: "error.binaryTitle",
      actionKey: "error.binaryAction",
      component: "runtime",
      recoveryKind: "user_action",
      safeAction: "open_runtime_settings",
      secondaryAction: "open_diagnostics",
      retryAllowed: false,
    };
  }

  // Server start / unreachable
  if (SERVER_START_OR_REACH_CODES.has(code)) {
    return {
      titleKey: "error.serverTitle",
      actionKey: "error.serverAction",
      component: "server",
      recoveryKind: "user_action",
      safeAction: "start_server",
      secondaryAction: "open_diagnostics",
      retryAllowed: true,
    };
  }

  // Runner offline (auto-recovering) vs Runner failures
  if (code === "runner_offline") {
    return {
      titleKey: "error.runnerConnectingTitle",
      actionKey: "error.runnerConnectingAction",
      component: "runner",
      recoveryKind: "auto_recovering",
      safeAction: "wait_auto_recovery",
      secondaryAction: "open_runtime_settings",
      retryAllowed: true,
    };
  }
  if (code === "runner_not_ready" || code === "runner_start_failed" || code === "runner_exited") {
    return {
      titleKey: "error.runnerTitle",
      actionKey: "error.runnerAction",
      component: "runner",
      recoveryKind: "user_action",
      safeAction: "start_runner",
      secondaryAction: "open_diagnostics",
      retryAllowed: true,
    };
  }
  if (code === "runner_enrollment_required" || code === "local_runner_required" || code === "not_configured" || code === "setup_not_found" || code === "cloudflare_environment_required") {
    return {
      titleKey: "error.runtimeTitle",
      actionKey: "error.runtimeAction",
      component: code === "cloudflare_environment_required" ? "tunnel" : "environment",
      recoveryKind: "user_action",
      safeAction: "open_setup",
      retryAllowed: false,
    };
  }

  // Service credential / elevation / service management
  if (SERVICE_CREDENTIAL_CODES.has(code)) {
    return {
      titleKey: "error.serviceCredentialTitle",
      actionKey: "error.serviceCredentialAction",
      component: "service",
      recoveryKind: "user_action",
      safeAction: "repair_runner_credential",
      secondaryAction: "open_runtime_settings",
      retryAllowed: false,
    };
  }
  if (ELEVATION_CODES.has(code)) {
    return {
      titleKey: "error.elevationTitle",
      actionKey: "error.elevationAction",
      component: "service",
      recoveryKind: "confirmation_required",
      safeAction: "open_runtime_settings",
      retryAllowed: false,
    };
  }
  if (code.startsWith("service_") || code === "component_not_configured") {
    return {
      titleKey: "error.serviceManagementTitle",
      actionKey: "error.serviceManagementAction",
      component: "service",
      recoveryKind: "user_action",
      safeAction: "open_diagnostics",
      secondaryAction: "open_runtime_settings",
      retryAllowed: true,
    };
  }

  // A failed credential update is not proof that the supplied credential is
  // invalid, nor permission to repeat an outcome-uncertain credential write.
  if (code === "credential_repair_failed") {
    return {
      titleKey: "error.credentialRepairFailedTitle",
      actionKey: "error.credentialRepairFailedAction",
      component: "auth",
      recoveryKind: "manual_check",
      safeAction: "open_diagnostics",
      retryAllowed: false,
    };
  }

  // Authentication & OAuth & Permissions
  if (AUTH_CODES.has(code) || code.startsWith("oauth_") || code.startsWith("auth_")) {
    return {
      titleKey: "error.authTitle",
      actionKey: "error.authAction",
      component: "auth",
      recoveryKind: "user_action",
      safeAction: "repair_user_credential",
      secondaryAction: "open_diagnostics",
      retryAllowed: false,
    };
  }
  if (PERMISSION_CODES.has(code)) {
    return {
      titleKey: "error.permissionTitle",
      actionKey: "error.permissionAction",
      component: "auth",
      recoveryKind: "manual_check",
      safeAction: "open_diagnostics",
      retryAllowed: false,
    };
  }
  if (code === "pairing_code_invalid") {
    return {
      titleKey: "error.pairingTitle",
      actionKey: "error.pairingAction",
      component: "auth",
      recoveryKind: "user_action",
      safeAction: "open_setup",
      retryAllowed: false,
    };
  }

  // Tunnel credentials / server-owned / network & Cloudflare control
  if (code === "tunnel_credentials" || code === "tunnel_auth_invalid" || code === "tunnel_token_invalid") {
    return {
      titleKey: "error.tunnelCredentialTitle",
      actionKey: "error.tunnelCredentialAction",
      component: "tunnel",
      recoveryKind: "user_action",
      safeAction: "check_connection",
      retryAllowed: false,
    };
  }
  if (code === "tunnel_server_owned" || code === "tunnel_host_busy" || code === "cloudflare_ingress_not_applied") {
    return {
      titleKey: "error.tunnelServerOwnedTitle",
      actionKey: "error.tunnelServerOwnedAction",
      component: "tunnel",
      recoveryKind: "confirmation_required",
      safeAction: code === "cloudflare_ingress_not_applied" ? "restart_server" : "open_runtime_settings",
      retryAllowed: false,
    };
  }
  if (
    code === "cloudflare_server_unreachable"
    || code === "cloudflare_control_unavailable"
    || code === "cloudflare_control_required"
    || code === "tunnel_unavailable"
    || code === "tunnel_start_failed"
    || code === "tunnel_proxy_invalid"
    || code === "tunnel_profile_missing"
    || code === "regular_tunnel_not_ready"
    || code === "regular_tunnel_not_configured"
    || code === "regular_tunnel_already_running"
    || code === "quick_share_not_ready"
    || code === "quick_share_start_failed"
    || code === "quick_share_unavailable"
    || code.startsWith("cloudflare_")
    || code.startsWith("tunnel_")
  ) {
    return {
      titleKey: "error.tunnelNetworkTitle",
      actionKey: "error.tunnelNetworkAction",
      component: "tunnel",
      recoveryKind: code === "tunnel_unavailable" ? "auto_recovering" : "user_action",
      safeAction: code === "tunnel_proxy_invalid" ? "open_network_settings" : "check_connection",
      secondaryAction: "open_diagnostics",
      retryAllowed: code !== "cloudflare_control_unavailable",
    };
  }

  // Project
  if (PROJECT_CODES.has(code) || code.startsWith("project_")) {
    return {
      titleKey: "error.projectTitle",
      actionKey: "error.projectAction",
      component: "project",
      recoveryKind: "user_action",
      safeAction: "open_projects",
      retryAllowed: code === "project_not_loaded",
    };
  }

  // CLI commands (stage-aware, keeping exact presentation.test.ts behavior)
  if (COMMAND_CODES.has(code)) {
    const phaseRule = diag.stage ? COMMAND_PHASE_RULES[diag.stage] : undefined;
    if (phaseRule) return phaseRule;
    return {
      titleKey: "error.enrollmentTitle",
      actionKey: "error.enrollmentAction",
      component: "runtime",
      recoveryKind: "user_action",
      safeAction: "open_diagnostics",
      secondaryAction: "open_activity",
      retryAllowed: code === "webcodex_command_timeout",
    };
  }

  // Runtime readiness & processes & contract & topology
  if (code === "runtime_not_ready") {
    return {
      titleKey: "error.runtimeTitle",
      actionKey: "error.runtimeAction",
      component: "runtime",
      recoveryKind: "user_action",
      safeAction: "start_runtime",
      secondaryAction: "open_diagnostics",
      retryAllowed: true,
    };
  }
  if (code === "process_start_failed" || code === "process_already_running" || code === "quick_share_already_running") {
    return {
      titleKey: "error.processTitle",
      actionKey: "error.processAction",
      component: "runtime",
      recoveryKind: "user_action",
      safeAction: "open_activity",
      secondaryAction: "open_diagnostics",
      retryAllowed: true,
    };
  }
  if (code === "webcodex_contract_invalid") {
    return {
      titleKey: "error.contractTitle",
      actionKey: "error.contractAction",
      component: "runtime",
      recoveryKind: "user_action",
      safeAction: "open_runtime_settings",
      retryAllowed: false,
    };
  }
  if (code === "unsupported_topology" || code === "quick_share_provider_invalid") {
    return {
      titleKey: "error.topologyTitle",
      actionKey: "error.topologyAction",
      component: "environment",
      recoveryKind: "user_action",
      safeAction: "open_setup",
      retryAllowed: false,
    };
  }

  // Contention / stale operation / cancellation
  if (code === "desktop_operation_busy" || code === "desktop_state_unavailable" || code === "desktop_operation_not_cancellable") {
    return {
      titleKey: "error.operationBusyTitle",
      actionKey: "error.operationBusyAction",
      component: "desktop",
      recoveryKind: "auto_recovering",
      safeAction: "wait_auto_recovery",
      retryAllowed: true,
    };
  }
  if (code === "desktop_operation_not_current") {
    return {
      titleKey: "error.operationStaleTitle",
      actionKey: "error.operationStaleAction",
      component: "desktop",
      recoveryKind: "user_action",
      safeAction: "refresh_state",
      retryAllowed: false,
    };
  }
  if (code === "desktop_operation_cancelled") {
    return {
      titleKey: "error.operationCancelledTitle",
      actionKey: "error.operationCancelledAction",
      component: "desktop",
      recoveryKind: "manual_check",
      safeAction: "observe_before_retry",
      retryAllowed: false,
    };
  }

  // Providers
  if (code.startsWith("mcp_provider_") || code.startsWith("coding_agent_")) {
    return {
      titleKey: "error.runtimeTitle",
      actionKey: "error.runtimeAction",
      component: "runtime",
      recoveryKind: "user_action",
      safeAction: "manage_providers",
      retryAllowed: false,
    };
  }

  // IPC & unknown
  if (code === "desktop_ipc_failed") {
    return {
      titleKey: "error.ipcTitle",
      actionKey: "error.ipcAction",
      component: "desktop",
      recoveryKind: "user_action",
      safeAction: "refresh_state",
      secondaryAction: "open_activity",
      retryAllowed: true,
    };
  }
  if (code === "desktop_unknown_error") {
    return {
      titleKey: "error.unknownTitle",
      actionKey: "error.unknownAction",
      component: "unknown",
      recoveryKind: "manual_check",
      safeAction: "open_diagnostics",
      secondaryAction: "open_activity",
      retryAllowed: false,
    };
  }
  if (code === "response_limit" || code === "response_invalid") {
    return {
      titleKey: "error.serverInternalTitle",
      actionKey: "error.serverInternalAction",
      component: "server",
      recoveryKind: "manual_check",
      safeAction: "open_diagnostics",
      secondaryAction: "open_activity",
      retryAllowed: false,
    };
  }
  if (code.startsWith("desktop_data_dir_") || code === "state_io" || code.startsWith("inventory_") || code.startsWith("support_bundle_") || code.startsWith("diagnostic_")) {
    return {
      titleKey: "error.configCorruptTitle",
      actionKey: "error.configCorruptAction",
      component: "config",
      recoveryKind: "manual_check",
      safeAction: "open_diagnostics",
      retryAllowed: false,
    };
  }

  return {
    titleKey: "error.unknownTitle",
    actionKey: "error.unknownAction",
    component: "unknown",
    recoveryKind: "manual_check",
    safeAction: "open_diagnostics",
    secondaryAction: "open_activity",
    retryAllowed: false,
  };
}

export function extractSafeErrorDiagnostics(error: DesktopError): SafeErrorDiagnostics {
  const origin = desktopErrorOrigin(error);
  const incoming = sanitizeIncomingDiagnostic(error.diagnostic);
  const details = error.details && typeof error.details === "object" && !Array.isArray(error.details)
    ? (error.details as Record<string, unknown>)
    : undefined;

  const stage = incoming?.stage
    ?? safeIdentifier(details?.phase, /^[a-z0-9_-]{2,96}$/)
    ?? safeIdentifier(details?.stage, /^[a-z0-9_-]{2,96}$/);
  const logicalCommand = safeIdentifier(details?.logical_command, /^[a-z0-9 _-]{2,96}$/);
  const executable = safeIdentifier(details?.executable, /^[A-Za-z0-9._-]{1,96}$/);
  const reasonCode = incoming?.reason_code
    ?? safeIdentifier(details?.reason_code, /^[a-z0-9_-]{2,96}$/);
  const httpStatus = incoming?.http_status
    ?? (typeof details?.http_status === "number" && Number.isInteger(details.http_status) && details.http_status >= 100 && details.http_status <= 599
      ? details.http_status
      : undefined)
    ?? (origin === "trusted_structured" ? inferHttpStatusFromMessage(error.message) : undefined);
  const exitCode = incoming?.exit_code
    ?? (typeof details?.exit_code === "number" && Number.isSafeInteger(details.exit_code) ? details.exit_code : undefined);
  const operationId = incoming?.operation_id ?? safeReferenceId(details?.operation_id);
  const generation = incoming?.generation ?? undefined;
  const resourceId = incoming?.resource_id ?? undefined;
  const activitySequence = incoming?.activity_sequence ?? undefined;

  const rule = classifyRule(error, { stage, reasonCode, httpStatus });

  // Always enforce the stricter fail-closed boundary on retryAllowed:
  // if either the backend diagnostic OR the frontend allowlist forbids auto-retry, keep it disabled.
  const retryAllowed = incoming ? (incoming.retry_allowed && rule.retryAllowed) : rule.retryAllowed;
  const component = incoming && incoming.component !== "unknown" ? incoming.component : rule.component;
  const recoveryKind = incoming?.recovery_kind ?? rule.recoveryKind;
  const safeAction = incoming && incoming.safe_action !== "none" ? incoming.safe_action : rule.safeAction;

  return {
    code: safeIdentifier(error.code, /^[a-z][a-z0-9_-]{1,95}$/) ?? "desktop_unknown_error",
    origin,
    component,
    stage: stage ?? undefined,
    logicalCommand,
    executable,
    reasonCode: reasonCode && reasonCode !== error.code ? reasonCode : undefined,
    httpStatus,
    exitCode,
    operationId,
    generation,
    resourceId,
    activitySequence,
    recoveryKind,
    safeAction,
    retryAllowed,
  };
}

const COMPONENT_KEYS: Record<ErrorComponent, MessageKey> = {
  runtime: "error.component.runtime",
  server: "error.component.server",
  runner: "error.component.runner",
  service: "error.component.service",
  tunnel: "error.component.tunnel",
  auth: "error.component.auth",
  project: "error.component.project",
  environment: "error.component.environment",
  migration: "error.component.migration",
  config: "error.component.config",
  desktop: "error.component.desktop",
  unknown: "error.component.unknown",
};

const STAGE_KEYS: Record<string, MessageKey> = {
  preflight: "error.stage.preflight",
  server_configuration: "error.stage.server_configuration",
  server_service_install: "error.stage.server_service_install",
  server_service_start: "error.stage.server_service_start",
  server_start: "error.stage.server_start",
  server_reachability: "error.stage.server_reachability",
  user_create: "error.stage.user_create",
  user_authentication: "error.stage.user_authentication",
  user_token_registration: "error.stage.user_token_registration",
  runner_token_registration: "error.stage.runner_token_registration",
  runner_enrollment: "error.stage.runner_enrollment",
  runner_configuration: "error.stage.runner_configuration",
  project_registration: "error.stage.project_registration",
  project_removal: "error.stage.project_removal",
  runner_service_install: "error.stage.runner_service_install",
  runner_service_start: "error.stage.runner_service_start",
  readiness: "error.stage.readiness",
  device_invitation: "error.stage.device_invitation",
  cloudflare_status: "error.stage.cloudflare_status",
  cloudflare_start: "error.stage.cloudflare_start",
  cloudflare_stop: "error.stage.cloudflare_stop",
  cloudflare_oauth: "error.stage.cloudflare_oauth",
  tunnel_status: "error.stage.tunnel_status",
  tunnel_identity_check: "error.stage.tunnel_identity_check",
  tunnel_service_ownership: "error.stage.tunnel_service_ownership",
  tunnel_readiness: "error.stage.tunnel_readiness",
  migration_handoff: "error.stage.migration_handoff",
  workspace_request: "error.stage.workspace_request",
  workspace_overview: "error.stage.workspace_request",
  workspace_projects: "error.stage.workspace_request",
  workspace_runner: "error.stage.workspace_request",
  workspace_windows: "error.stage.workspace_request",
  ipc_bridge: "error.stage.ipc_bridge",
};

const RECOVERY_KEYS: Record<RecoveryKind, MessageKey> = {
  auto_recovering: "error.recovery.autoRecovering",
  user_action: "error.recovery.userAction",
  manual_check: "error.recovery.manualCheck",
  confirmation_required: "error.recovery.confirmationRequired",
};

export const SAFE_ACTION_LABEL_KEYS: Record<Exclude<SafeRecoveryAction, "none">, MessageKey> = {
  wait_auto_recovery: "error.btn.waitAutoRecovery",
  retry: "error.btn.retry",
  refresh_state: "error.btn.refreshState",
  start_runtime: "error.btn.startRuntime",
  start_server: "error.btn.startServer",
  restart_server: "error.btn.restartServer",
  start_runner: "error.btn.startRunner",
  restart_runner: "error.btn.restartRunner",
  repair_runner_credential: "error.btn.repairRunnerCredential",
  repair_user_credential: "error.btn.repairUserCredential",
  check_connection: "error.btn.checkConnection",
  open_network_settings: "error.btn.openNetworkSettings",
  open_runtime_settings: "error.btn.openRuntimeSettings",
  open_diagnostics: "error.btn.openDiagnostics",
  open_activity: "error.btn.openActivity",
  open_projects: "error.btn.openProjects",
  open_setup: "error.btn.openSetup",
  inspect_service_ownership: "error.btn.inspectServiceOwnership",
  resolve_migration: "error.btn.resolveMigration",
  observe_before_retry: "error.btn.observeBeforeRetry",
  restore_configuration: "error.btn.restoreConfiguration",
  manage_providers: "error.btn.manageProviders",
};

export function errorStageLabel(stage: string | undefined, t: Translate): string | undefined {
  if (!stage) return undefined;
  const key = STAGE_KEYS[stage];
  return key ? t(key) : undefined;
}

export function resolveDesktopError(error: DesktopError, t: Translate): ResolvedDesktopError {
  const diagnostics = extractSafeErrorDiagnostics(error);
  const rule = classifyRule(error, diagnostics);
  const stageLabel = errorStageLabel(diagnostics.stage, t);
  const baseTitle = t(rule.titleKey);
  const title = diagnostics.httpStatus !== undefined && (error.code === "server_request_failed" || error.code === "workspace_unavailable")
    ? `${baseTitle} (HTTP ${diagnostics.httpStatus})`
    : baseTitle;

  return {
    title,
    action: t(rule.actionKey),
    stageLabel,
    componentLabel: t(COMPONENT_KEYS[diagnostics.component]),
    recoveryLabel: t(RECOVERY_KEYS[diagnostics.recoveryKind]),
    diagnostics,
    secondaryAction: rule.secondaryAction,
  };
}

/**
 * Determines whether a transient component error has been definitively resolved
 * by fresh readiness evidence. Never clears unrelated components, uncertain
 * write outcomes, or ownership/migration conflicts.
 */
export function isTransientErrorResolvedByState(error: DesktopError, state: DesktopState): boolean {
  // Retry permission concerns repeating the failed write; clearing an alert
  // after authoritative readiness is a separate operation. A Server port
  // conflict may forbid retry and still be resolved by Server=Ready.
  switch (error.code) {
    case "runner_offline":
    case "runner_not_ready":
    case "runner_start_failed":
    case "runner_exited":
      return state.readiness.runner === "ready";
    case "server_unreachable":
    case "server_not_ready":
    case "server_start_failed":
    case "server_exited":
    case "workspace_server_unreachable":
      return state.readiness.server === "ready";
    case "project_not_loaded":
      return state.readiness.project === "ready";
    case "runtime_not_ready":
      return state.readiness.runtime_ready;
    case "desktop_operation_busy":
    case "desktop_state_unavailable":
      return !state.current_operation;
    default:
      return false;
  }
}

/**
 * Checks whether an error belongs to an older environment/profile or older
 * generation than the canonical state currently displayed.
 */
export function isErrorStaleForState(
  error: DesktopError,
  state: DesktopState,
  errorIdentity?: string | null,
  currentIdentity?: string | null,
): boolean {
  if (errorIdentity && currentIdentity && errorIdentity !== currentIdentity) {
    return true;
  }
  const diagnostics = extractSafeErrorDiagnostics(error);
  // resource_id is not globally an Environment ID: Cloudflare auth errors
  // may carry a Profile ID, while Runner/Server errors may carry their own
  // identity. Compare it with persistent_environment only for components
  // whose resource identity is explicitly the Environment.
  if (
    diagnostics.resourceId
    && state.persistent_environment
    && (diagnostics.component === "environment" || diagnostics.component === "migration")
    && diagnostics.resourceId !== state.persistent_environment
  ) {
    return true;
  }
  if (
    diagnostics.generation !== undefined
    && state.state_generation !== undefined
    && diagnostics.generation < state.state_generation
    && isTransientErrorResolvedByState(error, state)
  ) {
    return true;
  }
  return isTransientErrorResolvedByState(error, state);
}

/**
 * Reconciles incoming DesktopState so contradicted transient errors (e.g.
 * runner_offline alongside runner=ready during a getState/refresh handoff)
 * are never rendered, while preserving errors for still-unhealthy components
 * and sticky uncertain/ownership errors.
 */
export function reconcileDesktopStateErrors(state: DesktopState): DesktopState {
  let nextComponentErrors = state.component_errors ? { ...state.component_errors } : undefined;
  let componentChanged = false;
  if (nextComponentErrors) {
    for (const [key, value] of Object.entries(nextComponentErrors) as [ErrorComponent, DesktopError | undefined][]) {
      if (value && isTransientErrorResolvedByState(value, state)) {
        delete nextComponentErrors[key];
        componentChanged = true;
      }
    }
  }
  const runtimeResolved = Boolean(state.runtime_error && isTransientErrorResolvedByState(state.runtime_error, state));
  if (!runtimeResolved && !componentChanged) return state;

  let nextRuntimeError = state.runtime_error ?? null;
  if (runtimeResolved) {
    const remaining = nextComponentErrors ? Object.values(nextComponentErrors).find(Boolean) ?? null : null;
    nextRuntimeError = remaining;
  }
  return {
    ...state,
    runtime_error: nextRuntimeError,
    ...(nextComponentErrors ? { component_errors: nextComponentErrors } : {}),
  };
}
