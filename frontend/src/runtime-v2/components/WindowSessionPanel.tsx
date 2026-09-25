import { CircleDot } from "lucide-react";
import { Select } from "@mantine/core";
import { useMemo } from "react";
import { projectFamilyName, sourceProjectRuntimeId } from "../../ui/projectPresentation.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { absoluteTime, shortId } from "../model/format.js";
import { groupRecentProgress } from "../model/work.js";
import type { ProjectRow, WindowDetail, WindowLinkedSession } from "../model/types.js";
import { windowSessionCatalog } from "../model/windowSessions.js";
import { useSessionWorkspace, type SessionLocation } from "../state/useSessionWorkspace.js";
import { ProgressCluster } from "./ProgressCluster.js";

type Props = {
  client: RuntimeV2Client;
  detail: WindowDetail;
  projects: ProjectRow[];
  language: RuntimeLanguage;
  selectedSessionId: string;
  onSelectSession: (sessionId: string) => void;
  onUnauthorized: () => void;
};

function projectFor(projects: ProjectRow[], id?: string): ProjectRow | undefined {
  return id ? projects.find((project) => project.id === id) : undefined;
}

function sessionLocation(
  linked: WindowLinkedSession | undefined,
  projects: ProjectRow[],
): SessionLocation | null {
  if (!linked?.project) return null;
  const project = projectFor(projects, linked.project);
  if (!project) return null;
  const source = projectFor(projects, sourceProjectRuntimeId(project));
  return {
    projectId: project.id,
    projectName: projectFamilyName(source || project, projects),
    runner: project.client_id,
    sessionId: linked.workflow_session_id,
  };
}

export function WindowSessionPanel({
  client,
  detail,
  projects,
  language,
  selectedSessionId,
  onSelectSession,
  onUnauthorized,
}: Props) {
  const t = (value: string) => translate(value, language);
  const sessions = useMemo(() => windowSessionCatalog(detail), [detail]);
  const selected = sessions.find((session) => session.workflow_session_id === selectedSessionId) || sessions[0];
  const location = useMemo(
    () => sessionLocation(selected, projects),
    [projects, selected?.project, selected?.workflow_session_id],
  );
  const session = useSessionWorkspace(
    client,
    Boolean(location),
    location,
    onUnauthorized,
    { loadMessages: false },
  );
  const progress = groupRecentProgress(session.detail);

  if (!sessions.length) {
    return (
      <div className="empty-work window-session-empty">
        <CircleDot size={22} />
        <h2>{t("No linked work Sessions")}</h2>
        <p>{t("This Window has not recorded an explicit Workflow Session relation yet.")}</p>
      </div>
    );
  }

  const options = sessions.map((linked, index) => {
    const loaded = session.detail?.session_id === linked.workflow_session_id ? session.detail : null;
    const title = linked.title || loaded?.title || t("Work Session") + " " + (index + 1);
    return {
      value: linked.workflow_session_id,
      label: title + " · " + shortId(linked.workflow_session_id, 16, 7),
    };
  });

  return (
    <section className="window-session-panel" aria-label={t("Work Sessions")}>
      <div className="window-session-toolbar">
        <Select
          aria-label={t("Work Session")}
          className="window-session-select"
          value={selected?.workflow_session_id || null}
          onChange={(value) => value && onSelectSession(value)}
          data={options}
          searchable
          allowDeselect={false}
          comboboxProps={{ withinPortal: true }}
          placeholder={t("Select a work Session")}
          nothingFoundMessage={t("No matching work Sessions")}
        />
        <span>{sessions.length} {t("Work Sessions")}</span>
      </div>

      {detail.sessions_truncated && (
        <div className="inventory-note window-session-bound">{t("Session relations are bounded; older linked Sessions may be omitted.")}</div>
      )}

      {!location ? (
        <div className="empty-inline window-session-content">{t("The selected Session project is not available to this workspace.")}</div>
      ) : (
        <div className="window-session-scroll">
          <div className="window-session-content">
            <div className="window-session-context">
              <code title={location.sessionId}>{location.sessionId}</code>
              <span>{location.projectName}</span>
              <span>{location.runner}</span>
              {selected && <span>{t("Last linked")} {absoluteTime(selected.last_linked_at_ms)}</span>}
            </div>

            <section className="progress-section window-session-activity">
              <div className="progress-heading">
                <span>{t("Session activity")}</span>
                <small>{t("Complete retained evidence for the selected Workflow Session.")}</small>
              </div>

              {session.detailAvailability === "loading" && !session.detail && (
                <div className="empty-inline">{t("Loading Session activity…")}</div>
              )}
              {session.detailAvailability === "denied" && (
                <div className="empty-inline">{t("This Session is no longer visible to the current credential.")}</div>
              )}
              {(session.detailAvailability === "error" || session.detailAvailability === "stale") && !session.detail && (
                <div className="empty-inline">{t("Session activity unavailable")}</div>
              )}

              <div className="timeline-clusters">
                {progress.map((group, index) => (
                  <ProgressCluster
                    key={group.source + "-" + group.intent + "-" + group.latestAt + "-" + index}
                    group={group}
                    language={language}
                  />
                ))}
              </div>

              {session.detail && !progress.length && (
                <div className="empty-inline">{t("No retained activity in this Session.")}</div>
              )}
              {session.detail?.activity_truncated && (
                <div className="inventory-note wide">{t("Session activity history is bounded by the retained ledger.")}</div>
              )}
            </section>
          </div>
        </div>
      )}
    </section>
  );
}
