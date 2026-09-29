use super::*;

fn user_spec() -> ServiceSpec {
    let mut spec = super::super::tests::sample_spec();
    spec.scope = ServiceScope::User;
    if let ServiceAccount::SystemUser {
        group,
        home,
        expected_identity,
        ..
    } = &mut spec.account
    {
        *group = None;
        *home = Some(
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .to_owned(),
        );
        *expected_identity = "1001".into();
    }
    // Validation of manager selection is independent of host path spelling.
    spec.program = std::env::current_exe().unwrap();
    spec.working_directory = spec.program.parent().unwrap().to_owned();
    spec
}

#[test]
fn old_specs_omit_scope_and_roundtrip_as_system_without_identity_changes() {
    let spec = super::super::tests::sample_spec();
    let old = serde_json::to_value(&spec).unwrap();
    assert!(old.get("scope").is_none());
    let restored: ServiceSpec = serde_json::from_value(old).unwrap();
    assert_eq!(restored, spec);
    assert_eq!(restored.scope, ServiceScope::System);
    let user = user_spec();
    assert_eq!(serde_json::to_value(&user).unwrap()["scope"], "user");
    assert_ne!(user, restored);
    assert!("automatic".parse::<ServiceScope>().is_err());
}

#[test]
fn user_scope_cannot_request_system_identity_or_socket_behaviors() {
    let user = user_spec();
    validate_spec(&user).unwrap();
    let mut wrong = user.clone();
    wrong.linux_socket = Some(LinuxSocketSpec {
        listen: "127.0.0.1:18880".into(),
    });
    assert!(validate_spec(&wrong).is_err());
    let mut wrong = user.clone();
    wrong.args.push("--windows-service".into());
    assert!(validate_spec(&wrong).is_err());
    let mut wrong = user.clone();
    wrong.account = ServiceAccount::WindowsVirtual {
        name: "NT SERVICE\\other".into(),
    };
    assert!(validate_spec(&wrong).is_err());
    let mut wrong = user.clone();
    if let ServiceAccount::SystemUser { group, .. } = &mut wrong.account {
        *group = Some("other".into());
    }
    assert!(validate_spec(&wrong).is_err());
    for id in ["0", "S-1-5-18", "S-1-5-19", "S-1-5-20"] {
        let mut wrong = user.clone();
        if let ServiceAccount::SystemUser {
            expected_identity, ..
        } = &mut wrong.account
        {
            *expected_identity = id.into();
        }
        assert!(validate_spec(&wrong).is_err());
    }
}

#[cfg(unix)]
#[test]
fn user_service_directory_creation_is_private_and_does_not_adopt_links() {
    use std::os::unix::fs::{symlink, MetadataExt};
    let tmp = crate::test_tempdir().unwrap();
    let uid = unsafe { libc::geteuid() };
    let path = tmp.path().join("config/systemd/user");
    ensure_user_service_directory(&path, uid).unwrap();
    assert_eq!(std::fs::symlink_metadata(&path).unwrap().mode() & 0o077, 0);
    let foreign = tmp.path().join("link");
    symlink(&path, &foreign).unwrap();
    assert!(ensure_user_service_directory(&foreign, uid).is_err());
    assert!(ensure_user_service_directory(&path, uid.saturating_add(1)).is_err());
}

#[test]
fn user_manager_operations_never_impersonate_another_saved_owner() {
    let mut spec = user_spec();
    if let ServiceAccount::SystemUser {
        expected_identity, ..
    } = &mut spec.account
    {
        *expected_identity = "definitely-not-this-owner".into();
    }
    assert!(verify_current_user(&spec).is_err());
}
