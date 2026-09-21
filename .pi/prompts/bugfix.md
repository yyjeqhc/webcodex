---
description: DEPRECATED alias for /principal-bugfix. Same risk-adaptive workflow.
---
`/bugfix` is a deprecated alias; `/principal-bugfix` is the supported command. Execute the complete workflow below for `$@` and mention the deprecation once in the closing digest.

Execute this workflow for: $@

Initialize a `bugfix` run before any phase. Resolve the shipped tool at
`${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}/npm/node_modules/principal-pi-skills/scripts/assurance-state.mjs`,
falling back to `scripts/assurance-state.mjs` in a source checkout. Serialize the exact request as
a one-line JSON object (`{"request":...}` with JSON escaping), pass it on stdin to
`node <tool> init --workflow bugfix`, and keep its `run_id`. Never interpolate request text into a
shell command.
If initialization or later state validation fails, stop instead of replacing persistence with
prose. For an effective critical run, report `BLOCKED_CRITICAL_ASSURANCE`; otherwise report
`BLOCKED_ASSURANCE`.

<!-- assurance:shared:start -->
## Assurance controller

Run `node <tool> contract` once for the exact event payloads and gate commands; do not invent
fields from this prose. The state tool is the deterministic authority. It parses `--assurance lean|standard|critical`
(`high` aliases critical), `--critical-scope`, and natural-language requests such as “treat this
as critical” or “escalate this run to critical”. Standard is the default. Critical with no scope
means the entire run; narrowing it is explicit. Read the effective profile from `init`, not from a
guess. Append every phase, task packet, workspace, digest, finding decision, evidence receipt,
approval, escalation, and finish choice with `node <tool> event --run-id <id>` and a JSON event on
stdin. Never edit `events.jsonl` or `snapshot.json`; the former is hash-chained and the latter is
derived. Pass `run_id`, effective assurance, scope, task/workspace IDs, authority, and available
digests into every phase.

First establish authority (the exact request/requirements), risk (`tiny`, `substantive`, or
`consequential`), current base/head, dependencies, interfaces, and finish intent. A typo is tiny;
it does not become consequential because a workflow exists. Policy may elevate standard to
critical for migrations, auth/authz, billing, destructive data, public API breaks, credentials,
protected history, production effects, or another evidenced one-way door. Record the trigger.
User-requested critical is a minimum. A downgrade requires the user's explicit authorization and
a recorded reason; never propose one because a control is inconvenient.

### Profiles

- **Lean:** use the smallest reversible path. A tiny clear change may go straight to Build; do not
  force a worktree, plan artifact, or delegated review. Still record and run the exact targeted
  check after the last change before finishing.
- **Standard (default, Option B):** plan substantive work as verifiable vertical slices; preflight
  dependencies and interface conflicts; prefer an owned workspace for substantive work; keep one
  durable writer; review at vertical milestones and completion; adjudicate findings before repair;
  require fresh verification and an explicit finish choice. Tiny work stays on the lean-sized path.
- **Critical (selected Option C controls):** the rules below apply only to the explicit critical
  scope. For consequential work, obtain an explicit design approval containing validation and
  observability, rollback and abort strategy, and one-way doors. Tiny critical work does not earn
  an architecture document, but it does retain the critical isolation/review/evidence gates.
  Critique the current plan in an independent fresh context before recording task packets or starting Build. Create an owned isolated
  branch worktree with the shipped workspace tool's `create --branch principal/<run_id>` and record
  its canonical path and `workspace_id`; if that or the required fresh contexts cannot be
  created, stop with `BLOCKED_CRITICAL_ASSURANCE` and list every missing control. Never substitute
  inline self-review. Only after critique APPROVE, persist independently verifiable task packets, then run the tool's
  `pre-build` gate for each scoped task.

A task packet uses `schemas/assurance-task-packet-v1.schema.json` and contains Task ID, Authority,
Global constraints, Out of scope, critical-scope match, files/dependencies, Done command, Review
risk, workspace ID, and plan/definition digests. Plan remains read-only: the controller records the
packet outside the product tree. If an approved replan makes an immutable packet stale, append
`task_packet_superseded` with its ID and reason, then issue a new ID; never silently rebind it.
Build remains the only durable source writer and runs inline. For
an owned workspace, bind Build to its canonical path as `WRITER_ROOT`: every read/edit/write/find/
grep/ls path must be absolute beneath that root and every bash command must begin by changing to
that root. Verify its expected branch before writing. If any operation cannot be rooted there, stop;
never fall back to the caller checkout while claiming isolation. These are behavioral rules, not
path confinement: unrestricted `bash` can leave the root or spawn arbitrary processes, and actual
containment needs an OS sandbox or constrained non-shell broker. One workspace has one writer lease
(`build`) among controller-governed children; it cannot exclude external writers. Parallelize only
read-only work; separate writer worktrees need an explicit merge owner. A runtime may give tasks
fresh builder contexts, but this portable workflow never requires delegated Build.

