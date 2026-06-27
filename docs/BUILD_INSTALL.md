# Build and Install Quick Reference

This is the short install path. See [DEPLOYMENT.md](DEPLOYMENT.md) for production details.

## Build binaries

Build the three current binaries for your host:

```text
webcodex
webcodex-agent
webcodex-cli
```

Do not run unauthenticated production deployments.

## Initial setup

Recommended single-user setup:

```bash
webcodex-cli setup single-user
```

This creates the initial user API token for GPT Actions/MCP and an agent token for the agent.

Compatibility commands still work, but should not be the first choice in new docs:

```bash
webcodex users ...
webcodex tokens ...
webcodex agent-tokens ...
```

## Agent config

Recommended agent config generation:

```bash
webcodex-cli agent init
```

`webcodex-agent init` remains available as a compatibility entry point.

Agent policy defaults:

- Missing or empty `allowed_roots` defaults to `$HOME`.
- Explicit `allowed_roots` replaces the `$HOME` default.
- To narrow an agent, set an explicit workspace root such as:

```toml
[policy]
allowed_roots = ["/root/git"]
```

The example above is a narrowing example, not the default.

## Auth reminders

Use:

```text
Authorization: Bearer <token>
```

for REST, polling, MCP, and GPT Actions.

`?token=` is allowed only for `/api/agents/ws` WebSocket handshake compatibility.

## systemd PATH reminder

systemd services do not read interactive shell startup files such as `~/.bashrc`. If commands need Rust/Cargo, Node, or Codex CLI, expose them through the agent `[shell].path_prepend` / `[shell].env` config or through the service manager's environment.

`runCodexTask` is optional and requires Codex CLI on the agent machine. It does not start a new `webcodex-agent`.
