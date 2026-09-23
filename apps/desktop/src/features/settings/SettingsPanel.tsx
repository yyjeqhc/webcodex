import { useEffect, useState, type ReactNode } from "react";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState, TunnelProxyMode } from "../../models/topology";
import { LANGUAGES, useLocale } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { useProduct } from "../../i18n/product";
import type { RunnerSettings } from "../../models/topology";
import { ComputerPermissions } from "./ComputerPermissions";
import { PowerShellInstallGuidance } from "./PowerShellInstallGuidance";
import { APPEARANCES, useAppearance } from "../../hooks/useAppearance";
import { AccentPicker } from "../../components/AccentPicker";
import { RuntimePanel } from "./RuntimePanel";
import { DiagnosticsPanel } from "./DiagnosticsPanel";
import { AboutPanel } from "./AboutPanel";
import { useShellText } from "../../i18n/runtime-shell";
import type { RuntimeUpdates } from "../../hooks/useRuntimeUpdates";

export function SettingsPanel({
  state,
  onState,
  onChangeSetup,
  onStopRuntime,
  onActivity,
  initialSection,
  updates,
}: {
  state: DesktopState;
  onState: (state: DesktopState) => void;
  onChangeSetup?: () => void;
  onStopRuntime?: () => void;
  onActivity?: () => void;
  initialSection?: "diagnostics" | "runtime";
  updates?: RuntimeUpdates;
}) {
  const { locale, setLocale, t } = useLocale();
  const s = useShellText();
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(initialSection === "diagnostics");
  const [runtimeOpen, setRuntimeOpen] = useState(initialSection === "runtime");
  const [networkOpen, setNetworkOpen] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  useEffect(() => { if (initialSection === "diagnostics") setDiagnosticsOpen(true); if (initialSection === "runtime") setRuntimeOpen(true); }, [initialSection]);
  const { appearance, setAppearance, accent, setAccent } = useAppearance();
  const p = useProduct();
  const [runnerSettings, setRunnerSettings] = useState<RunnerSettings | null>(null);
  const [restartingRunner, setRestartingRunner] = useState(false);
  const [runnerError, setRunnerError] = useState<DesktopError | null>(null);
  const [proxyMode, setProxyMode] = useState<TunnelProxyMode>(state.tunnel_proxy.mode);
  const [customProxy, setCustomProxy] = useState(state.tunnel_proxy.custom_url ?? "");
  const [savingProxy, setSavingProxy] = useState(false);
  const [proxyError, setProxyError] = useState<DesktopError | null>(null);
  const [launchAtLogin, setLaunchAtLogin] = useState<boolean | null>(null);
  const [savingLaunchAtLogin, setSavingLaunchAtLogin] = useState(false);
  const [launchAtLoginError, setLaunchAtLoginError] = useState<DesktopError | null>(null);
  const operationBusy = Boolean(state.current_operation);

  useEffect(() => {
    let cancelled = false;
    void desktopApi.runnerSettings().then(next => { if (!cancelled) setRunnerSettings(next); }).catch(() => undefined);
    void desktopApi.getLaunchAtLogin().then((enabled) => {
      if (!cancelled) setLaunchAtLogin(enabled);
    }).catch((value) => {
      if (!cancelled) setLaunchAtLoginError(normalizeDesktopError(value));
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const saveProxy = async () => {
    if (operationBusy) return;
    setSavingProxy(true);
    setProxyError(null);
    try {
      onState(await desktopApi.updateTunnelProxy(proxyMode, customProxy));
    } catch (value) {
      setProxyError(normalizeDesktopError(value));
    } finally {
      setSavingProxy(false);
    }
  };

  const updateLaunchAtLogin = async (enabled: boolean) => {
    if (savingLaunchAtLogin) return;
    setSavingLaunchAtLogin(true);
    setLaunchAtLoginError(null);
    try {
      setLaunchAtLogin(await desktopApi.setLaunchAtLogin(enabled));
    } catch (value) {
      setLaunchAtLoginError(normalizeDesktopError(value));
    } finally {
      setSavingLaunchAtLogin(false);
    }
  };
  return (
    <section className="page-section workspace-page settings-page" aria-labelledby="settings-title" data-webcodex-page="settings">
      <header className="page-heading-row"><h1 id="settings-title">{t("settings.title")}</h1></header>
      <section className="settings-section" aria-labelledby="settings-general-title">
        <h2 id="settings-general-title">{p("general")}</h2>
        <div className="setting-row"><label htmlFor="desktop-settings-locale">{t("locale.label")}</label><select id="desktop-settings-locale" value={locale} onChange={event => setLocale(event.target.value as typeof locale)} data-webcodex-control="locale">{LANGUAGES.map(language => <option key={language.value} value={language.value}>{language.label}</option>)}</select></div>
        <div className="setting-row"><label htmlFor="desktop-settings-appearance">{t("appearance.label")}</label><select id="desktop-settings-appearance" value={appearance} onChange={event => setAppearance(event.target.value as typeof appearance)} data-webcodex-control="appearance">{APPEARANCES.map(value => <option key={value} value={value}>{t(`appearance.${value}`)}</option>)}</select></div>
        <div className="setting-row"><span>{t("accent.label")}</span><AccentPicker color={accent} onChange={setAccent} /></div>
        <div className="setting-row"><label htmlFor="desktop-launch-at-login">{t("settings.launchAtLogin")}</label><input id="desktop-launch-at-login" type="checkbox" checked={launchAtLogin ?? false} onChange={event => void updateLaunchAtLogin(event.target.checked)} disabled={launchAtLogin === null || savingLaunchAtLogin} data-webcodex-control="launch-at-login" /></div>
        {launchAtLoginError && <SettingsError error={launchAtLoginError} />}
        <div className="setting-row"><span>{p("background")}</span><span className="setting-value">{p("keepRunning")}</span></div>
      </section>
      <ComputerPermissions />
      <SettingsDisclosure id="desktop-settings-diagnostics" label={s("Troubleshooting")} open={diagnosticsOpen} onOpenChange={setDiagnosticsOpen}>
        <DiagnosticsPanel state={state} onState={onState} />
      </SettingsDisclosure>
      <SettingsDisclosure id="desktop-settings-runtime" label={s("Runtime")} open={runtimeOpen} onOpenChange={setRuntimeOpen}>
        <RuntimePanel state={state} onState={onState} onActivity={onActivity} />
      </SettingsDisclosure>
      <SettingsDisclosure id="desktop-settings-network" label={p("network")} open={networkOpen} onOpenChange={setNetworkOpen}>
        <div className="field-group"><label htmlFor="desktop-tunnel-proxy-mode">{t("settings.tunnelProxy")}</label><select id="desktop-tunnel-proxy-mode" value={proxyMode} onChange={event => setProxyMode(event.target.value as TunnelProxyMode)} disabled={savingProxy || operationBusy} data-webcodex-control="tunnel-proxy-mode"><option value="auto">{t("settings.tunnelProxyAuto")}</option><option value="direct">{t("settings.tunnelProxyDirect")}</option><option value="custom">{t("settings.tunnelProxyCustom")}</option></select></div>
        {proxyMode === "custom" && <div className="field-group"><label htmlFor="desktop-tunnel-proxy-url">{t("settings.tunnelProxyCustomUrl")}</label><input id="desktop-tunnel-proxy-url" value={customProxy} onChange={event => setCustomProxy(event.target.value)} placeholder="http://127.0.0.1:7890" disabled={savingProxy || operationBusy} spellCheck={false} data-webcodex-control="tunnel-proxy-url" /></div>}
        <dl className="detail-list">
          <div><dt>{t("settings.tunnelProxyEffective")}</dt><dd>{state.tunnel_proxy.effective_proxy_present ? state.tunnel_proxy.effective_source : t("settings.tunnelProxyDirectValue")}</dd></div>
          <div><dt>{t("settings.tunnelProxyDetected")}</dt><dd>{state.tunnel_proxy.system_proxy_detected ? p("available") : p("notConfigured")}</dd></div>
        </dl>
        <button type="button" className="secondary-button" onClick={() => void saveProxy()} disabled={savingProxy || operationBusy || (proxyMode === "custom" && !customProxy.trim())} data-webcodex-action="save-tunnel-proxy">{savingProxy ? p("loading") : p("saveApply")}</button>
        {proxyError && <SettingsError error={proxyError} />}
      </SettingsDisclosure>
      <SettingsDisclosure id="desktop-settings-advanced" label={p("advanced")} open={advancedOpen} onOpenChange={setAdvancedOpen}>
        <div className="connection-actions">
          {onChangeSetup && <button type="button" className="secondary-button" disabled={operationBusy} onClick={onChangeSetup}>{p("serverConnection")}</button>}
          {onStopRuntime && state.topology?.server.kind === "local" && state.readiness.runtime_ready && <button type="button" className="secondary-button" disabled={operationBusy} onClick={onStopRuntime}>{p("stop")} WebCodex</button>}
        </div>
        {runnerSettings && <button type="button" className="secondary-button" disabled={operationBusy || restartingRunner || !runnerSettings.can_restart} data-webcodex-action="restart-owned-runner" onClick={async () => {
          if (operationBusy || restartingRunner) return;
          setRestartingRunner(true); setRunnerError(null);
          try { onState(await desktopApi.restartOwnedRunner(runnerSettings.target)); }
          catch (value) { setRunnerError(normalizeDesktopError(value)); }
          finally { setRestartingRunner(false); }
        }}>{p("restartRunner")}</button>}
        {runnerError && <SettingsError error={runnerError} />}
        {runnerSettings && <dl className="detail-list"><div><dt>Runner</dt><dd>{runnerSettings.target.config_path}</dd></div></dl>}
        <PowerShellInstallGuidance state={state} onState={onState} />
      </SettingsDisclosure>
      <AboutPanel state={state} updates={updates} />
    </section>
  );
}

function SettingsDisclosure({
  id,
  label,
  open,
  onOpenChange,
  children,
}: {
  id: string;
  label: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: ReactNode;
}) {
  return (
    <>
      <button
        type="button"
        className="settings-disclosure-trigger"
        aria-label={label}
        aria-expanded={open}
        aria-controls={id}
        onClick={() => onOpenChange(!open)}
      >
        {label}
      </button>
      {open && (
        // Keep the controlled panel out of the macOS named-landmark path. WebKit
        // currently exposes named regions as AX leaves (AXChildren=0), which
        // makes otherwise standard descendants unreachable to semantic Computer Use.
        // The trigger still carries the disclosure contract through
        // aria-expanded + aria-controls, while the descendants retain their own
        // native heading/input/button semantics.
        <div id={id} className="settings-disclosure-panel" data-webcodex-disclosure-panel={label}>
          {children}
        </div>
      )}
    </>
  );
}

function SettingsError({ error }: { error: DesktopError }) {
  const { t } = useLocale();
  const presentation = desktopErrorPresentation(error, t);
  return (
    <div className="error-card" role="alert">
      <strong>{presentation.title}</strong>
      <span>{presentation.action}</span>
      <details>
        <summary>{t("common.details")}</summary>
        <code>{error.code}</code>
        <p>{error.message}</p>
      </details>
    </div>
  );
}
