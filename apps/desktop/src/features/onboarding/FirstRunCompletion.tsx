import { useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import type { DesktopState } from "../../models/topology";
import { ChatgptObservation } from "../workspace/WorkspaceStatus";

// Runtime IDs, never an inferred folder name or user-provided label.
export function firstReadTarget(state: DesktopState) {
  const runner = state.topology?.runner.kind === "local" ? state.workspace_runner?.client_id : null;
  const project = state.project?.runtime_project_id;
  return runner && project?.startsWith(`agent:${runner}:`) ? { runner, project } : null;
}
export function FirstReadGuide({ state, onProjects }: { state: DesktopState; onProjects?: () => void }) {
  const { t } = useLocale(); const p = useProduct();
  const target = firstReadTarget(state);
  const runner = state.topology?.runner.kind === "local" ? state.workspace_runner?.client_id : null;
  const identity = JSON.stringify([state.persistent_environment, state.workspace_runner?.client_id, target?.project]);
  // An ephemeral user report never changes native observation or readiness.
  const [reportedIdentity, setReportedIdentity] = useState<string | null>(null);
  const [copiedIdentity, setCopiedIdentity] = useState<string | null>(null);
  const [copyFailed, setCopyFailed] = useState(false);
  useEffect(() => { setReportedIdentity(null); setCopiedIdentity(null); setCopyFailed(false); }, [identity]);
  const prompt = target ? t("completion.prompt", target) : runner ? t("completion.runtimePrompt", { runner }) : null;
  const localRunner = state.topology?.runner.kind === "local";
  const busy = Boolean(state.current_operation);
  return <section className="form-card first-read-guide" aria-labelledby="first-read-title">
    <h2 id="first-read-title">{t(target ? "completion.read" : "completion.runtimeRead")}</h2>
    {runner && prompt ? <>
      <dl className="completion-identities"><div><dt>{t("completion.runnerId")}</dt><dd><code>{runner}</code></dd></div>{target && <div><dt>{t("completion.projectId")}</dt><dd><code>{target.project}</code></dd></div>}</dl>
      <p className="completion-prompt">{prompt}</p>
      <button type="button" className="secondary-button" disabled={busy} onClick={() => {
        setCopyFailed(false);
        void writeText(prompt).then(() => setCopiedIdentity(identity)).catch(() => setCopyFailed(true));
      }}>{t(copiedIdentity === identity ? "completion.copied" : "completion.copy")}</button>
      {copyFailed && <p role="alert" className="workspace-notice">{t("completion.copyFailed")}</p>}
      <label className="checkbox-row"><input type="checkbox" checked={reportedIdentity === identity} disabled={busy}
        onChange={event => setReportedIdentity(event.target.checked ? identity : null)} />{t(target ? "completion.confirm" : "completion.runtimeConfirm")}</label>
      {reportedIdentity === identity && <p className="field-help" role="status">{t(target ? "completion.reported" : "completion.runtimeReported")}</p>}
    </> : <>
      <p>{t(!localRunner ? "completion.noRunner" : !state.workspace_runner ? "completion.identityPending" : "completion.projectNeeded")}</p>
      {localRunner && onProjects && <button type="button" className="secondary-button" disabled={busy} onClick={onProjects}>{p("projects")}</button>}
    </>}
    <ChatgptObservation state={state} />
  </section>;
}
export function FirstRunCompletion({ onComplete, onProjects, onConnection }: {
  onComplete: () => void; onProjects: () => void; onConnection: () => void;
}) {
  const { t } = useLocale();
  const p = useProduct();
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { heading.current?.focus(); }, []);
  return <section className="setup-shell first-run-completion" aria-labelledby="completion-title" data-webcodex-page="setup-completion">
    <h1 id="completion-title" ref={heading} tabIndex={-1}>{t("completion.title")}</h1>
    <div className="setup-actions">
      <button type="button" className="primary-button" onClick={onConnection}>{t("completion.connectApps")}</button>
      <button type="button" className="secondary-button" onClick={onProjects}>{p("projects")}</button>
      <button type="button" className="text-button" onClick={onComplete}>{t("completion.overview")}</button>
    </div>
  </section>;
}
