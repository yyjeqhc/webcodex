use super::*;

struct Fixture {
    _temp: tempfile::TempDir,
    store: EnvironmentStore,
    runtime: PathBuf,
    candidate: UpgradeCandidate,
}
impl Fixture {
    fn new() -> Self {
        let temp = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let runtime = temp.path().join("installed/webcodex-runtime");
        ensure_private_directory(runtime.parent().unwrap()).unwrap();
        ensure_private_directory(&runtime).unwrap();
        let mut artifacts = BTreeMap::new();
        for (name, target) in targets(&runtime).unwrap() {
            std::fs::write(&target, format!("old {name}")).unwrap();
            #[cfg(windows)]
            crate::storage::secure_windows_path(&target).unwrap();
            let path = temp.path().join(format!("new-{name}.exe"));
            std::fs::write(&path, format!("new {name}")).unwrap();
            let build_info: MachineBuildInfo = serde_json::from_value(json!({
                "schema_version":1,"binary":name,"version":"0.5.0","git_commit":"a".repeat(40),"git_dirty":false,"built_at":"1",
                "target":"x86_64-pc-windows-msvc","architecture":"x86_64","desktop_runtime_contract":{"min_generation":1,"max_generation":1},"environment_data_format":1
            })).unwrap();
            artifacts.insert(
                name,
                CandidateArtifact {
                    sha256: digest(&path).unwrap(),
                    path,
                    build_info,
                },
            );
        }
        let desktop = CandidateDesktop {
            path: artifacts["webcodex-desktop"].path.clone(),
            executable: "webcodex-desktop.exe".into(),
            sha256: artifacts["webcodex-desktop"].sha256.clone(),
            managed_files: BTreeMap::from([(
                "WebCodex.exe".into(),
                artifacts["webcodex-desktop"].sha256.clone(),
            )]),
        };
        Self {
            _temp: temp,
            store,
            runtime,
            candidate: UpgradeCandidate {
                version: "0.5.0".into(),
                source_sha: "a".repeat(40),
                platform: "win32-x64".into(),
                source_workflow_run_id: 1,
                source_workflow_ref: "fixture".into(),
                manifest_sha256: "b".repeat(64),
                provenance_verified: true,
                root: PathBuf::new(),
                artifacts,
                desktop: Some(desktop),
            },
        }
    }
    fn prepare(&self) {
        prepare_verified(
            &self.store,
            self.candidate.clone(),
            &self.runtime,
            InstallationKind::Legacy,
        )
        .unwrap();
    }
    fn replace(&self) {
        for (name, path) in targets(&self.runtime).unwrap() {
            std::fs::copy(&self.candidate.artifacts[&name].path, path).unwrap();
        }
    }
    fn assert_original(&self) {
        for (name, path) in targets(&self.runtime).unwrap() {
            assert_eq!(
                std::fs::read_to_string(path).unwrap(),
                format!("old {name}")
            );
        }
        assert!(self.store.load_environment().unwrap().is_none());
    }
}

#[test]
fn prepare_seals_a_package_receipt_without_inventing_an_environment() {
    let f = Fixture::new();
    f.prepare();
    let receipt = verify_receipt_inner(&f.store, &f.candidate, &f.runtime).unwrap();
    assert_eq!(receipt.targets.len(), 4);
    assert!(f.store.load_environment().unwrap().is_none());
    assert!(f.store.load_journal().unwrap().is_none());
    assert_eq!(
        super::super::ensure_upgrade_idle_under_lock(&f.store)
            .unwrap_err()
            .code,
        "upgrade_pending"
    );
    rollback_inner(&f.store, &f.runtime).unwrap();
    super::super::ensure_upgrade_idle_under_lock(&f.store).unwrap();
}

