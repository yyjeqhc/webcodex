import { useEffect, useRef, useState } from "react";
import { Blocks, Bot, FileText, Network, Puzzle, Terminal } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { PluginRegistrationForm } from "./PluginRegistrationForm";
import { McpProvidersPanel } from "./McpProvidersPanel";
import { CodingAgentsPanel } from "./CodingAgentsPanel";
import { SshResourcesPanel } from "./SshResourcesPanel";
import { useRunnerCapabilitiesText } from "../../i18n/runner-capabilities";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { ExtensionPathsEditor } from "./ExtensionPathsEditor";
import { ManagedInstructionsPanel } from "./ManagedInstructionsPanel";
import { useInstructionsText } from "../../i18n/instructions";
import { normalizeDesktopError } from "../../i18n/presentation";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState, RunnerSettings } from "../../models/topology";
import type { ExtensionsSnapshot, InstructionSummary } from "../../models/workspace";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { displayProjectPath, projectName, useWorkspace, workspaceQuery } from "../workspace/WorkspaceContext";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { ProjectPicker } from "../../../../../frontend/src/ui/ProjectPicker";
import { WorkspaceEmptyState } from "../../components/WorkspaceEmptyState";

type ExtensionTab = "codingAgents" | "sshResources" | "instructions" | "skills" | "mcpProviders" | "nativePlugins";
const TABS: ExtensionTab[] = ["codingAgents", "sshResources", "mcpProviders", "nativePlugins", "skills", "instructions"];
const TAB_ICONS = { codingAgents: Bot, sshResources: Terminal, mcpProviders: Network, nativePlugins: Blocks, skills: Puzzle, instructions: FileText };
const TAB_PURPOSES = { codingAgents: "codingAgentsPurpose", sshResources: "sshPurpose", mcpProviders: "mcpPurpose", nativePlugins: "pluginsPurpose", skills: "skillsPurpose", instructions: "instructionsPurpose" } as const;
export function ExtensionsPanel({ state, onState }: { state: DesktopState; onState: (state: DesktopState) => void }) {
  const { t } = useLocale(); const p = useProduct(); const c = useConnectionsTools(); const r = useRunnerCapabilitiesText(); const workspace = useWorkspace();
  const [tab, setTab] = useState<ExtensionTab>("codingAgents");
  const pathTab = tab === "instructions" || tab === "skills";
  const projectTab = pathTab || tab === "nativePlugins";
  const tabLabel = (value: ExtensionTab) => value === "instructions" || value === "nativePlugins" ? p(value) : value === "skills" ? "Skills" : value === "mcpProviders" ? c("mcpProviders") : r(value);
  const [project, setProject] = useState(state.project?.runtime_project_id || "");
  const [catalog, setCatalog] = useState<ExtensionsSnapshot | null>(null);
  const [settings, setSettings] = useState<RunnerSettings | null>(null);
  const [settingsFailed, setSettingsFailed] = useState(false);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [pendingRestart, setPendingRestart] = useState(false);
  const instructionsText = useInstructionsText();
  const [pathsApplied, setPathsApplied] = useState(false);
  const [pathsError, setPathsError] = useState<DesktopError | null>(null);
  const [revision, setRevision] = useState(0);
  const [document, setDocument] = useState<InstructionSummary | null>(null);
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => { if (project && !workspace.projects.some(row => row.id === project)) { setProject(""); setDocument(null); } }, [workspace.projects, project]);
  useEffect(() => {
    let cancelled = false;
    setCatalog(null); setFailed(false); setLoading(true); setDocument(null);
    void (project ? workspaceQuery<ExtensionsSnapshot>({ kind: "extensions", project }) : Promise.resolve(null))
      .then(value => { if (!cancelled) setCatalog(value); })
      .catch(() => { if (!cancelled) setFailed(true); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [project, revision]);
  // Runner paths are shared. Changing the project preview must not discard
  // configuration drafts or reload their authority fence.
  useEffect(() => {
    let cancelled = false;
    setSettingsFailed(false);
    void desktopApi.runnerSettings().then(value => { if (!cancelled) setSettings(value); }).catch(() => { if (!cancelled) setSettingsFailed(true); });
    return () => { cancelled = true; };
  }, [revision]);
  const disabled = busy || Boolean(state.current_operation);
  const refresh = () => setRevision(value => value + 1);
  const updatePaths = async (paths: RunnerSettings["paths"]) => {
    if (!settings || disabled || settingsFailed) return false;
    setBusy(true); setPathsError(null); setPathsApplied(false);
    try { onState(await desktopApi.updateRunnerSettings(settings.target, settings.paths, paths)); if (alive.current) { setPathsApplied(true); refresh(); } return true; }
    catch (value) { if (alive.current) setPathsError(normalizeDesktopError(value)); return false; }
    finally { if (alive.current) setBusy(false); }
  };
  const browsePath = async (kind: "instructions" | "skills") => {
    if (!settings || disabled || settingsFailed) return null;
    try {
      const target = await open({ title: p(kind === "instructions" ? "addInstructions" : "addSkill"), directory: kind === "skills", multiple: false });
      return typeof target === "string" && alive.current ? target : null;
    } catch (value) { if (alive.current) setPathsError(normalizeDesktopError(value)); return null; }
  };
  const restart = async () => {
    if (!settings?.can_restart || disabled) return;
    setBusy(true); setFailed(false);
    try { onState(await desktopApi.restartOwnedRunner(settings.target)); if (alive.current) { setPendingRestart(false); refresh(); } }
    catch { if (alive.current) setFailed(true); }
    finally { if (alive.current) setBusy(false); }
  };
  const plugins = catalog?.plugins.catalog?.plugins || [];
  const skills = catalog?.skills.catalog?.skills || [];
  return <div className="page-section workspace-page" data-webcodex-page="extensions">
    <header className="page-heading-row"><h1 id="extensions-title">{t("extensions.title")}</h1>{(projectTab || tab === "mcpProviders") && <button className="secondary-button" onClick={refresh} disabled={disabled || loading}>{p("refresh")}</button>}</header>
    <div className="extensions-layout">
    <div className="extensions-navigation" role="tablist" aria-orientation="vertical" aria-label={t("extensions.title")}>
      {TABS.map(value => { const Icon = TAB_ICONS[value]; return <button type="button" role="tab" key={value} id={`extension-tab-${value}`} aria-controls={`extension-view-${value}`} aria-selected={tab === value} tabIndex={tab === value ? 0 : -1} onClick={() => setTab(value)} onKeyDown={event => {
        if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
        event.preventDefault(); const next = event.key === "Home" ? TABS[0] : event.key === "End" ? TABS[TABS.length - 1] : TABS[(TABS.indexOf(value) + (["ArrowRight", "ArrowDown"].includes(event.key) ? 1 : TABS.length - 1)) % TABS.length]; setTab(next); window.document.getElementById(`extension-tab-${next}`)?.focus();
      }}><Icon size={18} aria-hidden="true" />{tabLabel(value)}</button>; })}
    </div>
    <div className="extensions-content" role="tabpanel" id={`extension-view-${tab}`} aria-labelledby={`extension-tab-${tab}`}>
    {settingsFailed && <div className="extension-apply-bar" role="alert"><span>{p("settingsUnavailable")}</span><button type="button" className="secondary-button" disabled={disabled} onClick={refresh}>{p("refresh")}</button></div>}
    <header className="extension-category-heading"><h2>{tabLabel(tab)}</h2><p>{p(TAB_PURPOSES[tab])}</p></header>
    <ManagedInstructionsPanel active={tab === "instructions"} settings={settings} disabled={disabled} onState={onState} onEnabled={refresh} />
    {settings && <div className={`extension-path-settings${tab === "skills" ? " primary" : ""}`} hidden={!pathTab}><h2>{p(tab === "skills" ? "skillPaths" : "instructionPaths")}</h2><ExtensionPathsEditor settings={settings} kind={tab === "skills" ? "skills" : "instructions"} disabled={disabled || settingsFailed} onSave={updatePaths} onBrowse={browsePath} /></div>}
    {pathsApplied && pathTab && <p className="extension-apply-bar" role="status">{instructionsText("pathsApplied")}</p>}
    {pathsError && pathTab && <div className="error-card" role="alert"><strong>{pathsError.message}</strong><span>{pathsError.next_action}</span><code>{pathsError.code}</code></div>}
    {failed && projectTab && <p role="alert" className="workspace-notice">{p("loadError")}</p>}
    {pendingRestart && tab === "nativePlugins" && <div className="extension-apply-bar" role="status"><span>{p("needsRestart")}</span>{settings?.can_restart && <button className="secondary-button" onClick={() => void restart()} disabled={disabled}>{p("restartRunner")}</button>}</div>}
    {loading && projectTab && <p role="status">{p("loading")}</p>}
    {tab === "nativePlugins" && <div className="configured-plugins">
      {settings && <PluginRegistrationForm disabled={disabled || settingsFailed} onAdd={async provider => {
        if (disabled || settingsFailed) return false; setBusy(true); setFailed(false);
        try { onState(await desktopApi.addRunnerPlugin(settings.target, provider)); if (alive.current) { setPendingRestart(true); refresh(); } return true; }
        catch { if (alive.current) setFailed(true); return false; } finally { if (alive.current) setBusy(false); }
      }} />}
      <h3>{p("configuredPlugins")}</h3><p className="field-help">{p("configuredPluginsHelp")}</p>
      {settings?.plugin_ids.map(id => <article className="extension-row" key={id}><strong>{id}</strong><span>{p("registered")}</span></article>)}
      {settings && !settings.plugin_ids.length && <p className="workspace-empty">{t("extensions.noPlugins")}</p>}
    </div>}
    <section className={projectTab ? "extension-project-preview" : undefined}>
      {projectTab && <><h2>{p(tab === "nativePlugins" ? "projectRunnerPlugins" : "projectExtensions")}</h2><div className="activity-project-filter"><span className="filter-label">{p("projects")}</span><ProjectPicker label={p("projects")} emptyLabel={p("noMatches")} searchLabel={p("search")} value={project} onChange={setProject} disabled={disabled} options={workspace.projects.filter(row => row.id).map(row => ({ value: row.id, label: projectName(row), detail: displayProjectPath(row.path) }))} /></div></>}
      {projectTab && !project && <WorkspaceEmptyState kind={tab === "instructions" ? "document" : "skill"} message={p("selectProjectPreview")} />}
      {tab === "codingAgents" && <CodingAgentsPanel state={state} onState={onState} settings={settings} onRestarted={() => { setPendingRestart(false); refresh(); }} />}
      {tab === "sshResources" && <SshResourcesPanel state={state} onState={onState} settings={settings} onRestarted={() => { setPendingRestart(false); refresh(); }} />}
      {tab === "instructions" && <>
        {catalog?.instructions.files.map(file => <article className="extension-row" key={`${file.source_scope}:${file.path}`}><div><strong>{file.source_scope === "runner" ? p("globalInstructions") : file.path.split(/[\\/]/).pop()}</strong><span>{file.source_scope === "runner" ? "Runner" : projectName(workspace.projects.find(row => row.id === project) || { id: project })} · {p("available")}</span><details><summary>{p("details")}</summary><code>{file.path}</code></details></div><button className="secondary-button" onClick={() => setDocument(file)} aria-label={`${p("open")} ${file.path.split(/[\\/]/).pop()}`}>{p("open")}</button></article>)}
        {catalog?.instructions.scan_complete && !catalog.instructions.files.length && <WorkspaceEmptyState kind="document" message={p("noInstructions")} />}
        {catalog && !catalog.instructions.scan_complete && <p className="workspace-notice">{p("unavailable")}</p>}
      </>}
      {tab === "skills" && <>
        {skills.map(skill => <article className="extension-row" key={skill.skill_id}><div><strong>{skill.name}</strong><p>{skill.description}</p><span>{skill.source_scope === "project" ? projectName(workspace.projects.find(row => row.id === project) || { id: project }) : "Runner"} · {p("available")}</span></div><details><summary>{p("details")}</summary><code>{skill.trust}</code></details></article>)}
        {catalog?.skills.available && !skills.length && <WorkspaceEmptyState kind="skill" message={p("noProjectSkills")} />}
        {catalog && !catalog.skills.available && <p className="workspace-notice">{p("unavailable")}</p>}
        {catalog?.skills.catalog?.truncated && <p>{p("partial")}</p>}
      </>}
      {tab === "mcpProviders" && <McpProvidersPanel state={state} onState={onState} settings={settings} onRestarted={() => { setPendingRestart(false); refresh(); }} />}
      {tab === "nativePlugins" && <>
        {plugins.map(plugin => <article className="extension-row native-plugin-row" key={plugin.plugin} data-plugin-id={plugin.plugin}><div><strong>{plugin.name || plugin.plugin}</strong><span>{p(plugin.status === "ready" ? "available" : plugin.status === "failed" ? "pluginLoadFailed" : "unavailable")}</span>
          {plugin.status === "failed" && <p>{p("pluginFailureHelp")}</p>}
          {plugin.errorCode && <details><summary>{p("details")}</summary><code>{plugin.errorCode}</code></details>}
        </div>
          {catalog?.can_reload_plugins && <button className="secondary-button" disabled={disabled} onClick={async () => {
            const id = plugin.plugin; if (!id || disabled) return; setBusy(true); setFailed(false);
            try { await workspaceQuery({ kind: "plugin_reload", project, plugin: id }); if (alive.current) refresh(); }
            catch { if (alive.current) setFailed(true); } finally { if (alive.current) setBusy(false); }
          }}>{p("reload")}</button>}
        </article>)}
        {catalog?.plugins.available && !plugins.length && <p className="workspace-empty">{p("noRunnerPlugins")}</p>}
        {catalog && !catalog.plugins.available && <p className="workspace-notice">{p("unavailable")}</p>}
        {catalog?.plugins.catalog?.truncated && <p>{p("partial")}</p>}
      </>}
    </section>
    </div>
    </div>
    {document && <InstructionDocument key={document.fingerprint} project={project} file={document} onClose={() => setDocument(null)} />}
  </div>;
}
function InstructionDocument({ project, file, onClose }: { project: string; file: InstructionSummary; onClose: () => void }) {
  const p = useProduct(); const [content, setContent] = useState<string | null>(null); const [failed, setFailed] = useState(false);
  useEffect(() => { let cancelled = false; void workspaceQuery<{ content: string }>({ kind: "instruction", project, source_scope: file.source_scope, path: file.path, fingerprint: file.fingerprint }).then(value => { if (!cancelled) setContent(value.content); }).catch(() => { if (!cancelled) setFailed(true); }); return () => { cancelled = true; }; }, [project, file]);
  return <WorkspaceDialog title={file.source_scope === "runner" ? p("globalInstructions") : file.path} onClose={onClose}>{failed ? <p role="alert">{p("loadError")}</p> : content === null ? <p role="status">{p("loading")}</p> : <pre className="instruction-content">{content}</pre>}{file.truncated && <p>{p("partial")}</p>}</WorkspaceDialog>;
}
