import {
  BriefcaseBusiness,
  FolderKanban,
  Languages,
  Lock,
  MoonStar,
  PanelLeftClose,
  PanelLeftOpen,
  Settings2,
  Server,
  Sun,
  X,
} from "lucide-react";
import { motion } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  LANGUAGE_STORAGE_KEY,
  loadLanguagePreference,
  translate,
  type RuntimeLanguage,
} from "../runtime_i18n.js";
import {
  clearRememberedRuntimeCredential,
  loadAppearancePreference,
  loadRememberedRuntimeCredential,
  persistAppearancePreference,
  persistRuntimeCredentialForTab,
  resolvedAppearance,
  type AppearancePreference,
} from "../runtime_storage.js";
import {
  ACCENT_CHANGE_EVENT,
  ACCENT_STORAGE_KEY,
  applyAccentPreference,
  loadAccentPreference,
  normalizeAccent,
  persistAccentPreference,
} from "../ui/accent.js";
import { locateSession } from "./api/sessions.js";
import { projectFamilyId } from "../ui/projectPresentation.js";
import { RuntimeV2Client } from "./api/client.js";
import { SessionWindowNavigation } from "./components/SessionWindowNavigation.js";
import { AuthGate } from "./components/AuthGate.js";
import { BrandMark } from "./components/ui/BrandMark.js";
import { AccentPicker } from "./components/ui/AccentPicker.js";
import { IconButton } from "./components/ui/IconButton.js";
import type { WorkSurface } from "./components/GoalWorkbench.js";
import type { Availability } from "./model/types.js";
import { workItemFromRecent, type WorkBucket } from "./model/work.js";
import { useRuntimeOverview } from "./state/useRuntimeOverview.js";
import type { SessionLocation } from "./state/useSessionWorkspace.js";
import { ProjectsView } from "./views/ProjectsView.js";
import { RuntimeView } from "./views/RuntimeView.js";
import { WorkView } from "./views/WorkView.js";

type PrimaryView = "work" | "projects" | "runtime";
type RuntimeTarget = { mode: "agents"; agentId: string };
const VIEW_KEY = "webcodex.runtime.v2.view.v1";
const BUCKET_PRIORITY: Record<WorkBucket, number> = { running: 0, attention: 1, active: 2, recent: 3 };

function availabilityDotClass(availability: Availability): string {
  if (availability === "available") return "good";
  if (availability === "stale" || availability === "denied" || availability === "error") return "warn";
  return "";
}

function initialView(): PrimaryView {
  try {
    const value = window.localStorage.getItem(VIEW_KEY);
    if (value === "projects" || value === "runtime" || value === "work") return value;
  } catch {
    // Fall back to Work.
  }
  return "work";
}

function initialToken(): string {
  return loadRememberedRuntimeCredential();
}

