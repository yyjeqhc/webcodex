# WebPi first-release final review

Date: 2026-09-26
Baseline HEAD: `63640bb8e9b11f6996b13d5a99812ba301eddbcd`
Working branch: `webpi`

## Validation result

The tested working tree is functionally green across the first-release matrix:

- Rust all-targets: 2748 passed, 1 ignored.
- WebPi Python: 111/111.
- Frontend: 178/178.
- Pi bridge: 38/38.
- Active brand/docs contract: 8/8.
- Markdown links: 528 checked, 0 missing.
- WSL zero-config WebSocket: 106/106.
- WSL zero-config polling: 106/106.
- Reconnect: 33/33.
- Job reconciliation: 70/70.
- Job recovery failures: 57/57.
- Retired shared-key product boundary: fail-closed smoke PASS.
- Coding-loop compare eval: 6/6; guided path 18 model-visible calls vs baseline 21, with 0 raw shell and complete handoff/cleanup.

Disposable Docker image validation did not enter the product build because Docker Hub OAuth returned EOF and required base images were not cached. Remote release-readiness CI must supply this proof.

## Verified goal fixes

- Windows `git_log` dispatch uses the Runner internal POSIX execution path instead of sending POSIX script text to PowerShell.
- Linux/WSL Server Tokio workers receive the same bounded 8 MiB worker-stack policy needed by real E2E request depth.
- Active E2E scripts use current WebPi environment/tool/output contracts, including read-revision edit fences and sparse Job observations.
- Retired shared-key Server mode is tested as fail-closed rather than advertised as a supported auth path.
- Reconnect/reconciliation/recovery smoke tests validate current Job semantics without treating omitted sparse diagnostics as required fields.
- Active user-facing WebPi naming/documentation surfaces were tightened while stable crate/wire/SDK identifiers and explicit historical evidence remain unchanged.
- Release documentation now matches fail-closed public publication workflows.
- Coding-loop eval now compares baseline and guided workflow quality using current contracts.

## Security/review verdict

`git diff --check` passed and workspace hygiene reported no blocking finding. Full Rust/auth/MCP/Job/security-oriented tests are green.

However, this audit began with a heavily dirty worktree. The current diff spans roughly 290 paths and includes large auth/OAuth/plugin/Runner/Job changes that existed before or outside the goal's attributable edits. No start-of-session file snapshot was recorded, so the current whole-tree diff cannot truthfully be labeled a goal-only change set.

Release-candidate handling:

- The user explicitly approved treating the full tested working tree as the release-candidate snapshot, including valid pre-existing development work, provided temporary/generated artifacts are cleaned and the final tree is not dirty.
- Do not discard or overwrite valid pre-existing work.
- Three unreferenced screenshot/debug artifacts under `artifacts/` were removed from the candidate before commit.
- A clean sibling worktree was created at `E:\WebPi\webpi-core-goal-audit` on branch `goal/webpi-audit-20260926`, but Runner project-path policy does not authorize that sibling directory, so WebPi will not use it as an execution path. It remains untouched after creation.
- The PR must be described as a first-release candidate snapshot rather than a goal-only patch.

## External authority blockers

### PR

Authenticated GitHub CLI access exists, `yyjeqhc/webcodex` grants the current account READ only, and the configured `upstream` push URL is disabled. The user explicitly approved creating a fork and opening a Draft PR for this release-candidate snapshot. Fork `iydjjjjjj/webcodex` now exists and Draft PR `yyjeqhc/webcodex#689` tracks branch `release/webpi-first-release-20260926`.

### Deployment

The user authorized temporary addition of `service:restart` and `service:deploy`. Those scopes were added in place to the existing `webpi-action` PAT through the supported local standalone bootstrap/admin flow and `/api/tokens/update_scopes`; no plaintext credential was printed or rotated. `runtime_status` confirms both service capabilities are now authorized.

### Plugin migration

The current OpenAI migration contract was reviewed. The migration package is in `PLUGIN_MIGRATION.md`. Execution waits for a reviewed deployed WebPi revision and the ChatGPT-side migration/plugin/custom-MCP UI permissions.
