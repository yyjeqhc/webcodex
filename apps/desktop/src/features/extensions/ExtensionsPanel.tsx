import { useState } from "react";
import { useRunnerCapabilitiesText } from "../../i18n/runner-capabilities";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { useInstructionsText } from "../../i18n/instructions";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import type { DesktopState } from "../../models/topology";
import { displayProjectPath, projectName } from "../workspace/WorkspaceContext";
import { ProjectPicker } from "../../../../../frontend/src/ui/ProjectPicker";
import { WorkspaceEmptyState } from "../../components/WorkspaceEmptyState";
import { ExtensionPathsEditor } from "./ExtensionPathsEditor";
import { InstructionDocument } from "./ProjectExtensionPreviews";
import { EXTENSION_PANELS, extensionPanel, extensionTabForKey, type ExtensionTab } from "./extension-panel-registry";
import type { ExtensionPanelContext } from "./extension-panel-types";
import { useExtensionsState } from "./useExtensionsState";

export type { ExtensionTab } from "./extension-panel-registry";

export function ExtensionsPanel({ state, onState, initialTab }: { state: DesktopState; onState: (state: DesktopState) => void; initialTab?: ExtensionTab }) {
  const { t } = useLocale(); const p = useProduct(); const c = useConnectionsTools(); const r = useRunnerCapabilitiesText();
  const instructionsText = useInstructionsText();
  const [tab, setTab] = useState<ExtensionTab>(() => extensionPanel(initialTab).id);
  const view = useExtensionsState(state, onState);
  const panel = extensionPanel(tab);
  const { Content, Configuration } = panel;
  const text = { product: p, connections: c, capabilities: r };
  const context: ExtensionPanelContext = {
    state, onState, settings: view.settings, settingsFailed: view.settingsFailed, disabled: view.disabled,
    catalog: view.catalog, project: view.project,
    projectLabel: projectName(view.projects.find(row => row.id === view.project) || { id: view.project }),
    pendingRestart: view.pendingRestart, onRefresh: view.refresh, onRestarted: view.onRestarted, onRestart: view.restart,
    onAddPlugin: view.addPlugin, onReloadPlugin: view.reloadPlugin, onOpenDocument: view.setDocument,
  };
  return <div className="page-section workspace-page" data-webcodex-page="extensions">
    <header className="page-heading-row"><h1 id="extensions-title">{t("extensions.title")}</h1>{panel.showRefresh && <button className="secondary-button" onClick={view.refresh} disabled={view.disabled || view.loading}>{p("refresh")}</button>}</header>
    <div className="extensions-layout">
      <div className="extensions-navigation" role="tablist" aria-orientation="vertical" aria-label={t("extensions.title")}>
        {EXTENSION_PANELS.map(entry => { const Icon = entry.Icon; return <button type="button" role="tab" key={entry.id} id={`extension-tab-${entry.id}`} aria-controls={`extension-view-${entry.id}`} aria-selected={tab === entry.id} tabIndex={tab === entry.id ? 0 : -1} onClick={() => setTab(entry.id)} onKeyDown={event => {
          const next = extensionTabForKey(entry.id, event.key);
          if (next === null) return;
          event.preventDefault(); setTab(next); window.document.getElementById(`extension-tab-${next}`)?.focus();
        }}><Icon size={18} aria-hidden="true" />{entry.title(text)}</button>; })}
      </div>
      <div className="extensions-content" role="tabpanel" id={`extension-view-${tab}`} aria-labelledby={`extension-tab-${tab}`}>
        {view.settingsFailed && <div className="extension-apply-bar" role="alert"><span>{p("settingsUnavailable")}</span><button type="button" className="secondary-button" disabled={view.disabled} onClick={view.refresh}>{p("refresh")}</button></div>}
        <header className="extension-category-heading"><h2>{panel.title(text)}</h2><p>{p(panel.purpose)}</p></header>
        {EXTENSION_PANELS.map(({ id, PersistentConfiguration }) => PersistentConfiguration && <PersistentConfiguration key={id} {...context} active={tab === id} />)}
        {/* One retained editor owns both path drafts, even while another panel is active. */}
        {view.settings && <div className={`extension-path-settings${panel.pathKind === "skills" ? " primary" : ""}`} hidden={!panel.pathKind}><h2>{p(panel.pathKind === "skills" ? "skillPaths" : "instructionPaths")}</h2><ExtensionPathsEditor settings={view.settings} kind={panel.pathKind ?? "instructions"} disabled={view.disabled || view.settingsFailed} onSave={view.updatePaths} onBrowse={view.browsePath} /></div>}
        {view.pathsApplied && panel.pathKind && <p className="extension-apply-bar" role="status">{instructionsText("pathsApplied")}</p>}
        {view.pathsError && panel.pathKind && <div className="error-card" role="alert"><strong>{view.pathsError.message}</strong><span>{view.pathsError.next_action}</span><code>{view.pathsError.code}</code></div>}
        {view.failed && panel.projectPreview && <p role="alert" className="workspace-notice">{p("loadError")}</p>}
        {view.loading && panel.projectPreview && <p role="status">{p("loading")}</p>}
        {Configuration && <Configuration {...context} />}
        <section className={panel.projectPreview ? "extension-project-preview" : undefined}>
          {panel.projectPreview && <><h2>{p(panel.projectPreview.title)}</h2><div className="activity-project-filter"><span className="filter-label">{p("projects")}</span><ProjectPicker label={p("projects")} emptyLabel={p("noMatches")} searchLabel={p("search")} value={view.project} onChange={view.setProject} disabled={view.disabled} options={view.projects.filter(row => row.id).map(row => ({ value: row.id, label: projectName(row), detail: displayProjectPath(row.path) }))} /></div></>}
          {panel.projectPreview && !view.project && <WorkspaceEmptyState kind={panel.projectPreview.emptyKind} message={p("selectProjectPreview")} />}
          <Content {...context} />
        </section>
      </div>
    </div>
    {view.document && <InstructionDocument key={view.document.fingerprint} project={view.project} file={view.document} onClose={() => view.setDocument(null)} />}
  </div>;
}
