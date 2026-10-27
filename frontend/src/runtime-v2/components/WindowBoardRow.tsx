import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { absoluteTime, dayAwareTime, relativeTime } from "../model/format.js";
import { WINDOW_TIER_TEXT, type WindowBoardRow as Row } from "../model/windowBoard.js";

export function WindowBoardRow({ row, selected, language, onSelect }: {
  row: Row; selected: boolean; language: RuntimeLanguage; onSelect: (key: string) => void;
}) {
  const t = (text: string) => translate(text, language);
  return <button type="button" className={"window-board-row" + (selected ? " selected" : "")}
    aria-current={selected ? "true" : undefined} onClick={() => onSelect(row.window.client_window_key)}
    data-testid={"work-window-row-" + row.window.client_window_key}>
    <span className={"window-board-dot " + row.tier} aria-label={t(WINDOW_TIER_TEXT[row.tier])} />
    <span className="window-board-name"><strong>{row.displayName}</strong>
      <small className="window-board-full-context">{row.sourceLabel} · {row.linkedSessions} {t("Sessions")}</small>
      <small className="window-board-compact-context">{[row.workspaceName, row.machine, row.sourceLabel].filter(Boolean).join(" · ")}</small>
    </span>
    <span className="window-board-machine">{row.machine || "—"}</span>
    <span className="window-board-project">{row.projectName || "—"}<code title={row.path}>{row.path}</code></span>
    <time className="window-board-first" title={absoluteTime(row.firstSeenAt)}>{dayAwareTime(row.firstSeenAt)}</time>
    <span className="window-board-last"><time title={absoluteTime(row.lastActiveAt)}>{relativeTime(row.lastActiveAt)}</time>
      <small>{dayAwareTime(row.lastActiveAt)}</small><em>{row.tier === "running" ? t("Running") : relativeTime(row.lastActiveAt)}</em></span>
    <span className={"window-board-action" + (row.tier === "running" ? " running" : "")} title={row.action}>{row.action || "—"}
      <small>{t("First active")} · {dayAwareTime(row.firstSeenAt)}</small></span>
  </button>;
}
