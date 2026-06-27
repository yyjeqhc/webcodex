# Runtime Status (Observability)

`runtime_status` is a read-only observability tool that lets deployers and
ChatGPT quickly determine whether WebCodex Runtime is healthy, which
projects/agents/jobs exist, and whether the current configuration is correct.
It never exposes tokens, API keys, full env, or stdout/stderr.

## How to call it

### GPT Actions (dedicated endpoint)

```
POST /api/runtime/status
```

Operation id: `getRuntimeStatus`. Request body: `{}` (empty). Requires Bearer
auth when `WEBCODEX_TOKEN` is set.

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/runtime/status \
  -H "Content-Type: application/json" \
  -d '{}'
```

### MCP

`runtime_status` appears in `tools/list` and can be called via `tools/call`:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": { "name": "runtime_status", "arguments": {} }
}
```

### Generic tool call

```
POST /api/tools/call
```

```json
{ "tool": "runtime_status", "params": {} }
```

## Response shape

The output is a structured JSON object:

```json
{
  "service": "webcodex",
  "version": "0.1.0",
  "server_time": 1719337980,
  "pid": 12345,
  "auth_enabled": true,
  "configured_public_url": "https://webcodex.example.com",
  "projects": {
    "configured": true,
    "count": 2,
    "config_path": "./projects.toml",
    "load_error": null
  },
  "agents": {
    "count": 1,
    "online_count": 1,
    "stale_count": 0,
    "offline_count": 0,
    "clients": [
      {
        "client_id": "workstation-1",
        "display_name": "Workstation",
        "owner": "alice",
        "status": "online",
        "connected": true,
        "agent_protocol_version": "polling-v1",
        "transport": "polling",
        "last_seen": 1719337980,
        "capabilities": { "shell": true, "file_read": true, ... },
        "projects_count": 1
      }
    ]
  },
  "jobs": {
    "agent_known_count": 0,
    "local_known_count": 1,
    "active_count": 1
  },
  "tools": {
    "count": 13,
    "names": ["list_tools", "list_projects", "list_agents", "runtime_status", ...]
  }
}
```

### Field notes

- `auth_enabled`: whether `WEBCODEX_TOKEN` is set. `true` when auth is required.
- `configured_public_url`: the value of `WEBCODEX_PUBLIC_URL`, or `null` when unset.
  This lets a deployer immediately see that the public URL has not been
  configured.
- `projects.configured`: `false` when `projects.toml` failed to load or is
  absent; `load_error` carries the reason in that case.
- `agents.online_count` / `stale_count` / `offline_count`: `online` = live
  connection with a recent heartbeat (within the 60s online window);
  `stale_count` = registered agents whose `last_seen` is older than the online
  window (a WebSocket agent flipping `online` -> `stale` is the case the console
  highlights); `offline_count` mirrors `stale_count` for the registered set
  (truly offline agents are removed from the registry on disconnect).
- `agents.clients[].transport`: `"websocket"` (preferred) or `"polling"`
  (fallback). The console distinguishes the two at a glance.
- `agents.clients[].last_seen`: unix timestamp (seconds) of the most recent
  heartbeat/result for the agent. Used to render how stale an agent is.
- `jobs.active_count`: jobs in `running`, `queued`, `agent_queued`, or
  `stop_requested` status. Counted across both agent-backed and local jobs.
- `tools.names`: the full list of tool names exposed by `ToolRuntime`.

### What is NOT exposed

To avoid leaking sensitive information, `runtime_status` never returns:

- `WEBCODEX_TOKEN` or any token value
- API keys or secrets
- The full process environment
- Complete project path lists (use `list_projects` for that)
- stdout / stderr of jobs

## Recommended troubleshooting flow

1. **`getRuntimeStatus`** — is the runtime healthy? Are projects configured?
   Are agents online? Is `auth_enabled` correct?
2. **`listProjects`** — which project ids are available?
3. **`listRuntimeTools`** — which tools are exposed?
4. **`runCodexTask`** — start a Codex task in a project.
5. **`getRuntimeJobStatus` / `getRuntimeJobLog`** — poll the returned `job_id`.

## Architecture

`runtime_status` is a `ToolCall::RuntimeStatus` variant implemented in
`ToolRuntime` (see `src/tool_runtime.rs`). It is a unit tool (no arguments).
Both the REST/GPT Actions wrapper (`/api/runtime/status`) and the MCP
`tools/call` path dispatch through `ToolRuntime::dispatch_with_auth`, so the
business logic lives in exactly one place. The wrappers stay thin.
