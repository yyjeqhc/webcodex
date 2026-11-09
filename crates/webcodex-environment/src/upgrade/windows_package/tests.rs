use super::*;

// MachineBuildInfo wire shape from v0.4.3's
// crates/webcodex-core/src/{build_info,desktop_runtime_contract}.rs. These are
// literal release identities, not values derived from the classifier. v0.4.3
// had agent_protocol_generation (null for CLI, 2 for Server/Runner) and no
// environment_data_format. built_at is a synthetic, valid timestamp.
const V043_BUILD_INFO: [&str; 2] = [
    r#"{"schema_version":1,"binary":"webcodex","version":"0.4.3","git_commit":"b96a59a712ca","git_dirty":false,"built_at":"1","target":"x86_64-pc-windows-msvc","architecture":"x86_64","desktop_runtime_contract":{"min_generation":1,"max_generation":1},"agent_protocol_generation":null}"#,
    r#"{"schema_version":1,"binary":"webcodex","version":"0.4.3","git_commit":"b96a59a712ca","git_dirty":false,"built_at":"1","target":"aarch64-pc-windows-msvc","architecture":"aarch64","desktop_runtime_contract":{"min_generation":1,"max_generation":1},"agent_protocol_generation":null}"#,
];

fn v043_builds(json: &str) -> [MachineBuildInfo; 3] {
    ["webcodex", "webcodex-server", "webcodex-runner"].map(|name| {
        let mut value: serde_json::Value = serde_json::from_str(json).unwrap();
        value["binary"] = json!(name);
        if name != "webcodex" {
            value["agent_protocol_generation"] = json!(2);
        }
        let info: MachineBuildInfo = serde_json::from_value(value).unwrap();
        info.validate(name).unwrap();
        info
    })
}

#[test]
fn v043_published_windows_identities_classify_as_legacy() {
    for fixture in V043_BUILD_INFO {
        let mut infos = v043_builds(fixture);
        assert!(infos.iter().all(pre_environment));
        assert_eq!(classify_builds(&infos).unwrap(), InstallationKind::Legacy);
        for info in &mut infos {
            info.git_commit = Some("b96a59a712ca5355ad8609cd861cd0c7acb9f99e".into());
        }
        assert_eq!(classify_builds(&infos).unwrap(), InstallationKind::Legacy);
    }
    assert_eq!(
        serde_json::to_string(&InstallationKind::Legacy).unwrap(),
        "\"legacy\""
    );
    assert_eq!(
        serde_json::from_str::<InstallationKind>("\"legacy\"").unwrap(),
        InstallationKind::Legacy
    );
}

#[test]
fn v043_legacy_identity_rejects_lookalikes_and_mixed_components() {
    for fixture in V043_BUILD_INFO {
        let original = v043_builds(fixture);
        for (field, value) in [
            ("git_commit", json!("b96a59a712cb")),
            ("git_commit", json!("b96a59a712c")),
            ("git_commit", json!("b96a59a712ca5")),
            (
                "git_commit",
                json!("b96a59a712ca5355ad8609cd861cd0c7acb9f990"),
            ),
            ("git_commit", serde_json::Value::Null),
            ("git_dirty", json!(true)),
            ("git_dirty", serde_json::Value::Null),
            ("version", json!("0.4.2")),
            ("target", json!("x86_64-pc-windows-gnu")),
            ("target", json!("x86_64-unknown-linux-gnu")),
            ("architecture", json!("arm")),
            ("environment_data_format", json!(1)),
            ("environment_data_format", json!(2)),
        ] {
            for component in 0..3 {
                let mut infos = original.clone();
                let mut changed = serde_json::to_value(&infos[component]).unwrap();
                changed[field] = value.clone();
                infos[component] = serde_json::from_value(changed).unwrap();
                assert!(!pre_environment(&infos[component]), "{field}");
                assert_eq!(
                    classify_builds(&infos).unwrap_err().code,
                    "package_identity",
                    "{field}"
                );
            }
        }
        // Both forms and both architectures are individually accepted, but a
        // package must still report the same exact identity in all components.
        let mut mixed = original.clone();
        mixed[1].git_commit = Some("b96a59a712ca5355ad8609cd861cd0c7acb9f99e".into());
        assert_eq!(
            classify_builds(&mixed).unwrap_err().code,
            "package_identity"
        );
        let mut mixed = original;
        mixed[2] = v043_builds(if fixture == V043_BUILD_INFO[0] {
            V043_BUILD_INFO[1]
        } else {
            V043_BUILD_INFO[0]
        })[2]
            .clone();
        assert_eq!(
            classify_builds(&mixed).unwrap_err().code,
            "package_identity"
        );
    }
}

#[test]
fn offline_preview_candidates_are_never_published_or_silently_trusted() {
    let f = Fixture::new(InstallationKind::Legacy);
    let mut candidate = f.candidate.clone();
    candidate.provenance_verified = false;
    candidate.source_workflow_run_id = 0;
    candidate.source_workflow_ref = "webcodex/local-preview".into();
    assert_eq!(
        candidate_authorized(&candidate),
        cfg!(all(windows, feature = "local-preview-installer"))
    );
    assert_eq!(
        require_local_preview(&candidate).is_ok(),
        cfg!(all(windows, feature = "local-preview-installer"))
    );
    candidate.source_workflow_ref = "webcodex/local-preview/other".into();
    assert!(!candidate_authorized(&candidate));
    assert!(require_local_preview(&candidate).is_err());
    candidate.source_workflow_ref = "webcodex/local-preview".into();
    candidate.source_workflow_run_id = 1;
    assert!(!candidate_authorized(&candidate));
    candidate.source_workflow_run_id = 0;
    candidate.provenance_verified = true;
    assert!(candidate_authorized(&candidate));
    assert!(require_local_preview(&candidate).is_err());
}

