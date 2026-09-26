import { displayProjectPath } from "../../../../../frontend/src/ui/projectPresentation";
import { useMemo, useState } from "react";
import { FolderOpen, Globe2 } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../lib/desktop-api";
import { useLocale, type MessageKey } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import type { DesktopError, DesktopState, ProjectSelection, SetupProgressStep } from "../../models/topology";

type SetupMode = "create" | "join";

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

interface FirstRunProps {
  state: DesktopState;
  onState: (state: DesktopState) => void;
  chooseModeFirst?: boolean;
  onComplete?: () => void;
}

export function FirstRun({ state, onState, chooseModeFirst = false, onComplete }: FirstRunProps) {
  const { t } = useLocale();
  const initialMode = useMemo<SetupMode | null>(() => {
    if (chooseModeFirst) return null;
    if (state.topology?.experience !== "full") return null;
    return state.topology.server.kind === "local" ? "create" : "join";
  }, [chooseModeFirst, state.topology]);
  const [mode, setMode] = useState<SetupMode | null>(initialMode);
  const [project, setProject] = useState<ProjectSelection | null>(state.project ?? null);
  const [serverUrl, setServerUrl] = useState(state.topology?.server.kind === "remote" ? state.topology.server.url : "");
  const [pairingCode, setPairingCode] = useState("");
  const [replacePairingCode, setReplacePairingCode] = useState(false);
  const [userToken, setUserToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const mutationBusy = busy || Boolean(state.current_operation);
  const samePersistentServer = Boolean(state.persistent_environment)
    && state.topology?.server.kind === "remote"
    && mode === "join"
    && serverUrl === state.topology.server.url;
  const reuseLegacyRegistration = !state.persistent_environment
    && state.topology?.experience === "full"
    && state.topology.server.kind === "remote"
    && mode === "join"
    && project?.path === state.project?.path
    && serverUrl === state.topology.server.url;
  const reusePersistentRunnerRegistration = samePersistentServer
    && state.topology?.runner.kind === "local";
  const reuseRunnerRegistration = reusePersistentRunnerRegistration || reuseLegacyRegistration;

  const chooseProject = async () => {
    setError(null);
    try {
      const selection = await open({ directory: true, multiple: false, title: t("setup.chooseProject") });
      if (typeof selection === "string") setProject(await desktopApi.inspectProject(selection));
    } catch (value) {
      setError(normalizeDesktopError(value));
    }
  };

  const run = async () => {
    if (!mode || mutationBusy) return;
    setBusy(true);
    setError(null);
    const oneTimeCode = pairingCode;
    const credential = userToken;
    setPairingCode("");
    setUserToken("");
    try {
      const next = await desktopApi.configureEnvironment({
        mode,
        serverUrl: mode === "join" ? serverUrl : null,
        projectPath: project?.path ?? null,
        pairingCode: mode === "join" && project ? oneTimeCode || null : null,
        userToken: mode === "join" && !project && !samePersistentServer ? credential : null,
        replacePairingCode,
      });
      setReplacePairingCode(false);
      onState(next);
      onComplete?.();
    } catch (value) {
      setError(normalizeDesktopError(value));
    } finally {
      setBusy(false);
    }
  };

  if (!mode) {
    return <section className="first-run" aria-labelledby="first-run-title" data-webcodex-page="first-run">
      <div className="eyebrow">{t("first.welcome")}</div>
      <h1 id="first-run-title">{t("first.title")}</h1>
      <p className="lede">{t("first.description")}</p>
      <ol className="setup-overview" aria-label={t("workspace.progress")}>
        <li><span>01</span>{t("workspace.prepare")}</li>
        <li><span>02</span>{t("workspace.connect")}</li>
        <li><span>03</span>{t("workspace.verify")}</li>
      </ol>
      <div className="entry-grid">
        <button className="entry-card recommended" onClick={() => setMode("create")} data-webcodex-action="choose-local-setup">
          <span className="entry-badge">{t("first.recommended")}</span>
          <span className="entry-icon" aria-hidden="true"><FolderOpen /></span>
          <strong>{t("first.localTitle")}</strong>
          <span>{t("first.localDescription")}</span>
        </button>
        <button className="entry-card" onClick={() => setMode("join")} data-webcodex-action="choose-remote-setup">
          <span className="entry-icon" aria-hidden="true"><Globe2 /></span>
          <strong>{t("first.remoteTitle")}</strong>
          <span>{t("first.remoteDescription")}</span>
        </button>
      </div>
    </section>;
  }

  const presentation = error ? desktopErrorPresentation(error, t) : null;
  const serverInvalid = error?.code === "server_url_invalid" || error?.code === "server_unreachable";
  const pairingInvalid = error?.code === "pairing_code_invalid";

  return <>
    <form className="setup-shell" aria-labelledby="setup-title" aria-busy={mutationBusy}
      data-webcodex-page="setup" onSubmit={event => { event.preventDefault(); void run(); }}>
      <button type="button" className="back-button" onClick={() => setMode(null)}
        data-webcodex-action="show-setup-options">{t("setup.back")}</button>
      <div className="eyebrow">{mode === "create" ? t("setup.localLabel") : t("setup.remoteLabel")}</div>
      <h1 id="setup-title">{mode === "create" ? t("setup.localTitle") : t("setup.remoteTitle")}</h1>
      <p className="lede">{mode === "create" ? t("setup.localDescription") : t("setup.remoteDescription")}</p>
      <div className="project-picker-card">
        <div>
          <span className="section-kicker">{t("setup.project")}</span>
          <strong>{project ? displayProjectPath(project.path) : t("setup.noLocalProject")}</strong>
          {project && <span className="project-meta">{t("setup.allowedRoot", {
            root: project.allowed_root,
            kind: project.is_git_repository ? t("setup.gitRepository") : t("setup.folder"),
          })}</span>}
          {!project && <span className="project-meta">{mode === "create"
            ? t("setup.serverOnly") : t("setup.viewerOnly")}</span>}
        </div>
        <button type="button" className="secondary-button" onClick={chooseProject} disabled={mutationBusy}
          data-webcodex-action="choose-project">{project ? t("setup.changeFolder") : t("setup.chooseFolder")}</button>
        {project && <button type="button" className="secondary-button" onClick={() => setProject(null)}
          disabled={mutationBusy}>{t("setup.skipFolder")}</button>}
      </div>
      {mode === "join" && <div className="form-card">
        <div className="field-group">
          <label htmlFor="setup-server-url">{t("setup.serverUrl")}</label>
          <input id="setup-server-url" type="url" value={serverUrl} onChange={event => setServerUrl(event.target.value)}
            placeholder="https://webcodex.example.com" disabled={mutationBusy}
            aria-describedby="setup-server-url-help" aria-invalid={serverInvalid || undefined}
            aria-errormessage={serverInvalid ? "setup-error" : undefined} />
          <span className="field-help" id="setup-server-url-help">{t("setup.serverUrlHelp")}</span>
        </div>
        {project && !reuseRunnerRegistration ? <div className="field-group">
          <label htmlFor="setup-pairing-code">{t("setup.pairingCode")}</label>
          <input id="setup-pairing-code" type="password" value={pairingCode}
            onChange={event => setPairingCode(event.target.value)} placeholder="wc_pair_…"
            autoComplete="off" spellCheck={false} disabled={mutationBusy}
            aria-describedby="setup-pairing-code-help" aria-invalid={pairingInvalid || undefined}
            aria-errormessage={pairingInvalid ? "setup-error" : undefined} />
          <span className="field-help" id="setup-pairing-code-help">{t("setup.pairingCodeHelp")}</span>
        </div> : !project && !samePersistentServer ? <div className="field-group">
          <label htmlFor="setup-user-token">{t("setup.userCredential")}</label>
          <input id="setup-user-token" type="password" value={userToken}
            onChange={event => setUserToken(event.target.value)}
            autoComplete="off" spellCheck={false} disabled={mutationBusy} />
          <span className="field-help">{t("setup.userCredentialHelp")}</span>
        </div> : <span className="field-help">
          {project
            ? t(reusePersistentRunnerRegistration ? "setup.reuseRunnerRegistrationHelp" : "setup.reuseEnrollmentHelp")
            : t("setup.reuseViewerCredentialHelp")}
        </span>}
      </div>}
      {error && <div className="error-card" role="alert" id="setup-error">
        <strong>{presentation?.title}</strong><span>{presentation?.action}</span>
        <details><summary>{t("common.details")}</summary><code>{error.code}</code><p>{error.message}</p></details>
        {error.code === "pairing_recovery_required" && <button type="button" className="secondary-button"
          onClick={() => { setReplacePairingCode(true); setError(null); }}>
          {t("setup.useNewPairingCode")}
        </button>}
      </div>}
      {mutationBusy && state.setup_progress && <div className="field-help" role="status" aria-live="polite">
        {t("setup.progressCurrent", { step: t(setupProgressLabels[state.setup_progress.step]) })}
      </div>}
      <div className="setup-actions">
        <button type="submit" className="primary-button" disabled={mutationBusy ||
          (mode === "join" && (!serverUrl.trim() || (project ? !reuseRunnerRegistration && !pairingCode.trim() : !samePersistentServer && !userToken.trim())))}
          data-webcodex-action={mode === "create" ? "configure-local" : "configure-remote"}>
          {mutationBusy ? t("common.checking") : mode === "create" ? t("setup.setUp") : t("setup.connect")}
        </button>
        <span className="action-help">{mutationBusy ? t("setup.verifying") : t("setup.noTerminal")}</span>
      </div>
    </form>
  </>;
}
