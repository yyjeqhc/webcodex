# WebCodex Runtime WebUI v2

## Visual language across renderers

Runtime WebUI, Desktop and Admin share a restrained glass visual language while keeping their independent bundles. Semantic canvas, solid surface, glass, text, border, status, radius and shadow values have light and dark variants. System appearance is the default; the chosen appearance is stored only as a non-secret local preference. Main reading surfaces, logs, tables and form fields remain opaque. Blur is limited to navigation, composer chrome and dialogs, with opaque fallbacks for unsupported or reduced-transparency environments. Reduced motion and visible keyboard focus apply in all three renderers.

Runtime keeps Work, Projects and Runtime as its primary destinations. Desktop groups its six existing destinations as Work and Configure without changing their IDs or shortcuts. Admin provides section navigation for Overview, Devices / Agents, Projects and Diagnostics; its wide data tables scroll inside labelled regions on narrow screens. Visual changes do not alter backend authority, credential handling or mutation behavior.

Runtime WebUI is the remote WebCodex workbench. It is not the Desktop surface and it is not a browser for backend domain tables. Desktop owns local lifecycle, secure connections, permissions, Skills/Plugins/MCP installation and machine-local settings. Runtime WebUI answers what work is happening, where it is happening, what needs attention, and what evidence supports that state.

## Primary information architecture

The only primary destinations are:

1. **Work** — task execution and Session continuation.
2. **Projects** — repository inventory and Project -> active Sessions.
3. **Runtime** — infrastructure evidence, Window Activity, Runner health and durable Agent diagnostics.

Workflow Sessions, Window Activity, Jobs, Agents and diagnostics are subordinate dimensions. They are never promoted into additional top-level product concepts.

## Work

Work is the default destination. It projects retained Workflow Session facts into an execution workspace rather than rendering a tool-call transcript.

The left rail is grouped as Running / Needs attention / Active / Recent. A row emphasizes task title, Project, Runner, current evidence-backed phase and update time. Running is derived only from an active call or active Job. Attention is derived only from retained open guidance/questions/risks/todos. A closed Session is not presented as successful unless validation/report evidence says so.

The center workspace presents task identity, current execution state, retained work counters, current call or Job evidence, grouped recent progress, model-reported progress, retained Session communication and the composer.

Low-level activity is grouped as Explored / Edited / Ran / Tested / Reviewed / Delegated / Waiting. Grouping is a deterministic presentation transform. It never upgrades Window recency into Session progress, never converts a timestamp into liveness, and never invents a phase or completion state.

The right inspector is Context-first. Context shows Project, Runner, branch, Jobs, attention and current validation. Evidence exposes canonical Session ID, Project ref/path, timestamps and linked Windows.

## Projects

Projects answers “what work is happening in this repository?”

Project inventory is server-side searchable and Runner-filterable. Cards show readable Project identity, Runner, branch/path summary, connectivity and active Session count. Opening a Project shows its active/recent Sessions. A Project may contain multiple Sessions, and each Session may be observed by zero, one or multiple Windows.

Project Git and Session Window counts are bounded enrichments. They are independently authorized and cancellable. Add Project preserves the existing resolve-or-register authority contract and never automatically replays an uncertain write.

## Runtime

Runtime contains Overview, Window Activity and Agents.

Overview shows only authoritative infrastructure facts already available from the Runtime Console APIs: Runner fleet, source/build alignment, active Jobs, visible Workflow Session counts, observed Windows and durable Agent inventory when communication:read is authorized.

Window Activity is an observation workbench. Window identity is evidence, not ownership. The relationship model is:

    Project <-> Session <-> Window

Both edges are many-to-many. A Window may observe several Sessions; a Session may be observed by several Windows. Relation kinds such as recording and work_on_project explain correlation, never ownership.

Agents keeps durable Agent identity, Endpoint readiness, Conversations and endpoint-scoped Inbox diagnostics under Runtime. Browser Endpoint attach/renew/detach remains window-local control state; durable Agent identity remains server-owned. Communication read/manage authority stays independent from Project and Runtime authority.

## Server state and stale semantics

React components do not fetch wire endpoints directly. Production layers are:

- api/ — existing HTTP contracts and credentials.
- model/ — bounded deterministic presentation projection.
- state/ — cancellable server-state hooks, stale/denied/error handling and view-scoped polling.
- components/ — reusable presentation units.
- views/ — Work / Projects / Runtime composition.

The last authorized successful snapshot may remain visible after an ordinary refresh failure and is explicitly marked stale. A 401 returns to the credential boundary. A 403/404 authority transition clears revoked detail rather than presenting it as current.

Polling is scoped to visible/meaningful state. Runtime Overview and Project inventory refresh at low frequency. Project Sessions refresh while that Project workspace is active. Window Activity refreshes more frequently only while its workbench is active. A closed Session with no active call or Job is not continuously polled.

## Browser storage

Runtime credentials may be retained only in tab-scoped sessionStorage. They are never written to localStorage or cookies. Lock clears the remembered credential.

Per-Project/Session drafts use sessionStorage. Language, appearance and the primary view are non-secret preferences and may use localStorage.

## Accessibility and responsive behavior

Core information never depends on hover. Interactive controls have visible focus treatment and native keyboard semantics. Reduced-motion and reduced-transparency preferences are respected.

At desktop widths Work uses three regions when space permits: Session list, execution workspace and Context inspector. At 1280 px the inspector may collapse to protect the execution workspace. Narrow screens retain all three primary destinations through a fixed bottom navigation and stack complex workbenches vertically rather than deleting functionality.

## Build and production integration

Runtime production assets are React + TypeScript + Vite and are committed as deterministic:

- frontend/dist/runtime.html
- frontend/dist/app.js
- frontend/dist/styles.css

The Server embeds those files and serves them at /runtime, /runtime/app.js and /runtime/styles.css. Production does not depend on a Vite development server.

The old Runtime classic concatenation bundle has been retired. frontend/scripts/build.mjs remains only for the Admin console. Runtime WebUI continues to reuse the small shared runtime_api.ts, runtime_storage.ts and runtime_i18n.ts modules.

## Multi-Agent extension point

Work presentation items allow optional actor identity and do not assume one Agent per Session. Future Coordinator / Worker / Reviewer events can therefore be projected into the same grouped timeline without changing the primary information architecture. AgentTask, Goal, Conversation and handoff events remain evidence inside Work or Runtime rather than new primary destinations.

## Runtime freshness and inspectable identity

The overview and Project Session lists refresh every five seconds while visible,
and immediately on focus or visibility return. Slow overview requests finish before
another poll begins; previous data stays visible after refresh failures. Runtime
shows the last successful sync time and a manual refresh action. Independent
server status and Runner inventory reads run concurrently.

Window headers and Session context expose selectable, complete canonical IDs with
copy actions and explicit clipboard failure feedback. Window Session filters include
all retained exact relations, even those without retained calls; selecting a Session
can open its full record using its exact Project relation. Retention limits remain
visible and never imply a complete history.

Runtime displays the existing allowlisted effective configuration projection
(authentication switches, MCP host wait budgets in seconds, request tracing), with
readable labels and canonical parameter names. It never reads raw environment
variables or displays credentials; the existing runtime-read boundary applies.
