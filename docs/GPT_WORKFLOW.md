# GPT workflow for Private Drop v4

Use `/codex-openapi-compact.json` for GPT Actions. It keeps the action set small
while preserving the main Codex loop.

## Core loop

1. Observe state with `getProjectContextBatch`.
2. Read only the files needed for the task.
3. Prefer `applyProjectEdit` for deterministic edits.
4. Run the smallest useful check first, then broader checks.
5. Commit with `runProjectGit` after checks pass.
6. Write a final report with `writeProjectReport`.

## Long-running task workflow (job+poll — REQUIRED for any task >30s)

**Default interface for all long tasks: `runJobOp`.**

Use `runProjectCheck` only for checks expected to complete in under 30 seconds.
For everything else — training, evaluation, data processing, multi-step scripts — use `runJobOp`.

### Step-by-step

1. **Create** the job (requires an active goal):

```json
{
  "op": "create",
  "project": "paper",
  "goal_id": "<active-goal-id>",
  "client_request_id": "eval-seeds-2026-06-03T08:00:00Z",
  "script_path": "scripts/codex_jobs/run_eval.sh",
  "reason": "run full evaluation across all seeds"
}
```

2. **Return immediately** to the user with the `job_id` and estimated duration. Do not wait.

3. **First poll at +30s**:

```json
{"op": "log", "project": "paper", "job_id": "<id>", "tail_lines": 30}
```

Confirm startup is healthy. Check `stderr_tail` for errors.

4. **Poll every 60–120s** using `op=status`. Inspect `job.status`, `job.elapsed_secs`, `job.oom_hint`.

5. **On completion**, use `op=summarize` for the full picture, then `getProjectContext` with `mode=experiment_outputs` to audit artifacts.

### Key fields (new in P1/P2)

| Field | Where | Meaning |
|---|---|---|
| `elapsed_secs` | `JobInfo` (status) | Wall-clock seconds since start |
| `oom_hint` | `JobInfo` (status) | `"possible_oom"` if OOM signals found in stderr |
| `log_total_lines` | `JobOpResponse` (log) | Total lines in stdout.log so far |
| `next_cursor` | `JobOpResponse` (log) | Use as `since_line` in next poll for incremental read |
| `since_line` | `JobOpRequest` (log) | Read from this line (1-based) instead of tail |

### Incremental log polling (avoids re-reading the same lines)

```
First call: {"op": "log", "tail_lines": 50}
  → returns log_total_lines=120, next_cursor=121

Next call: {"op": "log", "since_line": 121, "tail_lines": 200}
  → returns only new lines 121+
```

## 504 / timeout recovery

**Never re-create a job after a 504.** The task is almost certainly still running.

```json
{"op": "status", "project": "<proj>", "client_request_id": "eval-seeds-2026-06-03T08:00:00Z"}
```

- If job found: continue polling.
- If 404: job was not created. Re-create with the **same `client_request_id`**.

The server deduplicates by `client_request_id` — identical retries return the existing job, never a duplicate.

## Multi-line scripts (use script_path, not command_text)

For any command that needs multiple lines, pipes, or shell logic:

1. Write the script with `applyProjectEdit`:

```
scripts/codex_jobs/run_experiment.sh
```

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

SEED=${1:-42}
python train.py --seed "$SEED" --epochs 50 \
    --output results/seed_${SEED}/ \
    2>&1 | tee logs/train_seed_${SEED}.log
