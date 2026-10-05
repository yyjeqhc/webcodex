import { useState } from "react";
import { desktopApi } from "../../lib/desktop-api";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { useShellText } from "../../i18n/runtime-shell";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import type { DesktopError, DesktopState } from "../../models/topology";
import { statusKey } from "../workspace/WorkspaceStatus";

export function LocalServicesPanel({ state, onState }: { state: DesktopState; onState: (state: DesktopState) => void }) {
  const p = useProduct(); const s = useShellText(); const { t } = useLocale();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const environmentId = state.persistent_environment?.trim();
  const disabled = busy || Boolean(state.current_operation);
  const act = async (component: "server" | "runner", action: "start" | "stop" | "restart" | "repair_credential") => {
    if (!environmentId || disabled) return;
    setBusy(true); setError(null);
    try { onState(await desktopApi.environmentServiceAction({ environmentId, component, action })); }
    catch (value) { setError(normalizeDesktopError(value)); }
    finally { setBusy(false); }
  };
  const presentation = error && desktopErrorPresentation(error, t);
  return <div className="service-status-list" data-testid={environmentId ? "persistent-services" : undefined}>
    {(["server", "runner"] as const).filter(component => component === "server" || state.topology?.runner?.kind === "local").map(component => {
      const local = state.topology?.[component]?.kind === "local";
      const readiness = state.readiness[component];
      const running = readiness === "ready";
      const transitioning = readiness === "starting" || readiness === "connecting";
      const primary = running ? "restart" : "start";
      return <div className="service-status-row" key={component}>
        <div><h3>{p(component === "server" ? "serverConnection" : "localExecutionService")}</h3><span className="workspace-badge">{p(statusKey(state.readiness[component]))}</span></div>
        {environmentId && local && <div className="shell-actions">
          <button type="button" className="secondary-button" disabled={disabled || transitioning} onClick={() => void act(component, primary)} aria-label={`${p(primary)} ${s(component === "server" ? "Local Server" : "Local Runner")}`}>{p(primary)}</button>
          <details className="workspace-technical"><summary>{p("advanced")}</summary>
            <button type="button" className="secondary-button" disabled={disabled || readiness === "stopped"} onClick={() => void act(component, "stop")} aria-label={`${p("stop")} ${s(component === "server" ? "Local Server" : "Local Runner")}`}>{p("stop")}</button>
            {component === "runner" && state.can_repair_runner_credential && <><button type="button" className="secondary-button" disabled={disabled} onClick={() => void act("runner", "repair_credential")}>{s("Repair Runner credential")}</button><p className="field-help">{s("Credential repair uses the native operating system prompt.")}</p></>}
          </details>
        </div>
        }
      </div>;
    })}
    {environmentId && <p className="field-help">{p("serviceRecoveryHelp")}</p>}
    {presentation && <div className="error-card" role="alert"><strong>{presentation.title}</strong><span>{presentation.action}</span><details><summary>{p("details")}</summary><code>{error?.code}</code></details></div>}
  </div>;
}
