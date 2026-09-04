import type { ActivityEntry } from "../../models/topology";
import { useLocale } from "../../i18n/locale";
import { activityMessage, activitySource } from "../../i18n/presentation";

export function ActivityPanel({ activity }: { activity: ActivityEntry[] }) {
  const { t, formatTime } = useLocale();
  return (
    <section className="page-section" aria-labelledby="activity-title" data-webcodex-page="activity">
      <div className="eyebrow">{t("activity.eyebrow")}</div>
      <h1 id="activity-title">{t("activity.title")}</h1>
      <p className="lede">{t("activity.description")}</p>
      <div className="activity-list">
        {activity.length === 0 && <div className="empty-state">{t("activity.empty")}</div>}
        {[...activity].reverse().map((entry) => (
          <article className="activity-row" key={entry.sequence}>
            <i className={`status-dot ${entry.level === "error" ? "error" : entry.level === "warning" ? "pending" : "unknown"}`} aria-hidden="true" />
            <div>
              <strong>{activityMessage(entry, t)}</strong>
              <span>{activitySource(entry.source, t)} · {formatTime(entry.timestamp_ms)}</span>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

