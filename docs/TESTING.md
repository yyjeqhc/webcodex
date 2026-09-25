# Testing Strategy

Tests should protect current behavior: runtime tools, session guards, project
and file operations, Git and shell dispatch, Runner transports, MCP, OpenAPI,
and OAuth scope policy. Test count alone proves neither useful coverage nor
unnecessary complexity. Trace a test to a current entry point, consumer, or real
boundary; a passing test of an unused configuration parser or always-empty
policy does not justify keeping that mechanism.

When retiring a concept, remove its dedicated tests and fixtures with it. Keep
coverage of the surviving public behavior and safety boundaries, rather than
replacing removed assertions with source-text checks that freeze another
implementation detail. Also review layering, global state leakage, bounded
waits, and the cost of each test lane.

## Test Lanes

| Lane | Purpose | Default resources | Typical command |
|---|---|---|---|
| fast unit | Pure parsing, validation, helpers, local state machines, small fixtures. | No network, no global env mutation, no long sleeps. | `cargo test -p webcodex --lib tool_call` |
| contract/schema | Keep metadata, registry, MCP `tools/list`, OpenAPI, and runtime tool names synchronized. | No external network; in-process services are preferred. | `cargo test -p webcodex --lib metadata`; `cargo test -p webcodex --lib mcp`; `cargo test -p webcodex --lib openapi` |
| local integration | Exercise HTTP handlers, runtime dispatch, sessions, local agent registry, temp dirs, loopback listeners, and database fixtures. | Loopback only, isolated temp dirs, bounded waits, no shared mutable state without a lock. | `cargo test -p webcodex --lib runtime_http -- --nocapture`; `cargo test -p webcodex --lib session -- --nocapture` |
| Runner/LSP real-process | Process-tree ownership, real shell timeout/stop, polling dispatch timing, Plugin startup, validation/Git `ManagedChild`, JobManager descendant cleanup, and native LSP child lifecycle. Runner coverage is gated by `runner-real-process-tests`, which also enables the LSP crate's `real-process-tests` feature; these tests are ignored by default execution and share the `runner_real_process_` name prefix. | Real local child processes only; no external network. Run serially because the assertions intentionally exercise OS scheduling and process teardown. | `cargo test --locked -p webcodex-runner -p webcodex-lsp --features runner-real-process-tests runner_real_process -- --ignored --test-threads=1` |
| Process lifecycle real-process | `ManagedChild` ownership, graceful/forced termination, descendants, EOF, liveness, and reaping. Most lifecycle tests in the integration target are ignored; pure type/spawn-error smoke remains ordinary. | Real local helper processes and OS liveness probes. | `cargo test --locked -p webcodex-process --test managed_child -- --ignored --test-threads=1` |
| Persistent-shell timing | Timeout, concurrent busy-state, close-vs-exec, idle expiry, descendant teardown, and heavy adversarial PowerShell timing/status coverage. Fast state/error/exit smoke remains ordinary. | Real shell processes; serial execution only. | `cargo test --locked -p webcodex-persistent-shell -- --ignored --test-threads=1` |
| Desktop Windows real-process | Windows Desktop stdin-EOF shutdown and bounded-command process-tree reclamation. These tests are ignored by the ordinary Desktop suite and share the `desktop_real_process_windows_` name prefix. | Real local child processes only; no external network. Run serially so PowerShell startup and process teardown do not compete with the ordinary Desktop libtest pool. | `cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml desktop_real_process_windows_ -- --ignored --test-threads=1` |
| slow/manual ignored | Valuable coverage that is local but slow, serial, large-input, or global-state-sensitive. | Explicit operator opt-in; often `--ignored` and `--test-threads=1`. | Run the specific ignored test/filter documented by its subsystem. |
| e2e/deployment smoke | Prove that binaries, local services, GPT Actions schema, MCP, artifact transfer, and an agent can work together. | Temporary local services and loopback ports; real deployment only when explicitly requested. | `bash scripts/e2e_zero_config_ws.sh`; `bash scripts/smoke_deployment.sh`; `bash scripts/smoke_artifact_transfer.sh` |
| reconnect continuity | Runner disconnect/reconnect layer independence, stale-not-ready observations, reconciliation-aware recovering/lost transitions, server-restart durable Session plus explicit-session continuity, meaningful-activity scoping, and version-mismatch diagnostics. | In-process fixtures, no external network. | `cargo test -p webcodex --lib reconnect` |
| trusted smoke | Disposable git fixture full chain (start → edit → failing shell validation → fix → pass → git review → finish) asserting zero approval interruptions under `trusted_agent` authority, resolved failure evidence, dirty-worktree advisory-only, and bounded payloads; prints baseline counters. | Temp git fixture, no external network. | `cargo test -p webcodex --lib trusted_smoke` |
| real-process reconnect harness | Boot a real server plus reconciliation-capable runner, assert layered connection observations, crash the runner (layers degrade independently; running job enters `recovering`), restart with a new runner instance (old job is fenced to terminal `lost` with `runner_instance_replaced`, no server restart), then restart the server and verify runner auto-reconnect plus durable Session lookup and continuation by the original explicit `session_id`. It also prints post-deploy smoke facts (server version/commit, authority mode, version compatibility, runner shell dialect). | Local processes and loopback ports. | `bash scripts/e2e_reconnect_ws.sh` |
| real-process hosted-connect harness | Build the real Server, Runner, and CLI; start a shared-key-enabled loopback Server; run `webcodex connect`; verify same-key project visibility and a read, cross-key isolation, detached Runner survival, repeated-connect PID reuse, hosted `runner status`, explicit stop, secret-safe output/log/state, and an untouched Git checkout. | Local processes, isolated XDG config/state roots, a temp Git project, bounded curl and outer timeout, trap cleanup; never production. | `bash scripts/e2e_hosted_connect.sh` |
| real-process job reconciliation harness | Boot a real server plus a WebSocket runner that advertises `job_state_reconciliation`. Scenario A keeps a raw async Job running across a SERVER-only restart and asserts the SAME runner instance, original `job_id`, preserved ownership/project/session, non-regressing sequence/log cursors, `recovered_after_server_restart`, original-process stop, and one side-effect set. Scenario B lets a Job complete while the Server is offline and reconciles the terminal result without duplicate logs or execution. Scenario C forces `run_process` past its synchronous grace window, then proves the handed-off structured Job survives a Server restart and an old Server-epoch observation token refreshes immediately for the same `job_id`. Scenario D uses a delayed Cargo fixture to force a real `cargo_check` validation handoff past its sync window, then proves the same restart/token-refresh/stop contract with the validation command started exactly once. Ordinary Runner-owned Jobs keep the Runner process alive for these scenarios; `run_detached_process` restart survival is a separate supervisor-ownership contract covered by its focused Runner suites and production dogfood. | Local processes, temp dirs/ports/tokens, and a temp project; no production services or QUIC certs. Scenario D intentionally takes roughly the validation sync window plus restart time. | `bash scripts/e2e_job_reconciliation_ws.sh` |
| real-process job recovery failure/non-reconciliation harness | Cover the failure and non-reconciliation paths the happy-path reconciliation harness omits, using `WEBCODEX_JOB_RECOVERY_GRACE_SECS=10` (clamped, above the 5s floor) so the deadline is bounded without waiting the 120s default. Scenario C: kill the runner only (server stays up), let the job enter `recovering`, and assert the non-request-triggered recovery-timeout sweep transitions it to `lost` with `runner_recovery_deadline_exceeded`, `ended_at` set once, one list record, stop-on-lost stable, and the command never re-executes. Scenario D: instance B replaces instance A (same client_id, new `agent_instance_id`); A's job becomes `lost` with `runner_instance_replaced`, B starts its own new job, A's late update is rejected, first `ended_at`/reason preserved. Scenario E: a generation-2 Runner registered with `WEBCODEX_RUNNER_DISABLE_JOB_STATE_RECONCILIATION=1` (no capability, no inventory) dispatches a job and, on disconnect, deterministically fences it to `lost` with `runner_disconnected_without_reconciliation` (never `recovering`); after a server restart its public lost receipt remains observable within retention, and a same-client new no-reconciliation instance cannot revive execution. Scenario F: a long job across three server restarts keeps the same `job_id`, runs the command once, keeps `last_update_seq`/log cursors non-regressing and markers non-duplicating, and reaches a terminal `stopped` that survives a third restart with `ended_at` unchanged by terminal inventory replay. | Local processes, temp dirs/ports/tokens, and a temp project; no production services or QUIC certs. | `bash scripts/e2e_job_recovery_failures_ws.sh` |
| security auth matrix | Cover OAuth, scope policy, shared-key behavior, token classes, read-only session guards, and denied mutations. | No external identity provider by default; use local fixtures and synthetic tokens. | `cargo test -p webcodex --lib oauth -- --nocapture`; `cargo test -p webcodex --lib scope -- --nocapture`; `cargo test -p webcodex --lib metadata -- --nocapture` |
| MCP conformance baseline | Exercise the real Salvo `/mcp` handlers through a disposable loopback fixture against a pinned upstream referee, then gate on check-level coverage/classification rather than process exit alone. | Loopback product fixture; immutable external referee source/dependencies fetched only by the explicit script/CI lane; no production credentials. | `bash scripts/mcp_conformance.sh`; see [`MCP_CONFORMANCE.md`](MCP_CONFORMANCE.md). |

