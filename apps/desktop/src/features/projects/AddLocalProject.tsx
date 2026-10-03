import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import { useProduct } from "../../i18n/product";
import type { DesktopState, ProjectInspection } from "../../models/topology";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";

/** Optional manual registration; ordinary model-driven discovery is unchanged. */
export function AddLocalProject({ state, onState, onAdded }: {
  state: DesktopState; onState: (state: DesktopState) => void; onAdded: () => void;
}) {
  const p = useProduct();
  const [inspection, setInspection] = useState<ProjectInspection | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const inFlight = useRef(false);
  const alive = useRef(true);
  const target = JSON.stringify([state.topology, state.persistent_environment, state.workspace_runner]);
  const currentTarget = useRef(target);
  currentTarget.current = target;
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => { setInspection(null); setFailed(false); }, [target]);
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
        const inspected = await desktopApi.inspectProjectAccess(path);
        if (isCurrent()) setInspection(inspected);
      })}>{p("addLocalFolder")}</button>
    {!state.readiness.runtime_ready && <p className="field-help">{p("addLocalFolderNotReady")}</p>}
    {!inspection && failed && <p role="alert">{p("addLocalFolderFailed")}</p>}
    {inspection && <WorkspaceDialog title={p("addLocalFolder")} busy={busy} onClose={() => { setInspection(null); setFailed(false); }}>
      <code className="runtime-directory">{inspection.project.path}</code>
      <p>{p(inspection.authorization_required ? "addLocalFolderConsent" : "addLocalFolderAlreadyAuthorized")}</p>
      {failed && <p role="alert">{p("addLocalFolderFailed")}</p>}
      <div className="shell-actions">
        <button type="button" className="primary-button" disabled={disabled}
          onClick={() => void run(async isCurrent => {
            const next = await desktopApi.activateLocalProject(inspection.project.path);
            if (!isCurrent()) return;
            onState(next); setInspection(null); onAdded();
          })}>{p(inspection.authorization_required ? "addLocalFolderConfirm" : "addLocalProjectConfirm")}</button>
        <button type="button" className="secondary-button" disabled={busy}
          onClick={() => { setInspection(null); setFailed(false); }}>{p("cancel")}</button>
      </div>
    </WorkspaceDialog>}
  </div>;
}
