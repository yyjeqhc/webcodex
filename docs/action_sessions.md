# Action Sessions (Deprecated)

> **This document is deprecated.** It describes a dashboard
> (`/actions/sessions`), a query API (`POST /api/codex/action_sessions`), and
> audited endpoints that include `/api/desktop/task_op` and `/api/codex/check`.
> **None of these are mounted in the current runtime.** The Web UI dashboard
> has been removed.

## Current state

The action session storage code (`src/action_sessions.rs`,
`src/action_audit.rs`) remains in the tree as an internal metadata-only audit
trail for the legacy `/api/codex/*` routes that are still mounted. It is **not**
part of the GPT Actions or MCP surface. A read-only admin/debug query API is
now mounted at `/api/audit/*` — see [AUDIT_API.md](AUDIT_API.md).

What is still true and useful:

- The audit layer is metadata-only. It never stores raw secrets, bearer tokens,
  API keys, SSH private keys, `.env` contents, full stdout/stderr, full patch
  diffs, or full uploaded file contents.
- Sanitization redacts secret-like keys (`token`, `api_key`, `authorization`,
  `password`, `private_key`) and drops raw stdout/stderr/diff/base64 fields.

For supported runtime observability, use `runtime_status`
(`POST /api/runtime/status`, operation id `getRuntimeStatus`). See
[RUNTIME_STATUS.md](RUNTIME_STATUS.md). For read-only audit queries over the
action-session trail, see [AUDIT_API.md](AUDIT_API.md).
