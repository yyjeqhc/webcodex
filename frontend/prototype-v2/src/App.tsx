import {
  Activity,
  AlertTriangle,
  ArrowUpRight,
  Bot,
  Check,
  ChevronDown,
  CircleDot,
  Clock3,
  Code2,
  Command,
  Folder,
  GitBranch,
  HardDrive,
  LayoutDashboard,
  LoaderCircle,
  MessageSquare,
  Monitor,
  MoreHorizontal,
  Play,
  Search,
  Server,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
  X,
} from "lucide-react";
import { useMemo, useState } from "react";

type View = "work" | "projects" | "runtime";
type WorkState = "attention" | "running" | "active" | "recent";

type WorkItem = {
  id: string;
  title: string;
  project: string;
  runner: string;
  branch: string;
  state: WorkState;
  status: string;
  updated: string;
  note?: string;
};

const workItems: WorkItem[] = [
  {
    id: "runtime-e2e",
    title: "Runtime E2E private-path hardening",
    project: "webcodex-14b70807",
    runner: "special",
    branch: "fix/runtime-private-path-e2e",
    state: "running",
    status: "Running reconnect E2E",
    updated: "now",
    note: "3/4 steps complete · job running",
  },
  {
    id: "telemetry",
    title: "Tool ergonomics production telemetry analysis",
    project: "sf server admin",
    runner: "sf",
    branch: "main",
    state: "active",
    status: "Analyzing SQLite evidence",
    updated: "now",
  },
  {
    id: "identifier",
    title: "Model-facing identifier economy follow-up",
    project: "webcodex",
    runner: "special",
    branch: "main",
    state: "attention",
    status: "Ready for issue review",
    updated: "7m",
  },
  {
    id: "runmesh",
    title: "Inspect runmesh architecture and control plane",
    project: "runmesh",
    runner: "special",
    branch: "main",
    state: "recent",
    status: "Review complete",
    updated: "1h",
  },
  {
    id: "codex-web",
    title: "Review codex-chatgpt-web architecture delta",
    project: "codex-chatgpt-web",
    runner: "special",
    branch: "main",
    state: "recent",
    status: "Review complete",
    updated: "2h",
  },
];

const files = [
  "scripts/e2e_zero_config_ws.sh",
  "scripts/e2e_reconnect_ws.sh",
  "scripts/test-runner-config-reload-e2e.sh",
  "src/tool_runtime/kernel.rs",
  "src/tool_runtime/tests/dispatch.rs",
  ".github/workflows/ci.yml",
];

function NavButton({
  active,
  icon,
  label,
  meta,
  onClick,
}: {
  active: boolean;
  icon: React.ReactNode;
  label: string;
  meta?: string;
  onClick: () => void;
}) {
  return (
    <button className={"nav-button" + (active ? " active" : "")} onClick={onClick}>
      <span className="nav-icon">{icon}</span>
      <span>{label}</span>
      {meta && <small>{meta}</small>}
    </button>
  );
}

function WorkRow({
  item,
  selected,
  onSelect,
}: {
  item: WorkItem;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button className={"work-row" + (selected ? " selected" : "")} onClick={onSelect}>
      <span className={"work-state-dot " + item.state} />
      <span className="work-row-body">
        <strong>{item.title}</strong>
        <span className="work-row-location">{item.project} · {item.runner}</span>
        <span className="work-row-status">{item.status}</span>
      </span>
      <time>{item.updated}</time>
    </button>
  );
}

function WorkGroup({
  title,
  items,
  selectedId,
  onSelect,
}: {
  title: string;
  items: WorkItem[];
  selectedId: string;
  onSelect: (id: string) => void;
}) {
  if (!items.length) return null;
  return (
    <section className="work-group">
      <div className="work-group-heading">
        <span>{title}</span>
        <small>{items.length}</small>
      </div>
      <div className="work-group-list">
        {items.map((item) => (
          <WorkRow key={item.id} item={item} selected={item.id === selectedId} onSelect={() => onSelect(item.id)} />
        ))}
      </div>
    </section>
  );
}