// A native host fixture reports the historical Windows wire shape without
// accessing user state. This exercises probing/preflight on macOS too; native
// Windows executable locking, ACLs and installer acceptance remain separate.
fn installed_v043_program(path: &Path, info: &MachineBuildInfo) {
    let source = path.with_extension("rs");
    let encoded = serde_json::to_string(info).unwrap();
    std::fs::write(&source, format!(r#"fn main() {{
        if std::env::args().nth(1).as_deref() == Some("--build-info-json") {{ println!("{{}}", {encoded:?}); }}
        else {{ std::process::exit(2); }}
    }}"#)).unwrap();
    let output = std::process::Command::new("rustc")
        .args(["--crate-name", "legacy_fixture"])
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
async fn v043_probe_preflight_prepare_and_partial_rollback() {
    let f = Fixture::new(InstallationKind::Legacy);
    for info in v043_builds(V043_BUILD_INFO[0]) {
        installed_v043_program(&f.runtime.join(format!("{}.exe", info.binary)), &info);
    }
    assert_eq!(
        classify_inner(&f.store, &f.runtime).await.unwrap(),
        InstallationKind::Legacy
    );
    let mut checked = UpgradePreflight {
        ready: true,
        candidate: f.candidate.clone(),
        diagnostics: vec![],
        active_tasks: 0,
    };
    checked.candidate.provenance_verified = false;
    assert_eq!(
        preflight_verified(&f.store, &f.runtime, checked.clone())
            .await
            .unwrap_err()
            .code,
        "candidate_provenance"
    );
    checked.candidate.provenance_verified = true;
    assert!(
        preflight_verified(&f.store, &f.runtime, checked.clone())
            .await
            .unwrap()
            .ready
    );
    let originals: BTreeMap<_, _> = targets(&f.runtime)
        .unwrap()
        .into_iter()
        .map(|(name, path)| (name, digest(&path).unwrap()))
        .collect();
    f.prepare();
    assert_eq!(
        load_bound(&f.store, &f.runtime).unwrap().kind,
        InstallationKind::Legacy
    );
    assert!(
        preflight_verified(&f.store, &f.runtime, checked)
            .await
            .unwrap()
            .ready
    );
    verify_receipt_inner(&f.store, &f.candidate, &f.runtime).unwrap();
    f.replace();
    std::fs::remove_file(f.runtime.join("webcodex-runner.exe")).unwrap();
    assert!(finish_inner(&f.store, &f.runtime).await.is_err());
    // Recovery retains Legacy even when the partial install cannot be probed.
    assert_eq!(
        classify_inner(&f.store, &f.runtime).await.unwrap(),
        InstallationKind::Legacy
    );
    rollback_inner(&f.store, &f.runtime).unwrap();
    rollback_inner(&f.store, &f.runtime).unwrap();
    for (name, path) in targets(&f.runtime).unwrap() {
        assert_eq!(digest(&path).unwrap(), originals[&name]);
    }
    assert!(f.store.load_environment().unwrap().is_none());
}

struct Fixture {
    _temp: tempfile::TempDir,
    store: EnvironmentStore,
    runtime: PathBuf,
    candidate: UpgradeCandidate,
    kind: InstallationKind,
}
impl Fixture {
    fn new(kind: InstallationKind) -> Self {
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
            kind,
            _temp: temp,
            store,
            runtime,
            candidate: UpgradeCandidate {
                package_flavor: crate::unified_update::PackageFlavor::Full,
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
            self.kind,
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
    for kind in [InstallationKind::Legacy, InstallationKind::Unconfigured] {
        let f = Fixture::new(kind);
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
}

#[test]
fn package_trust_owner_target_receipt_and_old_bytes_are_independent_fences() {
    for kind in [InstallationKind::Legacy, InstallationKind::Unconfigured] {
        let f = Fixture::new(kind);
        let mut unproven = f.candidate.clone();
        unproven.provenance_verified = false;
        assert_eq!(
            prepare_verified(&f.store, unproven, &f.runtime, kind)
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
}

#[tokio::test]
async fn partial_or_unlaunchable_replacement_rolls_back_all_four_files_and_keeps_profile() {
    for kind in [InstallationKind::Legacy, InstallationKind::Unconfigured] {
        let f = Fixture::new(kind);
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
}

#[test]
fn backup_corruption_blocks_restoration_without_overwriting_installed_files() {
    for kind in [InstallationKind::Legacy, InstallationKind::Unconfigured] {
        let f = Fixture::new(kind);
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
}

#[tokio::test]
async fn environment_owner_path_is_preserved_and_package_coordinator_cannot_adopt_it() {
    let f = Fixture::new(InstallationKind::Legacy);
    let files = targets(&f.runtime).unwrap();
    let mut record = EnvironmentRecord {
        schema_version: 1,
        environment_id: "original-environment".into(),
        request: SetupRequest {
            runner_display_name: None,
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
