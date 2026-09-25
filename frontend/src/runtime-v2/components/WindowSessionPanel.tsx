import { CircleDot, Monitor } from "lucide-react";
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

function sessionTone(index: number): number {
  return index % 8;
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

  return (
    <section className="window-session-panel" aria-label={t("Work Sessions")}>
      <div className="window-session-switcher" role="tablist" aria-label={t("Sessions in this Window")}>
        {sessions.map((linked, index) => {
          const active = linked.workflow_session_id === selected?.workflow_session_id;
          const loaded = session.detail?.session_id === linked.workflow_session_id ? session.detail : null;
          const title = linked.title || loaded?.title || t("Work Session") + " " + (index + 1);
          const lifecycle = linked.lifecycle || loaded?.lifecycle;
          return (
            <button
              key={linked.workflow_session_id}
              type="button"
              role="tab"
              aria-selected={active}
              className={active ? "active" : ""}
              data-session-tone={sessionTone(index)}
              title={linked.workflow_session_id}
              onClick={() => onSelectSession(linked.workflow_session_id)}
            >
              <span className="window-session-color-dot" />
              <span>
                <strong>{title}</strong>
                <small>{shortId(linked.workflow_session_id, 16, 7)}</small>
              </span>
              {lifecycle && <em>{t(lifecycle)}</em>}
            </button>
          );
        })}
      </div>

      {detail.sessions_truncated && (
        <div className="inventory-note">{t("Session relations are bounded; older linked Sessions may be omitted.")}</div>
      )}

      {!location ? (
        <div className="empty-inline">{t("The selected Session project is not available to this workspace.")}</div>
      ) : (
        <div className="window-session-scroll">
          <div className="window-session-measure">
            <div className="window-session-summary">
              <div>
                <span>{t("Work Session")}</span>
                <strong>{selected?.title || session.detail?.title || shortId(location.sessionId)}</strong>
                <code title={location.sessionId}>{location.sessionId}</code>
              </div>
              <div>
                <span>{t("Project")}</span>
                <strong>{location.projectName}</strong>
                <small>{location.runner}</small>
              </div>
              <div>
                <span>{t("Window link")}</span>
                <strong>{t("Recorded in this Window")}</strong>
                <small>
                  {selected ? t("Last linked") + " " + absoluteTime(selected.last_linked_at_ms) : "—"}
                </small>
              </div>
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

            {session.detail?.linked_windows.length ? (
              <section className="window-session-linked-windows">
                <div className="progress-heading">
                  <span>{t("Linked Windows")}</span>
                  <small>{String(session.detail.linked_windows.length)}</small>
                </div>
                <div>
                  {session.detail.linked_windows.map((window) => (
                    <span key={window.client_window_key} title={window.client_window_key}>
                      <Monitor size={13} />
                      {shortId(window.client_window_key)} · {absoluteTime(window.last_seen_at_ms)}
                    </span>
                  ))}
                </div>
              </section>
            ) : null}
          </div>
        </div>
      )}
    </section>
  );
}
