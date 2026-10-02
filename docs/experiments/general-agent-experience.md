# General Agent experience: command and file workflows

This iteration keeps ChatGPT's web client as the reasoning caller. WebCodex owns
the authorized tools and evidence; it does not add another model service or a
second Agent loop. Research and local acceptance were performed on 2026-10-01.

## Research and current implementation

| Primary source | Relevant practice | Application to WebCodex |
| --- | --- | --- |
| [Codex execution handler](https://github.com/openai/codex/blob/main/codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs) | Distinguish live execution identity, completion and bounded output. | Retain WebCodex's original Job rather than redispatching; distinguish readiness waiting from log observation. |
| [SWE-agent ACI](https://github.com/SWE-agent/SWE-agent/blob/main/docs/background/aci.md) | Design file viewers, search and command feedback for model decisions; check edited code. | Reuse bounded reads/search and guarded edits, and validate the task's actual effects with focused checks. |
| [Aider coder](https://github.com/Aider-AI/aider/blob/main/aider/coders/base_coder.py) | Feed edit/lint/test failures into bounded repair cycles. | Make diagnosis, fresh observation, repair and validation explicit to the web caller; do not automatically replay uncertain effects. |

The inspected upstream `78826899b58d1284906db92e1b4287956c5b3e69` already
contains [#812](https://github.com/yyjeqhc/webcodex/pull/812): MCP failure-log
deduplication, output metadata/closeout guards, Work Result retention and general
task guidance. It also already has revision-fenced edits, transactional preflight,
same-Job continuation and bounded readiness waits. These mechanisms were reused.

The remaining demonstrated gap was in default compact discovery: its bootstrap
still said "coding/review", and command entry descriptions omitted the
independent-work/readiness-wait choice. The fix updates the legacy and stateless
bootstrap, editor, process, shell and script selection descriptions. Every changed
description fits the existing 420-character bound with its final recovery clause
intact. Business input schemas, language choices, annotations, authorization and
execution semantics stay canonical. Effects on model decisions remain an
inference until real model runs are collected.

## Local acceptance and reproducibility

The latest upstream source was merged into fork `main` without conflicts. The
result `3e4fcda5cbc5e4f85b479cd789af6a16f8f15892` has the same source tree as
`78826899`; its merge preserves the fork's history. The unrelated local
`issue-784-continuity` commit remains on its own branch. Only synchronized `main`
was pushed; the new improvements remain on `codex/general-agent-experience`.

Use the same current harness source for both current-contract runs, with
`--profile upgraded` for both. The historical `baseline` profile belongs to the
pre-output-receipt contract, not to this comparison. Reports retain each actual
build identity, binary hashes and the common fixture-script fingerprint. Setup
and upgrade-only checks are separate from the six comparable scenarios.

The driver uses disposable loopback Server/Runner processes and temporary
Projects. Locale, timezone, Python environment and the fixture Git commit date
are fixed. It independently checks actual files/bytes/hashes and CSV statistics,
observes expected exit failures, verifies one original pending Job through its
captured call sequence, and checks stale rejection before fresh-read recovery.
Missing-output and large-output/Work Result checks run separately. The App client
is an explicit synthetic fixture; it does not prove real ChatGPT card rendering.

```sh
cargo test -p webcodex --lib mcp::tests::tools -j 2
cargo test -p webcodex --lib mcp::tests::execution_feedback -j 2
python3 -m unittest discover -s scripts/tests -p test_generic_agent_acceptance.py
python3 -m unittest discover -s scripts/tests -p test_agent_loop_report.py
cargo build --profile dogfood -p webcodex --bin webcodex-server -p webcodex-runner --bin webcodex-runner -j 2
python3 scripts/e2e_generic_agent_ws.py --bin-dir target/dogfood --profile upgraded --source-version YOUR_BUILD_COMMIT --report /tmp/webcodex-generic-upgraded.json
```

### Observed local results

Both binary pairs reported version 0.4.4 with `dirty=false`. The baseline build
commit was `3e4fcda5cbc5e4f85b479cd789af6a16f8f15892`; the improved build was
`5c49bcb8bde148e7bdac7515896a5098b845ac38`. Documentation added after these
builds is not part of either binary's runtime source. The shared harness SHA-256
was `4e80f56c02eb0159b3537ae07f486a0663853c120be77e0c34cfdec800b0c253`.

| Comparable evidence | Latest-upstream baseline | Improved |
| --- | ---: | ---: |
| Independent task scenarios passed | 6/6 | 6/6 |
| Separate missing/large output checks passed | 2/2 | 2/2 |
| Meaningful outer calls | 23 | 23 |
| Actual MCP response bytes | 43,011 | 43,011 |
| Actual request bytes | 9,418 | 9,418 |
| Expected unsuccessful tool calls | 3 | 3 |
| Comparable task span, seconds | 1.236869 | 1.262349 |

All six independently verified scenario evidence objects agree after excluding
the ephemeral Job identity. The three unsuccessful tool calls are the intentional
exit-7 command, broken-code assertion and stale edit. Both missing-output guards
also correctly returned a blocking outcome in their separate checks.

The scripted sequence uses the same calls irrespective of selection descriptions,
so these equal counts/bytes do not measure a model-selection improvement. The
single-run timing difference is descriptive and does not establish a speed
regression or improvement. The changed discovery behavior is covered separately
by 53 MCP tool tests, including compact/full business-schema parity and legacy/
stateless instruction routing; four execution-feedback tests and 79 Python tests
also passed. Final Rust formatting and diff whitespace checks passed. Both
production builds emitted the same pre-existing unused `Value` import warning in
`src/tool_runtime/trace_diagnostics.rs`; it was outside this change.

Raw local reports retain the binary hashes, actual call sizes/order and scenario
evidence. They are saved under ignored
`target/agent-experience-20261001-5c49bcb8/{baseline,improved}.json` in the feature
worktree; no credential headers or full request/result payloads are recorded.

## Real ChatGPT replay

The six `generic_*` entries in `scripts/agent_loop_cases.json` provide the task
prompts, fixture recipes, expected files and independent correctness gates. Use
the existing Runner Project registration workflow for an isolated prepared
directory. Prepare only the selected case's inputs from the named fixture method;
the deterministic driver is not a recorded model run.

For each comparison, use a fresh identical directory, exact fixture Git base,
same prompt, public model selection and caller surface, and record the runtime
build separately. Use Direct and Host Code Mode as the existing protocol
specifies; experimental Code Mode can only invoke admitted nested tools, and Job
operations remain outside it. The stale-edit case requires the fixture operator
to append its sentinel after the first model read and confirm the write before
the stale attempt. A missing-output case is an expected negative guard check.

Collect existing ActionAudit metrics with `scripts/agent_loop_report.py` and
record manually observed contract repairs using its bounded annotation. Expected
command/test/stale failures are ordinary business evidence, not contract repairs.
Do not infer private reasoning, model turns or token use from service gaps or
response bytes. The reporter's surface comparator is for Direct versus Code
Mode; same-surface runtime-version reports can be read side by side without
claiming that comparator's pair gate passed.

No real ChatGPT benchmark or existing-service deployment was performed in this
iteration. The local scripted run demonstrates runtime contracts and preservation
of effects. It cannot establish that the model understands every task better or
will always continue in another turn. Those conclusions require actual web-client
samples, including cached-tool discovery and observed task completion.
