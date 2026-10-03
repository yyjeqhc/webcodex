# Review and discovery: bounded work with explicit semantics

## Basis and workspace

This change follows the review/discovery audit against `34de0604` and is based
on `dd1f548d945e6b757b32c4343b501f3e0499b095` (v0.5.0). The authorized
`special:/root/git/webcodex` checkout had another unfinished merge and active Git
processes. It was preserved. Implementation uses a managed worktree of that same
repository, `/root/.webcodex-worktrees/webcodex-a6e3b696`, on
`perf/review-discovery-simplicity`. No production deployment or service restart
is part of this change.

## One review source, not two competing representations

`GitReviewSourceIdentity` now owns either a workspace identity or the complete
canonical `CommittedGitScope`. The redundant `GitReviewScope` and JSON-to-identity
round trip are removed. A committed range is resolved once and retained with its
snapshot. Continuations use the same typed source rather than resolving the range
again or comparing two JSON objects constructed from that same source.

These removals do not remove external checks: exact caller/Project/Session and
paging inputs are still checked, malformed native frames still fail closed,
protected paths retain their checks, and workspace observations still compare the
live HEAD/tree/index/branch identity before publication.

### Workspace means the final net change

The workspace patch is **HEAD to the frozen complete worktree**, or empty tree to
the worktree in an unborn repository. It includes staged modifications and new
untracked files rather than showing only the unstaged `git diff` plane. Metadata
still distinguishes staged, unstaged, untracked and conflict state.

This is not an index-only commit plan: a staged edit followed by an unstaged
reversal can cancel in the net patch. The explicit patch basis is
`head_to_frozen_workspace`. `read_git_diff_hunks(cached=true)` remains the
index-only specialist. Existing ignored-file and protected-path behavior is not
expanded into permission to read arbitrary secrets.

### Less repeated work

| Path | Previous Runner observations | New Runner observations |
|---|---:|---:|
| Workspace initial page | 4 | 2 |
| Workspace continuation | 3 | 1 |
| Committed initial page | 4–5 | 3–4 |
| Committed continuation | 2 | 1 |

Initial workspace identity and metadata share one Runner request. The immutable
page and trailing live-identity check share the second request. A continuation
needs only the latter. A live change during that compound observation is tested
inside the script, before its trailing check—not after the completed observation
has already returned from the Runner.

Immutable review pages generate a patch once. Content-addressed object identities,
the isolated attribute view and the exact projection identify their source; two
additional full-patch rehashes would observe no independent mutable source. The
workspace owner still checks the actual live state after producing its page.

The standalone mutable diff specialist keeps its pre/post source fingerprints,
but each fingerprint captures the producer exit code in the **same pipeline**
through a private bounded status file. This reduces five diff executions to three
without masking upstream failure. Its temporary file is removed on all shell
exit paths; it is not a patch spool or persistent cache.

### Defaults and continuation ownership

Review uses three context lines, so an ordinary one-line edit does not exceed the
160-line hunk ceiling simply because of 80 lines of surrounding context on each
side. The standalone diff specialist retains its existing 80-line context.

Only the outer `review_changes.next_call` continues a review. The private diff
engine's competing recovery calls are not exposed. `diff.has_more` agrees with the
outer continuation, including an unfinished hunk fragment. Nonrecoverable
truncation remains visible; metadata completeness is not a claim of complete
patch coverage. `paths` narrows patch selection, not the whole-workspace identity
or metadata scope. A cheap cleanliness probe should use `get_git_status`, not
construct a review snapshot.

## Directory discovery: page before transport

A current Runner advertises the additive `file_list_page` capability. The Server
selects a typed `DirectoryPageRequest`; admission rechecks the actual Runner
capability under the registry lock. Unknown/older Runners retain the existing
complete-source listing path. A malformed native page is never retried as a
legacy operation or interpreted as a complete listing.

The Runner enumerates one directory, sorts leaf names and returns only the
requested page. JSON preserves newlines in filenames. The Server validates page
identity, counts, ordering, names, progress and byte bounds before reconstructing
Project-relative paths. Directory request cancellation reuses the existing
pending-read owner.

Bounds are 500 requested entries, 48 KiB native encoded page, and 16 MiB accounted
name/entry staging before a clear narrow-search error. The staging accounting is
not an allocator RSS measurement. New pages retain no persistent directory cache.

The producer still scans the directory on each call, but a bounded max-heap keeps
only the smallest `offset + limit` names before sorting that prefix. For the first
page, staging is proportional to the requested limit rather than directory size;
deep offsets still face the explicit staging ceiling. Work is
`O(N log min(N, offset + limit))`, not a filesystem seek. This removes full-source
transport and Server materialization, not enumeration of a live filesystem. Offsets are **not snapshot cursors**. Concurrent
directory changes may shift later pages. A generated exact `next_call` removes
mechanical argument reconstruction without promising an immutable inventory.
Use `list_project_tracked_files` query/globs/depth for broad repository discovery,
and content search when the desired implementation or symbol is known.

A synthetic 4,000-entry directory with long names, 73 entries per request, produced
at most 8,827 encoded bytes per page; the full listing exceeded 340,000 bytes.
Every entry was reconstructed once in sorted order. This is a transport/count
experiment, not a production latency claim.

## Project selection and diagnosis

