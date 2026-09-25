# Agent Architecture Decisions

Standing design context for agents working on WebCodex. **Executable constraints
live in [`AGENTS.md`](../../AGENTS.md).** This file explains durable product
structure so agents do not re-litigate settled shape during ordinary tasks.

Related product docs: [`ARCHITECTURE.md`](../ARCHITECTURE.md),
[`../architecture/durable-agent-runtime.md`](../architecture/durable-agent-runtime.md),
[`TESTING.md`](../TESTING.md).

---

## 1. Session dual model (architecture, not an operation checklist)

WebCodex has **two different "session" concepts**. They share a name in casual
speech but are **not interchangeable** and must not be merged by accident.
Full naming, lifecycle, compatibility, and non-goals:
[`session-model.md`](session-model.md).

### Workflow session (coding / tool ledger)

| Aspect | Contract |
|---|---|
| ID form | `wc_sess_*` |
| Purpose | Coding-task workflow: start/finish coding task, tool events, validation evidence, handoff |
| Storage | In-memory ledger with durable JSON-oriented session records (product surface for MCP / runtime tools) |
| Identity rules | Existing Workflow Session effects require an explicit business `session_id` or authorized wrapper `recording_session_id`; unknown ids fail closed and omission never infers a Session |
| Mutation policy | `normal` uses ordinary authority/permission rules; `read_only` denies write-like and shell/job-like tools; guard denial before mutation. The pre-0.4 `inspect` Session mode is retired and malformed persisted v2 rows remain row-closed rather than becoming Normal. |

Do **not** change `wc_sess_*` ID format, ledger event shape, or lifecycle
semantics casually. Session / guard / explicit-targeting work must preserve the
invariants linked from `AGENTS.md` §6 (domain rules) and the Session model.

### Durable Agent and collaboration boundary (standing)

A Workflow Session remains a bounded **execution and evidence unit** even when its
session-local message board is used for coordinator/worker handoff. Do not turn
`wc_sess_*` lifecycle, ownership, or evidence history into a generic chat room,
Agent identity, task queue, worker pool, or scheduler.

The independent durable collaboration domain now exists: Server-minted Agents,
replaceable Agent Endpoints, Conversations, Messages, recipient Deliveries, and
Wake Intents/Attempts. The earlier `Room/Discussion` placeholder is superseded by
this concrete Agent/Conversation model. Standing rules are:

- a durable Agent is not a browser window, MCP connection, credential, Runner,
  Runtime Project, or Workflow Session;
- Conversation participation governs communication only. It never confers Project,
  Workflow Session, Job, Artifact, shell, Computer, CodingAgent, or filesystem
  authority;
- Message, Delivery, Attention Event, Wake, and execution are separate durable facts. Message/read
  state never proves model-context retention; Event records a bounded semantic fact; Wake is only a reasoning/processing opportunity and never proves Agent Task or Goal completion;
- each concrete execution may still use an independent Workflow Session for tool
  calls, validation, Jobs, checkpoints, and review evidence; pure communication
  does not require an execution Session;
- the Session message board remains the explicit manual coordinator/worker handoff
  substrate. It is not migrated into Conversation and its todo semantics are not an
  Agent Task lease;
