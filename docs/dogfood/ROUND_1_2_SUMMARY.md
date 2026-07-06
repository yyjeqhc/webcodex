# WebCodex Dogfood Round 1 / Round 2 Summary

This document records the first structured dogfood results for WebCodex as an online GPT coding-agent runtime.

## Scope

The goal was to evaluate whether ChatGPT can use WebCodex to perform controlled coding workflows:

- inspect project state
- locate relevant files
- make minimal code edits
- run validation
- review diffs
- restore or leave workspace state intentionally
- finish with workspace hygiene and job state reporting

The initial dogfood fixture repository was:

```text
agent:oe:webcodex-dogfood-fixtures
/root/git/webcodex-dogfood-fixtures
```

The fixture was intentionally small and controlled. These results do not yet prove large-repository performance.

## Round 1: Acceptance Prompt Dogfood

Round 1 used strict developer-facing prompts with explicit tool expectations and fixed workflow constraints.

### MCP direct lane

| Task   | Description                                            | Result |
| ------ | ------------------------------------------------------ | ------ |
| DF-001 | `rust-mini` `normalize_whitespace` string behavior fix | PASS   |
| DF-002 | `rust-mini` even-length `median` logic fix             | PASS   |
| DF-003 | `multi-file-edit` `report_mode("debug")` semantic fix  | PASS   |

Observed successful workflow:

```text
start_coding_task
-> list/search/read
-> structured edit
-> cargo_test
-> show_changes / git_diff_hunks
-> git_restore_paths
-> show_changes
-> workspace_hygiene_check
-> finish_coding_task
```

Round 1 MCP direct established that the P0 coding loop works for:

* single-file string behavior fixes
* single-file numeric logic fixes
* small cross-file semantic localization
* structured validation
* clean restoration
* final workspace hygiene

No `run_shell` or `run_job` was required in the successful replayed runs.

### GPT Action generic callRuntimeTool lane

| Task          | Description                                                              | Result                                 |
| ------------- | ------------------------------------------------------------------------ | -------------------------------------- |
| DF-001 replay | Single-file `normalize_whitespace` fix through generic `callRuntimeTool` | PASS                                   |
| DF-003 replay | Cross-file `report_mode("debug")` fix through generic `callRuntimeTool`  | PASS after timeout contract correction |

The generic Action lane proved that `callRuntimeTool` can support a real coding loop, not just runtime sanity checks.

Important Action-specific contract:

```text
For cargo_* calls through GPT Action generic callRuntimeTool, pass timeout_secs: 120.
Do not pass wait_timeout_secs unless explicitly required by schema.
Use flattened top-level args.
Do not call start_session before start_coding_task for normal coding tasks.
```

Example:

```json
{
  "tool": "cargo_test",
  "project": "agent:oe:webcodex-dogfood-fixtures",
  "package": "multi-file-edit",
  "filter": "report_mode_debug_label",
  "timeout_secs": 120
}
```

## Round 1 Fixture Baseline Finding

The first DF-001 run exposed fixture baseline noise:

* `cargo_test` generated `Cargo.lock`
* `cargo_test` generated `target/`
* cleanup of `target/` triggered sensitive cleanup-path refusal
* an attempted shell cleanup with empty `cwd` produced a schema error
* final workspace was clean, but the session ledger contained unexpected failures

Resolution:

* commit `Cargo.lock` into the fixture baseline
* ignore build artifacts with:

```gitignore
/target/
**/target/
```

After this fixture baseline correction, the MCP direct and Action generic replays completed cleanly.

## Round 2: Casual Prompt Dogfood

Round 2 used natural user-style prompts instead of strict acceptance prompts.

Example prompt shape:

```text
项目是 agent:oe:webcodex-dogfood-fixtures。
median 偶数长度好像不对，你帮我看一下并修一下。
验证通过后不要提交，最后恢复 clean。
```

### Results

| Task   | Outcome                           | Notes                                                                           |
| ------ | --------------------------------- | ------------------------------------------------------------------------------- |
| CP-001 | Functional PASS, final clean PASS | Exploratory path miss and diagnostic failing test polluted finish verdict       |
| CP-002 | Functional PASS, final clean PASS | Used `run_shell` for cargo test; resolved syntax error caused mixed validation  |
| CP-003 | Functional PASS, final clean PASS | Correct minimal cross-file fix; diagnostic failing test polluted finish verdict |

