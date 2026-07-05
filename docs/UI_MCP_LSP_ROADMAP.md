# UI, MCP, and LSP Strategic Roadmap

This is a docs-only research RFC. It records product and architecture judgment
for the next UI, MCP client/server, and LSP-oriented work. It does not authorize
runtime behavior changes, new dependencies, a frontend project, or a broader
OpenAPI surface by itself.

## Current Product Position

WebCodex is best understood as a self-hosted, policy-aware runtime for letting
model-facing clients work on registered projects through controlled tools. It is
not a general remote shell, not a hosted IDE, and not an arbitrary MCP
marketplace.

The current core capabilities are:

- A protocol-independent runtime tool execution layer in `ToolRuntime`, with
  transport adapters translating REST, GPT Action, and MCP calls into typed
  `ToolCall` dispatch.
- MCP and GPT Action adapters over the same runtime surface, with `ToolKernel`
  centralizing metadata-backed authorization, session event recording, parsing,
  and dispatch.
- Agent-registered projects using ids such as
  `agent:<client_id>:<project_id>`, where project execution stays on the agent
  host and is constrained by local project policy.
- A session ledger and handoff model through `start_session`,
  `start_coding_task`, `finish_coding_task`, `session_summary`, and
  `session_handoff_summary`. The ledger is task-continuity metadata, not a full
  audit log.
- A safe smoke workspace pattern based on disposable agent-backed projects,
  `capabilities.recommended_for_smoke`, git-backed smoke selection when git
  behavior is tested, and artifact paths under `artifacts/smoke/`.
- Tool manifest and OpenAPI discovery surfaces. MCP can expose full tool schemas
  directly, while GPT Actions must stay within the operation budget and should
  prefer `callRuntimeTool` plus `tool_manifest` for broad runtime discovery.
- Patch, file, git, job, checkpoint, and artifact workflows that already form a
  practical coding loop: inspect, edit with structured tools, validate, review
  with `show_changes`, checkpoint when needed, and finish with a session
  closeout.

The strategic implication is that UI, MCP client features, and LSP features
should strengthen this runtime contract. They should not become parallel control
planes that bypass runtime authority, project identity, or session records.

## UI Direction

WebCodex does need a UI, but not a full IDE first. The highest-leverage UI is an
operations and review surface that makes the existing runtime state visible:
which agents are online, which projects are available, what sessions are in
progress, what changed, and which jobs or permission decisions need attention.

The first UI should be read-only by default because it can improve operator
confidence without creating a second write path. It should consume the same
runtime status and project/session summaries that model-facing tools consume.

### P1 UI

P1 should be a read-only dashboard:

- `runtime_status` overview: server build/status, permission profile, tool
  count, active job counts, and project health.
- `list_agents` and `list_projects`: agent liveness, project ids,
  `recommended_for_smoke`, git availability, shell profile status, and redacted
  policy summaries.
- Session list and handoff view: session title, project, state, recent bounded
  events, open handoff messages, validation status, and closeout verdict.
- `show_changes` summary: git status, diff stat, untracked smoke/tmp/test hints,
  session activity summary, and links to bounded diff hunk review when available.

This should answer: "Is WebCodex healthy, which project is safe to use, what did
the agent do, and is the current task ready to hand off or close?"

### Later UI Areas

After the read-only dashboard proves useful, the next UI areas are:

- Diff review viewer backed by `show_changes`, `git_diff_summary`, and
  `git_diff_hunks`, with no direct file mutation path in the viewer initially.
- Job monitor backed by `job_status`, bounded `job_tail`, and session job
  summaries. Command text, stdout, stderr, and tails should stay bounded and
  redacted according to the existing tool outputs.
- Checkpoint browser that lists checkpoint metadata, scope, and restore risk
  before any restore or discard action is offered.
- Artifact browser for bounded artifact metadata and preview/download decisions,
  not an unbounded project file browser or object store.
- Permission and audit view for high-risk tool decisions and session summaries,
  but only after the auth model and audit boundaries are stable enough to avoid
  mixing bootstrap admin, PAT, shared-key, and agent identities in one loose UI.

### Do Not Build First

Do not start with:

- A full IDE.
- A web terminal.
- Arbitrary shell UI.
- Browser-based raw file manager with broad write authority.
- Multi-user admin screens before the managed auth model, shared-key bridge, and
  audit semantics are stable.

