import { useEffect, useState } from "react";
import { RunnerCapacitySummary } from "../../components/RunnerCapacitySummary";
import { runnerCapacity } from "../../lib/runner-capacity";
import { Monitor } from "lucide-react";
import { useProduct } from "../../i18n/product";
import { useWorkspace } from "../workspace/WorkspaceContext";

export function RunnerDevices({ onComputerSettings, selectedDevice, onSelectDevice }: { onComputerSettings?: () => void; selectedDevice?: string; onSelectDevice?: (id: string) => void }) {
  const p = useProduct(); const workspace = useWorkspace();
  const [now, setNow] = useState(Date.now);
  const observedAt = workspace.fleetObservedAt;
  useEffect(() => {
    const updateClock = () => setNow(Date.now());
    updateClock();
    const timer = observedAt == null ? undefined : window.setTimeout(updateClock, Math.max(0, observedAt + 30000 - Date.now()));
    document.addEventListener("visibilitychange", updateClock);
    return () => { if (timer !== undefined) window.clearTimeout(timer); document.removeEventListener("visibilitychange", updateClock); };
  }, [observedAt]);
  const capacityExpired = observedAt == null || Math.max(now, Date.now()) - observedAt >= 30000;
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
        <RunnerCapacitySummary capacity={runnerCapacity(runner, stale || workspace.busy || capacityExpired)} />
        <p className="runner-desktop-state">{p(desktopLabel)}</p>
        <p className="runner-device-help">{p(help)}</p>
        {onSelectDevice && <button type="button" className="secondary-button" aria-pressed={selectedDevice === runner.client_id}
          aria-label={`${p("viewDeviceProjects")} · ${runner.client_id}`} onClick={() => onSelectDevice(runner.client_id)}>{p("viewDeviceProjects")}</button>}
        {local && runner.connected && !stale && desktop !== true && onComputerSettings && <button type="button" className="secondary-button" onClick={onComputerSettings}>{p("checkDesktopPermissions")}</button>}
        {local && <details className="runner-device-details"><summary>{p("deviceIdentifier")}</summary><code>{runner.client_id}</code></details>}
      </li>;
    })}</ul>
  </section>;
}
