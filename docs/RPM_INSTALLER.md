# RPM-family unified installer

WebCodex publishes RPM as a first-class unified installer format for the existing
Linux x64 and Linux arm64 runtime identities. RPM support does not add a new
runtime platform: runtime archives and source manifests remain the six canonical
platforms, while package selection is an independent eight-target model.

## Published targets

| Runtime | DEB target | RPM target | RPM architecture |
| --- | --- | --- | --- |
| `linux-x64` | `linux-x64-deb` | `linux-x64-rpm` | `x86_64` |
| `linux-arm64` | `linux-arm64-deb` | `linux-arm64-rpm` | `aarch64` |

Both package formats for one Linux runtime are built from the same verified
native candidate and bind the same
`webcodex-source-v<VERSION>-<PLATFORM>.json`. The release contract therefore
contains six runtime archives, six source manifests, and eight unified
installers.

RPM filenames are:

- `webcodex-unified-v<VERSION>-linux-x64.rpm`
- `webcodex-unified-v<VERSION>-linux-arm64.rpm`

RPM package metadata currently requires a canonical plain `X.Y.Z` release version; packaging fails closed rather than silently rewriting SemVer prerelease/build metadata.

The RPM is currently **unsigned**. Release verification records this explicitly
with `rpm --checksig` and still requires GitHub release identity,
`manifest.json`, `SHA256SUMS`, source-manifest provenance and package SHA-256.

## Installed layout

RPM and DEB intentionally install the same Linux layout:

```text
/usr/lib/webcodex/webcodex-desktop
/usr/lib/webcodex/webcodex-runtime/webcodex
/usr/lib/webcodex/webcodex-runtime/webcodex-server
/usr/lib/webcodex/webcodex-runtime/webcodex-runner
/usr/bin/webcodex -> ../lib/webcodex/webcodex-runtime/webcodex
/usr/bin/webcodex-server -> ../lib/webcodex/webcodex-runtime/webcodex-server
/usr/bin/webcodex-runner -> ../lib/webcodex/webcodex-runtime/webcodex-runner
/usr/share/applications/webcodex.desktop
/usr/share/doc/webcodex/unified-source-manifest.json        # human-facing copy; may be skipped by nodocs
/usr/share/webcodex/unified-source-manifest.json            # machine-readable RPM provenance
/usr/share/webcodex/upgrade-candidate/...
```

The non-doc source-manifest copy avoids RPM-family `nodocs` transaction policies removing machine-readable provenance. The final path is package payload used only to reconstruct a read-only candidate
before an RPM upgrade. It is not a second runtime or Environment.

## Build and static inspection

`scripts/package_unified_installer.py` uses the system `rpmbuild`; WebCodex
does not implement the RPM container format. A release Linux job compiles
Desktop/runtime once, creates one verified candidate, and emits both DEB and RPM
from that candidate.

Review an RPM without installing it:

```sh
rpm -qpi webcodex-unified-v<VERSION>-linux-x64.rpm
rpm -qpl webcodex-unified-v<VERSION>-linux-x64.rpm
rpm -qp --requires webcodex-unified-v<VERSION>-linux-x64.rpm
rpm -qp --scripts webcodex-unified-v<VERSION>-linux-x64.rpm
rpm --checksig webcodex-unified-v<VERSION>-linux-x64.rpm
```

Do not use `--nodeps` or `--force` to make a package appear compatible.

## Fresh install and upgrade authority

A fresh installation may be performed by the distribution package manager, for
example:

```sh
sudo dnf install ./webcodex-unified-v<VERSION>-linux-x64.rpm
```

"Fresh" means more than the RPM database having no `webcodex` package. The RPM
preinstall script also rejects canonical WebCodex Desktop/runtime/symlink/service
paths left by a DEB, manual, or legacy installation. This prevents a first RPM
transaction from overwriting an existing Environment outside the prepared upgrade
authority. Migrate/remove that installation or upgrade through its existing package
family instead.

Package installation itself does not auto-enable Server or Runner. Environment
configuration and service lifecycle remain owned by `webcodex environment`.

An **upgrade is intentionally stricter**. A direct `rpm -U` or `dnf upgrade`
of an already installed WebCodex package is not allowed to invent an upgrade
transaction. RPM `%pre` requires the owner receipt/same-package marker and the
root-owned recovery candidate prepared by the Desktop/Core update path; without
that state it fails closed. This restriction prevents RPM from overwriting the
managed runtime before Core has captured service and Environment state.

The automatic path is:

1. Select the exact RPM installer target from installed package provenance, with
   bounded `/etc/os-release` fallback only if no package manager owns WebCodex.
2. Re-establish release/manifest/checksum/source provenance.
3. Inspect the RPM file list and reject unsafe candidate entries.
4. Run fixed `/usr/bin/rpm2cpio`; stream the bounded result to private cache.
5. Run fixed `/usr/bin/cpio` with literal argv to extract only the canonical
   candidate subtree, without running RPM scriptlets.
6. Run existing Core verify/preflight/prepare as the original owner.
7. Re-verify in the privileged helper, freeze both RPM bytes and the recovery
   candidate under root-owned private storage.
8. Invoke fixed `/usr/bin/rpm --upgrade <verified-rpm>`.
9. Let `%pre/%post` verify/finish the same prepared transaction; next launch
   reconciles committed, rolled-back or uncertain state.

## Distribution scope

The RPM-family selector recognizes Fedora, RHEL/CentOS-family distributions,
Rocky/AlmaLinux and openEuler identities. Browser download UI never guesses a
Linux package family: users choose DEB or RPM explicitly.

Packaging smoke is expected on Fedora and openEuler. CentOS Stream 9 is an
ABI/dependency reality check, not a supported Desktop target in this change:
its glibc floor is 2.34 while the current Ubuntu 22.04 Desktop build gate allows
GLIBC up to 2.35, and the tested Stream 9 repositories expose
`webkit2gtk3(-devel)` but not the required `webkit2gtk4.1(-devel)` stack.
Therefore the RPM package format may be inspectable there, but WebCodex Desktop
support is blocked until a maintained EL9-compatible Desktop build baseline is
established. No verifier is weakened and no `--nodeps` workaround is used.

The current OE validation used the official `openeuler/openeuler:24.03-lts`
image. The image was retrieved successfully and reports x86_64 RPM 4.18.2, but
its base image does not contain the newly declared `cpio` or `polkit`
dependencies, and repository-backed `dnf install` did not complete within the
bounded validation window. No `--nodeps` or third-party image was substituted,
so a full openEuler install/uninstall smoke remains unproven in this change.

OpenRuyi and RISC-V are **not** claimed as supported runtime targets. The RPM
abstraction is reusable groundwork, but adding RISC-V requires its own runtime
platform, native build/provenance lane, ABI validation and installer target.
