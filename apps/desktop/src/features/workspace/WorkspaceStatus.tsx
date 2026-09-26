import { useLocale } from "../../i18n/locale";
import { productText, useProduct, type ProductKey } from "../../i18n/product";
import type { DesktopState } from "../../models/topology";
import { EMPTY_CONNECTIONS } from "../../models/connections-tools";
import { useConnectionsTools } from "../../i18n/connections-tools";

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
export function WorkspaceStatus({ state }: { state: DesktopState }) {
  const p = useProduct(); const c = useConnectionsTools();
  const connections = state.connections ?? EMPTY_CONNECTIONS;
  const hasLocalRunner = state.topology?.runner?.kind !== "none";
  const values: Array<[string, string]> = [["Server", state.readiness.server]];
  values.push(hasLocalRunner ? ["Runner", state.readiness.runner] : ["Local Runner", "notConfigured"]);
  if (state.topology?.experience === "quick_share") values.push(["Quick Share", state.readiness.exposure === "remote_ready" ? "ready" : state.readiness.exposure]);
  return <dl className="workspace-status-strip" aria-label={p("workspace")} role="status">
    {values.map(([name, status]) => <div key={name}><dt>{name === "Local Runner" ? p("localRunner") : name}</dt><dd><i className={`status-dot ${status === "ready" ? "ready" : status === "error" ? "error" : "unknown"}`} aria-hidden="true" />{status === "notConfigured" ? p("notConfigured") : p(statusKey(status))}</dd></div>)}
    {state.topology?.experience !== "quick_share" && <div><dt>{c("connections")}</dt><dd><i className={`status-dot ${connections.running > 0 ? "ready" : "unknown"}`} aria-hidden="true" />{connections.running} / {connections.profiles.length} {p("running")}</dd></div>}
  </dl>;
}
export function ChatgptObservation({ state }: { state: DesktopState }) {
  const p = useProduct(); const { locale } = useLocale();
  const at = state.chatgpt_activity?.last_meaningful_activity_at_ms;
  return <p className="workspace-observation">{at ? <>{p("lastChatgpt")} · <time dateTime={new Date(at).toISOString()}>{observationTime(at, locale)}</time></> : p("noChatgpt")}</p>;
}
