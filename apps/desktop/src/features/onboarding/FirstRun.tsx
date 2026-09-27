import { useState } from "react";
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
  const savedMode = state.topology?.experience === "full"
    ? state.topology.server.kind === "local" ? "create" : "join" : null;
  const [mode, setMode] = useState<SetupMode | null>(chooseModeFirst ? null : savedMode);
  const [runner, setRunner] = useState(savedMode ? state.topology?.runner.kind === "local" : true);
  const [serverUrl, setServerUrl] = useState(state.topology?.server.kind === "remote" ? state.topology.server.url : "");
  const [pairingCode, setPairingCode] = useState("");
  const [replacePairingCode, setReplacePairingCode] = useState(false);
  const [userToken, setUserToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const mutationBusy = busy || Boolean(state.current_operation);
  const origin = serverOrigin(serverUrl);
  const sameServer = mode === "join" && origin !== null && state.topology?.experience === "full"
    && state.topology.server.kind === "remote" && origin === serverOrigin(state.topology.server.url);
  const reuseRunner = sameServer && state.topology?.runner.kind === "local" && !replacePairingCode;
  const reuseViewer = sameServer && Boolean(state.persistent_environment);
  const needsCode = mode === "join" && runner && !reuseRunner;
  const needsToken = mode === "join" && !runner && !reuseViewer;
  const canSubmit = !mutationBusy && (mode === "create" || mode === "join" && origin !== null
    && (!needsCode || Boolean(pairingCode.trim())) && (!needsToken || Boolean(userToken.trim())));

  const clearInputs = () => {
    setPairingCode(""); setUserToken(""); setReplacePairingCode(false); setError(null);
  };
  const chooseMode = (next: SetupMode | null) => {
    if (mutationBusy) return;
    clearInputs(); setMode(next);
  };
  const run = async () => {
    if (!canSubmit) return;
    setBusy(true); setError(null);
    const code = pairingCode.trim();
    const credential = userToken.trim();
    setPairingCode(""); setUserToken("");
    try {
      const next = await desktopApi.configureEnvironment({
        mode, serverUrl: mode === "join" ? origin : null,
        // Preserve a legacy migration's recorded display project, but never
        // require or register a new default project during normal setup.
        projectPath: !state.persistent_environment && savedMode ? state.project?.path ?? null : null,
        runner,
        pairingCode: needsCode ? code : null,
        userToken: needsToken ? credential : null,
        replacePairingCode,
      });
      setReplacePairingCode(false); onState(next); onComplete?.();
    } catch (value) { setError(normalizeDesktopError(value)); }
    finally { setBusy(false); }
  };

  if (mode === "share") return <QuickShareSetup state={state} onState={onState}
    onBack={() => chooseMode(null)} onComplete={onComplete} />;

  if (!mode) return <section className="first-run" aria-labelledby="first-run-title" data-webcodex-page="first-run">
    <div className="eyebrow">{t("first.welcome")}</div>
    <h1 id="first-run-title">{t("first.title")}</h1>
    <p className="lede">{t("first.description")}</p>
    <div className="entry-grid">
      <button className="entry-card recommended" disabled={mutationBusy || Boolean(state.persistent_environment && savedMode === "join")}
        onClick={() => chooseMode("create")} data-webcodex-action="choose-local-setup">
        <span className="entry-badge">{t("first.recommended")}</span>
        <span className="entry-icon" aria-hidden="true"><FolderOpen /></span>
        <strong>{t("first.localTitle")}</strong><span>{t("first.localDescription")}</span>
      </button>
      <button className="entry-card" disabled={mutationBusy || Boolean(state.persistent_environment && savedMode === "create")}
        onClick={() => chooseMode("join")} data-webcodex-action="choose-remote-setup">
        <span className="entry-icon" aria-hidden="true"><Globe2 /></span>
        <strong>{t("first.remoteTitle")}</strong><span>{t("first.remoteDescription")}</span>
      </button>
      <button className="entry-card" disabled={mutationBusy} onClick={() => chooseMode("share")}
        data-webcodex-action="choose-quick-share-setup">
        <span className="entry-icon" aria-hidden="true"><Share2 /></span>
        <strong>{t("first.shareTitle")}</strong><span>{t("first.shareDescription")}</span>
      </button>
    </div>
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
      <p className="lede">{mode === "create" ? t("setup.localDescription") : t("setup.remoteDescription")}</p>
      <div className="form-card">
        <label className="checkbox-row"><input type="checkbox" checked={runner}
          disabled={mutationBusy || Boolean(savedMode && state.topology?.runner.kind === "local")}
          onChange={event => { clearInputs(); setRunner(event.target.checked); }} />{t("setup.localRunner")}</label>
        <p className="field-help">{runner ? t("setup.localRunnerHelp") : t(mode === "create" ? "setup.serverOnly" : "setup.viewerOnly")}</p>
      </div>
      {mode === "join" && <div className="form-card">
        <div className="field-group">
          <label htmlFor="setup-server-url">{t("setup.serverUrl")}</label>
          <input id="setup-server-url" type="url" value={serverUrl} required
            onChange={event => { clearInputs(); setServerUrl(event.target.value); }}
            placeholder="https://webcodex.example.com" disabled={mutationBusy} readOnly={Boolean(state.persistent_environment)}
            aria-describedby="setup-server-url-help" />
          <span className="field-help" id="setup-server-url-help">{t(state.persistent_environment ? "setup.savedEnvironment" : "setup.serverUrlHelp")}</span>
        </div>
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
      {(runner || mode === "create") && <p className="field-help">{t("setup.serviceConsent")}</p>}
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
