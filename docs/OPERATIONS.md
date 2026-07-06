# Operations Guide

[English](OPERATIONS.md)

This guide covers day-to-day WebCodex operations: server initialization, client enrollment, pairing, project registration, token management, and smoke testing. For first deployment, see [QUICK_START.md](QUICK_START.md). For production hardening, Nginx, QUIC, and OAuth2 details, see [DEPLOYMENT.md](DEPLOYMENT.md).

For the proposed ops health, deployment smoke, and agent fleet observability
roadmap, see [OPS_OBSERVABILITY_ROADMAP.md](OPS_OBSERVABILITY_ROADMAP.md).

Operator-friendly read-only checks are available through:

```bash
webcodex-cli ops status --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
webcodex-cli ops agents --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
webcodex-cli ops projects --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
webcodex-cli ops smoke-preflight \
  --server-url "$SERVER_URL" \
  --token-file "$USER_TOKEN_FILE" \
  --project agent:workstation:my-repo
webcodex-cli ops smoke-preflight \
  --server-url "$SERVER_URL" \
  --token-file "$USER_TOKEN_FILE" \
  --project agent:workstation:my-repo \
  --strict
```

These commands accept `--server-url`/`--url`, `--env-file`, `--token-file`,
`--token`, `--json`, and `--strict`. They require a user token/PAT or another
bearer token with suitable runtime/project/job read scopes. Prefer
`--token-file` for operator use; `--token` is supported for constrained
one-off calls but is easier to expose through shell history or process lists.
They do not print token or env values.

`WARN` means the check found something worth reviewing, but it is not
necessarily a deploy blocker. By default, ops commands exit `0` when they can
generate a report, even when `Overall: FAIL`. Add `--strict` for deployment
gates: `PASS` and `WARN` exit `0`, while `FAIL` exits `2`.

`ops smoke-preflight` short-circuits when the target project is missing,
offline, disconnected, or not git-backed. In that case it reports the blocking
reason without sending `show_changes` or `workspace_hygiene_check` to a stale or
offline agent.

## Server initialization

### Environment file

`webcodex-cli server init` creates the server environment file containing the bootstrap admin token and runtime settings.

```bash
SERVER_URL="https://webcodex.example.com"
ENV_FILE="/etc/webcodex/webcodex.env"
DATA_DIR="/var/lib/webcodex"
BIN="/opt/webcodex/bin/webcodex"
CLI="/opt/webcodex/bin/webcodex-cli"

sudo "$CLI" server init \
  --listen 127.0.0.1:8080 \
  --data-dir "$DATA_DIR" \
  --env-file "$ENV_FILE" \
  --public-url "$SERVER_URL"
```

This writes:

- `WEBCODEX_TOKEN` — the bootstrap/admin token. Used only for initial setup, user creation, pairing, and emergency admin. Do not put it in GPT Actions, MCP, or agent config.
- `WEBCODEX_ADDR` — the server listen address.
- `WEBCODEX_DATA` — the data directory path.
- `WEBCODEX_PUBLIC_URL` — the public HTTPS URL.

The env file is server-side only. Do not copy it to client machines.

### Loading the env file

For one-off admin CLI commands, load the env file:

```bash
set -a
. "$ENV_FILE"
set +a
```

Or pass `--env-file "$ENV_FILE"` when the command supports it.

## Server startup

### systemd (recommended)

```bash
sudo "$CLI" server install-service \
  --env-file "$ENV_FILE" \
  --bin "$BIN"

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex

"$CLI" server status --env-file "$ENV_FILE"
```

Use `--overwrite` only when replacing an existing unit.

### Manual foreground / background

For testing or environments without systemd:

```bash
# Foreground
WEBCODEX_ENV_FILE="$ENV_FILE" "$BIN"

# Background
nohup env WEBCODEX_ENV_FILE="$ENV_FILE" "$BIN" > /var/log/webcodex.log 2>&1 &
```

Manual mode does not provide automatic restart, log rotation, or boot persistence. Use systemd for production.

## Client enrollment

### Profile-based config (recommended)

Each client or user profile gets its own directory under `/etc/webcodex/clients/`:

```text
/etc/webcodex/clients/<profile>/agent.toml
/etc/webcodex/clients/<profile>/projects.d/
/etc/webcodex/clients/<profile>/webcodex-user-token
/etc/webcodex/clients/<profile>/webcodex-agent-token
```

Enroll a client with a profile:

```bash
"$CLI" client enroll \
  --server-url "$SERVER_URL" \
  --pairing-code <wc_pair_...> \
  --client-id workstation \
  --display-name "Workstation" \
  --profile workstation \
  --allowed-root /root/git
```