Exact Runner/Project filters are applied under the canonical authorized registry
snapshot before cloning Project inventories. Scanning remains proportional to the
selected inventory; cloned Project bodies are proportional to matching rows, not
all visible Projects. Compact selection retains paths so same-name worktrees can
be distinguished. Existing CLI/Console full-inventory consumers, smoke fields,
active-Job semantics and current authorization are preserved.

The preexisting unfiltered full-list default and filtered 100-result ceiling are
not converted into a new snapshot-pagination subsystem here. Use exact selection
or query to narrow that interface; this change does not claim to have added a
Project-list cursor.

Metadata diagnostics now record bounded selectors/page positions and result
counts for Project and directory discovery. Inventory bodies, file contents and
credential values are not captured. These facts distinguish changing directories
or pages from repeated identical requests without enabling full payload tracing.

## Warnings and validation

The initial `cargo test --workspace --no-run --locked --message-format=json`
completed and reported one default-build warning: redundant parentheses around a
CLI test closure. The closure and its panic-safe process cleanup remain unchanged.
No global warning suppression is used. The focused CLI process-preservation test
passed.

Validation uses an isolated target directory after a shared-target build produced
inconsistent dependency API observations. The other checkout's sources, merge and
build artifacts were not cleaned or reset. Focused/broad results and warning
counts are recorded in the final acceptance section below after source freezes.

## Reproduction

```sh
cargo test --locked -p webcodex --lib tool_runtime::tests::git -- --nocapture
cargo test --locked -p webcodex --lib page_producer_tests -- --nocapture
cargo test --locked -p webcodex --lib targeted_inventory -- --nocapture
cargo test --locked -p webcodex --lib list_project -- --nocapture
cargo test --locked -p webcodex-runner --bin webcodex-runner directory_page -- --nocapture
cargo test --locked -p webcodex-core --lib
```

## Upgrade and remaining boundaries

Review execution requires only the new Server: it uses existing Runner POSIX
execution rather than a new review protocol. Native directory transport savings
require the new Runner capability. Old Runners remain usable through the existing
bounded legacy source path; extremely large legacy sources can still fail before
paging. Neither application database migration nor Session ledger migration is
introduced. Model tool names and permission scopes are unchanged; review meaning,
compact selection paths and directory continuation output are explicitly updated.

There is no new unbounded patch cache, filesystem watcher, independent authority,
service reconfiguration, or claim that every tool is now constant-time. Large
workspace freezing, native Git processing, whole-directory enumeration and network
latency remain real costs. The production sf improvement must be measured after
a separately authorized deployment.

## Acceptance on the final implementation

All following executions used the isolated target directory. Focused subsets are
not additional unique tests beyond broader suites. Ordinary ignored tests are not
counted as executed.

| Scope | Result |
|---|---:|
| Runtime default library | 3,271 passed; 3 ignored |
| Runner binary | 1,010 passed; 4 ignored |
| Runner Registry library | 324 passed |
| Core library | 327 passed |
| Tool Contracts all-features library | 284 passed |
| Runner Config library | 20 passed |
| Git Runtime focused suite | 146 passed |
| Real diff producer/failure/cleanup test | 1 passed |
| Targeted Project inventory | 14 passed |
| CLI closure/process-preservation regression | 1 passed |

The first broad Runtime attempt failed the existing compact App schema ceiling
at 87,048 bytes versus 87,000. Duplicate descriptor wording was shortened to
86,863 bytes for the anonymous App profile; the ceiling, tool identities, source
semantics and permissions were not relaxed. The corrected Runtime suite passed. Two contract description assertions were updated
to assert live-directory and canonical-next-call semantics instead of the old
full-source-transport wording, while functional page/authority tests remain.

The broader Runner run also exposed an unrelated timing-fixture assumption. Four
configuration RPCs with 0.6-second replies can admit the fourth at 1.8 seconds,
before the 2-second cumulative deadline, then log its completion during cleanup.
The production result already correctly reported setup timeout and admitted no
prompt. The fixture now uses 0.75-second replies and additionally requires fewer
than four configuration **requests**, so the cumulative deadline must prevent the
next admission. Existing timeout, NotStarted, no-prompt and completed-count
assertions remain. No production ACP logic or timeout changed. The corrected
1,010-test Runner suite passed.

Default acceptance suites reported zero compiler-warning lines. The initial
warning was corrected at its source, not suppressed. An all-features build also
exposed `ObservedRunnerRequest.login`, a Code Mode test-only field always set to
`false` and never read. Both the field and its synthetic initializer were removed;
the actual Runner request and test target/Session assertions remain unchanged.
Final `cargo test --workspace --all-features --no-run --locked
--message-format=json` completed successfully with **29 test artifacts, zero
warnings and zero errors**. The compiled all-features Runtime binary then passed
the 146 Git tests and the exact Project/Session Code Mode read regression. This
is Linux compilation/execution, not native execution on other operating systems;
`--no-run` is not counted as a test execution. The machine-readable evidence is
`zero-warning-report.json`, with raw diagnostics retained beside it.

Raw logs and machine-readable acceptance reports are under
`/tmp/webcodex-review-discovery-fix/`, including the original failing attempts.
The previous shared-target API mismatch is excluded from final evidence; neither
another checkout's source nor its cache was cleaned to obtain passing results.
