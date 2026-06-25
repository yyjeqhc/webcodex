# Codex Usage Workflow (Deprecated)

> **This document is deprecated.** It was built around `pdctl.py workflow`,
> `snapshot`, `doctor`, `hook`, and `precommit` commands, which call the
> removed `/api/codex/project_workflow` and `/api/codex/project_doctor` routes.
> **Those routes are not mounted in the current runtime**, and `pdctl.py` itself
> is a legacy helper (see `scripts/pdctl.py` deprecation notice).

## Current Codex task workflow

The current runtime runs Codex CLI tasks through the shared `ToolRuntime`. The
recommended evidence loop:

1. `getRuntimeStatus` — confirm the runtime is healthy and projects are
   configured (`POST /api/runtime/status`).
2. `listProjects` — get the `project` id (`POST /api/projects/list`).
3. `runCodexTask` — start the task; capture `job_id`
   (`POST /api/codex/run`). The runtime constructs the Codex command; do not
   assemble raw shell to run Codex.
4. `getRuntimeJobStatus` — poll `job_id` until terminal
   (`POST /api/jobs/status`).
5. `getRuntimeJobLog` — read bounded stdout/stderr
   (`POST /api/jobs/log`).
6. `getProjectGitStatus` / `readProjectFile` — inspect the result
   (`POST /api/projects/git_status`, `POST /api/projects/read_file`).

Run verification with:

```bash
cargo fmt --check
cargo check
cargo test
```

See [GPT_ACTIONS.md](GPT_ACTIONS.md) and [README.md](../README.md) for the full
endpoint reference. There is no `recommended_next_action` / `action_budget_hint`
/ `hook_result` field in the current runtime.