function ToolCluster({
  icon,
  title,
  meta,
  tone = "neutral",
  children,
}: {
  icon: React.ReactNode;
  title: string;
  meta: string;
  tone?: "neutral" | "good" | "warn";
  children?: React.ReactNode;
}) {
  return (
    <details className={"tool-cluster " + tone}>
      <summary>
        <span className="tool-cluster-icon">{icon}</span>
        <span className="tool-cluster-title">
          <strong>{title}</strong>
          <small>{meta}</small>
        </span>
        <ChevronDown size={15} />
      </summary>
      {children && <div className="tool-cluster-detail">{children}</div>}
    </details>
  );
}

function Timeline() {
  return (
    <div className="task-run">
      <section className="task-prompt">
        <div className="task-prompt-label"><MessageSquare size={14} /> Task</div>
        <p>Continue in the existing managed worktree. Finish the Runtime E2E private-path work, validate the reconnect path, and keep production behavior unchanged.</p>
      </section>

      <section className="run-status-card">
        <div className="run-status-head">
          <span className="run-spinner"><LoaderCircle size={17} /></span>
          <div>
            <strong>Working</strong>
            <span>Running focused validation · 6m 42s</span>
          </div>
          <span className="live-badge"><span /> live</span>
        </div>
        <div className="run-plan">
          <div className="run-step done"><span><Check size={13} /></span><strong>Inspect reconnect and private-path flow</strong><small>12 source locations</small></div>
          <div className="run-step done"><span><Check size={13} /></span><strong>Patch normalization and regression coverage</strong><small>5 files · +84 −19</small></div>
          <div className="run-step running"><span><LoaderCircle size={13} /></span><strong>Run reconnect E2E and focused Rust tests</strong><small>running 00:41</small></div>
          <div className="run-step queued"><span>4</span><strong>Review final diff and report result</strong><small>queued</small></div>
        </div>
      </section>

      <section className="active-command">
        <div className="active-command-head">
          <span><TerminalSquare size={15} /></span>
          <div><strong>Current command</strong><code>./scripts/e2e_reconnect_ws.sh</code></div>
          <span className="command-running"><LoaderCircle size={13} /> running</span>
        </div>
        <pre>
          <span>[runner] reconnecting special → control</span>
          <span>[runner] workspace fence verified</span>
          <span>[e2e] private-path round trip ........ ok</span>
          <span>[e2e] reconnect continuity ........... running</span>
        </pre>
        <div className="active-command-foot"><span>job wc_job_cVX0lx…</span><span>41s · output streaming</span></div>
      </section>

      <section className="progress-section">
        <div className="progress-heading"><span>Recent progress</span><small>Low-level calls grouped by intent</small></div>
        <div className="timeline-clusters">
          <ToolCluster icon={<Search size={16} />} title="Explored reconnect implementation" meta="12 source locations · 38s">
            <div className="file-grid">{files.map((file) => <code key={file}>{file}</code>)}</div>
          </ToolCluster>
          <ToolCluster icon={<Code2 size={16} />} title="Implemented private-path hardening" meta="5 files · +84 −19" tone="good">
            <ul>
              <li><code>src/tool_runtime/kernel.rs</code> — normalize private path handoff</li>
              <li><code>scripts/e2e_reconnect_ws.sh</code> — add reconnect regression</li>
              <li><code>.github/workflows/ci.yml</code> — exercise Windows path lane</li>
            </ul>
          </ToolCluster>
          <ToolCluster icon={<ShieldCheck size={16} />} title="Previous checks passed" meta="cargo check · unit tests · diff check" tone="good" />
        </div>
      </section>

      <article className="agent-working-note">
        <span className="message-avatar agent"><Bot size={15} /></span>
        <div>
          <div className="message-meta"><strong>Agent</strong><time>just now</time></div>
          <p>The implementation is stable. I’m waiting on the final reconnect E2E before reviewing the diff; no deploy or service restart will be performed.</p>
        </div>
      </article>
    </div>
  );
}

