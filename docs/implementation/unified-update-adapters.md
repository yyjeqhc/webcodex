# Shared unified update adapters

This change gives Desktop and the management CLI one implementation for official stable release discovery, verified package download, cache admission, candidate extraction, installation classification and reconciliation. It does not introduce another installer, trust chain or upgrade transaction.

## Existing authorities retained

- `webcodex-environment::unified_update` owns official release/source manifest verification and the private installer cache. The cache remains `<resolved Desktop app-local-data root>/stable-updates-v1`; `update-state.json` remains schema 2. CLI and Desktop share its operation lock.
- `EnvironmentStore` owns one Environment root. Query helpers validate existing private storage without creating roots or setup fences. CLI callers select an explicit Environment root; a pending operation for another Environment is never retargeted.
- The existing upgrade journal owns prepare, original service state, snapshots, finish and restoration. The additional schema-1 projection contains only fixed component kinds, service scopes, bounded build/operation identities and recorded phases. It does not serialize the journal, service arguments, owner receipt, credentials or configuration bodies.
- Desktop retains notification preferences and platform graphical authorization. A host launch adapter receives only already-verified local paths; it cannot select a publisher, Environment, candidate or owner.

## State and target binding

The read-only projection preserves Prepared, Stopping, Stopped, SnapshotReady, Verifying, Committed, Restoring, RolledBack and RecoveryRequired. A recorded phase does not establish that an executor is currently active. Installer acknowledgement is distinct from Core commit and post-install file verification.

Explicit application binds the Environment ID, verified candidate manifest/package identity and selected operation. Core verifies these identities under its existing mutation lock. Existing advanced upgrade commands retain their contracts. New headless recovery admits only cases with a proof from the existing receipt/phase rules: a committed lease-release retry, an already restored retry, or initial preparation without a current-operation installer receipt or changed installed payload. Later or ambiguous handoffs require the existing manual recovery path; they do not authorize redispatch or competing restoration.

Explicit terminal owner actions can reconcile their exact cached pending operation through the existing installed-file verifier. This adapter takes existing cache and Core fences, does not provision missing directories or locks, and rejects superseding operations before cleanup. Status remains read-only.

Installed program identity and the running caller identity remain separate. Reconciliation checks the installed files against validated published component hashes. An older Desktop process is not evidence that committed installed files need rollback.

## Safety and acceptance boundary

Public status JSON is schema 1 and limited to 64 KiB. Mixed private configuration, arbitrary command output and unknown fields are not copied. A read-only status query performs no release discovery, installation, service control or storage migration.

Unified candidate validation still requires the four declared components to satisfy its existing version/source/data compatibility contract. Independent component rows do not relax that validator. Source, dirty, standalone and custom Runtime installations remain manual; the adapters do not convert installation types.

Windows target propagation through the outer NSIS installer is a separate prerequisite contribution: the existing path-only handoff cannot establish the newly selected Environment/candidate fence. This extraction does not claim that legacy Windows handoff is fenced. The Desktop UX contribution requires that prerequisite before guarded Windows application.

The subsequent Desktop and Linux CLI contributions consume these adapters separately. Linux CLI uses normal TTY sudo authorization for the narrow installed helper; non-TTY effects and macOS/Windows headless application are not admitted. Remote Runner installations are outside all local update operations.

Tests and source builds establish adapter behavior only. Real package installation, authorization, logout/reboot, service ownership and native upgrade/rollback acceptance remain recorded separately in `unified-deployment-validation.md`. This work performs no deployment or changes to existing host services.
