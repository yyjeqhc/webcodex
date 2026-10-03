import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import { useProduct } from "../../i18n/product";
import type { DesktopState, ProjectSelection } from "../../models/topology";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

/** Optional manual registration; ordinary model-driven discovery is unchanged. */
export function AddLocalProject({ state, onState, onAdded }: {
  state: DesktopState; onState: (state: DesktopState) => void; onAdded: () => void;
}) {
  const p = useProduct();
  const [project, setProject] = useState<ProjectSelection | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const inFlight = useRef(false);
  const alive = useRef(true);
  const target = JSON.stringify([state.topology, state.persistent_environment, state.workspace_runner]);
  const currentTarget = useRef(target);
  currentTarget.current = target;
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => { setProject(null); setFailed(false); }, [target]);
  const local = state.topology?.experience === "full" && state.topology.runner.kind === "local"
    && (Boolean(state.persistent_environment) || state.topology.server.kind === "local");
  const disabled = busy || Boolean(state.current_operation) || !state.readiness.runtime_ready;
  const run = async (action: (isCurrent: () => boolean) => Promise<void>) => {
    if (!local || disabled || inFlight.current) return;
    const observed = currentTarget.current;
    const isCurrent = () => alive.current && currentTarget.current === observed;
    inFlight.current = true; setBusy(true); setFailed(false);
    try { await action(isCurrent); }
    catch { if (isCurrent()) setFailed(true); }
    finally { inFlight.current = false; if (alive.current) setBusy(false); }
  };
  if (!local) return null;
  return <div className="workspace-notice">
    <p>{p("addLocalFolderHelp")}</p>
    <button type="button" className="secondary-button" disabled={disabled}
      onClick={() => void run(async isCurrent => {
        const path = await open({ title: p("addLocalFolder"), directory: true, multiple: false });
        if (typeof path !== "string" || !isCurrent()) return;
        const inspected = await desktopApi.inspectProject(path);
        if (isCurrent()) setProject(inspected);
      })}>{p("addLocalFolder")}</button>
    {!state.readiness.runtime_ready && <p className="field-help">{p("addLocalFolderNotReady")}</p>}
    {!project && failed && <p role="alert">{p("addLocalFolderFailed")}</p>}
    {project && <WorkspaceDialog title={p("addLocalFolder")} busy={busy} onClose={() => { setProject(null); setFailed(false); }}>
      <code className="runtime-directory">{project.path}</code>
      <p>{p("addLocalFolderConsent")}</p>
      {failed && <p role="alert">{p("addLocalFolderFailed")}</p>}
      <div className="shell-actions">
        <button type="button" className="primary-button" disabled={disabled}
          onClick={() => void run(async isCurrent => {
            const next = await desktopApi.activateLocalProject(project.path);
            if (!isCurrent()) return;
            onState(next); setProject(null); onAdded();
          })}>{p("addLocalFolderConfirm")}</button>
        <button type="button" className="secondary-button" disabled={busy}
          onClick={() => { setProject(null); setFailed(false); }}>{p("cancel")}</button>
      </div>
    </WorkspaceDialog>}
  </div>;
}
