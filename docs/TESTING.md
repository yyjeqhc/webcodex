# Testing Strategy

WebCodex has a large test surface because the product has several independent
contracts that must stay aligned: runtime tools, session guards, project and
file operations, Git and shell dispatch, agent transports, MCP, OpenAPI, OAuth
scope policy, and GPT Action exposure. The count is therefore mostly reasonable
complexity, not accidental expansion. The risk is not the number of tests by
itself; the risk is unclear layering, global state leakage, unbounded waits, and
tests with different cost profiles sharing the same default lane.

## Test Lanes

| Lane | Purpose | Default resources | Typical command |
|---|---|---|---|
| fast unit | Pure parsing, validation, helpers, local state machines, small fixtures. | No network, no global env mutation, no long sleeps. | `cargo test -p webcodex --lib tool_call` |
| contract/schema | Keep metadata, registry, MCP `tools/list`, OpenAPI, and runtime tool names synchronized. | No external network; in-process services are preferred. | `cargo test -p webcodex --lib metadata`; `cargo test -p webcodex --lib mcp`; `cargo test -p webcodex --lib openapi` |
| local integration | Exercise HTTP handlers, runtime dispatch, sessions, local agent registry, temp dirs, loopback listeners, and database fixtures. | Loopback only, isolated temp dirs, bounded waits, no shared mutable state without a lock. | `cargo test -p webcodex --lib runtime_http -- --nocapture`; `cargo test -p webcodex --lib session -- --nocapture` |
| slow/manual ignored | Valuable coverage that is local but slow, serial, large-input, or global-state-sensitive. | Explicit operator opt-in; often `--ignored` and `--test-threads=1`. | Run the specific ignored test/filter documented by its subsystem. |
| e2e/deployment smoke | Prove that binaries, local services, GPT Actions schema, MCP, artifact transfer, and an agent can work together. | Temporary local services and loopback ports; real deployment only when explicitly requested. | `bash scripts/e2e_zero_config_ws.sh`; `bash scripts/smoke_deployment.sh`; `bash scripts/smoke_artifact_transfer.sh` |
| reconnect continuity | Runner disconnect/reconnect layer independence, stale-not-ready observations, server-restart durable Session plus exact binding restoration, lost-job terminal semantics, meaningful-activity scoping, and version-mismatch diagnostics. | In-process fixtures, no external network. | `cargo test -p webcodex --lib reconnect` |
| trusted smoke | Disposable git fixture full chain (start → edit → failing shell validation → fix → pass → git review → finish) asserting zero approval interruptions under `trusted_agent` authority, resolved failure evidence, dirty-worktree advisory-only, and bounded payloads; prints baseline counters. | Temp git fixture, no external network. | `cargo test -p webcodex --lib trusted_smoke` |
| real-process reconnect harness | Boot a real server plus runner, assert layered connection observations, crash the runner (layers degrade independently; running job goes terminal `lost`), restart the runner (new connection instance, no server restart), then restart the server while preserving one HTTP window cookie and verify runner auto-reconnect, durable Session lookup by explicit id, and exact binding restoration to the original Session. It also prints post-deploy smoke facts (server version/commit, authority mode, version compatibility, runner shell dialect). | Local processes and loopback ports. | `bash scripts/e2e_reconnect_ws.sh` |
| real-process hosted-connect harness | Build the real Server, Runner, and CLI; start a shared-key-enabled loopback Server; run `webcodex connect`; verify same-key project visibility and a read, cross-key isolation, detached Runner survival, repeated-connect PID reuse, hosted `agent status`, explicit stop, secret-safe output/log/state, and an untouched Git checkout. | Local processes, isolated XDG config/state roots, a temp Git project, bounded curl and outer timeout, trap cleanup; never production. | `bash scripts/e2e_hosted_connect.sh` |
| real-process job reconciliation harness | Boot a real server plus a WebSocket runner that advertises `job_state_reconciliation`. Scenario A keeps a raw async Job running across a SERVER-only restart and asserts the SAME runner instance, original `job_id`, preserved ownership/project/session, non-regressing sequence/log cursors, `recovered_after_server_restart`, original-process stop, and one side-effect set. Scenario B lets a Job complete while the Server is offline and reconciles the terminal result without duplicate logs or execution. Scenario C forces `run_process` past its synchronous grace window, then proves the handed-off structured Job survives a Server restart and an old Server-epoch observation token refreshes immediately for the same `job_id`. Scenario D uses a delayed Cargo fixture to force a real `cargo_check` validation handoff past its sync window, then proves the same restart/token-refresh/stop contract with the validation command started exactly once. Ordinary Runner-owned Jobs keep the Runner process alive for these scenarios; `run_detached_process` restart survival is a separate supervisor-ownership contract covered by its focused Runner suites and production dogfood. | Local processes, temp dirs/ports/tokens, and a temp project; no production services or QUIC certs. Scenario D intentionally takes roughly the validation sync window plus restart time. | `bash scripts/e2e_job_reconciliation_ws.sh` |
| real-process job recovery failure/compat harness | Cover the failure and compatibility paths the happy-path reconciliation harness omits, using `WEBCODEX_JOB_RECOVERY_GRACE_SECS=10` (clamped, above the 5s floor) so the deadline is bounded without waiting the 120s default. Scenario C: kill the runner only (server stays up), let the job enter `recovering`, and assert the non-request-triggered recovery-timeout sweep transitions it to `lost` with `runner_recovery_deadline_exceeded`, `ended_at` set once, one list record, stop-on-lost stable, and the command never re-executes. Scenario D: instance B replaces instance A (same client_id, new `agent_instance_id`); A's job becomes `lost` with `runner_instance_replaced`, B starts its own new job, A's late update is rejected, first `ended_at`/reason preserved. Scenario E: a legacy runner registered with `WEBCODEX_RUNNER_DISABLE_JOB_STATE_RECONCILIATION=1` (no capability, no inventory) dispatches a job and, on disconnect, deterministically fences it to `lost` with `legacy_runner_disconnected` (never `recovering`); after a server restart the lost job has no durable record and a same-client new instance cannot revive it. Scenario F: a long job across three server restarts keeps the same `job_id`, runs the command once, keeps `last_update_seq`/log cursors non-regressing and markers non-duplicating, and reaches a terminal `stopped` that survives a third restart with `ended_at` unchanged by terminal inventory replay. | Local processes, temp dirs/ports/tokens, and a temp project; no production services or QUIC certs. | `bash scripts/e2e_job_recovery_failures_ws.sh` |
| security auth matrix | Cover OAuth, scope policy, shared-key behavior, token classes, read-only session guards, and denied mutations. | No external identity provider by default; use local fixtures and synthetic tokens. | `cargo test -p webcodex --lib oauth -- --nocapture`; `cargo test -p webcodex --lib scope -- --nocapture`; `cargo test -p webcodex --lib metadata -- --nocapture` |

