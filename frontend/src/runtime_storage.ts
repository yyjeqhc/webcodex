export const RUNTIME_CREDENTIAL_SESSION_KEY = "webpi.runtime.credential.v1";
export const APPEARANCE_STORAGE_KEY = "webpi.runtime.appearance.v1";
export const WORKSPACE_VIEW_STORAGE_KEY = "webpi.runtime.workspace-view.v1";
export const DRAFT_STORAGE_PREFIX = "webpi.runtime.draft.v1.";
export const DEVICE_DISCLOSURE_STORAGE_PREFIX = "webpi.runtime.runner-open.v1.";
export const APPEARANCE_MEDIA_QUERY = "(prefers-color-scheme: light)";

export type AppearancePreference = "system" | "light" | "dark";
export type RuntimeWorkspaceView = "home" | "projects" | "sessions" | "operations" | "windows" | "activity" | "extensions";

export function appearancePreference(value: unknown): AppearancePreference {
  return value === "light" || value === "dark" || value === "system" ? value : "system";
}

export function loadAppearancePreference(): AppearancePreference {
  try {
    return appearancePreference(window.localStorage.getItem(APPEARANCE_STORAGE_KEY));
  } catch {
    return "system";
  }
}

export function persistAppearancePreference(preference: AppearancePreference): void {
  try {
    window.localStorage.setItem(APPEARANCE_STORAGE_KEY, preference);
  } catch {
    /* Storage can be unavailable in hardened browser contexts. */
  }
}

export function resolvedAppearance(
  preference: AppearancePreference,
  prefersLight: boolean,
): "light" | "dark" {
  if (preference !== "system") return preference;
  return prefersLight ? "light" : "dark";
}

export function workspaceViewPreference(value: unknown): RuntimeWorkspaceView {
  return value === "sessions" || value === "operations" || value === "windows" || value === "projects" || value === "activity" || value === "extensions" ? value : "home";
}

export function loadWorkspaceViewPreference(): RuntimeWorkspaceView {
  try {
    return workspaceViewPreference(window.localStorage.getItem(WORKSPACE_VIEW_STORAGE_KEY));
  } catch {
    return "home";
  }
}

export function persistWorkspaceViewPreference(view: RuntimeWorkspaceView): void {
  try {
    window.localStorage.setItem(WORKSPACE_VIEW_STORAGE_KEY, view);
  } catch {
    /* Storage can be unavailable in hardened browser contexts. */
  }
}

export function loadRememberedRuntimeCredential(): string {
  try {
    return window.sessionStorage.getItem(RUNTIME_CREDENTIAL_SESSION_KEY)?.trim() || "";
  } catch {
    return "";
  }
}

export function persistRuntimeCredentialForTab(token: string, remember: boolean): void {
  try {
    if (remember && token) {
      window.sessionStorage.setItem(RUNTIME_CREDENTIAL_SESSION_KEY, token);
    } else {
      window.sessionStorage.removeItem(RUNTIME_CREDENTIAL_SESSION_KEY);
    }
  } catch {
    /* Storage can be unavailable in hardened browser contexts. */
  }
}

export function clearRememberedRuntimeCredential(): void {
  try {
    window.sessionStorage.removeItem(RUNTIME_CREDENTIAL_SESSION_KEY);
  } catch {
    /* Storage can be unavailable in hardened browser contexts. */
  }
}

export function currentDraftStorageKey(project: unknown, sessionId: unknown): string {
  const projectId = String(project || "");
  const workflowSessionId = String(sessionId || "");
  return projectId && workflowSessionId
    ? DRAFT_STORAGE_PREFIX + encodeURIComponent(projectId) + "." + encodeURIComponent(workflowSessionId)
    : "";
}

export function loadDraft(project: unknown, sessionId: unknown): string {
  const key = currentDraftStorageKey(project, sessionId);
  if (!key) return "";
  try {
    return window.sessionStorage.getItem(key) || "";
  } catch {
    return "";
  }
}

export function saveDraft(project: unknown, sessionId: unknown, text: string): void {
  const key = currentDraftStorageKey(project, sessionId);
  if (!key) return;
  try {
    if (text) window.sessionStorage.setItem(key, text);
    else window.sessionStorage.removeItem(key);
  } catch {
    /* Draft remains in active input when storage is unavailable. */
  }
}

export function clearDraft(project: unknown, sessionId: unknown): void {
  const key = currentDraftStorageKey(project, sessionId);
  if (!key) return;
  try {
    window.sessionStorage.removeItem(key);
  } catch {
    /* No-op in hardened browser contexts. */
  }
}

export function clearRuntimeDrafts(): void {
  try {
    const keys: string[] = [];
    for (let index = 0; index < window.sessionStorage.length; index += 1) {
      const key = window.sessionStorage.key(index);
      if (key?.startsWith(DRAFT_STORAGE_PREFIX)) keys.push(key);
    }
    for (const key of keys) window.sessionStorage.removeItem(key);
  } catch {
    /* No-op in hardened browser contexts. */
  }
}

export function deviceDisclosureStorageKey(clientId: string): string {
  return DEVICE_DISCLOSURE_STORAGE_PREFIX + encodeURIComponent(clientId);
}

export function storedDeviceDisclosure(clientId: string): boolean | null {
  try {
    const value = window.localStorage.getItem(deviceDisclosureStorageKey(clientId));
    return value === "open" ? true : value === "closed" ? false : null;
  } catch {
    return null;
  }
}

export function persistDeviceDisclosure(clientId: string, open: boolean): void {
  try {
    window.localStorage.setItem(deviceDisclosureStorageKey(clientId), open ? "open" : "closed");
  } catch {
    /* Disclosure remains active for current render. */
  }
}
