import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { runnerServerAddress } from "../../lib/runner-server-address";
import { NativeSelect } from "@mantine/core";
import { useServiceScopeText } from "../../i18n/service-scope";
import { FolderOpen, Globe2, Share2 } from "lucide-react";
import { desktopApi } from "../../lib/desktop-api";
import { useLocale, type MessageKey } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import type { DesktopError, DesktopState, SetupProgressStep } from "../../models/topology";
import { PowerShellInstallGuidance } from "../settings/PowerShellInstallGuidance";
import { QuickShareSetup } from "./QuickShareSetup";

type SetupMode = "create" | "join" | "share";
const setupProgressLabels: Record<SetupProgressStep, MessageKey> = {
  preflight: "setup.progress.preflight",
  server_configuration: "setup.progress.server_configuration",
  server_service_install: "setup.progress.server_service_install",
  server_service_start: "setup.progress.server_service_start",
  server_reachability: "setup.progress.server_reachability",
  user_authentication: "setup.progress.user_authentication",
  runner_enrollment: "setup.progress.runner_enrollment",
  runner_configuration: "setup.progress.runner_configuration",
  project_registration: "setup.progress.project_registration",
  runner_service_install: "setup.progress.runner_service_install",
  runner_service_start: "setup.progress.runner_service_start",
  readiness: "setup.progress.readiness",
};

// Match Core's origin-only input contract. A malformed address must never
// qualify for saved-credential reuse merely because its hostname looks familiar.
function serverOrigin(value: string): string | null {
  try {
    const url = new URL(value.trim());
    return ["http:", "https:"].includes(url.protocol) && !url.username && !url.password
      && url.pathname === "/" && !url.search && !url.hash ? url.origin : null;
  } catch { return null; }
}

interface FirstRunProps {
  state: DesktopState;
  onState: (state: DesktopState) => void;
  chooseModeFirst?: boolean;
  onComplete?: () => void;
}

