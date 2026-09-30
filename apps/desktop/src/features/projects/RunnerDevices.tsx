import { Monitor } from "lucide-react";
import { useProduct } from "../../i18n/product";
import { useWorkspace } from "../workspace/WorkspaceContext";

export function RunnerDevices({ onComputerSettings }: { onComputerSettings?: () => void }) {
  const p = useProduct(); const workspace = useWorkspace();
  if (!workspace.runners.length) return null;
  return <section className="runner-devices" aria-labelledby="runner-devices-title">
    <header><h2 id="runner-devices-title">{p("executionDevices")}</h2><p>{p("runnerRole")}</p></header>
    <ul>{workspace.runners.map(runner => {
      const local = runner.client_id === workspace.state.workspace_runner?.client_id;
      const stale = workspace.fleetStale || runner.status === "stale";
      const desktop = !stale && runner.connected ? runner.computer_session_availability : undefined;
      const desktopLabel = stale ? "desktopNeedsRefresh" : !runner.connected ? "desktopUnconfirmed" : desktop === true ? "desktopConnected" : desktop === false ? "desktopUnavailable" : "desktopNotReported";
      const help = stale ? "deviceStaleHelp" : !runner.connected ? "deviceOfflineHelp" : desktop === true ? "desktopConnectedHelp" : desktop === false ? "desktopUnavailableHelp" : "desktopNotReportedHelp";
      return <li className="runner-device" key={runner.client_id} data-runner-id={runner.client_id}>
        <div className="runner-device-heading"><Monitor size={20} aria-hidden="true" /><h3>{local ? p("thisComputer") : p("otherDevice")}</h3><span className={`workspace-badge${runner.connected && !stale ? " working" : ""}`}>{p(stale ? "deviceNeedsRefresh" : runner.connected ? "runnerConnected" : "runnerDisconnected")}</span></div>
        {!local && <p className="runner-device-identifier">{p("deviceIdentifier")} · <code>{runner.client_id}</code></p>}
        <p className="runner-desktop-state">{p(desktopLabel)}</p>
        <p className="runner-device-help">{p(help)}</p>
        {local && runner.connected && !stale && desktop !== true && onComputerSettings && <button type="button" className="secondary-button" onClick={onComputerSettings}>{p("checkDesktopPermissions")}</button>}
        {local && <details className="runner-device-details"><summary>{p("deviceIdentifier")}</summary><code>{runner.client_id}</code></details>}
      </li>;
    })}</ul>
  </section>;
}
