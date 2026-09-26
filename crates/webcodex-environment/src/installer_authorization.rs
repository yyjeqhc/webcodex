//! An explicit handoff from a user's prepared upgrade to a system package.
//! The root package process never opens its own default environment or runs a
//! user's candidate executable with administrator authority.
use crate::{EnvironmentStore, PreparedInstallationReceipt, SetupDiagnostic, SetupResultValue};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Authorization {
    schema_version: u16,
    receipt_path: PathBuf,
    receipt: PreparedInstallationReceipt,
    #[cfg(unix)]
    frozen: crate::upgrade::FrozenInstallerUpgrade,
}

fn diagnostic(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(code, message, "Prepare the upgrade as its original user, then explicitly authorize that receipt for the system installer")
}

pub(crate) fn system_directory() -> SetupResultValue<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        Ok(PathBuf::from("/var/lib/webcodex-installer"))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(PathBuf::from(
            "/Library/Application Support/WebCodexInstaller",
        ))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(diagnostic("installer_authority", "This installer runs in the original user's context and must use an explicit user receipt"))
    }
}

fn require_system_installer() -> SetupResultValue<()> {
    #[cfg(unix)]
    if unsafe { libc::geteuid() } == 0 {
        return Ok(());
    }
    Err(diagnostic(
        "installer_authority",
        "System package authorization requires administrator execution",
    ))
}

fn system_runtime_directory() -> SetupResultValue<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        Ok(PathBuf::from("/usr/lib/webcodex/webcodex-runtime"))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(PathBuf::from(
            "/Library/Application Support/WebCodex/runtime",
        ))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(diagnostic(
            "installer_authority",
            "A system package target is unavailable on this platform",
        ))
    }
}

/// Match package destinations to the user's original executable locations.
/// A package must not change an arbitrary path supplied in a user receipt.
pub fn verify_installer_targets(
    receipt: &PreparedInstallationReceipt,
    directory: &Path,
) -> SetupResultValue<()> {
    if !directory.is_absolute() {
        return Err(diagnostic(
            "installer_targets",
            "Installer destinations must be absolute",
        ));
    }
    let directory = directory.canonicalize().map_err(|_| {
        diagnostic(
            "installer_targets",
            "The previous runtime directory is unavailable",
        )
    })?;
    for name in ["webcodex", "webcodex-server", "webcodex-runner"] {
        let expected = directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        if receipt.targets.get(name) != Some(&expected) {
            return Err(diagnostic(
                "installer_targets",
                "The package destinations do not match the prepared runtime",
            ));
        }
    }
    #[cfg(target_os = "linux")]
    let desktop = (directory == Path::new("/usr/lib/webcodex/webcodex-runtime"))
        .then(|| PathBuf::from("/usr/lib/webcodex/webcodex-desktop"));
    #[cfg(target_os = "macos")]
    let desktop = (directory == Path::new("/Library/Application Support/WebCodex/runtime"))
        .then(|| PathBuf::from("/Applications/WebCodex Desktop.app"));
    #[cfg(windows)]
    let desktop = (directory.file_name().and_then(|name| name.to_str())
        == Some("webcodex-runtime"))
    .then(|| directory.parent().unwrap().join("WebCodex.exe"));
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let desktop: Option<PathBuf> = None;
    if let Some(desktop) = desktop {
        if receipt.desktop_target.as_ref() != Some(&desktop) {
            return Err(diagnostic(
                "installer_targets",
                "The managed Desktop target is absent or differs from the package destination",
            ));
        }
    }
    Ok(())
}

pub async fn authorize_prepared_installation(
    receipt_path: &Path,
    candidate_dir: &Path,
) -> SetupResultValue<PreparedInstallationReceipt> {
    require_system_installer()?;
    let receipt = crate::verify_prepared_installation(receipt_path, candidate_dir).await?;
    verify_installer_targets(&receipt, &system_runtime_directory()?)?;
    let store = EnvironmentStore::open(system_directory()?)?;
    let _lock = store.lock()?;
    let authorization = Authorization {
        schema_version: 1,
        receipt_path: receipt_path.to_path_buf(),
        receipt: receipt.clone(),
        #[cfg(unix)]
        frozen: crate::upgrade::freeze_installer_upgrade(&receipt)?,
    };
    save_authorization(&store, &authorization)?;
    Ok(receipt)
}

