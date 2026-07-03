# Operations Guide

[English](OPERATIONS.md)

This guide covers day-to-day WebCodex operations: server initialization, client enrollment, pairing, project registration, token management, and smoke testing. For first deployment, see [QUICK_START.md](QUICK_START.md). For production hardening, Nginx, QUIC, and OAuth2 details, see [DEPLOYMENT.md](DEPLOYMENT.md).

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
    "bind_current": false
  }
}
```

Returns a `wc_sess_*` session id in `output.session.session_id`. Keep that id
and pass it explicitly to subsequent project tools.

### 2. Discover and inspect

```json
{"tool": "runtime_status", "params": {}}
{"tool": "list_projects", "params": {}}
{"tool": "read_file", "params": {"project": "agent:workstation:my-repo", "path": "src/auth.rs"}}
{"tool": "search_project_text", "params": {"project": "agent:workstation:my-repo", "pattern": "authenticate", "path": "src"}}
{"tool": "show_changes", "params": {"project": "agent:workstation:my-repo", "session_id": "wc_sess_example", "include_diff": false}}
```

When choosing a smoke target from `list_projects`, prefer
`capabilities.recommended_for_smoke=true`. For git smoke, also require
`capabilities.git_available=true`; a project such as `agent:special:test-mcp`
may be safe but not git-backed.

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
validation by default.

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

### 6. Finish or hand off

```json
{
  "tool": "finish_coding_task",
  "params": {
    "project": "agent:workstation:my-repo",
    "session_id": "wc_sess_example",
    "include_handoff": true,
    "include_validation_summary": true
  }
}
```

For a read-only handoff without finish aggregation:

```json
{
  "tool": "session_handoff_summary",
  "params": {
    "session_id": "wc_sess_example",
    "project": "agent:workstation:my-repo",
    "include_validation": true
  }
}
```

`finish_coding_task.validation` and `session_handoff_summary.validation` are
ledger-derived summaries. They do not expose raw stdout/stderr, excerpt fields,
or `validation_output_summary`; the parser extracts only stable facts from safe
bounded metadata and does not infer root causes or suggest fixes.

### Session id semantics

**REST / GPT Action:**

- Top-level `recording_session_id` = recorder metadata for the current generic wrapper call; it is stripped before concrete tool dispatch.
- Top-level `session_id` = ordinary flattened tool input when `params`/`arguments` are absent.
- `params.session_id` = business parameter used by `show_changes` or `session_summary` to select which session to summarize.
- The two may be the same or different.
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
{"tool": "runtime_status", "params": {}}
```

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
   Actions unless debugging schemas.
3. Run `runtime_status`.
4. Confirm `start_coding_task` and `finish_coding_task` are available through
   the generic runtime tool path.
5. Confirm `session_handoff_summary` exposes `validation` when
   `include_validation` defaults to true.
6. On a `list_projects` entry with `capabilities.recommended_for_smoke=true`,
   run `start_coding_task`, `read_file` or `search_project_text`,
   `show_changes`, and `finish_coding_task`.
7. Run local or staging E2E and eval checks:

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

The current recommended GPT Action operation count is 27, and it must remain at
or below 30.

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
