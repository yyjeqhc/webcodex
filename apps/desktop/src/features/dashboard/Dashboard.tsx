import type { ActivityEntry, DesktopState } from "../../models/topology";
import type { TunnelConnection } from "../../models/connections-tools";
import { Button } from "@mantine/core";
import { ArrowRight, Folder, History, MessageSquare } from "lucide-react";
import { useProduct } from "../../i18n/product";
import { useLocale } from "../../i18n/locale";
import { activityMessage } from "../../i18n/presentation";
import { ChatgptObservationText } from "../workspace/WorkspaceStatus";
import type { ChatgptActivityQueryStatus } from "../../lib/desktop-observation";
import { useWorkspace } from "../workspace/WorkspaceContext";
import { useCloudflareControl } from "../connection/useCloudflareControl";
import { defaultGeminiProfile, verifiedPublicOrigin } from "../connection/gemini-status";

function GeminiStatus({ profile, ready }: { profile: TunnelConnection; ready: boolean }) {
  const { t } = useLocale();
  const { status, failure } = useCloudflareControl(profile, ready);
  if (failure || status?.lifecycle === "error" || status?.lifecycle === "disconnected") return t("common.error");
  if (!ready || status?.lifecycle === "stopped") return t("appLink.off");
  if (!status) return t("common.checking");
  if (status.lifecycle === "starting") return t("appLink.start");
  return t(verifiedPublicOrigin(profile, status, true) ? "appLink.ready" : "appLink.check");
}

interface DashboardProps {
  state: DesktopState;
  chatgptActivityQueryStatus?: ChatgptActivityQueryStatus;
  activity: ActivityEntry[];
  refreshing: boolean;
  onRefresh: () => Promise<void>;
  onNavigate: (page: "projects" | "activity") => void;
  onConnection: (client: "chatgpt" | "gemini") => void;
  onStopQuickShare: () => void;
}
export function Dashboard({ state, chatgptActivityQueryStatus, activity, refreshing, onRefresh, onNavigate, onConnection, onStopQuickShare }: DashboardProps) {
  const p = useProduct();
  const { t, formatTime } = useLocale();
  const workspace = useWorkspace();
  const profiles = state.connections?.profiles ?? [];
  const openai = profiles.filter(profile => !profile.provider || profile.provider.kind === "openai");
  const gemini = defaultGeminiProfile(profiles);
  const chatgpt = state.connections?.config_error ? "common.error"
    : openai.some(profile => profile.ready) && state.readiness.runtime_ready ? "appLink.ready"
    : openai.some(profile => profile.lifecycle === "starting") ? "appLink.start"
    : openai.some(profile => profile.lifecycle === "error" || profile.health === "degraded") ? "common.error"
    : state.topology?.server.kind === "remote" || !state.connections ? "appLink.check" : "appLink.off";
  const latest = [...activity].reverse().find(entry => !entry.event_kind.startsWith("process_"));
  return <section className="page-section workspace-page dashboard-page" aria-labelledby="home-title" data-webcodex-page="home">
    <header className="page-heading-row">
      <div><h1 id="home-title">{p("overview")}</h1>
      </div>
      <Button variant="default" className="secondary-button" disabled={Boolean(state.current_operation) || refreshing} onClick={() => void onRefresh()}>{p("refresh")}</Button>
    </header>
    <div className="dashboard-shortcuts">
      <button type="button" onClick={() => onConnection("chatgpt")}><MessageSquare aria-hidden="true" /><strong>ChatGPT</strong><span>{t(chatgpt)}<br /><ChatgptObservationText state={state} queryStatus={chatgptActivityQueryStatus} /></span><ArrowRight aria-hidden="true" /></button>
      <button type="button" onClick={() => onConnection("gemini")}><MessageSquare aria-hidden="true" /><strong>Gemini</strong><span>{gemini ? <GeminiStatus key={JSON.stringify([gemini.configuration_id, gemini.revision])} profile={gemini} ready={Boolean(state.persistent_environment && state.readiness.server === "ready")} /> : t(state.connections?.config_error ? "common.error" : "appLink.off")}</span><ArrowRight aria-hidden="true" /></button>
      <button type="button" onClick={() => onNavigate("projects")}><Folder aria-hidden="true" /><strong>{p("projects")}</strong><span>{workspace.projects.length}</span><ArrowRight aria-hidden="true" /></button>
      <button type="button" onClick={() => onNavigate("activity")}><History aria-hidden="true" /><strong>{p("desktopEvents")}</strong><span>{latest ? activityMessage(latest, t) + " · " + formatTime(latest.timestamp_ms) : p("noDesktopEvents")}</span><ArrowRight aria-hidden="true" /></button>
    </div>
    {state.topology?.experience === "quick_share" && state.quick_share && <button className="secondary-button" onClick={onStopQuickShare} disabled={Boolean(state.current_operation)}>{p("stop")} Quick Share</button>}
  </section>;
}
