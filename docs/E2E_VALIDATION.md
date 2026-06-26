# E2E Validation

This document describes the local end-to-end validation harness that proves the
WebCodex runtime — server + WebSocket agent + GPT Actions schema + MCP
JSON-RPC — runs correctly on a single host **before** pointing real ChatGPT at
it. It does not depend on the ChatGPT web UI or the real Codex CLI.

## Why this exists

Phase 15 is about "can it run before deploy?", not new features. The harness
boots a real `webcodex` server and a real `webcodex-agent` on the same
machine, wires them over the WebSocket transport, and exercises every public
surface with `curl`. If the harness passes, the same flow is what a real
ChatGPT GPT Action import will drive.

## Quick start

From the repo root:

```bash
bash scripts/e2e_zero_config_ws.sh
```

Requirements:

- `cargo` (builds and runs both binaries).
- `curl`, `python3`, `git` on `PATH`.
- A free TCP port on `127.0.0.1` (auto-picked, or set `E2E_PORT`).
- No real Codex CLI needed — the script creates a stub `CODEX_BIN`.

Expected output ends with:

```
[e2e] ---- summary ----
[e2e] passed: <N>
[e2e] failed: 0
[e2e] E2E smoke PASSED
```

On failure the script prints the server and agent log paths so you can inspect
what went wrong:

```
[e2e] ---- log locations ----
[e2e] server log: /tmp/webcodex-e2e-XXXXXX/server.log
[e2e] agent log:  /tmp/webcodex-e2e-XXXXXX/agent.log
```

## Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `E2E_PORT` | auto-pick | Bind port for the server. |
| `E2E_TOKEN` | `e2e-smoke-token` | Bearer token (`WEBCODEX_TOKEN`) for both server and agent. |
| `E2E_CLIENT_ID` | `e2e-agent` | Agent `client_id`. |
| `E2E_PROJECT_ID` | `smoke-proj` | Agent-side project id. The runtime id becomes `agent:e2e-agent:smoke-proj`. |
| `E2E_TRANSPORT` | `websocket` | Agent transport. Set to `polling` to exercise the fallback path. |
| `E2E_TIMEOUT_SECS` | `180` | Overall wall-clock cap before the script aborts and cleans up. |
| `E2E_SKIP_RUN` | `0` | If `1`, exit after dependency checks without running binaries (used by `bash -n` style validation in CI). |
| `CARGO_BIN` | `cargo` | Cargo binary to invoke. |

## What the script does

1. Creates a temporary runtime data dir, agent config, agent project file, and a
   stub Codex CLI binary under a fresh `mktemp -d`.
2. Initializes a tiny git repo as the agent project so `getProjectGitStatus`
   has something real to report.
3. Starts the server: `cargo run --bin webcodex` with `WEBCODEX_TOKEN`,
   `WEBCODEX_DATA`, `WEBCODEX_ADDR`, and `CODEX_BIN` pointing at the stub.
4. Starts the agent: `cargo run --bin webcodex-agent -- --config <tmp>`
   with `transport = "websocket"`.
5. Waits for the agent to register by polling `POST /api/runtime/status` until
   `output.agents.count == 1`.
6. Drives the GPT Actions surface via `curl`:
   - `POST /api/runtime/status` (`getRuntimeStatus`)
   - `POST /api/projects/list` (`listProjects`) — asserts the runtime id
     `agent:<client_id>:<project_id>` is present.
   - `POST /api/projects/git_status` (`getProjectGitStatus`)
   - `POST /api/projects/git_diff` (`getProjectGitDiff`)
   - `POST /api/projects/read_file` (`readProjectFile`)
   - `POST /api/projects/run_shell` (`runProjectShellCommand`)
   - `POST /api/codex/run` (`runCodexTask`) — starts an async job on the agent
     using the stub `CODEX_BIN`, captures `job_id`.
   - `POST /api/jobs/status` (`getRuntimeJobStatus`) — polls `job_id` to a
     terminal status.
   - `POST /api/jobs/log` (`getRuntimeJobLog`) — asserts the stub output is
     present.
7. Drives the MCP surface via `curl POST /mcp`:
   - `initialize` — asserts a `protocolVersion` is returned.
   - `tools/list` — asserts at least one tool is returned.
   - `tools/call` `list_projects` — asserts `structuredContent.success == true`
     and that the agent project id appears in the output.
