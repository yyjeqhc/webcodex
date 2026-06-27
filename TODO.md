# TODO

## Current maintenance backlog

- [ ] Consider sharing one production/test API router builder so OpenAPI, runtime HTTP tests, and production routes cannot drift.
- [ ] Add a dedicated docs CI check for forbidden historical document references and old product names.
- [ ] Add a lightweight validation command that checks `runtime_status`, `listAgents`, `listProjects`, OpenAPI operation count, and MCP tools/list parity.
- [ ] Keep the recommended setup path centered on `webcodex-cli setup single-user` and `webcodex-cli agent init`.
- [ ] Keep compatibility entry points documented as compatibility only: `webcodex users`, `webcodex tokens`, `webcodex agent-tokens`, and `webcodex-agent init`.
- [ ] Keep GPT Actions OpenAPI free of users, API-token, agent-token, setup, and audit management endpoints.
- [ ] Document any future policy-summary fields carefully so tokens, env values, `Authorization` headers, complete `agent.toml`, and shell `init_script` values are never exposed.
- [ ] Improve operator docs for optional `runCodexTask`: Codex CLI must be installed/configured on the agent machine and the action never starts a new `webcodex-agent`.
