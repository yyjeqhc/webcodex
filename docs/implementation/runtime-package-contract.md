# Full and Linux Runtime update contract

`PackageFlavor` describes installation contents, not an Environment role.
Full retains Desktop, CLI, Server and Runner; Runtime is Linux-only and contains
CLI, Server and Runner. Installing either package does not configure services.

Full's legacy source schema1, eight targets and four-component validation remain
strict. Runtime requires source schema2 with an explicit flavor, exactly three
components at their canonical candidate paths, matching native build/provenance
and no Desktop payload. Missing Desktop is never Runtime evidence.

Runtime qualification reads exact installed DEB/RPM package identity, fixed
root-protected package-owned programs and provenance. Preflight/prepare reject
flavor conversion before stopping services. Runtime receipts and frozen owner
handoffs bind the exact installer target, including package manager. Runtime
package hooks must explicitly supply that target; legacy Full invocation and
serialized receipts remain unchanged. Privileged dispatch rechecks current
package identity and layout before authorization and again before spawning.
Unknown/ambiguous identity fails closed. No package or service is adopted.

DEB hook verification admits dpkg's `half-installed` preinst and
`half-configured` postinst states while retaining exact package ownership and
candidate checks. Ordinary assessment, prepare, authorization and privileged
launch still require `installed`. Runtime same-package hooks also supply the
exact installer target; identical payload bytes do not permit changing package
manager.

Runtime recovery journals use schema2 and journal-only `runtime_<phase>` values;
internal phases/recovery semantics remain existing Core values. Old readers with
the closed legacy phase enum reject every Runtime journal instead of treating a
Desktop-less journal as an old Full transaction. Full journal wire bytes remain
unchanged. All reads/writes use the same codec and duplicate-field rejection.

A release declaring `manifest-v2.json` must supply its strict twelve-target
catalog and the deterministic Full compatibility view. New readers verify both
views/checksums; failure never falls back. Releases without v2 permit only the
legacy Full path. The existing 32-line SHA256SUMS bound is unchanged. Cache keys,
pending reconciliation, receipts and candidate projections retain flavor.

Shared status can display a verified installed target with its component set;
unknown probes retain unknown Full diagnostic rows rather than inventing Runtime.
Candidate identity is independently projected and a mismatched target cannot be
marked installable. These observations do not supply installation authority.

## Validation

Focused Linux dogfood tests verify source/catalog compatibility, exact component
bytes, Runtime receipt/hook target fences, every journal phase, cached identities,
private state bounds and existing upgrade/recovery behavior. CLI compiles without
Desktop. Counts and final integrated checks are recorded in the PR description.

The package-hook candidate relocation prerequisite is separately reviewed.
No real package installation, sudo interaction, logout/reboot, release or existing
service operation was performed. Windows/macOS Runtime packages are unsupported;
native Full regression acceptance remains the existing platform matrix.

The integrated public CLI entry test reproduced a stack overflow in the default
unoptimized CI profile; optimized dogfood tests had passed. The adapter now boxes
its large internal domain future, retaining the same command/authorization path.
The same default-profile public status test passes with the normal thread stack;
no stack-size override, timeout increase or weaker test was used. This correction
is a follow-up commit in this feature PR, alongside the existing dispatch and
Windows private-fixture corrections, rather than another contribution.

After integrating upstream `8719afd7`, 73 shared update tests, 38 upgrade
contract tests and 25 CLI Environment tests passed. The default unoptimized
public status regression also passed with the normal thread stack. All 13
Desktop native update-adapter tests and both Rust formatting checks passed.
The merge preserves the new Runner-name option, real help newlines and explicit
Runtime installer target. These are automated/source checks, not native
installation or service acceptance. CI repairs remain follow-up commits in
this feature PR; the duplicate repair PRs #939/#940 are closed.