Those areas are attractive, but they would create authority and UX pressure
before WebCodex has a mature approval model for user-facing mutation.

## MCP Client And Server Direction

There are four different concepts that should stay separate:

- WebCodex as MCP server: the existing `/mcp` endpoint exposes WebCodex runtime
  tools to MCP clients.
- WebCodex as MCP client: a future capability where WebCodex consumes external
  MCP servers or local MCP helpers.
- GPT Action `callRuntimeTool`: the generic OpenAPI operation that reaches the
  same runtime when a dedicated GPT Action operation is not appropriate.
- Local desktop/IDE integration and remote agent bridge: local capabilities that
  may be attached to WebCodex through explicit adapters, not by letting models
  speak arbitrary local protocols directly.

### Reusable Existing Capabilities

The existing runtime should remain the source of truth:

- `ToolRuntime` and `ToolKernel` provide the common dispatch and authorization
  path.
- `ToolMetadata`, registry schemas, OAuth scope policy, MCP annotations, and
  OpenAPI accepted names already provide the consistency hooks.
- Agent-registered project ids and agent policy boundaries already model where
  work can happen.
- The session ledger already records model-facing tool activity, high-risk
  permission decisions, validation events, handoff state, and closeout summaries.
- Artifact, checkpoint, job, file, patch, and git domains already provide useful
  backend capabilities without needing an MCP provider marketplace first.

### Keeping MCP And GPT Actions Consistent

MCP server tools and GPT Action runtime calls must stay aligned through the
runtime registry, not through hand-written transport-specific behavior. Adding or
renaming a runtime tool must continue to update tool parsing, known tool names,
metadata, registry schemas, OAuth scope policy, OpenAPI accepted names/examples,
MCP schema tests, and consistency tests together.

MCP can expose more tools directly because `tools/list` is schema-oriented. GPT
Actions must keep the dedicated operation count constrained, so new runtime
tools should normally be available through `callRuntimeTool` first. Dedicated
GPT Action operations should be reserved for stable, common workflows where the
operation budget and product value are clear.

### MCP Client First Step

WebCodex should not jump straight to arbitrary external MCP-server brokering.
The safer first MCP-client-like step is a local helper or bridge:

- It runs near the desktop or IDE and can authenticate to the WebCodex server as
  a known principal.
- It advertises a small set of explicit capabilities such as IDE diagnostics,
  editor selection, or local symbol index summaries.
- It translates those capabilities into bounded WebCodex runtime/provider calls
  instead of exposing raw external MCP tools directly to a model-facing surface.
- It records activity through the same session id and permission model used by
  the rest of WebCodex.

This preserves the useful local integration path without making WebCodex a blind
broker for tools whose schemas, authority, output size, and side effects are not
yet described by WebCodex metadata.

### MCP Security Model

The MCP client/server boundary should keep these constraints:

- Authentication remains explicit: shared-key quick start, PAT, OAuth2 runtime
  tokens, and agent tokens keep their separate purposes.
- Workspace trust is project-scoped through agent-registered ids, allowed roots,
  and agent policy. A local helper should not introduce a second project identity
  system.
- Tool permission decisions stay metadata-backed, risk-classed, and
  session-recorded. Read-only views should not silently upgrade to write or shell
  authority.
- Session recording should use explicit session ids where possible and should not
  depend on fragile current-session binding for cross-client handoff.
- Outputs remain bounded and redacted. MCP cannot be treated as permission to
  dump full repositories, full logs, env values, credentials, or unbounded LSP
  payloads.

### Do Not Expose Directly

Do not directly expose these to model-facing MCP/GPT Action surfaces:

- `run_codex`, unless a future explicit opt-in designs its authority,
  containment, discovery, and audit story. It should remain hidden.
- Bootstrap/admin credential management, account credential creation, agent token
  management, pairing/enrollment, and server management.
- Raw shell consoles, raw process control, arbitrary desktop input, or
  open-ended web terminal behavior.
- Raw external MCP tool brokering without WebCodex metadata, scope policy,
  bounded output rules, and session recording.
- Raw LSP JSON-RPC, whole-repo semantic dumps, full env files, tokens,
  credentials, or sensitive paths.

## LSP Direction