export function App() {
  const client = useMemo(() => new RuntimeV2Client(), []);
  const [token, setToken] = useState(initialToken);
  const [view, setViewState] = useState<PrimaryView>(initialView);
  const [selected, setSelected] = useState<SessionLocation | null>(null);
  const [workSurface, setWorkSurface] = useState<WorkSurface>("windows");
  const [workWindowTarget, setWorkWindowTarget] = useState<{ windowKey: string; sessionId: string } | null>(null);
  const [sessionNavigation, setSessionNavigation] = useState<SessionLocation | null>(null);
  const closeSessionNavigation = useCallback(() => setSessionNavigation(null), []);
  const [runtimeTarget, setRuntimeTarget] = useState<RuntimeTarget | null>(null);
  const [language, setLanguage] = useState<RuntimeLanguage>(loadLanguagePreference);
  const [appearance, setAppearance] = useState<AppearancePreference>(loadAppearancePreference);
  const [accent, setAccent] = useState(loadAccentPreference);
  const [notice, setNotice] = useState("");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [mobileSettingsOpen, setMobileSettingsOpen] = useState(false);
  const sessionLocator = useRef<AbortController | null>(null);

  if (token) client.setToken(token);
  else client.clearToken();

  useEffect(() => () => sessionLocator.current?.abort(), []);
  useEffect(() => {
    if (!mobileSettingsOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMobileSettingsOpen(false);
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [mobileSettingsOpen]);

  const lock = useCallback((message = "") => {
    sessionLocator.current?.abort();
    sessionLocator.current = null;
    clearRememberedRuntimeCredential();
    client.clearToken();
    setToken("");
    setSelected(null);
    setSessionNavigation(null);
    setWorkWindowTarget(null);
    setNotice(message);
  }, [client]);

  const handleUnauthorized = useCallback(() => {
    lock(translate("Your access key is no longer valid. Connect again.", language));
  }, [language, lock]);

  const overviewState = useRuntimeOverview(client, Boolean(token), handleUnauthorized);
  const overview = overviewState.data;
  const visibleProjectFamilyCount = useMemo(
    () => new Set((overview?.projects || []).map(projectFamilyId)).size,
    [overview?.projects],
  );

  const workItems = useMemo(() => {
    const rows = overview?.recent_sessions.sessions || [];
    return rows
      .map(workItemFromRecent)
      .sort((left, right) => {
        const bucket = BUCKET_PRIORITY[left.bucket] - BUCKET_PRIORITY[right.bucket];
        return bucket || right.updatedAt - left.updatedAt;
      });
  }, [overview]);

  const setView = useCallback((next: PrimaryView) => {
    setViewState(next);
    setMobileSettingsOpen(false);
    try { window.localStorage.setItem(VIEW_KEY, next); } catch { /* Preference remains in memory. */ }
  }, []);

  const openSession = useCallback((location: SessionLocation) => setSessionNavigation(location), []);

  const openSessionRecord = useCallback((location: SessionLocation) => {
    setSessionNavigation(null);
    setSelected(location);
    setWorkSurface("session");
    setView("work");
  }, [setView]);

  const openWork = useCallback(() => {
    setWorkSurface("windows");
    setView("work");
  }, [setView]);

  const openAgent = useCallback((agentId: string) => {
    setRuntimeTarget({ mode: "agents", agentId });
    setView("runtime");
  }, [setView]);

  const openWindow = useCallback((windowKey: string, sessionId = "") => {
    setSessionNavigation(null);
    setWorkWindowTarget({ windowKey, sessionId });
    setWorkSurface("windows");
    setView("work");
  }, [setView]);

  useEffect(() => {
    document.documentElement.lang = language;
    document.documentElement.dataset.language = language;
    try { window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language); } catch { /* Preference remains in memory. */ }
  }, [language]);

  useEffect(() => {
    const sync = (event: Event) => {
      const next = event instanceof StorageEvent ? event.key === ACCENT_STORAGE_KEY ? normalizeAccent(event.newValue) : null : normalizeAccent((event as CustomEvent<string>).detail);
      if (next) setAccent(next);
    };
    window.addEventListener(ACCENT_CHANGE_EVENT, sync);
    window.addEventListener("storage", sync);
    return () => { window.removeEventListener(ACCENT_CHANGE_EVENT, sync); window.removeEventListener("storage", sync); };
  }, []);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => {
      const resolved = resolvedAppearance(appearance, media.matches);
      document.documentElement.dataset.theme = appearance;
      document.documentElement.dataset.resolvedTheme = resolved;
      applyAccentPreference(accent, resolved);
      document.querySelector('meta[name="theme-color"]')?.setAttribute(
        "content",
        resolved === "light" ? "#f5f5f5" : "#0b0b0c",
      );
    };
    apply();
    persistAppearancePreference(appearance);
    media.addEventListener?.("change", apply);
    return () => media.removeEventListener?.("change", apply);
  }, [appearance, accent]);

  const connect = (nextToken: string, remember: boolean) => {
    sessionLocator.current?.abort();
    sessionLocator.current = null;
    setNotice("");
    client.setToken(nextToken);
    persistRuntimeCredentialForTab(nextToken, remember);
    setToken(nextToken);
  };

  const locateExactSession = useCallback(async (sessionId: string): Promise<boolean> => {
    sessionLocator.current?.abort();
    const controller = new AbortController();
    sessionLocator.current = controller;
    const response = await locateSession(client, sessionId, controller.signal);
    if (sessionLocator.current !== controller || controller.signal.aborted || !response) return false;
    sessionLocator.current = null;
    if (response.status === 401) {
      lock(translate("Your access key is no longer valid. Connect again.", language));
      return false;
    }
    if (!response.ok || !response.data) {
      setNotice(translate("Exact Session lookup failed", language));
      return false;
    }
    openSession({
      projectId: response.data.project_id,
      projectName: response.data.project_name || response.data.project_id,
      runner: response.data.client_id,
      sessionId: response.data.session_id,
    });
    setNotice("");
    return true;
  }, [client, language, lock, openSession]);

  const cycleAppearance = () => {
    setAppearance((current) => current === "system" ? "light" : current === "light" ? "dark" : "system");
  };
  const changeAccent = (color: string) => {
    setAccent(color);
    persistAccentPreference(color);
  };

  const preferenceControls = () => (
    <>
      <button type="button" title={translate("Language", language)} aria-label={translate("Language", language)} onClick={() => setLanguage((current) => current === "en" ? "zh-CN" : "en")}>
        <Languages size={17} /><span>{language === "en" ? "中文" : "English"}</span>
      </button>
      <button type="button" title={translate("Appearance", language)} aria-label={translate("Appearance", language)} onClick={cycleAppearance}>
        {appearance === "dark" ? <MoonStar size={17} /> : <Sun size={17} />}
        <span>{translate("Appearance", language)} · {translate(appearance === "system" ? "System" : appearance === "light" ? "Light" : "Dark", language)}</span>
      </button>
      <AccentPicker color={accent} onChange={changeAccent} language={language} />
      <button type="button" title={translate("Lock", language)} aria-label={translate("Lock", language)} onClick={() => lock()}>
        <Lock size={17} /><span>{translate("Lock", language)}</span>
      </button>
    </>
  );

  if (!token) {
    return (
      <>
        <AuthGate language={language} onConnect={connect} />
        <div className="auth-preferences-v2">
          <button type="button" onClick={() => setLanguage((current) => current === "en" ? "zh-CN" : "en")} aria-label={translate("Language", language)}>
            <Languages size={16} /> {language === "en" ? "中" : "EN"}
          </button>
          <button type="button" onClick={cycleAppearance} aria-label={translate("Appearance", language)}>
            {appearance === "dark" ? <MoonStar size={16} /> : <Sun size={16} />}
          </button>
          <AccentPicker color={accent} onChange={changeAccent} language={language} />
        </div>
        {notice && <div className="auth-notice" role="alert">{notice}</div>}
      </>
    );
  }

  return (
    <div className={"app-shell ui-canvas" + (sidebarCollapsed ? " sidebar-collapsed" : "")}>
      <aside className="app-nav ui-glass">
        <div className="nav-brand-row">
          <div className="brand">
            <BrandMark />
            <span>
              <strong>WebCodex</strong>
              <small><span className={"status-dot " + availabilityDotClass(overviewState.availability)} /> {translate("Runtime workspace", language)}</small>
            </span>
          </div>
          <IconButton
            className="sidebar-collapse"
            label={translate(sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar", language)}
            onClick={() => setSidebarCollapsed((value) => !value)}
          >
            {sidebarCollapsed ? <PanelLeftOpen size={17} /> : <PanelLeftClose size={17} />}
          </IconButton>
        </div>

        <nav aria-label={translate("Workspace views", language)}>
          <button className={"nav-button " + (view === "work" ? "active" : "")} type="button" onClick={openWork} aria-current={view === "work" ? "page" : undefined} title={translate("Work", language)}>
            {view === "work" && <motion.span className="ui-selection-rail" layoutId="runtime-nav-rail" aria-hidden="true" />}
            <span className="nav-icon"><BriefcaseBusiness size={18} /></span>
            <span className="nav-label">{translate("Work", language)}</span>
            <small>{overview?.active_windows || ""}</small>
          </button>
          <button className={"nav-button " + (view === "projects" ? "active" : "")} type="button" onClick={() => setView("projects")} aria-current={view === "projects" ? "page" : undefined} title={translate("Projects", language)}>
            {view === "projects" && <motion.span className="ui-selection-rail" layoutId="runtime-nav-rail" aria-hidden="true" />}
            <span className="nav-icon"><FolderKanban size={18} /></span>
            <span className="nav-label">{translate("Projects", language)}</span>
            <small>{visibleProjectFamilyCount || ""}</small>
          </button>
          <button className={"nav-button " + (view === "runtime" ? "active" : "")} type="button" onClick={() => setView("runtime")} aria-current={view === "runtime" ? "page" : undefined} title={translate("Runtime", language)}>
            {view === "runtime" && <motion.span className="ui-selection-rail" layoutId="runtime-nav-rail" aria-hidden="true" />}
            <span className="nav-icon"><Server size={18} /></span>
            <span className="nav-label">{translate("Runtime", language)}</span>
            <small>{overview?.active_jobs || ""}</small>
          </button>
        </nav>

        <div className="nav-spacer" />

        <div className="nav-utilities">{preferenceControls()}</div>

        <div className="profile">
          <span className="profile-avatar" aria-hidden="true"><Server size={16} /></span>
          <span><strong>{translate("Current Runtime", language)}</strong><small>{overview?.service || "WebCodex Server"}</small></span>
        </div>
      </aside>

      <div className="mobile-app-bar ui-glass">
        <div className="mobile-brand"><BrandMark /><strong>WebCodex</strong><span className={"status-dot " + availabilityDotClass(overviewState.availability)} /></div>
        <IconButton label={translate(mobileSettingsOpen ? "Close" : "Preferences", language)} aria-expanded={mobileSettingsOpen} aria-controls={mobileSettingsOpen ? "mobile-preferences" : undefined} onClick={() => setMobileSettingsOpen((value) => !value)}>{mobileSettingsOpen ? <X size={18} /> : <Settings2 size={18} />}</IconButton>
      </div>
      {mobileSettingsOpen && <div id="mobile-preferences" className="mobile-preferences ui-glass" role="group" aria-label={translate("Preferences", language)}>{preferenceControls()}</div>}

      <section className="app-content">
        {notice && <div className="global-notice" role="status">{notice}<IconButton label={translate("Close", language)} onClick={() => setNotice("")}><X size={16} /></IconButton></div>}
        {view === "work" && (
          <WorkView
            client={client}
            items={workItems}
            selected={selected}
            projects={overview?.projects || []}
            language={language}
            inventoryIncomplete={Boolean(overview?.recent_sessions.truncated || overview?.recent_sessions.scan_truncated)}
            surface={workSurface}
            onSurfaceChange={setWorkSurface}
            onOpenAgent={openAgent}
            onOpenWindow={openWindow}
            onOpenSession={openSession}
            onLocateSession={locateExactSession}
            requestedWindowKey={workWindowTarget?.windowKey}
            requestedSessionId={workWindowTarget?.sessionId}
            onRequestedWindowConsumed={() => setWorkWindowTarget(null)}
            onUnauthorized={handleUnauthorized}
          />
        )}
        {view === "projects" && (
          <ProjectsView
            client={client}
            language={language}
            runners={overview?.runners || []}
            onOpenSession={openSession}
            onOpenWindow={openWindow}
            onUnauthorized={handleUnauthorized}
          />
        )}
        {view === "runtime" && (
          <RuntimeView
            client={client}
            language={language}
            overview={overview}
            overviewAvailability={overviewState.availability}
            onOpenWork={() => { setWorkSurface("windows"); setView("work"); }}
            onUnauthorized={handleUnauthorized}
            target={runtimeTarget}
            onTargetConsumed={() => setRuntimeTarget(null)}
          />
        )}
      </section>

      {sessionNavigation && <SessionWindowNavigation
        key={sessionNavigation.projectId + ":" + sessionNavigation.sessionId}
        client={client} location={sessionNavigation} language={language}
        onClose={closeSessionNavigation} onOpenWindow={openWindow}
        onOpenRecord={openSessionRecord} onUnauthorized={handleUnauthorized}
      />}

      <nav className="mobile-primary-nav ui-glass" aria-label={translate("Workspace views", language)}>
        <button className={view === "work" ? "active" : ""} type="button" onClick={openWork} aria-current={view === "work" ? "page" : undefined}><BriefcaseBusiness size={18} /><span>{translate("Work", language)}</span></button>
        <button className={view === "projects" ? "active" : ""} type="button" onClick={() => setView("projects")} aria-current={view === "projects" ? "page" : undefined}><FolderKanban size={18} /><span>{translate("Projects", language)}</span></button>
        <button className={view === "runtime" ? "active" : ""} type="button" onClick={() => setView("runtime")} aria-current={view === "runtime" ? "page" : undefined}><Server size={18} /><span>{translate("Runtime", language)}</span></button>
      </nav>
    </div>
  );
}
