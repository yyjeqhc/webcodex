import { useEffect, useId, useState } from "react";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";

export function TunnelConfigDiagnostics({ state, onState, onApplied, onCancel }: {
  state: DesktopState; onState: (state: DesktopState) => void; onApplied?: () => void; onCancel?: () => void;
}) {
  const { t } = useLocale(); const p = useProduct(); const inputId = useId();
  const config = state.openai_tunnel_config;
  const [tunnelId, setTunnelId] = useState(config.effective_tunnel_id ?? config.saved_tunnel_id ?? "");
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const busy = saving || Boolean(state.current_operation);
  useEffect(() => { setTunnelId(config.effective_tunnel_id ?? config.saved_tunnel_id ?? ""); }, [config.saved_tunnel_id, config.effective_tunnel_id]);
  const save = async (useEnvironment = false) => {
    if (busy) return;
    const request = useEnvironment ? { action: "use_environment" as const } : { action: "save" as const, tunnelId, apiKey: apiKey || null };
    setApiKey(""); setSaving(true); setSaved(false); setError(null);
    try {
      onState(await desktopApi.updateTunnelConfig(request)); setSaved(true); onApplied?.();
    } catch (value) {
      setError(normalizeDesktopError(value));
      // A persisted configuration may outlive a failed tunnel restart. Observe,
      // never replay a write whose outcome is only partially known.
      try { onState(await desktopApi.getState()); } catch { /* Preserve the original failure. */ }
    } finally { setSaving(false); }
  };
  return <form className="connection-editor" aria-label={p("edit") + " Secure Tunnel"} data-webcodex-component="tunnel-config-diagnostics" onSubmit={event => { event.preventDefault(); void save(); }}>
    <div className="field-group"><label htmlFor={`${inputId}-id`}>Tunnel ID</label><input id={`${inputId}-id`} title="Tunnel ID" data-webcodex-control="tunnel-id" value={tunnelId} onChange={event => { setTunnelId(event.target.value); setSaved(false); }} placeholder="tunnel_…" maxLength={256} autoComplete="off" spellCheck={false} disabled={busy} required /></div>
    <div className="field-group"><label htmlFor={`${inputId}-key`}>API key</label><input id={`${inputId}-key`} title="API key" data-webcodex-control="tunnel-api-key" type="password" value={apiKey} onChange={event => { setApiKey(event.target.value); setSaved(false); }} maxLength={8192} autoComplete="new-password" spellCheck={false} disabled={busy} required={config.source !== "file"} aria-describedby={config.source === "file" ? `${inputId}-key-help` : undefined} />
      {config.source === "file" && <span className="field-help" id={`${inputId}-key-help`}>{p("savedKey")}</span>}
    </div>
    <div className="connection-actions"><button type="submit" className="primary-button" data-webcodex-action="save-tunnel-config" disabled={busy || !tunnelId.trim() || (config.source !== "file" && !apiKey.trim())}>{saving ? p("loading") : p("saveApply")}</button>{onCancel && <button type="button" className="secondary-button" onClick={onCancel} disabled={busy}>{p("cancel")}</button>}</div>
    {saved && <p role="status">{p("applied")}</p>}
    {error && <div className="error-card" role="alert"><strong>{desktopErrorPresentation(error, t).title}</strong><span>{desktopErrorPresentation(error, t).action}</span><details><summary>{p("details")}</summary><code>{error.code}</code><p>{error.message}</p></details></div>}
    <details className="workspace-technical"><summary>{p("advanced")}</summary><p>{t(config.source === "file" ? "tunnelConfig.sourceFile" : config.source === "invalid" ? "tunnelConfig.sourceInvalid" : "tunnelConfig.sourceEnvironment")}</p>
      {config.source !== "environment" && <button type="button" className="secondary-button" disabled={busy} onClick={() => void save(true)}>{t("tunnelConfig.useEnvironment")}</button>}
    </details>
  </form>;
}
