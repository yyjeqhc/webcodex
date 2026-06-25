# Audit API (read-only)

The Audit API is an **admin/debug-only** read-only surface for inspecting the
action-session audit trail that the runtime writes for every mounted action
endpoint (MCP `tools/call`, GPT Actions, and the `/api/codex/*` routes). It is
intentionally **not** part of the GPT Actions OpenAPI schema
(`/openapi.json`) — it is for operators, not for ChatGPT to call.

All endpoints are `POST`, protected by the same Bearer token (`DROP_TOKEN`) as
the rest of the API, and perform no write operations.

## Endpoints

### `POST /api/audit/sessions`

List recent action sessions, newest first.

Request body (all fields optional):

```json
{ "limit": 50, "status": "open" }
```

- `limit` — number of sessions to return. Default `50`, hard cap `200`.
- `status` — optional filter, one of `"open"` / `"closed"`. Omit for all.

Response:

```json
{
  "sessions": [
    {
      "session_id": "...",
      "title": null,
      "note": null,
      "status": "open",
      "created_at": 1719360000,
      "updated_at": 1719360010,
      "closed_at": null,
      "first_event_at": 1719360001,
      "last_event_at": 1719360010,
      "total_actions": 3,
      "success_count": 2,
      "failed_count": 1,
      "timeout_or_unknown_count": 0,
      "warning_count": 0,
      "total_duration_ms": 420,
      "changed_files_count": 2,
      "job_ids_count": 1
    }
  ]
}
```

Session records carry only metadata and aggregate counts — no prompts, outputs,
or secrets.

### `POST /api/audit/session`

Fetch one session summary plus its decoded events.

Request body:

```json
{ "session_id": "...", "events_limit": 50 }
```

- `session_id` — required.
- `events_limit` — number of events to return. Default `50`, hard cap `200`.

Response: `{ "session": <ActionSessionRecord>, "events": [<ActionEventView>] }`,
or `404` when the session id is unknown.

Each `ActionEventView` includes the endpoint, action name, status, duration,
changed files, and the decoded `ids` / `summary` payloads (passed through the
strict read-time sanitizer — see [Security](#security) below).

### `POST /api/audit/stats`

Return aggregate `ActionSessionStats` over a set of decoded events.

Request body (all fields optional):

```json
{ "session_id": "...", "limit": 20 }
```

- `session_id` — when supplied, stats cover that single session's events
  (internally capped at 500 events).
- When `session_id` is omitted, stats cover the events of the `limit` most
  recent sessions. `limit` defaults to `20` and is capped at `50`; each
  session contributes at most 200 events. This bounds the scan.

Response (`ActionSessionStats`):

```json
{
  "by_endpoint": { "/api/codex/edit": 2, "/api/codex/job": 1 },
  "by_project": { "demo": 3 },
  "by_status": { "success": 2, "failed": 1 },
  "edit_count": 2,
  "context_count": 0,
  "job_count": 1,
  "command_count": 0,
  "report_count": 0,
  "artifact_count": 0,
  "git_count": 0,
  "shell_count": 0,
  "changed_files_distinct_count": 2,
  "job_ids_distinct_count": 1
}
```

## Security

The audit trail is sanitized at **write time** (`action_sessions::sanitize_value`):
secret-like keys (`token`, `api_key`, `authorization`, `password`,
`private_key`, ...) are redacted, and raw `stdout` / `stderr` / `diff` /
`openapi_json` / `text` / `base64_content` fields are dropped; `command_text` /
`script_text` are replaced with a SHA-256 hash + a short redacted preview.

The Audit API applies an additional, **stricter read-time pass**
(`action_sessions::sanitize_value_for_read`) to every event payload it returns:
secret-like keys are **dropped entirely** (not even kept as `[redacted]`), so an
audit response never echoes sensitive field names or values.

Guarantees:

- No secret/token/api_key/password field name or value is ever returned.
- No full prompt, full stdout, full stderr, full diff, or file base64 content
  is ever returned.
- All `limit` parameters are bounded: default ≤ 50, hard cap ≤ 200.
- No write operations, no raw SQL, no arbitrary filter expressions.
- Not exposed in `/openapi.json` (enforced by test — see
  `LEGACY_FORBIDDEN_PATHS` in `src/openapi.rs`).

## What this is not

- Not a GPT Action. ChatGPT should not import or call these endpoints.
- Not a management API. There is no stop/delete/close/configure operation.
- Not a second tool-execution path. It only reads the audit trail that the
  shared `ToolRuntime` already writes.
