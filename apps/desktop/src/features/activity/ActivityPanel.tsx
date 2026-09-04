import type { ActivityEntry } from "../../models/topology";

export function ActivityPanel({ activity }: { activity: ActivityEntry[] }) {
  return (
    <section className="page-section">
      <div className="eyebrow">Activity</div>
      <h1>Recent runtime activity</h1>
      <p className="lede">A bounded, secret-redacted history of Desktop lifecycle events. Raw terminal output is not mirrored into this view.</p>
      <div className="activity-list">
        {activity.length === 0 && <div className="empty-state">No Desktop runtime activity yet.</div>}
        {[...activity].reverse().map((entry) => (
          <article className="activity-row" key={entry.sequence}>
            <i className={`status-dot ${entry.level === "error" ? "error" : entry.level === "warning" ? "pending" : "unknown"}`} />
            <div>
              <strong>{entry.message}</strong>
              <span>{entry.source.replaceAll("_", " ")} · {new Date(entry.timestamp_ms).toLocaleTimeString()}</span>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