- the asynchronous work object is an independent **Agent Task** with an exact fenced **Agent TaskAttempt**. It is not a Workflow Session todo, Job, or Conversation Message and is not inferred merely because one of those exists;
- **Goal** is an independent `wc_goal_*` high-level durable intent/control domain. It is not an Agent Task, Workflow Session, Job, Project selector, execution primitive, or scheduler; Goal identity/status/revision/correlation is never a bearer credential;
- Goal selection is exact durable identity or explicit creation only. Never infer the current Goal from Project, ClientWindow, credential, MCP/OpenAI session data, Conversation membership, Workflow Session, or shared timing;
- Goal lifecycle is currently closed to `active | completed | cancelled`. `finish_coding_task`, AgentTask/TaskAttempt completion, Job terminal state, or validation evidence do not automatically transition a Goal;
- ClientWindow liveness may be projected only as soft observational evidence from an exact authorized Goal through explicit Workflow Session correlations and re-authorized Project visibility. `last_seen` and `last_meaningful_activity` are distinct; five minutes without visible meaningful WebCodex activity may request human attention but is not proof of model failure and never mutates Goal/Task/Attempt state or authority;
- the first durable **Attention Event** kind is narrowly `agent_task_terminal`. Event is a semantic terminal fact, not a generic bus, scheduler, authority snapshot, or copied business payload. Exact TaskAttempt terminalization commits the required per-active-Goal Event/Wake facts atomically with Task/Attempt completion and keyed replay;
- `attention_event` Wake is distinct from A4b `agent_task_attempt` Wake: the former targets the completed Task's explicit assignee for Goal re-evaluation and never requires the terminal Attempt to heartbeat/hold a live lease; the latter still means execute one exact active fenced Attempt;
- the resumed attention turn must independently re-read exact Goal and AgentTask truth through ordinary authorization and explicitly decide Goal progression. Neither terminal Task outcome nor Event/Wake consumption auto-completes/reopens a Goal or auto-creates a successor Task;
- references among Goal, Conversation, Agent Task, Workflow Session, Job, CodingAgentRun,
  commit, PR, or Artifact provide correlation only. Dereferencing always re-runs
  the referenced object's normal authorization;
- automatic worker spawning, runnable-frontier scheduling, capacity management,
  dependency graphs, or orchestration are optional later control layers, not the
  definition of a durable Agent and not authority sources.

The standing Agent/asynchronous-work design is documented in
[`../architecture/durable-agent-runtime.md`](../architecture/durable-agent-runtime.md).
Current communication/wake implementation details live in
[`../architecture/durable-agent-conversation.md`](../architecture/durable-agent-conversation.md).
Current bounded Session handoff behavior remains documented in
[`manual-window-collaboration.md`](manual-window-collaboration.md).

### Action audit session (HTTP / operator audit)

| Aspect | Contract |
|---|---|
| ID form | UUID (HTTP action audit session) |
| Purpose | Operator/API action audit trail, idle timeout, transport-level audit |
| Storage | SQLite-backed audit records (not the coding ledger) |
| Isolation | Separate from workflow sessions; no automatic cross-reference today |

When docs or code say "session", identify which kind is meant. Cross-wiring
workflow ledger APIs to audit UUIDs (or the reverse) is a design change, not a
drive-by fix.

### One coding runtime (standing)

WebCodex has one coding runtime: ordinary ToolRuntime over Runner-registered Projects, Workflow Sessions, canonical Jobs, and normal read/search/edit/Git/validation/process/shell tools. `webcodex share` and `webcodex run` are lifecycle/auth/reachability conveniences around that runtime; they do not define Task/Run/Execution/Result/Approval business objects or a second model-facing tool registry.

Project-scoped credentials and project-share OAuth authenticate to a `ProjectGrant`. Runner visibility, canonical Project resolution, OAuth scopes, and normal permission policy enforce that boundary. Direct Adaptive tools and `call_runtime_tool` gateway dispatch share the same authority path. A guessed Runner/Project id grants no visibility and must not become an existence oracle. Project Agent Tokens remain Runner-transport credentials only.

Project-scoped model/API credentials cannot use ordinary Project registry mutation to turn broad Runner filesystem policy into new coding authority. `register_project`, `create_project`, and `unregister_project` fail closed for those credentials; path-based coding may reuse only an exact Project already present in the caller-visible ProjectGrant inventory.

Managed worktrees have one implementation: `work_on_project(mode=worktree)` asks the Runner to derive and register a normal managed-worktree Project from an already-visible source Project under Runner filesystem policy. Without an isolation request, local `share`/`run` uses its already registered Project.

### Correlation decision (standing)

If the two systems are linked later, use **optional, explicit, one-way**
correlation only:

