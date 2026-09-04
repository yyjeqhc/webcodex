import { useCallback, useEffect, useState } from "react";
import { desktopApi } from "./lib/desktop-api";
import type { ActivityEntry, DesktopError, DesktopState } from "./models/topology";
import { FirstRun } from "./features/onboarding/FirstRun";
import { Dashboard } from "./features/dashboard/Dashboard";
import { ProjectsPanel } from "./features/projects/ProjectsPanel";
import { ConnectionPanel } from "./features/connection/ConnectionPanel";
import { ActivityPanel } from "./features/activity/ActivityPanel";
import { SettingsPanel } from "./features/settings/SettingsPanel";
import { useLocale } from "./i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "./i18n/presentation";

type Navigation = "home" | "projects" | "connection" | "activity" | "settings";

const REGULAR_TUNNEL_OBSERVATION_INTERVAL_MS = 1_500;

export default function App() {
  const { locale, setLocale, t } = useLocale();
  const [state, setState] = useState<DesktopState | null>(null);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);
  const [navigation, setNavigation] = useState<Navigation>("home");
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const hasRegularTunnel = Boolean(state?.regular_tunnel);

  const load = useCallback(async () => {
    const initial = await desktopApi.getState();
    const next = initial.topology ? await desktopApi.refresh().catch(() => initial) : initial;
    setState(next);
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!hasRegularTunnel) return;

    let cancelled = false;
    let timeoutId: number | undefined;

    const scheduleObservation = () => {
      timeoutId = window.setTimeout(() => {
        void (async () => {
          try {
            const next = await desktopApi.getState();
            if (!cancelled) {
              setState((current) => (current?.regular_tunnel ? next : current));
            }
          } catch {
            // Ownership observation is best-effort. A transient invoke failure
            // must not create an error storm or a second concurrent observer.
          } finally {
            if (!cancelled) scheduleObservation();
          }
        })();
      }, REGULAR_TUNNEL_OBSERVATION_INTERVAL_MS);
    };

    scheduleObservation();
    return () => {
      cancelled = true;
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
    };
  }, [hasRegularTunnel]);

  useEffect(() => {
    if (navigation === "activity") {
      void desktopApi.activity().then(setActivity);
    }
  }, [navigation, state?.activity_sequence]);

  const runStateOperation = async (operation: () => Promise<DesktopState>) => {
    setError(null);
    try {
      setState(await operation());
    } catch (value) {
      setError(normalizeDesktopError(value));
    }
  };

  const refresh = async () => {
    setRefreshing(true);
    try {
      await runStateOperation(desktopApi.refresh);
    } finally {
      setRefreshing(false);
    }
  };

  if (!state) {
    return <div className="splash" role="status"><div className="brand-mark" aria-hidden="true">W</div><span>{t("app.loading")}</span></div>;
  }

  const needsSetup =
    !state.topology ||
    (!state.readiness.runtime_ready && state.topology.experience === "full");

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand"><div className="brand-mark" aria-hidden="true">W</div><div><strong>WebCodex</strong><span>Desktop</span></div></div>
        <nav aria-label={t("nav.main")}>
          {(["home", "projects", "connection", "activity", "settings"] as Navigation[]).map((item) => (
            <button
              key={item}
              className={navigation === item ? "active" : ""}
              onClick={() => setNavigation(item)}
              aria-current={navigation === item ? "page" : undefined}
              data-webcodex-action={`navigate-${item}`}
            >
              <span className={`nav-icon nav-${item}`} aria-hidden="true" />
              {t(`nav.${item}`)}
            </button>
          ))}
        </nav>
        <div className="sidebar-locale">
          <label htmlFor="desktop-sidebar-locale">{t("locale.label")}</label>
          <select
            id="desktop-sidebar-locale"
            value={locale}
            onChange={(event) => setLocale(event.target.value as typeof locale)}
            data-webcodex-control="locale"
          >
            <option value="zh-CN">{t("locale.zh")}</option>
            <option value="en-US">{t("locale.en")}</option>
          </select>
        </div>
        <div className="sidebar-status">
          <i className={`status-dot ${state.readiness.runtime_ready ? "ready" : "unknown"}`} aria-hidden="true" />
          <div><strong>{state.readiness.runtime_ready ? t("sidebar.runtimeReady") : t("sidebar.needsSetup")}</strong><span>{state.readiness.ready_for_chatgpt ? t("sidebar.chatgptReady") : t("sidebar.connectionIncomplete")}</span></div>
        </div>
      </aside>

      <main className="main-content">
        {error && <AppError error={error} />}
        {navigation === "home" && (needsSetup ? (
          <FirstRun state={state} onState={setState} />
        ) : (
          <Dashboard
            state={state}
            refreshing={refreshing}
            onRefresh={() => void refresh()}
            onStopQuickShare={() => void runStateOperation(desktopApi.stopQuickShare)}
            onStopRuntime={() => void runStateOperation(desktopApi.stopLocalRuntime)}
          />
        ))}
        {navigation === "projects" && <ProjectsPanel state={state} />}
        {navigation === "connection" && <ConnectionPanel state={state} onState={setState} />}
        {navigation === "activity" && <ActivityPanel activity={activity} />}
        {navigation === "settings" && <SettingsPanel state={state} />}
      </main>
    </div>
  );
}

function AppError({ error }: { error: DesktopError }) {
  const { t } = useLocale();
  const presentation = desktopErrorPresentation(error, t);
  return (
    <div className="error-card app-error" role="alert">
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

