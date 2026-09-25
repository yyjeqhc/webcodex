import { CircleDot } from "lucide-react";
import { useMemo } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { ProjectRow, WindowDetail } from "../model/types.js";
import { windowSessionCatalog } from "../model/windowSessions.js";
import { WindowActivityFeed } from "./WindowActivityFeed.js";

type Props = {
  client: RuntimeV2Client;
  detail: WindowDetail;
  projects: ProjectRow[];
  language: RuntimeLanguage;
  selectedSessionId: string;
  onSelectSession: (sessionId: string) => void;
  onUnauthorized: () => void;
};

export function WindowSessionPanel({
  detail,
  projects,
  language,
  selectedSessionId,
  onSelectSession,
}: Props) {
  const t = (value: string) => translate(value, language);
  const sessions = useMemo(() => windowSessionCatalog(detail), [detail]);

  if (!sessions.length) {
    return (
      <div className="empty-work window-session-empty">
        <CircleDot size={22} />
        <h2>{t("No linked work Sessions")}</h2>
      </div>
    );
  }

  return (
    <section className="window-session-panel" aria-label={t("Work Sessions")}>
      <WindowActivityFeed
        detail={detail}
        projects={projects}
        language={language}
        selectedSessionId={selectedSessionId || sessions[0].workflow_session_id}
        onSelectSession={onSelectSession}
      />
    </section>
  );
}
