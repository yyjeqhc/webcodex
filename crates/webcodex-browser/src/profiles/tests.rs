use super::*;
fn root() -> (TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("browser");
    (temp, root)
}
#[test]
fn managed_profile_is_exclusive_and_survives_owner_release() {
    let (_temp, root) = root();
    let profile = ManagedProfile::acquire(&root, "login").unwrap();
    let cookie = profile.path.join("cookie-fixture");
    std::fs::write(&cookie, "retained-login-fixture").unwrap();
    assert!(matches!(
        ManagedProfile::acquire(&root, "login"),
        Err(BrowserError {
            kind: "profile_busy",
            ..
        })
    ));
    assert!(ManagedProfile::acquire(&root, "other").is_ok());
    drop(profile);
    let recovered = ManagedProfile::acquire(&root, "login").unwrap();
    assert_eq!(
        std::fs::read_to_string(recovered.path.join("cookie-fixture")).unwrap(),
        "retained-login-fixture"
    );
}
#[test]
fn model_profile_names_cannot_select_arbitrary_paths() {
    for name in [
        "",
        ".",
        "..",
        "../Default",
        "/Users/person/Chrome",
        "C:\\Chrome",
        "login/session",
        "UPPER",
        "a\0b",
    ] {
        assert!(validate_profile_id(name).is_err(), "{name:?}");
    }
    assert!(validate_profile_id("daily-login_1").is_ok());
    assert!(validate_profile_id(&"x".repeat(49)).is_err());
}
#[cfg(unix)]
#[test]
fn state_links_and_nonprivate_directories_fail_before_profile_creation() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (temp, root) = root();
    private_dir(&root).unwrap();
    let alias = temp.path().join("alias");
    symlink(&root, &alias).unwrap();
    assert!(ManagedProfile::acquire(&alias, "login").is_err());
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(ManagedProfile::acquire(&root, "login").is_err());
}
#[cfg(unix)]
#[test]
fn live_chrome_singleton_cannot_be_replaced_or_adopted() {
    use std::os::unix::fs::symlink;
    let (_temp, root) = root();
    let profile = ManagedProfile::acquire(&root, "login").unwrap();
    symlink(
        format!("fixture-{}", std::process::id()),
        profile.path.join("SingletonLock"),
    )
    .unwrap();
    drop(profile);
    assert!(matches!(
        ManagedProfile::acquire(&root, "login"),
        Err(BrowserError {
            kind: "profile_busy",
            ..
        })
    ));
}