Install a profile-specific agent service:

```bash
"$CLI" agent install-service \
  --profile workstation \
  --bin /opt/webcodex/bin/webcodex-agent \
  --overwrite

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent-workstation
```

### Legacy flat paths (not recommended)

Older setups may use flat paths directly under `/etc/webcodex/`:

```text
/etc/webcodex/agent.toml
/etc/webcodex/projects.d/
/etc/webcodex/webcodex-user-token
/etc/webcodex/webcodex-agent-token
```

This layout does not support multiple clients on the same machine. Migrate to profile-based config when possible.

## Pairing flow

Pairing creates a short-lived code on the server side that the client exchanges to enroll. This avoids copying long-lived credentials between machines.

### Server/admin side

```bash
"$CLI" pairing create \
  --server-url "$SERVER_URL" \
  --env-file "$ENV_FILE" \
  --username alice \
  --client-id workstation \
  --display-name "Alice Workstation" \
  --ttl-secs 600
```

This returns a `wc_pair_*` code. Send only this code to the client user.

### Client side

```bash
"$CLI" client enroll \
  --server-url "$SERVER_URL" \
  --pairing-code <wc_pair_...> \
  --client-id workstation \
  --display-name "Alice Workstation" \
  --profile alice \
  --allowed-root /home/alice/git
```

### What not to copy

- Do not copy `WEBCODEX_TOKEN` to client machines.
- Do not copy `wc_agent_*` tokens between machines.
- Do not copy `wc_pat_*` tokens between machines.
- Do not put the bootstrap token in agent config or GPT Action config.
- Each client should generate its own tokens through `client enroll` or `token create-local`.

## Project registration

### register_project

`register_project` is an agent-level runtime tool. It registers an existing directory as a project on a connected agent.

```json
{
  "tool": "register_project",
  "params": {
    "client_id": "workstation",
    "id": "my-repo",
    "name": "My Repo",
    "path": "/root/git/my-repo",
    "allow_patch": true,
    "overwrite": true
  }
}
```

Key behaviors:

- Does not require the project to already exist in the agent's `projects.d/`.
- Finds the online agent by `client_id`.
- The agent validates that `path` exists and is within its `allowed_roots`.
- The agent writes `projects.d/<id>.toml` on its own machine.
- The resulting runtime project id is `agent:<client_id>:<project_id>` (e.g., `agent:workstation:my-repo`).

### create_project

`create_project` creates a new directory and registers it. It is subject to the agent's `allowed_roots` policy.

```json
{
  "tool": "create_project",
  "params": {
    "client_id": "workstation",
    "id": "tmp-smoke",
    "name": "Temporary Smoke Project",
    "path": "/root/git/tmp-smoke",
    "git_init": true,
    "allow_patch": true
  }
}
```

If `allowed_roots` is `/root/git`, then paths outside that root (e.g., `/tmp/...`) are rejected by default. For temporary or test projects, place them under the allowed root:

```text
/root/git/tmp-smoke-project
```

## Token model

### WEBCODEX_TOKEN

- Server bootstrap/admin token.
- Created by `server init`.
- Lives only in the server env file (`/etc/webcodex/webcodex.env`).
- Used for: initial setup, creating users, issuing account credentials, pairing, emergency admin.
- Do not use for: GPT Actions, MCP, agent connections, daily runtime calls.

### wc_pat_* (Personal API Token)

- Belongs to a user (owner).
- Generated locally by `webcodex-cli token create-local`; the server stores only the hash.
- Not bound to a single device — the same PAT works from any client.
- Used for: GPT Actions, MCP, REST API, `callRuntimeTool`, `tools/list`, `tools/call`.
- A single PAT can access multiple agents and projects under the same owner on the same server, provided the scopes are sufficient.
- Do not use for: agent WebSocket/QUIC connections.

### wc_agent_* (Agent Token)

- Belongs to an agent instance.
- Generated locally by `webcodex-cli agent-token create-local`; the server stores only the hash.
- Bound to a specific `client_id`.
- Used for: `webcodex-agent` WebSocket/QUIC connections only.
- Do not use for: GPT Actions, MCP, REST API calls.

### wc_acct_* (Account Credential)

- One-time credential issued by `webcodex-cli users create --issue-credential`.
- Used to locally create `wc_pat_*` and `wc_agent_*` tokens.
- Do not use for: GPT Actions, MCP, agent connections, or any ongoing auth.

### wc_oat_* (OAuth2 Access Token)

- Delegated token issued via the OAuth2 authorization code flow.
- Used for: GPT Actions and MCP when OAuth2 is enabled.
- Requires `WEBCODEX_OAUTH2_ENABLED=true` on the server.