#[test]
fn package_trust_owner_target_receipt_and_old_bytes_are_independent_fences() {
    let f = Fixture::new();
    let mut unproven = f.candidate.clone();
    unproven.provenance_verified = false;
    assert_eq!(
        prepare_verified(&f.store, unproven, &f.runtime, InstallationKind::Legacy)
            .unwrap_err()
            .code,
        "candidate_provenance"
    );
    f.prepare();
    let mut different = f.candidate.clone();
    different.manifest_sha256 = "c".repeat(64);
    assert_eq!(
        verify_receipt_inner(&f.store, &different, &f.runtime)
            .unwrap_err()
            .code,
        "upgrade_receipt"
    );
    let other = f.runtime.parent().unwrap().join("other/webcodex-runtime");
    assert_eq!(
        load_bound(&f.store, &other).err().unwrap().code,
        "upgrade_receipt"
    );
    let mut journal: PackageJournal = f.store.read_json(JOURNAL).unwrap().unwrap();
    let original_owner = journal.owner_identity.clone();
    journal.owner_identity = "other-owner".into();
    f.store.write_json(JOURNAL, &journal).unwrap();
    assert_eq!(
        load_bound(&f.store, &f.runtime).err().unwrap().code,
        "upgrade_receipt"
    );
    journal.owner_identity = original_owner;
    f.store.write_json(JOURNAL, &journal).unwrap();
    std::fs::write(f.runtime.join("webcodex-runner.exe"), "changed").unwrap();
    assert_eq!(
        verify_receipt_inner(&f.store, &f.candidate, &f.runtime)
            .unwrap_err()
            .code,
        "upgrade_program_changed"
    );
}

#[tokio::test]
async fn partial_or_unlaunchable_replacement_rolls_back_all_four_files_and_keeps_profile() {
    let f = Fixture::new();
    let profile = f._temp.path().join("desktop-profile");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("mcp-providers.json"),
        "user-owned provider configuration",
    )
    .unwrap();
    f.prepare();
    f.replace();
    std::fs::write(
        f.runtime.join("webcodex-server.exe"),
        "partial installer failure",
    )
    .unwrap();
    assert_eq!(
        finish_inner(&f.store, &f.runtime).await.unwrap_err().code,
        "upgrade_installed_hash"
    );
    rollback_inner(&f.store, &f.runtime).unwrap();
    rollback_inner(&f.store, &f.runtime).unwrap();
    f.assert_original();
    assert_eq!(
        std::fs::read_to_string(profile.join("mcp-providers.json")).unwrap(),
        "user-owned provider configuration"
    );
    f.prepare();
    f.replace();
    assert!(finish_inner(&f.store, &f.runtime).await.is_err());
    rollback_inner(&f.store, &f.runtime).unwrap();
    f.assert_original();
}

#[test]
fn backup_corruption_blocks_restoration_without_overwriting_installed_files() {
    let f = Fixture::new();
    f.prepare();
    f.replace();
    let journal: PackageJournal = f.store.read_json(JOURNAL).unwrap().unwrap();
    std::fs::write(&journal.programs[0].backup, "tampered").unwrap();
    assert_eq!(
        rollback_inner(&f.store, &f.runtime).unwrap_err().code,
        "upgrade_backup_changed"
    );
    for (name, path) in targets(&f.runtime).unwrap() {
        assert_eq!(digest(&path).unwrap(), f.candidate.artifacts[&name].sha256);
    }
}

#[tokio::test]
async fn environment_owner_path_is_preserved_and_package_coordinator_cannot_adopt_it() {
    let f = Fixture::new();
    let files = targets(&f.runtime).unwrap();
    let mut record = EnvironmentRecord {
        schema_version: 1,
        environment_id: "original-environment".into(),
        request: SetupRequest {
            service_scope: crate::service::ServiceScope::User,
            mode: EnvironmentMode::Join,
            server_url: "https://fixture.invalid".into(),
            project: None,
            runner: Some(true),
            account: current_account().unwrap(),
            binaries: RuntimeBinaries {
                cli: files["webcodex"].clone(),
                server: files["webcodex-server"].clone(),
                runner: files["webcodex-runner"].clone(),
            },
        },
        username: None,
        runner_client_id: Some("original-runner".into()),
        projects: Vec::new(),
        configured: true,
    };
    f.store.save_environment(&record).unwrap();
    assert_eq!(
        classify_inner(&f.store, &f.runtime).await.unwrap(),
        InstallationKind::Environment
    );
    assert_eq!(
        no_environment(&f.store).unwrap_err().code,
        "installer_environment"
    );
    record.request.account.identity = "other-user".into();
    f.store.save_environment(&record).unwrap();
    assert_eq!(
        classify_inner(&f.store, &f.runtime).await.unwrap_err().code,
        "installer_owner"
    );
    assert_eq!(
        f.store.load_environment().unwrap().unwrap().environment_id,
        "original-environment"
    );
}

