import { describe, expect, it } from "vitest";
import type { DesktopError, DesktopState } from "../models/topology";
import { messages, type MessageKey } from "./locale";
import { desktopCommandDiagnostics, desktopErrorPresentation, normalizeDesktopError } from "./presentation";
import {
  desktopErrorOrigin,
  extractSafeErrorDiagnostics,
  isErrorStaleForState,
  reconcileDesktopStateErrors,
  resolveDesktopError,
} from "../lib/desktop-errors";

const zh = (key: MessageKey, params?: Record<string, string | number>) => {
  let result = messages["zh-CN"][key];
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      result = result.replaceAll(`{{${name}}}`, String(value));
    }
  }
  return result;
};

describe("Desktop command error presentation", () => {
  it("routes Runtime launch and exit failures to binary recovery during setup", () => {
    for (const code of ["webcodex_command_start_failed", "webcodex_command_failed", "webcodex_command_timeout"]) {
      const error: DesktopError = { code, message: "failed", next_action: "Recheck Runtime", details: { phase: "runtime_probe", logical_command: "build-info" } };
      expect(desktopErrorPresentation(error, zh)).toEqual({ title: messages["zh-CN"]["error.binaryTitle"], action: messages["zh-CN"]["error.binaryAction"] });
    }
  });
  it("uses the safe command phase to present the failing subsystem", () => {
    const error: DesktopError = {
      code: "webcodex_command_failed",
      message: "WebCodex did not complete the requested operation",
      next_action: "Retry.",
      details: {
        phase: "server_init",
        logical_command: "server init",
        executable: "webcodex.exe",
        exit_code: 17,
        reason_code: "path_outside_allowed_roots",
      },
    };
    expect(desktopErrorPresentation(error, zh).title).toBe(messages["zh-CN"]["error.serverTitle"]);
    expect(desktopCommandDiagnostics(error)).toEqual({
      phase: "server_init",
      logicalCommand: "server init",
      executable: "webcodex.exe",
      exitCode: 17,
      reasonCode: "path_outside_allowed_roots",
    });
  });

  it("presents a missing explicit local Project as a Project recovery problem", () => {
    const error: DesktopError = {
      code: "project_not_ready",
      message: "Local setup requires an explicit project folder",
      next_action: "Choose the project folder that this Runner should manage, then retry setup.",
    };
    expect(desktopErrorPresentation(error, zh)).toEqual({
      title: messages["zh-CN"]["error.projectTitle"],
      action: messages["zh-CN"]["error.projectAction"],
    });
  });

  it("never projects arbitrary or secret-bearing detail fields", () => {
    const error: DesktopError = {
      code: "webcodex_command_failed",
      message: "failed",
      next_action: "Retry.",
      details: {
        phase: "login",
        logical_command: "login",
        executable: "C:\\private\\webcodex.exe",
        reason_code: "authorization_token_deadbeef",
        stderr: "Bearer super-secret-value",
      },
    };
    expect(desktopCommandDiagnostics(error)).toEqual({
      phase: "login",
      logicalCommand: "login",
      executable: undefined,
      exitCode: undefined,
      reasonCode: undefined,
    });
  });
});

