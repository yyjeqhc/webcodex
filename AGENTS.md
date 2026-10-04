# AGENTS.md — WebCodex Repository Delta

These rules supplement the builtin `webcodex.workflow`; they are the WebCodex-specific delta, not a second generic coding workflow. A deeper `AGENTS.md` governs its directory. Load linked domain guidance only when the task reaches that boundary.

WebCodex is actively developed. Features, fixes, and reliability work are welcome, but preserve credential, process-tree, transport, durability, authority, replay-safety, and boundedness contracts.

## 1. Product and authority invariants

- Protect real authority boundaries: credentials, public entry points, destructive actions, execution targets, process ownership, history, and published artifacts. Model new credential audiences, scopes, identities, leases, and replaceable targets explicitly; stale requests must not retarget or gain authority.
- Never print, commit, or expose credentials, authorization headers, private keys, tokens, secret files, or sensitive command output.
- Structured execution lifecycle state owns retry safety. Semantic ambiguity, authority changes, stale fences, replay uncertainty, and uncertain effects fail closed; presentation or model ergonomics never override execution authority.
- Keep one canonical machine representation per fact or follow-up. Static/internal taxonomies and forensic bookkeeping belong in runtime types, tests, telemetry, or exceptional diagnostics rather than duplicate model-facing fields. Model-facing compatibility requires a named concrete consumer; historical aliases are not retained merely because an older implementation emitted them.
- HTTP/MCP requests are stateless unless the exact adapter contract supplies a stable `ClientWindow`. Never infer Workflow Session, model-context retention, or authority from transport/audit IDs, credentials, Projects, or prior requests.
- Keep Connector Tasks, Workflow Sessions, durable Agents/Conversations/Wakes, and Agent Tasks distinct. Persistence of identity or history is not permission to replay an effect.
- Keep core execution and Job semantics protocol-, UI-, transport-, and OS-neutral. Host-specific behavior, including MCP App orchestration, belongs in explicit adapters.
- Boundedness is a correctness property for model surfaces, logs, persistence, scans, queues, and retained state. Do not turn ergonomic limits into authority bypasses or silently widen resource contracts.

Product direction: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## 2. WebCodex implementation conventions

- Follow existing authoritative paths and naming. For cross-layer changes, complete the relevant enums, registries, schemas, adapters, persistence/recovery rules, and projections end to end; do not create a parallel representation for convenience.
- Treat existing implementation, comments, tests, and compatibility code as evidence, not permanent product requirements. Remove obsolete adapters/configuration/tests together when their concrete consumer is gone, while preserving real wire, authority, durability, and release boundaries.
- Model-facing directness is exposure/ergonomics, not authority. Generic dispatch may serve an admitted tool only through the same canonical schema, authorization, permission, capability, and effect checks; hidden or unadmitted tools remain unavailable.
- ToolSpec descriptions must be self-contained about selection, authority, effects, retries, continuation, uncertainty, and recovery. Use the available description budget when semantics require it; arbitrary brevity must not erase safety or recovery meaning.
- Prefer one unambiguous model-visible next action. If removing a result field leaves the same business interpretation and exact next action, omit the duplicate by default. See [`docs/agent/tool-contract-guidelines.md`](docs/agent/tool-contract-guidelines.md).
- Prefer host-native artifact import/export or project transfer for binary files. Do not route complete binary payloads through model-generated Base64 merely to cross a tool boundary.
- New execution/provider integrations must preserve process-tree ownership, credential isolation, cancellation, timeout, durable Job identity, and recovery semantics rather than bypassing ToolRuntime contracts.
- Before filing a GitHub issue, follow **Reporting issues** in [`CONTRIBUTING.md`](CONTRIBUTING.md): distinguish directly reproduced facts from source-based inference or user reports, include discoverable evidence, and never invent missing environment details or a root cause.

## 3. Rust, tests, and validation

- Use the `dogfood` Cargo profile for optimized development builds. Reserve `release` for formal release/publication artifacts.
- Rust changes normally use the smallest package/focused tests that detect the changed behavior. Full workspace/library runs, `--all-targets`, ignored tests, real-process harnesses, and E2E are for explicit need, release/deployment, cross-cutting boundaries, broad conflict resolution, or a focused-coverage gap.
- Treat `cargo fmt` as finalization after Rust source stabilizes, not a mutation-by-mutation loop. CI/release formatting remains a final-state gate.
- Put tests in the existing domain test tree. Keep inline private-helper tests small; process/network/integration fixtures belong in dedicated modules.
- Process fixtures must mirror production I/O ownership: absent stdin is EOF; explicit stdin is those bytes then EOF; drain/capture stdout and stderr without pipe deadlocks; never accidentally inherit the invoking terminal. Prefer child-local configuration/environment over process-global mutation.
- For console-only or flaky failures, reproduce with one controlled boundary changed and add a deterministic regression that also fails headlessly. Retries, larger timeouts, and lower concurrency are diagnostic/resource controls, not substitutes for the demonstrated fix.
- Distinguish current failures, pre-existing failures, expected negative cases, and failures resolved by retry. Never weaken authentication, authorization, validation, schemas, sandboxing, ownership checks, or tests merely to make validation green.
- Public or persisted contract changes need focused compatibility/privacy/boundedness coverage. Recovery tests should prove identity/effect semantics, not only happy-path output.

Testing guidance: [`docs/TESTING.md`](docs/TESTING.md).

## 4. Git, deployment, and release authority

- Push, publish, tag, release, deploy, restart services, or otherwise mutate external systems only when the user explicitly authorizes that action and target.
- Do not force-push, move published tags, overwrite releases, or rewrite published history without explicit authorization naming that operation and target. Failed pre-publication tag recovery is allowed only through the documented guarded reclaim path.
- A named development/dogfood deployment is not a release. Deploy the reviewed source commit, preserve rollback, record build identity without hiding dirty state, and run focused smoke; this does not authorize version bumps, tags, or publication.
- Release/publication must obey immutable artifact and signing/provenance contracts. Follow [`docs/agent/release-process.md`](docs/agent/release-process.md) and [`docs/RELEASE_CHECKLIST.md`](docs/RELEASE_CHECKLIST.md).

## 5. Domain sources of truth

Route detailed decisions to the owning document instead of restating those contracts here:

- Public runtime/API surfaces: [`docs/agent/runtime-api-guidelines.md`](docs/agent/runtime-api-guidelines.md)
- Model-facing tool contracts and friction policy: [`docs/agent/tool-contract-guidelines.md`](docs/agent/tool-contract-guidelines.md)
- Workflow Sessions and request identity: [`docs/agent/session-model.md`](docs/agent/session-model.md)
- Manual multi-window collaboration: [`docs/agent/manual-window-collaboration.md`](docs/agent/manual-window-collaboration.md)
- Durable Agent identity, Conversation, Wake, and asynchronous Agent work: [`docs/architecture/durable-agent-runtime.md`](docs/architecture/durable-agent-runtime.md)
- Current durable Agent/Conversation/Wake implementation contract: [`docs/architecture/durable-agent-conversation.md`](docs/architecture/durable-agent-conversation.md)
- Authority boundaries: [`docs/agent/permission-model.md`](docs/agent/permission-model.md)
- Architecture decisions: [`docs/agent/architecture-decisions.md`](docs/agent/architecture-decisions.md)
- Release/recovery/deployment: [`docs/agent/release-process.md`](docs/agent/release-process.md) and [`docs/RELEASE_CHECKLIST.md`](docs/RELEASE_CHECKLIST.md)