function Inspector({ item }: { item: WorkItem }) {
  const [tab, setTab] = useState<"context" | "evidence">("context");
  const isRunning = item.state === "running";
  return (
    <aside className="inspector">
      <div className="inspector-header">
        <div>
          <span className="eyebrow">Session context</span>
          <strong>{tab === "context" ? "What matters now" : "Raw evidence"}</strong>
        </div>
        <button className="icon-button"><X size={16} /></button>
      </div>
      <div className="segmented">
        <button className={tab === "context" ? "active" : ""} onClick={() => setTab("context")}>Context</button>
        <button className={tab === "evidence" ? "active" : ""} onClick={() => setTab("evidence")}>Evidence</button>
      </div>

      {tab === "context" ? (
        <div className="inspector-content">
          <section className="context-hero">
            <span className="context-kicker"><CircleDot size={14} /> {isRunning ? "Running" : "Needs review"}</span>
            <strong>{item.title}</strong>
            <p>{isRunning ? "The agent is actively running the final reconnect E2E. No user action is required yet." : "The implementation is stable and ready for the next decision."}</p>
          </section>

          <section className="inspector-section">
            <h3>Current work</h3>
            <div className="fact-list">
              <div><span>Project</span><strong>{item.project}</strong></div>
              <div><span>Runner</span><strong>{item.runner}</strong></div>
              <div><span>Branch</span><strong><GitBranch size={13} /> {item.branch}</strong></div>
              <div><span>Last activity</span><strong>just now</strong></div>
            </div>
          </section>

          <section className="attention-card">
            <div className="attention-title"><AlertTriangle size={16} /><strong>Attention</strong></div>
            <p>{isRunning ? "No blocking attention. The current Job is still producing validation evidence." : "Review the latest result before publishing or continuing."}</p>
            <button className="text-button">{isRunning ? "Watch current Job" : "Review history"}</button>
          </section>

          <section className="inspector-section">
            <h3>Validation</h3>
            <div className="validation-mini">
              <span className={"status-dot " + (isRunning ? "running" : "good")} />
              <div><strong>{isRunning ? "Reconnect E2E is running" : "All focused checks passed"}</strong><small>{isRunning ? "3/4 steps complete · live" : "Fresh · 12:40:21"}</small></div>
            </div>
          </section>
        </div>
      ) : (
        <div className="inspector-content evidence">
          <section className="inspector-section">
            <h3>Session identity</h3>
            <dl>
              <div><dt>Session</dt><dd><code>wc_sess_PR6eJIjigJLNBoRj</code></dd></div>
              <div><dt>Lifecycle</dt><dd>active</dd></div>
              <div><dt>Mode</dt><dd>normal</dd></div>
              <div><dt>Created</dt><dd>2026-09-20 08:55:23</dd></div>
              <div><dt>Updated</dt><dd>2026-09-20 12:40:21</dd></div>
            </dl>
          </section>
          <section className="inspector-section">
            <h3>Workspace</h3>
            <dl>
              <div><dt>Project</dt><dd><code>agent:special:webcodex-14b70807-5f71dfae</code></dd></div>
              <div><dt>Path</dt><dd><code>/root/git/.webcodex-managed-worktrees/webcodex-14b70807</code></dd></div>
            </dl>
          </section>
          <section className="inspector-section">
            <h3>Linked runtime evidence</h3>
            <button className="evidence-row">
              <span><Monitor size={15} /></span>
              <span><strong>Window 8f9903e1…2556</strong><small>openai-session · 157 records</small></span>
              <ArrowUpRight size={14} />
            </button>
            <button className="evidence-row">
              <span><Play size={15} /></span>
              <span><strong>Job wc_job_cVX0lx…</strong><small>terminal · exit 0</small></span>
              <ArrowUpRight size={14} />
            </button>
          </section>
        </div>
      )}
    </aside>
  );
}

function WorkView() {
  const [selectedId, setSelectedId] = useState("runtime-e2e");
  const selected = workItems.find((item) => item.id === selectedId) ?? workItems[0];
  const grouped = useMemo(() => ({
    running: workItems.filter((item) => item.state === "running"),
    attention: workItems.filter((item) => item.state === "attention"),
    active: workItems.filter((item) => item.state === "active"),
    recent: workItems.filter((item) => item.state === "recent"),
  }), []);

  return (
    <div className="work-layout">
      <aside className="work-list-panel">
        <div className="work-list-header">
          <div><span className="eyebrow">Workspace</span><h1>Work</h1></div>
          <button className="icon-button" title="Search"><Search size={16} /></button>
        </div>
        <div className="work-list-scroll">
          <WorkGroup title="Running" items={grouped.running} selectedId={selectedId} onSelect={setSelectedId} />
          <WorkGroup title="Needs attention" items={grouped.attention} selectedId={selectedId} onSelect={setSelectedId} />
          <WorkGroup title="Active" items={grouped.active} selectedId={selectedId} onSelect={setSelectedId} />
          <WorkGroup title="Recent" items={grouped.recent} selectedId={selectedId} onSelect={setSelectedId} />
        </div>
      </aside>

      <main className="session-main">
        <header className="session-header">
          <div className="session-heading">
            <div className="breadcrumbs"><span>{selected.runner}</span><span>/</span><span>{selected.project}</span></div>
            <h2>{selected.title}</h2>
          </div>
          <div className="session-actions">
            <span className={"quiet-pill " + (selected.state === "running" ? "running" : "")}><CircleDot size={12} /> {selected.state === "running" ? "working · 6m" : "idle · 46m"}</span>
            <button className="icon-button"><MoreHorizontal size={17} /></button>
          </div>
        </header>
        <div className="timeline-scroll">
          <div className="timeline-measure">
            <Timeline />
          </div>
        </div>
        <div className="composer-row">
          <div className="composer">
            <textarea placeholder="Send a message to this work session…" rows={1} />
            <div className="composer-footer">
              <button className="composer-action"><Sparkles size={15} /> Options</button>
              <button className="send-button"><ArrowUpRight size={16} /></button>
            </div>
          </div>
        </div>
      </main>

      <Inspector item={selected} />
    </div>
  );
}