The MCP Apps' DOM/message-order regression tests run without browser or npm
dependencies: `node --test src/mcp_tests/*.test.mjs`. They execute the embedded
HTML scripts with deterministic Host messages and timers, covering initialization
ordering, terminal Goal convergence, foreground dispatch, finish retries, and
successive continuation Attempts. Rust projection and capability tests use
`cargo test --locked -p webcodex --lib result_app`,
`cargo test --locked -p webcodex --lib goal`, and
`cargo test --locked -p webcodex --lib agent_continuation`.

## Read snapshots and range planning

`cargo test --locked -p webcodex --lib read_cache` covers bounded Session
snapshot retention and read-only singleflight ownership. Run
`cargo test --locked -p webcodex --lib read_files` for the canonical read path,
range planning, byte-limit fallback, revision fences, and Runner replacement.
These use deterministic in-memory Runner fixtures, not deployed Runners.

The Session cache is a transfer optimization: every hit validates the current
full-file SHA through a one-line read on the owning Runner. It does not skip
the Runner's full-file scan, use TTL/mtime as freshness evidence, or make a
revision an immutable filesystem snapshot. A changed SHA discards the cached
ranges: fenced reads reject a stale revision immediately, while unfenced reads
read the requested range again. Concurrent identical physical reads
share only pending work within the same authority, Session, exact Project and
Runner incarnation; each canonical call still authorizes, records and projects
its own result. Without an explicit authorized Session, only in-flight sharing
is enabled. Snapshot retention is bounded to 64 entries / 8 MiB per runtime,
and the in-flight index to 128 entries (excess reads bypass sharing).

