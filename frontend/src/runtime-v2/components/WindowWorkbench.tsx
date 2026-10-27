import { useFlowingSelection } from "../../ui/useFlowingSelection.js";
import {
  ArrowLeft,
  Filter,
  RefreshCw,
  X,
  MessageSquare,
  Monitor,
  Search,
} from "lucide-react";
import { CopyIdentity } from "./ui/CopyIdentity.js";
import { FocusTrap, Popover, TextInput } from "@mantine/core";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  displayProjectPath,
  projectFamilyId,
  projectFamilyName,
  projectVariantLabel,
  sourceProjectRuntimeId,
} from "../../ui/projectPresentation.js";
import { ProjectPicker } from "../../ui/ProjectPicker.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { absoluteTime, clockTime, relativeTime } from "../model/format.js";
import type { ProjectRow, WindowSummary } from "../model/types.js";
import { windowSessionCatalog } from "../model/windowSessions.js";

import { useWindowWorkspace } from "../state/useWindowWorkspace.js";
import { WindowCollaboration } from "./WindowCollaboration.js";
import { WindowActivityFeed } from "./WindowActivityFeed.js";
import { WindowLabelEditor } from "./WindowLabelEditor.js";
import { WindowBoardRow } from "./WindowBoardRow.js";
import { WindowBoardSplit } from "./WindowBoardSplit.js";
import { compareWindowRows, matchesWindowRow, windowBoardRow, WINDOW_TIERS, WINDOW_TIER_TEXT, type WindowTier } from "../model/windowBoard.js";
import { useWindowBoardNavigation } from "../state/useWindowBoardNavigation.js";


type Props = {
  client: RuntimeV2Client;
  language: RuntimeLanguage;
  projects: ProjectRow[];
  onUnauthorized: () => void;
  onOpenSessionRecord?: (project: string, sessionId: string) => void;
  requestedWindowKey?: string;
  requestedSessionId?: string;
  onRequestedWindowConsumed?: () => void;
  onMobileDetailChange?: (open: boolean) => void;
};

type ProjectFamily = {
  id: string;
  label: string;
  runner: string;
  projectIds: Set<string>;
  windowCount: number;
  activeWindowCount: number;
  lastObservedAt: number;
};

function windowObservedAt(window: WindowSummary): number {
  return window.last_seen_at_ms || 0;
}

function projectFor(index: ReadonlyMap<string, ProjectRow>, id?: string): ProjectRow | undefined {
  return id ? index.get(id) : undefined;
}

function buildProjectFamilies(projects: ProjectRow[], projectIndex: ReadonlyMap<string, ProjectRow>, windows: WindowSummary[]): ProjectFamily[] {
  const groups = new Map<string, ProjectRow[]>();
  for (const project of projects) {
    const id = projectFamilyId(project);
    const rows = groups.get(id) || [];
    rows.push(project);
    groups.set(id, rows);
  }
  const families = [...groups.entries()].map(([id, rows]) => {
    const representative = projectIndex.get(id) || rows[0];
    return {
      id,
      label: projectFamilyName(representative, projectIndex),
      runner: representative.client_id,
      projectIds: new Set(rows.map((row) => row.id)),
      windowCount: 0,
      activeWindowCount: 0,
      lastObservedAt: 0,
    };
  });
  const byId = new Map(families.map((family) => [family.id, family]));
  for (const window of windows) {
    const project = projectFor(projectIndex, window.last_project);
    if (!project) continue;
    const family = byId.get(projectFamilyId(project));
    if (!family) continue;
    family.windowCount += 1;
    if (window.active_count > 0) family.activeWindowCount += 1;
    family.lastObservedAt = Math.max(family.lastObservedAt, windowObservedAt(window));
  }
  return families.sort((a, b) =>
    Number(b.activeWindowCount > 0) - Number(a.activeWindowCount > 0) ||
    Number(b.windowCount > 0) - Number(a.windowCount > 0) ||
    b.lastObservedAt - a.lastObservedAt ||
    a.label.localeCompare(b.label));
}