## Owner / client / project model

### Ownership

- Each agent has an `owner` (the user who created or enrolled it).
- Each PAT has an `owner` (the user who generated it).
- A PAT can only access agents and projects owned by the same user.
- Owner mismatch results in access denial.

### Client ID

The `client_id` is a stable identifier for an agent instance, typically named after the machine or role:

```text
workstation
laptop
server-a
container-dev
```

### Project ID format

Runtime project ids follow the pattern:

```text
agent:<client_id>:<project_id>
```

Examples:

```text
agent:workstation:my-repo
agent:laptop:my-repo
agent:server-a:service-api
agent:container-dev:tmp-smoke
```

The `client_id` portion identifies which agent hosts the project. The `project_id` portion is the local registry id from the agent's `projects.d/*.toml` file.

## GPT Action / MCP configuration

### Per-server, not per-device

Create GPT Actions and MCP connectors per server, not per device. If a server hosts multiple agents owned by the same user, a single PAT can access all of them.

Example GPT/MCP app names:

```text
WebCodex Production
WebCodex Staging
WebCodex Lab
```

### Token for GPT Actions

Use a `wc_pat_*` personal API token. Generate one with:

```bash
"$CLI" token create-local \
  --server "$SERVER_URL" \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --name gpt-action \
  --scopes runtime:read,project:read,project:write,job:run
```

Do not use:

- `WEBCODEX_TOKEN` — admin-only.
- `wc_agent_*` — agent-only.
- `wc_acct_*` — one-time enrollment only.

### Recommended scopes

| Scope | Purpose |
| --- | --- |
| `runtime:read` | Read runtime status, list tools, list agents. |
| `project:read` | Read files, search, git status/diff, show_changes. |
| `project:write` | Write files, apply patches, structured edits. |
| `job:run` | Run shell commands, Cargo helpers, Codex tasks. |
| `account:manage` | Optional: manage OAuth clients and tokens. |

### MCP with OAuth2

When OAuth2 is enabled (`WEBCODEX_OAUTH2_ENABLED=true`), MCP clients can use the authorization code flow instead of a static PAT:

- No PAT needed in the client config.
- The client redirects to `https://your-domain.example/oauth/authorize`.
- After consent, a `wc_oat_*` access token is issued.
- Scopes are delegated from the authorizing user.

### MCP with Bearer token

For static-token MCP clients:

- Use a `wc_pat_*` in the `Authorization: Bearer` header.
- Do not use `wc_agent_*` or `WEBCODEX_TOKEN`.

## Coding And Session Workflow

For coding tasks, prefer the deterministic coding-task aggregate tools. They
create and close out a session while keeping all continuity explicit.

### 1. Start a coding task

```json
{
  "tool": "start_coding_task",
  "params": {
    "project": "agent:workstation:my-repo",
    "title": "fix authentication bug",
    "include_tool_manifest": true,
    "bind_current": false
  }
}
```

Returns a `wc_sess_*` session id in `output.session.session_id`. Keep that id
and pass it explicitly to subsequent project tools. By default,
`start_coding_task` also returns compact `output.tool_manifest` without full
input/output schemas; set `include_tool_manifest=false` to omit it.
For bounded startup context, keep `include_tool_manifest=true` but pass
`tool_manifest_categories` such as `["workflow","session","git","edit",
"artifact","cleanup"]` and optionally `tool_manifest_limit`; the runtime clamps
the limit to 1..100 and reports whether the compact manifest was truncated. A
limit-driven `truncated=true` is expected bounded output, not `ResponseTooLarge`;
acceptance scripts should inspect the explicit limit and returned/total counts,
plus `truncation_reason="limit"`, `limit_applied=true`, `requested_limit`,
`returned_count`, and `total_count`.

For lightweight MCP direct or GPT Action sanity, call startup with
`include_runtime_status=true`, `compact_startup=true`,
`include_tool_manifest=true`, and a small `tool_manifest_limit`. Compact startup
returns build version/commit/dirty state, `tools.count`, `jobs.active_count`,
`agents.summary`, and effective/agent/server project status without `tools.names`,
full agent policy, `allowed_roots`, shell profile internals, command text,
stdout/stderr, env values, tokens, secrets, or full config values. Full
`include_runtime_status=true` without `compact_startup` remains available for
deeper troubleshooting and can include non-secret observability details such as
the public URL, tool names, agent policy summary, and allowed roots.
Read `output.startup_verdict.status` first. If it is `warn` or `fail`, inspect
`startup_verdict.checks` and `startup_verdict.suggested_next_actions`; detailed
startup fields remain the audit source.