| Decision | Choice |
|---|---|
| Direction | Action Audit → `workflow_session_id: Option<String>` (`wc_sess_*`) |
| Authority | Store on the audit side (prefer event/record); Workflow Session does not own an Action Audit id list |
| Lifecycle | Independent; audit never drives Workflow create/close/guards |
| Inference | Forbidden from current Action Session, time, thread, connection, window identity, or other implicit Workflow Session selection |
| Missing field | Keep unlinked behavior |
| Bad format | Parameter error; never silent remap |

The optional correlation contract is summarized above; dual-model lifecycle and
identity rules live in [`session-model.md`](session-model.md).

### Authority decision layer (standing)

Authority is a **decision layer** for whether a consequential tool invocation
may proceed under the active mode. It is **not** a Workflow Session manager,
not Action Audit, and not lifecycle tracing.

| Layer | Owns |
|---|---|
| Authority | auto-authorize / deny outcomes (`trusted_agent` \| `restricted`) |
| Workflow Session | task context and bounded evidence (`wc_sess_*`) |
| Action Audit | HTTP/operator action facts (SQLite) |
| Lifecycle trace | optional request-path observation |

**Default mode is `trusted_agent`** (self-hosted single-operator product
default): no human wait, no approval interruptions, every permission-bearing
call still records an auditable decision. Hard safety (path, secrets, session
guards, scopes, agent policy) remains independent and **not overridable** by
authority mode.

**Implementation:** module under `src/tool_runtime/permissions/`;
authoritative single evaluation at ToolRuntime **dispatch** before mutation;
kernel reuses the attached decision and does not re-evaluate. Modes:
`trusted_agent` auto-authorizes after hard safety; `restricted` denies runtime
tools; unknown values and any set legacy `WEBCODEX_PERMISSION_MODE` fail
closed (see §6). Full contract: [`permission-model.md`](permission-model.md).

---

## 2. Runtime/tool contract evolution (background)

WebCodex is an **internal / self-use** project. There are no supported external
API consumers, public SDKs, or third-party stable clients of the model-facing
runtime tool surface today.

Standing executable rules (also summarized in `AGENTS.md` and expanded in
[`tool-contract-guidelines.md`](tool-contract-guidelines.md)):

1. Do not retain compatibility fields for hypothetical consumers.
2. Do not emit both a canonical field and an alias field for the same concept.
3. Do not add deprecated aliases, legacy fallbacks, dual-output shapes, or
   version-translation layers without a concrete migration requirement.
4. When duplicate representations are found, choose one canonical structured
   representation and delete the others from outputs, schemas, tests, and docs
   in the same change.
5. Do not reject a recognized semantically inert parameter merely to enforce a
   presentation/resource bound that can be safely normalized or clamped. Strict
   rejection belongs to semantic ambiguity, authority, identity/fence, effect,
   privacy, and retry-safety boundaries.
6. Optimize for model-turn economy only after preserving truth: server-known
   mechanical repair may continue in the same call; unknown intent, uncertain
   effects, or missing authority must never be guessed to save a turn.

Before keeping any compatibility layer, name a **specific consumer** or a
**specific public/durable contract**. A `version` (or parser version) field may
identify protocol shape; it is not a reason to keep duplicate or alias fields.
Historical persisted truth is a separate concern from current model-facing tool
shape and must not be rewritten merely because the current tool contract changes.

When external stable consumers genuinely exist later, revise this decision
explicitly and define a bounded migration window for that concrete contract.

---

## 3. Test organization guidance

Executable editing rules for tests live in `AGENTS.md`. Additional layout
guidance:

- Prefer a `tests/` submodule over large ordinary test blocks in production
  `mod.rs` files.
- `src/tool_runtime/mod.rs` must remain a runtime module, not a test warehouse.
  Domain groups under `src/tool_runtime/tests/` include `schema`, `tool_call`,
  `dispatch`, `sessions`, `checkpoint`, `files`, `git`, `jobs`, and `metadata`.
