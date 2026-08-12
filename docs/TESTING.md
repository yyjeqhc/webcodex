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
| slow/manual ignored | Valuable coverage that is local but slow, serial, large-input, or global-state-sensitive. | Explicit operator opt-in; often `--ignored` and `--test-threads=1`. | `cargo test -p webcodex --lib import_http -- --ignored --nocapture --test-threads=1` |
| e2e/deployment smoke | Prove that binaries, local services, GPT Actions schema, MCP, artifact transfer, and an agent can work together. | Temporary local services and loopback ports; real deployment only when explicitly requested. | `bash scripts/e2e_zero_config_ws.sh`; `bash scripts/smoke_deployment.sh`; `bash scripts/smoke_artifact_transfer.sh` |
| reconnect continuity | Runner disconnect/reconnect layer independence, stale-not-ready observations, server-restart durable Session plus exact binding restoration, lost-job terminal semantics, meaningful-activity scoping, and version-mismatch diagnostics. | In-process fixtures, no external network. | `cargo test -p webcodex --lib reconnect` |
| trusted smoke | Disposable git fixture full chain (start → edit → failing shell validation → fix → pass → git review → finish) asserting zero approval interruptions under `trusted_agent` authority, resolved failure evidence, dirty-worktree advisory-only, and bounded payloads; prints baseline counters. | Temp git fixture, no external network. | `cargo test -p webcodex --lib trusted_smoke` |
| real-process reconnect harness | Boot a real server plus runner, assert layered connection observations, crash the runner (layers degrade independently; running job goes terminal `lost`), restart the runner (new connection instance, no server restart), then restart the server while preserving one HTTP window cookie and verify runner auto-reconnect, durable Session lookup by explicit id, and exact binding restoration to the original Session. It also prints post-deploy smoke facts (server version/commit, authority mode, version compatibility, runner shell dialect). | Local processes and loopback ports. | `bash scripts/e2e_reconnect_ws.sh` |
| real-process hosted-connect harness | Build the real Server, Runner, and CLI; start a shared-key-enabled loopback Server; run `webcodex connect`; verify same-key project visibility and a read, cross-key isolation, detached Runner survival, repeated-connect PID reuse, hosted `agent status`, explicit stop, secret-safe output/log/state, and an untouched Git checkout. | Local processes, isolated XDG config/state roots, a temp Git project, bounded curl and outer timeout, trap cleanup; never production. | `bash scripts/e2e_hosted_connect.sh` |
| real-process job reconciliation harness | Boot a real server plus a WebSocket runner that advertises `job_state_reconciliation`, start a long-running async job with deterministic marker output, stop the SERVER only (runner and job process stay alive), restart the server, and assert the SAME runner instance reconciles the in-flight job: original `job_id`, preserved ownership/project/session, non-regressing `last_update_seq`, non-duplicating monotonic log cursor, `recovered_after_server_restart` metadata, and `stop_job(original_job_id)` driving the original process group to `stopped` (never `lost`, single side-effect set). A second scenario starts a short job, stops the server while it runs, lets it complete on the runner, restarts the server, and asserts it reconciles to `completed` with `exit_code=0` and markers appearing exactly once. Runner process restart is intentionally out of scope; the runner process must stay alive. | Local processes, temp dirs/ports/tokens, and a temp project; no production services or QUIC certs. | `bash scripts/e2e_job_reconciliation_ws.sh` |
| real-process job recovery failure/compat harness | Cover the failure and compatibility paths the happy-path reconciliation harness omits, using `WEBCODEX_JOB_RECOVERY_GRACE_SECS=10` (clamped, above the 5s floor) so the deadline is bounded without waiting the 120s default. Scenario C: kill the runner only (server stays up), let the job enter `recovering`, and assert the non-request-triggered recovery-timeout sweep transitions it to `lost` with `runner_recovery_deadline_exceeded`, `ended_at` set once, one list record, stop-on-lost stable, and the command never re-executes. Scenario D: instance B replaces instance A (same client_id, new `agent_instance_id`); A's job becomes `lost` with `runner_instance_replaced`, B starts its own new job, A's late update is rejected, first `ended_at`/reason preserved. Scenario E: a legacy runner registered with `WEBCODEX_RUNNER_DISABLE_JOB_STATE_RECONCILIATION=1` (no capability, no inventory) dispatches a job and, on disconnect, deterministically fences it to `lost` with `legacy_runner_disconnected` (never `recovering`); after a server restart the lost job has no durable record and a same-client new instance cannot revive it. Scenario F: a long job across three server restarts keeps the same `job_id`, runs the command once, keeps `last_update_seq`/log cursors non-regressing and markers non-duplicating, and reaches a terminal `stopped` that survives a third restart with `ended_at` unchanged by terminal inventory replay. | Local processes, temp dirs/ports/tokens, and a temp project; no production services or QUIC certs. | `bash scripts/e2e_job_recovery_failures_ws.sh` |
| security auth matrix | Cover OAuth, scope policy, shared-key behavior, token classes, read-only session guards, and denied mutations. | No external identity provider by default; use local fixtures and synthetic tokens. | `cargo test -p webcodex --lib oauth -- --nocapture`; `cargo test -p webcodex --lib scope -- --nocapture`; `cargo test -p webcodex --lib metadata -- --nocapture` |