## Full Local Server Suite

Plain `cargo test` must work without a special environment variable or wrapper,
including when launched from an interactive console. On high-core developer or shared
hosts, `bash scripts/test_server.sh` is an optional resource-control entry point for the
complete Server package lane. It runs the same package command as ordinary Linux CI
(`cargo test --locked -p webcodex`) but, when `RUST_TEST_THREADS` is unset, caps
libtest fan-out at the smaller of the detected logical CPU count and 32. Set
`RUST_TEST_THREADS` explicitly when intentionally testing another concurrency level.
Cargo `-j` / `CARGO_BUILD_JOBS` controls compilation, not libtest execution concurrency.
A passing limited-concurrency run does not prove a console-only failure was caused by
parallelism; keep the original failing entry point in the final regression evidence.

## Explicit High-Cost Local Evidence

Ordinary `cargo test` and ordinary CI intentionally skip ignored timing/real-process
coverage. Run the smallest relevant group locally when changing one of these boundaries:

```bash
cargo test --locked -p webcodex-runner -p webcodex-lsp --features runner-real-process-tests runner_real_process -- --ignored --test-threads=1
cargo test --locked -p webcodex-process --test managed_child -- --ignored --test-threads=1
cargo test --locked -p webcodex-persistent-shell -- --ignored --test-threads=1
cargo test --locked -p webcodex --lib tool_runtime_real_process_ -- --ignored --test-threads=1
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml desktop_real_process_windows_ -- --ignored --test-threads=1
```

These commands are evidence for the boundary they exercise, not a routine pre-commit
check. Prefer one exact ignored test while iterating, then the relevant group when the
change is ready for review.

## CI Mapping

The lanes above define test semantics; workflows decide when to run them.

