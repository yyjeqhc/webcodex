# External Codex bidirectional acceptance — 2026-10-13

## Status

This is an **incomplete acceptance checkpoint**, not a claim that either
real-model path passed. The first real run reached an online authentication
failure. It preserved and reconciled the original managed execution through a
task-owned Server restart, then stopped its test processes.

At the time of this checkpoint, the existing Goal was still active and
its original six-hour budget had been removed. The blocked real-model test
needed normal Codex reauthentication, not a longer execution deadline. No
publication, merge, shared-service restart or authentication-file inspection
was performed during this recorded run. This is historical evidence, not a
statement of current Goal liveness or current Codex login state.

## Source and runtime identities

| Item | Observed identity |
|---|---|
| Source used to build test binaries | `f8bf8dab01371bb8551d586f94f5e3b4254b5371` |
| Development branch | `feat/codex-bidirectional-acceptance-20261013` |
| Test Server and Runner | `0.5.0`, commit `f8bf8dab0137`, `dirty=false`, build timestamp `1791852688` |
| Shared 1011 Server and special Runner | `0.5.0`, commit `45c9baa289ab`; not redeployed by this task |
| System Codex | `codex-cli 0.162.1` |
| ACP package used by the real run | `@agentclientprotocol/codex-acp@2.1.1` |
| Node / Python | `22.23.2` / `3.12.3` |
| Requested model / effort | `gpt-5.6-luna` / `medium`; no dispatched model prompt was observed |
| Tested harness SHA-256 | `422f1f3345fa17d072482cc54db16f7da264e9fe3d26baf447b08298c0774567` |

The adapter's bundled Codex resolved to 0.159.3. A separate configuration-only
comparison using it also failed at `session/new` with workspace-routing 401.
The real whole-chain run explicitly selected the system 0.162.1 CLI. Neither
comparison dispatched a coding prompt. The old cached ACP 1.6.2 was not used.

Task-owned dogfood binary SHA-256 values:

- Server: `c850a83789618629bf0c98a1a3403c11d44155695727b47fc43405c189414bf4`
- Runner: `46be8230a11ce1bdcadebb372af7488e2d608dabbafe310d3bba7c9073241744`

## Fixed integration boundaries

Ordinary MCP consumption needs an authenticated MCP endpoint, the target
tool's scope and an exact registered Project. A Goal, Workflow Session, durable
Agent, AgentTask or attempt reference is not required for this path. The test
uses only `runtime:read` and `project:read` on an isolated service account,
exposes `read_files` in the Codex client, and checks actual completed MCP
receipts against copied, independently readable alpha/beta fixtures.

Managed delegation uses the existing AgentTask/CodingAgentRun backend. The
Host creates a bounded Python clamp task, claims one Attempt and preserves the
exact Run binding before observing it. The fixed seven acceptance tests come
directly from the existing Runner dogfood fixture. Separate processes run the
fixed acceptance and the Agent's added tests; fixed-test and implementation
hashes guard the final acceptance evidence.

