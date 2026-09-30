import { useEffect, useState } from "react";
import { CircleHelp, FolderLock, Info, Monitor, Network, Settings2 } from "lucide-react";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState, TunnelProxyMode } from "../../models/topology";
import { LANGUAGES, useLocale } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { useProduct } from "../../i18n/product";
import type { RunnerSettings } from "../../models/topology";
import { ComputerPermissions } from "./ComputerPermissions";
import { RunnerFileAccess } from "./RunnerFileAccess";
import { PowerShellInstallGuidance } from "./PowerShellInstallGuidance";
import { APPEARANCES, useAppearance } from "../../hooks/useAppearance";
import { AccentPicker } from "../../components/AccentPicker";
import { RuntimePanel } from "./RuntimePanel";
import { DiagnosticsPanel } from "./DiagnosticsPanel";
import { AboutPanel } from "./AboutPanel";
import { LocalServicesPanel } from "./LocalServicesPanel";
import type { RuntimeUpdates } from "../../hooks/useRuntimeUpdates";

const SECTIONS = ["general", "access", "network", "runtime", "diagnostics", "about"] as const;
type SettingsSection = typeof SECTIONS[number];
const SECTION_ICONS = { general: Settings2, access: FolderLock, network: Network, runtime: Monitor, diagnostics: CircleHelp, about: Info };