8. Pulls `GET /openapi.json` and validates it with `python3` (no `jq`
   dependency):
   - operationIds match the expected 12-id set exactly.
   - no `/api/audit/*`, `/api/jobs/stop`, or legacy `/api/codex/*` paths.
   - every path is POST-only.
   - descriptions do not claim server-side `projects.toml` is the runtime
     project source.
9. Cleans up both background processes (trap on `INT`/`TERM`/`EXIT`) and
   reports pass/fail counts.

## MCP smoke coverage

The MCP smoke is in the same script (`scripts/e2e_zero_config_ws.sh`), under the
"MCP surface" section. It covers the three required JSON-RPC methods:

| Method | Asserts |
|--------|---------|
| `initialize` | `result.protocolVersion` is non-empty. |
| `tools/list` | `result.tools` is a non-empty array. |
| `tools/call` (`list_projects`) | `result.structuredContent.success == true` and the agent-registered project id is in `structuredContent.output`. |

To run the MCP checks manually against a running server:

```bash
TOKEN=e2e-smoke-token
BASE=http://127.0.0.1:8080

curl -sS -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -X POST "$BASE/mcp" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'

curl -sS -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -X POST "$BASE/mcp" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'

curl -sS -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -X POST "$BASE/mcp" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_projects","arguments":{}}}'
```

## GPT Actions schema smoke coverage

The schema smoke pulls `GET /openapi.json` and validates with the Python
standard library (no `jq` required). It asserts:

- The operation-id set is exactly the 12 documented ids:
  `listRuntimeTools`, `listProjects`, `getRuntimeStatus`, `runCodexTask`,
  `getRuntimeJobStatus`, `getRuntimeJobLog`, `readProjectFile`,
  `getProjectGitStatus`, `getProjectGitDiff`, `applyProjectPatch`,
  `runProjectShellCommand`, `callRuntimeTool`.
- No forbidden paths appear: `/api/audit/*`, `/api/jobs/stop`, and any legacy
  `/api/codex/*` route.
- Every path is POST-only (the GPT Actions surface is POST-only; `/openapi.json`
  itself is a separate GET route and must not appear inside `paths`).
- Descriptions do not describe server-side `projects.toml` as the runtime
  project source (the server is zero-project-config; projects come from agent
  registration).

## Mapping to a real ChatGPT GPT Action import

The harness proves the server side. To validate with real ChatGPT:

1. Deploy the server somewhere reachable from ChatGPT (e.g. a public host or a
   tunnel). Set `WEBCODEX_TOKEN` and `WEBCODEX_PUBLIC_URL`:
   ```bash
   WEBCODEX_TOKEN="<your-secret>" \
   WEBCODEX_PUBLIC_URL="https://your-server.example" \
   cargo run --bin webcodex
   ```
2. Start the agent on the machine that owns the project, pointing at the public
   server URL with `transport = "websocket"`.
3. In ChatGPT: **Settings → Actions → Import from URL**, enter
   `https://your-server.example/openapi.json`.
4. Configure Action authentication as **API Key**, type **HTTP**, header
   `Authorization`, value `Bearer <WEBCODEX_TOKEN>`.
5. Drive the same call flow the harness exercises:
   - `getRuntimeStatus` → is the agent online?
   - `listProjects` → copy a project id like `agent:<client_id>:<project_id>`.
   - `runCodexTask` → capture `job_id`.
   - `getRuntimeJobStatus` / `getRuntimeJobLog` → poll and read output.

If the local harness passes but ChatGPT fails, the gap is almost certainly
network/auth/URL configuration, not runtime logic.

## Polling transport fallback

To exercise the polling fallback instead of WebSocket, run:

```bash
E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh
```

Both transports feed the same `ShellClientRegistry` and `ToolRuntime`, so the
expected pass/fail set is identical. This is useful to confirm the polling path
was not regressed by WebSocket changes.

## Constraints

- The script does **not** implement QUIC.
- It does **not** restore server-side `projects.toml` as a runtime project
  source; the agent registers the project.
- It does **not** invoke the real Codex CLI — a stub binary is used so the test
  is deterministic and hermetic.
- It is a validation tool, not production service logic.
