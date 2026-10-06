import { useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import type { DesktopState } from "../../models/topology";
import { ConnectionEditor } from "../connection/ConnectionEditor";
import { ChatgptObservation, WorkspaceStatus } from "../workspace/WorkspaceStatus";

// Runtime IDs, never an inferred folder name or user-provided label.
export function firstReadTarget(state: DesktopState) {
  const runner = state.topology?.runner.kind === "local" ? state.workspace_runner?.client_id : null;
  const project = state.project?.runtime_project_id;
  return runner && project?.startsWith(`agent:${runner}:`) ? { runner, project } : null;
}
export function FirstReadGuide({ state, onProjects }: { state: DesktopState; onProjects?: () => void }) {
  const { t } = useLocale(); const p = useProduct();
  const target = firstReadTarget(state);
  const identity = JSON.stringify([state.persistent_environment, state.workspace_runner?.client_id, target?.project]);
  // An ephemeral user report never changes native observation or readiness.
  const [reportedIdentity, setReportedIdentity] = useState<string | null>(null);
  const [copiedIdentity, setCopiedIdentity] = useState<string | null>(null);
  const [copyFailed, setCopyFailed] = useState(false);
  useEffect(() => { setReportedIdentity(null); setCopiedIdentity(null); setCopyFailed(false); }, [identity]);
  const prompt = target ? t("completion.prompt", target) : null;
  const localRunner = state.topology?.runner.kind === "local";
  const busy = Boolean(state.current_operation);
  return <section className="form-card first-read-guide" aria-labelledby="first-read-title">
    <h2 id="first-read-title">{t("completion.read")}</h2>
    {target && prompt ? <>
      <dl className="completion-identities"><div><dt>{t("completion.runnerId")}</dt><dd><code>{target.runner}</code></dd></div><div><dt>{t("completion.projectId")}</dt><dd><code>{target.project}</code></dd></div></dl>
      <p className="completion-prompt">{prompt}</p>
      <button type="button" className="secondary-button" disabled={busy} onClick={() => {
        setCopyFailed(false);
        void writeText(prompt).then(() => setCopiedIdentity(identity)).catch(() => setCopyFailed(true));
      }}>{t(copiedIdentity === identity ? "completion.copied" : "completion.copy")}</button>
      {copyFailed && <p role="alert">{t("common.retry")}</p>}
      <label className="checkbox-row"><input type="checkbox" checked={reportedIdentity === identity} disabled={busy}
        onChange={event => setReportedIdentity(event.target.checked ? identity : null)} />{t("completion.confirm")}</label>
      {reportedIdentity === identity && <p className="field-help" role="status">{t("completion.reported")}</p>}
    </> : <>
      <p>{t(!localRunner ? "completion.noRunner" : !state.workspace_runner ? "completion.identityPending" : "completion.projectNeeded")}</p>
      {localRunner && onProjects && <button type="button" className="secondary-button" disabled={busy} onClick={onProjects}>{p("projects")}</button>}
    </>}
    <ChatgptObservation state={state} />
  </section>;
}
export function FirstRunCompletion({ state, onState, onComplete, onProjects, onConnection }: {
  state: DesktopState; onState: (state: DesktopState) => void; onComplete: () => void;
  onProjects: () => void; onConnection: () => void;
}) {
  const { t } = useLocale();
  const [editor, setEditor] = useState(false);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { heading.current?.focus(); }, []);
  const create = state.topology?.experience === "full" && state.topology.server.kind === "local";
  const busy = Boolean(state.current_operation);
  const connected = Boolean(state.connections?.profiles.some(profile => profile.ready));
  return <section className="setup-shell first-run-completion" aria-labelledby="completion-title" data-webcodex-page="setup-completion">
    <h1 id="completion-title" ref={heading} tabIndex={-1}>{t("completion.title")}</h1>
    <section className="form-card"><h2>{t("completion.role")}</h2><WorkspaceStatus state={state} /></section>
    <section className="form-card"><h2>{t("completion.connection")}</h2><p>{t(create ? "completion.createHelp" : "completion.joinHelp")}</p>
      {create && <div className="setup-actions">
        <button type="button" className="primary-button" disabled={busy || state.connections?.config_error} onClick={() => setEditor(true)}>{t("completion.addTunnel")}</button>
        <button type="button" className="secondary-button" disabled={busy} onClick={onConnection}>{t("nav.connection")}</button>
      </div>}
    </section>
    <FirstReadGuide state={state} onProjects={onProjects} />
    <div className="setup-actions"><button type="button" className="primary-button" disabled={busy} onClick={onComplete}>{t(create && !connected ? "completion.later" : "completion.overview")}</button></div>
    {create && editor && <ConnectionEditor profile={null} persistentLocal={Boolean(state.persistent_environment)} onState={onState} onClose={() => setEditor(false)} />}
  </section>;
}
