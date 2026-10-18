import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import { desktopIdentity, readDesktopObservation } from "../../lib/desktop-observation";
import { normalizeDesktopError } from "../../i18n/presentation";
import { useProduct } from "../../i18n/product";
import type { DesktopError, DesktopState, RunnerSettings, SettingsTarget } from "../../models/topology";

function displayAccessPath(path: string) {
  if (path.startsWith("\\\\?\\UNC\\")) return `\\\\${path.slice(8)}`;
  if (path.startsWith("\\\\?\\")) return path.slice(4);
  return path;
}

function accessPathKey(path: string) {
  const shown = displayAccessPath(path).replace(/\//g, "\\");
  return /^[A-Za-z]:\\/.test(shown) || shown.startsWith("\\\\")
    ? shown.toLocaleLowerCase("en-US")
    : shown;
}

function sameAccessPath(left: string, right: string) {
  return accessPathKey(left) === accessPathKey(right);
}

type AccessOperation = {
  generation: number;
  pickerGeneration: number;
  target: SettingsTarget;
  expected: string[];
  base: string[];
  controller: AbortController;
};

export function RunnerFileAccess({
  state,
  settings,
  disabled,
  loading = false,
  onState,
  onSettings,
}: {
  state: DesktopState;
  settings: RunnerSettings | null;
  disabled: boolean;
  loading?: boolean;
  onState: (state: DesktopState) => void;
  onSettings: (settings: RunnerSettings) => void;
}) {
  const p = useProduct();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const access = settings?.file_access;
  const defaultRoots = access?.default_roots ?? [];
  const configuredRoots = access?.configured_roots ?? [];
  const retainedDefaultRoots = configuredRoots.filter(root => defaultRoots.some(value => sameAccessPath(root, value)));
  const additionalRoots = configuredRoots.filter(root => !defaultRoots.some(value => sameAccessPath(root, value)));

  const identity = desktopIdentity(state);
  const boundary = JSON.stringify([identity, settings?.target, access]);
  const view = useRef({ boundary, disabled, generation: 0, pickerGeneration: 0 });
  // Remember transitions, including A -> B -> A and busy -> idle while the
  // native picker is open. Equality only at completion would miss these.
  if (view.current.boundary !== boundary) {
    view.current.boundary = boundary;
    view.current.generation++;
  }
  if (view.current.disabled !== disabled) {
    view.current.disabled = disabled;
    view.current.pickerGeneration++;
  }
  const alive = useRef(true);
  const action = useRef<AccessOperation | null>(null);
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; action.current?.controller.abort(); };
  }, []);
  const current = (operation: AccessOperation) => alive.current
    && !operation.controller.signal.aborted && action.current === operation && view.current.generation === operation.generation;
  const begin = (): AccessOperation | null => {
    if (!settings || !access || disabled || action.current || !alive.current) return null;
    const operation: AccessOperation = {
      generation: view.current.generation, pickerGeneration: view.current.pickerGeneration,
      target: { ...settings.target }, expected: [...access.configured_roots],
      base: [...(access.using_default_roots ? access.effective_roots : access.configured_roots)],
      controller: new AbortController(),
    };
    action.current = operation;
    setBusy(true); setError(null);
    return operation;
  };
  const finish = (operation: AccessOperation) => {
    if (action.current !== operation) return;
    action.current = null;
    if (alive.current) setBusy(false);
  };
  const write = async (operation: AccessOperation, roots: string[]) => {
    if (!current(operation)) return;
    let failed = false;
    try {
      // The native target and old-root compare-and-save checks remain decisive.
      const next = await desktopApi.updateRunnerAllowedRoots(operation.target, operation.expected, roots);
      if (!current(operation)) return;
      onState(next);
    } catch (value) {
      if (!current(operation)) return;
      failed = true;
      setError(normalizeDesktopError(value));
    }
    if (!current(operation)) return;
    try {
      const next = await readDesktopObservation(identity, operation.controller.signal, desktopApi.runnerSettings);
      if (!current(operation)) return;
      const target = next.target;
      if (target.config_path !== operation.target.config_path
        || target.client_id !== operation.target.client_id || target.server_url !== operation.target.server_url) {
        throw { code: "runner_settings_changed", message: p("settingsUnavailable"), next_action: p("refresh") };
      }
      onSettings(next);
    } catch (value) {
      if (current(operation) && !failed) setError(normalizeDesktopError(value));
    }
  };
  const apply = async (roots: string[]) => {
    const operation = begin();
    if (!operation) return;
    try { await write(operation, roots); }
    finally { finish(operation); }
  };
  const add = async () => {
    const operation = begin();
    if (!operation) return;
    try {
      const selected = await open({ title: p("addFolder"), directory: true, multiple: false });
      if (!current(operation) || view.current.disabled
        || operation.pickerGeneration !== view.current.pickerGeneration || typeof selected !== "string") return;
      if (operation.base.some(root => sameAccessPath(root, selected))) return;
      await write(operation, [...operation.base, selected]);
    } catch (value) {
      if (current(operation)) setError(normalizeDesktopError(value));
    } finally { finish(operation); }
  };

  return <section className="settings-section" aria-labelledby="settings-file-access-title" data-webcodex-section="file-access">
    <h2 id="settings-file-access-title">{p("fileAccess")}</h2>
    <div className="field-group">
      <strong>{p("runnerAllowedFolders")}</strong>
      <p>{p("fileAccessHelp")}</p>
    </div>
    {loading && <p className="workspace-notice" role="status">{p("loading")}</p>}
    {settings && access?.using_default_roots && <div className="workspace-notice"><p>{p("noCustomFolders")}</p><strong>{p("currentDefaultAccess")}</strong><PathList paths={access.effective_roots} /></div>}
    {settings && access && !access.using_default_roots && <>
      {retainedDefaultRoots.length > 0 && <div className="field-group"><strong>{p("currentDefaultAccess")}</strong><PathList paths={retainedDefaultRoots} /></div>}
      {additionalRoots.length > 0 && <div className="field-group"><strong>{p("additionalAuthorizedFolders")}</strong>
        <PathList paths={additionalRoots} removable={configuredRoots.length > 1} disabled={busy || disabled}
          onRemove={path => void apply(configuredRoots.filter(root => !sameAccessPath(root, path)))} />
      </div>}
    </>}
    {settings && access && !access.using_default_roots && <button type="button" className="text-button" disabled={busy || disabled} onClick={() => void apply([])} data-webcodex-action="restore-default-file-access">{p("restoreDefaultAccess")}</button>}
    {settings && access && !access.using_default_roots && <details><summary>{p("effectiveAccess")}</summary><PathList paths={access.effective_roots} /></details>}
    {settings && access?.allow_cwd_anywhere && <p className="workspace-notice" role="note">{p("fileAccessBroadPolicy")}</p>}
    <button type="button" className="secondary-button" disabled={!settings || busy || disabled} onClick={() => void add()} data-webcodex-action="add-allowed-root">{busy ? p("loading") : p("addFolder")}</button>
    {error && <div className="error-card" role="alert"><strong>{error.message}</strong><span>{error.next_action}</span><code>{error.code}</code></div>}
  </section>;
}

function PathList({ paths, removable = false, disabled = false, onRemove }: { paths: string[]; removable?: boolean; disabled?: boolean; onRemove?: (path: string) => void }) {
  const p = useProduct();
  return <ul className="settings-path-list">{paths.map(path => <li key={path}><code>{displayAccessPath(path)}</code>{removable && <button type="button" className="text-button" aria-label={`${p("removeFolder")}: ${displayAccessPath(path)}`} disabled={disabled} onClick={() => onRemove?.(path)}>{p("removeFolder")}</button>}</li>)}</ul>;
}
