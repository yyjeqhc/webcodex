# WebCodex Dogfood Round 3 Medium Casual Summary

This document records the Round 3 medium-complexity dogfood results for WebCodex as an online GPT coding-agent runtime.

Round 3 followed Round 1 strict acceptance prompts and Round 2 casual prompt regression. Its goal was to test whether improved default GPT operating instructions could support medium-complexity project exploration and task execution without long, developer-authored task protocols.

## Fixture

The Round 3 fixture repository was:

```text
agent:oe:webcodex-medium-fixtures
/root/git/webcodex-medium-fixtures
```

Baseline commit:

```text
06de7be init medium dogfood fixtures
```

The fixture contains:

* a Rust workspace
* multiple crates
* nested source and test directories
* similar helper names
* cross-crate call paths
* documentation-only tasks
* read-only audit tasks
* intentional behavioral bugs

The fixture intentionally remains small enough to audit, but large enough to test more realistic project navigation than the first `webcodex-dogfood-fixtures` repository.

## Round 3 Goals

Round 3 tested whether a WebCodex-enabled GPT could handle casual user prompts such as:

```text
项目是 agent:oe:webcodex-medium-fixtures。

inventory-api 里按用户输入 SKU 查找 item 好像有问题，你帮我定位并修一下。
验证通过后不要提交，最后恢复 clean。
```

The user prompts did not specify detailed tool paths. The model was expected to infer the normal WebCodex workflow:

```text
start_coding_task
-> inspect with list/search/read
-> make minimal structured edits
-> use structured validation
-> review changes
-> restore clean when requested
-> report final state and active jobs
```

## Summary Results

| Task   | Type                                       | Result                         | Notes                                                                                                                                                      |
| ------ | ------------------------------------------ | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| MC-001 | Single-crate helper fix                    | PASS                           | Correctly fixed `inventory_core::canonical_sku`; used structured validation; no shell; final clean                                                         |
| MC-002 | Cross-crate API lookup fix                 | FUNCTIONAL PASS / FIXTURE WARN | Correctly recognized `inventory-api` should use `inventory_core::canonical_sku`; discovered MF-002 depends on MF-001 helper behavior from a clean baseline |
| MC-003 | Indirect cross-crate report formatting fix | FUNCTIONAL PASS                | Correctly traced `cli-shell::public_stock_report` to `report_kit::render_stock_line`; minimal edit; structured validation; final clean                     |
| MC-004 | Docs-only flag consistency fix             | PASS                           | Updated `--workspace` to `--project`; used search/diff validation; did not run cargo; no shell; final clean                                                |
| MC-005 | Read-only helper audit                     | PASS                           | Correctly chose `inventory_core::canonical_sku` for user-entered SKU lookup; no edits; no shell; no cargo; final clean                                     |

## MC-001: inventory-core canonical SKU

Prompt shape:

```text
项目是 agent:oe:webcodex-medium-fixtures。

inventory-core 里的 SKU 规范化好像不对。你帮我看一下并修一下。
验证通过后不要提交，最后恢复 clean。
```

Observed result:

* Temporary fix:

```rust
pub fn canonical_sku(input: &str) -> String {
    input.trim().to_ascii_uppercase()
}
```

* Validation:

  * `cargo test -p inventory-core`
  * `cargo fmt -- --check`
* `run_shell`: not used
* structured validation: used
* final workspace: clean
* hygiene: clean
* active jobs: 0
* historical failures: none
* finish result: pass

Assessment:

```text
MC-001 PASS
```

The agent correctly located the relevant helper and avoided unrelated normalization helpers.

Minor note:

```text
The agent ran package-level validation rather than the narrow target test.
This was acceptable for the small fixture package, but large repositories should prefer targeted validation first.
```

## MC-002: inventory-api SKU lookup

Prompt shape:

```text
项目是 agent:oe:webcodex-medium-fixtures。

inventory-api 里按用户输入 SKU 查找 item 好像有问题，你帮我定位并修一下。
验证通过后不要提交，最后恢复 clean。
```

Observed fix:

```rust
use inventory_core::{canonical_sku, count_items_in_warehouse, total_quantity, Item};

pub fn find_item_by_sku<'a>(items: &'a [Item], user_input: &str) -> Option<&'a Item> {
    let sku = canonical_sku(user_input);
    items.iter().find(|item| item.sku == sku)
}
```

The agent also found that the clean baseline still has MF-001 unresolved:

```rust
pub fn canonical_sku(input: &str) -> String {
    input.to_string()
}
```

Therefore the targeted `inventory-api` test could not pass unless `canonical_sku` was also temporarily fixed.

Validation:

* `cargo test -p inventory-api find_item_by_sku_accepts_human_input`
* `cargo test -p inventory-core canonical_sku_trims_and_uppercases`
* structured `cargo_test`
* no `run_shell`
* final workspace: clean
* hygiene: clean
* active jobs: 0
* finish result: `fail / validation_mixed` because diagnostic and partial-fix validation failures were recorded before the final successful validations

Assessment:

```text
MC-002 FUNCTIONAL PASS / FIXTURE WARN
```

The agent performed good cross-crate reasoning. It did not incorrectly use `normalize_command_name`, `normalize_search_query`, or a local ad hoc normalization function.

However, the fixture task has a dependency:

```text
MF-002 depends on MF-001 canonical_sku behavior when tasks are run from a clean baseline.
```

This is not necessarily a defect. It is realistic: API-level symptoms often reveal a core-layer contract bug. The fixture documentation should explicitly state this dependency if MC-002 is intended as a combined cross-crate task.

## MC-003: cli-shell public stock report

Prompt shape:

```text
项目是 agent:oe:webcodex-medium-fixtures。

cli-shell 里的 public_stock_report("west", &items) 输出不对，你帮我定位并修一下。
验证通过后不要提交，最后恢复 clean。
```

Observed fix:

```rust
pub fn render_stock_line(summary: &StockSummary) -> String {
    format!("Warehouse {}: {} items", summary.warehouse, summary.total_quantity)
}
```

The user-visible failing behavior was in `cli-shell::public_stock_report`, but the correct fix point was `report_kit::render_stock_line`.

Validation:

* pre-fix reproduction:

  * `cargo test -p cli-shell public_stock_report_uses_public_label`
* post-fix:

  * `cargo test -p cli-shell public_stock_report_uses_public_label`
  * `cargo test -p report-kit render_stock_line_public_format`
  * `cargo fmt -- --check`
* `run_shell`: not used
* structured validation: used
* final workspace: clean
* hygiene: clean
* active jobs: 0

Assessment:

```text
MC-003 FUNCTIONAL PASS
```

The agent correctly followed the call path:

```text
cli_shell::public_stock_report
-> inventory_api::summarize_warehouse
-> report_kit::render_stock_line
```

It did not over-modify `cli-shell` or `inventory-api`.

Known issue:

```text
The agent attempted to mark a pre-fix failing test as expected, but used expected_failure_kind=test_failed while the runtime classified it as runtime_error.
This created an expectation mismatch even though the later validation passed.
```

This suggests that agents should usually pass only `expected_failure=true` for diagnostic pre-fix reproduction unless the runtime failure kind is known.

## MC-004: docs-only outdated flag

Prompt shape:

```text
项目是 agent:oe:webcodex-medium-fixtures。

docs 里好像还有旧的 --workspace 参数，现在应该改成 --project。
你帮我找一下并修正。验证通过后不要提交，最后恢复 clean。
```

Observed edit:

```text
docs/usage.md
```

Temporary change:

```text
webcodex task start --workspace demo
```

to:

```text
webcodex task start --project demo
```

Validation:

* searched docs for `--workspace`
* searched docs for `--project`
* reviewed diff
* restored file after validation
* did not run cargo
* did not use `run_shell`

Final state:

* workspace clean
* hygiene clean
* active jobs 0
* failed tool calls 0
* validation.status: `not_run`

Assessment:

```text
MC-004 PASS
```

`validation.status=not_run` is expected for docs-only work when validation is performed through search and diff review rather than cargo/test tooling.

Future finish summaries should represent this as:

```text
task_type: docs_only
non_cargo_validation: passed
```

instead of treating `not_run` as a generic warning.

## MC-005: read-only normalization helper audit

Prompt shape:

```text
项目是 agent:oe:webcodex-medium-fixtures。

这个仓库里有好几个 normalize / canonical 相关的 helper。
你帮我只读审计一下：如果是用户输入 SKU 查找 item，应该用哪个 helper？为什么不应该用其他几个？
不要修改文件。
```

Files read:

* `MEDIUM_TASKS.md`
* `docs/audit-notes.md`
* `crates/inventory-core/src/lib.rs`
* `crates/inventory-core/tests/behavior.rs`
* `crates/inventory-api/src/lib.rs`
* `crates/inventory-api/tests/behavior.rs`
* `crates/report-kit/src/lib.rs`

Searches:

* `normalize`
* `canonical`
* `find_item_by_sku`

Conclusion:

```text
User-entered SKU lookup should use inventory_core::canonical_sku.
```

Reasoning:

* `Item::new` stores item SKU values through `canonical_sku`.
* `find_item_by_sku` compares user input against stored item SKU values.
* Therefore user input should be normalized using the same canonical SKU helper before comparison.

