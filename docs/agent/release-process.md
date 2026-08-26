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
   CLI, MCP, or GPT Actions surface.
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

- the checkout is clean, its Cargo/npm version is the requested version, and its
  `HEAD` equals current remote `main`;
- the remote tag is an annotated commit tag;
- no GitHub Release exists for the tag and the npm version is absent;
- every matching authoritative `release-build` run is terminal and none concluded
  successfully.

Use `python3 scripts/release_operator.py reclaim-tag --version <VERSION> --confirm
v<VERSION> --root <EXACT_MAIN_WORKTREE>` rather than a naked `git push --delete`.
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

## 3. Operator checklist pointer

Before tagging or publishing, follow sections in
[`RELEASE_CHECKLIST.md`](../RELEASE_CHECKLIST.md). The final executable pre-tag gate is
`.github/workflows/release-readiness.yml`, dispatched through
`scripts/release_operator.py readiness-start` for one exact merged `main` SHA and
observed through the same durable state with `readiness-status`. Readiness is deliberately
two-stage. Stage 1 runs the canonical release check, complete locked Rust workspace suite
(package-sharded without test-name filters), frontend checks, both zero-config E2E transports,
and the coding-loop compare eval in parallel. No native release-profile or Server-image build
may start until every Stage-1 test gate succeeds. Stage 2 then fans out disposable
release-profile native validation for all six published platforms plus native `linux/amd64`
and `linux/arm64` Server-container build/runtime/health and digest-pinned bootstrap-generation
checks. These independent Stage-2 build lanes run in parallel so release wall time is governed
by the slowest required lane rather than their sum. The Server-image
readiness jobs use only local disposable images: they do not log in to a registry, upload
artifacts, push packages, or produce formal release candidates. Linux preserves the release
ABI/dependency gates, macOS x64/arm64 build on matching native runners and add the Runner suite,
and Windows includes the local npm-install smoke.
Product-documentation consistency and allowed legacy-term matches remain part of the
release-prep review rather than being guessed by an automated semantic checker.

The normal release topology deliberately separates roles. The release control host first
runs `release_operator.py preflight`, then GitHub Actions validates the exact pre-tag
source in the durable readiness workflow. After explicit authorization creates the
immutable tag, `release_operator.py build-start` / `build-status` bind one durable
`rb_*` request to the reviewed release-build workflow; GitHub Actions builds and assembles
one same-run native candidate bundle. The release control host collects that exact bundle
with `release_operator.py collect` (locked run id, source SHA, and tag; GitHub artifact
REST download, no `gh run download`) and stages npm with `stage-npm` using the retained
CI binaries without Cargo. After draft assets are uploaded, `verify-draft` compares the
GitHub asset SHA-256 digests and sizes against the retained bytes instead of downloading
the same ~100 MiB again. The privileged actions themselves—creating the immutable tag,
making the GitHub Release public, and `npm publish`—remain explicit human-authorized
steps rather than one opaque command. Publishing the GitHub Release then authorizes the
reviewed `release-image.yml` adapter to build the server-only `linux/amd64` +
`linux/arm64` image from that exact immutable tag, publish/reconcile it in GHCR, and
attach its immutable digest record plus a self-contained, digest-pinned clone-free
bootstrap asset to the same Release; the bootstrap is generated from the reviewed
publication-workflow source so guarded backfills do not depend on new tooling existing in
an older application tag. `release-build.yml` remains a read-only candidate producer with
no package-write authority. One well-connected Linux host performs the single full public-byte
verifier for npm and native Release archives after publication, while the image workflow
independently requires anonymous GHCR availability and verifies the public deployment-asset
hashes. Do not fan release downloads or rebuilds out to per-platform development
machines merely to prove that a foreign archive is downloadable. Native
execution/architecture evidence belongs to the reviewed release-build matrix; the
public verifier checks the published bytes without executing foreign binaries.

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
