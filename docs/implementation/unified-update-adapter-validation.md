# Shared update adapter validation

Recorded 2026-10-06 on Linux x64, against `upstream/main` `ed22e5c0`, with a dirty source development build. No release package, native installer, authorization prompt or existing host service was exercised.

| Check | Actual result |
| --- | --- |
| `cargo test --locked --offline -p webcodex-environment unified_update --profile dogfood` | 55 passed after the data-directory repair: discovery, bounded download, source/candidate verification, cache exclusion/privacy, reconciliation, installed-versus-running identity, noncreating status and safe component projection. |
| `cargo test --locked --offline -p webcodex-environment upgrade::status --profile dogfood` | 11 passed: all journal phases, terminal history, bounded whitelist/canary output, stale targets, original operation binding and conservative headless recovery. |
| Existing `upgrade::tests` | 9 passed with `--locked --offline --profile dogfood`: original transaction and restoration regression coverage. |
| Desktop `cargo check --offline --manifest-path apps/desktop/src-tauri/Cargo.toml --profile dogfood` | Passed against the extracted adapters. Two existing dead-code warnings remained. |
| Desktop native `cargo test --locked --offline --manifest-path apps/desktop/src-tauri/Cargo.toml updates` | 11 passed: existing update/cache and literal native handoff adapter tests. |
| Guarded terminal reconciliation, cache and status focused tests | 24 passed with `--locked --offline`: exact pending operation, stale/missing targets, exclusive existing fences, rollback reconciliation and unchanged files on rejected cleanup. |
| Workspace and Desktop Rust formatting, `git diff --check` | Passed. |
| Initial Windows production CI | Compiled, but failed its zero-warning gate with 11 platform-unused imports/constants/parameters introduced by the extraction. These were scoped to their Unix/Linux consumers; the Linux unified-update suite passed all 53 tests again. The replacement production zero-warning checks passed. |
| Windows Desktop test compilation | CI identified four test references to the moved path-normalization helper. Tests now use the existing path-identity comparison, and a typed projection from the shared resolver preserves the original Desktop error codes, fixed reasons and bounded path-kind details. All seven Windows junction tests remain. Replacement Windows test CI is recorded separately; this is not native installation acceptance. |
| Data-directory regression checks | Shared resolver 2 passed; Desktop error projection 2 passed. Both native library checks, Rust formatting and whitespace checks passed on Linux. A local Windows cross-target attempt stopped in `ring` because MSVC `lib.exe` is unavailable; it did not compile the changed adapter. |

Query tests use temporary private stores and prove missing roots/fences stay missing, existing permissions/mtime stay unchanged, and unknown probe fields do not cross the public status. The earlier creation-capable query constructor was identified by source inspection; no prior Linux permission mutation was reproduced.

Upgrade tests use disposable state and controlled backends. Their success does not establish actual DEB/RPM/PKG/EXE installation, system authorization, logout/reboot, real service ownership, database migration or native rollback. Those matrix rows remain pending in `unified-deployment-validation.md`.

This contribution is the shared prerequisite for separate Desktop UX, Linux terminal CLI, and Windows guarded-handoff PRs. Old Windows packages do not advertise a guarded handoff. No new remote update operation is supplied.

## Follow-up completeness repair

After merging `upstream/main` `6713748b` and the Windows invitation fixture repair,
two deterministic tests reproduced a cross-process cache defect: a long-lived
manager could overwrite a different updater's pending handoff and delete its
version cache. Both tests failed before the fix. Newly fenced mutations now
reload durable state; an unchanged record preserves this process's verified
ready/cancellation state, while externally changed records receive the original
conservative verification.

The three new regressions and the full focused `unified_update` group passed
(58 tests total) with the locked dogfood profile on Linux x64. The installation
regression also asserts zero launcher calls and unchanged pending bytes. These
are disposable fixture results, not package installation or service acceptance.

## Cache fence lifetime regression

The revised Linux CI package runs exposed `CacheUnavailable` in the exact
rolled-back reconciliation regression. A deterministic retained-duplicate test
then proved a separate lifecycle defect: closing the original descriptor alone
does not release a Unix flock while another descriptor shares its open-file
description. Cache operation guards now explicitly unlock on drop, matching the
existing Environment setup fence. Private opens, CLOEXEC, acquisition rules and
all six production callers' hold scopes are unchanged.

The retained-duplicate test failed before the fix and passed after it, covering
new/existing exclusive and existing shared fences. Debug `unified_update` passed
59 tests, including the CI-failing reconciliation test. The specific CI fork/exec
window remains a mechanism inference, not a captured process trace; revised CI
results must be checked independently.

An initial complete Environment package run passed 180 tests and failed an
unchanged service-directory fixture under this host's inherited umask `0002`.
That fixture passed in an isolated child with umask `0022`. The parent process,
fixture and production ownership checks were not modified.
