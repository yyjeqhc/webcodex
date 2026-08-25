# Release Readiness Checklist

This checklist is for final release readiness before tagging, publishing artifacts, updating client schemas, or deploying a new WebCodex server/agent/runtime build.

It governs release/publish rollouts and deployment of published releases. An explicitly requested development/dogfood deployment of a reviewed commit is governed by [`AGENTS.md`](../AGENTS.md) and [Agent Release Process Notes](agent/release-process.md): it does not require the version/tag/publication/artifact steps below, but it still uses the focused post-deployment smoke in section 10 where applicable.

Do not create tags, push commits, publish npm packages, create GitHub Releases, rewrite history, deploy, or touch secrets while running this checklist unless the operator explicitly requests that action.

## 1. Source Validation

After the release-prep PR is squash-merged, select the exact `origin/main` commit that would be tagged. Final pre-tag validation is a reviewed GitHub Actions workflow, not a sequence of hand-run release-host commands:

```bash
python3 scripts/release_operator.py readiness-start \
  --source-sha <MAIN_COMMIT> \
  --state-file <STATE_FILE>

python3 scripts/release_operator.py readiness-status \
  --state-file <STATE_FILE> \
  --wait-secs 3600
```

`readiness-start` first requires GitHub `main` to equal `<MAIN_COMMIT>`, writes a mode-0600 operator-local correlation state before dispatch, and dispatches `.github/workflows/release-readiness.yml` from `main`. The workflow itself requires its `github.sha` to equal the requested source, checks out that exact commit with no persisted credential, and never tags, publishes, deploys, or uploads release candidate binaries. Its native release-profile builds are disposable pre-tag validation only.

If dispatch delivery becomes uncertain or the run is not resolved before the short start timeout, **do not create a second state file and do not redispatch**. Continue with `readiness-status` on the original state; its unique request id recovers the exact workflow run when present. Only terminal `success` (operator exit 0) satisfies the pre-tag gate. A nonterminal/unresolved status is not release approval.

The readiness workflow runs the canonical `scripts/release_check.sh`, the locked full workspace suite, frontend typecheck/tests/committed-build check, WebSocket and polling zero-config E2E, coding-loop compare eval, and parallel native release-profile gates for all five published platforms. Linux x64/arm64 use the same native manylinux2014 userspaces and ABI/dependency checks as formal release builds; macOS arm64 performs a release build/package plus the Runner suite; Windows x64/arm64 perform native release builds, PE architecture checks, packaging, and the local npm-install smoke. These pre-tag artifacts are disposable and are never uploaded or promoted. `release_check.sh` includes formatting, workspace all-target check, focused metadata/schema/OpenAPI/MCP tests, release-tooling self-tests, Markdown local-link validation, and static current-contract/leakage guards.

For focused diagnosis outside the final workflow, the underlying commands remain available, but a local pass does not replace the exact-source readiness run.

## 2. Focused Runtime Tests

During implementation/review, run focused lanes when touching runtime metadata, schemas, OpenAPI, MCP, session, handoff, validation, or coding-task behavior. The final release-readiness run does **not** repeat every focused lane after the full workspace suite: `release_check.sh` already records metadata/schema/OpenAPI/MCP evidence, and the locked full workspace suite covers validation/handoff/coding-task tests. Re-run an individual lane only to diagnose a failure or when a review explicitly requires separate evidence.

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

Run `python3 scripts/check_markdown_links.py` (also included in `release_check.sh`) and require zero missing repository-local links. Product narrative and allowed legacy-term matches remain review judgments in the release-prep PR; the readiness workflow automates only deterministic checks.

## 4. Legacy Surface Guard

Scan docs and scripts for stale onboarding guidance:

```bash
rg "run_codex|Codex delegation|retained runner|future explicit opt-in|WEBCODEX_ENABLE_LEGACY_CODEX_RUN|PROJECTS_CONFIG|server_static|/api/codex|api/codex|projects.toml" README.md README.zh-CN.md docs deploy scripts SECURITY.md
```

Allowed matches are negative statements, release-note breaking changes, guard tests, and deployment comments that explicitly say the legacy path is removed or not required.

