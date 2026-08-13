# Release Readiness Checklist

This checklist is for final release readiness before tagging, publishing artifacts, updating client schemas, or deploying a new WebCodex server/agent/runtime build.

Do not create tags, push commits, publish npm packages, create GitHub Releases, rewrite history, deploy, or touch secrets while running this checklist unless the operator explicitly requests that action.

## 1. Source Validation

Run:

```bash
cargo fmt --check
cargo check --workspace --all-targets
cargo test --workspace -- --nocapture
git diff --check
git status --short --branch
```

For documentation-only release readiness work, the full test suite may be deferred, but the deferral must be reported.

## 2. Focused Runtime Tests

Run focused lanes when touching runtime metadata, schemas, OpenAPI, MCP, session, handoff, validation, or coding-task behavior:

```bash
cargo test -p webcodex --lib metadata -- --nocapture
cargo test -p webcodex --lib schema -- --nocapture
cargo test -p webcodex --lib openapi -- --nocapture
cargo test -p webcodex --lib mcp -- --nocapture
cargo test -p webcodex --lib validation -- --nocapture
cargo test -p webcodex --lib handoff -- --nocapture
cargo test -p webcodex --lib coding_task -- --nocapture
```

## 3. Product Documentation Check

Confirm the user-facing docs tell one story:

- README states the product position in the first screen.
- Quick Start has one recommended local-first path.
- Concepts explains server, agent, agent-registered projects, runtime project ids, ToolRuntime, MCP, GPT Actions, session, handoff, validation, review/hygiene, and `run_shell` as an escape hatch.
- Architecture starts with client/server/agent/codebase, security-boundary, and runtime-module diagrams before Rust module notes.
- MCP and GPT Actions both say they call the same WebCodex ToolRuntime.
- Security explains what the model can and cannot do, project access, agent trust boundary, shell/job risk, token handling, session/audit evidence, and revocation.
- The release PR / GitHub Release notes read like external release notes and include highlights, compatibility or breaking changes, known limitations, upgrade notes, and validation. Do not restore per-version release-note files as a second documentation source.
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

For every new binary and npm release, choose one candidate `<VERSION>` first and treat its tag and uploaded bytes as immutable once published:

- `Cargo.toml`, every local WebCodex workspace entry in `Cargo.lock`, `npm/webcodex/package.json`, `manifest.example.json`, and the npm self-tests must agree on `<VERSION>` before tagging.
- `npm/webcodex/manifest.json` is generated release metadata and is intentionally not tracked. Do not commit real checksums or create a post-tag checksum-only PR.
- Build the five published platforms (`linux-x64`, `linux-arm64`, `darwin-arm64`, `win32-x64`, `win32-arm64`) natively from the exact `v<VERSION>` tag through the reviewed release-build workflow. Do not rebuild an artifact on an intermediate packaging machine or substitute a cross-compiled artifact for native validation.
- For both `linux-x64` and `linux-arm64`, build in the native-architecture manylinux2014 userspace used by the release-build workflow. Before packaging, inspect all three ELF binaries with `readelf` and fail the release if any required `GLIBC_*` symbol version exceeds 2.17 or an unexpected host-specific `DT_NEEDED` dependency appears. The published Linux x64 and arm64 artifacts therefore share the glibc 2.17 floor.
- Pin one `WEBCODEX_BUILT_AT` value for the release. Every final `webcodex`, `webcodex-server`, and `webcodex-runner` binary must report `<VERSION>`, the same concrete tag commit, `dirty=false`, and the shared `built_at`.
- Windows packaging must use `scripts/package_release_artifact.ps1` in its default provenance-checked mode. `-AllowDevelopmentBuild` is for local/CI smoke only and its output must never be uploaded.
- The release control host is where the final archives are collected. Collect all five final archives there, then run `scripts/prepare_release_metadata.py` to validate archive contents and generate `manifest.json` plus `SHA256SUMS` from the exact bytes.
- Create the npm publication tree with `scripts/stage_npm_release.sh`. Its default mode requires a clean source worktree at exactly `v<VERSION>` and overlays the generated manifest without modifying Git. `--allow-development` is never valid for publication.
- Run `WEBCODEX_NPM_PACKAGE_DIR=<STAGE_DIR>/npm-package bash scripts/npm_package_smoke.sh` before npm publication. The smoke must validate the generated manifest, package contents, temporary installation, wrapper, and all three binaries.
- Upload `SHA256SUMS` with the native archives to the GitHub Release. Re-download uploaded assets to the release control host and verify their checksums before making the release public.
- If publishing a container image, build it from the exact immutable tag, verify the non-root runtime and health check, keep the Runner out of the server image, and record the immutable digest in the GitHub Release.

## 9. Release Sequence

1. Select a new `<VERSION>` that does not already exist as a Git tag, GitHub Release, or npm package version. Put version bumps, release notes, platform docs, packaging changes, and release tests in **one release-prep PR** and squash-merge it into `main`.
2. On the release control host, fast-forward to `origin/main`, require a clean worktree, run the source/release gates, and only after explicit operator authorization create the immutable annotated `v<VERSION>` tag. Never move that tag afterward.
3. Dispatch the reviewed release-build workflow for the exact tag and collect the five native artifacts (`linux-x64`, `linux-arm64`, `darwin-arm64`, `win32-x64`, `win32-arm64`). Verify version/build identity and native architecture for every target, enforce the `GLIBC_* <= 2.17` plus `DT_NEEDED` gates for both Linux targets, and transfer the exact candidate archives to the release control host.
4. On the release control host, run `scripts/prepare_release_metadata.py --version <VERSION> --artifact-dir <ARTIFACT_DIR> --output-dir <METADATA_DIR>`. Create a draft GitHub Release and upload the five archives plus `SHA256SUMS`; download them again and verify SHA-256 against the local exact bytes.
5. From a clean detached worktree at the immutable tag, run `scripts/stage_npm_release.sh --manifest <METADATA_DIR>/manifest.json --output-dir <STAGE_DIR>`, then run the npm package smoke against `<STAGE_DIR>/npm-package`.
6. Make the GitHub Release public and verify every generated manifest URL is reachable. From the release control host only, publish npm from the staged package (`npm publish --access public`) and verify the requested version/dist-tag in the registry.
7. Run public npm install acceptance on Linux x64, Windows x64, and native Windows arm64 when a suitable runner and network path are available. Then fast-forward any persistent release-control/build-host source checkouts used outside GitHub Actions to current `main` and clean temporary worktrees/bundles/pre-smoke artifacts.

## 10. Post-Deployment Acceptance Smoke

After deploying a new server, agent, or runtime build:

1. Refresh the GPT Action or MCP schema if runtime tool schemas changed.
2. Run compact `runtime_status`.
3. Run focused tool discovery.
4. Run `list_projects` and pick an agent-registered project marked appropriate for smoke when available.
5. Run a read-only coding task: `start_coding_task`, `read_file` or `search_project_text`, `show_changes(include_diff=false)`, `workspace_hygiene_check`, and `finish_coding_task(summary_only=true)`.
6. Run one small reversible edit task on a safe project and review the diff before accepting it.

Do not run production mutations as acceptance smoke.