`head_sha` alone does not identify uncommitted Build output. After each mutation and before every
evidence/review receipt, compute the candidate `tree_sha` with an initially absent temporary `GIT_INDEX_FILE`
(`git read-tree HEAD`, `git add -A`, `git write-tree`) so the real index is untouched; persist both
head and tree. Git-Ops must require its staged tree and final `HEAD^{tree}` to equal that receipt.

For every critical task, invoke the existing `principal-review` contract in two separate fresh
contexts: first `review axis: specification`, then `review axis: quality` (security and
maintainability included). Pass `Writer root:` as the canonical active writer path, its explicit
workspace ID, and `Expected candidate tree:`; the reviewer snapshots that root and verifies the
snapshot tree. Both must return APPROVE; no majority or partial gate counts. Record explicit
workspace/context IDs, record exact-target task evidence, and run `task-complete`. After all tasks
and their evidence, run a third fresh whole-change review over
both specification and quality; record APPROVE only when both sub-verdicts approve. Fresh
completion evidence after the last change includes the exact target checks, relevant full
suite/build/lint, a requirements trace, and risk-specific checks. For a regression, prove the test
fails against unfixed behavior when practical. Run the `finish` gate; stale receipts do not count.

### Delegation, findings, and changes in risk

When a step names `principal-*`, try that agent once. In lean/standard, only a missing subagent tool
or unknown agent falls back to the corresponding skill inline; say so in the digest. Any crash,
timeout, or empty response stops. In critical, absence is not permission for inline critique or
self-review: use another genuinely fresh governed context or return `BLOCKED_CRITICAL_ASSURANCE`.
Plan, Review, and Debug may experiment only in workspaces they own and remove; they never mutate the
active writer workspace. Build writes there once. Git-Ops stays inline and owns finalization.

A review verdict of `CHANGES-REQUESTED` enters adjudication; `UNVERIFIED` is never approval.
Before Build repairs review feedback, assign stable finding IDs and adjudicate each from
requirement/code/test evidence: `accepted`, `rejected` with reason, or `needs-context`. The last
stops for one load-bearing question. Pass only accepted IDs to Build's repair mode. Apply and verify
one accepted finding at a time, then review again; at most two repair rounds.

Escalation can happen at any time. If implementation started, freeze and record base/head/tree plus
current evidence, stop source writes, and elevate state. If a repair was active, append
`repair_suspended` with its finding/reason so the attempt remains auditable; then re-run critical
design/plan preflight, review completed tasks under both axes, backfill receipts, and only then clear
`critical_backfill_required`. Restart the accepted repair under the new critical task/workspace/Build
digests. No further Build or external side effect occurs while backfill is missing.

Before migration execution, push, publish, deletion, credential rotation, or production access in
any profile, record consequence-aware user approval and run the `side-effect` gate just in time;
critical never weakens or substitutes that gate. Approval for planning is not approval to execute.
Git-Ops finish mode verifies a fresh receipt, then offers exactly: merge locally, push/open PR, or
keep the branch. Start the Git-Ops phase, record one choice, and run `gate finalize`; this readiness
gate permits that active phase. For an external effect, record approval last and run `side-effect`
immediately before execution. Execute the choice, verify the staged and final `HEAD^{tree}` equal
the candidate, append `finalization_completed` with final branch/head/tree, then run `gate finish`.
Discard/cleanup occurs only on explicit request. Do not auto-push, publish, tag,
force-push, or destructively clean up.
<!-- assurance:shared:end -->

## Bugfix path

1. Invoke `principal-debug` (or Debug inline under the delegation rule). It reproduces and returns a
   proposed regression test/fix from a disposable workspace. NOT REPRODUCED, BLOCKED, or a design
   flaw stops; `Next: plan` surfaces the design change rather than hiding it in a repair.
2. Under critical assurance, turn the diagnosis into a Plan, run independent critique, then record
   its task packets before Build. Consequential fixes also need explicit approved design/rollback/abort.
3. Attach the chosen workspace and writer lease. Critical requires an owned isolated branch
   worktree; standard prefers one for substantive work. Run `pre-build`, then invoke Build inline:
   recreate the regression test, watch it fail, implement once, and record Red, Green, Full evidence
   plus changed paths.
4. Standard Review proves the regression test fails without the fix and passes with it in a
   disposable workspace. Critical runs separate specification and quality contexts for every task,
   then a fresh whole-change review. Adjudicate findings before one-at-a-time repair; two rounds max.
5. After fresh evidence, invoke Git-Ops inline in finish mode. Present and record one of merge, PR,
   or keep, then require clean finalize/side-effect gates before execution and finish after; wait if
   not already choose. End with `Digest:` followed by exactly six labeled lines:
   `Root cause:` (the concrete diagnosed cause), `Run/profile/scope:`, `Ref:` (including the full
   final commit SHA), `Assumptions/follow-ups:`, `Evidence gaps:`, and `Execution contexts:`
   (inline/fresh-context). No transcript narration.