WebCodex does not need to become a full built-in LSP implementation. The better
route is to consume existing language servers or IDE-derived diagnostics,
summarize the results, and expose bounded semantic capabilities through the
runtime.

The public WebCodex surface should use stable capability names such as
`code.symbols`, `code.diagnostics`, `code.references`, or `code.definition`
rather than exposing raw LSP protocol details. LSP should be treated as a
provider behind `ToolRuntime` and `ToolKernel`, not as a new model-facing
transport.

### P1 LSP Work

P1 should be design and bounded read-only capability work:

- Code navigation summary and symbol search design: define inputs, output
  limits, project/session metadata, and expected fallback behavior when no
  language server is configured.
- Diagnostics ingestion design: define how diagnostics are collected from
  structured validation tools, language servers, or local helpers without
  storing raw unbounded output.
- Bounded semantic context tools RFC: define read-only tools that can answer
  "what symbols exist here?", "what diagnostics are known?", and "where is this
  definition?" with explicit file/range limits.

P1 should not require WebCodex to launch and manage every language server itself.

### P2 LSP Work

P2 can add optional project-level integration:

- Per-project language server adapter configuration, likely agent-side because
  project files live on the agent host.
- Cached diagnostics keyed by project, file, version/hash, tool source, and
  timestamp.
- Safe references and definitions queries with response limits, path filtering,
  sensitive-path denial, and session recording.

### P3 LSP Work

P3 can explore richer IDE behavior:

- Refactor support such as rename or organize imports, guarded by preview,
  patch generation, and explicit apply steps.
- IDE-like UI integration where semantic navigation and diff review share a
  project/session view.
- Richer semantic providers such as tree-sitter, build-system indexes, or
  repository graph summaries when they can be bounded and audited.

## Cross-Direction Boundaries

These are hard boundaries for UI, MCP, and LSP work:

- UI must not bypass runtime authentication, authorization, permission policy, or
  session recording.
- MCP client features must not bypass `ToolRuntime` or `ToolKernel`; external or
  local capabilities need WebCodex metadata before becoming model-facing.
- LSP and semantic tools must not read secrets, env files, credentials, or whole
  repositories without explicit bounded queries.
- All model-facing actions must remain bounded, auditable, project-scoped,
  permission-aware, and redacted.
- `run_codex` remains hidden. UI, MCP, or LSP work must not automatically expose
  it through discovery, generic dispatch, or convenience wrappers.
- Smoke and validation flows should continue to prefer disposable, agent-backed
  projects and should not use production mutation as smoke coverage.

## Recommended Mainline Sequence

1. Ops/read-only UI. This makes the existing runtime observable without adding a
   new mutation surface. It also pressures the project, agent, session, and
   permission summaries to become stable product contracts.
2. Coding workflow closeout UX. `show_changes`, validation summaries,
   `finish_coding_task`, and `session_handoff_summary` already carry the data
   needed for a practical review loop. Improving the UX here gives immediate
   value to GPT Action and MCP users without building an IDE.
3. MCP server/client boundary docs. Before consuming external MCP servers,
   WebCodex needs clear rules for metadata, scopes, bounded outputs, local helper
   identity, workspace trust, and session recording.
4. Semantic/LSP bounded tools design. Design the stable capability layer before
   launching language servers. This keeps semantic features aligned with runtime
   authority and avoids leaking raw LSP complexity into model-facing tools.
5. Full UI/IDE later. A full IDE or web terminal should wait until auth,
   approval, audit, and semantic provider boundaries are stronger. Otherwise the
   UI will push WebCodex toward an unsafe parallel control plane.

This order favors observability and review first, then local integration and
semantic precision. It avoids investing in broad UI or IDE machinery before the
runtime contracts they would rely on are product-grade.

## Engineering Slices For Tomorrow

### 1. Dashboard Data Contract Doc

- Scope: Define a read-only dashboard data contract from existing
  `runtime_status`, `list_agents`, `list_projects`, `session_handoff_summary`,
  and `show_changes` outputs.
- Files likely touched: `docs/UI_MCP_LSP_ROADMAP.md` or a new focused docs file;
  possibly `docs/ARCHITECTURE.md` links.
- Tests: docs-only `git diff --check`; no cargo tests unless code changes.
- Risk: Low. Main risk is documenting fields that are not stable enough.
- Deploy needed: No.

