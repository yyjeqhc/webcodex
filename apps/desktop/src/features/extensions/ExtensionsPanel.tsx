import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { PluginRegistrationForm } from "./PluginRegistrationForm";
import { McpProvidersPanel } from "./McpProvidersPanel";
import { CodingAgentsPanel } from "./CodingAgentsPanel";
import { SshResourcesPanel } from "./SshResourcesPanel";
import { useRunnerCapabilitiesText } from "../../i18n/runner-capabilities";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { ExtensionPathsEditor } from "./ExtensionPathsEditor";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState, RunnerSettings } from "../../models/topology";
import type { ExtensionsSnapshot, InstructionSummary } from "../../models/workspace";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { projectName, useWorkspace, workspaceQuery } from "../workspace/WorkspaceContext";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

type ExtensionTab = "codingAgents" | "sshResources" | "instructions" | "skills" | "mcpProviders";
const TABS: ExtensionTab[] = ["codingAgents", "sshResources", "mcpProviders", "skills", "instructions"];
export function ExtensionsPanel({ state, onState }: { state: DesktopState; onState: (state: DesktopState) => void }) {
  const { t } = useLocale(); const p = useProduct(); const c = useConnectionsTools(); const r = useRunnerCapabilitiesText(); const workspace = useWorkspace();
  const [tab, setTab] = useState<ExtensionTab>("codingAgents");
  const projectTab = tab === "instructions" || tab === "skills";
  const [project, setProject] = useState(state.project?.runtime_project_id || "");
  const [catalog, setCatalog] = useState<ExtensionsSnapshot | null>(null);
  const [settings, setSettings] = useState<RunnerSettings | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [pendingRestart, setPendingRestart] = useState(false);
  const [manage, setManage] = useState(false);
  const [revision, setRevision] = useState(0);
  const [document, setDocument] = useState<InstructionSummary | null>(null);
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => { setProject(state.project?.runtime_project_id || ""); }, [state.project?.runtime_project_id]);
  useEffect(() => {
    let cancelled = false;
    setCatalog(null); setSettings(null); setFailed(false); setLoading(true); setDocument(null);
    void Promise.allSettled([
      project ? workspaceQuery<ExtensionsSnapshot>({ kind: "extensions", project }) : Promise.resolve(null),
      desktopApi.runnerSettings(),
    ]).then(([catalogResult, settingsResult]) => {
      if (cancelled) return;
      if (catalogResult.status === "fulfilled") setCatalog(catalogResult.value); else setFailed(true);
      if (settingsResult.status === "fulfilled") setSettings(settingsResult.value);
      setLoading(false);
    });
    return () => { cancelled = true; };
  }, [project, revision]);
  const disabled = busy || Boolean(state.current_operation);
  const refresh = () => setRevision(value => value + 1);
  const updatePaths = async (paths: RunnerSettings["paths"]) => {
    if (!settings || disabled) return false;
    setBusy(true); setFailed(false);
    try { onState(await desktopApi.updateRunnerSettings(settings.target, settings.paths, paths)); if (alive.current) { setPendingRestart(true); refresh(); } return true; }
    catch { if (alive.current) setFailed(true); return false; }
    finally { if (alive.current) setBusy(false); }
  };
  const addFile = async (kind: "instructions" | "skills") => {
    if (!settings || disabled) return;
    try {
      const target = await open({ title: p(kind === "instructions" ? "addInstructions" : "addSkill"), directory: kind === "skills", multiple: false });
      if (!target || typeof target !== "string" || !alive.current) return;
      const key = kind === "instructions" ? "instruction_files" : "skill_roots";
      if (settings.paths[key].includes(target)) return;
      await updatePaths({ ...settings.paths, [key]: [...settings.paths[key], target] });
    } catch { if (alive.current) setFailed(true); }
  };
  const restart = async () => {
    if (!settings?.can_restart || disabled) return;
    setBusy(true); setFailed(false);
    try { onState(await desktopApi.restartOwnedRunner(settings.target)); if (alive.current) { setPendingRestart(false); refresh(); } }
    catch { if (alive.current) setFailed(true); }
    finally { if (alive.current) setBusy(false); }
  };
  const plugins = catalog?.plugins.catalog?.plugins || catalog?.plugins.catalog?.providers || [];
  const skills = catalog?.skills.catalog?.skills || [];
  return <div className="page-section workspace-page" data-webcodex-page="extensions">
    <header className="page-heading-row"><h1 id="extensions-title">{t("extensions.title")}</h1>{(projectTab || tab === "mcpProviders") && <button className="secondary-button" onClick={refresh} disabled={disabled || loading}>{p("refresh")}</button>}</header>
    {projectTab && <div className="activity-project-filter"><label htmlFor="extensions-project">{p("projects")}</label><select id="extensions-project" value={project} onChange={event => setProject(event.target.value)} disabled={disabled}>{workspace.projects.filter(row => row.id).map(row => <option key={row.id} value={row.id}>{projectName(row)}</option>)}</select></div>}
    <div className="workspace-tabs" role="tablist" aria-label={t("extensions.title")}>
      {TABS.map(value => <button type="button" role="tab" key={value} id={`extension-tab-${value}`} aria-controls={`extension-view-${value}`} aria-selected={tab === value} tabIndex={tab === value ? 0 : -1} onClick={() => setTab(value)} onKeyDown={event => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault(); const next = TABS[(TABS.indexOf(value) + (event.key === "ArrowRight" ? 1 : TABS.length - 1)) % TABS.length]; setTab(next); window.document.getElementById(`extension-tab-${next}`)?.focus();
      }}>{value === "instructions" ? p("instructions") : value === "skills" ? "Skills" : value === "mcpProviders" ? c("mcpProviders") : r(value)}</button>)}
    </div>
    {failed && projectTab && <p role="alert" className="workspace-notice">{p("loadError")}</p>}
    {pendingRestart && (projectTab || tab === "mcpProviders") && <div className="extension-apply-bar" role="status"><span>{p("needsRestart")}</span>{settings?.can_restart && <button className="secondary-button" onClick={() => void restart()} disabled={disabled}>{p("restartRunner")}</button>}</div>}
    {loading && projectTab && <p role="status">{p("loading")}</p>}
    {(!loading || !projectTab) && <section role="tabpanel" id={`extension-view-${tab}`} aria-labelledby={`extension-tab-${tab}`}>
      {tab === "codingAgents" && <CodingAgentsPanel state={state} onState={onState} settings={settings} onRestarted={() => { setPendingRestart(false); refresh(); }} />}
      {tab === "sshResources" && <SshResourcesPanel state={state} onState={onState} settings={settings} onRestarted={() => { setPendingRestart(false); refresh(); }} />}
      {tab === "instructions" && <>
        <div className="extension-toolbar"><button type="button" className="secondary-button" disabled={!settings || disabled} onClick={() => void addFile("instructions")}>{p("addInstructions")}</button></div>
        {catalog?.instructions.files.map(file => <article className="extension-row" key={`${file.source_scope}:${file.path}`}><div><strong>{file.source_scope === "runner" ? p("globalInstructions") : file.path.split(/[\\/]/).pop()}</strong><span>{file.source_scope === "runner" ? "Runner" : projectName(workspace.projects.find(row => row.id === project) || { id: project })} · {p("available")}</span><details><summary>{p("details")}</summary><code>{file.path}</code></details></div><button className="secondary-button" onClick={() => setDocument(file)} aria-label={`${p("open")} ${file.path.split(/[\\/]/).pop()}`}>{p("open")}</button></article>)}
        {catalog?.instructions.scan_complete && !catalog.instructions.files.length && <p className="workspace-empty">{p("noInstructions")}</p>}
        {catalog && !catalog.instructions.scan_complete && <p className="workspace-notice">{p("unavailable")}</p>}
      </>}
      {tab === "skills" && <>
        <div className="extension-toolbar"><button type="button" className="secondary-button" onClick={() => void addFile("skills")} disabled={!settings || disabled}>{p("addSkill")}</button></div>
        {skills.map(skill => <article className="extension-row" key={skill.skill_id}><div><strong>{skill.name}</strong><p>{skill.description}</p><span>{skill.source_scope === "project" ? projectName(workspace.projects.find(row => row.id === project) || { id: project }) : "Runner"} · {p("available")}</span></div><details><summary>{p("details")}</summary><code>{skill.trust}</code></details></article>)}
        {catalog?.skills.available && !skills.length && <p className="workspace-empty">{p("noExtensions")}</p>}
        {catalog && !catalog.skills.available && <p className="workspace-notice">{p("unavailable")}</p>}
        {catalog?.skills.catalog?.truncated && <p>{p("partial")}</p>}
      </>}
      {tab === "mcpProviders" && <>
        <McpProvidersPanel state={state} onState={onState} settings={settings} onRestarted={() => { setPendingRestart(false); refresh(); }} />
        <details className="workspace-technical"><summary>{c("nativePlugins")}</summary>
        {plugins.map(plugin => <article className="extension-row" key={plugin.id || plugin.plugin}><div><strong>{plugin.name || plugin.id || plugin.plugin}</strong><span>{plugin.status === "error" ? p("unavailable") : plugin.status === "ready" || plugin.status === "available" ? p("available") : p("registered")} · {plugin.tool_count ?? plugin.tools?.length ?? "—"} {p("tools")}</span></div>
          {catalog?.can_reload_plugins && <button className="secondary-button" disabled={disabled} onClick={async () => {
            const id = plugin.id || plugin.plugin; if (!id || disabled) return; setBusy(true); setFailed(false);
            try { await workspaceQuery({ kind: "plugin_reload", project, plugin: id }); if (alive.current) refresh(); }
            catch { if (alive.current) setFailed(true); } finally { if (alive.current) setBusy(false); }
          }}>{p("reload")}</button>}
        </article>)}
        {catalog?.plugins.available && !plugins.length && <p className="workspace-empty">{p("noExtensions")}</p>}
        {catalog && !catalog.plugins.available && <p className="workspace-notice">{p("unavailable")}</p>}
        {settings && <PluginRegistrationForm disabled={disabled} onAdd={async provider => {
          if (disabled) return false; setBusy(true); setFailed(false);
          try { onState(await desktopApi.addRunnerPlugin(settings.target, provider)); if (alive.current) { setPendingRestart(true); refresh(); } return true; }
          catch { if (alive.current) setFailed(true); return false; } finally { if (alive.current) setBusy(false); }
        }} />}
        </details>
      </>}
      {settings && projectTab && <div className="workspace-technical"><button className="text-button" onClick={() => setManage(value => !value)} aria-expanded={manage}>{p("manage")}</button>{manage && <ExtensionPathsEditor settings={settings} disabled={disabled} onSave={updatePaths} />}</div>}
    </section>}
    {document && <InstructionDocument key={document.fingerprint} project={project} file={document} onClose={() => setDocument(null)} />}
  </div>;
}
function InstructionDocument({ project, file, onClose }: { project: string; file: InstructionSummary; onClose: () => void }) {
  const p = useProduct(); const [content, setContent] = useState<string | null>(null); const [failed, setFailed] = useState(false);
  useEffect(() => { let cancelled = false; void workspaceQuery<{ content: string }>({ kind: "instruction", project, source_scope: file.source_scope, path: file.path, fingerprint: file.fingerprint }).then(value => { if (!cancelled) setContent(value.content); }).catch(() => { if (!cancelled) setFailed(true); }); return () => { cancelled = true; }; }, [project, file]);
  return <WorkspaceDialog title={file.source_scope === "runner" ? p("globalInstructions") : file.path} onClose={onClose}>{failed ? <p role="alert">{p("loadError")}</p> : content === null ? <p role="status">{p("loading")}</p> : <pre className="instruction-content">{content}</pre>}{file.truncated && <p>{p("partial")}</p>}</WorkspaceDialog>;
}
