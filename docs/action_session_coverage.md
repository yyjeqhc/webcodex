# Action Session Route Coverage (Deprecated)

> **This document is deprecated.** It audits routes that no longer exist:
> `/api/messages`, `/api/files`, `/api/channels`, `/api/desktop/*`,
> `/api/codex/command`, `/api/codex/command_request*`, `/api/codex/check`, and
> `/api/codex/action_sessions`. **None of these routes are mounted in the
> current runtime.** The coverage table below is not maintained.

## Current state

The action session / audit layer (`src/action_sessions.rs`,
`src/action_audit.rs`) still exists as an internal metadata-only audit trail for
the legacy `/api/codex/*` routes that remain mounted (`context`,
`context_batch`, `edit`, `artifact`, `git`, `job`, `report`). However:

- There is **no query endpoint** (`/api/codex/action_sessions` is not mounted).
- The GPT Actions surface (`/openapi.json`) and MCP (`/mcp`) do not depend on
  action sessions.
- `runtime_status` (`POST /api/runtime/status`) is the supported observability
  entry point and never exposes tokens, secrets, or stdout/stderr.

A future phase may add a read-only audit viewer / action session query
endpoint. Until then, treat action sessions as internal-only. See
[RUNTIME_STATUS.md](RUNTIME_STATUS.md) for supported observability.
