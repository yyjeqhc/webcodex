import type { DesktopState } from "../../models/topology";
import { Button } from "@mantine/core";
import { ArrowRight, Folder, History, MessageSquare, Puzzle } from "lucide-react";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { useLocale } from "../../i18n/locale";
import { useWorkspace } from "../workspace/WorkspaceContext";
import { WorkspaceStatus, ChatgptObservation } from "../workspace/WorkspaceStatus";

interface DashboardProps {
  state: DesktopState; refreshing: boolean; onRefresh: () => Promise<void>;
  onResumeRuntime: () => void;
  onChangeSetup: () => void;
  onNavigate: (page: "projects" | "connection" | "activity" | "extensions") => void;
  onStopQuickShare: () => void;
}
export function Dashboard(props: DashboardProps) {
  const { state, onNavigate } = props;
  const p = useProduct(); const c = useConnectionsTools(); const { t } = useLocale();
  const workspace = useWorkspace();
  const busy = Boolean(state.current_operation);
  return <section className="page-section workspace-page dashboard-page" aria-labelledby="home-title" data-webcodex-page="home">
    <header className="page-heading-row">
      <h1 id="home-title">{p("overview")}</h1>
      <Button className="secondary-button" variant="default" disabled={busy || props.refreshing} onClick={() => void props.onRefresh()}>{p("refresh")}</Button>
    </header>
    <WorkspaceStatus state={state} />
    {!state.readiness.runtime_ready && <div className="workspace-quick-actions"><Button className="secondary-button" variant="default" onClick={state.readiness.next_action_kind === "restart_quick_share" ? props.onChangeSetup : props.onResumeRuntime} disabled={busy}>{state.readiness.next_action_kind === "restart_quick_share" ? p("restart") + " Quick Share" : p("start") + " WebCodex"}</Button></div>}
    <div className="dashboard-handoff">
      <MessageSquare size={24} aria-hidden="true" />
      <div><h2>{p("continueInChatgpt")}</h2><p>{p("chatgptWorkflow")}</p><ChatgptObservation state={state} /></div>
      <Button className="secondary-button" variant="default" aria-label={`${p("manage")} ${c("connections")}`} onClick={() => onNavigate("connection")}>{c("connections")}<ArrowRight size={16} aria-hidden="true" /></Button>
    </div>
    <div className="dashboard-shortcuts">
      <button type="button" onClick={() => onNavigate("projects")}><Folder aria-hidden="true" /><strong>{p("projects")}</strong><span>{workspace.projects.length} · {p("projectShortcut")}</span><ArrowRight size={16} aria-hidden="true" /></button>
      <button type="button" onClick={() => onNavigate("activity")}><History aria-hidden="true" /><strong>{p("activity")}</strong><span>{p("activityShortcut")}</span><ArrowRight size={16} aria-hidden="true" /></button>
      <button type="button" onClick={() => onNavigate("extensions")}><Puzzle aria-hidden="true" /><strong>{t("extensions.title")}</strong><span>{p("extensionShortcut")}</span><ArrowRight size={16} aria-hidden="true" /></button>
    </div>
    {state.topology?.experience === "quick_share" && state.quick_share && <button className="secondary-button" onClick={props.onStopQuickShare} disabled={busy}>{p("stop")} Quick Share</button>}
  </section>;
}
