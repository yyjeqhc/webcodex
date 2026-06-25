# Project Workflows (Deprecated)

> **This document is deprecated.** It describes `project_workflow`,
> `project_doctor`, and `project_hook` routes (`/api/codex/project_workflow`,
> `/api/codex/project_doctor`) and agent-native workflow routes
> (`/api/shell/projects/workflow`, `/api/shell/projects/workflow_job`,
> `/api/shell/jobs/shell`, `/api/shell/jobs/shell_batch`). **None of these
> routes are mounted in the current runtime.** The SSH executor referenced here
> has also been removed.

## Current runtime

The current runtime does not expose project workflow / doctor / hook execution
routes. Project inspection is done through the dedicated read-only GPT Actions
and the shared `ToolRuntime`:

- `listProjects` — `POST /api/projects/list`
- `readProjectFile` — `POST /api/projects/read_file`
- `getProjectGitStatus` — `POST /api/projects/git_status`
- `callRuntimeTool` — `POST /api/tools/call` (advanced; accepts `git_status`,
  `git_diff`, `read_file`, etc.)

Long-running work is started with `runCodexTask` (`POST /api/codex/run`) and
polled with `getRuntimeJobStatus` / `getRuntimeJobLog`. See
[GPT_ACTIONS.md](GPT_ACTIONS.md) and [README.md](../README.md).

The `[projects.<name>.hooks]` and `[projects.<name>.checks]` fields in
`projects.toml` are still parsed for backwards compatibility, but no current
runtime route invokes them.
