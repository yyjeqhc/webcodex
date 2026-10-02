# Agent Release Process Notes

Detailed release readiness lives in
[`RELEASE_CHECKLIST.md`](../RELEASE_CHECKLIST.md).

**Default agent policy** is defined in [`AGENTS.md`](../../AGENTS.md): external
changes, including deploys, require an explicit task and named destination. A
reviewed development build deployed only to named dogfood targets is distinct
from a release/publish rollout. This file expands that distinction; it does not
relax the default authorization rule.

---

## 1. Development dogfood deployments are not releases

An ordinary coding or review task never authorizes deployment or service
restart. When the user explicitly requests deployment of a reviewed development
commit to named dogfood targets, the operator may build/install/restart that
exact development build without starting the release process.

For iterative self-hosted deployment builds, prefer `cargo build --profile dogfood`
(and the resulting `target/dogfood/` binaries). This profile inherits release
runtime semantics while disabling LTO and enabling incremental compilation for
faster rebuilds. Formal release and publication artifacts continue to use the
`release` profile.

For that development deployment:

1. Change only the explicitly named targets. Do not create staging resources,
   deployment channels, or additional rollout state.
2. Record the requested source commit and verify the installed build with the
   existing build identity (`git_commit`, `git_dirty`, `built_at`). Do not invent
   a second build identity and do not mask or rewrite `git_dirty=true`.
3. Preserve the prior working build or another concrete rollback path before
   replacing or restarting the target.
4. Run focused post-deployment smoke appropriate to the changed Server, Runner,
   CLI or MCP surface.
5. Report the exact targets changed, build identity observed, smoke result, and
   rollback path.

A development dogfood deployment does **not** by itself authorize or require a
version bump, Git tag, GitHub Release, npm publication, release metadata, or
published release artifacts. If the task also requests release or publication,
the release contract below applies unchanged.

---

## 2. When release or publication operations are allowed

Only when **all** of the following hold:

1. The user **explicitly** requests a release, tag, push, GitHub Release, npm
   publish, or deployment of a published release.
2. The request names the **version**, **package**, **repository**, and
   **release target**.
3. The worktree is clean before the release starts, except for release files
   intentionally created during that task.
4. The agent verifies that the remote tag, GitHub Release, and npm package
   version do **not** already exist, unless an explicitly approved failed
   pre-publication tag has first been reclaimed through `release_operator.py
   reclaim-tag` under the bounded exception below.
5. No force-push, published-tag movement, published-commit amend, or release
   replacement. Reclaiming an eligible failed pre-publication tag is a distinct
   recovery operation, not permission to rewrite a published release identity.
6. Relevant release gates run; stop on the first failed gate.
7. Secrets, tokens, npm/GitHub tokens, `.env` contents, and credential files
   are never printed.
8. Any post-tag manifest/checksum commit is reported explicitly and must not
   move the release tag.
9. If the task conflicts with safety rules, stop and report before irreversible
   changes.

Who confirms: the **human requester** of the named release task. Agents do not
self-authorize releases.

What to record in the final report: version/target, gates run, tag/publish
results (if any), and any deferred checks.

---

### Failed pre-publication tag reclaim

A version tag may be reclaimed only when the human requester explicitly names that
version and authorizes the destructive recovery, and all of these facts hold at the
time of deletion:

- the checkout is clean, its Cargo/npm/Desktop/Tauri versions are the requested version, and its
  `HEAD` equals the selected remote release source ref (`main` or exactly `release/v<VERSION>`);
- the remote tag is an annotated commit tag;
- no GitHub Release exists for the tag and the npm version is absent;
- every matching authoritative `release-build` run is terminal and none concluded
  successfully.

Use `python3 scripts/release_operator.py reclaim-tag --version <VERSION> --source-ref
<SOURCE_REF> --confirm v<VERSION> --root <EXACT_SOURCE_WORKTREE>` rather than a naked `git push --delete`.
The operator fences the repository/source/version, scans bounded release-build
history, deletes the remote tag, reconciles the remote ref, and removes the matching
local tag. Its normal GitHub Release check is authenticated so draft releases are
visible. `--allow-public-release-check` is an explicit degraded mode for a public
repository only after the human operator separately confirms that no draft Release
exists; it is never an automatic fallback.

A successful authoritative release-build, any GitHub Release (including draft), or
an existing npm version closes this exception permanently for that version. Failed
historical workflow runs remain as audit evidence after a reclaim and are not
deleted or rewritten.

### Failed pre-publication build source-fix recovery

A failed authoritative `release-build` may expose a release-only build, packaging,
installer, provenance, or workflow-contract defect that pre-tag readiness does not
exercise. After that happens, the same version may take a bounded recovery path
without repeating readiness **only** when all of these conditions hold:

- at least one earlier readiness run for this release version/source line completed
  successfully before the failed authoritative build;
- the failed build is terminal and the diagnosed fix is confined to release/CI,
  packaging, installer, provenance, tests, or documentation surfaces rather than a
  product/runtime behavior change;
- the repaired exact `release/v<VERSION>` source passes its ordinary exact-source
  push CI and focused regression validation for the failed build contract;
- no GitHub Release exists for the tag, the npm version is absent, and no
  authoritative `release-build` for the version has succeeded;
- the human requester explicitly authorizes reclaiming and recreating the version
  tag at the repaired exact source.

In this recovery path, cancel or ignore any automatically started release-source
readiness evidence run. Use the guarded `reclaim-tag` operation, create a new
annotated tag only after the explicit tag authorization, then use a fresh durable
`build-start` / `build-status` state for the repaired tag. Do not resume a high-level
plan whose readiness/build state is bound to the superseded source, and do not
reuse artifacts from any earlier build attempt. A product/runtime source change
falls back to the normal readiness path.

## 3. Operator checklist pointer

Before tagging or publishing, follow sections in
[`RELEASE_CHECKLIST.md`](../RELEASE_CHECKLIST.md). For normal releases, cut `release/v<VERSION>` from the reviewed `main` commit chosen for the release before release prep, then target version/release-metadata work at that branch. For a hotfix, the release branch may instead start at the previous immutable release tag and carry only the required fix plus release prep. Once the branch is cut, unrelated PRs may keep merging into `main`; only the selected release source branch must remain stable through readiness and tag creation. Product fixes unique to the release branch are forward-ported to `main` separately, while release-only version metadata need not be merged back.

The final executable pre-tag gate is
`.github/workflows/release-readiness.yml`, observed through
`scripts/release_operator.py readiness-start` / `readiness-status` for one exact source branch/SHA pair. The source ref is either `main` or exactly `release/v<VERSION>`. Ordinary source-branch CI remains the common correctness authority and records complete Linux Rust/tooling coverage plus path-aware Windows x64, macOS Apple-Silicon, Desktop, and amd64 Server-image checks; scarce Linux ARM64, macOS Intel, and Windows ARM64 runners remain absent from ordinary CI.

For `release/v*`, pushing the exact source also starts `release-readiness.yml` in **source-evidence mode**. That run precomputes the same expensive release-specific evidence that used to sit serially behind operator dispatch: `extended-native.yml` supplies Linux ARM64 production coverage plus macOS Intel and Windows ARM64 runtime/Desktop build/install smoke; WebSocket/polling E2E and coding-loop compare eval run in parallel; then native `linux/amd64` and `linux/arm64` disposable Server-image jobs verify build/runtime/health/non-root behavior and digest-pinned bootstrap generation. The source-evidence run has read-only repository/Actions authority, uploads no artifacts, logs in to no registry, publishes nothing, and creates no formal release candidate.

After exact-source CI and source-evidence both succeed, `readiness-start` binds their exact run ids and attempts in durable local state. Its small operator-dispatched run re-fetches both immutable attempts with read-only Actions authority and fails closed unless workflow path, `push` event, source ref/SHA, repository, attempt, and terminal success all match. The expensive jobs are skipped in that dispatch instead of rebuilt. A release sourced directly from `main` has no precomputed source-evidence run; its dispatch retains the previous slow fallback and executes those expensive jobs itself, so ordinary main pushes do not consume scarce runners.

After immutable tagging, authoritative `release-build.yml` now starts with a cheap exact-tag preflight that runs the deterministic release/tooling contract before any scarce native matrix. Only that preflight can unlock the six-platform runtime/Desktop jobs. This is intentionally redundant with ordinary PR CI: it fences the exact tagged workflow source so YAML/shell/static packaging drift is rejected in minutes instead of after platform builds have consumed tens of minutes.

The core Release and distro/unified installers are separate concerns. By default `release-build.yml` produces the six native runtime archives plus the version-appropriate primary Desktop distributions and same-run release metadata. `include_unified_installers=true` is an explicit opt-in extension that additionally builds the Linux DEB/RPM, macOS PKG, Windows unified EXE, source manifests, and installer manifest. A unified-installer failure must not block the ordinary core Release when that option was not requested. The public download-page workflow likewise treats the installer manifest as optional: a core-only Release completes without a download-page artifact, while a present installer manifest is still validated fail-closed.

