# Agent Release Process Notes

Detailed release readiness lives in
[`RELEASE_CHECKLIST.md`](../RELEASE_CHECKLIST.md).

**Default agent policy** (no tag / no push / no npm publish / no GitHub Release /
no deploy unless an explicit release task satisfies the Release Exception) is
defined in [`AGENTS.md`](../../AGENTS.md). This file only expands operator
procedure; it does not relax defaults.

---

## 1. When release operations are allowed

Only when **all** of the following hold:

1. The user **explicitly** requests a release, tag, push, GitHub Release, npm
   publish, or deploy task.
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

## 2. Operator checklist pointer

Before tagging or publishing, follow sections in
[`RELEASE_CHECKLIST.md`](../RELEASE_CHECKLIST.md):

1. Source validation (fmt, check, full suite when required)
2. Focused runtime tests for touched domains
3. Product documentation consistency
4. Legacy surface guard
5. Remaining checklist items in that document

---

## 3. Non-goals for ordinary tasks

Ordinary development prompts do **not** authorize:

- `git tag` / annotated tags  
- `git push` / force-push  
- `npm publish`  
- GitHub Release creation  
- production deploy  

An explicit release prompt may override only the default no-tag / no-push /
no-GitHub-Release / no-npm-publish defaults when the Release Exception in
`AGENTS.md` is fully satisfied. It does **not** override no-force-push,
no-tag-overwrite, no-secrets, no-history-rewrite, or validation gates.