function ProjectsView() {
  const projects = [
    { name: "WebCodex", runner: "special", branch: "main", work: 5, active: 3, status: "online", updated: "now" },
    { name: "webcodex-14b70807", runner: "special", branch: "fix/runtime-private-path-e2e", work: 1, active: 1, status: "working", updated: "now" },
    { name: "runmesh", runner: "special", branch: "main", work: 1, active: 0, status: "idle", updated: "1h" },
    { name: "sf server admin", runner: "sf", branch: "main", work: 2, active: 2, status: "online", updated: "now" },
  ];
  const [selectedProject, setSelectedProject] = useState("WebCodex");
  const sessions = [
    { title: "Runtime E2E private-path hardening", status: "running", phase: "reconnect E2E · 00:41", windows: 2, updated: "now" },
    { title: "Model-facing identifier economy follow-up", status: "attention", phase: "ready for issue review", windows: 1, updated: "7m" },
    { title: "WebUI v2 information architecture prototype", status: "working", phase: "editing prototype", windows: 2, updated: "now" },
  ];
  return (
    <main className="page">
      <header className="page-heading">
        <div><span className="eyebrow">Code context</span><h1>Projects</h1><p>Project-level view: repository state plus the active Sessions currently using it.</p></div>
        <button className="primary-button">Add project</button>
      </header>
      <div className="filter-bar"><Search size={16} /><input placeholder="Search projects, runners, refs…" /><button>All runners <ChevronDown size={14} /></button></div>
      <div className="project-grid">
        {projects.map((project) => (
          <button className={"project-card" + (selectedProject === project.name ? " selected" : "")} key={project.name} onClick={() => setSelectedProject(project.name)}>
            <div className="project-card-head">
              <span className="project-icon"><Folder size={18} /></span>
              <span><strong>{project.name}</strong><small>{project.runner}</small></span>
              <span className={"status-pill " + (project.status === "working" ? "warn" : "good")}>{project.status}</span>
            </div>
            <div className="project-card-body">
              <div><GitBranch size={14} /><span>{project.branch}</span></div>
              <div><MessageSquare size={14} /><span>{project.active} active · {project.work} total</span></div>
              <div><Clock3 size={14} /><span>updated {project.updated}</span></div>
            </div>
            <span className="project-open">{project.active ? "Inspect active Sessions" : "Open project"} <ArrowUpRight size={14} /></span>
          </button>
        ))}
      </div>

      <section className="project-sessions">
        <div className="section-heading">
          <div><h2>{selectedProject} · active Sessions</h2><p>One Project can host several concurrent Sessions; each Session may also be observed by multiple Windows.</p></div>
          <span className="quiet-pill">3 active</span>
        </div>
        <div className="session-table">
          {sessions.map((session) => (
            <button className="project-session-row" key={session.title}>
              <span className={"session-live-dot " + session.status} />
              <span className="project-session-main"><strong>{session.title}</strong><small>{session.phase}</small></span>
              <span className="project-session-windows"><Monitor size={13} /> {session.windows} windows</span>
              <span className={"status-pill " + (session.status === "attention" ? "warn" : "good")}>{session.status}</span>
              <time>{session.updated}</time>
              <ArrowUpRight size={14} />
            </button>
          ))}
        </div>
      </section>
    </main>
  );
}

