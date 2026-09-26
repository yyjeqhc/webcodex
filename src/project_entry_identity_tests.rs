use super::*;

#[test]
fn project_entry_user_facing_identity_is_webpi() {
    let help = usage();
    assert!(help.contains("Usage: webpi share"));
    assert!(help.contains("webpi status"));
    assert!(help.contains("webpi doctor"));
    assert!(help.contains("webpi setup"));
    assert!(help.contains("webpi run"));
    assert!(help.contains("WebPi Server + Runner"));
    assert!(!help.contains("webcodex share"));
    assert!(!help.contains("WebCodex"));
}

#[test]
fn managed_download_user_agent_uses_webpi_product_identity() {
    assert_eq!(
        managed_download_user_agent(),
        format!("webpi/{}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn project_launcher_only_uses_companions_in_the_selected_installation() {
    let temp = tempfile::tempdir().unwrap();
    let installation = temp.path().join("installation");
    std::fs::create_dir(&installation).unwrap();
    let cli = installation.join(executable_name("webpi"));
    let foreign = temp.path().join(executable_name("webpi-server"));
    std::fs::write(foreign, b"parent-directory-is-not-the-installation").unwrap();
    assert!(companion_binary_beside(&cli, "webpi-server").is_none());
    let sibling = installation.join(executable_name("webpi-server"));
    std::fs::write(&sibling, b"selected-installation").unwrap();
    assert_eq!(companion_binary_beside(&cli, "webpi-server"), Some(sibling));
    assert!(companion_binary_beside(&cli, "webpi-runner").is_none());
}
