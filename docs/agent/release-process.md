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
   version do **not** already exist.
5. No force-push, tag overwrite, published-commit amend, or release replacement.
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

## 3. Operator checklist pointer

Before tagging or publishing, follow sections in
[`RELEASE_CHECKLIST.md`](../RELEASE_CHECKLIST.md). The final executable pre-tag gate is
`.github/workflows/release-readiness.yml`, dispatched through
`scripts/release_operator.py readiness-start` for one exact merged `main` SHA and
observed through the same durable state with `readiness-status`. It runs the canonical
release check, locked full workspace suite, frontend checks, both zero-config E2E
transports, and the coding-loop compare eval without producing release candidates.
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
steps rather than one opaque command. One well-connected Linux host performs the single
full public-byte verifier after publication. Do not fan release downloads or rebuilds out
to per-platform development machines merely to prove that a foreign archive is
downloadable. Native execution/architecture evidence belongs to the reviewed
release-build matrix; the public verifier checks the published bytes without executing
foreign binaries.

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
no-tag-overwrite, no-secrets, no-history-rewrite, or validation gates.