- `.github/workflows/ci.yml` is the ordinary repository gate. Its cheap `changes`
  job classifies the exact PR base...head path set before native scheduling, while
  the `contract` job remains mandatory for every configured pull request and every
  push to `main`. The classifier is deterministic and local to Git: it does not use
  commit messages or PR titles, and it emits frontend, per-platform, and package-lane
  requirements. For changed Rust/Cargo files it searches only bounded platform-marker
  lines from both the base and head file versions, so body-only changes inside an
  existing platform cfg remain visible without serializing near-complete file diffs.
  If that marker scan exceeds its bound, CI fails closed to native core plus
  architecture compilation while preserving path-derived package/Desktop decisions;
  only an untrustworthy changed-path inventory falls back to the complete native
  matrix. The contract lane always owns workspace-boundary self-test/checks,
  formatting, the heuristic test-inventory self-test/report (without count thresholds),
  and focused registry/OpenAPI/MCP schema and metadata parity. Main and Desktop
  frontend dependency installation/type/test/build steps run only when the classifier
  selects their respective frontend surface; full-native invocations select both.
- The heavy Linux Rust matrix `test-linux-rust` and Linux tooling lane
  `test-linux-tooling` run for every pull request as well as every push to `main`,
  including owner-authored PRs. They start in parallel with `contract` rather than
  waiting for unrelated frontend/static work. Native child lanes likewise wait only
  for the cheap `changes` classifier, while the stable macOS/Windows/native aggregates
  retain the mandatory `contract` gate. Pushes to `main`, external-contributor PRs,
  and owner PRs carrying `run-ci` force the complete deterministic native matrix.
  Real-process and timing-sensitive ignored tests are deliberately outside ordinary
  CI, including full-native overrides: run them explicitly when changing their
  lifecycle boundary or investigating platform behavior. Computer, platform-specific,
  Desktop, npm, packaging, signing, and release surfaces retain their existing
  independent deterministic lanes. The stable `test-macos`,
  `test-windows`, and `test-native` aggregates always resolve and verify each child
  lane is `success` when required or `skipped` when not required, avoiding a skipped
  required-check context that could leave branch protection pending.
- MCP dated-revision evidence has its own bounded `mcp-conformance` lane. It pins
  and freshly builds the upstream referee, runs the `2026-07-28` and `2025-11-25`
  server requirements against a test-only loopback WebCodex endpoint, validates
  the report gate itself, and uploads raw reports on success or failure.
  `test-native` requires this lane to succeed, so missing per-scenario verdicts,
  abnormal/infrastructure runs, stale or changed classifications, and unclassified
  new failures are merge-blocking. See [`MCP_CONFORMANCE.md`](MCP_CONFORMANCE.md)
  for baseline semantics.
- Linux Rust execution remains package-sharded: the server package `webcodex`, the
  Runner/LSP packages, and the remaining workspace crates run in parallel. The
  Runner/LSP shard compiles with `--features runner-real-process-tests` to prevent
  bitrot while ordinary local runs skip compiling manual real-process test bodies;
  ordinary libtest execution does not execute ignored tests. The remainder shard uses
  `--workspace --exclude webcodex --exclude webcodex-runner --exclude webcodex-lsp`,
  so newly added workspace members enter CI automatically rather than depending on a
  hand-maintained package list. The split changes scheduling, not process-ownership coverage.
- Linux tooling runs in parallel with the Rust shards and retains
  release-verification tooling, Markdown-link validation, and npm package-smoke
  tooling on every PR. It also runs the dependency-free MCP App DOM/message-order
  tests with its existing Node installation, independently of frontend path
  classification. The complete `cargo check --workspace --all-targets` pass is
  reserved for pushes to `main`, external-contributor PRs, and explicit `run-ci` PRs;
  ordinary owner PRs already pay for the package-sharded Rust test compilation and do
  not repeat that broad compile-only pass. macOS and Windows native jobs compile Runner,
  LSP, and Computer with `--features runner-real-process-tests` and keep deterministic
  native coverage, but they do not execute ignored real-process groups. Process-tree,
  detached-supervisor, shell timeout/stop, PowerShell stdin EOF, fake-SSH lifecycle,
  selected Plugin shutdown, and similar OS-scheduling-sensitive coverage is retained
  as explicit local evidence. The local-`sshd` SSH integration fixture remains
  Linux-only and manual because it depends on Linux daemon account/auth configuration.
