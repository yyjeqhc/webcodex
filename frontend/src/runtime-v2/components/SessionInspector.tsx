import { AlertTriangle, CircleDot, GitBranch, Monitor } from "lucide-react";
import { useState } from "react";
import { absoluteTime, projectDisplayName, relativeTime, shortId } from "../model/format.js";
import type { ProjectRow, SessionDetail } from "../model/types.js";
import type { WorkItem } from "../model/work.js";
import type { SessionLocation } from "../state/useSessionWorkspace.js";
import type { Availability } from "../model/types.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";

type Props = {
  item: WorkItem;
  location: SessionLocation;
  detail: SessionDetail | null;
  detailAvailability: Availability;
  project?: ProjectRow;
  branch?: string | null;
  language: RuntimeLanguage;
};

export function SessionInspector({
  item,
  location,
  detail,
  detailAvailability,
  project,
  branch,
  language,
}: Props) {
  const [tab, setTab] = useState<"context" | "evidence">("context");
  const t = (value: string) => translate(value, language);
  const validation = detail?.overview.validation || item.validation;
  const attention = detail?.overview.attention;
  const attentionTotal = detail && attention
    ? attention.open_guidance + attention.open_questions + attention.open_risks + attention.open_todos
    : item.attentionCount;
  const running = (detail?.running_jobs ?? item.runningJobs) > 0;
  const composeMessage = (kind: "note" | "guidance" | "question" | "todo") => {
    window.dispatchEvent(new CustomEvent("webcodex-runtime-compose-message", { detail: { kind } }));
  };

  return (
    <aside className="inspector" aria-label={t("Session context")}>
      <div className="inspector-header">
        <div>
          <span className="eyebrow">{t("Session context")}</span>
          <strong>{tab === "context" ? t("What matters now") : t("Raw evidence")}</strong>
        </div>
      </div>
      <div className="segmented" role="tablist">
        <button role="tab" aria-selected={tab === "context"} className={tab === "context" ? "active" : ""} onClick={() => setTab("context")}>{t("Context")}</button>
        <button role="tab" aria-selected={tab === "evidence"} className={tab === "evidence" ? "active" : ""} onClick={() => setTab("evidence")}>{t("Evidence")}</button>
      </div>

      {tab === "context" ? (
        <div className="inspector-content">
          <section className="context-hero">
            <span className="context-kicker"><CircleDot size={14} /> {running ? t("Running") : attentionTotal ? t("Needs attention") : item.lifecycle}</span>
            <strong>{item.title}</strong>
            <p>{item.phase}</p>
          </section>

          {detailAvailability === "stale" && (
            <p className="state-note warn">{t("Refresh failed · showing previous data")}</p>
          )}

          <section className="inspector-section">
            <h3>{t("Current work")}</h3>
            <div className="fact-list">
              <div><span>{t("Project")}</span><strong>{projectDisplayName(project?.name, location.projectId)}</strong></div>
              <div><span>{t("Runner")}</span><strong>{location.runner}</strong></div>
              <div><span>{t("Branch")}</span><strong><GitBranch size={13} /> {branch || t("Not checked")}</strong></div>
              <div className="fact-path"><span>{t("Path")}</span><strong><code title={project?.path}>{project?.path || "—"}</code></strong></div>
              <div><span>{t("Last activity")}</span><strong>{relativeTime(detail?.updated_at || item.updatedAt)}</strong></div>
              <div><span>{t("Jobs")}</span><strong>{detail?.running_jobs ?? item.runningJobs}</strong></div>
            </div>
          </section>

          <section className={"attention-card" + (attentionTotal ? " active" : "")}>
            <div className="attention-title"><AlertTriangle size={16} /><strong>{t("Attention")}</strong></div>
            <p>
              {attentionTotal
                ? String(attentionTotal) + " " + t("open attention items")
                : t("No blocking attention in the loaded Session evidence.")}
            </p>
          </section>


          <section className="inspector-section collaboration-panel">
            <h3>{t("Collaborate")}</h3>
            <p className="muted-copy">{t("Leave retained guidance, questions, todos, or notes for the next turn.")}</p>
            <div className="collaboration-quick-actions">
              <button type="button" onClick={() => composeMessage("guidance")}>{t("Guidance")}</button>
              <button type="button" onClick={() => composeMessage("question")}>{t("Question")}</button>
              <button type="button" onClick={() => composeMessage("todo")}>{t("Todo")}</button>
              <button type="button" onClick={() => composeMessage("note")}>{t("Note")}</button>
            </div>
          </section>
          <section className="inspector-section">
            <h3>{t("Validation")}</h3>
            <div className="validation-mini">
              <span className={"status-dot " + (
                validation.state === "pass" || validation.state === "passed"
                  ? "good"
                  : validation.unresolved_failure_count
                    ? "warn"
                    : "running"
              )} />
              <div>
                <strong>{validation.state || t("Not run")}</strong>
                <small>
                  {validation.latest_kind || t("No current validation evidence")}
                  {validation.latest_at ? " · " + relativeTime(validation.latest_at) : ""}
                </small>
              </div>
            </div>
          </section>
        </div>
      ) : (
        <div className="inspector-content evidence">
          <section className="inspector-section">
            <h3>{t("Session identity")}</h3>
            <dl>
              <div><dt>{t("Session")}</dt><dd><code title={location.sessionId}>{location.sessionId}</code></dd></div>
              <div><dt>{t("Lifecycle")}</dt><dd>{detail?.lifecycle || item.lifecycle}</dd></div>
              <div><dt>{t("Mode")}</dt><dd>{detail?.mode || item.mode}</dd></div>
              <div><dt>{t("Created")}</dt><dd>{absoluteTime(detail?.created_at)}</dd></div>
              <div><dt>{t("Updated")}</dt><dd>{absoluteTime(detail?.updated_at || item.updatedAt)}</dd></div>
            </dl>
          </section>

          <section className="inspector-section">
            <h3>{t("Workspace")}</h3>
            <dl>
              <div><dt>{t("Project")}</dt><dd><code title={location.projectId}>{project?.project_ref || location.projectId}</code></dd></div>
              <div><dt>{t("Path")}</dt><dd><code title={project?.path}>{project?.path || "—"}</code></dd></div>
            </dl>
          </section>

          <section className="inspector-section">
            <h3>{t("Linked Windows")}</h3>
            {detail?.linked_windows.length ? detail.linked_windows.map((window) => (
              <div className="evidence-row static" key={window.client_window_key}>
                <span><Monitor size={15} /></span>
                <span>
                  <strong>Window {shortId(window.client_window_key)}</strong>
                  <small>{window.source} · {window.relations.join(", ")}</small>
                </span>
              </div>
            )) : (
              <p className="muted-copy">
                {detail?.window_activity_available === false
                  ? t("Window activity unavailable")
                  : t("No linked Windows in retained evidence.")}
              </p>
            )}
          </section>
        </div>
      )}
    </aside>
  );
}