Standalone `runtime_status` also accepts `summary_only=true` or `compact=true`
for the same compact health shape. Use that for first-contact deployed sanity;
reserve full no-arg `runtime_status` for deeper troubleshooting.

Startup sanity verdict rules:

- PASS: compact runtime status is present, `tools.count` is nonzero,
  `jobs.active_count=0`, an agent is online when the task depends on an agent
  project, and requested git/workspace status is clean.
- WARN: runtime status or git/workspace was not requested, validation has not
  run yet, or `tool_manifest.truncated=true` with
  `truncation_reason="limit"`.
- FAIL: runtime status failed, blocking jobs are active, requested git/workspace
  status is dirty, or tool manifest generation failed.

The response also includes `output.permissions`. The current self-hosted
development profile is `policy=dev_auto_approve`, `auto_approve=true`, and
`human_approval_required=false`; future release profiles should prefer
`require_approval`.

### 2. Discover and inspect

```json
{"tool": "runtime_status", "params": {"summary_only": true}}
{"tool": "list_projects", "params": {}}
{"tool": "read_file", "params": {"project": "agent:workstation:my-repo", "path": "src/auth.rs"}}
{"tool": "search_project_text", "params": {"project": "agent:workstation:my-repo", "pattern": "authenticate", "path": "src"}}
{"tool": "show_changes", "params": {"project": "agent:workstation:my-repo", "session_id": "wc_sess_example", "include_diff": false}}
```

When choosing a smoke target from `list_projects`, prefer
entries in `projects` whose `capabilities.recommended_for_smoke=true`. The
output shape is `{count, projects, recommended_for_smoke}`. For git smoke, also
require `capabilities.git_available=true`; a project such as
`agent:special:test-mcp` may be safe but not git-backed.

### 3. Edit with structured tools

Prefer structured line edits when line numbers are known:

```json
{"tool": "replace_line_range", "params": {"project": "agent:workstation:my-repo", "path": "src/auth.rs", "start_line": 42, "end_line": 45, "new_text": "new content"}}
{"tool": "insert_at_line", "params": {"project": "agent:workstation:my-repo", "path": "src/auth.rs", "line": 50, "text": "inserted line"}}
{"tool": "delete_line_range", "params": {"project": "agent:workstation:my-repo", "path": "src/auth.rs", "start_line": 60, "end_line": 65}}
```

Use `apply_text_edits` for coordinated exact edits in one UTF-8 file. Use
`apply_patch_checked` for broader multi-file diffs after `validate_patch`. Use
whole-file writes only for new files or deliberate small overwrites.

### 4. Validate

```json
{"tool": "cargo_fmt", "params": {"project": "agent:workstation:my-repo"}}
{"tool": "cargo_check", "params": {"project": "agent:workstation:my-repo"}}
{"tool": "cargo_test", "params": {"project": "agent:workstation:my-repo"}}
{"tool": "validate_patch", "params": {"project": "agent:workstation:my-repo", "patch": "diff --git ..."}}
```

Use `run_shell` only when structured Cargo helpers, `validate_patch`, and
`apply_patch_checked` are insufficient. `run_shell` is not classified as
validation by default. Use `run_job` for bounded async diagnostics/build/test
work, then supervise it with `job_status`, `job_tail`, or `list_jobs`. To stop a
WebCodex-started job, call `stop_job` through `callRuntimeTool`/MCP with the
same `project`, the returned `job_id`, the explicit `session_id` when available,
and `confirm=true`. `stop_job` enforces job project/session ownership and never
returns stdout/stderr. It keeps the compatibility `stopped` field, but models
should prefer `stop_effect`, `terminal`, `terminal_pending`, and `final_status`.

### 5. Review and summarize

```json
{
  "tool": "show_changes",
  "recording_session_id": "wc_sess_example",
  "params": {
    "project": "agent:workstation:my-repo",
    "session_id": "wc_sess_example",
    "include_diff": false,
    "session_event_limit": 30
  }
}
```

```json
{
  "tool": "workspace_hygiene_check",
  "params": {
    "project": "agent:workstation:my-repo",
    "session_id": "wc_sess_example"
  }
}
```

Review order for coding closeout is deterministic: call `show_changes`, inspect
`clean`, `warnings`, `hunks_truncated`, and `suggested_next_actions`; then call
`workspace_hygiene_check`, inspect `clean`, `findings`, `warnings`, and
`suggested_next_actions`; then use `session_handoff_summary` or
`finish_coding_task` with `summary_only=true` for the compact aggregate verdict.
`show_changes` and `workspace_hygiene_check` expose top-level `verdict`
summaries; read them first, but keep the detailed fields as the auditable basis.
For final closeout reporting, use `finish_coding_task.finish_verdict` or
`finish_coding_task.verdict`, not nested `show_changes.verdict` or
`workspace_hygiene_check.verdict`.

