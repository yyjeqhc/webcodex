# Runtime-produced CLI regression baseline

Baseline source: `f387d0a19f481bba0ead3bc2dd6b89581f074536` (#1296).
The fixture and measurements were run on mini; production Server/Runner processes were not changed.

## Evidence and scope

[The Runtime fixture](../../src/tool_runtime/tests/cli_runtime_projection.rs) uses the existing
local Runner responder and `cli_call_for_test`. Actual filesystem enumeration, the generated
search program, Shell commands and captured Job output pass through Runtime, MCP and the CLI
projection. A related group shares one in-memory Runtime and temporary Git checkout; no
standalone Server is started per sample.

The 17 snapshots cover live/flat/folded file enumeration, contextual/compound search, successful
and failed Shell commands, versioned reads, rejected unknown edit fences, exact read recovery,
read/log truncation, Job start/running/completion, partial unknown-Job failure, a real one-second
timeout, and transport completion loss after a real side effect. The lost-completion case
injects the failure at the Registry boundary. It does not fabricate a final ToolResult or
re-execute the command.

Successful native Runner edits are deliberately **not** simulated in this local responder.
[The isolated native smoke](../../scripts/e2e_cli_runtime_output.py) reuses the existing single
Server/Runner process harness for dry-run/hash invariance, actual edit success, stale rejection,
explicit execution configuration, scope rejection and final CLI output capture. Its results
must be reported separately from the local fixture. Python syntax was checked; execution of
that optional lane is not claimed by this baseline document.

## Reproduction

Install the existing POSIX test prerequisites (Git, ripgrep and Python 3), then run:

```bash
cargo test --locked -p webcodex --lib cli_ -- --nocapture
```

Snapshots are refreshed only with explicit `WEBCODEX_UPDATE_CLI_SNAPSHOTS=1`; rerun without
that variable to verify the checked-in corpus. See the
[snapshot contract](../../src/tool_runtime/tests/cli_runtime_snapshots/README.md) for bounded
normalization. Original recovery calls are parsed and their identity/range/fence arguments
checked before normalization; a read continuation is actually invoked unchanged.

Before the fix, the focused projection regression ran 13 tests: 11 passed and two failed on
duplicate recovery calls and a double directory separator. The first Runtime run reproduced
the standalone `null` in a real partial observation; it also exposed a missing Job handoff
driver in the new test fixture, which was corrected by running the actual command and applying
typed lifecycle updates. That fixture issue is not counted as a product defect.

After the fixes and fixture correction, all 36 `cli_` tests passed, including the four Runtime
groups and existing CLI routing, schema, permissions, provider, truncation and recovery tests.

## Measured output

These are **final CLI text** samples from the local Runtime adapter, not serialized canonical
JSON estimates. Characters are Unicode scalar values. Response JSON bytes are reported
separately. No ChatGPT, Claude Code, Codex Host token count or remote end-to-end latency was
measured.

| Real sample | Text UTF-8 bytes before | After | Characters before | After | Response JSON bytes before | After |
|---|---:|---:|---:|---:|---:|---:|
| Two folded tracked directories | 93 | 91 | 93 | 91 | 188 | 186 |
| Completed Job plus unknown Job | 1158 | 1153 | 1154 | 1149 | 1320 | 1314 |

The purpose of this first change is correctness and a trustworthy baseline. The small byte
reductions are not presented as a Host token or performance improvement.

Representative actual directory output changed from `apps// (1 files)` / `src// (1 files)`
to `apps/ (1 files)` / `src/ (1 files)`. File paths, counts and truncation/source evidence
remain intact. Partial failure no longer starts with `null`; the unknown Job identity,
`unknown_job`, error and complete `list_jobs` recovery call remain visible.

[The pre-fix corpus](cli-runtime-output-before.json) retains the measured classes with the
same documented identity/clock normalization as the regression snapshots. Its leading
`isError` line is snapshot metadata and is not included in model-visible text measurements.
Dynamic revision lengths can change raw byte counts between runs; compare normalized text
when attributing an improvement, and retain raw measurements as observations.

A separate synthetic edge regression proves that exactly equal `continuation` and
`suggested_call` objects appear once, including equal `follow_up_kind`. Different Job IDs,
parameters or follow-up semantics still appear independently. The Runtime samples validate
complete callable recovery parameters; they did not reproduce two identical full recovery
calls, so no real-Runtime byte saving is attributed to that defensive deduplication.

## Baselines for subsequent work

Unoptimized real samples still expose the next issues: search context 343 UTF-8 bytes,
compound search approximately 326–327 bytes (revision length varies), Job initiation 764 bytes,
simple successful Shell 61 bytes, failed Shell 297 bytes, timeout 954 bytes, and unknown outcome
787 bytes. The Job initiation snapshot contains both `status: queued` and
`execution_state: started`; timeout/ownership consistency belongs in the separate execution
contract change. Search/backend defaults already removed by the canonical model projection
must not be reimplemented in a parallel CLI contract.
