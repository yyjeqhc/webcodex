# CI Lanes Proposal

This is a proposal for organizing WebCodex validation lanes. It is not a CI
configuration rewrite. The intent is to make the default lane fast and reliable
while keeping local integration, security, and smoke coverage visible.

## Proposed Lanes

| Lane | Trigger | Commands | Notes |
|---|---|---|---|
| static and compile | Every code PR. | `cargo fmt --check`; `cargo check --all-targets`; `git diff --check` | Required for normal code changes. Docs-only changes may use `git diff --check` plus status. |
| contract/schema | Every PR touching runtime tools, metadata, MCP, OpenAPI, OAuth scope policy, or registry code. | `cargo test --bin webcodex metadata`; `cargo test --bin webcodex mcp`; `cargo test --bin webcodex openapi` | Keeps tool names, schemas, and exposed action surfaces synchronized. |
| fast unit | Every code PR, or path-filtered when the suite grows. | Focused `cargo test --bin webcodex <filter>` commands for parsers, helpers, guards, and pure validation. | No external network, no long-running service, no unguarded env mutation. |
| local integration | Path-filtered PRs and scheduled runs. | `cargo test --bin webcodex runtime_http -- --nocapture`; `cargo test --bin webcodex session -- --nocapture` | In-process HTTP and loopback fixtures are allowed when isolated. Fixed ports and real internet are not. |
| security auth matrix | Auth, OAuth, scope, session guard, principal, or token changes. | `cargo test --bin webcodex oauth -- --nocapture`; `cargo test --bin webcodex scope -- --nocapture`; `cargo test --bin webcodex metadata -- --nocapture` | Denial paths and exact error contracts should keep explicit assertions. |
| slow/manual ignored | Manual or scheduled only until fixtures are serial and deterministic. | `cargo test --bin webcodex import_http -- --ignored --nocapture --test-threads=1` | Current home for the ignored `import_http` local mock server tests. |
| e2e/deployment smoke | Manual release validation and scheduled local smoke. | `bash scripts/e2e_zero_config_ws.sh`; `E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh`; `bash scripts/smoke_deployment.sh` | These may start temporary local services. Do not point them at real deployment targets unless the operator explicitly requests that. |

## Default CI Rules

- No external network in default PR CI. If a workflow needs the internet or a
  real deployment target, name it as manual smoke and require explicit operator
  input.
- No long-running services in default PR CI. E2E scripts must own their process
  lifecycle, use temporary directories, and clean up on failure.
- Local mock server tests must bind loopback on dynamic ports and avoid global
  rewrites unless guarded by a serial lane.
- Env mutation tests must use `TEST_ENV_LOCK` or an equivalent lock. The lane
  should fail if tests print real token values.
- Polling and sleep-based tests must be bounded by a clear timeout and should
  prefer notifications or state inspection where possible.

## `import_http` Lane Proposal

The four ignored `import_http` tests in `src/runtime_http.rs` should eventually
converge under a serial local integration lane:

```bash
cargo test --bin webcodex import_http -- --ignored --nocapture --test-threads=1
```

That lane should stay separate from fast unit and contract/schema validation
until the fixture is fully scoped:

- the download URL rewrite is reset through a guard,
- mock server tasks are shut down deterministically,
- agent completion waits have bounded failure messages,
- temp-dir project writes are isolated,
- the lane remains loopback-only.

Once those constraints are true, the lane can run on a schedule or on paths that
touch artifact import, runtime HTTP, downloader safety, or agent artifact save
behavior. It should not become part of the default fast lane merely because it
does not use the external internet.

## Inventory Command

Use the heuristic inventory before and after test-structure work:

```bash
bash scripts/test_inventory.sh
```

The output is a guide for triage, not a parser-quality source of truth. It
counts test attributes and reports sanitized clues for ignored tests, sleep or
timeout use, loopback listeners, env mutation, and `TEST_ENV_LOCK`. Use
`bash scripts/test_inventory.sh --details` when file/line-level triage is
needed. Large changes should reduce unowned risk clues or explain why they
remain.
