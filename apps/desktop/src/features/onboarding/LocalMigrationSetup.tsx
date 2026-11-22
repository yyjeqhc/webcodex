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
  migration_recovery_required: "migration.recovery",
  migration_target_conflict: "migration.changed",
};

export function LocalMigrationSetup({ state, onComplete, onCancel, onDiagnostics }: {
  state: DesktopState;
  onComplete: (next: DesktopState) => void;
  onCancel?: () => void;
  onDiagnostics?: () => void;
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
  const proceed = async () => {
    if (submitting.current || disabled) return;
    submitting.current = true;
    setBusy(true);
    setError(null);
    try {
      if (preview) {
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
  const recovery = error && recoveryHelp[error.code];
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
      <strong>{t("migration.attention")}</strong><p>{recovery ? t(recovery) : error.next_action}</p>
      <details><summary>{t("common.details")}</summary><code>{error.code}</code><p>{error.message}</p><p>{error.next_action}</p></details>
      {onDiagnostics && <button type="button" className="text-button" onClick={onDiagnostics}>{p("troubleshooting")}</button>}
    </div>}
    <div className="setup-actions">
      <button type="button" className="primary-button" disabled={disabled} onClick={() => void proceed()}>
        {t(busy ? "common.checking" : preview?.resuming ? "migration.continue" : preview ? "migration.confirm" : error ? "common.retry" : "migration.check")}
      </button>
      {(onCancel || preview || error) && <button type="button" className="secondary-button" disabled={busy} onClick={onCancel ?? (() => { setPreview(null); setError(null); })}>{p("cancel")}</button>}
    </div>
  </section>;
}
