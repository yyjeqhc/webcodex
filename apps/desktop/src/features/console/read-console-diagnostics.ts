import { normalizeDesktopError } from "../../i18n/presentation";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";

// Invalidate UI observations when the published target changes. The native
// diagnostics identity checks and credential-copy fence remain authoritative.
export function consoleIdentity(state: DesktopState): string {
  return JSON.stringify([
    state.persistent_environment, state.environment_setup?.environment_id,
    state.workspace_runner?.server_url, state.workspace_runner?.client_id,
    state.workspace_runner?.config_path,
    state.topology?.server.kind,
    state.topology?.server.kind === "remote" ? state.topology.server.url : null,
    state.project?.runtime_project_id,
    state.binaries?.directory, state.binaries?.version,
    state.binaries?.git_commit, state.binaries?.source,
  ]);
}

function retryDelay(signal: AbortSignal): Promise<void> {
  signal.throwIfAborted();
  return new Promise((resolve, reject) => {
    const cancel = () => {
      clearTimeout(timer);
      reject(signal.reason);
    };
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", cancel);
      resolve();
    }, 200);
    signal.addEventListener("abort", cancel, { once: true });
  });
}

// Only this read is retryable. Never wrap clipboard, browser-open, or other
// actions: a busy effect must retain its original explicit recovery semantics.
export async function readConsoleDiagnostics(identity: string, signal: AbortSignal) {
  const deadline = performance.now() + 2_000;
  const checkRetry = async (busy: DesktopError) => {
    signal.throwIfAborted();
    // getState reads the published snapshot without borrowing DesktopCore.
    const state = await desktopApi.getState().catch(() => null);
    signal.throwIfAborted();
    if (!state) throw busy;
    if (consoleIdentity(state) !== identity) {
      throw {
        code: "diagnostic_identity_changed",
        message: "The Runtime configuration changed while reading diagnostics.",
        next_action: "Refresh diagnostics for the current Runtime.",
      } satisfies DesktopError;
    }
    // No current operation means the busy operation has already finished.
    // Any still-active operation other than observation must fail closed.
    if ((state.current_operation && state.current_operation.kind !== "runtime_refresh")
      || performance.now() >= deadline) throw busy;
  };

  for (let retries = 0; ; retries += 1) {
    signal.throwIfAborted();
    try {
      const snapshot = await desktopApi.diagnostics();
      signal.throwIfAborted();
      return snapshot;
    } catch (value) {
      signal.throwIfAborted();
      const error = normalizeDesktopError(value);
      if (error.code !== "desktop_operation_busy" || retries >= 5
        || performance.now() >= deadline) throw value;
      await checkRetry(error);
      await retryDelay(signal);
      // A different operation or identity may have appeared during the delay,
      // before the workspace observer has delivered new component props.
      await checkRetry(error);
    }
  }
}
