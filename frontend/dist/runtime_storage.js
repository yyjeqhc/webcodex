export const RUNTIME_CREDENTIAL_SESSION_KEY = "webpi.runtime.credential.v1";
export const APPEARANCE_STORAGE_KEY = "webpi.runtime.appearance.v1";
export const WORKSPACE_VIEW_STORAGE_KEY = "webpi.runtime.workspace-view.v1";
export const DRAFT_STORAGE_PREFIX = "webpi.runtime.draft.v1.";
export const DEVICE_DISCLOSURE_STORAGE_PREFIX = "webpi.runtime.runner-open.v1.";
export const APPEARANCE_MEDIA_QUERY = "(prefers-color-scheme: light)";
export function appearancePreference(value) {
    return value === "light" || value === "dark" || value === "system" ? value : "system";
}
export function loadAppearancePreference() {
    try {
        return appearancePreference(window.localStorage.getItem(APPEARANCE_STORAGE_KEY));
    }
    catch {
        return "system";
    }
}
export function persistAppearancePreference(preference) {
    try {
        window.localStorage.setItem(APPEARANCE_STORAGE_KEY, preference);
    }
    catch {
        /* Storage can be unavailable in hardened browser contexts. */
    }
}
export function resolvedAppearance(preference, prefersLight) {
    if (preference !== "system")
        return preference;
    return prefersLight ? "light" : "dark";
}
export function workspaceViewPreference(value) {
    return value === "sessions" || value === "operations" || value === "windows" || value === "projects" || value === "activity" || value === "extensions" ? value : "home";
}
export function loadWorkspaceViewPreference() {
    try {
        return workspaceViewPreference(window.localStorage.getItem(WORKSPACE_VIEW_STORAGE_KEY));
    }
    catch {
        return "home";
    }
}
export function persistWorkspaceViewPreference(view) {
    try {
        window.localStorage.setItem(WORKSPACE_VIEW_STORAGE_KEY, view);
    }
    catch {
        /* Storage can be unavailable in hardened browser contexts. */
    }
}
export function loadRememberedRuntimeCredential() {
    try {
        return window.sessionStorage.getItem(RUNTIME_CREDENTIAL_SESSION_KEY)?.trim() || "";
    }
    catch {
        return "";
    }
}
export function persistRuntimeCredentialForTab(token, remember) {
    try {
        if (remember && token) {
            window.sessionStorage.setItem(RUNTIME_CREDENTIAL_SESSION_KEY, token);
        }
        else {
            window.sessionStorage.removeItem(RUNTIME_CREDENTIAL_SESSION_KEY);
        }
    }
    catch {
        /* Storage can be unavailable in hardened browser contexts. */
    }
}
export function clearRememberedRuntimeCredential() {
    try {
        window.sessionStorage.removeItem(RUNTIME_CREDENTIAL_SESSION_KEY);
    }
    catch {
        /* Storage can be unavailable in hardened browser contexts. */
    }
}
export function currentDraftStorageKey(project, sessionId) {
    const projectId = String(project || "");
    const workflowSessionId = String(sessionId || "");
    return projectId && workflowSessionId
        ? DRAFT_STORAGE_PREFIX + encodeURIComponent(projectId) + "." + encodeURIComponent(workflowSessionId)
        : "";
}
export function loadDraft(project, sessionId) {
    const key = currentDraftStorageKey(project, sessionId);
    if (!key)
        return "";
    try {
        return window.sessionStorage.getItem(key) || "";
    }
    catch {
        return "";
    }
}
export function saveDraft(project, sessionId, text) {
    const key = currentDraftStorageKey(project, sessionId);
    if (!key)
        return;
    try {
        if (text)
            window.sessionStorage.setItem(key, text);
        else
            window.sessionStorage.removeItem(key);
    }
    catch {
        /* Draft remains in active input when storage is unavailable. */
    }
}
export function clearDraft(project, sessionId) {
    const key = currentDraftStorageKey(project, sessionId);
    if (!key)
        return;
    try {
        window.sessionStorage.removeItem(key);
    }
    catch {
        /* No-op in hardened browser contexts. */
    }
}
export function clearRuntimeDrafts() {
    try {
        const keys = [];
        for (let index = 0; index < window.sessionStorage.length; index += 1) {
            const key = window.sessionStorage.key(index);
            if (key?.startsWith(DRAFT_STORAGE_PREFIX))
                keys.push(key);
        }
        for (const key of keys)
            window.sessionStorage.removeItem(key);
    }
    catch {
        /* No-op in hardened browser contexts. */
    }
}
export function deviceDisclosureStorageKey(clientId) {
    return DEVICE_DISCLOSURE_STORAGE_PREFIX + encodeURIComponent(clientId);
}
export function storedDeviceDisclosure(clientId) {
    try {
        const value = window.localStorage.getItem(deviceDisclosureStorageKey(clientId));
        return value === "open" ? true : value === "closed" ? false : null;
    }
    catch {
        return null;
    }
}
export function persistDeviceDisclosure(clientId, open) {
    try {
        window.localStorage.setItem(deviceDisclosureStorageKey(clientId), open ? "open" : "closed");
    }
    catch {
        /* Disclosure remains active for current render. */
    }
}