Discovery taxonomy is intentional: `start_coding_task` and
`finish_coding_task` are `workflow` category tools for the coding lifecycle.
`start_session`, `bind_current_session`, `session_summary`, and
`session_handoff_summary` are `session` category tools for raw ledger and
session-control workflows. Use `category=workflow` for lifecycle discovery and
`category=session` for session ledger/control discovery.

### 6. Finish or hand off

```json
{
  "tool": "finish_coding_task",
  "params": {
    "project": "agent:workstation:my-repo",
    "session_id": "wc_sess_example",
    "include_handoff": true,
    "include_workspace": true,
    "include_hygiene": true,
    "include_validation_summary": true,
    "include_diff": false,
    "summary_only": true
  }
}
```

`finish_coding_task` and `session_handoff_summary` should be used with
`summary_only=true` for compact handoff and closeout checks. For handoff, also
pass `include_workspace=true` and `include_validation=true`. For finish, pass
`include_workspace=true`, `include_hygiene=true`,
`include_validation_summary=true`, `include_diff=false`, and keep
`include_handoff=true` when a handoff aggregate is useful.
`finish_coding_task.include_workspace` is a compatibility flag matching
`session_handoff_summary.include_workspace`: it controls the nested handoff
workspace block when `include_handoff=true`; the top-level finish
workspace/show_changes check keeps its existing default behavior.
For `finish_coding_task(summary_only=true)`, `output.finish_verdict` is an
alias of `output.verdict`; read `status`, `blocking`, and `blocking_reasons`
from either final closeout verdict, not from nested review tool verdicts. The
compact detail fields remain the final auditable basis. The verdict is an
additive UX summary and does not change authorization, permissions, guards,
session binding, expected-failure classification, MCP direct errors, or job
lifecycle behavior. Top-level `suggested_next_actions` mirrors the final
closeout actions from the verdict.

For `summary_only=true` final outputs, sanity checks should reject stdout/stderr
bodies, command text, tails, and excerpts. Raw lower-level diagnostic/status
payloads may contain empty string fields such as `stderr: ""`; treat non-empty
stdout/stderr bodies as sensitive/high-noise unless explicitly requested, and
never allow env values, tokens, or secrets to appear.

`finish_coding_task` and `session_handoff_summary` include a bounded `jobs`
section. `active_count` remains a compatibility broad active count. New fields
split it into `blocking_active_count` and `nonblocking_active_count`, with
`running_count`, `stop_requested_count`, and `terminal_pending_count` for model
closeout decisions. `queued`, `running`, `started`, and `agent_queued` are
blocking active states and produce `active_jobs_present`. `stop_requested` is
nonblocking terminal-pending state and produces `jobs_terminal_pending` with
`blocking=false`; it should not prompt "stop active jobs before proceeding" by
itself. The jobs summary includes only metadata such as `job_id`, `kind`,
`status`, `project`, and timestamps; it does not include raw stdout/stderr,
tails, excerpts, or command text.

Compact handoff/finish verdict rules:

- PASS: `workspace_clean=true`, `jobs.blocking_active_count=0`,
  `tool_failures.unexpected_count=0`,
  `tool_failures.expectation_mismatch_count=0`,
  `tool_failures.unexpected_success_count=0`, and `hygiene_clean=true`.
- WARN: `validation.status=not_run` with or without ledger-derived
  `review_evidence`, resolved historical validation failures are present
  (`validation.status=mixed`, `latest_status=passed`,
  `historical_failures.resolved=true`, and
  `historical_failures.unresolved=false`), resolved validation-like historical
  tool failures from `cargo_fmt`, `cargo_check`, or `cargo_test` were followed
  by passed structured validation while workspace and hygiene checks are clean,
  matched expected failures are present (`expected_count>0` while
  unexpected/mismatch/unexpected-success counts are all zero),
  non-git/git-unavailable review context, terminal-pending
  nonblocking jobs, or bounded startup/manifest/review output was truncated only
  because of an explicit limit.
- FAIL: workspace is dirty, blocking jobs are active, unexpected tool failures
  exist, expected-failure mismatches exist, expected-failure calls unexpectedly
  succeeded, hygiene failed, validation failed, or mixed validation still has an
  unresolved/latest failure.

Unresolved validation failures and non-validation tool failures remain
blocking. Callers should still inspect `validation.historical_failures` and
`finish_verdict.warning_reasons` to distinguish resolved validation feedback
from a clean first-pass run.

For a read-only handoff without finish aggregation:

