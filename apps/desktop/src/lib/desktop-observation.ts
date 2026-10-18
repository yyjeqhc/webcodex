import { normalizeDesktopError } from "../i18n/presentation";
import { desktopApi } from "./desktop-api";
import type { DesktopError, DesktopState } from "../models/topology";

// Invalidate observations and drafts when their published target changes.
// Native target/revision checks remain authoritative for every mutation.
export function desktopIdentity(state: DesktopState): string {
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

// Only idempotent observations belong here. Never pass a mutation, clipboard,
// browser-open, probe, restart, or export action: those keep explicit recovery.
export async function readDesktopObservation<T>(
  identity: string,
  signal: AbortSignal,
  read: () => Promise<T>,
): Promise<T> {
  const deadline = performance.now() + 2_000;
  const checkRetry = async (busy: DesktopError) => {
    signal.throwIfAborted();
    // The published snapshot is readable while DesktopCore is borrowed.
    const state = await desktopApi.getState().catch(() => null);
    signal.throwIfAborted();
    if (!state) throw busy;
    if (desktopIdentity(state) !== identity) {
      throw {
        code: "diagnostic_identity_changed",
        message: "The Runtime configuration changed while reading its settings.",
        next_action: "Refresh the current Runtime.",
      } satisfies DesktopError;
    }
    // Idle also covers a refresh that ended before this observation.
    if ((state.current_operation && state.current_operation.kind !== "runtime_refresh")
      || performance.now() >= deadline) throw busy;
  };

  for (let retries = 0; ; retries += 1) {
    signal.throwIfAborted();
    try {
      const snapshot = await read();
      signal.throwIfAborted();
      return snapshot;
    } catch (value) {
      signal.throwIfAborted();
      const error = normalizeDesktopError(value);
      // runnerSettings currently uses desktop_state_unavailable while its core
      // is borrowed. Real failures retain their original error after this bound.
      if ((error.code !== "desktop_operation_busy" && error.code !== "desktop_state_unavailable")
        || retries >= 5 || performance.now() >= deadline) throw value;
      await checkRetry(error);
      await retryDelay(signal);
      // Recheck immediately before another read, including stale-props cases.
      await checkRetry(error);
    }
  }
}
