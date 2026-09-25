import { useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { ProjectRow, WindowDetail } from "../model/types.js";
import { shortId } from "../model/format.js";
import { useSessionWorkspace, type SessionLocation } from "../state/useSessionWorkspace.js";
import { SessionCollaboration } from "./SessionCollaboration.js";

export function WindowCollaboration({ client, detail, projects, language, onUnauthorized, onOpenSession }: {
  client: RuntimeV2Client;
  detail: WindowDetail;
  projects: ProjectRow[];
  language: RuntimeLanguage;
  onUnauthorized: () => void;
  onOpenSession: (location: SessionLocation) => void;
}) {
  const t = (value: string) => translate(value, language);
  const [selectedId, setSelectedId] = useState("");
  const linked = detail.linked_sessions.find((row) => row.workflow_session_id === selectedId);
  const project = projects.find((row) => row.id === linked?.project);
  const location = linked && project ? {
    projectId: project.id, projectName: project.name || project.id,
    runner: project.client_id, sessionId: linked.workflow_session_id,
  } : null;
  return <>
    <div className="window-collaboration-heading">
      <h2>{t("Window collaboration")}</h2>
      <p>{t("Choose a linked session to read messages and reply.")}</p>
      <label className="activity-project-filter">
        <span>{t("Session")}</span>
        <select value={location ? selectedId : ""} onChange={(event) => setSelectedId(event.target.value)}>
          <option value="">{t("Choose a session…")}</option>
          {detail.linked_sessions.map((row) => <option key={row.workflow_session_id} value={row.workflow_session_id}
            disabled={!projects.some((project) => project.id === row.project)}>
            {row.title || shortId(row.workflow_session_id)}
          </option>)}
        </select>
      </label>
      {detail.sessions_truncated && <p>{t("Showing recent linked sessions only.")}</p>}
      {!detail.linked_sessions.length && <p>{t("This window has no linked sessions yet. Its activity is available in the activity tab.")}</p>}
      {location && <button className="text-button" onClick={() => onOpenSession(location)}>{t("Open session activity")}</button>}
    </div>
    {location && <LinkedSessionCollaboration key={location.projectId + ":" + location.sessionId} client={client} location={location} language={language} onUnauthorized={onUnauthorized} />}
  </>;
}

function LinkedSessionCollaboration({ client, location, language, onUnauthorized }: {
  client: RuntimeV2Client;
  location: SessionLocation;
  language: RuntimeLanguage;
  onUnauthorized: () => void;
}) {
  const session = useSessionWorkspace(client, true, location, onUnauthorized);
  return <SessionCollaboration location={location} session={session} language={language} />;
}