Round 2 demonstrated that casual prompts can complete the user-visible task, but default model behavior still needed productization:

* structured validation was not always selected
* diagnostic pre-fix failures were recorded as unexpected ledger failures
* exploratory path misses were treated too strictly in final verdicts
* final user outcome and historical tool failures were not clearly separated

Key observation:

```text
The model can do the work; the product needs better default operating instructions and finish semantics for normal development workflows.
```

## Default Instruction v1 / v1.1

A general WebCodex coding-assistant instruction was added to guide normal user prompts.

Key rules:

* start normal coding/review tasks with `start_coding_task`
* inspect before editing
* prefer structured edits over shell or whole-file rewrites
* prefer structured validation:

  * `cargo_test` for `cargo test`
  * `cargo_check` for `cargo check`
  * `cargo_fmt` for `cargo fmt`
* use `run_shell` only as an escape hatch
* prefer minimal relevant validation before broad validation
* do not commit, push, tag, release, or publish unless explicitly requested
* mark known failing pre-fix reproduction tests as `expected_failure` when supported
* clearly separate final state from resolved historical failures

Action generic additions:

* use flattened top-level args
* do not call `start_session` before `start_coding_task`
* pass `timeout_secs: 120` to `cargo_*` tools
* do not pass `wait_timeout_secs` unless required by schema

## Round 2.5: Casual Regression After Default Instruction v1

After updating GPT default instructions, the same casual prompt style was rerun.

| Task   | Outcome | Notes                                                                                           |
| ------ | ------- | ----------------------------------------------------------------------------------------------- |
| CP-001 | PASS    | Structured `cargo_test`, no shell, final clean; minor exploratory path miss remained            |
| CP-002 | PASS    | Structured `cargo_test`, no shell, final clean                                                  |
| CP-003 | PASS    | Structured `cargo_test`, no shell, final clean; diagnostic pre-fix failure was clearly reported |

Round 2.5 showed material improvement:

* `run_shell` fallback was eliminated in the observed casual regression
* structured validation preference was followed
* final cleanup discipline remained stable
* cross-file semantic localization remained correct
* diagnostic failures were reported more clearly, even if not always marked as `expected_failure` at call time

## Current Conclusion

WebCodex is now validated for small controlled coding tasks under two modes:

1. strict acceptance prompts
2. casual user-style prompts with improved default GPT instructions

Supported evidence:

* small single-file bug fixes
* small numeric logic fixes
* small cross-file semantic localization
* structured validation
* clean workspace restoration
* final job-state reporting

Current limitation:

```text
The tested fixture is intentionally small. These results do not yet prove large-repository exploration, long-horizon planning, expensive validation selection, or complex multi-module refactors.
```

## Backlog

### Prompt / GPT instruction backlog

* Keep the WebCodex Agent Operating Contract in GPT default instructions.
* Continue emphasizing structured validation over `run_shell`.
* Continue requiring final summaries to include:

  * `finish_verdict`
  * `validation.status`
  * `workspace_clean`
  * `hygiene_clean`
  * `active_jobs`
  * `run_shell_used`
  * resolved historical failures

### Runtime / finish semantics backlog

The current ledger is intentionally strict, but real development includes normal intermediate failures.

Future `finish_coding_task` / handoff summaries should distinguish:

```text
final_state: pass/fail
latest_validation: passed/failed/unknown
workspace_clean: true/false
historical_failures:
  count: N
  resolved: true/false
  categories:
    - diagnostic_pre_fix_validation_failure
    - exploratory_path_miss
    - schema_argument_error
    - real_tool_failure
overall_user_outcome: pass/warn/fail
```

This would preserve strict auditability without confusing users when a diagnostic pre-fix failure is later resolved.

### Next dogfood stage

Do not jump directly to a large real repository. Create a medium-complexity fixture first.

The medium fixture should include:

* multiple crates/packages
* nested directories
* similar function names
* similar test names
* cross-module semantic bugs
* outdated docs
* a read-only audit task
* tasks requiring minimal validation selection

Goal:

```text
Validate whether casual user prompts still produce correct project exploration, minimal edits, structured validation, and clean finish behavior in a more realistic repository shape.
```
