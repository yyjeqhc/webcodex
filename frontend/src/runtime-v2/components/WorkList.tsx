import { ArrowUpRight, LoaderCircle, Search } from "lucide-react";
import { useMemo } from "react";
import { relativeTime } from "../model/format.js";
import type { WorkBucket, WorkItem } from "../model/work.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";

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
  onSearch,
  onLocateExact,
  onSelect,
}: Props) {
  const t = (value: string) => translate(value, language);
  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query || /^wc_sess_[A-Za-z0-9_-]+$/.test(query)) return items;
    return items.filter((item) =>
      [item.title, item.projectName, item.projectId, item.runner, item.phase, item.sessionId]
        .some((value) => value.toLowerCase().includes(query))
    );
  }, [items, search]);
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
      </div>
      <div className="work-search">
        <Search size={15} />
        <input
          aria-label={t("Search Sessions")}
          placeholder={t("Search work or paste a Session ID…")}
          value={search}
          onChange={(event) => onSearch(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") onLocateExact();
          }}
        />
        {/^wc_sess_/.test(search.trim()) && (
          <button type="button" onClick={onLocateExact} disabled={locating}>
            {locating ? <LoaderCircle size={14} /> : <ArrowUpRight size={14} />}
          </button>
        )}
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
