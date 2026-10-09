//! Explicit installed Linux package identity. Absence of Desktop is never an
//! installation classifier. Queries are bounded, read-only and use literal argv.
use super::super::{InstallerTarget, PackageFlavor};
#[cfg(target_os = "linux")]
use super::super::{PackageFormat, RuntimePlatform};
#[cfg(target_os = "linux")]
use crate::process::CommandOutputExt;
use crate::RuntimeBinaries;
#[cfg(target_os = "linux")]
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// Normal assessment and privileged dispatch require a configured package.
/// Only the explicit package-hook verifier admits dpkg's in-progress states;
/// candidate bytes, exact ownership and target checks still apply there.
#[derive(Clone, Copy)]
pub(crate) enum PackageInspection {
    Installed,
    InstallerHook,
}

#[cfg(target_os = "linux")]
fn deb_status_matches(status: &[u8], inspection: PackageInspection) -> bool {
    status == b"installed"
        || matches!(inspection, PackageInspection::InstallerHook)
            && matches!(status, b"half-installed" | b"half-configured")
}

#[cfg(target_os = "linux")]
fn query(program: &str, arguments: &[&std::ffi::OsStr]) -> Option<Vec<u8>> {
    let output = std::process::Command::new(program)
        .args(arguments)
        .output_with_limits(Duration::from_secs(2), 4096)
        .ok()?;
    output.status.success().then_some(output.stdout)
}
#[cfg(target_os = "linux")]
pub(crate) fn installed_package() -> Option<InstallerTarget> {
    installed_package_for(PackageInspection::Installed)
}

