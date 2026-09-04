import type { DesktopState } from "../../models/topology";

export function ProjectsPanel({ state }: { state: DesktopState }) {
  return (
    <section className="page-section">
      <div className="eyebrow">Projects</div>
      <h1>Projects on this computer</h1>
      <p className="lede">Desktop only manages folders you explicitly choose. It does not scan sibling repositories or the whole disk.</p>
      {state.project ? (
        <article className="detail-card">
          <span className="section-kicker">Configured project</span>
          <strong>{state.project.path}</strong>
          <dl className="detail-list">
            <div><dt>Status</dt><dd>{state.readiness.project.replaceAll("_", " ")}</dd></div>
            <div><dt>Allowed root</dt><dd>{state.project.allowed_root}</dd></div>
            <div><dt>Git</dt><dd>{state.project.is_git_repository ? "Repository detected" : "Not required"}</dd></div>
          </dl>
        </article>
      ) : (
        <div className="empty-state">No project has been configured yet.</div>
      )}
    </section>
  );
}