describe("Unified Desktop error normalization, classification, and reconciliation", () => {
  it("distinguishes trusted structured, trusted partial, Tauri IPC, and unknown exceptions without leaking raw text", () => {
    const structured = normalizeDesktopError({
      code: "server_request_failed",
      message: "Local user creation failed (HTTP 400)",
      next_action: "Check setup inputs",
      diagnostic: {
        component: "server",
        stage: "user_create",
        reason_code: "http_bad_request",
        recovery_kind: "user_action",
        safe_action: "open_diagnostics",
        retry_allowed: false,
        http_status: 400,
      },
    });
    expect(desktopErrorOrigin(structured)).toBe("trusted_structured");
    expect(structured.code).toBe("server_request_failed");
    expect(structured.diagnostic?.stage).toBe("user_create");

    // Error-code identifiers may name credentials without containing credential
    // values. Do not collapse them into desktop_unknown_error.
    for (const code of [
      "credential_repair_failed",
      "service_credentialinvalid",
      "tunnel_token_invalid",
      "oauth_authorization_failed",
    ]) {
      const named = normalizeDesktopError({ code, message: "private text", next_action: "Inspect" });
      expect(named.code).toBe(code);
      expect(extractSafeErrorDiagnostics(named).code).toBe(code);
    }
    expect(normalizeDesktopError({ code: "wc_pat_sensitive_value" }).code).toBe("desktop_unknown_error");

    const partial = normalizeDesktopError({ code: "service_foreignservice" });
    expect(desktopErrorOrigin(partial)).toBe("trusted_partial");
    expect(partial.code).toBe("service_foreignservice");

    const ipc = normalizeDesktopError(new Error("Tauri IPC invoke failed: Bearer wc_pat_secret_canary"));
    expect(desktopErrorOrigin(ipc)).toBe("ipc_transport");
    expect(ipc.code).toBe("desktop_ipc_failed");
    expect(JSON.stringify(ipc)).not.toContain("wc_pat_secret_canary");

    const unknown = normalizeDesktopError(new Error("Unexpected crash with password=super_secret_canary"));
    expect(desktopErrorOrigin(unknown)).toBe("unknown");
    expect(unknown.code).toBe("desktop_unknown_error");
    expect(JSON.stringify(unknown)).not.toContain("super_secret_canary");
    expect(extractSafeErrorDiagnostics(unknown).retryAllowed).toBe(false);
  });

  it("keeps HTTP 400 and HTTP 503 diagnostics and failure stages distinct", () => {
    const err400: DesktopError = {
      code: "server_request_failed",
      message: "Local user creation failed (HTTP 400)",
      next_action: "Check inputs",
      diagnostic: {
        component: "server",
        stage: "user_create",
        reason_code: "http_bad_request",
        recovery_kind: "user_action",
        safe_action: "open_diagnostics",
        retry_allowed: false,
        http_status: 400,
      },
    };
    const err503: DesktopError = {
      code: "server_request_failed",
      message: "Server temporarily unavailable (HTTP 503)",
      next_action: "Wait and refresh",
      diagnostic: {
        component: "server",
        stage: "user_create",
        reason_code: "http_service_unavailable",
        recovery_kind: "auto_recovering",
        safe_action: "refresh_state",
        retry_allowed: true,
        http_status: 503,
      },
    };
    const res400 = resolveDesktopError(err400, zh);
    const res503 = resolveDesktopError(err503, zh);

    expect(res400.title).toContain("HTTP 400");
    expect(res503.title).toContain("HTTP 503");
    expect(res400.title).not.toBe(res503.title);
    expect(res400.stageLabel).toBe(messages["zh-CN"]["error.stage.user_create"]);
    expect(res503.stageLabel).toBe(messages["zh-CN"]["error.stage.user_create"]);
    expect(res400.diagnostics.retryAllowed).toBe(false);
    // A 503 allows read-only status checks, not replay of the original POST.
    expect(res503.diagnostics.retryAllowed).toBe(false);
    expect(res503.diagnostics.safeAction).toBe("refresh_state");
    expect(res503.diagnostics.recoveryKind).toBe("auto_recovering");
  });

  it("classifies port conflict, ownership conflict, migration, tunnel credentials, and uncertain outcomes accurately without generic fallback", () => {
    const fallbackTitle = messages["zh-CN"]["error.fallbackTitle"];
    const cases: Array<{ error: DesktopError; expectedTitle: string; expectedRetry: boolean }> = [
      {
        error: {
          code: "server_start_failed",
          message: "failed to bind HTTP listener 127.0.0.1:8787 (os error 10048)",
          next_action: "Inspect port",
          diagnostic: {
            component: "server",
            stage: "server_start",
            reason_code: "local_port_unavailable",
            recovery_kind: "user_action",
            safe_action: "open_diagnostics",
            retry_allowed: false,
            exit_code: 1,
          },
        },
        expectedTitle: messages["zh-CN"]["error.serverPortTitle"],
        expectedRetry: false,
      },
      {
        error: { code: "service_foreignservice", message: "Foreign service", next_action: "Inspect" },
        expectedTitle: messages["zh-CN"]["error.ownershipTitle"],
        expectedRetry: false,
      },
      {
        error: { code: "migration_conflict", message: "Migration conflict", next_action: "Inspect" },
        expectedTitle: messages["zh-CN"]["error.migrationTitle"],
        expectedRetry: false,
      },
      {
        error: { code: "tunnel_credentials", message: "Missing tunnel credentials", next_action: "Update" },
        expectedTitle: messages["zh-CN"]["error.tunnelCredentialTitle"],
        expectedRetry: false,
      },
      {
        error: { code: "operation_uncertain", message: "Uncertain write", next_action: "Observe" },
        expectedTitle: messages["zh-CN"]["error.uncertainTitle"],
        expectedRetry: false,
      },
      {
        error: { code: "authentication_required", message: "Unauthorized (HTTP 401)", next_action: "Reauth" },
        expectedTitle: messages["zh-CN"]["error.authTitle"],
        expectedRetry: false,
      },
    ];
    for (const item of cases) {
      const resolved = resolveDesktopError(item.error, zh);
      expect(resolved.title).toBe(item.expectedTitle);
      expect(resolved.title).not.toBe(fallbackTitle);
      expect(resolved.diagnostics.retryAllowed).toBe(item.expectedRetry);
    }
  });

  it("reconciles recovered component errors without clearing unrelated component or uncertain errors, and fences stale identities", () => {
    const runnerOffline: DesktopError = { code: "runner_offline", message: "Runner offline", next_action: "Wait" };
    const serverUnreachable: DesktopError = { code: "server_unreachable", message: "Server offline", next_action: "Start Server" };
    const uncertain: DesktopError = { code: "operation_uncertain", message: "Outcome unknown", next_action: "Observe" };

    const stateRunnerRecovered = {
      persistent_environment: "env-a",
      state_generation: 5,
      readiness: {
        server: "error",
        runner: "ready",
        exposure: "disabled",
        project: "configured",
        runtime_ready: false,
        ready_for_chatgpt: false,
        summary: "Needs attention",
        summary_kind: "service_needs_attention",
      },
      runtime_error: runnerOffline,
      component_errors: {
        runner: runnerOffline,
        server: serverUnreachable,
      },
    } as DesktopState;

    const reconciled = reconcileDesktopStateErrors(stateRunnerRecovered);
    expect(reconciled.component_errors?.runner).toBeUndefined();
    expect(reconciled.component_errors?.server).toEqual(serverUnreachable);
    expect(reconciled.runtime_error).toEqual(serverUnreachable);

    // Port conflicts are not safe to retry, but a new Server=Ready
    // observation proves the old error is no longer current.
    const resolvedPortConflict = {
      ...stateRunnerRecovered,
      readiness: { ...stateRunnerRecovered.readiness, server: "ready" },
      runtime_error: {
        code: "server_start_failed",
        message: "Port was busy",
        next_action: "Inspect",
        diagnostic: {
          component: "server",
          recovery_kind: "manual_check",
          safe_action: "open_diagnostics",
          retry_allowed: false,
          reason_code: "local_port_unavailable",
        },
      },
    } as DesktopState;
    expect(reconcileDesktopStateErrors(resolvedPortConflict).runtime_error).toBeNull();

    // HTTP 503 allows checking Server readiness, never replaying the
    // original setup/Workspace POST whose outcome is not proven.
    const pendingPost = normalizeDesktopError({
      code: "server_request_failed",
      message: "Server returned HTTP 503",
      next_action: "Check Server",
      diagnostic: {
        component: "server",
        stage: "user_create",
        recovery_kind: "auto_recovering",
        safe_action: "refresh_state",
        retry_allowed: true,
        http_status: 503,
      },
    });
    const pendingDiag = extractSafeErrorDiagnostics(pendingPost);
    expect(pendingDiag.safeAction).toBe("refresh_state");
    expect(pendingDiag.retryAllowed).toBe(false);

    // Uncertain errors survive even when all components appear ready.
    const readyWithUncertain = {
      ...stateRunnerRecovered,
      readiness: {
        ...stateRunnerRecovered.readiness,
        server: "ready",
        runner: "ready",
        project: "ready",
        runtime_ready: true,
      },
      runtime_error: uncertain,
      component_errors: { desktop: uncertain },
    } as DesktopState;
    expect(reconcileDesktopStateErrors(readyWithUncertain).runtime_error).toEqual(uncertain);

    // Stale profile / environment identity fences old errors from overwriting new state.
    expect(isErrorStaleForState(serverUnreachable, stateRunnerRecovered, "env-old", "env-a")).toBe(true);
    expect(isErrorStaleForState(
      { ...serverUnreachable, diagnostic: { component: "environment", recovery_kind: "user_action", safe_action: "start_server", retry_allowed: true, resource_id: "env-old" } },
      stateRunnerRecovered,
      "env-a",
      "env-a",
    )).toBe(true);
    // A Cloudflare auth error is scoped to its Profile, not to the Environment;
    // do not suppress it merely because its resource_id differs.
    const cloudflareAuth: DesktopError = {
      code: "cloudflare_authentication_failed",
      message: "Cloudflare authorization rejected",
      next_action: "Inspect",
      diagnostic: {
        component: "auth",
        recovery_kind: "user_action",
        safe_action: "check_connection",
        retry_allowed: false,
        resource_id: "cf-profile-1",
      },
    };
    expect(isErrorStaleForState(cloudflareAuth, stateRunnerRecovered, "env-a", "env-a")).toBe(false);
  });
});
