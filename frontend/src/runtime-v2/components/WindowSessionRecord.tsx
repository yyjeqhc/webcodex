import { ArrowLeft } from "lucide-react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { selectedSessionWork } from "../model/work.js";
import { useSessionWorkspace, type SessionLocation } from "../state/useSessionWorkspace.js";
import { SessionExecution } from "./SessionExecution.js";

type Props = {
  client: RuntimeV2Client;
  location: SessionLocation;
  language: RuntimeLanguage;
  onUnauthorized: () => void;
  onBack: () => void;
  onOpenWindow: (windowKey: string) => void;
};

/** A focused, separately authorized Session read inside its originating Window. */
export function WindowSessionRecord({ client, location, language, onUnauthorized, onBack, onOpenWindow }: Props) {
  const t = (value: string) => translate(value, language);
  const session = useSessionWorkspace(client, true, location, onUnauthorized);
  if (!session.detail) {
    const denied = session.detailAvailability === "denied";
    const failed = session.detailAvailability === "error";
    return <section className="window-session-record-empty" aria-label={t("Session record")}>
      <button className="text-button" type="button" onClick={onBack}><ArrowLeft size={16} />{t("Back to Window")}</button>
      <div className="empty-work" role="status">
        <p>{t(denied ? "Session unavailable" : failed ? "Could not load Session activity." : "Loading work evidence…")}</p>
        {failed && <button className="text-button" type="button" onClick={session.refresh}>{t("Retry")}</button>}
      </div>
    </section>;
  }
  return <SessionExecution embedded item={selectedSessionWork(location, session.detail)} location={location}
    session={session} language={language} onOpenWindow={onOpenWindow} onBackToWindows={onBack} />;
}