- Shared setup belongs in `tests/support.rs` or a narrow domain helper.
- Prefer table-driven tests for repeated matrices; keep exact assertions for
  security, destructive actions, required schema fields, session guards, and
  transport envelopes.
- Suggested soft limits: split files beyond ~2,000 lines or mixed domains;
  extract fixtures when a single test exceeds ~80 lines.
- After mechanical test moves, keep names and assertions stable first; semantic
  cleanup in a separate change.
- Use `#[ignore]` only for real external dependencies, long network behavior, or
  intentionally heavy integration; document why.

See also [`TESTING.md`](../TESTING.md).

---

## 4. Validation evidence semantics (product)

- Dedicated validation tools and `run_shell`/terminal `run_job` calls declaring
  `validation`, `test`, `build`, `format`, or `release` purpose project into the
  same bounded execution-evidence contract. Tool name is not the source of
  truth for whether validation occurred.
- Evidence carries execution source, stable assertion/command identity,
  purpose, bounded command summary, project-relative cwd, shell/executor,
  execution state, exit code, detected summary, bounded output metadata,
  timestamps, and failure classification. Full unbounded logs are not ledger
  evidence. The projection distinguishes immutable raw ToolResult success from
  validator/correctness success and request-scoped evidence gaps.
- Structured validation target identity describes what was executed. Cargo test
  count assertions such as `require_tests` / `min_tests` are invocation-scoped
  evidence requirements, not part of that target identity and not durable task
  obligations.
- Retry resolution for real validation failures is exact by stable target
  identity. A later successful validation can resolve only failures for that
  identity; it never deletes or rewrites historical events. Request-scoped
  evidence assertion failures remain visible as evidence gaps rather than
  correctness failures.
- Closeout and review expose `historical_failures`, `resolved_failures`,
  `unresolved_failures`, and separate evidence-gap facts. Resolved or stale
  history is advisory; only current actionable command/test failures are hard
  blockers.
- `validation_summary` is a read of existing ledger evidence; it does not
  re-run Cargo/shell or replace `finish_coding_task`. Handoff and finish reuse
  this projection instead of building independent validation truth.
- `continuation_feedback` (surfaced by `finish_coding_task` and
  `session_handoff_summary`; its `validation_delta` part
  also by `validation_summary`) is a deterministic, read-only projection of
  the prior attempt over existing ledger/evidence/Job/message-board state. It
  is never an LLM summary, never a new verdict, never a second attempt state
  machine, and introduces no new persistent table. Validation delta is only
  comparable when scope/evidence and parser identity are proven; otherwise it
  reports a stable reason code. `scope_identity` is an opaque, domain-separated
  SHA-256 over the normalized structured scope — it never re-exposes command
  text or absolute paths. When the attempt boundary has been evicted by the
  bounded event window, the projection reports `complete = false` rather than
  masquerading a truncated window as the session start. See
  [`session-model.md`](session-model.md) §Continuation feedback.

Closeout is an Agent-ready fact package, not a context-free engineering judge.
Its primary layers are `facts`, `hard_blockers`, and `advisories`. Ordinary
dirty worktrees, bounded truncation, optional validation not observed, and
resolved history are advisory. Permission/session-guard denial, unresolved
workspace conflicts, command/test failures, blocking active executions,
sensitive-path risks, and consistency errors are deterministic blockers.

---

## 5. Refactor preference (design stance)

- Prefer small, reviewable refactors over unbounded accretion when a module
  becomes a dumping ground.
- Do not mix behavior changes with mechanical moves unless unavoidable; report
  any semantic change explicitly.
- Do not preserve obsolete compatibility layers by default (see §2).
- Structural refactors that reduce coupling or clarify ownership are allowed
  when scoped to the task; unrelated broad rewrites are not.

---

## 6. Canonical two-mode authority with fail-closed legacy env rejection

The permission-mode system (`WEBCODEX_PERMISSION_MODE` with
`dev_auto_approve` / `audit_only` / `require_approval`) is replaced by one
canonical authority mode.