- Exact-source release acceptance is a separate trust boundary from ordinary CI.
  Release readiness first binds the exact source to a successful `main`-push CI run;
  main pushes force the complete deterministic native classification. Readiness then
  runs its release-specific E2E/eval and disposable Server-image checks. Manual
  real-process evidence remains separate and must never be inferred from a passing CI
  run. Follow [`RELEASE_CHECKLIST.md`](RELEASE_CHECKLIST.md) and
  `.github/workflows/release-readiness.yml`.

## Default Test Principles

- No external network by default. Tests that need HTTP should use in-process
  clients or loopback mock servers. Real internet, real cloud services, and real
  deployment targets belong in explicitly named manual smoke workflows.
- Local mock server tests must be isolated. Bind to `127.0.0.1:0`, avoid fixed
  ports, scope URL rewrites to the test fixture, reset global overrides even on
  failure paths, and stop spawned tasks when the fixture drops.
- Prefer explicit config inputs or child-local `Command::env` / `env_remove` over
  mutating the parallel test process's environment. Existing tests that must mutate
  process environment require `TEST_ENV_LOCK` or an equivalent shared guard and
  panic-safe restoration of previous values. A lock used only by writers does not
  isolate unguarded readers or children inheriting temporary values; use an isolated
  child process when those readers cannot be controlled. Do not print token values
  while diagnosing these tests.
- Tests that touch HTTP/auth behavior must use `AuthEnvGuard` or an equivalent
  `TEST_ENV_LOCK` guard for auth mode env, especially
  `WEBCODEX_SHARED_KEY_ENABLED`, `WEBCODEX_ALLOW_ANONYMOUS`,
  and `WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE`. Managed-token rejection tests should
  explicitly disable direct shared-key fallback and open anonymous mode before
  asserting that an unknown or wrong bearer returns 401.
- Keep the auth-mode semantics separate in tests:
  `WEBCODEX_SHARED_KEY_ENABLED` is direct Bearer shared-key fallback, while
  `WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE` is only the OAuth authorize bridge.
  Quick-start shared-key mode intentionally accepts an unknown non-`wc_` Bearer
  as a lightweight shared-key principal, but invalid `wc_` managed-token
  prefixes and empty or whitespace Bearer values must still be rejected.
- Sleep, timeout, and polling tests must be bounded. Positive readiness uses one
  absolute deadline created once for the whole wait and never reset after partial
  progress; prefer channels, notifications, or direct state inspection. A test whose
  correctness materially depends on real process startup, scheduler timing, a long
  sleep, or a wall-clock timeout belongs in ignored explicit evidence rather than the
  default suite. Short deterministic negative probes, semantic grace windows, and
  exact count/protocol iterations may remain when they are the contract.
  Async Runner shutdown waits are notification-driven by `ShutdownCoordinator`;
  tests should signal that state directly rather than sleep for a presumed polling interval.
- Ignored tests are not dead tests. Each ignored test should have a reason and a
  documented command for running it intentionally. Ordinary CI never opts into
  `--ignored`: `runner_real_process_`, `desktop_real_process_windows_`, slow timing
  regressions, and real Codex/LSP dogfood remain explicit local/operator evidence.

## Process Fixture Contracts and Regression Workflow

The local Runner fixture is an adapter, not a second execution contract. Keep its
stdin, output capture, exit-status, deadline, and cleanup behavior aligned with the
production Runner. In particular, `Command::spawn()` inherits stdin by default whereas
`Command::output()` closes it by default; switching between them requires an explicit
I/O decision. In the common test helper:

- `None` must configure `Stdio::null()`, overriding any preconfigured stdin.
  `Some(b"")` must also produce EOF, while a nonempty payload supplies exactly its
  bytes followed by EOF. No ordinary fixture reads the developer's terminal.
- Capture both output streams without waiting on undrained bounded pipes, preserve
  exit status, and retain bounded waits plus cleanup of fixture-owned processes.
  Explicit input larger than pipe capacity must not deadlock the harness either.
- Keep cwd, temporary repositories, child environment, and input handles local to the
  fixture. Do not change process-global stdin or environment to simulate a console.