```json
{
  "tool": "session_handoff_summary",
  "params": {
    "session_id": "wc_sess_example",
    "project": "agent:workstation:my-repo",
    "include_validation": true,
    "summary_only": true
  }
}
```

Smoke and acceptance tests can mark intentional negative paths with runtime
testing metadata:

```json
{
  "tool": "stop_job",
  "params": {
    "project": "agent:workstation:my-repo",
    "session_id": "wc_sess_example",
    "job_id": "missing-job",
    "confirm": false,
    "expected_failure": true,
    "expected_failure_kind": "confirmation_required",
    "assertion_name": "stop_job requires confirm=true"
  }
}
```

`expected_failure`, `expected_failure_kind`, `test_expect_failure_kind`, and
`assertion_name` are ledger metadata only. They do not change authorization,
permission decisions, hard guards, execution, `command_started`, or the
immediate success/error result. `finish_coding_task` and
`session_handoff_summary` classify matching expected failures separately from
unexpected failures. They surface `expectation_mismatch_count` when the actual
`failure_kind` / `error_kind` differs, and `unexpected_success_count` when a
call marked `expected_failure=true` succeeds. Only unexpected failures,
mismatches, or unexpected successes should trigger "review failed tool calls"
style next actions; matched expected failures may produce an informational
`expected failure assertions matched` action.

In GPT Actions, that same expected negative path may still show an outer
`tool_error` because `/api/tools/call` returns HTTP 400 for a concrete runtime
`ToolResult.success=false`. Do not treat the outer GPT Action label alone as a
transport failure. Judge intentional negative-path smoke from the immediate
`failure_kind` / `error_kind` and from
`session_handoff_summary(summary_only=true).tool_failures` or
`finish_coding_task(summary_only=true).tool_failures`. The classifier separates
`expected_count`, `unexpected_count`, `expectation_mismatch_count`, and
`unexpected_success_count`; expected failures must not bypass auth, permission,
guards, `session_project_mismatch`, confirmation requirements, schema checks,
invalid JSON handling, or unknown-tool failure semantics.

`finish_coding_task.validation` and `session_handoff_summary.validation` are
ledger-derived summaries. They do not expose raw stdout/stderr, excerpt fields,
or `validation_output_summary`; the parser extracts only stable facts from safe
bounded metadata and does not infer root causes or suggest fixes. Summaries
include `status` and `reason`: `events_total=0` yields `status=not_run` and
`reason=no_validation_tool_invoked`; all-success, all-failure, and mixed ledgers
yield `passed`, `failed`, and `mixed`. `validation.status=mixed` remains strict
ledger history. Summary outputs also include `latest_status` and
`historical_failures`; a mixed ledger with a later successful validation and no
unresolved historical failure may warn instead of fail. `finish_coding_task`
may also downgrade resolved historical `cargo_fmt`, `cargo_check`, or
`cargo_test` tool failures when later structured validation passed and the
workspace/hygiene checks are clean. Non-validation tool failures and unresolved
validation failures remain blocking. `not_run` means no structured validation
tool was invoked, so docs-only or read-only work should interpret it with task
context.

`finish_coding_task.review_evidence` and
`session_handoff_summary.review_evidence` are separate ledger-derived,
non-cargo review summaries. They count successful read/search/diff/workspace/
hygiene inspection tools such as `read_file`, `search_project_text`,
`show_changes`, `git_diff_hunks`, and `workspace_hygiene_check`.
`finish_coding_task.review_evidence` may include the closeout review calls that
`finish_coding_task` performs itself. Compact review evidence also includes a
bounded `tools` list for explainability. It never includes file contents,
stdout/stderr, diff hunks, command text, tokens, secrets, or raw input payloads.
For docs-only or read-only audit tasks, `validation.status=not_run` can coexist
with `review_evidence.total>0`; compact verdicts remain `warn` and use
`validation_not_run_with_review_evidence` instead of treating the task as passed.
Review evidence is not a replacement for structured validation.

`finish_coding_task.permissions` and `session_handoff_summary.permissions`
summarize high-risk permission decisions from the session ledger. A high-risk
tool is one that is not read-only, is destructive, or is shell/job-like according
to runtime metadata. Under `dev_auto_approve`, those tools record
`status=auto_approved` after hard safety checks pass. Auto approval does not
bypass auth, OAuth scopes, read-only sessions, explicit deny guards,
cross-project session mismatch denial, path safety, sensitive path denial, or
agent policy. The permission summaries are bounded metadata only and must not
contain stdout/stderr, command bodies, patches, file contents, env, tokens,
secrets, or excerpts. `approved_count` remains a compatibility manual approval
count; use `manual_approved_count`, `auto_approved_count`, and
`total_approved_count` for clear totals.

