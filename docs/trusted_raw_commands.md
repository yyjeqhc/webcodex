# Trusted Raw Commands (Deprecated)

> **This document is deprecated.** It describes
> `create_trusted_raw_and_approve`, `runJobOp create` with `trusted=true` +
> `script_text`, `goal_id`-scoped execution, and the SSH executor. **The
> `command_request` / goal workflow and the SSH executor have been removed from
> the current runtime.** None of these operation ids or fields exist.

## Current runtime

The current runtime does not expose a trusted-raw-command or goal-scoped
approval flow. Execution is driven through the shared `ToolRuntime`:

- Shell execution: the `run_shell` tool (via `callRuntimeTool`,
  `POST /api/tools/call`), subject to project roots and agent policy.
- Long-running work: `runCodexTask` (`POST /api/codex/run`) for Codex CLI
  tasks, or the `run_job` tool for async shell jobs. Poll with
  `getRuntimeJobStatus` / `getRuntimeJobLog`.
- Project paths are always confined to configured roots; traversal and absolute
  paths are rejected.

There is no `goal_id`, no `client_request_id`, no `trusted` flag, and no
`script_text` job mode in the current API. See [GPT_ACTIONS.md](GPT_ACTIONS.md)
and [README.md](../README.md) for the supported surface.
