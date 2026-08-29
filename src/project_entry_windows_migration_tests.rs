use super::setup_service::{harden_existing_runtime_private_state, ProjectConfig, ProjectPaths};
use super::windows_private_state::{
    current_user_sid_for_test, dacl_sddl, set_broad_test_file_dacl,
};
use super::ProjectCommandOptions;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(args: &[&str], cwd: &Path) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn legacy_paths(name: &str) -> (tempfile::TempDir, ProjectPaths) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(name);
    let state = temp.path().join("state");
    fs::create_dir(&root).unwrap();
    git(&["init", "-q"], &root);
    let options = ProjectCommandOptions {
        root,
        profile: "personal".to_string(),
        state_dir: Some(state),
        json: false,
        console_assets_dir: None,
    };
    let (_, paths) = ProjectConfig::resolve(&options).unwrap();
    fs::create_dir_all(&paths.data).unwrap();
    fs::create_dir_all(&paths.logs).unwrap();
    (temp, paths)
}

fn assert_hardened(path: &Path, current_user_sid: &str) {
    let sddl = dacl_sddl(path, false).unwrap();
    assert!(sddl.starts_with("D:P"), "DACL must be protected: {sddl}");
    assert!(
        sddl.contains(&format!(";;;{current_user_sid})")),
        "current user ACE must remain: {sddl}"
    );
    assert!(sddl.contains(";;;SY)"), "SYSTEM ACE must remain: {sddl}");
    for broad in [";;;WD)", ";;;BU)", ";;;BA)"] {
        assert!(
            !sddl.contains(broad),
            "broad Windows ACE {broad} remained: {sddl}"
        );
    }
}

#[test]
fn legacy_runtime_file_acls_are_migrated_without_changing_contents() {
    let (_temp, paths) = legacy_paths("legacy-runtime-acls");
    let files = [
        (paths.data.join("webcodex.db"), b"legacy-main-db".as_slice()),
        (
            paths.data.join("webcodex.db-wal"),
            b"legacy-existing-wal".as_slice(),
        ),
        (
            paths.data.join("webcodex.db-shm"),
            b"legacy-existing-shm".as_slice(),
        ),
        (
            paths.logs.join("server.log"),
            b"legacy-server-log\n".as_slice(),
        ),
        (
            paths.logs.join("agent.log"),
            b"legacy-agent-log\n".as_slice(),
        ),
    ];

    for (path, content) in &files {
        fs::write(path, content).unwrap();
        set_broad_test_file_dacl(path).unwrap();
        let sddl = dacl_sddl(path, false).unwrap();
        assert!(
            sddl.contains(";;;WD)") && sddl.contains(";;;BU)"),
            "fixture must start with explicit broad ACEs: {sddl}"
        );
    }
    let before = files
        .iter()
        .map(|(path, _)| (path.clone(), fs::read(path).unwrap()))
        .collect::<Vec<(PathBuf, Vec<u8>)>>();

    harden_existing_runtime_private_state(&paths).unwrap();

    let current_user_sid = current_user_sid_for_test().unwrap();
    for (path, content) in before {
        assert_eq!(
            fs::read(&path).unwrap(),
            content,
            "migration changed {path:?}"
        );
        assert_hardened(&path, &current_user_sid);
    }
}

#[test]
fn legacy_runtime_acl_migration_does_not_create_absent_sqlite_sidecars() {
    let (_temp, paths) = legacy_paths("legacy-absent-sidecars");
    let db = paths.data.join("webcodex.db");
    let server_log = paths.logs.join("server.log");
    fs::write(&db, b"legacy-db").unwrap();
    fs::write(&server_log, b"legacy-log\n").unwrap();
    set_broad_test_file_dacl(&db).unwrap();
    set_broad_test_file_dacl(&server_log).unwrap();
    let wal = paths.data.join("webcodex.db-wal");
    let shm = paths.data.join("webcodex.db-shm");
    assert!(!wal.exists());
    assert!(!shm.exists());

    harden_existing_runtime_private_state(&paths).unwrap();

    assert!(!wal.exists(), "migration must not create an absent WAL");
    assert!(!shm.exists(), "migration must not create an absent SHM");
    let current_user_sid = current_user_sid_for_test().unwrap();
    assert_hardened(&db, &current_user_sid);
    assert_hardened(&server_log, &current_user_sid);
}

#[test]
fn legacy_runtime_acl_migration_rejects_direct_reparse_file() {
    use std::os::windows::fs::symlink_file;

    let (temp, paths) = legacy_paths("legacy-reparse");
    let target = temp.path().join("outside-runtime-state.db");
    fs::write(&target, b"must-stay-untouched").unwrap();
    let link = paths.data.join("webcodex.db");
    symlink_file(&target, &link).expect("Windows test host must support file symlinks");
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());

    let error = harden_existing_runtime_private_state(&paths).unwrap_err();

    assert_eq!(error.code, "project_registration_invalid");
    assert!(error
        .message
        .contains("unsafe existing private Windows runtime state"));
    assert_eq!(fs::read(&target).unwrap(), b"must-stay-untouched");
}
