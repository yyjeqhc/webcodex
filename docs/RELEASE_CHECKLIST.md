# Release Readiness Checklist

This checklist is for release readiness and final acceptance before tagging,
publishing artifacts, updating GPT Actions/MCP schemas, or deploying a new
server/agent/runtime build. It is a procedure, not a release log.

Do not create tags, push commits, publish npm packages, create GitHub Releases,
rewrite history, or touch secrets while running this checklist.

## 1. Source Validation

Run:

```bash
cargo fmt --check
cargo check --all-targets
cargo test --bin webcodex -- --nocapture
git diff --check
git status --short --branch
```

The full `cargo test --bin webcodex -- --nocapture` suite is the final source
acceptance gate. For documentation-only readiness work, it may be deferred, but
the deferral must be reported.

## 2. Focused Runtime Tests

Run focused lanes when touching runtime metadata, schemas, OpenAPI, MCP, session,
handoff, validation, or coding-task docs:

```bash
cargo test --bin webcodex metadata -- --nocapture
cargo test --bin webcodex schema -- --nocapture
cargo test --bin webcodex openapi -- --nocapture
cargo test --bin webcodex mcp -- --nocapture
cargo test --bin webcodex validation -- --nocapture
cargo test --bin webcodex handoff -- --nocapture
cargo test --bin webcodex coding_task -- --nocapture
```

## 3. E2E Smoke

Run both supported zero-config transports against a safe local test project:

```bash
bash scripts/e2e_zero_config_ws.sh
E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh
```

These smokes must not target a production repository. Any write checks must stay
within disposable probe files or a temporary project.

## 4. Eval Harness

Run the coding-loop comparison:

```bash
EVAL_MODE=compare bash scripts/eval_coding_loop.sh
```

The eval harness measures scripted WebCodex tool-call mechanics. It is not a
full model-behavior evaluation.

## 5. Security And Leakage Checks

Run:

```bash
grep -R "python3 -c" -n src/tool_runtime src/bin src/shell_client || true
grep -R "run_agent_helper" -n . || true
```

Confirm:

- No secrets, `.env`, credentials, token files, or generated deployment env files
  were touched.
- Runtime paths did not regain `python3 -c` or `run_agent_helper`.
- `finish_coding_task.validation` and `session_handoff_summary.validation` do not
  expose raw stdout/stderr, excerpt fields, or `validation_output_summary`.
- The validation parser is described conservatively: stable facts only, no
  root-cause inference, no fix suggestions, no LSP/tree-sitter, and no LLM
  summarization.
- `run_shell` is not documented or classified as the default validation source.

## 6. Packaging And Docs Check

Confirm:

- `README.md`, GPT Actions, MCP, Concepts, Operations, and eval docs tell the
  same coding workflow story: `start_coding_task`, inspect, structured edits,
  structured validation, review, then `finish_coding_task` or
  `session_handoff_summary`.
- Session docs distinguish durable session ledger records from process-local
  current-session bindings.
- `start_session` is documented as creating a session record, not as automatically
  binding future calls.
- `session_handoff_summary` is documented as requiring an explicit `session_id`.
- OpenAPI operation count and the generic `callRuntimeTool` description are
  checked. Runtime-only tools must remain available through `callRuntimeTool`
  unless there is an explicit product decision and operation-count budget for a
  dedicated GPT Action.
- GPT Actions flattened fields for `callRuntimeTool` remain explicitly listed in
  `ToolCallRequest.properties`; do not loosen `additionalProperties`.
- npm package name, artifact names, service install instructions, and release
  artifact wording are verified if relevant to the release.

## 7. Post-Deployment Acceptance Smoke

After deploying a new server, agent, or runtime build:

1. Refresh the GPT Action or MCP schema when tool schemas changed.
2. Run `tool_manifest` or `list_tools` through the target integration.
3. Run `runtime_status`.
4. Verify `start_coding_task` and `finish_coding_task` are available through the
   generic runtime tool path.
5. Run read-only `show_changes` against a safe test project.
6. Run the E2E smoke and eval harness locally or in staging.

Do not run production mutations as acceptance smoke. Any write-path smoke must
use a safe test project or temporary project under an allowed root.