### 2. Session List RFC

- Scope: Specify how a read-only session list should page, filter by project,
  summarize status, and link to handoff/finish details without becoming a full
  audit API.
- Files likely touched: docs first; later `src/tool_runtime/sessions.rs`,
  `src/tool_runtime/handoff.rs`, and session tests if a runtime list tool is
  added.
- Tests: docs-only now; later `cargo test --bin webcodex session -- --nocapture`
  and `cargo test --bin webcodex metadata -- --nocapture` if a tool is added.
- Risk: Medium. It can blur session ledger and audit log responsibilities.
- Deploy needed: No for docs; yes for runtime API changes.

### 3. Diff Review Viewer Shape

- Scope: Define a UI-friendly diff review model backed by `show_changes`,
  `git_diff_summary`, and `git_diff_hunks`, including limits and redaction
  behavior.
- Files likely touched: docs now; later runtime git metadata/tests only if output
  gaps are found.
- Tests: docs-only now; later focused git/show_changes tests if schema changes.
- Risk: Low to medium. Main risk is encouraging large diff payloads.
- Deploy needed: No for docs; yes for runtime output changes.

### 4. MCP Boundary Matrix

- Scope: Write a matrix that maps WebCodex-as-MCP-server,
  WebCodex-as-MCP-client, GPT Action `callRuntimeTool`, local helpers, and remote
  agents to auth, trust, session, and permission responsibilities.
- Files likely touched: `docs/MCP.md`, `docs/GPT_ACTIONS.md`,
  `docs/UI_MCP_LSP_ROADMAP.md`.
- Tests: `git diff --check`; no cargo tests for docs-only.
- Risk: Low. Product naming must stay precise.
- Deploy needed: No.

### 5. Local Helper Threat Model

- Scope: Define a local helper/bridge threat model for desktop or IDE summaries:
  principal identity, project trust, capability registration, redaction, and
  session recording.
- Files likely touched: docs first; later auth/runtime docs and tests if it
  becomes an implemented adapter.
- Tests: docs-only now; later auth/scope/metadata tests for implementation.
- Risk: Medium. A helper can accidentally become an unbounded local authority
  channel if not constrained.
- Deploy needed: No for docs; yes for implementation.

### 6. Semantic Tool Capability RFC

- Scope: Define `code.symbols`, `code.diagnostics`, `code.definition`, and
  `code.references` capability shapes without choosing an LSP process manager.
- Files likely touched: docs first; later `src/tool_runtime/types.rs`,
  registry metadata/schema modules, OAuth scope policy, MCP/OpenAPI tests if
  runtime tools are added.
- Tests: docs-only now; later metadata, MCP, OpenAPI, and consistency tests for
  any runtime tool.
- Risk: Medium. The names can become public contract too early.
- Deploy needed: No for docs; yes for runtime tools.

### 7. Diagnostics Ingestion Design

- Scope: Specify how validation-derived diagnostics, optional LSP diagnostics,
  and cached summaries should be normalized, bounded, and linked to sessions.
- Files likely touched: docs first; later validation summary code, session
  ledger code, and tool-runtime tests.
- Tests: docs-only now; later `cargo test --bin webcodex session -- --nocapture`
  and focused metadata tests.
- Risk: Medium. Diagnostics can leak file contents or become stale if cache keys
  are weak.
- Deploy needed: No for docs; yes for runtime storage/output changes.

### 8. Permission/Audit UI Readiness Review

- Scope: Inventory what permission decision and audit-like data is safe for a
  read-only UI before adding admin screens.
- Files likely touched: docs first; later auth/audit docs and route tests if a
  UI-facing read API is added.
- Tests: docs-only now; later auth, oauth, scope, and metadata tests for
  implemented routes.
- Risk: Medium to high. It can expose sensitive operational metadata or confuse
  task ledger data with security audit data.
- Deploy needed: No for docs; yes for implemented routes.

## Open Questions

- Should a future dashboard read directly from dedicated read-only HTTP routes,
  or should it call the same runtime tools through the generic tool API?
- How much session ledger history should be retained and paginated before it
  becomes an audit product with separate retention policy?
- What is the minimal local helper capability set that gives IDE value without
  creating an arbitrary desktop automation surface?
- Which semantic capability names should become stable public runtime tools, and
  which should remain provider-internal until proven?