Iteration 9 execution/reporting changes use the existing domain lanes rather
than a new test suite:

- `cargo test -p webcodex --lib validation_events -- --nocapture` covers dedicated
  and declared-purpose generic execution evidence plus exact retry identity;
- `cargo test -p webcodex --lib tool_runtime::tests::jobs -- --nocapture` covers
  shell/cwd metadata, bounded job tails, detected summaries, and cursors;
- `cargo test -p webcodex --lib tool_runtime::tests::coding_task -- --nocapture`
  and `cargo test -p webcodex --lib tool_runtime::tests::handoff -- --nocapture`
  cover facts/advisories/hard blockers and the `detail=minimal|standard|full`
  startup projection (the only startup projection control);
- `cargo test -p webcodex --lib read_file -- --nocapture`, `metadata`, `mcp`, and
  `openapi` cover the single read representation, layered readiness, and
  project-bound versus operator surfaces.

## Iteration 9 Final Acceptance

The Iteration 9 close-out is a real-process, real-protocol pass, not in-process
function calls. Run these lanes and procedures against binaries built from the
current tree:

Focused and real-process lanes:

```bash
cargo test -p webcodex --lib trusted_smoke
cargo test -p webcodex --lib reconnect
cargo test -p webcodex --lib select_lines_tests   # bounded job-log tail + non-duplicating cursor
bash scripts/e2e_reconnect_ws.sh               # real server+runner restart/reconnect, durable session
bash scripts/e2e_job_reconciliation_ws.sh      # real active-job reconciliation across a server restart (runner stays alive)
bash scripts/e2e_job_recovery_failures_ws.sh   # real runner-loss / instance-replace / legacy / repeated-restart failure & compat paths
bash scripts/e2e_zero_config_ws.sh             # real MCP initialize/tools_list/tools_call + REST workflow
```

Real project-bound MCP and OpenAPI/HTTP acceptance against a connector-configured
process (no operator token, no `/opt/webcodex`):

```bash
tmp=$(mktemp -d); repo="$tmp/repo"; state="$tmp/state"
git -C "$repo" init -q 2>/dev/null || { mkdir -p "$repo"; git -C "$repo" init -q; }
# ... seed a commit in "$repo" ...
webcodex setup --root "$repo" --state-dir "$state" --json
webcodex agent start --root "$repo" --state-dir "$state" &   # boots server+runner+connector
port=$(grep -oP 'port = \K[0-9]+' "$state/project.toml")
conn=$(cat "$state/credentials/connector-key")
# MCP JSON-RPC: initialize, tools/list (exactly 13 canonical, no operator runtime),
# then task_start → files_read → code_navigate → edits_apply → commands_run → checks_run →
# task_review → task_finish, and task_resume after a fresh initialize:
curl -fsS -H "Authorization: Bearer $conn" -H 'Content-Type: application/json' \
  -X POST "http://127.0.0.1:$port/mcp" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
# OpenAPI projection is the same 12 operations:
curl -fsS -H "Authorization: Bearer $conn" "http://127.0.0.1:$port/openapi.json"
# Boundary: a project credential must be denied operator-only routes (HTTP 403):
curl -s -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $conn" \
  -H 'Content-Type: application/json' -X POST "http://127.0.0.1:$port/api/tools/call" \
  -d '{"tool":"read_file","params":{"project":"x","path":"README.md"}}'
```

