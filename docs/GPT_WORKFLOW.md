# GPT Workflow (Deprecated)

> **This document is deprecated.** It describes the removed v4 GPT workflow
> built around `/codex-openapi-gpt.json`, `/codex-openapi-compact.json`,
> desktop tasks, `command_request` / goal workflow, `runJobOp`,
> `runProjectCheck`, `getProjectContextBatch`, and SSH execution. **None of
> these endpoints or operation ids exist in the current runtime.**

## Current GPT Actions surface

The current runtime exposes a single, minimal OpenAPI schema at
`GET /openapi.json`. See:

- [README.md](../README.md) — runtime overview and current endpoints.
- [GPT_ACTIONS.md](GPT_ACTIONS.md) — the import guide, the 9 current
  operation ids, and the recommended call flow.
- [RUNTIME_STATUS.md](RUNTIME_STATUS.md) — observability via `getRuntimeStatus`.

The recommended flow is now:

1. `getRuntimeStatus` — runtime health / config / agents.
2. `listProjects` — discover project ids.
3. `runCodexTask` — start a Codex CLI task; capture `job_id`.
4. `getRuntimeJobStatus` / `getRuntimeJobLog` — poll `job_id`.

Do not use `/codex-openapi-gpt.json`, `/codex-openapi-compact.json`, goal ids,
`client_request_id` job deduplication, or `runJobOp` — those are gone.