function RuntimeView() {
  const initialMode = new URLSearchParams(window.location.search).get("runtime") === "windows" ? "windows" : "overview";
  const [mode, setMode] = useState<"overview" | "windows">(initialMode);
  const [selectedWindow, setSelectedWindow] = useState("8f9903e1…2556");
  const windows = [
    { id: "8f9903e1…2556", source: "openai-session", runner: "special", project: "WebCodex", sessions: 3, records: 157, updated: "now" },
    { id: "773fa0d4…2556", source: "openai-session", runner: "sf", project: "sf server admin", sessions: 2, records: 180, updated: "30s" },
    { id: "b6ef7251…26c3", source: "openai-session", runner: "msi", project: "Voice Notifications", sessions: 1, records: 28, updated: "7m" },
    { id: "f0abd204…4e61", source: "openai-session", runner: "special", project: "runmesh", sessions: 2, records: 74, updated: "1h" },
  ];
  const currentWindow = windows.find((window) => window.id === selectedWindow) ?? windows[0];
  const linkedSessions = [
    { title: "Runtime E2E private-path hardening", project: "webcodex-14b70807", relation: "recording", windows: 2, activity: "run_shell · now" },
    { title: "WebUI v2 information architecture prototype", project: "WebCodex", relation: "work_on_project", windows: 2, activity: "apply_text_edits · now" },
    { title: "Model-facing identifier economy follow-up", project: "WebCodex", relation: "recording", windows: 1, activity: "GitHub issues · 11m" },
  ];

  return (
    <main className="page runtime-page">
      <header className="page-heading runtime-heading">
        <div><span className="eyebrow">System evidence</span><h1>Runtime</h1><p>Infrastructure, Windows and low-level evidence stay available without competing with task-oriented Work.</p></div>
        <span className="quiet-pill"><span className="status-dot good" /> connected</span>
      </header>
      <div className="runtime-tabs">
        <button className={mode === "overview" ? "active" : ""} onClick={() => setMode("overview")}><Server size={15} /> Overview</button>
        <button className={mode === "windows" ? "active" : ""} onClick={() => setMode("windows")}><Monitor size={15} /> Window activity <span>156</span></button>
      </div>

      {mode === "overview" ? (
        <>
          <div className="runtime-metrics">
            <div><span><Server size={17} /> Runners</span><strong>4</strong><small>4 online · builds aligned</small></div>
            <div><span><Play size={17} /> Active jobs</span><strong>2</strong><small>0 need attention</small></div>
            <div><span><Monitor size={17} /> Observed windows</span><strong>156</strong><small>many-to-many Session evidence</small></div>
            <div><span><Bot size={17} /> Durable agents</span><strong>3</strong><small>3 endpoints ready</small></div>
          </div>
          <section className="runtime-section">
            <div className="section-heading"><div><h2>Runner fleet</h2><p>Execution capacity and source alignment.</p></div><button className="text-button">Diagnostics <ArrowUpRight size={13} /></button></div>
            {["special", "oe", "sf", "msi"].map((runner, index) => (
              <div className="runtime-row" key={runner}>
                <span className="runner-icon"><Monitor size={17} /></span>
                <span><strong>{runner}</strong><small>{index === 3 ? "Windows · Runner online" : "Linux · Runner online"}</small></span>
                <span className="runtime-row-meta">{index === 0 ? "2 jobs · 29 projects" : index === 2 ? "telemetry active" : "idle"}</span>
                <span className="status-pill good"><Check size={12} /> aligned</span>
              </div>
            ))}
          </section>
          <section className="runtime-section">
            <div className="section-heading"><div><h2>Recent runtime events</h2><p>Window and Job evidence remains one level below Work.</p></div><button className="text-button" onClick={() => setMode("windows")}>Open Window activity <ArrowUpRight size={13} /></button></div>
            <div className="event-log">
              <div><Activity size={15} /><span><strong>Window activity</strong><small>special · WebCodex · just now</small></span><time>now</time></div>
              <div><TerminalSquare size={15} /><span><strong>Job completed</strong><small>runtime-e2e · exit 0</small></span><time>2m</time></div>
              <div><HardDrive size={15} /><span><strong>Runner heartbeat</strong><small>sf · source aligned</small></span><time>4m</time></div>
            </div>
          </section>
        </>
      ) : (
        <div className="windows-workbench">
          <aside className="window-list">
            <div className="window-list-head"><div><strong>Observed windows</strong><small>Client-window evidence across authorized Projects</small></div><button className="icon-button"><Search size={15} /></button></div>
            {windows.map((window) => (
              <button className={"window-row" + (window.id === selectedWindow ? " selected" : "")} key={window.id} onClick={() => setSelectedWindow(window.id)}>
                <span className="window-icon"><Monitor size={15} /></span>
                <span className="window-row-main"><strong>Window {window.id}</strong><small>{window.source} · {window.runner}</small><small>{window.project}</small></span>
                <span className="window-row-side"><time>{window.updated}</time><small>{window.sessions} sessions</small></span>
              </button>
            ))}
          </aside>

          <section className="window-detail">
            <header className="window-detail-head">
              <div><span className="eyebrow">Window evidence</span><h2>Window {currentWindow.id}</h2><p>{currentWindow.source} · {currentWindow.runner} · last observed {currentWindow.updated}</p></div>
              <span className="quiet-pill">{currentWindow.records} records</span>
            </header>

            <section className="relation-summary">
              <div><strong>{currentWindow.sessions}</strong><span>linked Sessions</span></div>
              <div><strong>2</strong><span>Projects touched</span></div>
              <div><strong>2</strong><span>relation kinds</span></div>
              <p>A Window is observation evidence, not ownership. Sessions can link to several Windows, and one Window can observe several Sessions.</p>
            </section>

            <section className="window-section">
              <div className="section-heading"><div><h2>Linked Sessions</h2><p>Many-to-many correlation with current Project and relation evidence.</p></div></div>
              <div className="linked-session-list">
                {linkedSessions.map((session) => (
                  <button className="linked-session" key={session.title}>
                    <span className="session-live-dot working" />
                    <span className="linked-session-main"><strong>{session.title}</strong><small>{session.project}</small></span>
                    <span className="relation-pill">{session.relation}</span>
                    <span className="linked-window-count"><Monitor size={13} /> {session.windows} windows</span>
                    <span className="linked-session-activity">{session.activity}</span>
                    <ArrowUpRight size={14} />
                  </button>
                ))}
              </div>
            </section>

            <section className="window-section">
              <div className="section-heading"><div><h2>Recent activity</h2><p>Raw evidence is still available when diagnosing a specific Window.</p></div></div>
              <div className="window-event-stream">
                <div><span className="event-marker running" /><span><strong>run_process</strong><small>WebUI v2 prototype · working</small></span><time>now</time></div>
                <div><span className="event-marker good" /><span><strong>apply_text_edits</strong><small>WebCodex · completed</small></span><time>1m</time></div>
                <div><span className="event-marker good" /><span><strong>read_files</strong><small>WebCodex · 3 files</small></span><time>3m</time></div>
                <div><span className="event-marker" /><span><strong>GitHub issue creation</strong><small>identifier economy follow-up</small></span><time>11m</time></div>
              </div>
            </section>
          </section>
        </div>
      )}
    </main>
  );
}

