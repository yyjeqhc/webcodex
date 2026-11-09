# Windows Unified Local Preview (#1394)

This is a **locally built, unsigned, Windows x64 test installer**, not an official
WebCodex release. It exists to validate the official v0.4.3 Desktop -> v0.5
migration **before** any v0.5.1 publication. Never present it as a published
upgrade or use it without an independent recovery backup.

## Build on MSI (no CI required)

Requirements: Windows x64, Rust/Cargo, Node.js 22, Python 3.12 (py -3.12),
NSIS 3.13 (makensis.exe), 7-Zip (7z.exe, including Chocolatey's bundled
copy), and a **clean committed** WebCodex checkout.
From E:\git\webcodex on fix/1394-windows-v043-upgrade:

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
   history row counts, and installed binary hashes. On MSI a non-secret baseline
   is already stored under target\issue1394-validation\baseline.
4. Close the v0.4.3 Desktop and stop its Server/Runner/Tunnel processes. Do not
   stop unrelated instances. On MSI the separate Windows app shares the old
   Desktop Runner identity msi-80028ec57388455c, so its connection may drop.
   The independent client_id=msi Runner can provide a control channel.
5. Launch the LOCAL PREVIEW .exe, acknowledge its explicit warning, and check
   that it selects the existing install directory. Do not use silent install
   mode for the first acceptance test.
6. Reopen Desktop using its **saved** runtime; do not enroll new identities.
   Reconnect the original Runner, complete the Environment import and compare
   Client/Project/Tunnel IDs, credentials and history. Restart and check again.
7. On failure, retain windows-package-upgrade.json,
   windows-package-prepared.json and upgrade-backups/ intact. Binary rollback
   does not automatically reverse Desktop data imports or recover live Jobs.

A passing local build, unit test or installer-classify result is **not** native
end-to-end upgrade acceptance. The full path remains to be tested on Windows.