export function FirstRun({ state, onState, chooseModeFirst = false, onComplete }: FirstRunProps) {
  const { t } = useLocale();
  const scopeText = useServiceScopeText();
  const setup = state.environment_setup;
  const savedMode = setup?.mode ?? (state.topology?.experience === "full"
    ? state.topology.server.kind === "local" ? "create" : "join" : null);
  const savedRunner = setup?.runner ?? (savedMode ? state.topology?.runner.kind === "local" : true);
  const savedServer = setup?.server_url ?? (state.topology?.server.kind === "remote" ? state.topology.server.url : "");
  const savedProject = setup ? setup.project_path : (!state.persistent_environment && savedMode ? state.project?.path ?? null : null);
  const pending = Boolean(setup && !setup.configured);
  const contextKey = JSON.stringify([setup ?? null, savedMode, savedRunner, savedServer, savedProject, state.persistent_environment]);
  const [activeContext, setActiveContext] = useState(contextKey);
  const [serviceScope, setServiceScope] = useState<"user" | "system">(setup?.service_scope ?? "user");
  const [mode, setMode] = useState<SetupMode | null>(chooseModeFirst ? null : savedMode);
  const [runner, setRunner] = useState(savedRunner);
  const [serverUrl, setServerUrl] = useState(savedServer);
  const [projectPath, setProjectPath] = useState<string | null>(savedProject);
  const [advanced, setAdvanced] = useState(Boolean(savedMode && !savedRunner));
  const [pairingCode, setPairingCode] = useState("");
  const [replacePairingCode, setReplacePairingCode] = useState(false);
  const [userToken, setUserToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const currentContext = useRef(contextKey);
  currentContext.current = contextKey;
  const currentSetup = useRef(setup);
  currentSetup.current = setup;
  useEffect(() => {
    if (activeContext === contextKey) return;
    setActiveContext(contextKey); setMode(chooseModeFirst && !pending && !busy ? null : savedMode);
    setRunner(savedRunner); setServerUrl(savedServer); setProjectPath(savedProject);
    setServiceScope(setup?.service_scope ?? "user"); setAdvanced(Boolean(savedMode && !savedRunner));
    setPairingCode(""); setUserToken(""); setReplacePairingCode(false); setError(null);
  }, [activeContext, contextKey, chooseModeFirst, savedMode, savedRunner, savedServer, savedProject, setup?.service_scope, pending, busy]);
  const mutationBusy = busy || Boolean(state.current_operation) || activeContext !== contextKey;
  const canChooseServiceScope = !pending && !setup?.configured && !state.persistent_environment && (chooseModeFirst || !savedMode);
  const canChooseProject = !pending && !state.persistent_environment && !setup?.configured;
  const origin = serverOrigin(serverUrl);
  const recordedServer = setup?.mode === "join" ? setup.server_url
    : state.topology?.experience === "full" && state.topology.server.kind === "remote" ? state.topology.server.url : null;
  const sameServer = mode === "join" && origin !== null && recordedServer !== null && origin === serverOrigin(recordedServer);
  const reuseRunner = sameServer && (state.topology?.runner.kind === "local" || Boolean(setup?.configured && setup.runner)) && !replacePairingCode;
  const reuseViewer = sameServer && Boolean(state.persistent_environment || setup?.configured);
  const needsCode = mode === "join" && runner && !reuseRunner;
  const needsToken = mode === "join" && !runner && !reuseViewer;
  const address = runnerServerAddress(serverUrl);
  const addressIssue = mode === "join" && !advanced && !savedMode ? address.issue : null;
  const canSubmit = !mutationBusy && !addressIssue && (mode === "create" || mode === "join" && origin !== null
    && (!needsCode || Boolean(pairingCode.trim())) && (!needsToken || Boolean(userToken.trim())));

  const clearInputs = () => {
    setPairingCode(""); setUserToken(""); setReplacePairingCode(false); setError(null);
  };
  const chooseMode = (next: SetupMode | null, advancedChoice = false) => {
    if (mutationBusy) return;
    if (pending && next && next !== savedMode) return;
    clearInputs(); setMode(next);
    if (next) {
      setAdvanced(advancedChoice || Boolean(savedMode && !savedRunner));
      if (!savedMode) setRunner(!advancedChoice);
      else if (advancedChoice && !pending && !savedRunner) setRunner(false);
    }
  };
  const chooseProject = async () => {
    if (mutationBusy || !canChooseProject) return;
    const expectedContext = contextKey;
    setBusy(true); setError(null);
    try {
      const path = await open({ directory: true, multiple: false, title: t("setup.chooseProject") });
      if (typeof path === "string") {
        const project = await desktopApi.inspectProject(path);
        if (currentContext.current === expectedContext) setProjectPath(project.path);
      }
    } catch (value) { if (currentContext.current === expectedContext) setError(normalizeDesktopError(value)); }
    finally { setBusy(false); }
  };
  const run = async () => {
    if (!canSubmit) return;
    const expectedContext = contextKey;
    setBusy(true); setError(null);
    const code = pairingCode.trim();
    const credential = userToken.trim();
    const submittedIntent = { environmentId: setup?.environment_id, mode, runner, projectPath: runner ? projectPath : null, origin, serviceScope };
    const stillCurrent = () => {
      if (currentContext.current === expectedContext) return true;
      const projected = currentSetup.current;
      return Boolean(projected && (!submittedIntent.environmentId || projected.environment_id === submittedIntent.environmentId) && projected.mode === submittedIntent.mode && projected.runner === submittedIntent.runner
        && projected.project_path === submittedIntent.projectPath && projected.service_scope === submittedIntent.serviceScope
        && (submittedIntent.mode === "create" || serverOrigin(projected.server_url) === submittedIntent.origin));
    };
    setPairingCode(""); setUserToken("");
    try {
      const next = await desktopApi.configureEnvironment({
        ...((canChooseServiceScope || pending) ? { serviceScope } : {}),
        mode, serverUrl: mode === "join" ? origin : null,
        projectPath: runner ? projectPath : null,
        runner,
        pairingCode: needsCode ? code : null,
        userToken: needsToken ? credential : null,
        replacePairingCode,
      });
      if (stillCurrent()) { setReplacePairingCode(false); onState(next); onComplete?.(); }
    } catch (value) { if (stillCurrent()) setError(normalizeDesktopError(value)); }
    finally { setBusy(false); }
  };

  if (mode === "share") return <QuickShareSetup state={state} onState={onState}
    onBack={() => chooseMode(null)} onComplete={onComplete} />;

  if (!mode) return <section className="first-run" aria-labelledby="first-run-title" data-webcodex-page="first-run">
    <div className="eyebrow">{t("first.welcome")}</div>
    <h1 id="first-run-title">{t("first.title")}</h1>
    <p className="lede">{t("first.description")}</p>
    <div className="entry-grid">
      <button className="entry-card recommended" disabled={mutationBusy || Boolean((state.persistent_environment || setup?.configured || pending) && savedMode === "join")}
        onClick={() => chooseMode("create")} data-webcodex-action="choose-local-setup">
        <span className="entry-badge">{t("first.recommended")}</span>
        <span className="entry-icon" aria-hidden="true"><FolderOpen /></span>
        <strong>{t("first.localTitle")}</strong><span>{t("first.localDescription")}</span>
      </button>
      <button className="entry-card recommended" disabled={mutationBusy || Boolean((state.persistent_environment || setup?.configured || pending) && savedMode === "create")}
        onClick={() => chooseMode("join")} data-webcodex-action="choose-remote-setup">
        <span className="entry-icon" aria-hidden="true"><Globe2 /></span>
        <strong>{t("first.remoteTitle")}</strong><span>{t("first.remoteDescription")}</span>
      </button>
    </div>
    <details><summary>{t("first.advanced")}</summary>
      <div className="entry-grid">
        <button className="entry-card" disabled={mutationBusy || Boolean(savedMode && savedMode !== "create") || Boolean(savedRunner && savedMode)}
          onClick={() => chooseMode("create", true)} data-webcodex-action="choose-server-only-setup">
          <strong>{t("first.serverOnlyTitle")}</strong><span>{t("setup.serverOnly")}</span>
        </button>
        <button className="entry-card" disabled={mutationBusy || Boolean(savedMode && savedMode !== "join") || Boolean(savedRunner && savedMode)}
          onClick={() => chooseMode("join", true)} data-webcodex-action="choose-viewer-setup">
          <strong>{t("first.viewerTitle")}</strong><span>{t("setup.viewerOnly")}</span>
        </button>
        <button className="entry-card" disabled={mutationBusy || pending} onClick={() => chooseMode("share")}
          data-webcodex-action="choose-quick-share-setup">
          <span className="entry-icon" aria-hidden="true"><Share2 /></span>
          <strong>{t("first.shareTitle")}</strong><span>{t("first.shareDescription")}</span>
        </button>
      </div>
    </details>
    {state.persistent_environment && <p className="field-help">{t("setup.savedEnvironment")}</p>}
  </section>;

  const presentation = error ? desktopErrorPresentation(error, t) : null;
  return <>
    <form className="setup-shell" aria-labelledby="setup-title" aria-busy={mutationBusy}
      data-webcodex-page="setup" onSubmit={event => { event.preventDefault(); void run(); }}>
      <button type="button" className="back-button" disabled={mutationBusy} onClick={() => chooseMode(null)}
        data-webcodex-action="show-setup-options">{t("setup.back")}</button>
      <div className="eyebrow">{mode === "create" ? t("setup.localLabel") : t("setup.remoteLabel")}</div>
      <h1 id="setup-title">{mode === "create" ? t("setup.localTitle") : t("setup.remoteTitle")}</h1>
      <p className="lede">{mode === "create" ? t(runner ? "setup.localDescription" : "setup.serverOnly") : t(runner ? "setup.remoteDescription" : "setup.viewerOnly")}</p>
      {savedMode && !state.persistent_environment && <p className="workspace-notice" role="note">{scopeText("migrationHelp")}</p>}
      <div className="form-card">
        <p className="field-help">{runner ? t("setup.localRunnerHelp") : t(mode === "create" ? "setup.serverOnly" : "setup.viewerOnly")}</p>
        <details open={advanced} onToggle={event => setAdvanced(event.currentTarget.open)}>
          <summary>{t("first.advanced")}</summary>
          <label className="checkbox-row"><input type="checkbox" checked={runner}
            disabled={mutationBusy || pending || Boolean(savedMode && savedRunner)}
            onChange={event => { clearInputs(); setRunner(event.target.checked); }} />{t("setup.localRunner")}</label>
        </details>
      </div>
      {runner && canChooseProject && <div className="project-picker-card">
        <div>
          <strong>{t("setup.initialProject")}</strong>
          <p className="field-help">{t("setup.initialProjectHelp")}</p>
          {projectPath && <code>{projectPath}</code>}
        </div>
        <div className="setup-project-actions">
          <button type="button" className="secondary-button" disabled={mutationBusy} onClick={() => void chooseProject()}
            data-webcodex-action="choose-project">{t(projectPath ? "setup.changeFolder" : "setup.chooseFolder")}</button>
          {projectPath && <button type="button" className="secondary-button" disabled={mutationBusy} onClick={() => setProjectPath(null)}
            data-webcodex-action="clear-project">{t("setup.clearInitialProject")}</button>}
        </div>
      </div>}
      {mode === "join" && <div className="form-card">
        <div className="field-group">
          <label htmlFor="setup-server-url">{t("setup.serverUrl")}</label>
          <input id="setup-server-url" type="url" value={serverUrl} required
            onChange={event => { clearInputs(); setServerUrl(event.target.value); }}
            placeholder="https://webcodex.example.com" disabled={mutationBusy} readOnly={Boolean(state.persistent_environment || setup?.configured) || pending}
            aria-describedby="setup-server-url-help" />
          <span className="field-help" id="setup-server-url-help">{t(state.persistent_environment ? "setup.savedEnvironment" : "setup.serverUrlHelp")}</span>
        </div>
        {addressIssue && serverUrl.trim() && <p className="field-help" role="alert">{t(addressIssue === "loopback"
          ? "setup.serverUrlLoopback" : addressIssue === "openai" ? "setup.serverUrlOpenai" : "setup.serverUrlInvalid")}</p>}
        {needsCode ? <div className="field-group">
          <label htmlFor="setup-pairing-code">{t("setup.pairingCode")}</label>
          <input id="setup-pairing-code" type="password" value={pairingCode} required
            onChange={event => setPairingCode(event.target.value)} autoComplete="off" spellCheck={false} disabled={mutationBusy} />
          <span className="field-help">{t("setup.pairingCodeHelp")}</span>
        </div> : needsToken ? <div className="field-group">
          <label htmlFor="setup-user-token">{t("setup.userCredential")}</label>
          <input id="setup-user-token" type="password" value={userToken} required
            onChange={event => setUserToken(event.target.value)} autoComplete="off" spellCheck={false} disabled={mutationBusy} />
          <span className="field-help">{t("setup.userCredentialHelp")}</span>
        </div> : <div className="field-help"><strong>{t(runner ? "setup.reuseEnrollment" : "setup.reuseViewerCredential")}</strong>
          <p>{t(runner ? "setup.reuseRunnerRegistrationHelp" : "setup.reuseViewerCredentialHelp")}</p></div>}
      </div>}
      {(runner || mode === "create") && (canChooseServiceScope
        ? <div className="form-card">
            <strong>{scopeText(serviceScope)}</strong>
            <p className="field-help">{scopeText(serviceScope === "user" ? "userHelp" : "systemHelp")}</p>
            <details><summary>{scopeText("advanced")}</summary>
              <NativeSelect label={scopeText("title")} value={serviceScope} disabled={mutationBusy}
                data={[{value:"user",label:scopeText("user")},{value:"system",label:scopeText("system")}]}
                onChange={event => { const value=event.currentTarget.value; if (value === "user" || value === "system") setServiceScope(value); }} />
            </details>
          </div>
        : <p className="field-help">{t("setup.serviceConsent")}</p>)}
      {error && <div className="error-card" role="alert" id="setup-error">
        <strong>{presentation?.title}</strong><span>{presentation?.action}</span>
        <details><summary>{t("common.details")}</summary><code>{error.code}</code><p>{error.message}</p></details>
        {error.code === "pairing_recovery_required" && <button type="button" className="secondary-button" disabled={mutationBusy}
          onClick={() => { setReplacePairingCode(true); setError(null); }}>{t("setup.useNewPairingCode")}</button>}
      </div>}
      {mutationBusy && state.setup_progress && <div className="field-help" role="status" aria-live="polite">
        {t("setup.progressCurrent", { step: t(setupProgressLabels[state.setup_progress.step]) })}
      </div>}
      <div className="setup-actions">
        <button type="submit" className="primary-button" disabled={!canSubmit}
          data-webcodex-action={mode === "create" ? "configure-local" : "configure-remote"}>
          {mutationBusy ? t("common.checking") : mode === "create" ? t("setup.setUp") : t("setup.connect")}
        </button>
        <span className="action-help">{mutationBusy ? t("setup.verifying") : t("setup.noTerminal")}</span>
      </div>
    </form>
    {runner && <PowerShellInstallGuidance state={state} onState={onState} />}
  </>;
}
