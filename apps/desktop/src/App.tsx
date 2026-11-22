import { useState } from "react";
import { WorkspaceProvider } from "./features/workspace/WorkspaceContext";
import { Button } from "@mantine/core";
import { DesktopMantineProvider } from "./components/DesktopMantineProvider";
import { DesktopErrorAlert, type DesktopErrorActions } from "./components/DesktopErrorAlert";
import { BrandMark } from "../../../frontend/src/ui/BrandMark";
import { ExtensionsPanel, type ExtensionTab } from "./features/extensions/ExtensionsPanel";
import { ComputerPermissions } from "./features/settings/ComputerPermissions";
import { Sidebar } from "./components/Sidebar";
import { useDesktopWorkspace } from "./hooks/useDesktopWorkspace";
import { desktopApi } from "./lib/desktop-api";
import { useRuntimeUpdates } from "./hooks/useRuntimeUpdates";
import { UpdateBanner } from "./features/settings/AboutPanel";
import { ReadinessBanner } from "./features/dashboard/ReadinessBanner";
import { FirstRunCompletion } from "./features/onboarding/FirstRunCompletion";
import { FirstRun } from "./features/onboarding/FirstRun";
import { Dashboard } from "./features/dashboard/Dashboard";
import { ProjectsPanel } from "./features/projects/ProjectsPanel";
import { ConnectionPanel } from "./features/connection/ConnectionPanel";
import { ActivityPanel } from "./features/activity/ActivityPanel";
import { SettingsPanel } from "./features/settings/SettingsPanel";
import { RuntimeConsolePanel } from "./features/console/RuntimeConsolePanel";
import { useLocale } from "./i18n/locale";
import { operationLabel } from "./i18n/presentation";

export default function App() {
  return <DesktopMantineProvider><DesktopApp /></DesktopMantineProvider>;
}