`src/tool_runtime/tests/runner_fixtures.rs` provides ordinary deterministic coverage
for missing, empty, and large explicit input, plus a subprocess regression of the
existing unborn-repository workflow. That test re-executes one exact libtest case with
finite invalid parent input: an incorrect fixture lets `git mktree` consume the input
and fail immediately, even in headless CI. It verifies that the child actually runs
one test. With an interactive console or an open pipe, the same defect instead waits
for EOF and surfaces as a misleading Runner command timeout. These tests assert I/O
semantics, not scheduler latency, so they belong in the ordinary lane:

```bash
cargo test --locked -p webcodex --lib runner_fixtures
```

For a newly reported console-only or cross-suite failure, use this sequence:

1. Record the exact revision, command, test target, executed count, input mode, and
   relevant concurrency settings. Separate compile time from test execution time.
   Compare the same case with closed stdin and with a parent-owned pipe whose writer
   stays open. Do not use `communicate()` for that probe: it closes the child's stdin
   and can hide an EOF bug. Bound the probe and clean up only its owned processes.
2. Make the smallest deterministic regression fail on the old implementation before
   changing the helper. Use finite poisoned input or controlled local state where
   possible, rather than making every CI run wait for the original timeout.
3. After the fix, run the focused regression and repeat the original failing mode.
   For changes to a shared fixture or cross-suite behavior, also run the affected
   package at default concurrency; a filtered/serial pass alone is insufficient.
   Let Cargo select/build the current test binary rather than guessing a potentially
   stale executable under `target/`.

As related test areas are changed, improve them incrementally: move pure projection,
parsing, and policy assertions out of real Git/shell workflows; retain a small set of
real process-boundary contract tests; inject settings rather than adding new global
environment guards; and replace scheduler sleeps with owned readiness signals and
one absolute deadline. Keep actual lifecycle/timing evidence in the existing explicit
lanes. Do not weaken assertions, ignore a deterministic failure, extend deadlines, or
serialize the entire suite merely to obtain a green run.

## `import_http` Coverage

Conversation-import tests use bounded loopback fixtures in the ordinary local-integration
surface. Legacy ignored HTTP fixtures for redirect and download-size limits were retired
after equivalent boundary coverage moved to the current MCP import path; source-name
fallback is covered directly at the import-name helper. Keep new coverage on the current
transport/runtime path instead of preserving duplicate historical fixtures.

Run the current heuristic inventory with:

```bash
bash scripts/test_inventory.sh
```

The script is intentionally heuristic. It scans all Git-tracked Rust files across
the workspace, so crate-local tests (including Runner tests) are included. Using
the Git index as the source set excludes ordinary untracked `target/` output and
scratch files without maintaining a second ignore list. It does not access the
network or modify the workspace. The output includes a
stable tab-separated area summary for the root `webcodex` package and each
`crates/*` member, plus sanitized risk clues. Use
`bash scripts/test_inventory.sh --details` for a full sanitized file/line list,
and `bash scripts/test_inventory.sh --self-test` to exercise the inventory
contract against a temporary Git fixture.

## Current Test Layout Notes

Recent structure work moved large test groups out of production roots:

- OAuth HTTP endpoint tests are rooted at `src/oauth_http/tests.rs` and grouped
  by endpoint/domain under `src/oauth_http/tests/*`.
- CLI tests are grouped under
  `crates/webcodex-cli/src/webcodex_cli/tests/*`.
- CLI help smoke coverage lives with the CLI test modules and covers common
  help entry points, so new command help should extend that smoke coverage.
- Runtime HTTP tests live under `src/runtime_http/tests/*`; historical ignored
  import fixtures should not be retained once equivalent current-path coverage exists.
- Tool runtime tests live under `src/tool_runtime/tests/*` by domain.

Do not add large ordinary test blocks to production facade files when one of
these `tests/` module trees already exists. Exact full-suite pass counts should
come from a fresh `cargo test -p webcodex --lib` run; this document should not be
treated as the source of truth for exact counts.

## Runner observability contract

Run `python scripts/tests/test_runner_observability_contract.py` for the exact-name
projection guard used by CI. It covers observation producers and readers while
leaving Runner registration wire keys and durable Agent APIs unchanged. Behavior
coverage lives in the `metadata`, `runtime_http`, `runtime_console_http`,
`admin_http`, `runner_capabilities`, and `startup_runner_tests` Server filters,
and the CLI `ops` and `server::status` filters. Run
`pwsh -NoProfile -File scripts/test_windows_runner_readiness.ps1` for Windows
readiness parsing. These checks do not replace Linux socket-activation E2E.
