use super::*;

#[test]
fn package_handoff_is_exact_argv_not_shell_text() {
    let path = Path::new("/private/cache/1.2.3/a b;$(touch bad).pkg");
    let (program, args) = package_program(InstallerPlatform::DarwinArm64, path).unwrap();
    assert_eq!(program, "/usr/sbin/installer");
    assert_eq!(
        args,
        vec![
            std::ffi::OsString::from("-pkg"),
            path.as_os_str().to_owned(),
            "-target".into(),
            "/".into()
        ]
    );
    let (program, args) = package_program(InstallerPlatform::LinuxX64, path).unwrap();
    assert_eq!(program, "/usr/bin/dpkg");
    assert_eq!(args[1], path.as_os_str());
    assert!(package_program(InstallerPlatform::Win32X64, path).is_err());
    assert!(package_program(InstallerPlatform::DarwinArm64, Path::new("relative.pkg")).is_err());
}

#[test]
fn root_snapshot_is_rehashed_bounded_atomic_and_private() {
    let temp = crate::test_tempdir().unwrap();
    let user = PrivateUpdateCache::open(temp.path().join("user")).unwrap();
    user.write("download.pkg", b"official installer").unwrap();
    let privileged = PrivateUpdateCache::open(temp.path().join("root")).unwrap();
    let path = user.file("download.pkg").unwrap();
    let uid = unsafe { libc::geteuid() };
    let hash = sha256(b"official installer");
    assert_eq!(
        copy_installer(&path, uid, &privileged, "candidate.pkg", &hash, 8),
        Err(UpdateError::DownloadTooLarge)
    );
    assert_eq!(
        copy_installer(
            &path,
            uid,
            &privileged,
            "candidate.pkg",
            &"0".repeat(64),
            64
        ),
        Err(UpdateError::ChecksumMismatch)
    );
    assert!(!privileged.file("candidate.pkg").unwrap().exists());
    assert!(!privileged.file("installer.part").unwrap().exists());
    copy_installer(&path, uid, &privileged, "candidate.pkg", &hash, 64).unwrap();
    user.write("download.pkg", b"changed after verification")
        .unwrap();
    assert_eq!(
        privileged.read("candidate.pkg", 64).unwrap().unwrap(),
        b"official installer"
    );
    assert_eq!(
        std::fs::metadata(privileged.file("candidate.pkg").unwrap())
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn privileged_copy_rejects_links_other_owners_and_nonprivate_downloads() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let temp = crate::test_tempdir().unwrap();
    let user = PrivateUpdateCache::open(temp.path().join("user")).unwrap();
    let root = PrivateUpdateCache::open(temp.path().join("root")).unwrap();
    user.write("file.pkg", b"pkg").unwrap();
    let file = user.file("file.pkg").unwrap();
    let uid = unsafe { libc::geteuid() };
    let hash = sha256(b"pkg");
    assert_eq!(
        copy_installer(&file, uid.saturating_add(1), &root, "x.pkg", &hash, 64),
        Err(UpdateError::ProvenanceFailed)
    );
    let alias = user.file("alias.pkg").unwrap();
    symlink(&file, &alias).unwrap();
    assert_eq!(
        copy_installer(&alias, uid, &root, "x.pkg", &hash, 64),
        Err(UpdateError::ProvenanceFailed)
    );
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        copy_installer(&file, uid, &root, "x.pkg", &hash, 64),
        Err(UpdateError::ProvenanceFailed)
    );
}

#[test]
fn unknown_handoff_is_not_safe_to_restore_or_retry() {
    let unknown = InstallerLaunchNotice::not_started(UpdateError::RecoveryRequired);
    assert!(!unknown.started && !unknown.safe_to_restore);
    let denied = InstallerLaunchNotice::not_started(UpdateError::AuthorizationRequired);
    assert!(!denied.started && denied.safe_to_restore);
}
