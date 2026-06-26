# Job 504 Recovery (Deprecated)

> **This document is deprecated.** It describes `runJobOp` with
> `op=recover`/`status`/`log`/`summarize`, `client_request_id` job
> deduplication, and SSH process probes. **`runJobOp` and the SSH executor no
> longer exist in the current runtime.**

## Current job lifecycle

The current runtime uses dedicated, named job endpoints backed by the shared
`ToolRuntime`:

- Start a Codex task: `POST /api/codex/run` (`runCodexTask`) → returns
  `job_id`.
- Poll status: `POST /api/jobs/status` (`getRuntimeJobStatus`).
- Read logs: `POST /api/jobs/log` (`getRuntimeJobLog`), with `tail_lines` and
  `offset` (`next_stdout_line`) for bounded pagination.

Recovery behavior:

- **Local jobs** write metadata under `.codex/jobs/<job_id>/`. When
  `job_status` / `job_log` receive an unknown local-looking `job_id`, the
  runtime searches configured projects for
  `.codex/jobs/<job_id>/metadata.json`, verifies the job belongs to a
  configured project, and rejects paths outside project roots. So a server
  restart does not lose local job history.
- **Agent jobs** are tracked in memory by `ShellClientRegistry`. A server
  restart loses in-flight agent jobs (a known limitation; see
  [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md)).

Timeout & process reclamation (local jobs):

- Local jobs are spawned with `setsid` so the wrapper shell is a session and
  process-group leader; its pid is recorded as `process_group_id` in
  `metadata.json`.
- When `job_status` / `job_log` detect a `running` local job past
  `max_runtime_secs`, the runtime sends `SIGTERM` to the whole process group
  (`kill -TERM -<pgid>`), escalates to `SIGKILL` after a short grace window if
  still alive, and persists a terminal `lost` status with a `note`. The group
  is never left running.
- An internal `POST /api/jobs/stop` stops a running local job the same way and
  marks it `stopped`. It is a thin REST wrapper over `ToolRuntime::stop_job`
  and is **not** a GPT Action (absent from `openapi.json`).
- Old metadata written before pid/pgid tracking has no `pid` file or
  `process_group_id`; on timeout/stop such jobs are only marked
  `lost`/`stopped` — the runtime never guesses a pid to kill.
- Kill failures never panic; a conservative terminal status is persisted
  regardless.

There is no `client_request_id` deduplication and no `runJobOp` aggregate op in
the current runtime. Do not re-create a job blindly after a timeout; poll its
`job_id` with `getRuntimeJobStatus` first.

See [GPT_ACTIONS.md](GPT_ACTIONS.md) and [README.md](../README.md).
