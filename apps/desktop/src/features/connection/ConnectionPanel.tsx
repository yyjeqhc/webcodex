import type { DesktopState } from "../../models/topology";

export function ConnectionPanel({ state }: { state: DesktopState }) {
  return (
    <section className="page-section">
      <div className="eyebrow">Connection</div>
      <h1>ChatGPT reachability</h1>
      <p className="lede">Reachability is separate from WebCodex permissions and execution mode. A Tunnel does not grant additional authority.</p>

      <div className="connection-current detail-card">
        <span className="section-kicker">Current</span>
        <strong>{currentConnection(state)}</strong>
        <p>{state.readiness.exposure === "remote_ready" ? "External endpoint readiness is verified." : "No verified external endpoint is active for the regular Service."}</p>
      </div>

      <div className="section-heading">
        <div><span className="section-kicker">Regular Service tunnels</span><h2>Next desktop slice</h2></div>
        <span className="planned-badge">D2</span>
      </div>
      <div className="two-column-grid">
        <article className="detail-card muted-card">
          <strong>OpenAI Secure MCP Tunnel</strong>
          <p>{state.openai_tunnel_configured ? "Required environment is configured; credential values stay in Rust and are never returned here." : "Required environment is not currently detected."}</p>
          <span>Regular Server lifecycle integration is intentionally not faked in D1.</span>
        </article>
        <article className="detail-card muted-card">
          <strong>Cloudflare</strong>
          <p>Quick Share already reuses the canonical Cloudflare lifecycle.</p>
          <span>Regular Server foreground tunnel ownership remains D2.</span>
        </article>
      </div>

      <details className="advanced-enrollment detail-card">
        <summary>Advanced enrollment: shared key / existing profile</summary>
        <p>
          The existing <code>webcodex connect</code> profile remains the lifecycle owner for shared-key setups.
          Desktop D1 does not duplicate its credential storage and never treats a Server bootstrap token as a Runner or MCP credential.
        </p>
      </details>
    </section>
  );
}

function currentConnection(state: DesktopState) {
  const exposure = state.topology?.exposure;
  if (!exposure || exposure.kind === "none") return "Local only";
  if (exposure.kind === "existing_https") return `Existing HTTPS · ${exposure.url}`;
  if (exposure.kind === "cloudflare") return "Cloudflare Quick Share";
  return "OpenAI Secure Tunnel Quick Share";
}

