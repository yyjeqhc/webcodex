# Phase 5A — `webcodex-cli`, policy summary, HOME default

Phase 5A adds a standalone management/setup binary and a few observability /
default-policy improvements. It is intentionally additive: the existing
`webcodex` server and `webcodex-agent` binaries keep working unchanged.

## New binary: `webcodex-cli`

`src/bin/webcodex-cli.rs` is a standalone binary for management and setup. It
does **not** start a server. It reuses existing shared modules instead of
duplicating logic:

- `admin_cli` (inlined via `#[path]`) — users / tokens / agent-tokens commands.
- `agent_init` (new shared module `src/agent_init.rs`) — `agent init` config
  generation + 0600 file writing. `webcodex-agent` and `webcodex-cli` both use
  it, so the large generation/writing code is not duplicated.

### Commands

```
webcodex-cli --help
webcodex-cli --version

# Users
webcodex-cli users create   --server-url URL --token T --username USER [--display-name N] [--role ROLE]
webcodex-cli users list     --server-url URL --token T

# Personal API tokens (GPT Actions)
webcodex-cli tokens create  --server-url URL --token T --username USER [--name N] [--scope SCOPE...]
webcodex-cli tokens list    --server-url URL --token T --username USER
webcodex-cli tokens revoke  --server-url URL --token T --username USER --token-id ID

# Agent tokens
webcodex-cli agent-tokens create --server-url URL --token T --username USER --client-id ID [--name N] [--scope SCOPE...]
webcodex-cli agent-tokens list   --server-url URL --token T --username USER
webcodex-cli agent-tokens revoke --server-url URL --token T --username USER --token-id ID

# Agent config init (same agent.toml as `webcodex-agent init`)
webcodex-cli agent init --server-url URL --token <wc_agent_token> --client-id ID --owner USER \
  [--display-name N] [--transport websocket|polling] [--projects-dir PATH] \
  [--allowed-root PATH...] [--allow-cwd-anywhere BOOL] --output PATH|- [--overwrite]

# First-pass single-user setup
webcodex-cli setup single-user --server-url URL --token <bootstrap> --username USER \
  --client-id ID --output-dir PATH [--display-name N] [--role admin] \
  [--gpt-token-name chatgpt-action] [--agent-token-name "<client-id> agent"] [--json]
```

Token resolution order for management/setup commands: `--token` >
`--token-file` > `WEBCODEX_TOKEN`. For `agent init`: `--token` >
`--token-file` > `WEBCODEX_AGENT_TOKEN`.

### `setup single-user` behavior

1. Create the user (tolerates a "user already exists" error and continues).
2. Create a personal API token for GPT Actions with scopes
   `runtime:read`, `project:read`, `project:write`, `job:run`.
3. Create an agent token bound to `--client-id` with scopes
   `agent:register`, `agent:poll`, `agent:result`, `agent:job_update`.
4. Save the returned plaintext tokens to 0600 files under `--output-dir`:
   `webcodex-user-token` and `webcodex-agent-token`.
5. Print a concise summary: username, client_id, **token prefixes only**,
   output file paths, and next steps. The bootstrap token is never printed.

With `--json`, the summary is machine-readable JSON (paths + prefixes; no full
token values).

### Security

- Never prints the bootstrap token, Authorization header, env files, or full
  `agent.toml` contents with secrets.
- The only path that emits token-bearing content is the explicit
  `agent init --output -` stdout path, requested deliberately by the user.
- Tests use fake tokens only.

## Compatibility with existing binaries

- `webcodex users/tokens/agent-tokens ...` admin commands remain working
  (compatibility wrappers around the same `admin_cli` module).
- `webcodex-agent init` remains working (now delegates to the shared
  `agent_init` module).
- `webcodex --help` / `--version` and `webcodex-agent --help` / `--version`
  still work.

## Agent policy summary (runtime status / listAgents)

Registration now carries a sanitized `AgentPolicySummary`:

```json
{
  "policy": {
    "allow_raw_shell": true,
    "allow_cwd_anywhere": false,
    "allowed_roots": ["/root"],
    "max_timeout_secs": 3600,
    "max_output_bytes": 262144
  }
}
```

This is exposed per-agent in `runtime_status` (`agents.clients[].policy`) and
`listAgents` (`agents[].policy`) for both websocket and polling agents.

**Never exposed:** token, Authorization header, env, full agent.toml contents,
shell `init_script` contents, or shell env values. Older agents that register
without a policy surface `null` for the field and remain fully compatible.

## Default `allowed_roots` = `$HOME`

Agent policy behavior changed:

- If `allowed_roots` is explicitly configured in `agent.toml`, it is used as-is
  (overrides the HOME default).
- If `allowed_roots` is missing or empty, it defaults to `[$HOME]`.
- If `HOME` is unavailable and `allow_cwd_anywhere` is false, config loading
  fails with a clear error.
- Sensitive path protections (dangerous system roots) still apply; allowing
  `$HOME` does not grant access to token/env/ssh/config secrets.

This applies both at `agent init` generation time and at `load_config` runtime
time, so a minimal `agent.toml` without an explicit `[policy]` section works
predictably.

## `runCodexTask` clarification

`runCodexTask` (`run_codex`) starts a Codex CLI job in the project cwd on the
owning agent machine. It:

- **requires** the Codex CLI to be installed and configured on the agent
  machine;
- does **not** start a new WebCodex agent;
- should be skipped in favor of the normal WebCodex file/shell tools
  (`readProjectFile`, `runProjectShellCommand`, `applyProjectPatchChecked`,
  …) when the Codex CLI is unavailable on the agent.

No functional Codex integration changes were made in this phase.

## OpenAPI / MCP boundaries

- OpenAPI operation count remains **27**.
- `user` / `token` / `agent-token` / `setup` / `audit` endpoints are **not**
  exposed in the GPT Actions OpenAPI schema.
- MCP does not expose token creation.
- No duplicate operation IDs.
- GPT Actions still uses a personal API token; agents/clients still use a
  `wc_agent` token.

## Known limitations / TODO

- `setup single-user` is a first pass: it does not yet revoke/rotate existing
  tokens, does not write an `agent.toml`, and does not implement `--force-create-tokens`
  semantics beyond accepting the flag. Audit is not implemented (deferred).
- `webcodex-agent` is not renamed yet (deferred to a later phase).
