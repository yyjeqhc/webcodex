# Compatibility policy for the v0.5 transition

Status: development policy, checked against the published
[v0.4.6 release](https://github.com/yyjeqhc/webcodex/releases/tag/v0.4.6) on
2026-10-06. v0.4.6 resolves to
`5072608fe74b2bfa6e8f208515c04242f475582f`; the audit baseline on main was
`25af63a90ab16582e53671e481c3d3796f50b4ce`. This document does not announce a
v0.5 release or claim native upgrade acceptance.

## Rule and support boundary

Retain compatibility for supported published installations, their persisted data,
and named active external protocol consumers. Do not retain an adapter, alias,
translation, or duplicate reader solely because an unreleased main revision
previously emitted a different representation. Remove the obsolete production
path and its dedicated fixtures together, preserving coverage of surviving
authority, recovery, and data-integrity boundaries.

This rule does not preserve every old model tool name. v0.5 intentionally changes
the model/API surface: clients refresh the advertised MCP schema and use the new
canonical tools. Retired tool names and App resource versions fail closed.
Persisted installation identity, credentials, saved history, Runner wire
contracts, and negotiated Desktop management contracts are separate concerns.
See [the API transition](implementation/v05-tool-surface.md) and
[Desktop/Runtime compatibility](DESKTOP_RUNTIME_COMPATIBILITY.md).

A package version or Git revision is not a protocol generation. Keep declared
management ranges, Runner capabilities, and dated MCP protocol negotiation.
A supported older Runner missing a new capability must not receive an unsupported
operation, nor lose unrelated supported operations merely because its build differs.

## The actual stable upgrade sources

**v0.4.6 already contains Environment and is the minimum direct v0.5 upgrade source.**
Its schema-1 records, service ownership, update receipts, and Tunnel configuration
are published data contracts. v0.5 directly upgrades an existing v0.4.6-or-newer
Environment and reuses its selected owner, service scope, Runtime paths,
Runner/Project identities, private credential bindings, and recovery records.

Installations older than v0.4.6 use v0.4.6 as the bridge. Their historical
pre-Environment Linux service handoff and official Windows v0.4.3 package
classification are intentionally not reimplemented in v0.5.

The following code is retained for concrete consumers:

| Surface | Published/current consumer and reason to retain |
|---|---|
| `crates/webcodex-environment/src/types/setup_wire.rs` | v0.4.6 writes `create/join/user_create/user_join` and may omit `service_scope` or `runner`. Scope/mode conflict checks prevent a user service from being interpreted as a system service. |
| Environment storage, migration journals, upgrade receipts and rollback | Preserve the original target, owner, credentials, data format, and uncertain-effect recovery across an upgrade. A missing optional new field is not a reason to reset an installation. |
| `upgrade/windows_package.rs` | The current Windows installer still owns `Fresh`, `Unconfigured`, and `Environment` package transactions, sealed receipts and rollback. The official v0.4.3 `Legacy` classifier is no longer a v0.5 direct-upgrade path. |
| Desktop schema-1 state, backup recovery and saved Runner identity recovery | These readers shipped in v0.4.6; losing them can discard saved projects or associate an old local Runtime with a different Runner. |
| Desktop `secrets/tunnel-config.json` and its singleton-to-profile conversion | Both the schema-2 profile catalog and the older singleton reader shipped in v0.4.6. Non-persistent runtimes still use this store. |
| Environment `tunnel.json` and private per-profile `webcodex.env` | v0.4.6 records contain `profile_id/installed/started` without `host_mode`. Missing owner mode must remain standalone, not silently become embedded. |
| Workflow Session snapshots and historical context-revision decoding | Stable retained history has an actual disk consumer. Tool renames do not authorize rewriting Sessions, Jobs, replay IDs, or audit history. |
| Update metadata absence and unsupported-format handling | A release or development build without verified installer metadata must not be treated as a safe automatic installation candidate. |
| Retired-config rejection and sensitive-path exclusions | Reject retired authority/registry settings before use; do not silently select a different default. Old secret/config paths stay protected even when discovery is retired. |
| MCP dated revisions and Runner/management protocol checks | These serve external Hosts and independently upgraded components. Their lifecycle follows observed protocol consumers, not the v0.5 package number. |
| `ActionAudit`, `ActionSession`, `ActionEvent` | Current tool audit and history infrastructure. The removed GPT Actions HTTP adapter does not make this domain obsolete. |

The scope decoder and default-value coverage are in
[`types/setup_wire.rs`](../crates/webcodex-environment/src/types/setup_wire.rs).
The Windows classifications and package transaction are in
[`upgrade/windows_package.rs`](../crates/webcodex-environment/src/upgrade/windows_package.rs).
The current installer calls are in
[`build_windows_unified_bootstrap.py`](../scripts/build_windows_unified_bootstrap.py).

## Native Tunnel replacement

v0.5 uses the native Rust Tunnel implementation. Replacing the external client's
execution/downloader/doctor path does not retire the stable configuration and
service ownership that previously launched it. The external implementation was
already removed by [#914](https://github.com/yyjeqhc/webcodex/pull/914); keeping
`standalone` means a separately managed native `webcodex-tunnel` lifecycle,
not retaining the official external `tunnel-client` implementation.

For existing profiles:

1. Keep the original profile ID, Tunnel ID, private credentials and owner.
   An upgrade does not implicitly move a standalone profile into the Server.
2. In persistent mode, EnvironmentStore owns the canonical catalog. The historical
   Desktop catalog acts as a profile/Tunnel identity conflict check; it must not
   overwrite a canonical key that has been rotated independently.
3. Treat catalog disagreement as a reconciliation problem. Do not delete the
   historical file, choose an arbitrary matching profile, or clear an uncertain
   execution marker to make startup succeed.
4. Switch an existing profile to embedded only through the explicit owner-change
   flow after the previous owner is cleanly stopped and its standalone service
   uninstalled. Windows system-service ownership transfer remains unsupported.

See [Server-owned Tunnels](architecture/server-owned-tunnels.md) for exact
configuration, proxy, owner-transfer, readiness, drain and uncertainty contracts.

### A saved-only v0.4.6 Desktop profile needs explicit reconciliation

In v0.4.6, saving a Desktop profile can precede its successful Environment
configuration. A legitimate older Desktop catalog can therefore contain a profile
missing from `tunnel.json`. Current persistent-mode Desktop reports
`tunnel_catalog_conflict` for that mismatch. The conflict check is
intentional; it is not proof that the old file is disposable or that credentials
should be copied automatically.

The existing CLI can configure the missing profile in the **same** Environment
using its **original** profile ID and Tunnel credentials:

```text
webcodex environment configure-tunnel ORIGINAL_PROFILE \
  --environment-dir ORIGINAL_ENVIRONMENT \
  --host standalone \
  --credentials-file PROTECTED_CREDENTIALS_JSON
```

Use a private JSON containing `tunnel_id` and `api_key`, or the command's hidden
input. Never put the API key in arguments or logs. This command installs and
starts the standalone service; it is **not a save-only import**. Run it only
when that lifecycle action is intended. Existing conflicting bindings are
rejected, not overwritten. A profile must use its original identity to satisfy
the Desktop conflict check; choosing a new profile name leaves the old mismatch.
Reconcile every missing profile by its original ID before expecting the complete
Desktop catalog to become available again.

Choosing `--host embedded` instead is an explicit new owner decision, with its
own platform and lifecycle preconditions. It is not a universal migration
shortcut. A dedicated save-only reconciliation flow is not implemented here.

## Removed unreleased compatibility

The old `read_pdf_chunk` App-only tool was introduced in
[#902](https://github.com/yyjeqhc/webcodex/pull/902) and superseded by
`read_app_artifact_chunk` in
[#904](https://github.com/yyjeqhc/webcodex/pull/904), on unreleased main. It is absent
from the v0.4.6 source tag. Current PDF, DOCX and spreadsheet App code all calls
the generic reader, so the old registry entry, parser variant, dispatch path,
PDF-only implementation and projection branches are removed together.

Keep the generic reader's exact grant, Project/path/size/SHA binding, bounded
range reads, stale-content rejection, and App-only admission. The Work Result
PDF metadata path (`work_result_files` / `webcodex/pdfChunk`) remains a current consumer;
its similar name does not make it part of the retired tool.

Earlier completed removals are recorded in
[v0.5 pruning](implementation/v05-pruning.md): historical model aliases, App URI
aliases, old authority-setting translation, obsolete startup discovery, and
the GPT Actions/OpenAPI adapter. Do not recreate them to accommodate a stale
cached Host schema. Reopen the reader or reconnect/refresh the MCP schema.

## Future removal criteria

The v0.5 support floor is explicit: v0.4.6 is the bridge for older installations.
The removed Linux migration commands and Windows v0.4.3 classifier must not be
recreated as convenience fallbacks.

Future support-floor changes should likewise name a published bridge before
removing readers or migrations. Current `Fresh/Unconfigured/Environment` Windows
transactions, sealed receipts, owner/path checks and rollback remain installation
infrastructure rather than legacy-version support.
- Keep reject-before-use guards where silently ignoring old settings could
  change authority, execution target, registry selection, or retained data.
- Retire a dated MCP protocol only after identifying its actual remaining Host
  consumers and making a separate protocol-support decision.

Validate the smallest changed public boundary and use native upgrade acceptance
when changing installer, service, ACL, process-owner or credential behavior.
A parser unit test or Linux check cannot establish successful Windows/macOS
installation, reboot recovery, or a live Host schema refresh.