export function WindowWorkbench({
  client,
  language,
  projects,
  onUnauthorized,
  onOpenSessionRecord,
  requestedWindowKey,
  requestedSessionId,
  onRequestedWindowConsumed,
  onMobileDetailChange,
}: Props) {
  const t = (value: string) => translate(value, language);
  const projectIndex = useMemo(() => new Map(projects.map(project => [project.id, project])), [projects]);
  const [projectFamily, setProjectFamily] = useState("");
  const selectedProjects = useMemo(() => projectFamily ? projects.filter(project => projectFamilyId(project) === projectFamily).map(project => project.id) : undefined, [projects, projectFamily]);
  const windows = useWindowWorkspace(client, !selectedProjects || selectedProjects.length > 0, onUnauthorized, { refreshMs: 3_000, loadDetail: true, autoSelect: false, initialWindowKey: requestedWindowKey, projects: selectedProjects });
  const navigation = useWindowBoardNavigation(windows.selectedKey, windows.select, onMobileDetailChange);
  const [tierFilter, setTierFilter] = useState<"all" | "named" | WindowTier>("all");
  const [editingKey, setEditingKey] = useState("");
  const [search, setSearch] = useState("");
  const [centerTab, setCenterTab] = useState<"window" | "collaboration">("window");
  const selectionRef = useRef<HTMLDivElement | null>(null);
  useFlowingSelection(selectionRef, centerTab, { variant: "underline" });
  const [selectedSessionId, setSelectedSessionId] = useState("");

  const historicalFamilies = useMemo(() => buildProjectFamilies(projects, projectIndex, windows.historicalWindows), [projects, projectIndex, windows.historicalWindows]);
  const familyById = useMemo(() => new Map(historicalFamilies.map(family => [family.id, family])), [historicalFamilies]);
  const families = useMemo(() => {
    const active = new Map<string, number>();
    for (const window of windows.windows) {
      const project = projectFor(projectIndex, window.last_project);
      if (project && window.active_count > 0) {
        const id = projectFamilyId(project); active.set(id, (active.get(id) || 0) + 1);
      }
    }
    return historicalFamilies.map(family => ({ ...family, activeWindowCount: active.get(family.id) || 0 }));
  }, [historicalFamilies, projectIndex, windows.windows]);
  const activeFamily = families.find(family => family.id === projectFamily);
  const windowIndex = useMemo(() => new Map(windows.windows.map(window => [window.client_window_key, window])), [windows.windows]);

  const now = Date.now();
  const rows = useMemo(() => windows.windows.map(row => windowBoardRow(row, projectIndex, now, t("Window"))).sort(compareWindowRows), [windows.windows, projectIndex, now, language]);
  const matching = rows.filter(row => {
    const family = familyById.get(projectFamily);
    return (!family || (row.window.last_project && family.projectIds.has(row.window.last_project))) && matchesWindowRow(row, search);
  });
  const filtered = matching.filter(row => tierFilter === "all" || (tierFilter === "named" ? row.named : row.tier === tierFilter));

  const detail = windows.detail;
  const linkedSessions = useMemo(() => detail ? windowSessionCatalog(detail) : [], [detail?.activity, detail?.linked_sessions]);
  // A different Window keeps the operator's current view but never inherits
  // the previous Window's Session focus.
  useEffect(() => {
    if (!windows.selectedKey) return;
    setSelectedSessionId("");
  }, [windows.selectedKey]);
  useEffect(() => {
    if (!requestedWindowKey) return;
    if (windows.selectedKey !== requestedWindowKey) {
      windows.select(requestedWindowKey);
      return;
    }
    setSearch("");
    setProjectFamily("");
    setCenterTab("window");
    setSelectedSessionId(requestedSessionId || "");
    onRequestedWindowConsumed?.();
  }, [onRequestedWindowConsumed, requestedWindowKey, requestedSessionId, windows.selectedKey, windows.select]);
  const selectedSummary = windowIndex.get(windows.selectedKey);
  const activeRequest = detail?.active_requests.slice().sort((a, b) => b.started_at_ms - a.started_at_ms)[0];
  const currentProjectId = activeRequest?.project || selectedSummary?.last_project || detail?.activity[0]?.project;
  const currentProject = projectFor(projectIndex, currentProjectId);
  const sourceProject = currentProject ? projectFor(projectIndex, sourceProjectRuntimeId(currentProject)) : undefined;
  const currentActivity = activeRequest?.tool_name ||
    selectedSummary?.last_activity_name ||
    detail?.activity[0]?.tool_name ||
    detail?.activity[0]?.method;
  const lastObservedAt = Math.max(detail?.last_seen_at_ms || 0, selectedSummary?.last_seen_at_ms || 0) || undefined;
  const lastMeaningfulAt = Math.max(detail?.last_meaningful_activity_at_ms || 0, selectedSummary?.last_meaningful_activity_at_ms || 0) || undefined;
  const firstObservedAt = detail?.first_seen_at_ms || selectedSummary?.first_seen_at_ms;
  const isActive = (selectedSummary?.active_count ?? detail?.active_count ?? 0) > 0;
  const currentProjectName = currentProject
    ? projectFamilyName(sourceProject || currentProject, projectIndex)
    : undefined;
  const currentWorkspaceName = currentProject?.lineage
    ? projectVariantLabel(currentProject)
    : currentProjectName;
  const currentRunnerId = currentProject?.client_id;
  const currentDirectory = displayProjectPath(currentProject?.path);
  const hasSelectedWindow = Boolean(windows.selectedKey);
  const visibleActiveCount = selectedSummary?.active_count ?? detail?.active_count ?? 0;
  const selectedNavigationRow: WindowSummary | undefined = detail ? {
    ...selectedSummary,
    label: selectedSummary && Object.hasOwn(selectedSummary, "label") ? selectedSummary.label : detail.label,
    client_window_key: detail.client_window_key, source: detail.source,
    last_project: currentProjectId, last_seen_at_ms: lastObservedAt || 0,
    first_seen_at_ms: firstObservedAt, last_meaningful_activity_at_ms: lastMeaningfulAt,
    last_activity_name: currentActivity, active_count: visibleActiveCount,
    linked_session_count: selectedSummary?.linked_session_count ?? detail.sessions_returned,
    recorder_gap_count: selectedSummary?.recorder_gap_count ?? 0,
  } : selectedSummary;
  const selectedRow = selectedNavigationRow && windowBoardRow(selectedNavigationRow, projectIndex, now, t("Window"));
  const selectedIsPinned = Boolean(windows.selectedKey && !filtered.some(row => row.window.client_window_key === windows.selectedKey));
  const renderWindow = (row: ReturnType<typeof windowBoardRow>) => <WindowBoardRow key={row.window.client_window_key}
    row={row} selected={windows.selectedKey === row.window.client_window_key} language={language} onSelect={navigation.open} onRename={key => { navigation.open(key); setEditingKey(key); }} />;
  const fact = (name: string, value: string | undefined) => <div><dt>{t(name)}</dt><dd>{value || "—"}</dd></div>;
  const identity = <dl className="window-board-facts window-board-id-facts">
    <div data-testid="current-window-identity" title={windows.selectedKey}><dt>{t("Window ID")}</dt><dd><CopyIdentity value={windows.selectedKey} label={t("Window ID")} language={language} /></dd></div>
    {currentProjectId && <div><dt>{t("Project ID")}</dt><dd><CopyIdentity value={currentProjectId} label={t("Project ID")} language={language} /></dd></div>}
    {fact("Last request activity", absoluteTime(lastObservedAt))}
  </dl>;

  return (
    <div className="window-primary-workbench" data-testid="window-primary-workbench" data-mobile-detail={navigation.mobile && hasSelectedWindow || undefined}>
      <div className="window-board-toolbar">
        <TextInput type="search" aria-label={t("Search Windows")} leftSection={<Search size={14} />} value={search}
          onChange={event => setSearch(event.currentTarget.value)} placeholder={t("Search windows or projects…")} />
        <div className="window-board-chips">
          {(["all", "running", "recent", "named"] as const).map(tier => <button key={tier} type="button" aria-pressed={tierFilter === tier}
            onClick={() => setTierFilter(tier)}>{t(tier === "all" ? "All" : tier === "named" ? "Named" : WINDOW_TIER_TEXT[tier])} <span>{matching.filter(row => tier === "all" || (tier === "named" ? row.named : row.tier === tier)).length}</span></button>)}
          <Popover position="bottom-start" width={320}>
            {/* A server-side Project filter changes every count, so it stays visible while active. */}
            <Popover.Target><button type="button" className={activeFamily ? "active" : undefined}
              aria-label={activeFamily ? t("Filter") + ": " + activeFamily.label : undefined}>
              <Filter size={14} />{activeFamily ? <span className="window-board-filter-label">{activeFamily.label}</span> : t("Filter")}
            </button></Popover.Target>
            <Popover.Dropdown>
          <ProjectPicker
            label={t("Projects")}
            allLabel={t("All projects")}
            emptyLabel={t("No matching projects")}
            searchLabel={t("Search projects")}
            value={projectFamily}
            onChange={setProjectFamily}
            options={families.map((row) => ({
              value: row.id,
              label: row.label,
              detail: row.windowCount
                ? [
                    row.activeWindowCount ? row.activeWindowCount + " " + t("active") : "",
                    row.windowCount + " " + t(row.windowCount === 1 ? "Window" : "Windows"),
                    row.runner,
                  ].filter(Boolean).join(" · ")
                : t("No Window activity") + " · " + row.runner,
            }))}
          />
            </Popover.Dropdown>
          </Popover>
        </div>
        <div className="window-board-sync"><span>{t("Synced")} {clockTime(windows.updatedAt ?? undefined)}</span>
          <button type="button" aria-label={t("Refresh")} onClick={windows.refresh}><RefreshCw size={16} /></button></div>
      </div>
      <WindowBoardSplit selected={hasSelectedWindow} label={t("Resize window list")}>
      <section className="window-board-list" aria-label={t("Windows")}>
        <div className="window-board-columns" aria-hidden="true"><span /><span>{t("Name")}</span><span className="window-board-machine">{t("Machine")}</span>
          <span className="window-board-project">{t("Project")} · {t("Directory")}</span><span className="window-board-first">{t("First active")}</span>
          <span>{t("Last active")}</span><span className="window-board-action">{t("Latest activity")}</span></div>
        {selectedIsPinned && <section className="window-current-selection"><h3>{t("Current Window")}</h3>{selectedRow ? renderWindow(selectedRow) : <button type="button" className="window-board-pinned" aria-current="true" data-testid={"work-window-row-" + windows.selectedKey} onClick={windows.refresh}>
          {t("Window")} · {windows.selectedKey.slice(0, 4)}<small>{t(windows.detailAvailability === "error" || windows.detailAvailability === "denied" ? "Window activity unavailable" : "Loading recent activity…")}</small></button>}</section>}
        {WINDOW_TIERS.map(tier => {
          const group = filtered.filter(row => row.tier === tier);
          return group.length > 0 && <section key={tier}><h3><span className={"window-board-dot " + tier} aria-hidden="true" />{t(WINDOW_TIER_TEXT[tier])} <span>{group.length}</span></h3>{group.map(row => renderWindow(row.window.client_window_key === windows.selectedKey && selectedRow ? selectedRow : row))}</section>;
        })}
          {windows.availability === "loading" && <div className="empty-inline">{t("Loading Window activity…")}</div>}
          {windows.availability === "stale" && <div className="inventory-note">{t("Window activity refresh failed; showing previous observations.")}</div>}
          {(windows.availability === "error" || windows.availability === "denied") && <div className="empty-inline">{t("Window activity unavailable")}</div>}
          {windows.availability === "available" && !filtered.length && <div className="empty-panel"><Monitor size={18} /><strong>{t("No matching Windows")}</strong></div>}
          {windows.truncated && <div className="inventory-note">{t("Window inventory is bounded; not all observed Windows are loaded.")} <button type="button" className="text-button" disabled={windows.loadingMore} onClick={windows.loadMore}>{t("Load more")} · {windows.windows.length} / {windows.total}</button></div>}
      </section>

      {hasSelectedWindow && <>
      {navigation.drawer && <button className="window-board-backdrop" type="button" aria-label={t("Close")} onClick={navigation.close} />}
      <FocusTrap active={navigation.drawer} innerRef={navigation.detailRef}>
      <section role={navigation.drawer ? "dialog" : undefined} aria-modal={navigation.drawer || undefined} className="window-board-detail" tabIndex={-1} aria-label={t("Window details")}
        data-layout={navigation.mobile ? "page" : navigation.drawer ? "drawer" : "split"}>
            <header className="window-board-detail-header">
              {navigation.mobile && <button type="button" aria-label={t("Back")} onClick={navigation.close}><ArrowLeft size={18} /></button>}
              <div><WindowLabelEditor key={windows.selectedKey} name={selectedRow?.displayName || t("Window") + " · " + windows.selectedKey.slice(0, 4)}
                label={selectedNavigationRow?.label} mobile={navigation.mobile} language={language} editing={editingKey === windows.selectedKey}
                saving={windows.labelSaving || !selectedNavigationRow} error={windows.labelError} onEditingChange={editing => setEditingKey(editing ? windows.selectedKey : "")}
                onSave={label => windows.saveLabel(windows.selectedKey, label)} />
                <span className={"window-board-status" + (isActive ? " running" : "")}>{selectedRow && <span className={"window-board-dot " + (isActive ? "running" : selectedRow.tier)} aria-hidden="true" />}{isActive ? t("Running") + (currentActivity ? " · " + currentActivity : "") : selectedRow ? t(WINDOW_TIER_TEXT[selectedRow.tier]) + " · " + relativeTime(selectedRow.lastActiveAt) : t("Loading…")}</span></div>
              {!navigation.mobile && <button type="button" aria-label={t("Close")} onClick={navigation.close}><X size={18} /></button>}
            </header>
            <div className="window-board-context">
              <dl className="window-board-facts">
                {fact("Machine", currentRunnerId)}{fact("Source", selectedRow?.sourceLabel)}{fact("Project", currentWorkspaceName)}
                {fact("First active", absoluteTime(firstObservedAt))}
                {fact("Last active", absoluteTime(lastMeaningfulAt ?? lastObservedAt))}
                <div className="window-board-path"><dt>{t("Directory")}</dt><dd><code>{currentDirectory || "—"}</code></dd></div>
              </dl>
              {/* Canonical IDs stay one disclosure away so activity keeps the detail height. */}
              <details><summary>{t("IDs")}</summary>{identity}</details>
            </div>
            <div ref={selectionRef} className="session-view-tabs window-center-tabs equal-view-tabs" role="tablist" aria-label={t("Window views")}>
              <button
                id="window-activity-tab"
                role="tab"
                aria-selected={centerTab === "window"}
                aria-controls="window-activity-panel"
                className={centerTab === "window" ? "active" : ""}
                type="button"
                onClick={() => setCenterTab("window")}
              >
                <Monitor size={14} />
                {t("Window")}
              </button>
              <button
                id="window-collaboration-tab"
                role="tab"
                aria-selected={centerTab === "collaboration"}
                aria-controls="window-collaboration-panel"
                className={centerTab === "collaboration" ? "active" : ""}
                type="button"
                onClick={() => setCenterTab("collaboration")}
              >
                <MessageSquare size={14} />
                {t("Collaboration")}
              </button>
            </div>

            <div
              id="window-activity-panel"
              className="window-board-panel"
              role="tabpanel"
              aria-labelledby="window-activity-tab"
              hidden={centerTab !== "window"}
            >
              {windows.detailAvailability === "stale" && <div className="inventory-note">{t("Window activity refresh failed; showing previous observations.")}</div>}
              {windows.detailHydrating && detail?.detail_level !== "full" && (
                <div className="window-progressive-note">{t("Loading history…")}</div>
              )}
              {detail ? (
                <WindowActivityFeed
                  client={client}
                  key={detail.client_window_key}
                  detail={detail}
                  projects={projects}
                  language={language}
                  selectedSessionId={selectedSessionId}
                  onSelectSession={setSelectedSessionId}
                  onOpenSessionRecord={onOpenSessionRecord}
                />
              ) : (
                <div className="empty-inline">
                  {t(
                    windows.detailAvailability === "error" || windows.detailAvailability === "denied"
                      ? "Window activity unavailable"
                      : "Loading recent activity…",
                  )}
                </div>
              )}
            </div>

            <div
              id="window-collaboration-panel"
              className="window-board-panel window-collaboration-pane"
              role="tabpanel"
              aria-labelledby="window-collaboration-tab"
              hidden={centerTab !== "collaboration"}
            >
              {windows.selectedKey && (
                <WindowCollaboration
                  key={windows.selectedKey}
                  client={client}
                  windowKey={windows.selectedKey}
                  active={centerTab === "collaboration"}
                  selectedSessionId={selectedSessionId}
                  sessions={linkedSessions}
                  onSelectSession={setSelectedSessionId}
                  language={language}
                  onUnauthorized={onUnauthorized}
                />
              )}
            </div>
      </section>
      </FocusTrap>
      </>}
      </WindowBoardSplit>
    </div>
  );
}
