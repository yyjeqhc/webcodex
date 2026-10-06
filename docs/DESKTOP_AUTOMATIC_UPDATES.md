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

Cache admission or invalid persisted update state can require recovery before a
pending transaction is known. These recovery errors remain visible in About and
the banner even without a newer release or after **Later**; they do not create a
pending transaction or authorize another installer. A failed automatic-download
preference save is also shown when no update is available.

The current unified release workflow uses an explicit shared Cargo target directory
before compiling the independent Desktop workspace; native binaries and macOS app
bundles are collected from that same directory. Before upload, the retained bundle
is verified according to the explicit `include_unified_installers` input:
true requires all eight installers and six source manifests; the default false
retains normal core-only validation. New durable release plans record a unified
requirement, actually dispatch that input as true, and carry the requirement
through collection, npm staging and draft verification. Build state records the
selection; a strict plan never accepts an older/core-only build state as unified
success. Low-level metadata/collection/public-verification commands expose
`--require-unified-installers` for current unified deliveries. Entirely absent
installer/source sets fail this requirement; historical read-only verification
without it and Desktop's **View release** fallback remain supported. Present but
null installer declarations are invalid, rather than legacy absence.

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

| Native OS / Rust architecture | Runtime platform | Installer targets |
| --- | --- | --- |
| Linux / `x86_64` | `linux-x64` | `linux-x64-deb`, `linux-x64-rpm` |
| Linux / `aarch64` | `linux-arm64` | `linux-arm64-deb`, `linux-arm64-rpm` |
| macOS / `x86_64` | `darwin-x64` | `darwin-x64-pkg` |
| macOS / `aarch64` | `darwin-arm64` | `darwin-arm64-pkg` |
| Windows / `x86_64` | `win32-x64` | `win32-x64-exe` |
| Windows / `aarch64` | `win32-arm64` | `win32-arm64-exe` |

`RuntimePlatform` remains the six-entry runtime/source identity. `InstallerTarget`
is the separate eight-entry package authority and includes `PackageFormat`.
On Linux, Desktop first checks whether the installed WebCodex package is owned by
dpkg or RPM; only when neither owns it does it use bounded `/etc/os-release`
`ID`/`ID_LIKE` classification. Conflicting evidence fails closed. Merely
having `rpm` installed on Ubuntu does not select RPM. Package names remain
`webcodex-unified-v<VERSION>-<PLATFORM>.<EXT>`; DEB and RPM for one Linux
runtime both bind the same `webcodex-source-v<VERSION>-<PLATFORM>.json`.

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
  <VERSION>/<INSTALLER-TARGET>/
    installer.part
    <canonical installer filename>
    manifest.json
    source-manifest.json
    expanded/                 # Unix inspection, only after explicit Install
```

State has schema version 2 and a 24 KiB bound. The frontend sees neither these
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
shell. Automatic handoff requires `windows_guarded_bootstrap_contract: 1` in the raw
Windows CLI build-info object of the existing official source manifest. Its
original object hash and CLI byte hash bind the attestation; the native release
workflow requires the outer builder and provenance to attest that same protocol,
source, run and candidate-manifest bytes. Existing installer-entry and generic
MachineBuildInfo schemas stay unchanged, so older strict updater readers still
parse the same release. Absent or unsupported capability remains manual-only;
a version number or executable probe never implies protocol support.

The current-user outer NSIS bootstrap extracts and verifies its candidate CLI.
Guarded mode keeps that candidate CLI for preparation, receipt verification,
finish and rollback, passing the selected Environment and candidate manifest
identity. It rejects fresh, legacy or unconfigured classification instead of
falling back. Manual installer invocation retains its existing routing.

A private, bounded, per-launch nonce exchange reports the operation returned by
Core under the preparing lock. Desktop persists that exact operation in the
existing pending record before acknowledging permission for the inner installer
to replace files. Finish and rollback use Core's guarded APIs with the captured
operation; the exchange adds no transaction authority. Unix preparation also
captures its operation directly rather than inferring it from a later journal.

Only an acknowledged operation permits Desktop exit. Spawn-only, timeout or
uncertain acknowledgement leaves recovery required, with no competing rollback
or automatic second attempt. Reconciliation requires the exact pending operation
plus verified installed bytes and Core commitment; a later operation for the same
candidate cannot complete this handoff. The bootstrap remains current-user;
machine-wide or foreign-owner installations are not automatically adopted.

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

Desktop requires `pkexec` plus the tools for the selected package family.
DEB targets require fixed `/usr/bin/dpkg` and `/usr/bin/dpkg-deb`; RPM targets
require fixed `/usr/bin/rpm`, `/usr/bin/rpm2cpio` and `/usr/bin/cpio`.
`pkexec` uses the graphical system authorization agent and disables its
internal terminal agent. Missing or ambiguous package-family evidence fails
closed.

DEB keeps the existing control-archive candidate path. RPM cannot safely inspect
a new package payload from `%pre`, so Desktop first queries a bounded RPM file
inventory, rejects links/special files in the canonical candidate subtree, runs
`rpm2cpio` without a shell, writes a bounded private cpio file, and invokes
`cpio` with literal argv to extract only
`/usr/share/webcodex/upgrade-candidate`. No RPM scriptlet executes during this
inspection. Core then performs the same complete candidate verification,
preflight and prepare as every other Unix package.

After preparation, the privileged helper re-verifies release provenance and the
owner receipt, copies the RPM into the root-private updater cache, freezes a
root-owned recovery candidate, and invokes exactly `/usr/bin/rpm --upgrade
<verified-rpm>`. RPM `%pre/%post` consume that prepared transaction and fail
closed if an upgrade is attempted without it. Fresh RPM installation remains a
normal package-manager operation. No `dnf` shell-out, `sudo`, `--nodeps`,
`--force`, password storage or direct Desktop writes to `/usr` are added.
Dependency resolution for manual fresh install belongs to the distribution
package manager; package-manager database repair is separate from
File/Environment rollback.

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
| Debian/Ubuntu x64 | DEB graphical authorization, dependencies/ownership, owner receipt, systemd state and package/upgrade recovery |
| Fedora x64 | RPM family detection, read-only candidate extraction, package ownership/dependencies, prepared transaction, Desktop relaunch and recovery |
| CentOS Stream 9 x64 | ABI/dependency evidence only: GLIBC 2.34 is within the numeric ceiling, but the tested repositories lack the required WebKitGTK 4.1 development stack, so Desktop support is not claimed |
| openEuler 24.03 x64 | Trusted-image/package metadata validation; full repository-backed install smoke remained blocked by repository access during bounded validation |

The six-platform mapping and metadata fixtures do not substitute for native
Windows/Linux installation tests. None of these tests authorizes changing a
production Desktop, Server or Runner.
