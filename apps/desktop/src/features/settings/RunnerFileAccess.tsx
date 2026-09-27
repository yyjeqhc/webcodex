import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import { normalizeDesktopError } from "../../i18n/presentation";
import { useProduct } from "../../i18n/product";
import type { DesktopError, DesktopState, RunnerSettings } from "../../models/topology";

export function RunnerFileAccess({
  settings,
  disabled,
  onState,
  onSettings,
}: {
  settings: RunnerSettings | null;
  disabled: boolean;
  onState: (state: DesktopState) => void;
  onSettings: (settings: RunnerSettings) => void;
}) {
  const p = useProduct();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const access = settings?.file_access;

  const apply = async (roots: string[]) => {
    if (!settings || busy || disabled) return;
    setBusy(true);
    setError(null);
    try {
      const next = await desktopApi.updateRunnerAllowedRoots(settings.target, access?.configured_roots ?? [], roots);
      onState(next);
      onSettings(await desktopApi.runnerSettings());
    } catch (value) {
      setError(normalizeDesktopError(value));
      try { onSettings(await desktopApi.runnerSettings()); } catch { /* keep the last confirmed view */ }
    } finally {
      setBusy(false);
    }
  };

  const add = async () => {
    if (!settings || busy || disabled) return;
    try {
      const selected = await open({ title: p("addFolder"), directory: true, multiple: false });
      if (typeof selected !== "string") return;
      const base = access?.using_default_roots ? access.effective_roots : (access?.configured_roots ?? []);
      if (base.includes(selected)) return;
      await apply([...base, selected]);
    } catch (value) {
      setError(normalizeDesktopError(value));
    }
  };

  return <section className="settings-section" aria-labelledby="settings-file-access-title" data-webcodex-section="file-access">
    <h2 id="settings-file-access-title">{p("fileAccess")}</h2>
    <div className="field-group">
      <strong>{p("runnerAllowedFolders")}</strong>
      <p>{p("fileAccessHelp")}</p>
    </div>
    {!settings && <p className="workspace-notice">{p("loading")}</p>}
    {settings && access?.using_default_roots && <div className="workspace-notice"><p>{p("noCustomFolders")}</p><strong>{p("currentDefaultAccess")}</strong><PathList paths={access.effective_roots} /></div>}
    {settings && access && !access.using_default_roots && <PathList paths={access.configured_roots} removable={access.configured_roots.length > 1} disabled={busy || disabled} onRemove={path => void apply(access.configured_roots.filter(root => root !== path))} />}
    {settings && access && !access.using_default_roots && <button type="button" className="text-button" disabled={busy || disabled} onClick={() => void apply([])} data-webcodex-action="restore-default-file-access">{p("restoreDefaultAccess")}</button>}
    {settings && access && !access.using_default_roots && <details><summary>{p("effectiveAccess")}</summary><PathList paths={access.effective_roots} /></details>}
    {settings && access?.allow_cwd_anywhere && <p className="workspace-notice" role="note">{p("fileAccessBroadPolicy")}</p>}
    <button type="button" className="secondary-button" disabled={!settings || busy || disabled} onClick={() => void add()} data-webcodex-action="add-allowed-root">{busy ? p("loading") : p("addFolder")}</button>
    {error && <div className="error-card" role="alert"><strong>{error.message}</strong><span>{error.next_action}</span><code>{error.code}</code></div>}
  </section>;
}

function PathList({ paths, removable = false, disabled = false, onRemove }: { paths: string[]; removable?: boolean; disabled?: boolean; onRemove?: (path: string) => void }) {
  const p = useProduct();
  return <ul className="settings-path-list">{paths.map(path => <li key={path}><code>{path}</code>{removable && <button type="button" className="text-button" aria-label={`${p("removeFolder")}: ${path}`} disabled={disabled} onClick={() => onRemove?.(path)}>{p("removeFolder")}</button>}</li>)}</ul>;
}
