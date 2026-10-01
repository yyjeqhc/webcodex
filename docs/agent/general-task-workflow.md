# General task workflow

WebCodex supports ordinary directories as Projects. The ChatGPT caller drives the existing Project, Workflow Session and Runner Job loop for file organization, data processing, output generation, environment diagnosis and coding. The current task remains caller-driven; these changes do not establish an unattended Agent or guarantee a later chat turn.

## Select tools and verify the actual effect

- Read text with `read_files`; reuse its `read_revision` as `expected_read_revision` for `edit_project_files`. Batch edits use the existing transactional preflight and stale-state protection.
- Copy binaries with `transfer_project_artifact`. Use `run_script` for directory organization, conversions and related batch operations. Keep conflict policy explicit and check preserved inputs and destination contents independently.
- Use structured `run_process` for literal argv and `run_shell` when shell syntax is necessary. Inspect a failed command, correct its cause and validate again. An unknown effect requires observation before another write.
- Retain the original pending Job. Do independent work first, then use one bounded readiness wait when dependencies block further work. Observe the same Job for details; never redispatch to recover its output.
- Check counts, paths, formats, statistics and hashes for file work. Use the relevant tests and Git review for code work. A script report or model claim is information to check, not completion evidence.

## Finish with observed outputs

`finish_coding_task` retains its existing name and optional projections. Its new optional `outputs` field accepts up to 16 unique non-sensitive project-relative regular files, with paths bounded to 512 UTF-8 bytes and files to 256 MiB each. Omission preserves the previous behavior.

For each declared path, the Server calls the existing canonical artifact metadata tool with the same authorization and exact Session. The Runner computes size, SHA-256 and MIME metadata; larger files use its existing bounded streaming hash implementation. Content-read segment limits are unchanged. An unknown MIME uses the existing artifact-export presentation fallback; that label does not validate the file format. Missing files and failed observations block closeout. Existing output files do not prove that their contents satisfy the task.

The closed `task_outputs` receipt contains observed metadata and its Unix timestamp. The latest finish receipt is retained in the Session ledger for Work Result; a later finish without valid outputs invalidates the earlier receipt. Card refresh does not rehash these files or imply they are still unchanged. Verified rows expose an explicit chat export request that asks the caller to check the saved SHA again before exporting. Failed rows have no export action.

## MCP feedback and deployed contracts

MCP failure messages omit exact duplicate stdout/stderr tail blocks while preserving their canonical output fields, diagnostics and recovery guidance. HTTP results and retained logs remain available. MCP `observe_jobs` defaults to `summary_only=true`, compacting proven successful validation logs; failures, unknown results and ordinary commands retain evidence. Explicit `summary_only=false` expands from the original observation cursor.

`tool_manifest` describes the deployed canonical contract and admitted routes; it does not install Host callables. If ChatGPT has cached older direct definitions, use an admitted gateway fallback where the manifest allows it. MCP App presentation must retain its direct route. New Work Result cards use resource v14; older resource URIs remain readable.

## Repeatable acceptance

Build Server and Runner with the `dogfood` profile, then run:

```sh
python3 scripts/e2e_generic_agent_ws.py --bin-dir target/dogfood --profile upgraded --source-version YOUR_COMMIT --report /tmp/webcodex-generic-upgraded.json
python3 -m unittest discover -s scripts/tests -p test_generic_agent_acceptance.py
```

Run `--profile baseline` with binaries compiled from the recorded baseline commit and the same harness source. The harness owns disposable loopback services and directories, then stops them. It covers mixed text/PNG/PDF/binary files with a destination conflict, inconsistent CSV encodings/columns, command failure and repair, a pending Job with independent work, and guarded code edits followed by failure/repair/test/review. Upgrade-only checks exercise missing outputs and real metadata-to-finish-to-ledger-to-WorkResult retention for a 10 MiB+1 file.

Reports distinguish comparable task calls/bytes/wall time from setup and extra checks, and record fixture fingerprints and independent evidence. They are deterministic tool/transport acceptance, not a ChatGPT model benchmark or a measure of model tokens, reasoning quality or future user interventions. Desktop/browser source regressions must be reported separately from real GUI tests on a capable Runner.