| Decision | Choice |
|---|---|
| Env var | `WEBCODEX_AUTHORITY_MODE` = `trusted_agent` \| `restricted` |
| Default (unset/empty) | `trusted_agent`; source reported as `default` |
| `trusted_agent` | Consequential runtime tools auto-execute after hard safety with no approval interruptions; external release actions remain user-task-scoped; every permission-bearing call records an auditable ledger decision (`policy=trusted_agent`, `status=auto_approved`, `reason=trusted_agent_authority`) |
| `restricted` | Consequential runtime tools deny (`restricted_requires_human_authorization`); there is no separate Connector approval loop |
| Legacy env set | Unambiguous legacy values migrate: `dev_auto_approve` → `trusted_agent`, `require_approval` → `restricted`; legacy-only configuration reports `migrated_env:WEBCODEX_PERMISSION_MODE`. Unknown or conflicting legacy/current values remain invalid and fail closed with source `rejected_legacy_env:WEBCODEX_PERMISSION_MODE` |
| Shared surfaces | Both modes share the same tool implementations, schemas, session model, evidence, and audit records |
| Projection | `runtime_status` and internal full startup diagnostics report one canonical `authority` object; the sparse external `work_on_project` projection omits it. The old `permissions` profile object is deleted |

Hard boundaries are never relaxed by authority mode: OAuth scopes, project
boundary/allowed roots, explicitly read-only sessions, path and sensitive-path
policy, concurrent-overwrite guards, credential redaction, job cancel/reclaim,
and immutable release targets. Full contract:
[`permission-model.md`](permission-model.md).

---

## 7. Connection layers are an observation contract

`runtime_status.connection_layers` reports facts that were actually observed;
it never infers readiness from configuration.

| Decision | Choice |
|---|---|
| Layer envelope | Every layer carries `{status, observed_at, source, age_secs, stale_after_secs, reason_code}` plus layer facts |
| No config-inferred readiness | Runtime/Project readiness comes from authenticated canonical runtime and Runner/Project observations. `runner_process` never fakes "running"; a stale registration is never presented as callable |
| Explicit Workflow targeting | Workflow Sessions have no implicit credential/window selection. Ordinary project tools without an explicit business Session or authorized wrapper recorder execute unlinked to Workflow Session state |
| Full-runtime start/continue | `work_on_project(session_id=<id>)` continues exactly that authorized Active same-project Session; omission creates a fresh Workflow Session. Stable window or credential identity never selects a Workflow Session. `work_on_project` calls the shared coding workflow engine directly; there is no second internal ToolCall identity |
| Canonical model coding bootstrap | `work_on_project` is the external runtime coding bootstrap. `registered_tool_specs` defines the canonical model-visible runtime universe used by discovery and generic ToolCall admission. There is one model-facing runtime contract: Adaptive Runtime. Canonical `ToolDefinition` rank defines direct admission/order; ordinary model-visible long-tail tools use `call_runtime_tool`; an admitted direct target may also use the gateway as an invocation fallback. Retired wire names such as `start_coding_task` fail closed before dispatch. |
| Adaptive Runtime presentation | MCP and GPT Actions project the same canonical Adaptive routing policy. Protocol/App-only extensions are admitted independently by server-owned protocol capability and App metadata; they do not create another runtime surface. Direct/gateway dispatch preserves the target tool's scopes, Project authority, permission, argument, Runner capability, effect, and Session/ACK semantics. `WEBCODEX_MCP_COMPACT_SCHEMAS` changes MCP discovery schema projection only; unset defaults to compact discovery and explicit true/false overrides that projection. Runtime status, MCP initialize/discover/info, and tools/list audit summaries do not emit a redundant runtime-surface taxonomy. |
| Meaningful-activity rule | `last_successful_tool_call` records only successful meaningful calls, scoped by principal/project/surface/session/tool. `runtime_status`, `list_tools`, `list_runners`, `list_projects`, and `tool_manifest` never refresh it. Bounded in-memory store; no arguments, outputs, or secrets |
| Independence | Layers degrade independently; `not_observed` on one layer must not be collapsed into a global offline verdict |

