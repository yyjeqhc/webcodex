import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useProduct } from "../../i18n/product";
import { normalizeDesktopError } from "../../i18n/presentation";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState, PluginRegistration, RunnerSettings } from "../../models/topology";
import type { ExtensionsSnapshot, InstructionSummary } from "../../models/workspace";
import { useWorkspace, workspaceQuery } from "../workspace/WorkspaceContext";
import type { ExtensionPathKind } from "./extension-panel-types";

// One owner for observations, mutation guards and settings fences. Switching a
// renderer never creates a second settings controller or implicitly replays work.
export function useExtensionsState(state: DesktopState, onState: (state: DesktopState) => void) {
  const p = useProduct();
  const workspace = useWorkspace();
  const [project, setProject] = useState(state.project?.runtime_project_id || "");
  const [catalog, setCatalog] = useState<ExtensionsSnapshot | null>(null);
  const [settings, setSettings] = useState<RunnerSettings | null>(null);
  const [settingsFailed, setSettingsFailed] = useState(false);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [pendingRestart, setPendingRestart] = useState(false);
  const [pathsApplied, setPathsApplied] = useState(false);
  const [pathsError, setPathsError] = useState<DesktopError | null>(null);
  const [revision, setRevision] = useState(0);
  const [document, setDocument] = useState<InstructionSummary | null>(null);
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => {
    if (project && !workspace.projects.some(row => row.id === project)) { setProject(""); setDocument(null); }
  }, [workspace.projects, project]);
  useEffect(() => {
    let cancelled = false;
    setCatalog(null); setFailed(false); setLoading(true); setDocument(null);
    void (project ? workspaceQuery<ExtensionsSnapshot>({ kind: "extensions", project }) : Promise.resolve(null))
      .then(value => { if (!cancelled) setCatalog(value); })
      .catch(() => { if (!cancelled) setFailed(true); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [project, revision]);
  // Runner paths are shared. Project preview changes must not discard drafts
  // or reload the exact Runner settings fence.
  useEffect(() => {
    let cancelled = false;
    setSettingsFailed(false);
    void desktopApi.runnerSettings().then(value => { if (!cancelled) setSettings(value); }).catch(() => { if (!cancelled) setSettingsFailed(true); });
    return () => { cancelled = true; };
  }, [revision]);
  const disabled = busy || Boolean(state.current_operation);
  const refresh = () => setRevision(value => value + 1);
  const onRestarted = () => { setPendingRestart(false); refresh(); };
  const updatePaths = async (paths: RunnerSettings["paths"]) => {
    if (!settings || disabled || settingsFailed) return false;
    setBusy(true); setPathsError(null); setPathsApplied(false);
    try { onState(await desktopApi.updateRunnerSettings(settings.target, settings.paths, paths)); if (alive.current) { setPathsApplied(true); refresh(); } return true; }
    catch (value) { if (alive.current) setPathsError(normalizeDesktopError(value)); return false; }
    finally { if (alive.current) setBusy(false); }
  };
  const browsePath = async (kind: ExtensionPathKind) => {
    if (!settings || disabled || settingsFailed) return null;
    try {
      const target = await open({ title: p(kind === "instructions" ? "addInstructions" : "addSkill"), directory: kind === "skills", multiple: false });
      return typeof target === "string" && alive.current ? target : null;
    } catch (value) { if (alive.current) setPathsError(normalizeDesktopError(value)); return null; }
  };
  const restart = async () => {
    if (!settings?.can_restart || disabled) return;
    setBusy(true); setFailed(false);
    try { onState(await desktopApi.restartOwnedRunner(settings.target)); if (alive.current) onRestarted(); }
    catch { if (alive.current) setFailed(true); }
    finally { if (alive.current) setBusy(false); }
  };
  const addPlugin = async (provider: PluginRegistration) => {
    if (!settings || disabled || settingsFailed) return false;
    setBusy(true); setFailed(false);
    try { onState(await desktopApi.addRunnerPlugin(settings.target, provider)); if (alive.current) { setPendingRestart(true); refresh(); } return true; }
    catch { if (alive.current) setFailed(true); return false; }
    finally { if (alive.current) setBusy(false); }
  };
  const reloadPlugin = async (id: string) => {
    if (!id || disabled || !project || !catalog?.can_reload_plugins) return;
    setBusy(true); setFailed(false);
    try { await workspaceQuery({ kind: "plugin_reload", project, plugin: id }); if (alive.current) refresh(); }
    catch { if (alive.current) setFailed(true); }
    finally { if (alive.current) setBusy(false); }
  };
  return {
    projects: workspace.projects, project, setProject, catalog, settings, settingsFailed,
    loading, failed, disabled, pendingRestart, pathsApplied, pathsError, document, setDocument,
    refresh, onRestarted, updatePaths, browsePath, restart, addPlugin, reloadPlugin,
  };
}