Workflow Session handoff is bounded saved context and optional evidence
provenance. It does not own the managed Attempt. External reports remain
reported claims; they cannot supply native Job or managed backend terminal
authority. See [the current managed binding contract](acp-coding-agent-run.md#managed-agenttask-binding)
and [the integration guide](../../integrations/codex/README.md).

## Real run 01

Outer native Job: `wc_job_9sXaebyN5EbLJMBD`, observed terminal exit code **1**.

Task-owned report on special:
`/root/git/webcodex/target/codex-bidirectional-20261013/run-01/report.json`.
The adjacent `host-job.json` explicitly correlates that report with the outer
Job and this Goal/Session. The native Job is not the managed Run.

### Ordinary MCP consumer

The real Codex process exited 1. No verified completed MCP tool receipt was
retained, so ordinary tool-consumption acceptance remains **not passed**.

The bounded diagnostic classifier matched `refresh_token_invalidated` and
`failed_to_refresh_token`, together with 401/unauthorized markers. These are
classification counts, not HTTP request counts. The temporary consumer
transcript was deleted after evidence extraction.

Regular `codex login status` reported a ChatGPT login and `codex doctor --json`
reported locally configured File/ChatGPT credentials. Those checks did not prove
online authorization. The later invalidated-refresh diagnostic provides the
concrete reason to use the official account/login flow before another model run.
No auth files, token values or full private configuration were read or exposed.

### Managed task and original-execution recovery

| Identity | Value |
|---|---|
| Project | `agent:codex-acceptance-runner:fixture` |
| Assignee | `wc_dagent_ZYVOF7gZw0p6wJ68` |
| Task | `wc_agent_task_mXdOujlhgGrtQY2l` |
| Attempt | `wc_agent_task_attempt_iyViqq_LQ0MEYUm_` |
| Attempt controller generation | `1` |
| Run | `wc_agent_run_SDXvptoE155puMIHRUshuyeQ0jGi5Ro0sobKBoRQCqU` |
| Provider | `codex` |

The private attempt record retains the exact claim/fence. Public selectors do
not replace reauthorization.

The actual state sequence was `starting -> failed`, with
`coding_agent_session_new_failed`. The final execution classification was
`not_started`; Task and Attempt both reconciled to `failed`.

During `starting`, the task-owned Server changed from PID 3181630 to 3182226,
using the same database, while Runner PID 3181737 survived. Exact-bound replay
and subsequent reconciliation referred to the same Run. The ACP audit recorded
**one adapter process and zero prompt dispatches**.

This proves recovery of the original **startup failure** across an actual
Server process restart. It does not prove recovery during a dispatched coding
prompt, successful coding, or a real-model outcome-unknown scenario. The report
correctly retains `managed.passed=false` and `recovery.passed=false`.

An independent reviewer found both the fixture's Git diff and status empty.
The seven-test baseline failed as expected against the unimplemented scaffold;
that negative control is not successful business acceptance.

## Focused verification and review

| Evidence | Result | Layer and limit |
|---|---|---|
| Dogfood build — `wc_job_LTvCqE5TS9t7dlRK` | Passed | Actual Server/Runner build, not an execution acceptance |
| Store AgentTask tests — `wc_job_uk8wkjURRQhwNbs_` | 49 passed | Deterministic SQLite/fence/replay/lease tests |
| Reopened-DB runtime test — `wc_job_N1Ycj2hwm3n_i2EP` | 1 passed | Fresh runtime and injected original backend terminal inventory |
| Runner recovery tests — `wc_job_N7tTtMyP0Wbut6E9` | 5 passed | Fake ACP, including real helper process and descendant cleanup |
| Real bidirectional run — `wc_job_9sXaebyN5EbLJMBD` | Failed | Real Codex/ACP processes, online authentication blocked before coding |
| Independent fixture inspection | Empty diff/status | No implementation or fixed-test edits |
| Markdown local-link check and diff whitespace check | Passed | Documentation structure only |

The focused tests cover unresolved-execution replacement refusal after ordinary
lease expiry, exact-attempt terminal reconciliation, persisted unique bindings,
post-barrier uncertainty, corrupt/missing durable records and child-tree cleanup.
They must not be described as real Codex model acceptance.

Independent review corrected the acceptance harness before the real run:
fixed tests run separately from Agent-added tests; raw consumer diagnostics are
projected and removed; failed stages remain independent; cleanup first observes
the original Run; stderr is drained; the explicitly owned consumer process
group is checked even when its leader exits.

After the real run, service-log cleanup was tightened to retain only fixed
bounded diagnostic categories and delete the explicitly owned raw log files.
The helper was applied to the three closed run-01 service logs (207, 1063 and 0
bytes); all reported `raw_deleted=true`. The separate `service-log-cleanup.json`
records this operation without changing the original report. Its script SHA-256
is `8e4fb657dd467d43f413f6a06503867448a4d8795a288bc67d22edc02ea90873`.
This later cleanup revision was independently reviewed; it was not used for a
second model run. The original real-run evidence remains scoped to `422f…`.

The documentation now describes the implemented TaskAttempt binding instead of
deferring all durable Agent integration. No product Rust lifecycle, authority,
schema or persistence changes were justified by the authentication failure.

## Goal / Host continuation investigation

Exact active Goal: `wc_goal_Z78pUAPzN72kTIOk`.
Exact Workflow Session: `wc_sess_kzhKxpHJAYgKM0bP`.
Controller: `wc_dagent_ajWQsknTyJMjQcb0`.

The initial endpoint was `wc_endpoint_4qKFltYZ60ZlYS6U`, generation 1. It
expired. The morning continuation was a user message, not a consumed Wake.

Read-only inspection of sf's live database and correlated request records found
a successful continuation binding, but no Goal attention event and no Wake.
In Shanghai time, the last meaningful correlated activity was 00:24:55.975,
the last Goal sync was 00:27:06.663, and the five-minute inactivity threshold
would first be reached at 00:29:55.975. The endpoint's final lease expiry was
00:29:56.406. No sync reached that threshold.

The examined implementation evaluates Goal stall state on valid observation
syncs; it has no independent timer scanner. Why the Host stopped its syncs is
not established by the Server evidence. No new-round/precise-Wake-consume
success is claimed, and no shared continuation state was altered to fabricate it.

## Configuration, cleanup and next entry

All test instances, local PATs, Runner configuration, fixture and reports are
under `target/codex-bidirectional-20261013`. No new development worktree was
created. MCP configuration is per invocation; the ACP provider exists only in
the task-owned Runner configuration.

The original and restarted Servers, Runner and consumer PID were independently
checked absent after cleanup; the consumer process group was also stopped. The
original Run has a known failed terminal state. Private recovery files remain
mode 0600 under a 0700 directory and must stay local. A later observation must
reuse the recorded task-owned database/configuration, never a shared instance.

The next model-dependent action requires the user's normal Codex login on
special. After authentication changes, retain this failed execution as evidence
and follow the canonical task-attempt contract for a deliberate new bounded
acceptance execution. Do not reclassify it, infer live-model success from a
configuration flag, or retry uncertain original work under a new identity.

## Issue coverage

[#1076](https://github.com/yyjeqhc/webcodex/issues/1076) explicitly asks about an
existing external executor that is not a CodingAgentRun provider. The current
ACP/TaskAttempt backend has implemented ownership and recovery machinery, with
the deterministic and startup-failure evidence above. This does not fulfill a
generic foreign-executor attestation/binding contract, trusted business-progress
ingress, universal released Host continuation, or Windows installer acceptance.
Ordinary MCP consumption and successful managed coding still require the
blocked real-model acceptance.

For [#569](https://github.com/yyjeqhc/webcodex/issues/569), the next narrow design
is an exact schedule-occurrence identity bound to an existing execution, with
explicit overlap/catch-up policy and recovery of that same occurrence. A Goal
stall observation is not a durable cron scheduler.

For [#784](https://github.com/yyjeqhc/webcodex/issues/784), the next useful
acceptance is a real same-authority saved-handoff transfer across credentials
and an explicitly configured model review, with credential/capability
diagnostics. Saved decisions and progress do not imply restoration of hidden
reasoning, unsaved full transcripts, or automatic account/model routing.

These are scope conclusions rather than acceptance of the original Issue
#1076 end-to-end requirements. The recorded test did not publish, merge or
post to external discussions; a subsequent PR may carry this historical
checkpoint without turning any missing acceptance into a passing result.
