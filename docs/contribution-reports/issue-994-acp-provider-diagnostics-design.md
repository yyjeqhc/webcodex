# Issue #994: ACP Provider read-only diagnostics

Status: **historical Runner-only stage 1–2 design and verification**. The statements
about missing consumers below describe that branch at the recorded baseline. Current
`main` also has the authorized Server/ToolRuntime and CLI entry described in
[stage three](../design/issue-994-server-diagnose-entry-design.md). This report's
original platform, test counts and remaining acceptance limits are preserved; it is
not current tool-discovery guidance or proof that all of Issue #994 is accepted.

## Scope and safety contract

Upstream issue: https://github.com/yyjeqhc/webcodex/issues/994
Base: upstream `main` commit `c425f837b990c000b4dec196650d96e410c67594`.

This is the historical design and verification record for a **Runner-only
partial implementation**, not acceptance of the complete operator-facing
Issue #994. At this Runner-only stage, a typed `Diagnose` operation existed, but no
Server/ToolRuntime, Desktop, CLI, or MCP surface invoked it. The subsequent
entry had to independently authorize the exact Runner, provider generation, and
Project (and explicitly authorize process launch for a live probe). Do not
claim the complete issue or deploy an unreviewed operator-facing entry.

### Initial building block (historical stage 1)

- `CodingAgentManager::declared_provider_env_presence(provider_id)`
  scopes lookup to an explicitly selected **configured** provider.
- A pure source-presence helper probes only names declared by the selected
  provider's `env_from_env` mapping in the **calling Runner process**.
- Returns only destination names mapped to booleans. Never includes source
  names, values, executable paths, arguments, or undeclared environment.
- Non-Unicode environment values are not stringified.
- Tests cover present/missing mappings, unknown provider, unlisted-source
  exclusion, serialization non-disclosure and no child process execution.
- Stage-1 caveat: the canonical helper then lacked a consumption path; Rust could
  report `dead_code` until the diagnostic operation was added.

### Historical checklist before the Runner-only PR

1. Add a read-only Runner request/response variant, bound to an explicitly
   selected provider ID and instance, with the existing Runner/provider
   observation authority and no implicit `coding_agent:run` grant.
2. Reuse the same `CodingAgentManager` primitive, executed inside the
   active Runner process. A separately invoked CLI shell environment is not
   an acceptable surrogate for the running service process.
3. Implement a **bounded ephemeral** ACP `initialize` then `session/new`
   probe through the existing managed-child / bounded frame transport.
   No `session/prompt`, `session/set_config_option`, provider tool calls,
   config mutation, or unbounded retries.
4. Project only bounded option IDs/types and allowlisted select choices;
   classify raw ACP parse/transport failures with constant safe codes.
5. Add negative tests: forbidden calls absent on the fake provider's log;
   init/new failure, oversized/malformed output, choices budget, secret values
   absent in *all* results and errors, timeout and process-tree cleanup.
6. Validate schema/authorization/identity in Runner, Server and model-facing
   tool contracts, and run focused ARM64 + E2E tests before proposing PR(s).

Preserve exact warning that an ACP provider's initialize/session/new may
trigger an external login prompt. The implementation must fail closed rather
than silently proceed when provider-side side effects are not understood.

## Version safety

No Gitea `dev`/`prod`, wa1 Live, or QUIC private-security branch changes.
Source worktree: `webcodex-b-acp-diagnostics-20261014` in its independent
upstream contribution branch.

## Verified Runner request and protocol boundary (2026-10-14)

The original internal environment-presence helper is now part of an
explicit, typed `CodingAgentRequest::Diagnose` operation. It is not the
existing Start/Observe/Cancel operation and **never** launches an ACP
process, creates a Run, submits a prompt or mutates Provider configuration.

- Core transport uses a strict request type containing a correlation
  run_id, exact provider and provider-instance IDs, runtime Project ID,
  and canonical Project root. The response has only provider identity and
  configured destination-to-boolean presence.
- `validate_request` fences identifier lengths, Project identity and
  project-root syntax. `validate_response_for_request` rejects mismatched
  provider generations, excess environment entries and bad keys.
- Runner dispatch checks its current provider instance and registered
  Project root before reading environment presence. Read-only Projects
  may be observed; the pre-existing Start operation still requires
  writable Project authority. No arbitrary environment variable names
  are supplied by the caller.
- `webcodex-runner-registry::enqueue_coding_agent` is the established
  dispatch fence: caller RunnerAccess, exact connected Runner identity,
  advertised provider instance and the checked request provider binding.
  The new Diagnose operation rides that existing typed fence.
