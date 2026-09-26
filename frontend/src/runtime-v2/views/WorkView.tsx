import { CircleDot } from "lucide-react";
import { useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { SessionExecution } from "../components/SessionExecution.js";
import { GoalWorkbench, type WorkSurface } from "../components/GoalWorkbench.js";
import { SessionInspector } from "../components/SessionInspector.js";
import { WorkList } from "../components/WorkList.js";
import { WindowWorkbench } from "../components/WindowWorkbench.js";
import { selectedWorkFromDetail, workBucket, type WorkItem } from "../model/work.js";
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
  surface?: WorkSurface;
  onSurfaceChange?: (surface: WorkSurface) => void;
  onOpenAgent?: (agentId: string) => void;
  onOpenWindow?: (windowKey: string) => void;
  onOpenSession: (location: SessionLocation) => void;
  onLocateSession: (sessionId: string) => Promise<boolean>;
  onUnauthorized: () => void;
  requestedWindowKey?: string;
  requestedSessionId?: string;
  onRequestedWindowConsumed?: () => void;
};

export function WorkView({
  client,
  items,
  selected,
  projects,
  language,
  inventoryIncomplete,
  surface = "windows",
  onSurfaceChange = () => {},
  onOpenAgent = () => {},
  onOpenWindow = () => {},
  onOpenSession,
  onLocateSession,
  onUnauthorized,
  requestedWindowKey,
  requestedSessionId,
  onRequestedWindowConsumed,
}: Props) {
  const t = (value: string) => translate(value, language);
  const [search, setSearch] = useState("");
  const [locating, setLocating] = useState(false);
  const session = useSessionWorkspace(client, Boolean(selected && surface === "session"), selected, onUnauthorized);
  const project = selected ? projects.find((row) => row.id === selected.projectId) : undefined;
  const git = useProjectGit(client, Boolean(selected && surface === "session"), selected?.projectId || "");

  if (surface === "goals") {
    return (
      <GoalWorkbench
        client={client}
        language={language}
        projects={projects}
        surface={surface}
        onSurfaceChange={onSurfaceChange}
        onOpenSession={onOpenSession}
        onOpenAgent={onOpenAgent}
        onOpenWindow={onOpenWindow}
        onUnauthorized={onUnauthorized}
      />
    );
  }

  if (surface === "windows") {
    return (
      <WindowWorkbench
        client={client}
        language={language}
        projects={projects}
        surface={surface}
        onSurfaceChange={onSurfaceChange}
        onUnauthorized={onUnauthorized}
        requestedWindowKey={requestedWindowKey}
        requestedSessionId={requestedSessionId}
        onRequestedWindowConsumed={onRequestedWindowConsumed}
      />
    );
  }

  const selectedBase = selected
    ? items.find((item) => item.sessionId === selected.sessionId && item.projectId === selected.projectId)
    : undefined;

  const fallbackItem: WorkItem | null = selected
    ? selectedBase || {
        key: selected.projectId + ":" + selected.sessionId,
        sessionId: selected.sessionId,
        projectId: selected.projectId,
        projectName: selected.projectName,
        runner: selected.runner,
        title: session.detail?.title || selected.sessionId,
        lifecycle: session.detail?.lifecycle || "retained",
        mode: session.detail?.mode || "normal",
        updatedAt: session.detail?.updated_at || 0,
        bucket: session.detail ? workBucket(session.detail) : "recent",
        phase: session.detail?.overview.reported_progress?.text || session.detail?.lifecycle || "Retained",
        runningCall: Boolean(session.detail?.running_call),
        runningJobs: session.detail?.running_jobs || 0,
        attentionCount: session.detail ? (
          session.detail.overview.attention.open_guidance +
          session.detail.overview.attention.open_questions +
          session.detail.overview.attention.open_risks +
          session.detail.overview.attention.open_todos
        ) : 0,
        validation: session.detail?.overview.validation || {
          state: "not_run",
          unresolved_failure_count: 0,
          history_complete: false,
          history_truncated: false,
        },
        reportedProgress: session.detail?.overview.reported_progress,
      }
    : null;

  const selectedItem = fallbackItem ? selectedWorkFromDetail(fallbackItem, session.detail) : null;
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

  return (
    <div className="work-layout">
      <WorkList
        items={items}
        selectedKey={selected ? selected.projectId + ":" + selected.sessionId : ""}
        search={search}
        locating={locating}
        language={language}
        inventoryIncomplete={inventoryIncomplete}
        surface={surface}
        onSurfaceChange={onSurfaceChange}
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
        <SessionExecution item={selectedItem} location={selected} session={session} language={language} onOpenWindow={onOpenWindow} />
      ) : (
        <main className="session-main ui-workbench-surface">
          <div className="empty-work">
            <CircleDot size={22} />
            <h2>{t("Select a work Session")}</h2>
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
