import { useCallback, useEffect, useState } from "react";
import { desktopApi } from "./lib/desktop-api";
import type { ActivityEntry, DesktopState } from "./models/topology";
import { FirstRun } from "./features/onboarding/FirstRun";
import { Dashboard } from "./features/dashboard/Dashboard";
import { ProjectsPanel } from "./features/projects/ProjectsPanel";
import { ConnectionPanel } from "./features/connection/ConnectionPanel";
import { ActivityPanel } from "./features/activity/ActivityPanel";
import { SettingsPanel } from "./features/settings/SettingsPanel";

type Navigation = "home" | "projects" | "connection" | "activity" | "settings";

export default function App() {
  const [state, setState] = useState<DesktopState | null>(null);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);
  const [navigation, setNavigation] = useState<Navigation>("home");
  const [refreshing, setRefreshing] = useState(false);

  const load = useCallback(async () => {
    const initial = await desktopApi.getState();
    const next = initial.topology ? await desktopApi.refresh().catch(() => initial) : initial;
    setState(next);
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (navigation === "activity") {
      void desktopApi.activity().then(setActivity);
    }
  }, [navigation, state?.activity_sequence]);

  const refresh = async () => {
    setRefreshing(true);
    try {
      setState(await desktopApi.refresh());
    } finally {
      setRefreshing(false);
    }
  };

  if (!state) {
    return <div className="splash"><div className="brand-mark">W</div><span>Loading WebCodex…</span></div>;
  }

  const needsSetup =
    !state.topology ||
    (!state.readiness.runtime_ready && state.topology.experience === "full");

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand"><div className="brand-mark">W</div><div><strong>WebCodex</strong><span>Desktop</span></div></div>
        <nav>
          {(["home", "projects", "connection", "activity", "settings"] as Navigation[]).map((item) => (
            <button
              key={item}
              className={navigation === item ? "active" : ""}
              onClick={() => setNavigation(item)}
            >
              <span className={`nav-icon nav-${item}`} />
              {item[0].toUpperCase() + item.slice(1)}
            </button>
          ))}
        </nav>
        <div className="sidebar-status">
          <i className={`status-dot ${state.readiness.runtime_ready ? "ready" : "unknown"}`} />
          <div><strong>{state.readiness.runtime_ready ? "Runtime ready" : "Needs setup"}</strong><span>{state.readiness.ready_for_chatgpt ? "ChatGPT ready" : "Connection not complete"}</span></div>
        </div>
      </aside>

      <main className="main-content">
        {navigation === "home" && (needsSetup ? (
          <FirstRun state={state} onState={setState} />
        ) : (
          <Dashboard
            state={state}
            refreshing={refreshing}
            onRefresh={() => void refresh()}
            onStopQuickShare={() => void desktopApi.stopQuickShare().then(setState)}
            onStopRuntime={() => void desktopApi.stopLocalRuntime().then(setState)}
          />
        ))}
        {navigation === "projects" && <ProjectsPanel state={state} />}
        {navigation === "connection" && <ConnectionPanel state={state} />}
        {navigation === "activity" && <ActivityPanel activity={activity} />}
        {navigation === "settings" && <SettingsPanel state={state} />}
      </main>
    </div>
  );
}

