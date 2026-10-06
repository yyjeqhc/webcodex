import { useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { save } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";
import type { InventoryDocumentKind, PathEntry, PathInventory } from "../../models/path-inventory";
import { useConfigurationData, type ConfigurationDataKey } from "../../i18n/configuration-data";
import { useLocale } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";

function purposeKey(entry: PathEntry): ConfigurationDataKey {
  if (entry.id === "environment.root") return "environment_root";
  if (entry.id === "desktop.root") return "desktop_root";
  const semantics: Record<string, ConfigurationDataKey> = { service_journal: "log", service_lifecycle: "log", service_diagnostics: "log", request_trace: "log", project_registration: "registry", runtime_binary: "binary", computer_session: "computer_session", upgrade_recovery: "upgrade_backups", project_reference: "project" };
  if (semantics[entry.purpose]) return semantics[entry.purpose];
  const purpose = entry.id.replaceAll(".", "_");
  const exact: Record<string, ConfigurationDataKey> = { environment_setup: "setup", environment_user_credential: "credential", environment_enrollment_recovery: "enrollment_recovery", environment_project_addition: "project_change", environment_project_removal: "project_change", environment_migration: "migration", environment_upgrade: "upgrade", environment_upgrade_backups: "upgrade_backups", runtime_cli: "binary", runtime_server: "binary", runtime_runner: "binary", environment_record: "environment_record", desktop_settings: "configuration", desktop_settings_recovery: "recovery", desktop_locale: "locale", desktop_tunnel_config: "configuration", desktop_download_cache: "cache", desktop_activity: "activity", desktop_process_output: "output", tunnel_profiles: "profiles" };
  if (exact[entry.purpose]) return exact[entry.purpose];
  if (exact[purpose]) return exact[purpose];
  for (const [suffix, key] of [["configuration", "configuration"], ["database", "database"], ["data", "data"], ["trace", "log"], ["lifecycle_log", "log"], ["registry", "registry"], ["computer_session", "computer_session"], ["project_reference", "project"]] as const) {
    if (purpose.endsWith(`_${suffix}`) || entry.purpose.endsWith(`_${suffix}`)) return key;
  }
  return "unknown";
}
const sourceKeys: Record<string, ConfigurationDataKey> = { selected: "selected", saved_record: "saved_record", saved_journal: "saved_journal", configured: "configured", derived: "derived", platform: "platform", tauri: "platform", environment: "environment", desktop_state: "desktop_state", desktop_preferences: "desktop_preferences", in_memory: "in_memory" };
const issueKeys: Record<string, ConfigurationDataKey> = { environment_changed: "changed", inventory_truncated: "limited", tunnel_profiles_truncated: "limited", project_references_truncated: "limited", runner_binding_unconfirmed: "runner_binding", environment_record_invalid: "record_unavailable", environment_record_unavailable: "record_unavailable" };
const inventoryErrorCodes = new Set(["inventory_incomplete", "inventory_changed", "inventory_location_unavailable", "inventory_export_file_exists_or_unavailable", "inventory_export_managed_path", "inventory_export_path_invalid", "inventory_export_path_unconfirmed", "inventory_serialization_failed", "inventory_too_large", "inventory_write_unconfirmed"]);
function inventoryError(value: unknown): DesktopError {
  const error = normalizeDesktopError(value);
  return { code: inventoryErrorCodes.has(error.code) ? error.code : "inventory_unavailable", message: "", next_action: "refresh" };
}
const componentNames: Record<string, string> = { desktop: "Desktop", environment: "Environment", server: "Server", runner: "Runner", tunnel: "Tunnel", runtime: "Runtime" };

export function ConfigurationDataPanel({ state }: { state: DesktopState }) {
  const c = useConfigurationData(); const { t } = useLocale();
  // This key is identity only. It never reaches native commands or exported files.
  const context = JSON.stringify([state.persistent_environment ?? null, state.topology ?? null, state.binaries ?? null, state.workspace_runner ?? null]);
  const currentContext = useRef(context); currentContext.current = context;
  const generation = useRef(0);
  const inFlight = useRef<number | null>(null);
  const [snapshot, setSnapshot] = useState<{ context: string; inventory: PathInventory } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [notice, setNotice] = useState<ConfigurationDataKey | null>(null);
  const data = snapshot?.context === context ? snapshot.inventory : null;
  const refresh = async () => {
    const request = ++generation.current; const identity = context;
    inFlight.current = null;
    setSnapshot(null); setBusy(true); setError(null); setNotice(null);
    try {
      const inventory = await desktopApi.pathInventory();
      if (generation.current === request && currentContext.current === identity) setSnapshot({ context: identity, inventory });
    } catch (value) {
      if (generation.current === request && currentContext.current === identity) setError(inventoryError(value));
    } finally {
      if (generation.current === request && currentContext.current === identity) setBusy(false);
    }
  };
  useEffect(() => {
    void refresh();
    return () => { generation.current++; };
  }, [context]); // State changes invalidate observations before the next request.
  const disabled = busy || Boolean(state.current_operation);
  const exportDisabled = disabled || Boolean(data?.issues.some(issue => issue.code.endsWith("_truncated")));
  const act = async (action: (valid: () => boolean) => Promise<ConfigurationDataKey | null>) => {
    if (disabled || !data || inFlight.current === generation.current) return;
    const request = generation.current; const identity = context;
    inFlight.current = request;
    const valid = () => request === generation.current && identity === currentContext.current;
    setBusy(true); setError(null); setNotice(null);
    try { const result = await action(valid); if (valid()) setNotice(result); }
    catch (value) { if (valid()) { setError(inventoryError(value)); setSnapshot(null); } }
    finally { if (valid()) { inFlight.current = null; setBusy(false); } }
  };
  const exportDocument = (kind: InventoryDocumentKind) => act(async valid => {
    const path = await save({ title: c(kind === "inventory" ? "inventory" : "manifest"), defaultPath: kind === "inventory" ? "webcodex-path-inventory.json" : "webcodex-backup-manifest.json", filters: [{ name: "JSON", extensions: ["json"] }] });
    if (!path || !valid()) return null;
    await desktopApi.exportInventoryDocument(kind, path, data!.revision);
    return "exported";
  });
  const renderEntry = (entry: PathEntry) => {
    const local = entry.status === "present" && (entry.kind === "file" || entry.kind === "directory");
    const knownPath = local ? entry.canonical_path : null;
    const unitName = entry.log_source?.unit_name;
    const safeUnitName = unitName && /^[a-zA-Z0-9_.@ -]{1,200}$/.test(unitName) ? unitName : null;
    return <article key={entry.id} className="configuration-location">
      <h4>{entry.component === "environment" ? c("environment_component") : componentNames[entry.component] ?? c("unknown")} · {c(purposeKey(entry))}</h4>
      <p className="configuration-status">{c(entry.status)}</p>
      {entry.kind === "system_log" && <p className="field-help">{c(entry.log_source?.kind === "systemd_journal" ? "journal_view" : entry.log_source?.kind === "windows_events" ? "windows_events" : entry.log_source?.kind === "task_scheduler" ? "task_scheduler" : "journal")}</p>}
      {entry.kind === "in_memory" && <p className="field-help">{c("memory")}</p>}
      <dl className="detail-list">
        {safeUnitName && <div><dt>{c("unit")}</dt><dd><code>{safeUnitName}</code></dd></div>}
        {entry.log_source?.service_scope && <div><dt>{c("source")}</dt><dd>{c(entry.log_source.service_scope)}</dd></div>}
        <div><dt>{c("source")}</dt><dd>{c(sourceKeys[entry.source] ?? "unknown")}</dd></div>
        {(entry.kind === "file" || entry.kind === "directory" || entry.kind === "remote_reference") && <>
          <div><dt>{c("path")}</dt><dd><code>{entry.configured_path ?? c("unknown")}</code></dd></div>
          {entry.canonical_path && entry.canonical_path !== entry.configured_path && <div><dt>{c("canonical")}</dt><dd><code>{entry.canonical_path}</code></dd></div>}
        </>}
      </dl>
      <div className="shell-actions">
        {knownPath && <button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async () => { await writeText(knownPath); return "copied"; })}>{c("copy")}</button>}
        {local && entry.directory_to_open && <button type="button" className="secondary-button" disabled={disabled} onClick={() => void act(async () => { await desktopApi.openInventoryLocation(entry.id, data!.revision); return null; })}>{c("open")}</button>}
      </div>
    </article>;
  };
  const presentation = error ? desktopErrorPresentation(error, t) : null;
  return <section className="settings-section configuration-data" aria-labelledby="configuration-data-title">
    <h2 id="configuration-data-title">{c("title")}</h2>
    <button type="button" className="secondary-button" disabled={Boolean(state.current_operation)} onClick={() => void refresh()}>{c("refresh")}</button>
    {busy && <p role="status">{c("loading")}</p>}
    {data && <>
      {data.issues.length > 0 && <div className="workspace-notice" role="status"><p>{c("partial")}</p>{[...new Set(data.issues.map(issue => issueKeys[issue.code]).filter((key): key is ConfigurationDataKey => Boolean(key)))].map(key => <p key={key}>{c(key)}</p>)}</div>}
      <section aria-labelledby="configuration-roots-title"><h3 id="configuration-roots-title">{c("roots")}</h3>{data.roots.length ? data.roots.map(renderEntry) : <p>{c("empty")}</p>}</section>
      <section aria-labelledby="configuration-entries-title"><h3 id="configuration-entries-title">{c("entries")}</h3>{data.entries.length ? data.entries.map(renderEntry) : <p>{c("empty")}</p>}</section>
      <p className="workspace-notice" id="configuration-export-notice">{c("privacy")}</p>
      <div className="shell-actions" aria-describedby="configuration-export-notice">
        <button type="button" className="secondary-button" disabled={exportDisabled} onClick={() => void exportDocument("inventory")}>{c("inventory")}</button>
        <button type="button" className="secondary-button" disabled={exportDisabled} onClick={() => void exportDocument("backup_manifest")}>{c("manifest")}</button>
      </div>
    </>}
    {notice && <p role="status">{c(notice)}</p>}
    {presentation && <div className="error-card" role="alert"><strong>{presentation.title}</strong><span>{presentation.action}</span><code>{error!.code}</code></div>}
  </section>;
}