Starting with `v0.4.3`, the primary build owns macOS Apple-Silicon plus Windows x64/ARM64 Desktop candidates, while macOS Intel Desktop remains a post-publication supplemental workflow so its slow DMG build does not delay the core Release. The `darwin-x64` runtime archive itself remains part of the primary six-platform bundle. Post-publication consumers prefer machine-readable `--build-info-json` over formatting assumptions about `--version`; historical short commit identities may be accepted only when they still identify the exact immutable release source.

For normal human operation, prefer `release_operator.py doctor` before the release window and one durable high-level `release-init` / `release-resume` plan during the release. The plan composes the same low-level readiness/build/collect/stage/verify primitives without weakening their exact-source correlation. It automatically advances only recoverable phases and returns explicit `needs_authorization` states before immutable tag creation, draft creation, and public GitHub/npm publication; it returns `needs_reconciliation` instead of deleting/repeating local outputs whose completion is uncertain. `release-status` is read-only, and every low-level operator command remains available for diagnosis or bounded recovery.

The lower-level topology deliberately separates roles. The release control host first runs `release_operator.py preflight` against the exact source ref/SHA, then GitHub Actions validates that pre-tag source in the durable readiness workflow. After explicit authorization creates the immutable tag, the tag becomes the release source authority: `release_operator.py build-start` / `build-status` bind one durable `rb_*` request to `release-build.yml` dispatched from that exact tag. `main` and the release branch may advance after tagging without invalidating the build or publication plan. The workflow always builds the six native runtime archives from the exact tag. For `v0.4.3+`, its core same-run bundle contains the macOS Apple-Silicon and Windows x64/ARM64 Desktop distributions; the historical `v0.4.2`-and-earlier contract retains macOS Intel in that bundle. The current public macOS distribution contract intentionally uses ad-hoc signing and requires no Apple release credentials or notarization. Stable bundle identifiers, Runner identifier `dev.webcodex.runner`, TCC usage descriptions, native identity smoke, and exact-source evidence remain required. Developer ID/notarization is a future distribution-mode migration and must be introduced explicitly with credentials and clean-machine acceptance rather than becoming an implicit release prerequisite.

The release control host collects the exact primary bundle with `release_operator.py collect` (locked run id, source SHA, and tag; GitHub artifact REST download, no `gh run download`) and stages npm from the retained runtime bytes without Cargo. Draft verification compares GitHub-provided asset digests and sizes against those retained bytes. Creating the immutable tag, making the GitHub Release public, and `npm publish` remain explicit human-authorized steps.

Publishing a `v0.4.3+` GitHub Release independently activates two post-publication adapters. `release-image.yml` publishes/reconciles the server-only multi-arch GHCR image and its digest-pinned bootstrap assets, validating structured build identity rather than a hand-formatted commit string. `release-desktop-darwin-x64.yml` runs on the native Intel runner, downloads and verifies the already-published immutable `darwin-x64` runtime archive against the primary `SHA256SUMS`, validates its machine-readable build identity against the exact tagged source, and attaches the reconciled ad-hoc-signed DMG plus its dedicated `.sha256` sidecar to the same Release. It never rewrites the primary `SHA256SUMS`. Reruns treat an already-published DMG as immutable authority, verify any existing checksum against those bytes, and may derive only a missing checksum from the existing DMG. An orphan checksum without its DMG or duplicate/conflicting assets fail closed instead of replacing published bytes. The Intel Desktop adapter is intentionally outside the primary Release critical path and can also be manually backfilled from reviewed `main` for the exact public tag.
One well-connected Linux host performs the full public-byte verifier for npm, the six native Release archives, and the primary Desktop distributions. For `v0.4.3+`, Intel Desktop publication is explicitly outside that core acceptance boundary: no supplemental bytes is valid, a one-file intermediate state is reported as publication in progress without failing the core Release, and a complete DMG/checksum pair is verified independently. Native macOS readiness/smoke owns code-signature evidence; the Linux verifier hashes published DMGs but does not substitute for `codesign` validation. Do not fan release downloads or rebuilds out to per-platform development machines merely to prove that a foreign archive is downloadable.

---

## 4. Non-goals for ordinary tasks

Ordinary development prompts do **not** authorize:

- `git tag` / annotated tags  
- `git push` / force-push  
- `npm publish`  
- GitHub Release creation  
- development or production deploy/restart

An explicit development deployment request authorizes only the named dogfood
targets and operations described in section 1; it does not imply release or
publication. An explicit release prompt may override only the default no-tag /
no-push / no-GitHub-Release / no-npm-publish defaults when the explicit delivery
rules in `AGENTS.md` are satisfied. It does **not** override no-force-push,
published-tag immutability, no-secrets, no-history-rewrite, or validation gates;
the only tag-reuse exception is the guarded failed pre-publication reclaim defined
above.