fn save_authorization(
    store: &EnvironmentStore,
    authorization: &Authorization,
) -> SetupResultValue<()> {
    if let Some(previous) = store.read_json::<Authorization>("authorization.json")? {
        if previous != *authorization {
            return Err(diagnostic(
                "installer_authorization_conflict",
                "A different prepared upgrade already owns this installer authorization",
            ));
        }
    }
    store.write_json("authorization.json", authorization)
}

#[cfg(unix)]
pub(crate) fn load_authorized_upgrade(
    store: &EnvironmentStore,
) -> SetupResultValue<(
    PreparedInstallationReceipt,
    crate::upgrade::FrozenInstallerUpgrade,
)> {
    let authorization: Authorization = store.read_json("authorization.json")?.ok_or_else(|| {
        diagnostic(
            "installer_authorization_required",
            "This package upgrade has no explicitly authorized user receipt",
        )
    })?;
    if authorization.schema_version != 1
        || authorization.receipt_path
            != authorization
                .receipt
                .environment_dir
                .join("upgrade-prepared.json")
    {
        return Err(diagnostic(
            "installer_authorization_invalid",
            "The installer authorization is invalid",
        ));
    }
    verify_installer_targets(&authorization.receipt, &system_runtime_directory()?)?;
    if authorization.frozen.root != authorization.receipt.environment_dir
        || authorization.frozen.operation_id != authorization.receipt.operation_id
        || authorization.frozen.environment_id != authorization.receipt.environment_id
        || authorization.frozen.owner.identity != authorization.receipt.owner_identity
        || authorization.frozen.manifest_sha256 != authorization.receipt.manifest_sha256
        || authorization
            .frozen
            .desktop
            .as_ref()
            .map(|item| &item.target)
            != authorization.receipt.desktop_target.as_ref()
        || authorization
            .frozen
            .programs
            .iter()
            .map(|(name, target, _, _)| (name.clone(), target.clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
            != authorization.receipt.targets
    {
        return Err(diagnostic(
            "installer_authorization_invalid",
            "The protected frozen upgrade differs from its receipt",
        ));
    }
    Ok((authorization.receipt, authorization.frozen))
}

#[cfg(unix)]
pub(crate) fn clear_authorization_under_lock(store: &EnvironmentStore) -> SetupResultValue<()> {
    if store
        .read_json::<Authorization>("authorization.json")?
        .is_some()
    {
        std::fs::remove_file(store.root().join("authorization.json"))
            .map_err(|_| SetupDiagnostic::io())?;
        std::fs::File::open(store.root())
            .and_then(|file| file.sync_all())
            .map_err(|_| SetupDiagnostic::io())?;
    }
    Ok(())
}

/// Used by deb/pkg hooks. All verification stays in Core, including private
/// owner checks and exact receipt/journal/candidate comparison.
pub async fn verify_installer_authorization(
    candidate_dir: &Path,
) -> SetupResultValue<PreparedInstallationReceipt> {
    require_system_installer()?;
    let store = EnvironmentStore::open(system_directory()?)?;
    let _lock = store.lock()?;
    let authorization: Authorization = store.read_json("authorization.json")?.ok_or_else(|| {
        diagnostic(
            "installer_authorization_required",
            "This package upgrade has no explicitly authorized user receipt",
        )
    })?;
    if authorization.schema_version != 1 {
        return Err(diagnostic(
            "installer_authorization_invalid",
            "The installer authorization version is unsupported",
        ));
    }
    let receipt =
        crate::verify_prepared_installation(&authorization.receipt_path, candidate_dir).await?;
    if receipt != authorization.receipt {
        return Err(diagnostic(
            "installer_authorization_conflict",
            "The original user's prepared operation changed after authorization",
        ));
    }
    verify_installer_targets(&receipt, &system_runtime_directory()?)?;
    Ok(receipt)
}

/// This removes only the package handoff. Service recovery belongs to the
/// original user's `upgrade-finish` or `upgrade-rollback` transaction.
pub fn cancel_installer_authorization() -> SetupResultValue<()> {
    require_system_installer()?;
    let store = EnvironmentStore::open(system_directory()?)?;
    let _lock = store.lock()?;
    if store
        .read_json::<Authorization>("authorization.json")?
        .is_some()
    {
        std::fs::remove_file(store.root().join("authorization.json"))
            .map_err(|_| SetupDiagnostic::io())?;
        #[cfg(unix)]
        std::fs::File::open(store.root())
            .and_then(|file| file.sync_all())
            .map_err(|_| SetupDiagnostic::io())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(root: &Path) -> PreparedInstallationReceipt {
        PreparedInstallationReceipt {
            schema_version: 1,
            operation_id: "operation".into(),
            environment_id: "environment".into(),
            environment_dir: root.join("environment"),
            owner_identity: "123".into(),
            manifest_sha256: "a".repeat(64),
            targets: ["webcodex", "webcodex-server", "webcodex-runner"]
                .into_iter()
                .map(|name| {
                    (
                        name.into(),
                        root.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
                    )
                })
                .collect(),
            desktop_target: None,
        }
    }

    #[cfg(unix)]
    fn frozen(receipt: &PreparedInstallationReceipt) -> crate::upgrade::FrozenInstallerUpgrade {
        crate::upgrade::FrozenInstallerUpgrade {
            root: receipt.environment_dir.clone(),
            operation_id: receipt.operation_id.clone(),
            owner: crate::LocalAccount {
                name: "owner".into(),
                identity: receipt.owner_identity.clone(),
                home: receipt.environment_dir.clone(),
            },
            environment_id: receipt.environment_id.clone(),
            services: vec![],
            all_services: vec![],
            programs: vec![],
            desktop: None,
            manifest_sha256: receipt.manifest_sha256.clone(),
            record_fingerprint: "a".repeat(64),
            candidate_fingerprint: "b".repeat(64),
            installed_cli: receipt.targets["webcodex"].clone(),
            candidate_cli_sha256: "c".repeat(64),
            backup_cli: receipt.environment_dir.join("backup-webcodex"),
        }
    }

    #[test]
    fn handoff_cannot_change_owner_operation_or_destinations() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = EnvironmentStore::open(root.join("authorization")).unwrap();
        let prepared = receipt(&root);
        verify_installer_targets(&prepared, &root).unwrap();
        let authorization = Authorization {
            schema_version: 1,
            receipt_path: prepared.environment_dir.join("upgrade-prepared.json"),
            receipt: prepared.clone(),
            #[cfg(unix)]
            frozen: frozen(&prepared),
        };
        save_authorization(&store, &authorization).unwrap();
        save_authorization(&store, &authorization).unwrap();
        let mut changed = prepared;
        changed.operation_id = "different".into();
        let next = Authorization {
            schema_version: 1,
            receipt_path: authorization.receipt_path.clone(),
            receipt: changed.clone(),
            #[cfg(unix)]
            frozen: frozen(&changed),
        };
        assert_eq!(
            save_authorization(&store, &next).unwrap_err().code,
            "installer_authorization_conflict"
        );
        changed
            .targets
            .insert("webcodex-server".into(), root.join("unrelated-program"));
        assert_eq!(
            verify_installer_targets(&changed, &root).unwrap_err().code,
            "installer_targets"
        );
        assert_eq!(
            store
                .read_json::<Authorization>("authorization.json")
                .unwrap()
                .unwrap()
                .receipt
                .operation_id,
            "operation"
        );
    }
}
