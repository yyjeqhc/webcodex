import {
  BriefcaseBusiness,
  FolderKanban,
  Languages,
  Lock,
  MoonStar,
  Server,
  Sun,
} from "lucide-react";
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
import { locateSession } from "./api/sessions.js";
import { RuntimeV2Client } from "./api/client.js";
import { AuthGate } from "./components/AuthGate.js";
import type { WorkSurface } from "./components/GoalWorkbench.js";
import type { Availability } from "./model/types.js";
import { workItemFromRecent, type WorkBucket } from "./model/work.js";
import { useRuntimeOverview } from "./state/useRuntimeOverview.js";
import type { SessionLocation } from "./state/useSessionWorkspace.js";
import { ProjectsView } from "./views/ProjectsView.js";
import { RuntimeView } from "./views/RuntimeView.js";
import { WorkView } from "./views/WorkView.js";

type PrimaryView = "work" | "projects" | "runtime";
type RuntimeTarget = { mode: "windows"; windowKey: string } | { mode: "agents"; agentId: string };
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
  const [workSurface, setWorkSurface] = useState<WorkSurface>("goals");
  const [runtimeTarget, setRuntimeTarget] = useState<RuntimeTarget | null>(null);
  const [language, setLanguage] = useState<RuntimeLanguage>(loadLanguagePreference);
  const [appearance, setAppearance] = useState<AppearancePreference>(loadAppearancePreference);
  const [notice, setNotice] = useState("");
  const sessionLocator = useRef<AbortController | null>(null);

  if (token) client.setToken(token);
  else client.clearToken();

  useEffect(() => () => sessionLocator.current?.abort(), []);

  const lock = useCallback((message = "") => {
    sessionLocator.current?.abort();
    sessionLocator.current = null;
    clearRememberedRuntimeCredential();
    client.clearToken();
    setToken("");
    setSelected(null);
    setNotice(message);
  }, [client]);

  const handleUnauthorized = useCallback(() => {
    lock(translate("Your access key is no longer valid. Connect again.", language));
  }, [language, lock]);

  const overviewState = useRuntimeOverview(client, Boolean(token), handleUnauthorized);
  const overview = overviewState.data;

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
    try { window.localStorage.setItem(VIEW_KEY, next); } catch { /* Preference remains in memory. */ }
  }, []);

  const openSession = useCallback((location: SessionLocation) => {
    setSelected(location);
    setWorkSurface("sessions");
    setView("work");
  }, [setView]);

  const openWork = useCallback(() => {
    setWorkSurface("goals");
    setView("work");
  }, [setView]);

  const openAgent = useCallback((agentId: string) => {
    setRuntimeTarget({ mode: "agents", agentId });
    setView("runtime");
  }, [setView]);

  const openWindow = useCallback((windowKey: string) => {
    setRuntimeTarget({ mode: "windows", windowKey });
    setView("runtime");
  }, [setView]);

  useEffect(() => {
    if (selected || !workItems.length) return;
    const first = workItems[0];
    setSelected({
      projectId: first.projectId,
      projectName: first.projectName,
      runner: first.runner,
      sessionId: first.sessionId,
    });
  }, [selected, workItems]);

  useEffect(() => {
    document.documentElement.lang = language;
    document.documentElement.dataset.language = language;
    try { window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language); } catch { /* Preference remains in memory. */ }
  }, [language]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => {
      const resolved = resolvedAppearance(appearance, media.matches);
      document.documentElement.dataset.theme = appearance;
      document.documentElement.dataset.resolvedTheme = resolved;
      document.querySelector('meta[name="theme-color"]')?.setAttribute(
        "content",
        resolved === "light" ? "#f4f5f7" : "#0a0c10",
      );
    };
    apply();
    persistAppearancePreference(appearance);
    media.addEventListener?.("change", apply);
    return () => media.removeEventListener?.("change", apply);
  }, [appearance]);

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
        </div>
        {notice && <div className="auth-notice" role="alert">{notice}</div>}
      </>
    );
  }

  const runningCount = workItems.filter((item) => item.bucket === "running").length;
  const attentionCount = workItems.filter((item) => item.bucket === "attention").length;

  return (
    <div className="app-shell">
      <aside className="app-nav">
        <div className="brand">
          <span className="brand-mark">W</span>
          <span>
            <strong>WebCodex</strong>
            <small><span className={"status-dot " + availabilityDotClass(overviewState.availability)} /> {translate("Runtime workspace", language)}</small>
          </span>
        </div>

        <nav aria-label={translate("Workspace views", language)}>
          <button className={"nav-button " + (view === "work" ? "active" : "")} type="button" onClick={openWork}>
            <span className="nav-icon"><BriefcaseBusiness size={18} /></span>
            <span>{translate("Work", language)}</span>
            <small>{runningCount || attentionCount ? runningCount + attentionCount : ""}</small>
          </button>
          <button className={"nav-button " + (view === "projects" ? "active" : "")} type="button" onClick={() => setView("projects")}>
            <span className="nav-icon"><FolderKanban size={18} /></span>
            <span>{translate("Projects", language)}</span>
            <small>{overview?.visible_projects || ""}</small>
          </button>
          <button className={"nav-button " + (view === "runtime" ? "active" : "")} type="button" onClick={() => setView("runtime")}>
            <span className="nav-icon"><Server size={18} /></span>
            <span>{translate("Runtime", language)}</span>
            <small>{overview?.active_jobs || ""}</small>
          </button>
        </nav>

        <div className="nav-spacer" />

        <div className="nav-utilities">
          <button type="button" onClick={() => setLanguage((current) => current === "en" ? "zh-CN" : "en")}>
            <Languages size={16} /><span>{language === "en" ? "中文" : "English"}</span>
          </button>
          <button type="button" onClick={cycleAppearance}>
            {appearance === "dark" ? <MoonStar size={16} /> : <Sun size={16} />}
            <span>{translate("Appearance", language)} · {translate(appearance === "system" ? "System" : appearance === "light" ? "Light" : "Dark", language)}</span>
          </button>
          <button type="button" onClick={() => lock()}>
            <Lock size={16} /><span>{translate("Lock", language)}</span>
          </button>
        </div>

        <div className="profile">
          <span className="profile-avatar">R</span>
          <span><strong>{translate("Current Runtime", language)}</strong><small>{overview?.service || "WebCodex Server"}</small></span>
        </div>
      </aside>

      <section className="app-content">
        {notice && <div className="global-notice" role="status">{notice}<button type="button" onClick={() => setNotice("")}>×</button></div>}
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
            onUnauthorized={handleUnauthorized}
          />
        )}
        {view === "projects" && (
          <ProjectsView
            client={client}
            language={language}
            runners={overview?.runners || []}
            onOpenSession={openSession}
            onUnauthorized={handleUnauthorized}
          />
        )}
        {view === "runtime" && (
          <RuntimeView
            client={client}
            language={language}
            overview={overview}
            overviewAvailability={overviewState.availability}
            projects={overview?.projects || []}
            onOpenSession={openSession}
            onUnauthorized={handleUnauthorized}
            target={runtimeTarget}
            onTargetConsumed={() => setRuntimeTarget(null)}
          />
        )}
      </section>

      <nav className="mobile-primary-nav" aria-label={translate("Workspace views", language)}>
        <button className={view === "work" ? "active" : ""} type="button" onClick={openWork}><BriefcaseBusiness size={18} /><span>{translate("Work", language)}</span></button>
        <button className={view === "projects" ? "active" : ""} type="button" onClick={() => setView("projects")}><FolderKanban size={18} /><span>{translate("Projects", language)}</span></button>
        <button className={view === "runtime" ? "active" : ""} type="button" onClick={() => setView("runtime")}><Server size={18} /><span>{translate("Runtime", language)}</span></button>
      </nav>
    </div>
  );
}