---

## 8. Canonical external startup projection (hard cut)

`work_on_project` is the only external runtime coding bootstrap. The retired
`start_coding_task` wire/API name and its advanced input schema are not accepted.
The shared coding workflow engine owns startup behavior directly. Bounded
`minimal|standard|full` diagnostic projections are retained only behind test
seams; those controls are not a public tool argument or ToolCall identity.

| Decision | Choice |
|---|---|
| Removed wire entry | `start_coding_task` has no current ToolDefinition or compatibility schema and follows ordinary unknown-tool rejection; `work_on_project` is canonical |
| External projection | `work_on_project` returns one deterministic sparse startup projection and does not expose full runtime/connection/authority diagnostics |
| Internal `standard` | Default bounded Coding brief used by shared startup plumbing: strict session/project/workspace, incremental repository instructions, bounded continuation evidence, semantic-navigation summary, blockers/warnings, and concrete next actions |
| Internal `minimal` / `full` | Retained only as implementation-level projection modes for internal callers/tests; they are not generic HTTP/MCP tool inputs |
| Rule snapshot lifecycle | Fresh sessions load bounded content; unchanged same-process continuations reuse the in-memory fingerprint snapshot without repeating content; source additions/deletions/content/truncation changes return new bounded content; explicit or restart-restored Sessions reload because durable storage never contains rule bodies |
| Unknown/removed external fields | Unknown or removed tool names fail closed before legacy argument interpretation; `work_on_project` keeps its own strict schema |

No alias or dual shape is kept for the removed flags (consistent with §2).

---

## 9. Mixed-version diagnostics without compatibility fallback

Runner registration reports `process_started_at` and
`build {version, git_commit, git_dirty}`; `runtime_status` projects
package/protocol compatibility separately from exact source alignment.

| Decision | Choice |
|---|---|
| Compatibility shape | `version_compatibility.status` is `compatible \| version_mismatch \| capability_mismatch \| no_runners`; each Runner reports `version_matches_server`, protocol facts, and compatibility reason/action. Package-version compatibility is not exact source identity. |
| Source alignment | `version_compatibility.source_alignment.status` is `aligned \| different \| unknown \| no_runners`; per-Runner source alignment reports `git_commit_matches_server` and `source_matches_server`. Exact alignment is true only when commits match and both builds explicitly report `git_dirty=false`; differing commits or a dirty side are different, incomplete build facts are unknown. |
| Connected ≠ compatible | Transport liveness never implies protocol/package compatibility or exact source alignment. |
| Direction | Compatibility and source-alignment facts provide separate actions; there are no fallback shims or version-translation layers. |
| Shell dialects | `ShellProfilesSummary` reports `default_dialect` (`sh` \| `bash` \| `custom`) and `available_dialects`; each profile entry reports `dialect`. The server never guesses the remote shell; custom profiles that do not map to sh/bash report `custom`, and agents needing deterministic syntax must pass an explicit `shell=sh\|bash`. No PATH/env/init-script contents are ever sent |

---

## 10. Model execution and durable continuation

The standing direction for model-facing execution is defined in
[`ARCHITECTURE.md`](../ARCHITECTURE.md). The durable decisions are:

1. **Structured lifecycle is execution truth.** Retry safety must not depend on
   interpreting prose. `command_started`, completion state, failure
   classification, Job state, and guidance must not contradict one another.
2. **Prefer direct argv/process execution for one native executable.** Use shell
   command strings when shell semantics or a short, tightly related command chain
   is the point; keep independent effect or validation boundaries separate. Long
   script content belongs in a bounded payload channel rather than an ever-larger
   quoted string.
3. **One execution may outlive one tool/model turn.** When work exceeds a short
   synchronous grace window, the same execution should continue as a durable Job;
   handoff must not be implemented as cancel-and-retry.
