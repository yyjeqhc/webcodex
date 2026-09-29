# Build identity at the composition boundary

## Scope and ownership

Baseline: `9877de8e44849bfb006bb0191d9c9a19ffb886a6`.
Workspace: special `/root/git/webcodex-review`.

`webcodex-core` retains the stable `BuildInfo`/`RuntimeBuildInfo` types and pure
formatters, including the existing machine-readable Desktop/runtime contract.
It no longer has a build script or captures checkout-dependent environment values.

The small `webcodex-build-info` crate owns the previous collector unchanged and
provides the familiar `current`, `runtime_build_info`, `version_output`,
`machine_build_info`, and `build_info_json` composition facade. Only Server,
Runner, CLI and the separately built Desktop consume it. Its sole dependency is
core; core and domain packages never depend on this provider. No new external
package, runtime Git lookup, mutable global initialization, collector copies or
public wire/schema changes are introduced. Workspace policy pins this direction.

One additional Cargo package is intentional: it isolates volatile generated
identity while keeping one collector for all executable surfaces. Re-exporting
this provider from core would recreate the original invalidation fan-out and is
not allowed. Formatting and metadata types are stable even when Git identity
changes. This does not remove legitimate recompilation after core/domain source
changes or guarantee that composition/linking is cheap.

## Identity and compatibility

The moved collector preserves exact existing behavior:

- Git commit and tracked dirty state remain truthful, including stat-only changes,
  staged/unstaged edits, deleted tracked paths, packed refs and absent reflogs.
- Linked worktrees sharing a target directory resolve their own manifest/Git
  identity; returning to the original checkout cannot retain a sibling identity.
- `WEBCODEX_GIT_COMMIT`, `WEBCODEX_GIT_DIRTY`, `WEBCODEX_BUILT_AT`, and
  `SOURCE_DATE_EPOCH` keep their existing precedence. Tests set these only in child
  environments; production configuration is not modified.
- Non-Git source archives retain explicit unknown Git state and the established
  timestamp fallback. Unknown is not silently converted into clean.
- `--version`, `--build-info-json`, runtime status and Desktop identity retain
  their existing fields, protocol generation and early-exit behavior.

As before, tracked Git changes define the dirty flag; this work does not invent
an untracked-file cleanliness guarantee. Deployment is not part of this change.

## Measured invalidation experiment

The same command was run against the baseline and candidate, using the existing
review target directory. Each revision received a warm run, an identical repeat,
and a child-only `WEBCODEX_GIT_COMMIT=111111111111` override. Actual source files
and Git refs were not changed between phases. Other identity override variables
were removed from those child environments. The override is synthetic test data,
not a claim about a deployed binary's source.

```sh
# Run at each reviewed revision, after saving any unrelated workspace changes.
# Warm/repeat use that revision's actual short SHA; the third run changes only
# a child-process override. Do not deploy artifacts built with the test override.
for identity in "$(git rev-parse --short=12 HEAD)" "$(git rev-parse --short=12 HEAD)" 111111111111; do
  env -u WEBCODEX_GIT_DIRTY -u WEBCODEX_BUILT_AT -u SOURCE_DATE_EPOCH \
    WEBCODEX_GIT_COMMIT="$identity" \
    cargo check --locked --offline -p webcodex -p webcodex-runner -p webcodex-cli \
      --message-format=json
done
```

For each `compiler-artifact`, inspect `fresh`; count a workspace package if any
of its targets is not fresh. A fresh build-script target does not imply its
library is fresh. All three invocations exited successfully in both revisions.

| Observation | Baseline | Candidate |
|---|---:|---:|
| Identical repeat: rechecked workspace packages | 0 | 0 |
| Identical repeat: observed elapsed seconds | 0.420 | 0.388 |
| Identity-only change: rechecked workspace packages | 15 | 4 |
| Identity-only change: observed elapsed seconds | 12.406 | 6.239 |

The candidate's four affected packages were `webcodex-build-info`, `webcodex`,
`webcodex-runner`, and `webcodex-cli`. Core, Tool contracts, Session, Validation,
Store, Workspace, Runner config/registry, native LSP and computer primitives stayed
fresh. The warm runs (73.931 / 60.853 seconds) include initial compilation/source
migration and are **not** a speed comparison.

This is one local `cargo check` observation per case, not a controlled multi-run
benchmark, release-build/link-time measurement, or throughput claim. The stable
acceptance criterion is the reduced invalidation set, not a fixed time saving.

## Regression coverage

`python3 -m unittest scripts.tests.test_build_identity -v` exercises the shipped
collector against disposable real Git/Cargo fixtures without registry dependencies:
packed refs/no reflog and ordinary/linked worktrees; tracked dirty transitions and
stat-only changes; same-source/no-op caching; commit-only and documentation-only
changes that preserve core/domain artifacts; implementation-only recompilation;
shared-target worktree switching; and source archive/release override precedence.
The contract CI lane now runs these tests explicitly.

Core formatting/Desktop contract tests and the new provider's tests protect the
unchanged machine-readable schema and metadata. `workspace_boundary_check.sh`
checks all 21 packages and rejects upward core/domain dependencies. Root and
Desktop lockfiles add only the local provider/dependency entries; unrelated
registry dependency versions are not intentionally changed.
