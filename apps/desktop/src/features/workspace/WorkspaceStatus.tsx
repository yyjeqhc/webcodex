import { useLocale } from "../../i18n/locale";
import { productText, useProduct, type ProductKey } from "../../i18n/product";
import type { DesktopState } from "../../models/topology";
import type { ConnectionsSnapshot } from "../../models/connections-tools";

export function observationTime(timestamp: number | null | undefined, locale: string, now = Date.now()): string {
  if (!timestamp) return productText(locale, "noActivity");
  const seconds = Math.min(0, Math.round((timestamp - now) / 1000));
  const format = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  if (seconds > -60) return format.format(seconds, "second");
  if (seconds > -3600) return format.format(Math.round(seconds / 60), "minute");
  if (seconds > -86_400) return format.format(Math.round(seconds / 3600), "hour");
  return format.format(Math.round(seconds / 86_400), "day");
}
export function statusKey(status: string | undefined): ProductKey {
  if (status === "ready" || status === "running") return "running";
  if (status === "starting" || status === "connecting") return "starting";
  if (status === "error" || status === "unavailable") return "unavailable";
  if (!status || status === "stopped" || status === "disabled") return "stopped";
  return "unknown";
}
export function tunnelReadinessText(connections: ConnectionsSnapshot | undefined, p: (key: ProductKey) => string): string {
  if (!connections || connections.config_error) return p("tunnelStatusUnavailable");
  if (!connections.profiles.length) return p("noTunnels");
  return p("tunnelsReadyCount").replace("{ready}", String(connections.running)).replace("{total}", String(connections.profiles.length));
}
// These observations describe the configured Desktop environment, not every
// service running on this machine or the Server's external connectivity.
export function desktopStatusPresentation(state: DesktopState, p: (key: ProductKey) => string) {
  const localRunnerConfigured = state.topology?.runner?.kind === "local";
  const quickShare = state.topology?.experience === "quick_share";
  const quickShareStatus = state.readiness.exposure === "remote_ready" ? "ready" : state.readiness.exposure;
  return {
    localRunnerConfigured,
    primaryLabel: p(localRunnerConfigured ? "localExecutionService" : "serverConnection"),
    primaryStatus: localRunnerConfigured ? state.readiness.runner : state.readiness.server,
    primaryReady: localRunnerConfigured ? state.readiness.runtime_ready : state.readiness.server === "ready",
    connectionLabel: quickShare ? "Quick Share" : p("tunnels"),
    connectionText: quickShare ? `Quick Share · ${p(statusKey(quickShareStatus))}` : tunnelReadinessText(state.connections, p),
    connectionReady: quickShare ? quickShareStatus === "ready" : Boolean(state.connections && !state.connections.config_error && state.connections.running > 0),
    externalConnectionUnobserved: !quickShare && state.topology?.server.kind === "remote",
  };
}
export function WorkspaceStatus({ state }: { state: DesktopState }) {
  const p = useProduct();
  const presentation = desktopStatusPresentation(state, p);
  const values: Array<[string, string]> = [[p("serverConnection"), state.readiness.server]];
  if (presentation.localRunnerConfigured) values.push([p("localExecutionService"), state.readiness.runner]);
  if (state.topology?.experience === "quick_share") values.push(["Quick Share", state.readiness.exposure === "remote_ready" ? "ready" : state.readiness.exposure]);
  return <dl className="workspace-status-strip" aria-label={p("workspace")} role="status">
    {values.map(([name, status]) => <div key={name}><dt>{name}</dt><dd><i className={`status-dot ${status === "ready" ? "ready" : status === "error" ? "error" : "unknown"}`} aria-hidden="true" />{p(statusKey(status))}</dd></div>)}
    {state.topology?.experience !== "quick_share" && <div><dt>{presentation.connectionLabel}</dt><dd><i className={`status-dot ${presentation.connectionReady ? "ready" : "unknown"}`} aria-hidden="true" />{presentation.connectionText}</dd></div>}
  </dl>;
}
export function ChatgptObservation({ state }: { state: DesktopState }) {
  const p = useProduct(); const { locale } = useLocale();
  const at = state.chatgpt_activity?.last_meaningful_activity_at_ms;
  return <p className="workspace-observation">{at ? <>{p("lastChatgpt")} · <time dateTime={new Date(at).toISOString()}>{observationTime(at, locale)}</time></> : p("noChatgpt")}</p>;
}
