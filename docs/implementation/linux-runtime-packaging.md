# Linux Runtime packages and release metadata

Linux x64/arm64 DEB and RPM packaging gains an explicit `runtime` flavor. Full
remains the default: source schema 1 contains Desktop, CLI, Server and Runner.
Runtime source schema 2 requires `package_flavor: "runtime"`, exactly the three
CLI/Server/Runner artifacts, and no Desktop payload. An absent Desktop file or a
newer version never selects Runtime.

The `runtime-native` release job downloads the same-run native archive, verifies
its checksum/member set/ELF baseline, probes those exact bytes on the matching
architecture, and packages them. It does not rebuild runtime or compile Desktop,
and installs no GTK/WebKit build dependencies. Source manifests retain native
build-info, component hashes, source commit and workflow identity. Metadata
assembly, collection and public verification compare Runtime component hashes
with the unchanged native archive bytes.

## Package ownership

Runtime is `webcodex-runtime`; Full remains `webcodex`. DEB/RPM metadata declares
reciprocal `Conflicts`, with no `Replaces`, `Obsoletes` or package conversion.
Both use `/usr/lib/webcodex/webcodex-runtime` and the existing three `/usr/bin`
links. Runtime has no Desktop executable, application launcher or GTK/WebKit
runtime dependencies.

Runtime DEB provenance is
`/usr/share/doc/webcodex-runtime/unified-source-manifest.json`; Runtime RPM
provenance/candidate are under `/usr/share/webcodex-runtime`. Root-protected
recovery remains `/var/lib/webcodex-installer/recovery/candidate`. Maintainer
scripts retain the manifest-bound candidate CLI check, installation detection,
owner receipt, same-package marker and guarded finish. Fresh installation does
not start services; failed owner finish retains authorization/recovery evidence.
Runtime verification hooks, including same-package checks before and after
installation, pass their literal `linux-{x64|arm64}-runtime-{deb|rpm}` installer
target to Core. Core binds authorization to the schema-2 receipt and verifies
the same package manager and flavor for same-package checks. Full hooks retain
their existing argument contract.

RPM packages preserve the already measured binary bytes, including their retained
recovery candidates. RPM buildroot stripping and debug rewriting are disabled;
dependency generation and the guarded maintainer scripts remain enabled. The
native Runtime packaging job builds and extracts disposable Full/Runtime RPM
fixtures to verify that installed and retained bytes match their input hashes.
Core's flavor-aware candidate/receipt and installed-package ownership changes
are prerequisites; Python packaging alone does not authorize a package switch.

## Release selection and compatibility

Low-level dispatch requires both
`build-start --include-unified-installers --include-runtime-installers`.
`include_runtime_installers` defaults to false and requires Full selection.
The Runtime workflow requires a canonical `vX.Y.Z` release tag, consistent with
the existing RPM version contract; verification tags are unsupported for this
optional flavor. No release tag or workflow was invoked locally.

High-level `release-init --require-runtime-installers` persists the selection
alongside the existing Full requirement. Build-state schema 3 and plan-state
schema 4 retain it on resume. Older schemas decode Runtime as false while
preserving their stored Full selection. Collection, npm staging, draft checks
and public verification accept `--require-runtime-installers` together with
`--require-unified-installers`. Missing, partial, extra or inconsistent material
fails closed; Runtime artifacts cannot enter metadata assembly without selection.

| Item | Contract |
| --- | --- |
| Target | `linux-{x64\|arm64}-runtime-{deb\|rpm}` |
| Package | `webcodex-runtime-vVERSION-PLATFORM.FORMAT` |
| Source | `webcodex-runtime-source-vVERSION-PLATFORM.json` |
| Canonical manifest | `manifest-v2.json`, schema 2, six native archives/twelve installer targets; only four Runtime entries have `flavor: "runtime"` |
| Legacy view | `manifest.json`, generated from v2, unchanged six archives/eight Full targets/six Full sources; no new schema/flavor fields |
| npm | Uses the legacy view and the same three retained native binary archive bytes |

Both manifests are checksummed and must agree with the deterministic projection.
A malformed or mismatched present v2 never falls back to the legacy view.
Duplicate JSON fields and conflicting source digests are rejected.

The complete primary release has exactly **32 SHA256SUMS records**: six archives,
three primary Desktop distributions, twelve installers, eight sources, both
installer manifests and the existing runtime compatibility manifest. Sidecars,
provenance/ELF evidence and supplemental Desktop/server publication assets remain
outside this contract. The retained ZIP cap is 36 entries: those 32 assets plus
`SHA256SUMS`, `release-build.json` and two ELF reports. Drafts admit at most 33
assets including `SHA256SUMS`. Exact-set checks remain mandatory. Historical
Full/core-only checksum sets remain supported.

## Validation and remaining acceptance

Focused tests cover collection without Desktop, flavor fences, synthetic DEB
contents, RPM metadata/scripts, provenance versus archive bytes, twelve-target
collection, checksum bounds, old Full manifest readers, tampering/no fallback,
and persisted selection/migration. Existing Full, core-only, Windows guarded
handoff, macOS evidence and publication regressions run together. Workflow YAML
and embedded Bash syntax are checked separately.

The synthetic DEB test builds/inspects disposable bytes without installation.
RPM rendering, provenance fixtures and Python tests do not establish native
installation or owner-context upgrade acceptance. Real x64/arm64 installation,
upgrade/rollback and service behavior require reviewed native CI/acceptance
runs. This contribution is not a release, deployment or installation change.

After merging the contract branch with upstream `8719afd7`, the five Runtime,
metadata, release-plan, collection and publication suites ran 81 tests, all
passing. Repository-local Markdown links (772) and diff formatting passed.
The earlier 464-test tooling run remains tied to its earlier source; native
package installation and release publication remain unexecuted. CI fixes are
commits in the existing dependent feature PRs, not separate prerequisite PRs.
