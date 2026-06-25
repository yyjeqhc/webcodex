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

There is no `client_request_id` deduplication and no `runJobOp` aggregate op in
the current runtime. Do not re-create a job blindly after a timeout; poll its
`job_id` with `getRuntimeJobStatus` first.

See [GPT_ACTIONS.md](GPT_ACTIONS.md) and [README.md](../README.md).