4. **Job/observation is the execution continuation and observation API.** Durable
   Job identity, lifecycle, bounded logs, observation token, cancellation,
   ownership, and recovery/lost semantics remain OS-, transport-, and
   presentation-neutral. This does not itself mean model/Host continuation; batch
   observation should reuse this model rather than create a second scheduler or
   revision system.
5. **Optional host UI is an adapter, not an owner.** MCP Apps or another host may
   observe Jobs and later resume a model, but core execution cannot depend on
   Apps, MCP Tasks, MRTR, elicitation, progress extensions, or iframe state. MCP
   App presentation is a Server-level optional adapter: `WEBCODEX_MCP_APPS_ENABLED`
   defaults on and may disable App capability advertisement, descriptor linkage,
   presentation metadata, and static App resources without disabling canonical
   MCP tools/results or non-App resource delivery. If automatic model resume is
   provided, exactly one durable continuation domain owns each logical resume
   event; independent Job Views, cards, or Host views do not race to resume the
   model. A long build/watch reaching terminal state may be an explicit input to
   Goal/AgentTask orchestration, but the card is never the trigger or continuation
   owner. For Agent-bound continuation, the Agent Wake / Wake Delivery Attempt
   domain owns that logical continuation; Host/controller state is adapter-local
   delivery state rather than a second WebCodex continuation truth.
6. **Transport fallback must preserve execution semantics.** Polling, WebSocket,
   and QUIC may differ in delivery behavior, but none may silently duplicate a
   command or turn a transport stall into a false pre-start rejection.

Do not broaden an execution task into fleet upgrade management, Windows SCM
productization, a generic process/service API, PTY support, or polished MCP App UI
unless the user task explicitly requires that scope.

---

## 11. 0.4 compatibility floor

`v0.4.0` is the compatibility floor for **concrete compatibility domains** such
as durable persisted state, mixed-version Server/Runner operation, shipped
operator/install workflows, and named external/public contracts. The
`0.3.x -> 0.4.0` boundary remains an intentional cleanup point for Runner
generation, retired CLI aliases, pre-0.4 persisted state, authority, and
configuration.

The floor does **not** freeze every model-facing ToolSpec argument, result field,
projection, or historical spelling through the `0.4.x` patch series. There is no
supported third-party stable runtime-tool SDK today. During active development,
a model-facing tool shape may therefore be simplified or broken when that removes
duplicate truth, misleading semantics, or avoidable turn friction and no named
consumer requires the old shape.

Compatibility code is retained only when it has a concrete consumer: accepted
persisted state, mixed-version Server/Runner operation, a current external
workflow or installer, a published artifact contract, or a required fail-closed
security/privacy migration boundary. An implementation plus tests that only
assert that implementation exists is not by itself a consumer. Published-but-
unused CLI/API aliases and duplicate machine-readable fields may therefore be
removed after an exact consumer search.

Where a concrete consumer does exist, compatibility remains narrow and
fail-closed. Protocol generation 2 remains the 0.4 Server/Runner rolling
baseline; additive capabilities do not expand its required set, and absence is
handled as unavailable rather than inferred authority. Durable DB, Workflow
Session, and registry state is migrated or quarantined deterministically rather
than silently reinterpreted. MCP `structuredContent` remains the canonical
machine-readable `tools/call` result; `content.text` is the concise human
fallback by default. A named host that cannot expose `structuredContent` may use
the explicit `WEBCODEX_MCP_TEXT_JSON_COMPAT=true` compatibility projection to
mirror that same canonical JSON into standard text content without changing the
source of truth.

