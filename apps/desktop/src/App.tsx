import { WorkspaceProvider } from "./features/workspace/WorkspaceContext";
import brandIcon from "./assets/brand.png";
import { ExtensionsPanel } from "./features/extensions/ExtensionsPanel";
import { ComputerPermissions } from "./features/settings/ComputerPermissions";
import { Sidebar } from "./components/Sidebar";
import { useDesktopWorkspace } from "./hooks/useDesktopWorkspace";
import { desktopApi } from "./lib/desktop-api";
import type {
  DesktopError,
} from "./models/topology";
import { FirstRun } from "./features/onboarding/FirstRun";
import { Dashboard } from "./features/dashboard/Dashboard";
import { ProjectsPanel } from "./features/projects/ProjectsPanel";
import { ConnectionPanel } from "./features/connection/ConnectionPanel";
import { ActivityPanel } from "./features/activity/ActivityPanel";
import { SettingsPanel } from "./features/settings/SettingsPanel";
import { useLocale } from "./i18n/locale";
import { desktopCommandDiagnostics, desktopErrorPresentation, operationLabel } from "./i18n/presentation";

export default function App() {
  const { t } = useLocale();
  const { state, activity, navigation, setNavigation, refreshing, error, setError, cancelSubmittingId, showSetup, setShowSetup, setStartupAttempt, mainRef, commitState, openSetup, chooseLocalProject, refresh, resumeRuntime, cancelCurrentOperation, runStateOperation } = useDesktopWorkspace();
  if (!state) {
    return (
      <main className="splash">
        <img className="brand-mark" src={brandIcon} alt="" />
        {error ? (
          <section className="startup-error" aria-label="WebCodex">
            <AppError error={error} />
            <button
              className="primary-button"
              type="button"
              onClick={() => {
                setError(null);
                setStartupAttempt((attempt) => attempt + 1);
              }}
              data-webcodex-action="retry-desktop-startup"
            >
              {t("common.retry")}
            </button>
          </section>
        ) : (
          <span role="status">{t("app.loading")}</span>
        )}
      </main>
    );
  }

  const needsSetup = !state.topology || showSetup;

  return (
    <WorkspaceProvider state={state}><div className="app-shell">
      <Sidebar state={state} navigation={navigation} setNavigation={setNavigation} />

      <main className="main-content" ref={mainRef} tabIndex={-1}>
        {navigation === "home" && showSetup && state.topology && (
          <button className="back-button" onClick={() => setShowSetup(false)}>
            <span aria-hidden="true">← </span>{t("home.backToOverview")}
          </button>
        )}
        {state.current_operation && (
          <section
            className={`operation-status ${state.current_operation.phase}`}
            role="status"
            aria-live="polite"
            aria-label={t("operation.statusLabel")}
            data-webcodex-operation={state.current_operation.kind}
          >
            <div>
              <span className="section-kicker">
                {state.current_operation.phase === "cancelling"
                  ? t("operation.cancelling")
                  : t("operation.running")}
              </span>
              <strong>{operationLabel(state.current_operation.kind, t)}</strong>
              <span>{t("operation.cancelNote")}</span>
            </div>
            {state.current_operation.cancellable && (
              <button
                className="secondary-button"
                type="button"
                disabled={
                  state.current_operation.phase === "cancelling" ||
                  cancelSubmittingId === state.current_operation.id
                }
                onClick={() => void cancelCurrentOperation()}
                data-webcodex-action="cancel-desktop-operation"
              >
                {state.current_operation.phase === "cancelling"
                  ? t("operation.cancelling")
                  : t("operation.cancel")}
              </button>
            )}
          </section>
        )}
        {error && <AppError error={error} />}
        {navigation !== "settings" && <ComputerPermissions welcome />}
        {navigation === "home" && (needsSetup ? (
          <FirstRun
            state={state}
            onState={commitState}
            chooseModeFirst={showSetup}
            onComplete={() => setShowSetup(false)}
          />
        ) : (
          <Dashboard
            state={state}
            refreshing={refreshing}
            onRefresh={() => void refresh()}
            onResumeRuntime={() => void resumeRuntime()}
            onChooseProject={() => void chooseLocalProject()}
            onOpenProject={(path) => void runStateOperation(() => desktopApi.activateLocalProject(path))}
            onChangeSetup={openSetup}
            onNavigate={setNavigation}
            onStopQuickShare={() => void runStateOperation(desktopApi.stopQuickShare)}
            onStopRuntime={() => void runStateOperation(desktopApi.stopLocalRuntime)}
          />
        ))}
        {navigation === "projects" && (
          <ProjectsPanel state={state} onChooseProject={() => void chooseLocalProject()} onSelectProject={(path) => void runStateOperation(() => desktopApi.activateLocalProject(path))} />
        )}
        {navigation === "connection" && <ConnectionPanel state={state} onState={commitState} />}
        {navigation === "activity" && <ActivityPanel activity={activity} />}
        {navigation === "extensions" && <ExtensionsPanel state={state} onState={commitState} />}
        {navigation === "settings" && <SettingsPanel state={state} onState={commitState} onChangeSetup={openSetup} onStopRuntime={() => void runStateOperation(desktopApi.stopLocalRuntime)} />}
      </main>
    </div></WorkspaceProvider>
  );
}

function AppError({ error }: { error: DesktopError }) {
  const { t } = useLocale();
  const presentation = desktopErrorPresentation(error, t);
  const diagnostics = desktopCommandDiagnostics(error);
  return (
    <div className="error-card app-error" role="alert">
      <strong>{presentation.title}</strong>
      <span>{presentation.action}</span>
      <details>
        <summary>{t("common.details")}</summary>
        <code>{error.code}</code>
        <p>{error.message}</p>
        {diagnostics && (
          <dl className="error-diagnostics">
            {diagnostics.phase && <><dt>phase</dt><dd><code>{diagnostics.phase}</code></dd></>}
            {diagnostics.logicalCommand && <><dt>command</dt><dd><code>{diagnostics.logicalCommand}</code></dd></>}
            {diagnostics.executable && <><dt>executable</dt><dd><code>{diagnostics.executable}</code></dd></>}
            {diagnostics.exitCode !== undefined && <><dt>exit code</dt><dd><code>{diagnostics.exitCode}</code></dd></>}
            {diagnostics.reasonCode && <><dt>reason</dt><dd><code>{diagnostics.reasonCode}</code></dd></>}
          </dl>
        )}
      </details>
    </div>
  );
}
