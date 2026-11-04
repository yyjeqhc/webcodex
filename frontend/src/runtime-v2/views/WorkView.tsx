import { CircleDot } from "lucide-react";
import { useState, type ReactNode } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { SessionExecution } from "../components/SessionExecution.js";
import { GoalWorkbench, WorkSurfaceSwitch, type WorkSurface } from "../components/GoalWorkbench.js";
import { SessionInspector } from "../components/SessionInspector.js";
import { WorkList } from "../components/WorkList.js";
import { WindowWorkbench } from "../components/WindowWorkbench.js";
import { selectedSessionWork, type WorkItem } from "../model/work.js";
import type { ProjectRow } from "../model/types.js";
import { useProjectGit } from "../state/useProjectGit.js";
import { useSessionWorkspace, type SessionLocation } from "../state/useSessionWorkspace.js";

type Props = {
  client: RuntimeV2Client;
  items: WorkItem[];
  selected: SessionLocation | null;
  projects: ProjectRow[];
  language: RuntimeLanguage;
  inventoryIncomplete: boolean;
  hydratingSessions?: boolean;
  surface?: WorkSurface;
  onSurfaceChange?: (surface: WorkSurface) => void;
  onOpenAgent?: (agentId: string) => void;
  onOpenWindow?: (windowKey: string) => void;
  onOpenSession: (location: SessionLocation) => void;
  onBackToWindows?: () => void;
  onLocateSession: (sessionId: string) => Promise<boolean>;
  onUnauthorized: () => void;
  requestedWindowKey?: string;
  requestedSessionId?: string;
  onRequestedWindowConsumed?: () => void;
  onMobileDetailChange?: (open: boolean) => void;
};

export function WorkView({
  client,
  items,
  selected,
  projects,
  language,
  inventoryIncomplete,
  hydratingSessions = false,
  surface = "windows",
  onSurfaceChange = () => {},
  onOpenAgent = () => {},
  onOpenWindow = () => {},
  onOpenSession,
  onBackToWindows = () => onSurfaceChange("windows"),
  onLocateSession,
  onUnauthorized,
  requestedWindowKey,
  requestedSessionId,
  onRequestedWindowConsumed,
  onMobileDetailChange,
}: Props) {
  const t = (value: string) => translate(value, language);
  const [search, setSearch] = useState("");
  const [locating, setLocating] = useState(false);
  const session = useSessionWorkspace(client, Boolean(selected && surface === "session"), selected, onUnauthorized);
  const project = selected ? projects.find((row) => row.id === selected.projectId) : undefined;
  const git = useProjectGit(client, Boolean(selected && surface === "session"), selected?.projectId || "");

  // Keep one selector mounted when the underlying workbench changes.
  const withNavigation = (body: ReactNode) => <div className={"work-view-shell" + (surface === "windows" ? " window-board-shell" : "")}>
    <div className="work-view-navigation"><WorkSurfaceSwitch surface={surface} onSurfaceChange={onSurfaceChange} language={language} /></div>
    <div className="work-view-body">{body}</div>
  </div>;

  if (surface === "goals") {
    return withNavigation(
      <GoalWorkbench
        client={client}
        language={language}
        projects={projects}
        onOpenSession={onOpenSession}
        onOpenAgent={onOpenAgent}
        onOpenWindow={onOpenWindow}
        onUnauthorized={onUnauthorized}
      />
    );
  }

  if (surface === "windows") {
    return withNavigation(
      <WindowWorkbench
        client={client}
        language={language}
        projects={projects}
        onUnauthorized={onUnauthorized}
        requestedWindowKey={requestedWindowKey}
        requestedSessionId={requestedSessionId}
        onRequestedWindowConsumed={onRequestedWindowConsumed}
        onMobileDetailChange={onMobileDetailChange}
      />
    );
  }

  const selectedBase = selected
    ? items.find((item) => item.sessionId === selected.sessionId && item.projectId === selected.projectId)
    : undefined;

  const selectedItem = selected ? selectedSessionWork(selected, session.detail, selectedBase) : null;
  const sessionDenied = Boolean(selected && session.detailAvailability === "denied");

  const open = (item: WorkItem) => onOpenSession({
    projectId: item.projectId,
    projectName: item.projectName,
    runner: item.runner,
    sessionId: item.sessionId,
  });

  const locateExact = async () => {
    const value = search.trim();
    if (!/^wc_sess_(?:[A-Za-z0-9_-]{16}|[0-9a-f]{32})$/.test(value)) return;
    setLocating(true);
    try {
      await onLocateSession(value);
    } finally {
      setLocating(false);
    }
  };

  return withNavigation(
    <div className="work-layout">
      <WorkList
        items={items}
        selectedKey={selected ? selected.projectId + ":" + selected.sessionId : ""}
        search={search}
        locating={locating}
        language={language}
        inventoryIncomplete={inventoryIncomplete}
        onSearch={setSearch}
        onLocateExact={() => void locateExact()}
        onSelect={open}
      />

      {sessionDenied ? (
        <main className="session-main ui-workbench-surface">
          <div className="empty-work">
            <CircleDot size={22} />
            <h2>{t("Session unavailable")}</h2>
            <p>{t("This Session is no longer visible to the current credential.")}</p>
          </div>
        </main>
      ) : selectedItem && selected ? (
        <SessionExecution item={selectedItem} location={selected} session={session} language={language} onOpenWindow={onOpenWindow} onBackToWindows={onBackToWindows} />
      ) : (
        <main className="session-main ui-workbench-surface">
          <div className="empty-work">
            <CircleDot size={22} />
            <h2>{t(hydratingSessions ? "Loading work Sessions…" : "Select a work Session")}</h2>
            <p>{t("Running work and attention requests appear first. Raw evidence stays one level deeper.")}</p>
          </div>
        </main>
      )}

      {!sessionDenied && selectedItem && selected && (
        <SessionInspector
          item={selectedItem}
          location={selected}
          detail={session.detail}
          detailAvailability={session.detailAvailability}
          project={project}
          branch={git?.branch}
          language={language}
        />
      )}
    </div>
  );
}