Other helpers were correctly rejected:

* `normalize_search_query`: search query semantics; lower-case output
* `normalize_report_label`: report label semantics; replaces spaces with `-`
* `normalize_command_name`: CLI command semantics; replaces underscores and lowercases
* direct raw comparison: fails for human input such as `"  ab-12  "`

Final state:

* no file modifications
* no `run_shell`
* no cargo
* workspace clean
* hygiene clean
* active jobs 0
* validation.status: `not_run`
* finish verdict: warn

Assessment:

```text
MC-005 PASS
```

The warning is acceptable because code validation was not applicable to a read-only audit.

One transient `read_file` timeout was retried successfully. This should be classified as:

```text
transient_read_timeout_resolved
```

not as a task failure.

## Round 3 Findings

### 1. Medium casual prompts are viable

Round 3 shows that casual prompts with default WebCodex operating instructions can support:

* single-crate logic fixes
* cross-crate API fixes
* indirect call-chain localization
* docs-only edits
* read-only semantic audits

### 2. Structured validation preference improved

Across MC-001 through MC-005:

* `run_shell` was not used
* structured `cargo_test` / `cargo_fmt` were used for code tasks
* cargo was not run for docs-only and read-only audit tasks

This suggests the WebCodex Agent Operating Contract v1.1 materially improved default tool selection.

### 3. Finish and validation semantics remain too code-test centric

Docs-only and read-only audit tasks naturally have no cargo/test validation.

Current summaries may report:

```text
validation.status: not_run
finish_verdict: warn
```

even when the task is successfully completed through read/search/diff review.

Future finish summaries should distinguish:

```text
task_type: code_edit | docs_edit | read_only_audit
code_validation: passed | failed | not_applicable
non_cargo_validation: passed | failed | not_applicable
workspace_clean: true | false
historical_failures_resolved: true | false
```

### 4. Historical diagnostic failures still pollute validation status

Several tasks intentionally ran failing tests before fixing the bug. This is normal development behavior.

The final state can be correct while the ledger reports:

```text
validation.status: mixed
```

A better user-facing finish model would separate:

```text
latest_validation: passed
historical_failures:
  count: N
  resolved: true
overall_user_outcome: pass
```

### 5. expected_failure_kind is hard to predict

MC-003 showed that a diagnostic failing cargo test was expected by the model, but the runtime classified it differently than the model predicted.

Recommendation:

```text
When reproducing a known bug, agents should pass expected_failure=true without expected_failure_kind unless the runtime failure kind is already known.
```

Runtime-side improvement:

```text
cargo_test failures should expose a stable validation-oriented failure kind such as validation_failed or test_failed.
```

### 6. MF-002 depends on MF-001 from a clean baseline

MC-002 revealed that `inventory-api` SKU lookup cannot pass from the clean baseline by only changing `inventory-api`, because `inventory_core::canonical_sku` itself is intentionally broken.

This can be preserved as a realistic cross-layer task, but the fixture docs should say so explicitly.

## Recommended Backlog

### Documentation / prompt backlog

* Keep WebCodex Agent Operating Contract v1.1 as default GPT instruction.
* Add a rule: for expected diagnostic validation failures, prefer `expected_failure=true` without `expected_failure_kind` unless known.
* Keep final summaries reporting:

  * what changed
  * how validation was performed
  * whether `run_shell` was used
  * finish verdict
  * validation status
  * workspace clean
  * hygiene clean
  * active jobs
  * resolved historical failures

### Runtime / finish backlog

* Add task-type aware finish semantics:

  * code edit
  * docs edit
  * read-only audit
* Add non-cargo validation reporting for search/diff/read-only validation.
* Separate final state from historical failures.
* Add stable cargo validation failure kinds.
* Consider classifying transient read timeouts separately from task-level failures.

### Fixture backlog

* Update `MEDIUM_TASKS.md` to clarify that MF-002 may require checking and temporarily fixing `inventory_core::canonical_sku` when running from a clean baseline.
* Optionally add a second independent API lookup task later if an isolated cross-crate call-site-only fix is needed.

## Current Conclusion

Round 3 provides stronger evidence than the small fixture rounds:

```text
WebCodex can support medium-complexity casual coding-agent workflows where the model must navigate multiple crates, disambiguate similar helpers, choose minimal edits, use structured validation, avoid unnecessary shell usage, and restore clean state.
```

The main remaining work is not basic coding-loop capability. It is product polish:

* better finish semantics
* clearer validation classification
* stronger handling of diagnostic failures
* more realistic large-repository exploration tests
