# Desktop-managed global instructions (#783)

## Scope and ownership

Desktop provides an explicit, locally edited instruction file at
`<effective Desktop data root>/instructions/AGENTS.md`. It consumes the same
already-resolved Desktop root used by the application state, including the
Windows physical-profile-root policy. It does not call Tauri's raw data-dir
resolver again, add implicit Runner discovery, or change the ordering, authority
or precedence of Runner instructions.

The existing custom Runner/global and Project instruction preview stays read-only.
Managed editing does not use that preview because effective-instruction reads can
be truncated. There is no path argument in the managed read/save Tauri API and no
arbitrary-file editor, new MCP tool, Runtime schema or Runner capability.

Opening Instructions only observes the fixed path; it does not create the file,
append it to configuration or migrate custom paths. A fresh Desktop with no
Runner/Project may save a local draft. Only Save or Enable creates missing owned
directories. Read-only access to a not-yet-created data root reports a missing
file without creating that root.

## User flow

1. Open Tools / Instructions, edit **WebCodex managed instructions**, and Save.
2. Explicitly **Enable global instructions** for the configured local Runner.
   Existing custom files and their order remain; the managed path is appended
   only if absent. The configured limit is not raised to make room automatically.
3. Subsequent content saves change only the managed file. An enabled Runner reads
   the new bytes at its next instruction request; already-delivered model context
   is not retroactively replaced. Content editing needs neither reload nor restart.

A separate Enable on a missing, unchanged file creates an empty file. Enable is
blocked while the editor has an unsaved draft. The configuration indicator reports
membership in the observed on-disk Runner settings, not a proof of live activation.
The explicit successful apply result is separate. Additional instruction files
remain available through the existing picker and Advanced path editor.

Editor drafts survive Project/catalog refresh and tab switches within Tools.
Save conflicts leave the draft intact. Reloading over a dirty draft requires an
explicit discard confirmation. No autosave, refresh polling, retry of a mutation,
or automatic migration is introduced. Navigating away from the whole Tools page
still uses the existing page lifecycle; this is not a persisted draft store.

## Filesystem boundary

The editor reads the complete regular UTF-8 file, at most 1 MiB. An existing empty
file and a missing file have different revisions. Saves compare the observed
SHA-256 revision before staging and again immediately before replacing, use a
same-directory exclusive temporary file, sync its bytes and atomically replace
the destination. Temporary-file cleanup is restricted to the owned temporary name.
Content hashes are edit-concurrency tokens, not Runner configuration generations.

Desktop serializes its own edits. The final digest recheck catches ordinary
external-editor updates, including changes while staging. Like the existing
configuration CAS, this is not an OS transaction with arbitrary non-cooperating
writers: it cannot universally exclude a last-instant external write between the
final check and rename. There is no force-overwrite option.

On Unix, root and child directory handles are opened without following their
owned leaf; child creation, reads, temporary files and replacement use `*at`
operations against held directory descriptors. Location identity is rechecked,
so swapping the managed directory for a symlink cannot redirect the commit or its
cleanup. Nonblocking/no-follow reads reject FIFO, special files and symlinks.
Files use mode 0600 and new directories 0700. The effective root's permitted
ancestors (such as system-managed macOS path aliases) retain existing semantics.

On Windows, root/managed-directory handles reject reparse points and are held
without write/delete sharing; files are opened with OPEN_REPARSE_POINT and checked
as regular files. Atomic replacement uses MoveFileExW with replacement and
write-through flags. A Junction regression is included in the Windows suite;
macOS validation does not substitute for running that platform test.

The synced temporary file and atomic replacement prevent preview truncation or
partial-write loss. Unix directory sync is best-effort after commit; an already
committed replacement is not described as rolled back. This is not a general
filesystem sandbox or a claim of power-loss-proof distributed transactions.

## Shared settings application

Instruction/Skill paths and allowed roots use the same staged-settings transaction:

`exact saved target + expected paths -> stage TOML -> check -> generation-fenced reload`

The transaction preserves credentials, unrelated fields and comments. Changed
path lists are checked/reloaded, not handled by Restart Runner. MCP/ACP/native
plugin controls keep their separate existing restart semantics.

A failed check or definitely rejected reload restores only the still-matching
candidate, never an external edit. A transport failure or indeterminate reload
causes one read-only check and returns reconciliation-required. The candidate is
retained, not resent or rolled back. A generation increase alone does not prove
which request changed it; unchanged generation also does not prove that a timed-out
reload cannot execute later. This replaces the previous allowed-roots-only
inference rather than duplicating it for instructions.

If first Enable created a file but configuration fails, the file is retained for
the user; content is never deleted as a configuration rollback. The frontend
reobserves settings/file once and keeps the error visible. Users with an offline
Runner can still save content, but path activation requires reconnecting and an
explicit apply. No service is automatically restarted.

## Validation and operations

Focused tests cover full-file reads above preview budgets, 1 MiB boundaries,
invalid UTF-8, missing/empty revisions, ordinary external edits, same-instance
concurrent editors, links/special files, directory replacement, exact fixed-path
IPC, physical-root plumbing without a Project/Runner, custom-path preservation,
conditional rollback, and uncertain reload without replay. Frontend coverage
includes explicit enable/save, offline drafts, stale responses, root StrictMode
setup/cleanup and dirty-draft discard confirmation. Existing custom/project
preview and other extension flows remain in their existing tests.

Native macOS validation for this change: the full Desktop Rust library passed
258 tests with 5 existing ignored; the complete Desktop frontend suite passed
188 tests in 13 files, including seven managed-editor cases. TypeScript, form
control contract and production frontend build passed. The existing seven
Desktop Rust warnings, one environment warning and frontend >500 kB chunk
warning remain; their thresholds were not changed. No Server/workspace full suite,
Windows native run or UI process driving production settings is implied.

The native mini checkout has an existing self-signed **WebCodex Local Development**
identity. Any dogfood candidate for this task must use that same certificate and
stable app/Runner requirements, not the ad-hoc defaults of the generic local DMG
helper. Creating a signed candidate is distinct from installing it: the existing
`/Applications/WebCodex Desktop.app`, its data and the independent control Runner
must not be replaced or restarted by the feature tests. The PR records exact
validation and signed-candidate evidence after completion.
