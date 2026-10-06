# Guarded native installer target handoff

Source inspection of `upstream/main` `ed22e5c0` found that the Windows outer
bootstrap received only an Environment directory. It could not recheck the
Environment ID selected by the UI if the saved record changed before prepare.
The legacy bootstrap also did not acknowledge the exact operation to Desktop.
This gap was inferred from source; no real Windows race was reproduced.

The repair keeps the existing installer, candidate verifier and Core journal.
An ephemeral private request binds the selected Environment and candidate to a
launch nonce. The candidate CLI invokes guarded Core preparation and returns
the receipt captured under that preparing lock. Desktop records that exact
operation before acknowledging that replacement may begin. Guarded verification,
finish and rollback recheck the same selected operation. An unconfirmed exchange
retains pending/manual recovery and never falls back to a fresh installation.

The Windows CLI's additive raw build-info attestation is hashed by the existing
source manifest and checked against same-run bootstrap provenance. It is not a
new release manifest field or trust chain. Old strict installer-manifest readers
retain their original schema; old Windows packages without an admitted bootstrap
contract remain manual-only for the new guarded updater. Existing manual install
commands retain their contracts.

The shared Unix adapter also consumes the receipt from the exact preparing
invocation instead of learning an operation from a later journal observation.
The separate Desktop and Linux CLI contributions depend on this repair. No
persisted transaction phase, identity, ownership or database recovery rule changes.

## Actual verification boundary

Recorded 2026-10-06 with source baseline `ed22e5c0` and shared prerequisite
`40d927db`. Focused Rust and packaging-script tests use disposable fixtures on
Linux x64. They exercise target/nonce/operation mismatches, fail-closed unknown
results, bounded exchange files, legacy metadata compatibility and
source/provenance binding.

| Check | Actual result |
| --- | --- |
| Environment `unified_update`, after shared integration | 58 passed with `--locked --offline --profile dogfood`. |
| Environment `upgrade::status` | 11 passed with `--locked --offline --profile dogfood`, including the receipt captured by the preparing invocation. |
| CLI `guarded_` | 3 passed, including additive build-info compatibility and rejection without creating the selected Environment. |
| Existing Environment `upgrade::tests` | 9 passed after integration: original owner/receipt, exact payload restore and data recovery boundaries. |
| Six related packaging-script suites | 74 tests: 73 passed and one existing PowerShell-host skip. Covers Windows installer preparation, release metadata, download page, bundle collection, unified input collection and unified package validation. |
| Desktop native installer adapter, integrated consumer | 4 passed, including literal Windows target-file arguments. |
| Rust formatting and diff-whitespace checks | Passed. |

A Windows MSVC cross-check was blocked in the dependency build by missing
`lib.exe`/Windows SDK tools. There is no NSIS compiler or authorized Windows
installation target on this host. No native EXE installation, authorization,
service operation, logout/reboot or real upgrade/rollback was performed. Those
acceptance rows remain pending in `unified-deployment-validation.md`.
