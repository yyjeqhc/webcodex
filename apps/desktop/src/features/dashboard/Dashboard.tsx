import type { DesktopState } from "../../models/topology";

interface DashboardProps {
  state: DesktopState;
  refreshing: boolean;
  onRefresh: () => void;
  onStopQuickShare: () => void;
  onStopRuntime: () => void;
}

export function Dashboard({
  state,
  refreshing,
  onRefresh,
  onStopQuickShare,
  onStopRuntime,
}: DashboardProps) {
  const isQuickShare = state.topology?.experience === "quick_share";
  return (
    <section className="page-section">
      <div className="page-heading-row">
        <div>
          <div className="eyebrow">Home</div>
          <h1>WebCodex</h1>
          <p className="lede">{state.readiness.summary}</p>
        </div>
        <button className="secondary-button" onClick={onRefresh} disabled={refreshing}>
          {refreshing ? "Checking…" : "Refresh"}
        </button>
      </div>

      <div className="status-grid">
        <StatusCard
          title="Service"
          value={serviceLabel(state)}
          state={state.readiness.server}
          explanation={serviceExplanation(state)}
        />
        <StatusCard
          title="Runner"
          value={runnerLabel(state.readiness.runner)}
          state={state.readiness.runner}
          explanation="The Runner executes work for projects on this computer."
        />
        <StatusCard
          title="Projects"
          value={state.readiness.project === "ready" ? "1 ready" : projectLabel(state.readiness.project)}
          state={state.readiness.project}
          explanation={state.project?.path ?? "No project selected"}
        />
        <StatusCard
          title="ChatGPT Connection"
          value={connectionLabel(state)}
          state={state.readiness.exposure}
          explanation={connectionExplanation(state)}
        />
      </div>

      <div className={`readiness-banner ${state.readiness.ready_for_chatgpt ? "ready" : "pending"}`}>
        <div>
          <span className="section-kicker">Overall</span>
          <strong>{state.readiness.ready_for_chatgpt ? "Ready to use" : state.readiness.summary}</strong>
        </div>
        {state.readiness.next_action && <span>{state.readiness.next_action}</span>}
      </div>

      {state.quick_share && (
        <div className="handoff-card">
          <div>
            <span className="section-kicker">Quick Share handoff</span>
            <strong>{state.quick_share.ready_for_chatgpt ? "Connection handoff ready" : "Handoff needs action"}</strong>
            {state.quick_share.mcp_url && <code>{state.quick_share.mcp_url}</code>}
            <span>{quickShareClipboardLabel(state.quick_share.clipboard_state, state.quick_share.clipboard_contains)}</span>
          </div>
          <button className="danger-button" onClick={onStopQuickShare}>Stop Share</button>
        </div>
      )}

      {!isQuickShare && state.topology && (
        <div className="runtime-actions">
          <span>Desktop stops only processes that it started. Existing user-managed processes are left alone.</span>
          <button className="secondary-button" onClick={onStopRuntime}>Stop Desktop-owned runtime</button>
        </div>
      )}
    </section>
  );
}

function StatusCard({
  title,
  value,
  state,
  explanation,
}: {
  title: string;
  value: string;
  state: string;
  explanation: string;
}) {
  const tone = state === "ready" || state === "remote_ready" ? "ready" : state === "error" ? "error" : state === "unknown" ? "unknown" : "pending";
  return (
    <article className="status-card">
      <span className="section-kicker">{title}</span>
      <div className="status-value"><i className={`status-dot ${tone}`} />{value}</div>
      <p>{explanation}</p>
    </article>
  );
}

function serviceLabel(state: DesktopState) {
  if (state.readiness.server === "ready") return state.topology?.server.kind === "remote" ? "Connected" : "Running";
  return state.readiness.server.replaceAll("_", " ");
}

function serviceExplanation(state: DesktopState) {
  if (state.topology?.server.kind === "remote") return `Existing Server · ${state.topology.server.url}`;
  return "WebCodex Service on this computer.";
}

function runnerLabel(value: string) {
  return value === "ready" ? "Connected" : value.replaceAll("_", " ");
}

function projectLabel(value: string) {
  return value.replaceAll("_", " ");
}

function quickShareClipboardLabel(state: string, contains: string) {
  if (state !== "copied") return "Clipboard handoff is unavailable.";
  if (contains === "tunnel_id") return "OpenAI Tunnel handoff copied to the clipboard.";
  if (contains === "bearer_credential") return "Temporary connection credential copied to the clipboard.";
  if (contains === "sensitive_mcp_url") return "Temporary connection URL copied to the clipboard.";
  return "Connection handoff copied to the clipboard.";
}

function connectionLabel(state: DesktopState) {
  const exposure = state.topology?.exposure;
  if (!exposure || exposure.kind === "none") return "Local only";
  if (exposure.kind === "cloudflare") return "Cloudflare";
  if (exposure.kind === "open_ai_tunnel") return "OpenAI Secure Tunnel";
  return "Existing HTTPS";
}

function connectionExplanation(state: DesktopState) {
  if (state.readiness.exposure === "remote_ready") return "The Server endpoint is ready for a ChatGPT connection.";
  if (state.readiness.exposure === "local_ready") return "Runtime is healthy; external reachability has not been configured.";
  return "External reachability is not fully verified.";
}

