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
| fast unit | Pure parsing, validation, helpers, local state machines, small fixtures. | No network, no global env mutation, no long sleeps. | `cargo test --bin webcodex tool_call` |
| contract/schema | Keep metadata, registry, MCP `tools/list`, OpenAPI, and runtime tool names synchronized. | No external network; in-process services are preferred. | `cargo test --bin webcodex metadata`; `cargo test --bin webcodex mcp`; `cargo test --bin webcodex openapi` |
| local integration | Exercise HTTP handlers, runtime dispatch, sessions, local agent registry, temp dirs, loopback listeners, and database fixtures. | Loopback only, isolated temp dirs, bounded waits, no shared mutable state without a lock. | `cargo test --bin webcodex runtime_http -- --nocapture`; `cargo test --bin webcodex session -- --nocapture` |
| slow/manual ignored | Valuable coverage that is local but slow, serial, large-input, or global-state-sensitive. | Explicit operator opt-in; often `--ignored` and `--test-threads=1`. | `cargo test --bin webcodex import_http -- --ignored --nocapture --test-threads=1` |
| e2e/deployment smoke | Prove that binaries, local services, GPT Actions schema, MCP, and an agent can work together. | Temporary local services and loopback ports; real deployment only when explicitly requested. | `bash scripts/e2e_zero_config_ws.sh`; `bash scripts/smoke_deployment.sh` |
| security auth matrix | Cover OAuth, scope policy, shared-key behavior, token classes, read-only session guards, and denied mutations. | No external identity provider by default; use local fixtures and synthetic tokens. | `cargo test --bin webcodex oauth -- --nocapture`; `cargo test --bin webcodex scope -- --nocapture`; `cargo test --bin webcodex metadata -- --nocapture` |

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
- Sleep, timeout, and polling tests must have bounded timeouts. Prefer channels,
  notifications, direct state inspection, or bounded retry loops over raw sleeps.
- Ignored tests are not dead tests. Each ignored test should have a reason and a
  documented lane for running it intentionally.

## Current `import_http` Inventory

`src/runtime_http.rs` currently contains four ignored `import_http` tests:

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