#[cfg(target_os = "linux")]
fn installed_package_for(inspection: PackageInspection) -> Option<InstallerTarget> {
    let platform = RuntimePlatform::current()?;
    let mut found = Vec::new();
    for (name, flavor) in [
        ("webcodex", PackageFlavor::Full),
        ("webcodex-runtime", PackageFlavor::Runtime),
    ] {
        if query(
            "/usr/bin/dpkg-query",
            &[
                "-W".as_ref(),
                "-f=${db:Status-Status}".as_ref(),
                name.as_ref(),
            ],
        )
        .as_deref()
        .is_some_and(|status| deb_status_matches(status, inspection))
        {
            found.push(InstallerTarget {
                platform,
                format: PackageFormat::Deb,
                flavor,
            });
        }
        if query(
            "/usr/bin/rpm",
            &["-q".as_ref(), "--quiet".as_ref(), name.as_ref()],
        )
        .is_some()
        {
            found.push(InstallerTarget {
                platform,
                format: PackageFormat::Rpm,
                flavor,
            });
        }
    }
    (found.len() == 1).then(|| found[0])
}
#[cfg(target_os = "linux")]
pub(super) fn provenance_path(target: InstallerTarget) -> Option<PathBuf> {
    let package = if target.flavor.is_full() {
        "webcodex"
    } else {
        "webcodex-runtime"
    };
    match target.format {
        PackageFormat::Deb => Some(PathBuf::from(format!(
            "/usr/share/doc/{package}/unified-source-manifest.json"
        ))),
        PackageFormat::Rpm => Some(PathBuf::from(format!(
            "/usr/share/{package}/unified-source-manifest.json"
        ))),
        _ => None,
    }
}
#[cfg(target_os = "linux")]
fn root_protected(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    path.ancestors().all(|ancestor| {
        std::fs::symlink_metadata(ancestor).is_ok_and(|m| {
            !m.is_symlink()
                && m.uid() == 0
                && m.mode() & 0o022 == 0
                && (ancestor != path || m.is_file())
        })
    })
}
#[cfg(target_os = "linux")]
pub(crate) fn owned_layout(target: InstallerTarget, binaries: &RuntimeBinaries) -> bool {
    let Some(provenance) = provenance_path(target) else {
        return false;
    };
    let root = Path::new("/usr/lib/webcodex/webcodex-runtime");
    let paths = [&binaries.cli, &binaries.server, &binaries.runner];
    if paths
        .into_iter()
        .zip(super::super::RUNTIME_BINARIES)
        .any(|(p, n)| p != &root.join(n) || !root_protected(p))
        || !root_protected(&provenance)
    {
        return false;
    }
    let package = if target.flavor.is_full() {
        "webcodex"
    } else {
        "webcodex-runtime"
    };
    let desktop = target
        .flavor
        .is_full()
        .then(|| PathBuf::from("/usr/lib/webcodex/webcodex-desktop"));
    if desktop.as_ref().is_some_and(|p| !root_protected(p)) {
        return false;
    }
    let owned = paths
        .into_iter()
        .chain([&provenance])
        .chain(desktop.as_ref())
        .all(|path| match target.format {
            PackageFormat::Deb => query("/usr/bin/dpkg-query", &["-S".as_ref(), path.as_os_str()])
                .is_some_and(|bytes| {
                    std::str::from_utf8(&bytes)
                        .is_ok_and(|s| s.trim_end() == format!("{package}: {}", path.display()))
                }),
            PackageFormat::Rpm => {
                query(
                    "/usr/bin/rpm",
                    &[
                        "-qf".as_ref(),
                        "--qf".as_ref(),
                        "%{NAME}".as_ref(),
                        path.as_os_str(),
                    ],
                )
                .as_deref()
                    == Some(package.as_bytes())
            }
            _ => false,
        });
    owned
}
/// Runtime admission requires installed package ownership and fixed root-protected
/// destinations. Full's existing development path is unchanged, but a known
/// installed Runtime can never be prepared as Full.
pub(crate) fn verify_candidate_flavor(
    flavor: PackageFlavor,
    binaries: &RuntimeBinaries,
    inspection: PackageInspection,
) -> crate::SetupResultValue<Option<InstallerTarget>> {
    let bad = || {
        crate::SetupDiagnostic { code: "upgrade_package_flavor".into(), message: "The candidate package flavor differs from the verified installed package; installation types cannot be converted by an update".into(), recovery: "Use the existing manual installation path for a different package flavor".into() }
    };
    #[cfg(target_os = "linux")]
    {
        let managed = binaries.cli == Path::new("/usr/lib/webcodex/webcodex-runtime/webcodex");
        if !flavor.is_full() || managed {
            let installed = installed_package_for(inspection).ok_or_else(bad)?;
            if installed.flavor != flavor || !owned_layout(installed, binaries) {
                return Err(bad());
            }
            return Ok(Some(installed));
        }
        Ok(None)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (binaries, inspection);
        if flavor.is_full() {
            Ok(None)
        } else {
            Err(bad())
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn installed_package() -> Option<InstallerTarget> {
    None
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn owned_layout(_: InstallerTarget, _: &RuntimeBinaries) -> bool {
    false
}

/// Runtime hooks bind even same-version verification to the package manager.
/// Legacy Full hooks omit the target; absence never authorizes Runtime.
pub(crate) fn verify_candidate_target(
    flavor: PackageFlavor,
    observed: Option<InstallerTarget>,
    expected: Option<InstallerTarget>,
) -> crate::SetupResultValue<()> {
    let matches = match expected {
        None => flavor.is_full(),
        Some(target) => {
            target.valid()
                && target.flavor == flavor
                && Some(target.platform) == super::super::RuntimePlatform::current()
                && if cfg!(target_os = "linux") {
                    observed == Some(target)
                } else {
                    flavor.is_full()
                }
        }
    };
    if matches {
        return Ok(());
    }
    Err(crate::SetupDiagnostic {
        code: "installer_package_target".into(),
        message: "The package hook does not match the installed flavor and package manager".into(),
        recovery: "Use the installer for the existing package target".into(),
    })
}

/// Re-read package identity at the privileged effect boundary, including the
/// package manager. Saved receipts never authorize an installation conversion.
#[cfg(target_os = "linux")]
pub(crate) fn verify_installed_target(
    target: InstallerTarget,
    binaries: &RuntimeBinaries,
) -> crate::SetupResultValue<()> {
    if installed_target_matches(target, installed_package(), owned_layout(target, binaries)) {
        return Ok(());
    }
    Err(crate::SetupDiagnostic {
        code: "upgrade_package_ownership".into(),
        message: "The installed package identity or ownership changed before installation".into(),
        recovery: "Refresh the installation assessment; do not reuse this installer handoff".into(),
    })
}
#[cfg(any(target_os = "linux", test))]
fn installed_target_matches(
    expected: InstallerTarget,
    observed: Option<InstallerTarget>,
    owned: bool,
) -> bool {
    observed == Some(expected) && owned
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn dpkg_transitional_states_are_limited_to_package_hook_verification() {
        for state in [b"half-installed".as_slice(), b"half-configured"] {
            assert!(deb_status_matches(state, PackageInspection::InstallerHook));
            assert!(!deb_status_matches(state, PackageInspection::Installed));
        }
        for inspection in [
            PackageInspection::Installed,
            PackageInspection::InstallerHook,
        ] {
            assert!(deb_status_matches(b"installed", inspection));
            for state in [
                b"not-installed".as_slice(),
                b"config-files",
                b"unpacked",
                b"",
                b"installed\n",
            ] {
                assert!(!deb_status_matches(state, inspection));
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn same_package_runtime_hook_requires_the_exact_installed_manager() {
        let platform = super::super::super::RuntimePlatform::current().unwrap();
        let deb = InstallerTarget::runtime(platform, super::super::super::PackageFormat::Deb);
        let rpm = InstallerTarget::runtime(platform, super::super::super::PackageFormat::Rpm);
        let full = InstallerTarget::new(platform, super::super::super::PackageFormat::Deb);
        for target in [deb, rpm] {
            verify_candidate_target(PackageFlavor::Runtime, Some(target), Some(target)).unwrap();
            for expected in [
                None,
                Some(full),
                Some(if target == deb { rpm } else { deb }),
            ] {
                assert_eq!(
                    verify_candidate_target(PackageFlavor::Runtime, Some(target), expected)
                        .unwrap_err()
                        .code,
                    "installer_package_target"
                );
            }
            assert!(verify_candidate_target(PackageFlavor::Runtime, None, Some(target)).is_err());
        }
        verify_candidate_target(PackageFlavor::Full, Some(full), None).unwrap();
    }

    #[test]
    fn changed_package_flavor_format_and_unknown_ownership_fail_closed() {
        let full = InstallerTarget::ALL[4];
        let runtime = InstallerTarget {
            flavor: PackageFlavor::Runtime,
            ..full
        };
        let rpm = InstallerTarget {
            format: super::super::super::PackageFormat::Rpm,
            ..runtime
        };
        assert!(installed_target_matches(runtime, Some(runtime), true));
        for observed in [None, Some(full), Some(rpm)] {
            assert!(!installed_target_matches(runtime, observed, true));
        }
        assert!(!installed_target_matches(runtime, Some(runtime), false));
    }
}