- A dedicated Runner regression verifies success, missing source, no
  child process/no Run persistence, stale provider rejection, wrong
  Project rejection and read-only Project acceptance.
- A core transport regression verifies typed roundtrip, exact provider
  binding and response size/key rejection.

Verified locally in the isolated ARM64 build target:
- `cargo check --locked -p webcodex-core -p webcodex-runner -j 2`:
  PASS on the intermediate wire patch.
- `cargo test --locked -p webcodex-core --lib
  diagnostic_wire_is_identity_fenced_and_does_not_accept_raw_secrets`:
  1 passed / 0 failed.
- `cargo test --locked -p webcodex-runner --bin webcodex-runner
  acp_diagnose_runner_request_is_identity_fenced_read_only_and_no_spawn`:
  1 passed / 0 failed after adding read-only Project coverage.
- `cargo fmt --all -- --check` and `git diff --check`: PASS.

**Still not externally callable:** no Server/ToolRuntime or UI
presentation invokes the new operation yet. The eventual Server entry
must first independently resolve and authorize the exact requested
Project and Runner; no generic public environment-inspection endpoint
is acceptable. Old Runner protocol generations must fail closed rather
than silently accepting unsupported Diagnose requests. The second
stage live ACP initialize/session-new probe is not implemented. Do not
advertise this internal Runner request branch as complete #994 or open
a completion PR without these pieces.

## Verified live ACP probe stage (2026-10-16)

Stage two of the maintainer-endorsed plan is now implemented on the same
branch, still behind the explicit, typed `CodingAgentRequest::Diagnose`
fence.

- The request carries `include_live_acp` (serde default `false`). Without
  it, Diagnose remains the no-spawn env-presence operation and the
  response carries `live_acp: null`.
- With it, the Runner launches the provider exactly like a Coding Run
  (explicit `env_from_env` resolution, `env_clear`, authorized Project
  cwd, managed process group), performs **only** `initialize` then
  `session/new`, projects the live `config_options`, and always
  terminates the whole process tree via a drop guard on every exit path
  (success, fault, timeout, Runner shutdown).
- The projection is a closed bounded identity view: option id/label,
  `select`/`boolean` kind, safe current selection, and choice values with
  labels. Budgets: 32 options, 64 choices per option, 128-byte
  identities, 256-byte labels (truncated). A select currentValue is emitted
  only when it matches an actually projected advertised choice; unlisted
  private-looking current values are not reflected. Options outside the identity
  vocabulary are dropped and reported through `options_truncated` /
  `choices_truncated` rather than widening the contract.
- Failures never carry provider text: a closed
  `CodingAgentDiagnoseOutcome` (`ok`, `env_unavailable`, `spawn_failed`,
  `process_exited`, `transport_failed`, `rejected`,
  `protocol_unsupported`, `invalid_response`, `timeout`, `busy`,
  `interrupted`)
  plus the reached `CodingAgentDiagnosePhase` are the only fault
  vocabulary. A failed probe always reports empty options. stderr is
  drained to a sink and never captured.
- One live probe runs at a time per Runner (`live_probe_permit`), under
  a single bounded budget (30s total, matching `ACP_SETUP_TIMEOUT`). Other
  concurrent live probes return `busy` immediately rather than queueing
  outside this deadline. Diagnostic stdout frames are capped while reading,
  so a provider cannot stream an unterminated line into unbounded memory.
- `validate_response_for_request` rejects unrequested `live_acp`, a
  missing `live_acp` when authorized, non-v1 protocol versions, options
  on failed outcomes, and any identity/label outside the budgets.
- New Runner regressions cover: exactly `initialize` + `session/new` on
  the wire with no prompt/config writes and no Run persistence; env
  source missing without spawn; the full fault matrix (rpc/malformed/
  oversized/eof at both stages) staying classified and secret-free;
  timeout terminating the whole tree including a spawned descendant;
  provider exit classified without leaking the status code; and the
  oversized-options projection passing end-to-end response validation.
  Core regressions cover the authorized/unauthorized `live_acp` wire
  fencing and the closed budgets.

Focused x86 verification (2026-10-16):
- `cargo test --locked -p webcodex-core --lib diagnostic`: 12 passed.
- `cargo test --locked -p webcodex-runner --bin webcodex-runner
  diagnostics::`: 18 passed / 0 failed.

**Still intentionally out of this branch:** no Server/ToolRuntime or UI
surface invokes Diagnose yet; the registry fence
(`enqueue_coding_agent`) already type-checks the operation, but the
future Server entry must still independently resolve and authorize the
exact Project and Runner before forwarding it. That presentation work,
plus ARM64/E2E confirmation and maintainer review of the probe, remains
before #994 acceptance can be claimed.
