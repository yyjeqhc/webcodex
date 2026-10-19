import { useEffect, useMemo, useRef, useState } from "react";
import { Button, NativeSelect, TextInput } from "@mantine/core";
import { Search } from "lucide-react";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopState } from "../../models/topology";
import type { UnregisterObservation, WorkspaceProject } from "../../models/workspace";
import { useProduct } from "../../i18n/product";
import { displayProjectPath, projectName, runnerInventoryComplete, useWorkspace } from "../workspace/WorkspaceContext";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { ProjectRows } from "./ProjectRows";
import { RunnerDevices } from "./RunnerDevices";
import { projectRunnerId } from "./project-device";
import { AddLocalProject } from "./AddLocalProject";
import { AddDevice } from "./AddDevice";

export function ProjectsPanel({ onComputerSettings, onState }: { onComputerSettings?: () => void; onState?: (state: DesktopState) => void }) {
  const p = useProduct();
  const workspace = useWorkspace();
  const [query, setQuery] = useState("");
  const [deviceSelection, setDeviceSelection] = useState<{ context: string; id: string } | null>(null);
  const authorizationLost = workspace.error && ["authenticationRequired", "permissionDenied"].includes(workspace.errorReason);
  const selectedDevice = !authorizationLost && deviceSelection?.context === workspace.contextKey ? deviceSelection.id : "";
  const observedDeviceIds = useMemo(() => Array.from(new Set([
    ...workspace.runners.map(runner => runner.client_id),
    ...workspace.projects.map(projectRunnerId).filter((id): id is string => Boolean(id)),
  ])).sort(), [workspace.runners, workspace.projects]);
  const inventoryComplete = !workspace.loading && !workspace.fleetStale && runnerInventoryComplete(workspace.runner);
  // Derive the displayed filter from the same inventory as the results, before
  // the cleanup effect. A removed option must never make the select show All
  // while the rows still use the previous device.
  const device = inventoryComplete && !observedDeviceIds.includes(selectedDevice) ? "" : selectedDevice;
  // An incomplete refresh cannot revoke a prior observation. Keep the selected
  // option alongside the filter so the select never silently displays All.
  const deviceIds = device && !inventoryComplete && !observedDeviceIds.includes(device)
    ? [...observedDeviceIds, device].sort() : observedDeviceIds;
  useEffect(() => {
    if (!deviceSelection) return;
    if (deviceSelection.context !== workspace.contextKey
      || authorizationLost
      || (inventoryComplete && !observedDeviceIds.includes(deviceSelection.id))) {
      setDeviceSelection(null);
    }
  }, [deviceSelection, workspace.contextKey, authorizationLost, inventoryComplete, observedDeviceIds]);
  const selectDevice = (id: string) => setDeviceSelection(id ? { context: workspace.contextKey, id } : null);
  const fromDeviceCard = (id: string) => {
    selectDevice(id);
    const filter = document.getElementById("projects-device-filter");
    filter?.focus();
    filter?.scrollIntoView?.({ block: "nearest" });
  };
  const [removal, setRemoval] = useState<UnregisterObservation | null>(null);
  const [removing, setRemoving] = useState(false);
  const [removeError, setRemoveError] = useState(false);
  const [selected, setSelected] = useState<string[]>([]);
  const [batchConfirm, setBatchConfirm] = useState<UnregisterObservation[] | null>(null);
  const [batchProgress, setBatchProgress] = useState("");
  const [batchIssues, setBatchIssues] = useState<string[]>([]);
  const removalOwner = useRef<{ context: string; token: number } | null>(null);
  const removalToken = useRef(0);
  if (removalOwner.current?.context !== workspace.contextKey) removalOwner.current = null;
  useEffect(() => () => { removalOwner.current = null; }, []);
  const beginRemoval = () => {
    if (removalOwner.current) return null;
    const owner = { context: workspace.contextKey, token: ++removalToken.current };
    removalOwner.current = owner;
    setRemoving(true);
    return owner;
  };
  const finishRemoval = (owner: { context: string; token: number }) => {
    if (removalOwner.current !== owner) return;
    removalOwner.current = null;
    setRemoving(false);
  };
  useEffect(() => {
    setSelected([]);
    setBatchProgress("");
    setRemoving(false);
    setRemoval(null);
    setBatchConfirm(null);
    setBatchIssues([]);
    setRemoveError(false);
  }, [workspace.contextKey]);
  const canManageProjects = Boolean(onState && workspace.state.workspace_runner);

  const prepareRemoval = async (project: WorkspaceProject) => {
    if (!canManageProjects || removing || workspace.state.current_operation || !project.id) return;
    const owner = beginRemoval();
    if (!owner) return;
    setRemoveError(false);
    try {
      const observation = await desktopApi.prepareProjectUnregister(project.id);
      if (removalOwner.current === owner) setRemoval(observation);
    } catch {
      if (removalOwner.current === owner) setRemoveError(true);
    } finally {
      finishRemoval(owner);
    }
  };

  const unregister = async () => {
    if (!removal || !onState || removing || workspace.state.current_operation) return;
    const owner = beginRemoval();
    if (!owner) return;
    setRemoveError(false);
    try {
      const state = await desktopApi.unregisterProject(removal);
      if (removalOwner.current === owner) {
        onState(state);
        workspace.removeProject(removal.project);
      }
    } catch {
      if (removalOwner.current === owner) setRemoveError(true);
    } finally {
      if (removalOwner.current === owner) {
        setRemoval(null);
        workspace.refresh();
      }
      finishRemoval(owner);
    }
  };

  const prepareSelectedRemoval = async () => {
    if (!onState || removing || workspace.state.current_operation) return;
    const ids = selected.filter(id => workspace.projects.some(project => project.id === id && project.connected && projectRunnerId(project) === workspace.state.workspace_runner?.client_id));
    if (!ids.length) return;
    const owner = beginRemoval();
    if (!owner) return;
    setRemoveError(false);
    setBatchIssues([]);
    setBatchConfirm(null);
    try {
      const observations: UnregisterObservation[] = [];
      for (let offset = 0; offset < ids.length; offset += 16) {
        const chunk = ids.slice(offset, offset + 16);
        const prepared = await desktopApi.prepareProjectsUnregister(chunk);
        if (removalOwner.current !== owner) return;
        if (prepared.length !== chunk.length || prepared.some((item, index) => item.project !== chunk[index])) {
          throw new Error("Unregister observations did not match the selected projects");
        }
        observations.push(...prepared);
      }
      // The confirmation owns these exact paths and revisions. Never replace
      // them with a fresh inventory after the user has confirmed.
      setBatchConfirm(observations);
    } catch {
      if (removalOwner.current === owner) setRemoveError(true);
    } finally {
      finishRemoval(owner);
    }
  };

  const unregisterSelected = async () => {
    if (!onState || !batchConfirm?.length || removing || workspace.state.current_operation) return;
    const observations = batchConfirm;
    const owner = beginRemoval();
    if (!owner) return;
    setRemoveError(false);
    setBatchIssues([]);
    setBatchConfirm(null);
    try {
      let completed = 0;
      for (let offset = 0; offset < observations.length; offset += 16) {
        if (removalOwner.current !== owner) return;
        const chunk = observations.slice(offset, offset + 16);
        setBatchProgress(`${completed}/${observations.length}`);
        if (workspace.state.persistent_environment) {
          for (const observation of chunk) {
            try {
              const next = await desktopApi.unregisterProject(observation);
              if (removalOwner.current !== owner) return;
              onState(next);
              workspace.removeProject(observation.project);
              setSelected(current => current.filter(id => id !== observation.project));
              completed++;
              setBatchProgress(`${completed}/${observations.length}`);
            } catch {
              if (removalOwner.current === owner) {
                setRemoveError(true);
                setBatchIssues([observation.project]);
              }
              return;
            }
          }
        } else {
          const result = await desktopApi.unregisterProjects(chunk);
          if (removalOwner.current !== owner) return;
          onState(result.state);
          const succeeded = result.items.filter(item => item.success).map(item => item.project);
          for (const id of succeeded) workspace.removeProject(id);
          setSelected(current => current.filter(id => !succeeded.includes(id)));
          completed += succeeded.length;
          setBatchProgress(`${completed}/${observations.length}`);
          const issues = result.items.filter(item => !item.success).map(item => `${item.project}: ${item.status}`);
          if (issues.length || result.items.length !== chunk.length) {
            setBatchIssues(issues.length ? issues : chunk.map(item => item.project));
            setRemoveError(true);
            return;
          }
        }
      }
    } catch {
      if (removalOwner.current === owner) setRemoveError(true);
    } finally {
      if (removalOwner.current === owner) {
        setBatchProgress("");
        workspace.refresh();
      }
      finishRemoval(owner);
    }
  };

  const rows = workspace.projects.filter(project =>
    (!device || projectRunnerId(project) === device) &&
    `${projectName(project)} ${displayProjectPath(project.path)} ${projectRunnerId(project) || ""}`
      .toLocaleLowerCase()
      .includes(query.trim().toLocaleLowerCase()),
  );
  const selectable = rows.filter(project => project.id && project.connected && projectRunnerId(project) === workspace.state.workspace_runner?.client_id).map(project => project.id);
  const allSelectableSelected = selectable.length > 0 && selectable.every(id => selected.includes(id));
  return <section className="page-section workspace-page" aria-labelledby="projects-title" data-webcodex-page="projects">
    <header className="page-heading-row"><h1 id="projects-title">{p("projects")} <span className="heading-count">{workspace.projects.length}</span></h1>
      <div className="project-heading-actions">
        {onState && <AddLocalProject state={workspace.state} onState={onState} onAdded={workspace.refresh} />}
        <button type="button" className="secondary-button" onClick={workspace.refresh} disabled={workspace.busy}>{p("refresh")}</button>
      </div>
    </header>
    <div className="workspace-project-filters">
      <div className="workspace-search"><TextInput id="projects-search" label={p("search")} type="search" leftSection={<Search size={16} aria-hidden="true" />} value={query} onChange={event => setQuery(event.currentTarget.value)} /></div>
      <div className="workspace-search"><NativeSelect size="md" classNames={{ input: "ui-mantine-input", label: "ui-mantine-label" }} id="projects-device-filter" label={p("executionDevice")} value={device} onChange={event => selectDevice(event.currentTarget.value)} data={[
        { value: "", label: p("allDevices") },
        ...deviceIds.map(id => ({ value: id, label: id === workspace.state.workspace_runner?.client_id ? `${p("thisComputer")} · ${id}` : id })),
      ]} /></div>
    </div>
    {workspace.error && <div className="workspace-notice" role="alert">{p(workspace.errorReason)}</div>}
    <div className="project-filter-summary"><span role="status">{p("projectFilterCount").replace("{shown}", String(rows.length)).replace("{total}", String(workspace.projects.length))}</span>
      <div className="project-selection-actions">
        {(device || query) && <button type="button" className="text-button" onClick={() => { selectDevice(""); setQuery(""); }}>{p("clearProjectFilters")}</button>}
        {canManageProjects && <>
        <Button variant="subtle" size="sm" disabled={removing || !selectable.length} onClick={() => {
          setSelected(current => allSelectableSelected ? current.filter(id => !selectable.includes(id)) : Array.from(new Set([...current, ...selectable])));
        }}>{p(allSelectableSelected ? "deselectVisibleProjects" : "selectVisibleProjects")}</Button>
        {selected.some(id => workspace.projects.some(project => project.id === id)) && <Button color="red.8" size="sm" disabled={removing} onClick={() => void prepareSelectedRemoval()}>{p("removeSelectedProjects").replace("{count}", String(selected.length))}</Button>}
        {batchProgress && <span role="status">{batchProgress}</span>}
        </>}
      </div>
    </div>
    <ProjectRows projects={rows} onUnregister={canManageProjects ? prepareRemoval : undefined} onSelect={canManageProjects ? (id, checked) => setSelected(current => checked ? Array.from(new Set([...current, id])) : current.filter(value => value !== id)) : undefined} selected={selected} busy={removing || Boolean(workspace.state.current_operation)} />
    {removeError && <p role="alert" className="workspace-notice">{p("unregisterError")}</p>}
    {batchIssues.length > 0 && <ul aria-label={p("failedProjects")} className="workspace-notice">{batchIssues.map(issue => <li key={issue}>{issue}</li>)}</ul>}
    {batchConfirm && <WorkspaceDialog title={p("removeSelectedProjects").replace("{count}", String(batchConfirm.length))} onClose={() => setBatchConfirm(null)} busy={removing}>
      <p>{p("unregisterDescription")}</p>
      <ul>{batchConfirm.map(observation => <li key={observation.project}><strong>{projectName({ id: observation.project, path: observation.path })}</strong> <code>{displayProjectPath(observation.path)}</code></li>)}</ul>
      <div className="shell-actions">
        <Button variant="default" onClick={() => setBatchConfirm(null)} disabled={removing}>{p("cancel")}</Button>
        <Button color="red.8" onClick={() => void unregisterSelected()} disabled={removing || Boolean(workspace.state.current_operation)}>{p("removeSelectedProjects").replace("{count}", String(batchConfirm.length))}</Button>
      </div>
    </WorkspaceDialog>}
    {removal && <WorkspaceDialog title={p("unregisterProject")} onClose={() => setRemoval(null)} busy={removing}>
      <p>{p("unregisterDescription")}</p>
      <code>{displayProjectPath(removal.path)}</code>
      <div className="shell-actions">
        <Button variant="default" onClick={() => setRemoval(null)} disabled={removing}>{p("cancel")}</Button>
        <Button color="red.8" onClick={() => void unregister()} disabled={removing || Boolean(workspace.state.current_operation)}>{p("unregisterProject")}</Button>
      </div>
    </WorkspaceDialog>}
    {!rows.length && !workspace.error && !workspace.loading && <p className="workspace-empty">{p(query || device ? "noMatches" : "noProjects")}</p>}    {workspace.runner?.projects_truncated && <p className="workspace-notice">{p("partial")}</p>}
    {workspace.runner && <details className="project-access-help"><summary>{p("projectAccessHelp")}</summary><p>{p("projectAccessScope")}</p></details>}
    <AddDevice state={workspace.state} onViewDevices={workspace.refresh} />
    <RunnerDevices onComputerSettings={onComputerSettings} selectedDevice={device} onSelectDevice={fromDeviceCard} />
  </section>;
}