function DesktopApp() {
  const { t } = useLocale();
  const [completionEnvironment, setCompletionEnvironment] = useState<string | null>(null);
  const [connectionFocus, setConnectionFocus] = useState<"chatgpt" | "gemini">();
  const [extensionTab, setExtensionTab] = useState<ExtensionTab>();
  const [settingsSection, setSettingsSection] = useState<"diagnostics" | "runtime" | "network" | "access" | undefined>();
  const { state, activity, navigation, previousPage, setNavigation, refreshing, preserveWorkspacePollDeadline, error, setError, cancelSubmittingId, showSetup, setupOrigin, closeSetup, setStartupAttempt, mainRef, commitState, openSetup, refresh, resumeRuntime, cancelCurrentOperation, runStateOperation } = useDesktopWorkspace();
  const updates = useRuntimeUpdates(Boolean(state && !state.current_operation && !state.configuration_issue));
  const openSettings = (section: "diagnostics" | "runtime" | "network" | "access") => { setSettingsSection(section); setNavigation("settings"); };
  if (!state) {
    return (
      <main className="splash">
        <BrandMark />
        {error ? (
          <section className="startup-error" aria-label="WebCodex">
            <DesktopErrorAlert error={error} showSafeMessage className="app-error" />
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
  const shellErrorActions: DesktopErrorActions = {
    onRefresh: () => void refresh(),
    onRetry: () => void refresh(),
    onStartRuntime: () => void resumeRuntime(),
    onStartServer: () => void resumeRuntime(),
    onRestartServer: () => openSettings("runtime"),
    onStartRunner: () => void resumeRuntime(),
    onRestartRunner: () => openSettings("runtime"),
    onRepairRunnerCredential: () => openSettings("runtime"),
    onRepairUserCredential: () => openSettings("diagnostics"),
    onCheckConnection: () => setNavigation("connection"),
    onOpenNetwork: () => openSettings("network"),
    onOpenRuntime: () => openSettings("runtime"),
    onOpenDiagnostics: () => openSettings("diagnostics"),
    onOpenActivity: () => setNavigation("activity"),
    onOpenProjects: () => setNavigation("projects"),
    onOpenSetup: openSetup,
    onRestoreConfiguration: () => openSettings("diagnostics"),
    onManageProviders: () => { setExtensionTab("mcpProviders"); setNavigation("extensions"); },
  };

  return (
    <WorkspaceProvider state={state} suspended={refreshing} preservePollDeadline={preserveWorkspacePollDeadline}><div className="app-shell">
      <Sidebar state={state} navigation={navigation} setNavigation={setNavigation} />

      <main className="main-content" ref={mainRef} tabIndex={-1}>
        {navigation === "settings" && previousPage === "connection" && <button className="back-button" onClick={() => setNavigation("connection")}>{t("setup.backConnections")}</button>}
        {navigation === "home" && showSetup && (state.topology || setupOrigin !== "home") && (
          <button className="back-button" onClick={closeSetup}>
            <span aria-hidden="true">← </span>{t(setupOrigin === "settings" ? "setup.backSettings" : setupOrigin === "connection" ? "setup.backConnections" : "home.backToOverview")}
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
        {error && !(navigation === "home" && state.runtime_error?.code === error.code) && (
          <DesktopErrorAlert
            error={error}
            actions={shellErrorActions}
            disabled={refreshing || Boolean(state.current_operation)}
            showSafeMessage
            className="app-error"
          />
        )}
        {navigation !== "settings" && !needsSetup && <ComputerPermissions welcome />}
        {navigation === "home" && !needsSetup && <UpdateBanner updates={updates} />}
        {navigation === "home" && (state.configuration_issue || (!needsSetup && !completionEnvironment && (state.topology || state.runtime_error))) && <ReadinessBanner state={state} onState={commitState} onDiagnostics={() => openSettings("diagnostics")} onRuntime={() => openSettings("runtime")} onConnection={() => setNavigation("connection")} onActivity={() => setNavigation("activity")} onProjects={() => setNavigation("projects")} onSetup={openSetup} onRefresh={() => void refresh()} onProviders={kind => { setExtensionTab(kind === "mcp" ? "mcpProviders" : "codingAgents"); setNavigation("extensions"); }} />}
        {navigation === "home" && !state.configuration_issue && (!needsSetup && completionEnvironment !== null && completionEnvironment === (state.environment_setup?.environment_id ?? state.persistent_environment) && state.topology?.experience === "full" ? (
          <FirstRunCompletion onComplete={() => setCompletionEnvironment(null)}
            onProjects={() => { setCompletionEnvironment(null); setNavigation("projects"); }}
            onConnection={() => { setCompletionEnvironment(null); setNavigation("connection"); }} />
        ) : needsSetup ? (
          <FirstRun
            state={state}
            onState={commitState}
            onDiagnostics={() => openSettings("diagnostics")}
            chooseModeFirst={showSetup}
            onComplete={next => {
              if (!setupOrigin && next?.readiness.runtime_ready) { setNavigation("connection"); return; }
              closeSetup();
              setCompletionEnvironment(next?.readiness.runtime_ready && (!setupOrigin || setupOrigin === "home") ? next?.environment_setup?.environment_id ?? next?.persistent_environment ?? null : null);
            }}
          />
        ) : (
          <Dashboard
            state={state}
            refreshing={refreshing}
            onRefresh={refresh}
            activity={activity}
            onConnection={client => { setConnectionFocus(client); setNavigation("connection"); }}
            onNavigate={setNavigation}
            onStopQuickShare={() => void runStateOperation(desktopApi.stopQuickShare)}
          />
        ))}
        {navigation === "projects" && (
          <ProjectsPanel onComputerSettings={() => openSettings("access")} onState={commitState} />
        )}
        {navigation === "connection" && <ConnectionPanel initialClient={connectionFocus} key={state.persistent_environment} state={state} onState={commitState} onSettings={openSettings} />}
        {navigation === "activity" && <ActivityPanel activity={activity} />}
        {navigation === "extensions" && <ExtensionsPanel state={state} onState={commitState} initialTab={extensionTab} />}
        {navigation === "console" && <RuntimeConsolePanel state={state} onSettings={() => openSettings("runtime")} />}
        {navigation === "settings" && <SettingsPanel state={state} onState={commitState} onChangeSetup={() => { setSettingsSection("runtime"); openSetup(); }} onStopRuntime={() => void runStateOperation(desktopApi.stopLocalRuntime)} initialSection={settingsSection} onActivity={() => setNavigation("activity")} onConnection={() => setNavigation("connection")} updates={updates} />}
      </main>
    </div></WorkspaceProvider>
  );
}
