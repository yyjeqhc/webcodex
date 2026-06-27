# TODO

## Current maintenance backlog

- [ ] Consider sharing one production/test API router builder so OpenAPI, runtime HTTP tests, and production routes cannot drift.
- [ ] Add a dedicated docs CI check for forbidden historical document references and old product names.
- [x] Add a lightweight validation command that checks `runtime_status`, `listAgents`, `listProjects`, OpenAPI operation count, and token boundaries.
- [ ] Keep the recommended server bootstrap path centered on `webcodex-cli server init`, `webcodex-cli server install-service`, and `webcodex-cli server status`.
- [x] Add a separate client-side setup/enroll flow for user API tokens and agent tokens.
- [ ] Keep compatibility entry points documented as compatibility only: `webcodex users`, `webcodex tokens`, `webcodex agent-tokens`, and `webcodex-agent init`.
- [ ] Keep GPT Actions OpenAPI free of users, API-token, agent-token, pairing/enrollment, setup, doctor, npm, server management, and audit endpoints.
- [ ] Document any future policy-summary fields carefully so tokens, env values, `Authorization` headers, complete `agent.toml`, and shell `init_script` values are never exposed.
- [ ] Improve operator docs for optional `runCodexTask`: Codex CLI must be installed/configured on the agent machine and the action never starts a new `webcodex-agent`.