// Native PE fixtures intentionally implement the old v0.4.3 wire identity and
// reject Environment with exit 2. They never open the real Desktop data root.
fn native_program(path: &Path, info: &MachineBuildInfo) {
    let source = path.with_extension("rs");
    let encoded = serde_json::to_string(info).unwrap();
    std::fs::write(&source,format!(r#"fn main() {{
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("--build-info-json") {{ println!("{{}}", {encoded:?}); }}
        else if args.get(1).map(String::as_str) == Some("--version") {{ println!("webcodex 0.4.3"); }}
        else {{ std::process::exit(2); }}
    }}"#)).unwrap();
    let output = std::process::Command::new("rustc")
        .arg("--crate-name")
        .arg("legacy_fixture")
        .arg(&source)
        .arg("-o")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success(), "native fixture compilation failed");
    #[cfg(windows)]
    crate::storage::secure_windows_path(path).unwrap();
}

#[tokio::test]
async fn pre_environment_detection_uses_v043_build_identity_not_unknown_command_exit() {
    let f = Fixture::new();
    for (name, path) in targets(&f.runtime).unwrap() {
        let mut info = f.candidate.artifacts[&name].build_info.clone();
        info.version = "0.4.3".into();
        info.git_commit = Some(OFFICIAL_V043.into());
        info.environment_data_format = None;
        native_program(&path, &info);
    }
    assert_eq!(
        classify_inner(&f.store, &f.runtime).await.unwrap(),
        InstallationKind::Legacy
    );
    let checked = UpgradePreflight {
        ready: true,
        candidate: f.candidate.clone(),
        diagnostics: vec![],
        active_tasks: 0,
    };
    assert!(
        preflight_verified(&f.store, &f.runtime, checked)
            .await
            .unwrap()
            .ready
    );
    let mut unsupported = f.candidate.artifacts["webcodex"].build_info.clone();
    unsupported.environment_data_format = None;
    unsupported.version = "0.4.2".into();
    native_program(&f.runtime.join("webcodex.exe"), &unsupported);
    assert_eq!(
        classify_inner(&f.store, &f.runtime).await.unwrap_err().code,
        "legacy_identity"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn native_v043_legacy_detection_receipt_replacement_finish_and_rollback() {
    let mut f = Fixture::new();
    for (name, target) in targets(&f.runtime).unwrap() {
        let mut old = f.candidate.artifacts[&name].build_info.clone();
        old.version = "0.4.3".into();
        old.git_commit = Some(OFFICIAL_V043.into());
        old.environment_data_format = None;
        native_program(&target, &old);
        let artifact = f.candidate.artifacts.get_mut(&name).unwrap();
        native_program(&artifact.path, &artifact.build_info);
        artifact.sha256 = digest(&artifact.path).unwrap();
    }
    f.candidate.desktop.as_mut().unwrap().managed_files.insert(
        "WebCodex.exe".into(),
        f.candidate.artifacts["webcodex-desktop"].sha256.clone(),
    );
    let rejected = std::process::Command::new(f.runtime.join("webcodex.exe"))
        .args(["environment", "upgrade-preflight", "--json"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(2));
    assert_eq!(
        classify(&f.store, &f.runtime).await.unwrap(),
        InstallationKind::Legacy
    );
    // Published candidate verification has its own tests; exercise the exact
    // native legacy preflight boundary with an independently verified fixture.
    let checked = UpgradePreflight {
        ready: true,
        candidate: f.candidate.clone(),
        diagnostics: vec![],
        active_tasks: 0,
    };
    assert!(
        preflight_verified(&f.store, &f.runtime, checked)
            .await
            .unwrap()
            .ready
    );
    let originals: BTreeMap<_, _> = targets(&f.runtime)
        .unwrap()
        .into_iter()
        .map(|(n, p)| (n, digest(&p).unwrap()))
        .collect();
    f.prepare();
    verify_receipt_inner(&f.store, &f.candidate, &f.runtime).unwrap();
    f.replace();
    finish(&f.store, &f.runtime).await.unwrap();
    finish(&f.store, &f.runtime).await.unwrap();
    assert_eq!(
        rollback(&f.store, &f.runtime).unwrap_err().code,
        "upgrade_committed"
    );
    // A separate failed transaction restores native old programs exactly.
    let original_journal: PackageJournal = f.store.read_json(JOURNAL).unwrap().unwrap();
    for p in &original_journal.programs {
        std::fs::copy(&p.backup, &p.target).unwrap();
    }
    f.prepare();
    f.replace();
    std::fs::remove_file(f.runtime.join("webcodex-runner.exe")).unwrap();
    assert!(finish(&f.store, &f.runtime).await.is_err());
    rollback(&f.store, &f.runtime).unwrap();
    for (name, path) in targets(&f.runtime).unwrap() {
        assert_eq!(digest(&path).unwrap(), originals[&name]);
    }
    assert!(f.store.load_environment().unwrap().is_none());
}
