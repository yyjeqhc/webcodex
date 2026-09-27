# Desktop automatic stable updates

Desktop updates the unified installation, not just the Tauri executable. The
product path is **stable discovery → automatic verified download → explicit
installation confirmation → existing Environment upgrade → reconciliation**.
No updater effect starts or restarts a service, requests administrator authority,
or closes Desktop before the user confirms **Install and close WebCodex**.

## Product behavior

**About** and the update banner show the current download and the existing
Runtime compatibility notice. **Automatically download stable updates** defaults
to enabled and lives in the existing Desktop settings. Turning it off cancels an
active download, not an installer. **Later** hides the ordinary banner for 24
hours without deleting a verified package. Pending installation/recovery remains
visible even while ordinary notices are snoozed.

Discovery compares the stable `release_version` with the installed **Desktop**
version. `runtime_version` and the compatibility range remain independent: a
release `0.5.0` with Runtime `0.4.9` is a Desktop update from `0.4.9`, even when
the old Desktop is compatible with that Runtime.

Only a clean, aligned, bundled, official unified installation in the canonical
package layout is eligible for one-click replacement. Installation also requires
a configured local Environment belonging to the current account and selecting
those exact installed Runtime files. An otherwise managed installation without
that Environment may download, but cannot install through Desktop. Source,
dirty, custom-Runtime and standalone installations retain the release link and
an optional explicit download; they cannot be silently converted into a managed
installation. Install rechecks the published bytes of all four installed
executables rather than trusting a version string or a package receipt alone.

A release without the unified installer manifest is a supported legacy fallback:
**View release**, not a failed download. Notification-only older Desktops need a
manual unified installation of the first updater-capable release; the new code
cannot retroactively add an updater to an already published older executable.

## Canonical manifests and trust chain

`webcodex-release-manifest.json` remains the release/Runtime compatibility
contract. The existing download-page/npm **`manifest.json`** remains the single
installer-download contract; there is no third updater-only manifest.

1. Reconstruct the official `yyjeqhc/webcodex` GitHub API route for the exact
   `v<VERSION>` tag. Reject draft, prerelease, malformed version, duplicate asset
   names and mismatched tags. Never execute a `browser_download_url` supplied by
   the API or a URL recovered from local state.
2. Fetch canonical `manifest.json` and `SHA256SUMS`. Validate the known schema,
   exact version, three Runtime binary names, exact six-platform Runtime and
   installer sets, canonical filenames/URLs and lowercase SHA-256 values.
   Cross-check the manifest bytes and all six Runtime/installer/source entries
   against `SHA256SUMS`.
3. Fetch and hash the current platform's canonical source manifest. Validate its
   native target, architecture, immutable source SHA, exact release-build workflow
   tag, nonzero run identity, four component build-info hashes, clean source,
   shared contracts and Desktop payload declaration.
4. Stream the installer into a private partial file while computing SHA-256.
   Only an exact hash match can become the canonical completed filename.
5. Before installation, re-establish publisher metadata and rehash the cached
   file. Unix extracts the package's embedded candidate without running package
   hooks, then uses Core's existing complete candidate verifier and
   `upgrade-preflight`/`upgrade-prepare`. Windows executes the verified outer
   NSIS bootstrap, whose existing transaction owns those steps.
6. Original-owner receipts, protected package targets, the installer authorization
   broker, `installer-finish`, `upgrade-finish` and rollback remain owned by
   `webcodex-environment`. Download success is not a new source of installation
   authority.

This is GitHub HTTPS publisher provenance plus byte/transaction verification,
not a new independent signing-key or attestation system. Existing platform
signing/authorization policy is unchanged. Cached ready flags, paths, version
strings and locally edited metadata cannot authorize replacement on their own.

## Local platform selection

| Native OS / Rust architecture | Manifest platform | Unified package |
| --- | --- | --- |
| Linux / `x86_64` | `linux-x64` | `.deb` |
| Linux / `aarch64` | `linux-arm64` | `.deb` |
| macOS / `x86_64` | `darwin-x64` | `.pkg` |
| macOS / `aarch64` | `darwin-arm64` | `.pkg` |
| Windows / `x86_64` | `win32-x64` | `.exe` |
| Windows / `aarch64` | `win32-arm64` | `.exe` |

