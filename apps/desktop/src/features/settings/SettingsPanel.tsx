import type { DesktopState } from "../../models/topology";

export function SettingsPanel({ state }: { state: DesktopState }) {
  return (
    <section className="page-section">
      <div className="eyebrow">Settings</div>
      <h1>Desktop diagnostics</h1>
      <p className="lede">Advanced build and integration details. Runtime credentials are intentionally absent.</p>
      <article className="detail-card">
        <span className="section-kicker">WebCodex binaries</span>
        {state.binaries ? (
          <dl className="detail-list">
            <div><dt>Version</dt><dd>{state.binaries.version}</dd></div>
            <div><dt>Source revision</dt><dd>{state.binaries.git_commit}</dd></div>
            <div><dt>Directory</dt><dd>{state.binaries.directory}</dd></div>
            <div><dt>Resolution</dt><dd>{state.binaries.source}</dd></div>
          </dl>
        ) : (
          <p>Binaries are checked when setup or diagnostics first needs them.</p>
        )}
      </article>
      <article className="detail-card">
        <span className="section-kicker">Runtime identity</span>
        <p>{state.project?.runtime_project_id ?? "Not established yet."}</p>
      </article>
      <article className="detail-card">
        <span className="section-kicker">Secrets</span>
        <strong>OS-native persistence is not enabled in D1</strong>
        <p>Local pairing stays in Rust memory. A remote one-time code is held only for the active form submission and then cleared. OpenAI Tunnel values are exposed to the UI only as configured/not configured. No secret is written to Desktop JSON state or localStorage.</p>
      </article>
    </section>
  );
}