Verify `trusted_agent` auto-authorizes `commands_run` (no approval interruption)
while `WEBCODEX_AUTHORITY_MODE=restricted agent start` makes the same call return
`approval_required`. For the deployed-host equivalent (runtime status, authority,
connection layers, version compatibility, shell dialect, and the finish/handoff
smoke), see [Deployment](DEPLOYMENT.md#smoke-checks).

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

## Current `import_http` Inventory

`src/runtime_http/tests/import_http_tests.rs` currently contains four ignored
`import_http` tests:

- `import_http_does_not_follow_302_redirect`
- `import_http_rejects_content_length_over_limit`
- `import_http_rejects_chunked_body_after_limit_without_content_length`
- `import_http_success_uses_source_name_fallback_for_missing_target`

These tests do not access the external internet. They use a loopback mock HTTP
server, rewrite the import download base URL, create temporary project roots,
and in one success case drive asynchronous agent completion. They remain
ignored because they combine several local-integration risks:

- a global download URL override that must be reset,
- a serial import test lock that protects the test body but still makes the lane
  unsuitable for high-parallel default runs,
- raw loopback listener setup and spawned async server tasks,
- large body coverage around `MAX_IMPORT_FILE_BYTES`,
- polling with short sleeps while waiting for agent requests,
- temp-dir project roots and artifact writes.

This coverage is useful and should be preserved. The current default behavior is
to keep it out of the fast and contract/schema lanes until the fixture can be
made deterministic enough for a serial local integration lane.

## Path To A Serial Local Integration Lane

The next structural step is to run the four `import_http` ignored tests under a
named serial local integration lane without changing their assertions:

1. Keep the tests local-only and run them with `--test-threads=1`.
2. Replace the global URL rewrite with a fixture-scoped guard or downloader
   injection, if the runtime boundary allows it.
3. Replace sleep polling for agent completion with a bounded notification or a
   helper that fails with a clear timeout.
4. Keep the mock HTTP server on `127.0.0.1:0` and make task shutdown explicit.
5. Promote the lane from manual to scheduled or path-filtered CI only after the
   inventory shows no unguarded env mutation, leaked global state, or unbounded
   waits.

Run the current heuristic inventory with:

```bash
bash scripts/test_inventory.sh
```

The script is intentionally heuristic. It scans only `src`, `docs`, and `tests`
when those directories exist, does not access the network, does not modify the
workspace, and reports counts plus sanitized risk clues. Use
`bash scripts/test_inventory.sh --details` for a full sanitized file/line list.

## Current Test Layout Notes

Recent structure work moved large test groups out of production roots:

- OAuth HTTP endpoint tests are rooted at `src/oauth_http/tests.rs` and grouped
  by endpoint/domain under `src/oauth_http/tests/*`.
- CLI tests are grouped under
  `crates/webcodex-cli/src/webcodex_cli/tests/*`.
- CLI help smoke coverage lives with the CLI test modules and covers common
  help entry points, so new command help should extend that smoke coverage.
- Runtime HTTP tests live under `src/runtime_http/tests/*`; the only currently
  ignored tests tracked by the inventory are the four `import_http` tests listed
  above.
- Tool runtime tests live under `src/tool_runtime/tests/*` by domain.

Do not add large ordinary test blocks to production facade files when one of
these `tests/` module trees already exists. Exact full-suite pass counts should
come from a fresh `cargo test -p webcodex --lib` run; recent full-suite scale is
roughly 1.7k passing tests plus the four ignored `import_http` tests, but this
document should not be treated as the source of truth for exact counts.