`InstallerPlatform` in Core is the Rust mapping authority. Unsupported targets
keep discovery and **View release**, with no guessed installer. Package names are
`webcodex-unified-v<VERSION>-<PLATFORM>.<EXT>`; source documents are
`webcodex-source-v<VERSION>-<PLATFORM>.json`.

## Network, cache and retry bounds

Requests use no GitHub token or user credential. HTTPS redirects allow only
`github.com`, `api.github.com`, `release-assets.githubusercontent.com` and
`objects.githubusercontent.com`, with no URL credentials, nonstandard HTTPS
port or unbounded redirect chain. Metadata has a 30-second aggregate bound;
transfer connections have a 5-second connect timeout, 20-second read timeout
and 30-minute overall bound. Stable discovery retains its shorter existing
startup bound.

Installer downloads are limited to **768 MiB**, matching the retained bundle's
per-member limit. Both `Content-Length` and the streamed count are enforced.
Manifest/source metadata limits are 256 KiB / 1 MiB. A failed hash, truncated
response, oversized stream or cancellation cannot produce an executable final
candidate. The final pre-install hash check also removes a corrupt candidate.

The Desktop-owned layout is:

```text
<Desktop app data>/stable-updates-v1/
  update.lock
  update-state.json
  <VERSION>/<PLATFORM>/
    installer.part
    <canonical installer filename>
    manifest.json
    source-manifest.json
    expanded/                 # Unix inspection, only after explicit Install
```

State has schema version 1 and a 24 KiB bound. The frontend sees neither these
paths nor Environment receipts. Private files/ACLs, atomic writes and a
cross-process lock reuse Core primitives without configuring another Environment.
A process-local lock additionally serializes download and installation attempts.

Restart discards abandoned partials and revalidates a completed file against
fresh publisher metadata; it does not trust the persisted `ready_to_install`
bit. Completed verified downloads are reused, but partial byte-range resumption
is not implemented: interrupted transfers restart from zero. Cleanup retains
only the current target, removes obsolete targets once installed, and never
follows links or deletes unrelated root entries. Directory traversal and cleanup
have explicit depth/entry budgets. Unix package expansion is time-bounded and
its resulting tree is checked against 8 GiB / 50,000-entry limits.

Automatic stable discovery is limited to once per 24 hours. Download failures
have a separate one-hour retry window; explicit **Retry** bypasses it. A user
cancellation stays paused until explicit retry, and a rolled-back installation
is never automatically retried. Metadata unavailability may postpone download
or installation; it never changes normal Runtime readiness.

## Explicit installation and OS authority

### Windows

Desktop holds the downloaded outer `.exe` against write/delete sharing while
rehashing and mapping it through `CreateProcess`, with literal arguments and no
shell. The current-user NSIS bootstrap owns candidate extraction, existing Core
preflight/prepare, the inner package, finish and rollback. Desktop does not add a
new elevation path or replace its own executable. Any OS authorization remains
subject to the existing installer policy. A durable pending record precedes
launch, and Desktop closes only after a confirmed process launch.

The outer bootstrap is current-user; machine-wide/foreign-owner installations
are not automatically adopted. V1 has no dedicated bootstrap acknowledgment
channel, so an early failure before it writes a Core transaction remains
**recovery required**, not a speculative success or automatic second attempt.

### macOS

The installed trusted CLI's canonical path and root-owned, non-user-writable
ancestor chain are checked before elevation and again at the handoff. The user
side performs existing Core preflight/prepare, persisting the original-owner
pending operation first. The OS owns the administrator prompt; Desktop does not
request or store the password.

The narrow `environment installer-apply` CLI adapter independently rechecks
publisher metadata, the complete candidate and prepared receipt. It copies and
rehashes the installer into a root-private cache before `/usr/sbin/installer`
opens it. Package hooks continue using the existing authorization and owner
broker. The helper distinguishes **started**, known **not started**, and
**unknown**; only an exact operation/version acknowledgment authorizes Desktop
exit. Known pre-launch failures restore through Core. Unknown outcomes never
trigger a competing rollback or re-dispatch from Desktop.