```

2. Create the job with `script_path`:

```json
{
  "op": "create",
  "project": "paper",
  "goal_id": "<id>",
  "script_path": "scripts/codex_jobs/run_experiment.sh",
  "script_args": ["42"],
  "client_request_id": "train-seed42-2026-06-03T08:00:00Z"
}
```

## Monitoring running tasks

**Never use raw `ps`, `tail`, or `find` commands to monitor a job.** Use the Job API:

| Task | Op |
|---|---|
| Is it still running? | `op=status` → `job.status`, `job.elapsed_secs` |
| Show progress | `op=log` → `stdout_tail`, `stderr_tail` |
| Did it OOM? | `op=status` → `job.oom_hint` |
| Kill the job | `op=stop` |
| Summary of all jobs | `op=summarize` |

## Experiment audit (after job completion)

After a training or evaluation job finishes, audit artifacts in one call:

```json
{
  "project": "paper",
  "mode": "experiment_outputs",
  "limit": 100
}
```

Returns:
- `git_status` — staged/unstaged changes
- `output_files` — CSV, JSON, MD, PNG, LOG, etc.
- `checkpoint_files` — `.pt`, `.ckpt`, `.joblib`, `.npz`, etc.
- `large_files (>20MB)` — with size and `gitignored=yes/no`
- `untracked_new` — all new untracked files

Optional filter by recency:

```json
{"mode": "experiment_outputs", "limit": 50, "query": "30"}
```

(`query` = `since_minutes`: only files modified in the last 30 minutes)

## runProjectCheck — short checks only

`runProjectCheck` is for pre-configured suite commands that finish quickly:
- `fmt` — cargo fmt --check (seconds)
- `test` — cargo test (under 2 min for unit tests)
- `build` — quick build check

**Do NOT use `runProjectCheck` for:**
- Anything expected to take more than 30s
- Training, evaluation, dataset processing
- Any e2e test or experiment

For these, use `runJobOp` with `op=check` (async) or `op=create`.

## Goal-scoped workflow

Goal-scoped execution reduces repeated approvals without removing the user approval boundary.

1. GPT proposes a goal with `runCommandRequestOp`:

```json
{"op":"create_goal","project":"private-drop-v4","title":"Implement bounded task","summary":"What GPT will and will not touch.","ttl_secs":7200}
```

2. The goal starts as `pending` and grants no execution rights.
3. The user explicitly approves the returned `goal_id` in chat.
4. GPT activates the goal:

```json
{"op":"approve_goal","goal_id":"<goal-id>"}
```

5. While the goal is active, GPT may run bounded operations such as:

```json
{"op":"create_and_approve","project":"private-drop-v4","goal_id":"<goal-id>","command":"test","reason":"verify changes"}
```

or:

```json
{"op":"create_raw_and_approve","project":"private-drop-v4","goal_id":"<goal-id>","command_text":"git status --short","reason":"inspect state"}
```

6. GPT closes the goal when done:

```json
{"op":"close_goal","goal_id":"<goal-id>","reason":"completed"}
```

## Safety rules

- `create_goal` does not grant execution rights.
- Only `active` and unexpired goals allow `*_and_approve` operations.
- `pending`, `rejected`, `expired`, and `closed` goals cannot auto-approve commands.
- Raw commands still require `allow_raw_command_requests = true`.
- Configured commands still require `allow_command_requests = true`.
- Raw commands are single-line, length-limited, and checked for high-risk tokens.
- All command executions still create normal `command_requests` audit records.

## Example 1: 90-second heartbeat smoke test

```
User: "Run a 90-second heartbeat smoke test"

GPT:
1. [create goal] → goal_id=G1
2. [user approves G1]
3. [activate goal]
4. [applyProjectEdit] write scripts/codex_jobs/heartbeat.sh:
   #!/usr/bin/env bash
   for i in $(seq 1 9); do echo "beat $i"; sleep 10; done; echo "done"
5. [runJobOp create] script_path="scripts/codex_jobs/heartbeat.sh"
   client_request_id="heartbeat-2026-06-03T08:00Z"
   → job_id=J1
6. Tell user: "Job J1 started. Will poll in 30s."
7. (30s) [runJobOp log] tail_lines=20
   → "beat 1\nbeat 2\nbeat 3..." → healthy
8. (60s) [runJobOp status] → running, elapsed_secs=60
9. (90s) [runJobOp status] → completed, elapsed_secs=91, exit_code=0
10. [runJobOp summarize] → Markdown table
11. [close goal]
```

## Example 2: Paper experiment eval job

```
User: "Run formal evaluation for donor-distance experiment, seeds 1-5"

GPT:
1. [create goal] → goal_id=G2
2. [user approves G2]
3. [activate goal]
4. [getProjectContextBatch] modes=[agent_context, overview, git_status]
5. [applyProjectEdit] write scripts/codex_jobs/eval_donor_distance.sh:
   #!/usr/bin/env bash
   set -euo pipefail
   for seed in 1 2 3 4 5; do
     python eval.py --seed $seed --checkpoint checkpoints/best.pt \
       --output results/eval_seed_${seed}.csv
   done
6. [runJobOp create]
   script_path="scripts/codex_jobs/eval_donor_distance.sh"
   client_request_id="donor-dist-eval-2026-06-03T08:00Z"
   → job_id=J2
7. Tell user: "Job J2 started. Estimated 10-15 min. Will poll at +30s."
8. (30s) [runJobOp log] → confirm seed 1 started
9. (every 2min) [runJobOp status] → check elapsed_secs, oom_hint
10. On complete: [runJobOp summarize]
11. [getProjectContext] mode=experiment_outputs → audit CSV files
12. Report results to user
13. [close goal]
```

## Diagram assets

- `docs/diagrams/goal-workflow.svg`: browser-friendly static diagram.
- `docs/diagrams/goal-workflow.mmd`: Mermaid source for Markdown renderers.
- `docs/diagrams/goal-workflow.html`: standalone HTML diagram.
- `docs/diagrams/goal-workflow.excalidraw.json`: editable Excalidraw scene.
