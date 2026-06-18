# Trusted Raw Commands

Single-user self-hosted advanced mode for executing multi-line shell commands
within whitelisted project roots, with audit logging and safety guardrails.

## When to Use

- **Short checks**: `create_trusted_raw_and_approve` (sync, default 120s timeout, max 1800s)
- **Long scripts**: `runJobOp create` with `trusted=true` + `script_text` (async)
- **Large output**: use `response_mode=summary` (default) or `minimal`

## Important: `create_trusted_raw` (non-approve) is Unsupported

`create_trusted_raw` without immediate approval is currently **unsupported**.
The approve workflow does not know how to re-execute trusted raw commands.

Instead, use:
- `create_trusted_raw_and_approve` for short synchronous trusted commands
- `runJobOp create` with `trusted=true` + `script_text` for async trusted jobs

## How Trusted Async Jobs Work

When you use `runJobOp create` with `trusted=true` and `script_text`:

1. The server validates the script (denylist, secret read, background escape)
2. A job directory is created at `.codex/jobs/<job_id>/`
3. The script is written to `.codex/jobs/<job_id>/script.sh` with:
   ```bash
   #!/usr/bin/env bash
   set -euo pipefail
   <your script_text>
   ```
4. The job command executes `bash .codex/jobs/<job_id>/script.sh`
5. The create response is lightweight (job_id + status, no stdout/stderr)
6. Use `runJobOp recover/status/log/summarize` to check results later

### SSH Executor Limitation

Trusted `script_text` jobs are **not yet supported** for SSH executor projects.
For SSH projects, use `script_path` (pointing to a script file in the project)
or create the script file manually before running the job.

## Security Boundaries

Trusted mode retains these limits:

- **Project root**: commands run only in whitelisted project directories
- **Denylist**: blocks `rm -rf /`, `mkfs`, `dd of=/dev`, `systemctl`, `git push`,
  `docker system prune`, fork bombs, and other system-destructive commands
- **Secret protection**: blocks `cat .env`, `cat id_rsa`, `cat *.pem`, etc.
  Use `ls` to see filenames, not content
- **No background escape**: `nohup`, `disown`, trailing `&` are blocked;
  use `runJobOp` for async execution
- **Output truncation**: stdout max 40k, stderr max 20k
- **Audit logging**: every execution recorded to `.codex/audit/`
- **Timeout**: default 120s, max 1800s

## Recovery After Timeout (504)

If a trusted raw command times out:

1. Check `runJobOp recover` or `runJobOp status` with the `job_id`
2. Do NOT rerun the same command blindly
3. For long tasks, use `runJobOp create` with `trusted=true` and `script_text`

## Do NOT

- Read secrets (`.env`, `id_rsa`, `*.pem`, `*.key`)
- Run `git push`
- Use `nohup` or background `&` — use jobs instead
- Modify system services or daemon configs

## Examples

### Short multi-line check (sync)

```bash
grep -RIn "foo" src tests || true
```

```bash
python3 - <<'PY'
from pathlib import Path
print(Path("src").exists())
PY
```

### Multi-line script (async job, local executor only)

```json
{
  "op": "create",
  "project": "my-project",
  "goal_id": "goal-123",
  "trusted": true,
  "script_text": "cargo fmt --check\ncargo clippy -- -D warnings\ncargo test",
  "reason": "pre-merge checks"
}
```

### Response modes

- `summary` (default): tail of stdout/stderr
- `full`: more output, still truncated at 40k/20k
- `minimal`: only exit_code, duration_ms, cwd
