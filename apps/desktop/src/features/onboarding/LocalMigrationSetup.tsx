import { useEffect, useRef, useState } from "react";
import { Select } from "@mantine/core";
import { useLocale, type MessageKey } from "../../i18n/locale";
import { normalizeDesktopError } from "../../i18n/presentation";
import { useProduct } from "../../i18n/product";
import { useServiceScopeText } from "../../i18n/service-scope";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";

const recoveryHelp: Partial<Record<string, MessageKey>> = {
  legacy_runner_owner_unknown: "migration.runnerOwner",
  legacy_server_owner_unknown: "migration.serverOwner",
  migration_inventory_incomplete: "migration.inventory",
  migration_tunnel_startup_conflict: "migration.tunnelStartup",
  migration_runner_busy: "migration.jobs",
  legacy_runner_layout_unsupported: "migration.layout",
  legacy_runner_configuration: "migration.layout",
  legacy_project_registry: "migration.projectRegistry",
  legacy_project_identity: "migration.projectRegistry",
  legacy_tunnel_credentials: "migration.tunnelCredentials",
  legacy_tunnel_profile: "migration.tunnelCredentials",
  legacy_tunnel_owner: "migration.tunnelCredentials",
  migration_reset_available: "migration.resetAvailable",
  migration_recovery_required: "migration.recovery",
  migration_target_conflict: "migration.changed",
  legacy_server_binding: "migration.changed",
  legacy_runner_binding: "migration.changed",
  legacy_data_path: "migration.changed",
  legacy_user_credential: "migration.changed",
  legacy_identity: "migration.changed",
  legacy_server_configuration: "migration.changed",
  legacy_env_ambiguous: "migration.changed",
  legacy_process_changed: "migration.changed",
  legacy_owner_changed: "migration.changed",
  migration_conflict: "migration.changed",
};

export function LocalMigrationSetup({ state, onComplete, onCancel, onDiagnostics, onReset }: {
  state: DesktopState;
  onComplete: (next: DesktopState) => void;
  onCancel?: () => void;
  onDiagnostics?: () => void;
  onReset?: (next: DesktopState) => void;
}) {
  const { t } = useLocale();
  const p = useProduct();
  const scopeText = useServiceScopeText();
  const [preview, setPreview] = useState<Awaited<ReturnType<typeof desktopApi.inspectLocalMigration>> | null>(null);
  const [scope, setScope] = useState<"user" | "system">("user");
  const [error, setError] = useState<DesktopError | null>(null);
  const [busy, setBusy] = useState(false);
  const submitting = useRef(false);
  const mounted = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const disabled = busy || Boolean(state.current_operation);
  const canReset = error?.code === "migration_reset_available";
  const proceed = async () => {
    if (submitting.current || disabled) return;
    submitting.current = true;
    setBusy(true);
    setError(null);
    try {
      if (canReset) {
        const cleaned = await desktopApi.resetLocalMigration();
        if (mounted.current) onReset?.(cleaned);
        const result = await desktopApi.inspectLocalMigration();
        if (mounted.current) setPreview(result);
      } else if (preview) {
        const next = await desktopApi.configureEnvironment({
          mode: "create", expectedLocalTarget: { kind: "legacy", fingerprint: preview.fingerprint },
          ...(preview.resuming ? {} : { serviceScope: scope }),
        });
        if (mounted.current) onComplete(next);
      } else {
        const result = await desktopApi.inspectLocalMigration();
        if (mounted.current) setPreview(result);
      }
    } catch (cause) {
      if (mounted.current) {
        setPreview(null);
        setError(normalizeDesktopError(cause));
      }
    } finally {
      submitting.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  const recovery = error ? recoveryHelp[error.code] ?? "migration.genericError" : null;
  return <section className="setup-shell migration-setup" aria-labelledby="migration-heading" aria-busy={busy}>
    <h2 id="migration-heading">{t("migration.title")}</h2>
    <p>{t("migration.help")}</p>
    {preview && <>
      {preview.resuming && <p role="status">{t("migration.resume")}</p>}
      {!preview.resuming && <p className="workspace-notice">{t("migration.impact")}</p>}
      {!preview.resuming && <details className="setup-options"><summary>{t("first.advanced")}</summary>
        <Select label={scopeText("title")} value={scope} allowDeselect={false} disabled={disabled}
          data={[{ value: "user", label: scopeText("user") }, { value: "system", label: scopeText("system") }]}
          onChange={v => { if (v === "user" || v === "system") setScope(v); }} />
        <p className="field-help">{scopeText(scope === "system" ? "systemHelp" : "userHelp")}</p>
      </details>}
    </>}
    {error && <div className="error-card" role="alert">
      <strong>{t("migration.attention")}</strong><p>{recovery ? t(recovery) : t("migration.genericError")}</p>
      <details><summary>{t("common.details")}</summary><code>{error.code}</code><p>{error.message}</p><p>{error.next_action}</p></details>
      {onDiagnostics && <button type="button" className="text-button" onClick={onDiagnostics}>{p("troubleshooting")}</button>}
    </div>}
    <div className="setup-actions">
      <button type="button" className="primary-button" disabled={disabled} onClick={() => void proceed()}>
        {t(busy ? "common.checking" : canReset ? "migration.resetAndRecheck" : preview?.resuming ? "migration.continue" : preview ? "migration.confirm" : error ? "common.retry" : "migration.check")}
      </button>
      {(onCancel || preview || error) && <button type="button" className="secondary-button" disabled={busy} onClick={onCancel ?? (() => { setPreview(null); setError(null); })}>{p("cancel")}</button>}
    </div>
  </section>;
}
