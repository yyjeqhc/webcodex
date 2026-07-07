# Release Readiness Checklist

This checklist is for final release readiness before tagging, publishing artifacts, updating client schemas, or deploying a new WebCodex server/agent/runtime build.

Do not create tags, push commits, publish npm packages, create GitHub Releases, rewrite history, deploy, or touch secrets while running this checklist unless the operator explicitly requests that action.

## 1. Source Validation

Run:

```bash
cargo fmt --check
cargo check --all-targets
cargo test --bin webcodex -- --nocapture
git diff --check
git status --short --branch
```

For documentation-only release readiness work, the full test suite may be deferred, but the deferral must be reported.

## 2. Focused Runtime Tests

Run focused lanes when touching runtime metadata, schemas, OpenAPI, MCP, session, handoff, validation, or coding-task behavior:

```bash
cargo test --bin webcodex metadata -- --nocapture
cargo test --bin webcodex schema -- --nocapture
cargo test --bin webcodex openapi -- --nocapture
cargo test --bin webcodex mcp -- --nocapture
cargo test --bin webcodex validation -- --nocapture
cargo test --bin webcodex handoff -- --nocapture
cargo test --bin webcodex coding_task -- --nocapture
```

## 3. Product Documentation Check

Confirm the user-facing docs tell one story:

- README states the product position in the first screen.
- Quick Start has one recommended local-first path.
- Concepts explains server, agent, agent-registered projects, runtime project ids, ToolRuntime, MCP, GPT Actions, session, handoff, validation, review/hygiene, and `run_shell` as an escape hatch.
- Architecture starts with client/server/agent/codebase, security-boundary, and runtime-module diagrams before Rust module notes.
- MCP and GPT Actions both say they call the same WebCodex ToolRuntime.
- Security explains what the model can and cannot do, project access, agent trust boundary, shell/job risk, token handling, session/audit evidence, and revocation.
- Release Notes read like external release notes and include highlights, breaking changes, known limitations, upgrade notes, validation, and next steps.
- Roadmap stays short and does not promise a full IDE replacement, autonomous ops, arbitrary computer use, or universal client compatibility.

Run a markdown local link check and report markdown file count, local link count, and missing local link count.

## 4. Legacy Surface Guard

Scan docs and scripts for stale onboarding guidance:

```bash
rg "run_codex|Codex delegation|retained runner|future explicit opt-in|WEBCODEX_ENABLE_LEGACY_CODEX_RUN|PROJECTS_CONFIG|server_static|/api/codex|api/codex|projects.toml" README.md README.zh-CN.md docs deploy scripts SECURITY.md
```

Allowed matches are negative statements, release-note breaking changes, guard tests, and deployment comments that explicitly say the legacy path is removed or not required.

Do not allow docs that ask users to configure server-side project onboarding, imply legacy routes exist, imply `run_codex` exists, or describe retained runner / future opt-in behavior as the current plan.

## 5. E2E Smoke

Run both supported zero-config transports against a safe local test project:

```bash
bash scripts/e2e_zero_config_ws.sh
E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh
```

These smokes must not target a production repository. Any write checks must stay within disposable probe files or a temporary project.

## 6. Eval Harness

Run the coding-loop comparison:

```bash
EVAL_MODE=compare bash scripts/eval_coding_loop.sh
```

The eval harness measures scripted WebCodex tool-call mechanics. It is not a full model-behavior evaluation.

## 7. Security And Leakage Checks

Confirm:

- No secrets, `.env`, credentials, token files, generated deployment env files, or Authorization headers were touched or printed.
- `finish_coding_task` and `session_handoff_summary` compact outputs do not expose raw stdout/stderr bodies, command text, tails, excerpts, env values, tokens, or secrets.
- `run_shell` is documented as a bounded escape hatch, not the default validation source.
- Model-facing runtime docs keep admin, account, pairing, token-management, and agent-token management outside MCP and GPT Actions.

## 8. Packaging And Artifact Checks

For the planned v0.2.0 binary release:

- Rust package metadata should be `0.2.0`.
- Artifact names and platform coverage should match the release notes.
- Release artifact smoke should use `webcodex --version`, `webcodex-cli --version`, and `webcodex-agent --version`.
- Confirm whether any npm wrapper installs 0.2.0 before documenting npm as a 0.2.0 install path.

## 9. Post-Deployment Acceptance Smoke

After deploying a new server, agent, or runtime build:

1. Refresh the GPT Action or MCP schema if runtime tool schemas changed.
2. Run compact `runtime_status`.
3. Run focused tool discovery.
4. Run `list_projects` and pick an agent-registered project marked appropriate for smoke when available.
5. Run a read-only coding task: `start_coding_task`, `read_file` or `search_project_text`, `show_changes(include_diff=false)`, `workspace_hygiene_check`, and `finish_coding_task(summary_only=true)`.
6. Run one small reversible edit task on a safe project and review the diff before accepting it.

Do not run production mutations as acceptance smoke.
