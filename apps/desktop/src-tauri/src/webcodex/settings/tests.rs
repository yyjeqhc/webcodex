use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    runtime: StoredRuntime,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "desktop-settings-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("runner.toml");
        std::fs::write(&path, "# keep this comment\nserver_url = 'http://127.0.0.1:7891'\nclient_id = 'fixture'\ntoken = 'fixture-secret'\n[policy]\nallowed_roots = ['/exact']\n[plugins]\n[[plugins.providers]]\nid = 'example'\nname = 'Example'\ncommand = 'node'\nargs = ['fixture-secret']\n").unwrap();
        Self {
            dir,
            runtime: StoredRuntime {
                server_url: "http://127.0.0.1:7891".into(),
                server_env_file: None,
                runner_config: Some(path),
                user_token_file: None,
                runner_client_id: Some("fixture".into()),
                project_id: None,
                runtime_project_id: None,
            },
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn empty() -> RunnerPaths {
    RunnerPaths {
        instruction_files: vec![],
        skill_roots: vec![],
    }
}

#[test]
fn settings_preserve_credentials_policy_plugins_and_comments() {
    let f = Fixture::new();
    let next = RunnerPaths {
        instruction_files: vec![f.dir.join("global.md").to_string_lossy().into_owned()],
        skill_roots: vec![f.dir.join("skills").to_string_lossy().into_owned()],
    };
    update(
        &f.runtime,
        SettingsUpdate {
            target: target(&f.runtime).unwrap(),
            expected: empty(),
            paths: next.clone(),
        },
    )
    .unwrap();
    let projection = inspect(&f.runtime, false).unwrap();
    assert!(projection.paths == next);
    assert_eq!(projection.plugin_ids, vec!["example"]);
    assert!(!serde_json::to_string(&projection)
        .unwrap()
        .contains("fixture-secret"));
    let text = read(f.runtime.runner_config.as_ref().unwrap()).unwrap();
    for preserved in [
        "# keep this comment",
        "token = 'fixture-secret'",
        "allowed_roots = ['/exact']",
        "args = ['fixture-secret']",
    ] {
        assert!(text.contains(preserved));
    }
    assert!(update(
        &f.runtime,
        SettingsUpdate {
            target: target(&f.runtime).unwrap(),
            expected: empty(),
            paths: empty()
        }
    )
    .is_err());
    update(
        &f.runtime,
        SettingsUpdate {
            target: target(&f.runtime).unwrap(),
            expected: next,
            paths: empty(),
        },
    )
    .unwrap();
    assert!(inspect(&f.runtime, false).unwrap().paths == empty());
}

#[test]
fn managed_instruction_activation_appends_preserves_and_rolls_back_exact_candidate() {
    let f = Fixture::new();
    let custom = f.dir.join("company.md").to_string_lossy().into_owned();
    let skills = f.dir.join("skills").to_string_lossy().into_owned();
    let expected = RunnerPaths {
        instruction_files: vec![custom.clone()],
        skill_roots: vec![skills.clone()],
    };
    update(
        &f.runtime,
        SettingsUpdate {
            target: target(&f.runtime).unwrap(),
            expected: empty(),
            paths: expected.clone(),
        },
    )
    .unwrap();
    let before = read(f.runtime.runner_config.as_ref().unwrap()).unwrap();
    let managed = f.dir.join("instructions/AGENTS.md");
    let edit = stage_managed_instructions(
        &f.runtime,
        target(&f.runtime).unwrap(),
        expected.clone(),
        &managed,
    )
    .unwrap();
    let after = inspect(&f.runtime, false).unwrap();
    assert_eq!(
        after.paths.instruction_files,
        vec![custom, managed.to_string_lossy().into_owned()]
    );
    assert_eq!(after.paths.skill_roots, vec![skills]);
    let duplicate = stage_managed_instructions(
        &f.runtime,
        target(&f.runtime).unwrap(),
        after.paths.clone(),
        &managed,
    )
    .unwrap();
    assert!(duplicate.candidate_unchanged().unwrap());
    assert_eq!(
        inspect(&f.runtime, false)
            .unwrap()
            .paths
            .instruction_files
            .len(),
        2
    );
    assert!(edit.rollback_if_unchanged().unwrap());
    assert_eq!(
        read(f.runtime.runner_config.as_ref().unwrap()).unwrap(),
        before
    );
    let edit =
        stage_managed_instructions(&f.runtime, target(&f.runtime).unwrap(), expected, &managed)
            .unwrap();
    let path = f.runtime.runner_config.as_ref().unwrap();
    let changed = format!("{}\n# concurrent operator edit\n", read(path).unwrap());
    std::fs::write(path, &changed).unwrap();
    assert!(!edit.rollback_if_unchanged().unwrap());
    assert_eq!(read(path).unwrap(), changed);
}

#[test]
fn file_access_reports_configured_and_effective_roots_and_preserves_unrelated_config() {
    let f = Fixture::new();
    let before = read(f.runtime.runner_config.as_ref().unwrap()).unwrap();
    let projection = inspect(&f.runtime, false).unwrap();
    assert_eq!(projection.file_access.configured_roots, vec!["/exact"]);
    assert_eq!(projection.file_access.effective_roots, vec!["/exact"]);
    assert!(!projection.file_access.using_default_roots);
    assert!(!projection.file_access.allow_cwd_anywhere);

    let allowed = f.dir.join("allowed");
    std::fs::create_dir_all(&allowed).unwrap();
    let edit = stage_allowed_roots_update(
        &f.runtime,
        AllowedRootsUpdate {
            target: target(&f.runtime).unwrap(),
            expected: vec!["/exact".into()],
            roots: vec![allowed.to_string_lossy().into_owned()],
        },
    )
    .unwrap();
    assert!(edit.candidate_unchanged().unwrap());
    let updated = read(f.runtime.runner_config.as_ref().unwrap()).unwrap();
    assert!(updated.contains("# keep this comment"));
    assert!(updated.contains("token = 'fixture-secret'"));
    assert!(updated.contains("args = ['fixture-secret']"));
    let projection = inspect(&f.runtime, false).unwrap();
    assert_eq!(
        projection.file_access.configured_roots,
        vec![allowed.to_string_lossy()]
    );
    assert_eq!(
        projection.file_access.effective_roots,
        vec![allowed.to_string_lossy()]
    );
    assert!(edit.rollback_if_unchanged().unwrap());
    assert_eq!(
        read(f.runtime.runner_config.as_ref().unwrap()).unwrap(),
        before
    );
}

#[test]
fn file_access_empty_configuration_reports_home_default_and_rejects_missing_root() {
    let f = Fixture::new();
    let path = f.runtime.runner_config.as_ref().unwrap();
    let original = read(path).unwrap();
    let empty = original.replace("allowed_roots = ['/exact']", "allowed_roots = []");
    persist_text(path, &original, &empty).unwrap();
    let projection = inspect(&f.runtime, false).unwrap();
    assert!(projection.file_access.configured_roots.is_empty());
    assert!(projection.file_access.using_default_roots);
    assert_eq!(
        projection.file_access.effective_roots,
        webcodex_runner_config::home_allowed_root()
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    );
    let before = read(path).unwrap();
    assert!(stage_allowed_roots_update(
        &f.runtime,
        AllowedRootsUpdate {
            target: target(&f.runtime).unwrap(),
            expected: vec![],
            roots: vec![f.dir.join("does-not-exist").to_string_lossy().into_owned()],
        },
    )
    .is_err());
    assert_eq!(read(path).unwrap(), before);
}

#[test]
fn file_access_can_remove_stale_roots_one_at_a_time() {
    let f = Fixture::new();
    let path = f.runtime.runner_config.as_ref().unwrap();
    let stale_a = f.dir.join("removed-a").to_string_lossy().into_owned();
    let stale_b = f.dir.join("removed-b").to_string_lossy().into_owned();

    let original = read(path).unwrap();
    let mut doc = original.parse::<DocumentMut>().unwrap();
    doc["policy"]["allowed_roots"] = toml_edit::value(
        vec![stale_a.clone(), stale_b.clone()]
            .into_iter()
            .collect::<Array>(),
    );
    let seeded = doc.to_string();
    persist_text(path, &original, &seeded).unwrap();

    let projection = inspect(&f.runtime, false).unwrap();
    assert_eq!(
        projection.file_access.configured_roots,
        vec![stale_a.clone(), stale_b.clone()]
    );

    stage_allowed_roots_update(
        &f.runtime,
        AllowedRootsUpdate {
            target: target(&f.runtime).unwrap(),
            expected: vec![stale_a, stale_b.clone()],
            roots: vec![stale_b.clone()],
        },
    )
    .expect("removing one stale root must not revalidate an unchanged stale survivor");

    let projection = inspect(&f.runtime, false).unwrap();
    assert_eq!(projection.file_access.configured_roots, vec![stale_b]);
}

#[cfg(windows)]
#[test]
fn file_access_accepts_existing_windows_drive_root() {
    let f = Fixture::new();
    let drive_root = std::env::current_dir()
        .unwrap()
        .ancestors()
        .last()
        .unwrap()
        .to_path_buf();
    assert!(drive_root.is_absolute() && drive_root.is_dir());
    stage_allowed_roots_update(
        &f.runtime,
        AllowedRootsUpdate {
            target: target(&f.runtime).unwrap(),
            expected: vec!["/exact".into()],
            roots: vec![drive_root.to_string_lossy().into_owned()],
        },
    )
    .expect("an existing Windows drive root is a valid explicit allowed root");
}

#[test]
fn settings_reject_identity_changes_traversal_duplicates_and_bounds() {
    let mut f = Fixture::new();
    for invalid in [
        vec!["relative".into()],
        vec![f.dir.join("../escape").to_string_lossy().into_owned()],
        vec![f.dir.to_string_lossy().into_owned(); 2],
        vec![f.dir.to_string_lossy().into_owned(); 17],
    ] {
        assert!(update(
            &f.runtime,
            SettingsUpdate {
                target: target(&f.runtime).unwrap(),
                expected: empty(),
                paths: RunnerPaths {
                    instruction_files: invalid,
                    skill_roots: vec![]
                }
            }
        )
        .is_err());
    }
    f.runtime.runner_client_id = Some("other".into());
    assert!(inspect(&f.runtime, false).is_err());
    assert!(update(
        &f.runtime,
        SettingsUpdate {
            target: target(&f.runtime).unwrap(),
            expected: empty(),
            paths: empty()
        }
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn settings_refuse_symlink_and_write_private_permissions() {
    use std::os::unix::{fs::symlink, fs::PermissionsExt};
    let mut f = Fixture::new();
    update(
        &f.runtime,
        SettingsUpdate {
            target: target(&f.runtime).unwrap(),
            expected: empty(),
            paths: empty(),
        },
    )
    .unwrap();
    let path = f.runtime.runner_config.clone().unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let link = f.dir.join("link.toml");
    symlink(path, &link).unwrap();
    f.runtime.runner_config = Some(link);
    assert!(inspect(&f.runtime, false).is_err());
}
