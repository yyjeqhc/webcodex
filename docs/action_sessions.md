## Action Sessions

Action Sessions add a lightweight audit layer on top of Private Drop's Codex and desktop actions.
They answer a simple question: during one GPT/Codex task, which actions ran, how many times, and what happened?

### What it does

- Records one event for each audited action call.
- Groups nearby calls into a rolling action session.
- Persists session and event metadata in SQLite.
- Exposes a small API for listing sessions, reading a session timeline, renaming, and closing.
- Adds a dashboard page for recent sessions and per-session detail.

### Session grouping

Private Drop does not require GPT to send a stable conversation id.
Instead it uses a rolling active session strategy:

1. If the request includes `X-Action-Session-Id`, that id is used.
2. Otherwise, the server looks for the most recent open session.
3. If the last event was within `ACTION_SESSION_IDLE_TIMEOUT_SECS` (currently 1800 seconds / 30 minutes), the event is appended there.
4. If not, the server creates a new open session.
5. A closed session is never auto-reused.

This lets ordinary GPT Actions use the feature without any schema changes, while still allowing explicit grouping later.

### What gets recorded

Each action event stores metadata such as:

- `event_id`
- `session_id`
- `started_at`, `ended_at`, `duration_ms`
- `endpoint`
- `action_name`
- `operation`
- `project`
- `status`
- `http_status`
- `error_summary`
- `warning_summary`
- `changed_files`
- selected ids such as `job_id`, `job_ids`, `report_id`, `message_id`
- request/response size estimates when available
- endpoint-specific summary fields

Each session keeps rolling counters such as:

- total actions
- success / failed / timeout-or-unknown counts
- warning count
- total duration
- changed-files count
- job-id count
- first / last event timestamps

The dashboard also computes grouped counts by endpoint, project, and status.

### What is intentionally not recorded

The audit layer is metadata-only.
It does **not** store:

- raw secrets
- bearer tokens
- API keys
- SSH private keys
- `.env` contents
- full stdout/stderr
- full patch diff
- full uploaded file contents
- raw base64 payloads
- raw `script_text` / `command_text`

### Sanitization rules

Request and response summaries are sanitized before persistence:

- secret-like keys such as `token`, `api_key`, `authorization`, `password`, `private_key` are redacted
- raw `stdout_tail`, `stderr_tail`, `stdout`, `stderr`, and `diff` fields are dropped
- `base64_content` is dropped
- `script_text` and `command_text` are replaced with:
  - command kind
  - line count
  - char count
  - sha256 hash
  - first-line preview, truncated and redacted
- long text is trimmed to a short summary
- error text is capped to 500 characters

### Current audited endpoints

Current coverage includes:

- `/api/codex/context_batch`
- `/api/codex/edit`
- `/api/codex/artifact`
- `/api/codex/git`
- `/api/codex/check`
- `/api/codex/report`
- `/api/desktop/task_op`

The schema, storage, and sanitizers also support action session queries through
`/api/codex/action_sessions`.

### API

Action session queries use:

- `POST /api/codex/action_sessions`

Supported ops:

- `list`
- `get`
- `events`
- `stats`
- `rename`
- `close`

Example body:

```json
{
  "op": "get",
  "session_id": "example-session-id",
  "limit": 200
}
```

### Frontend

Dashboard pages:

- `/actions/sessions`
- `/actions/sessions/{session_id}`

The list page shows recent sessions, status, action counts, top endpoints, and top projects.
The detail page shows summary cards, grouped counts, timeline entries, changed files, ids, and safe summaries.

### GPT usage

GPT does not need to pass a session id.
The server will auto-group calls into a rolling session.

If a workflow wants explicit grouping, it may send:

- header `X-Action-Session-Id: <id>`

Future request-body support can be layered on later if needed.

### Current gaps

- `job` and `command_request_op` still need fuller branch-by-branch end-to-end audit coverage.
- Panic paths are not specially captured; normal success, failure, and rejection paths are.

### Using the timeline for 504 recovery

When a task hits a timeout or uncertain result:

1. Open the latest action session.
2. Inspect the recent timeline.
3. Check whether `applyProjectEdit` or another mutating action already succeeded.
4. Use recorded `changed_files`, endpoint, project, and id metadata to decide whether to inspect files, `git_status`, or job recovery before retrying.

This is especially useful for the "write succeeded but response timed out" failure mode.