Do not allow docs that ask users to configure server-side project onboarding, imply legacy routes exist, imply `run_codex` exists, or describe retained runner / future opt-in behavior as the current plan.

## 5. E2E Smoke

The exact-source release-readiness workflow builds dedicated debug `webcodex-server` / `webcodex-runner` test fixtures once for both supported zero-config transports and, in parallel, performs disposable native release-profile validation for all five published platforms. None of those binaries or archives are uploaded or treated as release candidates; `.github/workflows/release-build.yml` remains the only formal candidate producer after the immutable tag exists.

The underlying smoke commands remain:

```bash
bash scripts/e2e_zero_config_ws.sh
E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh
```

These commands remain useful for diagnosis, but do not rerun them manually after a successful readiness workflow merely to duplicate evidence. They must never target a production repository; any write checks stay within disposable probe files or a temporary project.

## 6. Eval Harness

The exact-source release-readiness workflow runs `EVAL_MODE=compare bash scripts/eval_coding_loop.sh`. Run it separately only for focused diagnosis. The eval harness measures scripted WebCodex tool-call mechanics; it is not a full model-behavior evaluation.

## 7. Security And Leakage Checks

Confirm:

- No secrets, `.env`, credentials, token files, generated deployment env files, or Authorization headers were touched or printed.
- `finish_coding_task` and `session_handoff_summary` compact outputs do not expose raw stdout/stderr bodies, command text, tails, excerpts, env values, tokens, or secrets.
- `run_shell` is documented as a bounded escape hatch, not the default validation source.
- Model-facing runtime docs keep admin, account, pairing, token-management, and agent-token management outside MCP and GPT Actions.

## 8. Packaging And Artifact Checks

For every new binary and npm release, choose one candidate `<VERSION>` first and treat its tag and uploaded bytes as immutable once published:

- If `<VERSION>` already has a tag only because an earlier pre-publication attempt failed, reclaim it **before preflight** only through `python3 scripts/release_operator.py reclaim-tag --version <VERSION> --confirm v<VERSION> --root <EXACT_MAIN_WORKTREE>`. The command requires clean exact remote `main`, matching Cargo/npm versions, an annotated tag, no GitHub Release, no npm version, no active matching release-build, and no successful authoritative release-build. Keep failed historical runs as evidence. The normal Release check is authenticated; `--allow-public-release-check` is permitted only after the human operator separately confirms there is no draft Release.
- Before pre-tag readiness, run `python3 scripts/release_operator.py preflight --version <VERSION> --source-sha <MAIN_COMMIT> --root <EXACT_MAIN_WORKTREE>`. It requires a clean exact source, matching Cargo/npm versions, GitHub `main` at that source, an unused Git tag/GitHub Release/npm version, and usable GitHub/npm publication identities without printing credentials.
- `Cargo.toml`, every local WebCodex workspace entry in `Cargo.lock`, `npm/webcodex/package.json`, `manifest.example.json`, and the npm self-tests must agree on `<VERSION>` before tagging.
- `npm/webcodex/manifest.json` is generated release metadata and is intentionally not tracked. Do not commit real checksums or create a post-tag checksum-only PR.
- Build the five published platforms (`linux-x64`, `linux-arm64`, `darwin-arm64`, `win32-x64`, `win32-arm64`) natively from the exact `v<VERSION>` tag through the reviewed release-build workflow. Do not rebuild an artifact on an intermediate packaging machine or substitute a cross-compiled artifact for native validation.
- Dispatch `release-build.yml` through `release_operator.py build-start` with a fresh mode-0600 state file, then observe that same state with `build-status`. The operator requires GitHub `main` and the remote annotated tag to resolve to the exact release source, injects one durable `rb_<24 hex>` request id into the workflow run name, and never blind-redispatches after an uncertain POST outcome. Only terminal success for the bound run/source satisfies the build gate.
- For both `linux-x64` and `linux-arm64`, build in the native-architecture manylinux2014 userspace used by the release-build workflow. Before packaging, inspect all three ELF binaries with `readelf` and fail the release if any required `GLIBC_*` symbol version exceeds 2.17 or an unexpected host-specific `DT_NEEDED` dependency appears. The published Linux x64 and arm64 artifacts therefore share the glibc 2.17 floor.
- Pin one `WEBCODEX_BUILT_AT` value for the release. Every final `webcodex`, `webcodex-server`, and `webcodex-runner` binary must report `<VERSION>`, the same concrete tag commit, `dirty=false`, and the shared `built_at`.
- Windows packaging must use `scripts/package_release_artifact.ps1` in its default provenance-checked mode. `-AllowDevelopmentBuild` is for local/CI smoke only and its output must never be uploaded.
- After all five native jobs succeed, the release-build workflow must aggregate only artifacts from that same workflow run, verify every per-target SHA sidecar, and upload one `<ARCHIVE_STEM>-bundle`. For a real `v<VERSION>` release the bundle contains the five unchanged native archives, `manifest.json`, `SHA256SUMS`, both Linux ELF reports, and `release-build.json`; the canonical `scripts/prepare_release_metadata.py` still generates the publish metadata, but it now runs inside this same-run assemble job.
- The release control host collects that single assembled bundle instead of fetching five target artifacts independently. Use `python3 scripts/release_operator.py collect --run-id <RUN_ID> --source-sha <TAG_COMMIT> --tag v<VERSION> --output-dir <BUNDLE_DIR>` as the canonical path. `collect` resolves exactly one `*-bundle` artifact from the locked successful run, validates its run/source/expiry/SHA-256 metadata, downloads it by artifact id through the GitHub REST API (not `gh run download`), verifies the artifact zip digest, extracts it with bounded path/type/size checks, and validates `release-build.json`, `SHA256SUMS`, archive membership, and release manifest consistency before atomically exposing `<BUNDLE_DIR>`. It reuses `GH_TOKEN`/`GITHUB_TOKEN` or the current `gh auth` login without printing credentials. Never substitute an archive from another workflow run.
- Create and validate the npm publication tree with `python3 scripts/release_operator.py stage-npm --bundle-dir <BUNDLE_DIR> --source-root <EXACT_TAG_WORKTREE> --output-dir <STAGE_DIR>`. It revalidates the retained bundle, requires a clean worktree at the immutable tag, extracts only the exact verified `linux-x64` binaries into a bounded temporary directory, then runs the existing staging helper and `npm_package_smoke.sh --binary-dir`. The release path never invokes Cargo and compares installed native files byte-for-byte with the retained CI candidates.
- Create a draft GitHub Release and upload exactly the five native archives plus `SHA256SUMS`. Before making it public, run `python3 scripts/release_operator.py verify-draft --bundle-dir <BUNDLE_DIR>`; require the draft asset set, sizes, states, and GitHub-provided `sha256:` digests to match the retained bundle. Do not re-download the same ~100 MiB of draft assets on the control host. The post-public `verify_public_release.py` run is the single full public-byte download/verification pass.
- Server container publication is owned by `.github/workflows/release-image.yml`, separate from the read-only `release-build.yml` candidate workflow. It runs only after a GitHub Release is public (or by guarded manual backfill of that already-public immutable tag), builds native `linux/amd64` and `linux/arm64` images from the exact annotated tag with the release build identity, verifies the non-root runtime, exact Server/CLI identity, health check, and absence of the Runner, then publishes immutable `v<VERSION>` / `<VERSION>` image tags (with SemVer `+` build metadata encoded as `_`, since Docker tags do not allow `+`) and records the multi-arch digest plus child digests in the Release asset `webcodex-server-image.json`. Only the current latest stable GitHub Release may move the mutable `latest` tag. Reruns must reconcile an existing immutable image instead of overwriting it.
- GitHub creates the first GHCR package as private by default. On the first WebCodex Server image publication only, the repository owner must change `webcodex-server` package visibility to **Public** in GitHub Packages, then rerun the failed anonymous-availability gate. After activation, require anonymous inspection/pull availability for every release; do not distribute registry credentials to end users.

## 9. Release Sequence