V1 uses the existing macOS Security framework's deprecated
`AuthorizationExecuteWithPrivileges` API with an exact protected CLI and literal
argv, rather than adding a signed background helper. This is an explicit native
dogfood limitation: macOS authorization, real package hooks and next-launch
behavior still require an isolated real release-to-release test. Replacement
with a separately signed helper is a future platform integration, not permission
to introduce a password prompt or shell-based fallback.

### Linux

Desktop requires `pkexec`, `dpkg` and `dpkg-deb`. `pkexec` uses the graphical
system authorization agent; its internal terminal agent is disabled. Missing
support or rejected authorization fails explicitly. The same protected CLI,
original-owner preparation, root-private package copy and existing package
hooks are used. No direct writes to `/usr`, stored sudo password or arbitrary
shell invocation are added. V1 uses `dpkg --install`, not dependency fetching;
missing dependencies/package-manager repair require the normal distribution
workflow. File/Environment rollback is not proof that a failed package-manager
database transaction has been repaired.

## State and recovery

The local phases are `idle`, `checking`, `available`, `downloading`, `verifying`,
`ready_to_install`, `preparing`, `installing_or_handed_off` and `failed`.
Progress uses a percentage only with a known total; otherwise only transferred
bytes are shown. UI errors are selected from a closed error-kind vocabulary,
not arbitrary Rust errors or process output.

A launch acknowledgment is never displayed as **installed successfully**. On
next launch, Desktop compares the pending Environment/operation/source identity,
Core's transaction outcome and the actual clean installed Desktop build.
Confirmed commitment with the matching installed generation clears the cache;
confirmed rollback keeps a visible failure. Missing, stale, foreign or uncertain
transaction evidence blocks another install and requests recovery. A pending
record can never advertise installation authority, even with a stale ready phase.

For recovery, first inspect the local **Environment**, `webcodex environment
status` and `webcodex environment doctor`. Establish whether the OS installer is
still running and whether the package manager is consistent. Use the existing
owner-side `upgrade-finish` / `upgrade-rollback` workflow only when its documented
preconditions are met. Do not delete the Environment journal or launch a second
installer to clear an unknown outcome. Unix root-side retained intent is also
reconciled against Core before another privileged attempt.

## Validation and native dogfood

Run from the repository root unless a subshell changes directory:

```sh
(cd apps/desktop && npm run typecheck && npm test && npm run build)
cargo test --locked -p webcodex-environment -p webcodex-cli --lib
(cd apps/desktop/src-tauri && cargo check --locked --all-targets && cargo test --locked --lib updates::)
python3 -m unittest scripts.tests.test_updater_publication scripts.tests.test_build_download_page scripts.tests.test_collect_release_bundle scripts.tests.test_prepare_release_metadata scripts.tests.test_verify_public_release
cargo fmt --all -- --check
(cd apps/desktop/src-tauri && cargo fmt --all -- --check)
git diff --check
```

The isolated macOS fixture is an explicit opt-in:

```sh
(cd apps/desktop/src-tauri && cargo test --locked --lib desktop_real_process_macos_verified_update_package_fixture -- --ignored --nocapture --test-threads=1)
```

It builds a disposable package using real `pkgbuild`, streams it over loopback
into a private cache, verifies hashes, expands it with real `pkgutil`, checks its
source document and proves Core rejects its incomplete executable payload. The
fixture has a failing preinstall hook to detect accidental hook execution. It
never requests authorization, invokes the system installer, calls native service
managers or touches an installed Environment. It does **not** prove a successful
privileged install, positive native prepare, service migration or rollback.

Subsequent authorized native dogfood must use isolated installation roots or
purpose-built disposable machines, with a previous updater-capable official
release and a candidate from the exact release pipeline:

| Host | Required remaining evidence |
| --- | --- |
| mini / macOS | OS prompt accept/cancel, real Core prepare and package hooks, exact launch acknowledgment, Desktop exit/reopen, service identity preservation and rollback |
| MSI / Windows x64 | Auto-download, current-user/UAC behavior, outer/inner NSIS upgrade, early bootstrap failure, Desktop exit/reopen, Server/Runner identity and rollback |
| OE / Debian or Ubuntu x64 | Graphical authorization, package dependencies/ownership, owner receipt, systemd state, Environment preservation and package/upgrade recovery |

The six-platform mapping and metadata fixtures do not substitute for native
Windows/Linux installation tests. None of these tests authorizes changing a
production Desktop, Server or Runner.
