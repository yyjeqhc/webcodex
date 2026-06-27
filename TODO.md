# TODO

## Current maintenance backlog

- [ ] Share one production/test API router builder so OpenAPI, runtime HTTP tests, and production routes cannot drift.
- [ ] Add a dedicated docs CI check for deleted-document references, old product names, and obvious real token patterns.
- [ ] Keep compatibility entry points documented as compatibility only: `webcodex users`, `webcodex tokens`, `webcodex agent-tokens`, and `webcodex-agent init`.
- [ ] Keep GPT Actions OpenAPI and MCP free of users, API-token, agent-token, pairing/enrollment, setup, doctor, npm, server management, and audit management endpoints.
- [ ] Document future policy-summary fields carefully so tokens, env values, `Authorization` headers, complete `agent.toml`, and shell `init_script` values are never exposed.
- [ ] Keep optional `runCodexTask` docs clear: Codex CLI must be installed/configured on the agent machine and the action never starts a new `webcodex-agent`.
