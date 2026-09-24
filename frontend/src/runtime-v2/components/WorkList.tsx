import { ActionIcon, TextInput } from "@mantine/core";
import { ArrowUpRight, Search } from "lucide-react";
import { motion } from "motion/react";
import { useMemo, useState } from "react";
import { relativeTime } from "../model/format.js";
import type { WorkBucket, WorkItem } from "../model/work.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { WorkSurfaceSwitch, type WorkSurface } from "./GoalWorkbench.js";

const BUCKET_ORDER: WorkBucket[] = ["running", "attention", "active", "recent"];
export const BUCKET_LABEL: Record<WorkBucket, string> = {
  running: "Jobs running",
  attention: "Needs attention",
  active: "Active Sessions",
  recent: "Recent Sessions",
};

type Props = {
  items: WorkItem[];
  selectedKey: string;
  search: string;
  locating: boolean;
  language: RuntimeLanguage;
  inventoryIncomplete: boolean;
  surface: WorkSurface;
  onSurfaceChange: (surface: WorkSurface) => void;
  onSearch: (value: string) => void;
  onLocateExact: () => void;
  onSelect: (item: WorkItem) => void;
};

export function WorkList({
  items,
  selectedKey,
  search,
  locating,
  language,
  inventoryIncomplete,
  surface,
  onSurfaceChange,
  onSearch,
  onLocateExact,
  onSelect,
}: Props) {
  const t = (value: string) => translate(value, language);
  const [projectFilter, setProjectFilter] = useState("");
  const projectOptions = [...new Map(items.map((item) => [item.projectId, item.projectName])).entries()];
  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    const scoped = items.filter((item) => !projectFilter || item.projectId === projectFilter);
    if (!query || /^wc_sess_[A-Za-z0-9_-]+$/.test(query)) return scoped;
    return scoped.filter((item) =>
      [item.title, item.projectName, item.projectId, item.runner, item.phase, item.sessionId]
        .some((value) => value.toLowerCase().includes(query))
    );
  }, [items, search, projectFilter]);
  const groups = useMemo(
    () => BUCKET_ORDER.map((bucket) => ({
      bucket,
      items: filtered.filter((item) => item.bucket === bucket),
    })),
    [filtered],
  );

  return (
    <aside className="work-list-panel">
      <div className="work-list-header">
        <div><span className="eyebrow">{t("Workspace")}</span><h1>{t("Work")}</h1></div>
        <WorkSurfaceSwitch surface={surface} onSurfaceChange={onSurfaceChange} language={language} />
      </div>
      <div className="work-list-filters">
        <label className="activity-project-filter">
          <span>{t("Project filter")}</span>
          <select value={projectFilter} onChange={(event) => setProjectFilter(event.target.value)}>
            <option value="">{t("All Projects")}</option>
            {projectOptions.map(([id, name]) => <option key={id} value={id}>{name} · {id}</option>)}
          </select>
        </label>
        <TextInput className="work-search-field" type="search" leftSection={<Search size={15} />}
          aria-label={t("Search Sessions")}
          placeholder={t("Search work or paste a Session ID…")}
          value={search}
          onChange={(event) => onSearch(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") onLocateExact();
          }}
          rightSection={/^wc_sess_/.test(search.trim()) ? <ActionIcon variant="light" size="sm"
            aria-label={t("Locate exact Session")} onClick={onLocateExact} loading={locating}>
            <ArrowUpRight size={14} />
          </ActionIcon> : null}
        />
      </div>
      <div className="work-list-scroll">
        {inventoryIncomplete && (
          <div className="inventory-note">{t("Recent Session inventory is bounded. Paste an exact Session ID to locate omitted work.")}</div>
        )}
        {groups.map(({ bucket, items: bucketItems }) => bucketItems.length ? (
          <section className="work-group" key={bucket}>
            <div className="work-group-heading"><span>{t(BUCKET_LABEL[bucket])}</span><small>{bucketItems.length}</small></div>
            <div className="work-group-list">
              {bucketItems.map((item) => (
                <button
                  className={"work-row" + (selectedKey === item.key ? " selected" : "")}
                  type="button"
                  onClick={() => onSelect(item)}
                  data-testid={"work-row-" + item.sessionId}
                  key={item.key}
                >
                  {selectedKey === item.key && <motion.span className="ui-selection-rail" layoutId="runtime-session-rail" aria-hidden="true" />}
                  <span className={"work-state-dot " + item.bucket} />
                  <span className="work-row-body">
                    <strong>{item.title}</strong>
                    <span className="work-row-location">{item.projectName} · {item.runner}</span>
                    <span className="work-row-status">{item.phase}</span>
                  </span>
                  <time>{relativeTime(item.updatedAt)}</time>
                </button>
              ))}
            </div>
          </section>
        ) : null)}
        {!filtered.length && (
          <div className="empty-panel"><Search size={18} /><strong>{t("No matching Sessions")}</strong></div>
        )}
      </div>
    </aside>
  );
}
