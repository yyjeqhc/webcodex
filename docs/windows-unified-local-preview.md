# Windows Unified Local Preview (#1394)

This is an **unsigned Windows x64 test installer** built locally or by the
dedicated GitHub Actions Preview workflow, not an official WebCodex release. It exists to validate the official v0.4.3 Desktop -> v0.5
migration **before** any v0.5.1 publication. Never present it as a published
upgrade or use it without an independent recovery backup.

## Build on Windows x64 (no CI required)

Requirements: Windows x64, Rust/Cargo, Node.js 22, Python 3.12 (py -3.12),
NSIS 3.13 (makensis.exe), 7-Zip (7z.exe, including Chocolatey's bundled
copy), and a **clean committed** WebCodex checkout.
From the repository root on fix/1394-windows-v043-upgrade:

    .\scripts\build_windows_unified_preview.ps1

The script recompiles all four native components with one exact committed Git
SHA, enables local-preview-installer **only for the candidate CLI**, collects
four verified build-info records and SHA-256 digests, stages the Tauri payload,
then **extracts and hashes the exact installed files from the completed inner
Tauri NSIS package** before creating the final Source Manifest and outer NSIS
Bootstrap. Tauri changes Desktop PE resource bytes during bundling; the
pre-bundle executable hash must not be used as the installed Desktop hash.
It **does not run** any installer or
stop processes. If rerunning for the same commit, explicitly inspect and remove
target\unified-preview\<12-char-commit> before rebuilding.

Output files (inside target\unified-preview\<12-char-commit>):

    webcodex-unified-LOCAL-PREVIEW-<sha12>-v0.5.0-win32-x64.exe
    webcodex-unified-LOCAL-PREVIEW-<sha12>-v0.5.0-win32-x64.exe.sha256
    webcodex-unified-LOCAL-PREVIEW-<sha12>-v0.5.0-win32-x64.exe.provenance.json
    candidate-final\source-manifest.json
    candidate-final\SHA256SUMS

The manifest declares local_preview=true, workflow reference
webcodex/local-preview, workflow run ID 0, and local-preview-host for each
component probe. These values do **not** pretend to be CI provenance.

## GitHub Actions CI Preview (no Release or tag)

The opt-in workflow [Windows Unified LOCAL PREVIEW](../.github/workflows/windows-unified-preview.yml)
can build an unsigned Windows x64 acceptance candidate on GitHub-hosted Windows.
Before this workflow is merged into the default branch, it is triggered only
by same-repository updates to `fix/1394-windows-v043-upgrade` / PR #1405.
After merging, a maintainer can use **Actions -> Windows Unified LOCAL PREVIEW ->
Run workflow** to build the default branch manually. No tag, GitHub Release,
installer signing or deployment is performed.

Open the successful workflow run under **Actions**, find **Artifacts**, and
select the `windows-unified-LOCAL-PREVIEW-<run-id>-<attempt>` ZIP. The ZIP
contains the unsigned installer, SHA-256 checksum, local source/provenance
manifests and a testing warning. Artifacts expire after **14 days** and
may require a GitHub sign-in to download. Check the workflow run's exact
source SHA and verify the installer against its `.sha256` file before testing.
The dedicated CI job checks its repository, workflow/event identity and exact
checked-out PR-head (or main-branch) source SHA **before compiling**. The
manifest deliberately retains the local-preview markers, while the Artifact
README records the actual GitHub run and checksum; it never borrows release
provenance. GitHub hosting does **not** convert an unsigned Preview into an
official published upgrade. The real-release path still requires a separately
published, source-verified build and review.

## Security boundary

A normal WebCodex/CLI build cannot accept a local-preview manifest. The
feature-built candidate CLI requires an explicit --local-preview option on
package-upgrade-preflight, package-upgrade-prepare and package-upgrade-verify.
The local-only path preserves identity matching, complete component digests,
ownership and running-process checks, backup receipts, installed-hash
verification, and rollback. Finish/rollback recover the original journal
without accepting another candidate. Normal published source verification
remains mandatory for official installation transactions.

These self-asserted local SHA-256 hashes provide **integrity**, not
**publisher authentication**. Only use binaries you compiled yourself on a
trusted machine. Do not distribute Preview builds as official releases.

## Before running the installer

1. Confirm that the old CLI, Server and Runner are official Windows v0.4.3
   (Git SHA b96a59a712ca) and the target belongs to the current user.
2. Make a **consistent** Server SQLite backup (stop writers or use SQLite's
   backup API); preserve the installed program directory, Desktop state, Runner
   config/registries, Project IDs and Tunnel credentials. Verify recovery.
3. Record Runner client_id, registered Project IDs, Tunnel profile IDs, Server
   history row counts, and installed binary hashes. Save a sanitized baseline
   outside the installation directory before beginning the upgrade.
4. Close the v0.4.3 Desktop and stop its Server/Runner/Tunnel processes. Do not
   stop unrelated instances. If another tool client shares the Desktop Runner
   identity, its connection will drop during the upgrade. Use an independent
   Runner or other out-of-band control channel for recovery.
5. Launch the LOCAL PREVIEW .exe, acknowledge its explicit warning, and check
   that it selects the existing install directory. Do not use silent install
   mode for the first acceptance test.
6. Reopen Desktop using its **saved** runtime; do not enroll new identities.
   Reconnect the original Runner and, if offered, complete the Environment import.
   Compare Client/Project/Tunnel IDs, credentials and history before and after a
   restart, and confirm that the Server reports the original Runner as online.
7. On failure, retain windows-package-upgrade.json,
   windows-package-prepared.json and upgrade-backups/ intact. Binary rollback
   does not automatically reverse Desktop data imports or recover live Jobs.

A passing local build, unit test or installer-classify result is **not** native
end-to-end upgrade acceptance. The full path remains to be tested on Windows.
