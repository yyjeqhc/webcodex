# Job 504 Recovery Guide for GPT Actions

When a `runJobOp create` or `runJobOp status` call times out (504), the job may
already exist or have completed. Do **not** re-run the job. Follow this flow:

1. **Recover**: Call `runJobOp` with `op=recover` and the same `client_request_id`
   used in the original `create`. This reads only metadata (no logs, no process
   checks) and returns the job's ID, status, and exit_code if available.

2. **Lightweight status**: If `recover` shows the job exists, call
   `runJobOp op=status detail=basic` for a slightly richer status check.
   `detail=basic` skips OOM detection and SSH process probes, reducing 504 risk.
   Note: basic is metadata/status-file based and may be stale for SSH jobs;
   use `detail=logs`, `log`, or `summarize` for deeper inspection.

3. **Read logs only when needed**: Call `runJobOp op=status detail=logs` or
   `op=log` to read stdout/stderr tails. `tail_lines` only affects
   `detail=logs` or `op=log`; the default `detail=basic` never reads logs.

4. **Idempotent create**: Always pass `client_request_id` when creating jobs.
   If `create` is retried with the same ID, the existing job is returned instead
   of creating a duplicate.

## Summary of detail levels

| Op | Detail | SSH calls | Logs | OOM | Process check |
|----|--------|-----------|------|-----|---------------|
| recover | — | 1-2 | No | No | No |
| status | basic | 1-2 | No | No | No |
| status | logs | 2-3 | Yes | No | No |
| log | — | 1 | Yes | No | No |