The product concept and public lifecycle namespace are **Runner**. The local
primary config filename is `runner.toml`, and all newly generated 0.4.x
configuration uses that name. Persisted pre-0.4 startup state has a deliberately
narrow compatibility window through the 0.4.x line: automatic/default/profile
discovery still accepts a legacy-only `agent.toml`, while a directory containing
both names fails closed so a stale legacy file cannot silently look authoritative.
A directory containing neither creates/targets `runner.toml`. Explicit
`--config PATH` remains exact and does not reinterpret the chosen filename.
Explicit `--profile` similarly selects its authoritative profile directory
before environment defaults are considered. `WEBCODEX_RUNNER_CONFIG` is the
canonical default-path env override; legacy-only `WEBCODEX_AGENT_CONFIG` remains
a deprecated fallback during 0.4.x, while setting both env names is ambiguous and
fails closed. These persisted startup aliases are scheduled for removal at the
0.5.0 compatibility boundary rather than a 0.4.x patch/minor restart.

Runner-owned project registries use `project_registry_dir` and
`project-registry/` for new state. During 0.4.x, a legacy-only persisted
`projects_dir` field is normalized into the canonical runtime
`project_registry_dir`; configuring both fields remains an error. The old
`--projects-dir` CLI spelling stays retired because interactive CLI aliases are
not required for cold-start compatibility. The physical legacy `projects.d/`
directory remains readable in place when it is the sole default registry layout,
so upgrading does not require an implicit data move. If both default directory
names exist WebCodex fails closed; it does not merge, copy, rename, or choose
between two registries implicitly. The registry remains a directory of
Runner-owned project registration records, not a second workspace or project-root
abstraction.

Project registration provenance is also normalized before the `v0.4.0` floor.
The generic project-record `kind` field remains open project metadata, while the
optional `registration_source` field describes how the record entered the
Runner registry (`explicit` or `auto_registered`). New path auto-registration
persists `registration_source = "auto_registered"` and does not persist a fake
`kind`. For old records only, absent `registration_source` plus the exact
historical `kind = "auto_registered"` sentinel remains a compatibility fallback;
a present new field is authoritative. This interpretation is kept separate from
the raw record representation used for project revision/CAS hashing, so merely
upgrading WebCodex does not change an unchanged legacy record's revision. During
rolling upgrades a new Runner may still project the historical sentinel on its
inventory and path-operation wire results for a newly auto-registered project
with no genuine kind so an old Server can classify it, while new Servers use the
explicit additive provenance field.
Public `list_projects.source` remains `agent_registered` / `auto_registered`.

The pre-0.4 managed-temporary-project lifecycle is retired rather than carried
into the `v0.4.0` product contract. Current project creation uses explicit
`create_project`; existing directories use `work_on_project(path)` or
`register_project`. The retired `temporary_projects_root` key no longer has a
typed Runner configuration meaning. Runner configuration intentionally ignores
obsolete unknown top-level keys, so an old file containing this key remains
loadable without preserving its former validation, warning, or runtime field.
Existing project-registry records whose generic
`kind = "managed_temporary"` value predates this cleanup remain readable as
ordinary registrations; current Server projections do not treat that value as
an active lifecycle. A legacy Server request containing the old
`managed_temporary_project` create flag fails closed on a new Runner before any
filesystem mutation. This retirement does not change protocol generation,
baseline capabilities, `client_id`, `agent_project_id`, or runtime project ids.

Other older `agent_*` names below have concrete token, persisted-state, or wire
consumers and are therefore retained rather than cosmetically duplicated. In
particular, `WEBCODEX_AGENT_TOKEN`, `wc_agent_*`, `agent_instance_id`, runtime
project ids of the form `agent:<client_id>:<project_id>`, and established
DB/wire `agent_*` fields keep their existing names. This local filename migration does not imply
a Server/Runner protocol-generation or wire-identity rename. Public Runner
observations now use `runner_instance_id`, `runner_protocol_generation`, and
`runners`; the retained `agent_*` names above refer to wire/persisted contracts,
not observation aliases. See [Runner observability](runner-observability.md).

Compatibility never requires retaining a known authentication bypass, unsafe
authority, ambiguous or stale identity, or weakened fail-closed validation. A
security- or correctness-required break is allowed only when it is explicit,
narrowly scoped, documented, and accompanied by the required upgrade action in
release notes.
