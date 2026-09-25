import { useState } from "react";
import { WorkspaceProvider } from "./features/workspace/WorkspaceContext";
import { Alert, Button } from "@mantine/core";
import { DesktopMantineProvider } from "./components/DesktopMantineProvider";
import { BrandMark } from "../../../frontend/src/ui/BrandMark";
import { ExtensionsPanel } from "./features/extensions/ExtensionsPanel";
import { ComputerPermissions } from "./features/settings/ComputerPermissions";
import { Sidebar } from "./components/Sidebar";
import { useDesktopWorkspace } from "./hooks/useDesktopWorkspace";
import { desktopApi } from "./lib/desktop-api";
import { useRuntimeUpdates } from "./hooks/useRuntimeUpdates";
import { UpdateBanner } from "./features/settings/AboutPanel";
import { ReadinessBanner } from "./features/dashboard/ReadinessBanner";
import { useShellText } from "./i18n/runtime-shell";
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
  return <DesktopMantineProvider><DesktopApp /></DesktopMantineProvider>;
}

function DesktopApp() {
  const { t } = useLocale();
  const s = useShellText();
  const [settingsSection, setSettingsSection] = useState<"diagnostics" | "runtime" | undefined>();
  const { state, activity, navigation, setNavigation, refreshing, error, setError, cancelSubmittingId, showSetup, setShowSetup, setStartupAttempt, mainRef, commitState, openSetup, chooseLocalProject, refresh, resumeRuntime, cancelCurrentOperation, runStateOperation } = useDesktopWorkspace();
  const updates = useRuntimeUpdates(Boolean(state && !state.current_operation && !state.configuration_issue));
  const openSettings = (section: "diagnostics" | "runtime") => { setSettingsSection(section); setNavigation("settings"); };
  if (!state) {
    return (
      <main className="splash">
        <BrandMark />
        {error ? (
          <section className="startup-error" aria-label="WebCodex">
            <AppError error={error} />
            <Button
              className="primary-button"
              type="button"
              onClick={() => {
                setError(null);
                setStartupAttempt((attempt) => attempt + 1);
              }}
              data-webcodex-action="retry-desktop-startup"
            >
              {t("common.retry")}
            </Button>
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
              <Button
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
              </Button>
            )}
          </section>
        )}
        {error && <><AppError error={error} /><div className="shell-actions"><button type="button" className="secondary-button" onClick={() => openSettings("diagnostics")}>{s("Diagnostics")}</button><button type="button" className="text-button" onClick={() => openSettings("runtime")}>{s("Select another Runtime")}</button></div></>}
        {navigation !== "settings" && <ComputerPermissions welcome />}
        {navigation === "home" && <UpdateBanner updates={updates} />}
        {navigation === "home" && (state.topology || state.configuration_issue) && <ReadinessBanner state={state} onState={commitState} onDiagnostics={() => openSettings("diagnostics")} onRuntime={() => openSettings("runtime")} onConnection={() => setNavigation("connection")} />}
        {navigation === "home" && !state.configuration_issue && (needsSetup ? (
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
            onChangeSetup={openSetup}
            onNavigate={setNavigation}
            onStopQuickShare={() => void runStateOperation(desktopApi.stopQuickShare)}
            onStopRuntime={() => void runStateOperation(desktopApi.stopLocalRuntime)}
          />
        ))}
        {navigation === "projects" && (
          <ProjectsPanel state={state} onState={commitState} onChooseProject={() => void chooseLocalProject()} />
        )}
        {navigation === "connection" && <ConnectionPanel state={state} onState={commitState} />}
        {navigation === "activity" && <ActivityPanel activity={activity} />}
        {navigation === "extensions" && <ExtensionsPanel state={state} onState={commitState} />}
        {navigation === "settings" && <SettingsPanel state={state} onState={commitState} onChangeSetup={openSetup} onStopRuntime={() => void runStateOperation(desktopApi.stopLocalRuntime)} initialSection={settingsSection} onActivity={() => setNavigation("activity")} updates={updates} />}
      </main>
    </div></WorkspaceProvider>
  );
}

function AppError({ error }: { error: DesktopError }) {
  const { t } = useLocale();
  const presentation = desktopErrorPresentation(error, t);
  const diagnostics = desktopCommandDiagnostics(error);
  return (
    <Alert className="error-card app-error" role="alert" variant="light" color="red">
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
    </Alert>
  );
}