## CI Mapping

The lanes above define test semantics; workflows decide when to run them.

- `.github/workflows/ci.yml` is the ordinary repository gate. Every configured
  pull-request workflow run gets the lightweight `contract` job without requiring
  the `run-ci` label. It covers frontend type/test/dist checks, workspace-boundary
  checks, formatting, and focused registry/OpenAPI/MCP schema and metadata parity.
- The heavier Linux `test`, native macOS `test-macos`, and native Windows
  `test-windows` jobs run on every push to `main`. External pull requests run
  them automatically subject to GitHub fork protections; owner-authored pull
  requests opt in with the `run-ci` label.
- Heavy Linux CI runs frontend checks, workspace-boundary and release-tooling
  checks, Markdown-link validation, formatting, and the locked workspace test
  suite. macOS CI compiles the release production surfaces and runs the native
  Runner suite, including detached ownership/restart recovery. The local-`sshd`
  SSH integration fixture remains Linux-only because it depends on Linux daemon
  account/auth configuration; macOS still compiles and tests the SSH client and
  pure command-shaping surface. Windows CI runs formatting, native Windows
  package tests, npm checks, and the Windows artifact-to-install smoke.
- Exact-source release acceptance is separate from ordinary pull-request CI.
  Follow [`RELEASE_CHECKLIST.md`](RELEASE_CHECKLIST.md) and
  `.github/workflows/release-readiness.yml`.
- Slow/manual and real-process lanes remain explicit targeted evidence unless
  a workflow names them. Do not infer that one lane ran merely because another CI
  job passed.

## Default Test Principles

- No external network by default. Tests that need HTTP should use in-process
  clients or loopback mock servers. Real internet, real cloud services, and real
  deployment targets belong in explicitly named manual smoke workflows.
- Local mock server tests must be isolated. Bind to `127.0.0.1:0`, avoid fixed
  ports, scope URL rewrites to the test fixture, reset global overrides even on
  failure paths, and stop spawned tasks when the fixture drops.
- Tests that mutate process environment must acquire `TEST_ENV_LOCK` or an
  equivalent shared guard, save the previous value, and restore or remove it at
  the end. Do not print token values while diagnosing these tests.
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
- Sleep, timeout, and polling tests must have bounded timeouts. Prefer channels,
  notifications, direct state inspection, or bounded retry loops over raw sleeps.
- Ignored tests are not dead tests. Each ignored test should have a reason and a
  documented lane for running it intentionally.

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