export function SettingsPanel({
  state,
  onState,
  onChangeSetup,
  onStopRuntime,
  onActivity,
  onConnection,
  initialSection,
  updates,
}: {
  state: DesktopState;
  onState: (state: DesktopState) => void;
  onChangeSetup?: () => void;
  onStopRuntime?: () => void;
  onActivity?: () => void;
  onConnection?: () => void;
  initialSection?: "diagnostics" | "runtime" | "network" | "access";
  updates?: RuntimeUpdates;
}) {
  const { locale, setLocale, t } = useLocale();
  const [section, setSection] = useState<SettingsSection>(initialSection ?? "general");
  const [visited, setVisited] = useState<SettingsSection[]>([initialSection ?? "general"]);
  const selectSection = (next: SettingsSection) => {
    setSection(next);
    setVisited(current => current.includes(next) ? current : [...current, next]);
  };
  useEffect(() => { if (initialSection) selectSection(initialSection); }, [initialSection]);
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
  const configuredProxyMode = state.tunnel_proxy.mode === "auto" ? t("settings.tunnelProxyAuto") : state.tunnel_proxy.mode === "direct" ? t("settings.tunnelProxyDirect") : t("settings.tunnelProxyCustom");
  const effectiveProxyPath = state.tunnel_proxy.effective_source === "system" ? p("systemProxy") : state.tunnel_proxy.effective_source === "environment" ? p("environmentProxy") : state.tunnel_proxy.effective_source === "custom" ? p("customProxy") : state.tunnel_proxy.effective_source === "invalid_custom" ? t("settings.tunnelProxyCustom") : t("settings.tunnelProxyDirectValue");
  const detectedProxy = state.tunnel_proxy.effective_source === "environment" ? p("environmentProxy") : state.tunnel_proxy.system_proxy_detected ? p("systemProxy") : p("notConfigured");
  const labels: Record<SettingsSection, string> = { general: p("general"), access: p("accessAndPermissions"), network: p("network"), runtime: p("runtimeAndServices"), diagnostics: p("troubleshooting"), about: p("aboutAndUpdates") };
  const descriptions: Record<SettingsSection, string> = { general: p("generalSummary"), access: p("accessSummary"), network: p("networkSummary"), runtime: p("runtimeSummary"), diagnostics: p("diagnosticsSummary"), about: p("aboutSummary") };

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
      <div className="settings-layout">
      <div className="settings-navigation" role="tablist" aria-label={t("settings.title")} aria-orientation="vertical">
        {SECTIONS.map(value => {
          const Icon = SECTION_ICONS[value];
          return <button type="button" role="tab" key={value} id={`settings-tab-${value}`} aria-controls={`desktop-settings-${value}`} aria-selected={section === value} tabIndex={section === value ? 0 : -1} onClick={() => selectSection(value)} onKeyDown={event => {
            if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
            event.preventDefault();
            const next = event.key === "Home" ? SECTIONS[0] : event.key === "End" ? SECTIONS[SECTIONS.length - 1] : SECTIONS[(SECTIONS.indexOf(value) + (event.key === "ArrowDown" ? 1 : SECTIONS.length - 1)) % SECTIONS.length];
            selectSection(next); document.getElementById(`settings-tab-${next}`)?.focus();
          }}><Icon size={18} aria-hidden="true" /><span>{labels[value]}</span></button>;
        })}
      </div>
      <div className="settings-content">
      <p className="settings-description">{descriptions[section]}</p>
      {/* Keep visited views mounted so switching categories preserves drafts and
          in-flight operations. Plain containers keep controls reachable in WebKit. */}
      <div id="desktop-settings-general" hidden={section !== "general"}>
      <section className="settings-section" aria-labelledby="settings-general-title">
        <h2 id="settings-general-title">{p("general")}</h2>
        <div className="setting-row"><label htmlFor="desktop-settings-locale">{t("locale.label")}</label><select id="desktop-settings-locale" value={locale} onChange={event => setLocale(event.target.value as typeof locale)} data-webcodex-control="locale">{LANGUAGES.map(language => <option key={language.value} value={language.value}>{language.label}</option>)}</select></div>
        <div className="setting-row"><label htmlFor="desktop-settings-appearance">{t("appearance.label")}</label><select id="desktop-settings-appearance" value={appearance} onChange={event => setAppearance(event.target.value as typeof appearance)} data-webcodex-control="appearance">{APPEARANCES.map(value => <option key={value} value={value}>{t(`appearance.${value}`)}</option>)}</select></div>
        <div className="setting-row"><span>{t("accent.label")}</span><AccentPicker color={accent} onChange={setAccent} /></div>
        <div className="setting-row"><label htmlFor="desktop-launch-at-login">{t("settings.launchAtLogin")}</label><input id="desktop-launch-at-login" type="checkbox" checked={launchAtLogin ?? false} onChange={event => void updateLaunchAtLogin(event.target.checked)} disabled={launchAtLogin === null || savingLaunchAtLogin} data-webcodex-control="launch-at-login" /></div>
        {launchAtLoginError && <SettingsError error={launchAtLoginError} />}
        <div className="setting-row"><span>{p("background")}</span><span className="setting-value">{p("keepRunning")}</span></div>
      </section>
      </div>
      <div id="desktop-settings-access" hidden={section !== "access"}>
      <RunnerFileAccess settings={runnerSettings} disabled={operationBusy} onState={onState} onSettings={setRunnerSettings} />
      {visited.includes("access") && <ComputerPermissions />}
      </div>
      <div id="desktop-settings-diagnostics" hidden={section !== "diagnostics"}>
        {visited.includes("diagnostics") && <DiagnosticsPanel state={state} onState={onState} onActivity={onActivity} onRuntime={() => selectSection("runtime")} onConnection={onConnection} />}
      </div>
      <div id="desktop-settings-network" hidden={section !== "network"} className="settings-network">
        <h2>{t("settings.tunnelProxy")}</h2>
        <div className="field-group"><label htmlFor="desktop-tunnel-proxy-mode">{t("settings.tunnelProxy")}</label><select id="desktop-tunnel-proxy-mode" value={proxyMode} onChange={event => setProxyMode(event.target.value as TunnelProxyMode)} disabled={savingProxy || operationBusy} data-webcodex-control="tunnel-proxy-mode"><option value="auto">{t("settings.tunnelProxyAuto")}</option><option value="direct">{t("settings.tunnelProxyDirect")}</option><option value="custom">{t("settings.tunnelProxyCustom")}</option></select></div>
        {proxyMode === "custom" && <div className="field-group"><label htmlFor="desktop-tunnel-proxy-url">{t("settings.tunnelProxyCustomUrl")}</label><input id="desktop-tunnel-proxy-url" value={customProxy} onChange={event => setCustomProxy(event.target.value)} placeholder="http://127.0.0.1:7890" disabled={savingProxy || operationBusy} spellCheck={false} data-webcodex-control="tunnel-proxy-url" /></div>}
        <dl className="detail-list" data-webcodex-tunnel-routing>
          <div><dt>{p("configuredMode")}</dt><dd>{configuredProxyMode}</dd></div>
          <div><dt>{p("detectedProxy")}</dt><dd>{detectedProxy}</dd></div>
          <div><dt>{p("effectiveConnectionPath")}</dt><dd>{effectiveProxyPath}</dd></div>
          <div><dt>{p("tunnelStatus")}</dt><dd>{state.readiness.exposure}</dd></div>
        </dl>
        <button type="button" className="secondary-button" onClick={() => void saveProxy()} disabled={savingProxy || operationBusy || (proxyMode === "custom" && !customProxy.trim())} data-webcodex-action="save-tunnel-proxy">{savingProxy ? p("loading") : p("saveApply")}</button>
        {proxyError && <SettingsError error={proxyError} />}
      </div>
      <div id="desktop-settings-runtime" hidden={section !== "runtime"}>
        <section className="settings-section">
        <h2>{p("serviceControls")}</h2><p className="field-help">{p("serviceControlsHelp")}</p>
        {visited.includes("runtime") && <LocalServicesPanel state={state} onState={onState} />}
        <div className="connection-actions">
          {onChangeSetup && <button type="button" className="secondary-button" disabled={operationBusy} onClick={onChangeSetup}>{p("changeServer")}</button>}
          {!state.persistent_environment && onStopRuntime && state.topology?.server.kind === "local" && state.readiness.runtime_ready && <button type="button" className="secondary-button" disabled={operationBusy} onClick={onStopRuntime}>{p("stop")} WebCodex</button>}
        </div>
        {!state.persistent_environment && runnerSettings && <><p className="field-help">{p("restartRunnerHelp")}</p><button type="button" className="secondary-button" disabled={operationBusy || restartingRunner || !runnerSettings.can_restart} data-webcodex-action="restart-owned-runner" onClick={async () => {
          if (operationBusy || restartingRunner) return;
          setRestartingRunner(true); setRunnerError(null);
          try { onState(await desktopApi.restartOwnedRunner(runnerSettings.target)); }
          catch (value) { setRunnerError(normalizeDesktopError(value)); }
          finally { setRestartingRunner(false); }
        }}>{p("restartRunner")}</button></>}
        {runnerError && <SettingsError error={runnerError} />}
        {runnerSettings && <details className="workspace-technical"><summary>{p("details")}</summary><dl className="detail-list"><div><dt>Runner</dt><dd>{runnerSettings.target.config_path}</dd></div></dl></details>}
        <PowerShellInstallGuidance state={state} onState={onState} />
        </section>
        {visited.includes("runtime") && <RuntimePanel state={state} onState={onState} onActivity={onActivity} onUpdates={() => selectSection("about")} />}
      </div>
      <div id="desktop-settings-about" hidden={section !== "about"}>
        {visited.includes("about") && <AboutPanel state={state} updates={updates} />}
      </div>
      </div>
      </div>
    </section>
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