### Session id semantics

**REST / GPT Action:**

- Top-level `recording_session_id` = recorder metadata for the current generic wrapper call; it is stripped before concrete tool dispatch.
- Top-level `session_id` = ordinary flattened tool input when `params`/`arguments` are absent.
- `params.session_id` = business parameter used by `show_changes` or `session_summary` to select which session to summarize.
- The two may be the same or different.
- `tool_manifest` is the recommended way to discover accepted flattened args.
  It returns `accepted_flattened_args` and `deprecated_or_unsupported_args` per
  tool without full schemas.
- `start_session` creates a session record but does not automatically bind
  future calls.
- `session_handoff_summary` requires explicit `session_id`; it never implicitly
  uses current-session binding.

**MCP:**

- `_session_id` in arguments = reserved recorder metadata. Stripped before tool dispatch.
- `session_id` in arguments = business parameter for `show_changes` or `session_summary`.
- Current-session bindings are process-local in-memory convenience state, not
  the durable session ledger.

## Smoke Test (read-only)

Use this sequence to verify a deployment without modifying any project.

Assumes a registered project `agent:workstation:my-repo`.

```json
{"tool": "runtime_status", "params": {"summary_only": true}}
```

Confirm service/build, `tools.count`, `jobs.active_count`, agent summary, and
project effective status. Use full no-arg `runtime_status` only when you need
deeper details such as `output.permissions.policy`; for development builds this
is normally `dev_auto_approve`, and release deployments should plan to use
`require_approval`.

```json
{"tool": "list_agents", "params": {}}
```

```json
{"tool": "list_projects", "params": {}}
```

```json
{"tool": "start_session", "params": {"project": "agent:workstation:my-repo", "title": "smoke test"}}
```

```json
{"tool": "read_file", "params": {"project": "agent:workstation:my-repo", "path": "README.md", "start_line": 1, "limit": 10}}
```

```json
{"tool": "show_changes", "params": {"project": "agent:workstation:my-repo", "session_id": "wc_sess_example", "include_diff": false}}
```

```json
{"tool": "session_summary", "params": {"session_id": "wc_sess_example"}}
```

## Post-Deployment Acceptance Smoke

After deploying a new server, agent, or runtime build:

1. Refresh the GPT Action or MCP schema if runtime tool schemas changed.
2. Run `tool_manifest` or focused `list_tools` with `summary_only=true` plus
   `category`, `features`, or `limit`; avoid full `listRuntimeTools` in GPT
   Actions unless debugging schemas. If `truncated=true` is caused by the
   caller-supplied limit, `truncation_reason="limit"` confirms it is a bounded
   response rather than `ResponseTooLarge`.
3. Run `runtime_status(summary_only=true)` or `runtime_status(compact=true)`;
   prefer `projects.effective.status/count` over legacy `projects.count` when
   `projects.toml` is not configured but agent projects are registered. For
   workflow sanity, also use
   `start_coding_task(include_runtime_status=true, compact_startup=true)` and
   inspect `startup_verdict.status`; reserve full runtime status for deeper
   troubleshooting.
4. Confirm `start_coding_task` and `finish_coding_task` are available through
   the generic runtime tool path.
5. Confirm `session_handoff_summary` exposes `validation` when
   `include_validation` defaults to true.
6. On a `list_projects.projects[]` entry with
   `capabilities.recommended_for_smoke=true`, run `start_coding_task`,
   `read_file` or `search_project_text`,
   `show_changes`, and `finish_coding_task`.
7. Run local or staging E2E and eval checks:

Preferred deployed generic sanity sequence:

1. `runtime_status(summary_only=true)` or `runtime_status(compact=true)`.
2. `tool_manifest`.
3. `tool_manifest(category=runtime)`, `tool_manifest(category=session)`, and
   `tool_manifest(category=git)` for focused discovery.
4. `show_changes(include_diff=false)` on the selected smoke project.
5. `workspace_hygiene_check` on the same smoke project.
6. `finish_coding_task(summary_only=true, include_workspace=true,
   include_hygiene=true, include_handoff=true,
   include_validation_summary=true, include_diff=false)` with the explicit
   `session_id`.

```bash
bash scripts/e2e_zero_config_ws.sh
E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh
EVAL_MODE=compare bash scripts/eval_coding_loop.sh
```

## Binary Deployment and Rollback Checklist

Use this short runbook for a conservative binary deployment. Adjust service
names and install paths to match the host, and keep token values in the
operator's shell or secret manager rather than in commands, logs, or docs.

1. Build the release binaries:

```bash
cargo build --release --bins
```

2. Back up the current install directory:

```bash
backup_dir="/opt/webcodex/bin.backups/$(date -u +%Y%m%dT%H%M%SZ)"
sudo install -d -m 0755 "$backup_dir"
sudo cp -a /opt/webcodex/bin/. "$backup_dir/"
```

3. Install the new binaries:

```bash
sudo install -m 0755 target/release/webcodex /opt/webcodex/bin/webcodex
sudo install -m 0755 target/release/webcodex-agent /opt/webcodex/bin/webcodex-agent
sudo install -m 0755 target/release/webcodex-cli /opt/webcodex/bin/webcodex-cli
```

4. Restart services on the appropriate hosts:

```bash
sudo systemctl restart webcodex
sudo systemctl restart webcodex-agent
```

5. Verify the public schema and operation budget:

```bash
curl -fsS https://webcodex.example.com/openapi.json > /tmp/webcodex-openapi.json
python3 - /tmp/webcodex-openapi.json <<'PY'
import json
import sys

schema = json.load(open(sys.argv[1], encoding="utf-8"))
ops = [
    op.get("operationId")
    for methods in schema.get("paths", {}).values()
    for op in methods.values()
    if isinstance(op, dict)
]
print(f"operation_count={len(ops)}")
if len(ops) > 30:
    raise SystemExit("operation_count exceeds GPT Actions limit")
PY
```

The current recommended GPT Action operation count is 25, and it must remain at
or below 30. Runtime/MCP tools such as `stop_job` remain available through the
generic `callRuntimeTool` surface and do not add dedicated GPT Action operations.

6. Run deployment smoke checks:

```bash
WEBCODEX_PUBLIC_URL="https://webcodex.example.com" \
WEBCODEX_TOKEN="<wc_pat_or_allowed_shared_key>" \
bash scripts/smoke_deployment.sh

WEBCODEX_SMOKE_RUN=1 \
WEBCODEX_PUBLIC_URL="https://webcodex.example.com" \
WEBCODEX_TOKEN="<wc_pat_or_allowed_shared_key>" \
bash scripts/smoke_artifact_transfer.sh
```

For GPT Actions, re-import the schema from `/openapi.json` when needed, then run
a read-only discovery/status smoke before mutation. For MCP, reconnect the
client and run `initialize`, `tools/list`, and a read-only `tools/call` such as
`runtime_status` or `list_projects`.

GPT Actions and MCP should use a managed `wc_pat_*` token or a
deployment-allowed shared key. `wc_agent_*` is only for `webcodex-agent`; do not
copy it into GPT Actions or MCP configuration.

7. Check service logs:

```bash
journalctl -u webcodex --since "15 minutes ago"
journalctl -u webcodex-agent --since "15 minutes ago"
```

8. Roll back from the backup if smoke or logs show a deployment regression:

```bash
sudo cp -a "$backup_dir"/. /opt/webcodex/bin/
sudo systemctl restart webcodex
sudo systemctl restart webcodex-agent
```

Do not use production mutation as smoke coverage. Any write-path smoke must stay
inside a disposable test project or temporary project under an allowed root.
Use artifact paths such as `artifacts/smoke/<name>.artifact` or
`artifacts/smoke/<name>.txt`. For abort cleanup verification, prefer
`artifact_upload_abort.final_file_exists` or
`read_project_artifact_metadata` with `allow_missing=true`; do not use an
expected read failure to prove absence. In session summaries,
`policy_rejected` means policy blocked the request before a write.

### register_project example

```json
{
  "tool": "register_project",
  "params": {
    "client_id": "workstation",
    "id": "my-repo",
    "name": "My Repo",
    "path": "/root/git/my-repo",
    "allow_patch": true,
    "overwrite": true
  }
}
```

### create_project example

```json
{
  "tool": "create_project",
  "params": {
    "client_id": "workstation",
    "id": "tmp-smoke",
    "name": "Temporary Smoke Project",
    "path": "/root/git/tmp-smoke",
    "git_init": true,
    "allow_patch": true
  }
}
```

## Related docs

- [DEPLOYMENT.md](DEPLOYMENT.md) — production hardening, Nginx, QUIC, OAuth2.
- [QUICK_START.md](QUICK_START.md) — first deployment walkthrough.
- [AUTH_MODEL.md](AUTH_MODEL.md) — credential model summary.
- [AUTH_ARCHITECTURE.md](AUTH_ARCHITECTURE.md) — internal auth pipeline.
- [GPT_ACTIONS.md](GPT_ACTIONS.md) — GPT Action setup and tool surface.
- [MCP.md](MCP.md) — MCP endpoint, client config, and troubleshooting.
- [AGENT_PROJECTS.md](AGENT_PROJECTS.md) — agent project registry format.