export function App() {
  const [view, setView] = useState<View>(() => {
    const requested = new URLSearchParams(window.location.search).get("view");
    return requested === "projects" || requested === "runtime" ? requested : "work";
  });
  return (
    <div className="app-shell">
      <aside className="app-nav">
        <div className="brand">
          <span className="brand-mark">W</span>
          <span><strong>WebCodex</strong><small><span className="status-dot good" /> connected</small></span>
        </div>
        <nav>
          <NavButton active={view === "work"} icon={<LayoutDashboard size={18} />} label="Work" meta="3" onClick={() => setView("work")} />
          <NavButton active={view === "projects"} icon={<Folder size={18} />} label="Projects" onClick={() => setView("projects")} />
          <NavButton active={view === "runtime"} icon={<Server size={18} />} label="Runtime" onClick={() => setView("runtime")} />
        </nav>
        <div className="nav-spacer" />
        <button className="command-hint"><Command size={15} /><span>Commands</span><kbd>⌘K</kbd></button>
        <div className="profile">
          <span className="profile-avatar">Y</span>
          <span><strong>Current runtime</strong><small>special · main</small></span>
        </div>
      </aside>
      <section className="app-content">
        {view === "work" && <WorkView />}
        {view === "projects" && <ProjectsView />}
        {view === "runtime" && <RuntimeView />}
      </section>
    </div>
  );
}