1. Put version bumps, release notes, platform docs, packaging changes, and release tests in **one release-prep PR** and squash-merge it into `main`. Resolve the exact merged commit. If an explicitly approved failed pre-publication tag for `<VERSION>` exists, run the guarded `release_operator.py reclaim-tag` now and require successful remote-ref reconciliation. Then run `release_operator.py preflight --version <VERSION> --source-sha <MAIN_COMMIT> --root <EXACT_MAIN_WORKTREE>`; require all three publication namespaces (Git tag, GitHub Release, npm version) to be unused and both publication identities to be available.
2. Run `release_operator.py readiness-start` with a fresh durable state file, then `readiness-status --wait-secs 3600` on that same state. Require terminal success for the exact source, including all five native release-profile pre-tag gates. If dispatch/observation is uncertain, recover from the same state instead of blind redispatch.
3. After explicit human authorization, create and push the immutable annotated `v<VERSION>` tag at that exact source; never move it. Then run `release_operator.py build-start --source-sha <TAG_COMMIT> --tag v<VERSION> --state-file <BUILD_STATE>` once, and `build-status --state-file <BUILD_STATE> --wait-secs 7200` on the same state until terminal. Require all five native targets plus `assemble` to succeed in that one bound workflow run.
4. Collect only that run's `<ARCHIVE_STEM>-bundle` with `scripts/release_operator.py collect`, passing the bound run id, exact tag commit, and tag. The collector must finish successfully before publication; its output directory is the retained candidate bundle.
5. From a clean detached worktree at the immutable tag, run `scripts/release_operator.py stage-npm --bundle-dir <BUNDLE_DIR> --source-root <TAG_WORKTREE> --output-dir <STAGE_DIR>`. This performs exact-tag staging and existing-binary npm smoke without recompilation.
6. Create a **draft** GitHub Release and upload the five retained archives plus `SHA256SUMS`. Run `scripts/release_operator.py verify-draft --bundle-dir <BUNDLE_DIR>` and require all six GitHub asset digests/sizes/states to match the retained bytes. This digest gate replaces the former full draft re-download on the publication host.
7. After explicit human authorization, make the verified GitHub Release public. From the release control host only, publish npm from `<STAGE_DIR>/npm-package` (`npm publish --access public --registry https://registry.npmjs.org/`) and verify the requested version/dist-tag; `package.json` also pins the same public registry through `publishConfig`. If publish success is followed by delayed registry visibility, poll rather than publishing twice.
8. Require `.github/workflows/release-image.yml` to finish successfully for the same public `v<VERSION>` Release. It must publish/reconcile `ghcr.io/yyjeqhc/webcodex-server:v<VERSION>` for both Linux architectures, attach `webcodex-server-image.json`, and pass its anonymous GHCR availability gate. For the first-ever package only, make the package Public once and rerun that failed gate; do not rebuild or retag the immutable image merely to change visibility.
9. From one trusted, well-connected Linux verification host, run `python3 scripts/verify_public_release.py <VERSION>`. This remains the single full public-byte acceptance pass for npm plus the five native GitHub archives; the container workflow independently proves the public GHCR digest/platform contract. The verifier cross-checks npm manifest / `SHA256SUMS` / GitHub asset digests, validates archive membership and foreign binary architecture without executing those binaries, and rechecks Linux GLIBC/`DT_NEEDED` metadata. Native per-OS public installs are optional targeted diagnostics, not a routine release fan-out. Then clean disposable publication worktrees and prefixes while retaining the versioned release audit bundle and durable readiness/build state files.

## 10. Post-Deployment Acceptance Smoke

After deploying a new server, agent, or runtime build:

1. Refresh the GPT Action or MCP schema if runtime tool schemas changed.
2. Run compact `runtime_status`.
3. Run focused tool discovery.
4. Run `list_projects` and pick an agent-registered project marked appropriate for smoke when available.
5. Run a read-only coding task: `work_on_project`, `read_file` or `search_project_text`, `show_changes(include_diff=false)`, `workspace_hygiene_check`, and `finish_coding_task(summary_only=true)`.
6. Run one small reversible edit task on a safe project and review the diff before accepting it.

Do not run production mutations as acceptance smoke.
