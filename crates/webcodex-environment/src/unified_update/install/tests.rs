use super::super::download::{DownloadPhase, PendingInstall};
use super::*;

fn record() -> UpdateRecord {
    let mut record = UpdateRecord::default();
    record.version = Some("1.2.3".into());
    let platform = crate::unified_update::RuntimePlatform::current().unwrap();
    record.target = crate::unified_update::InstallerTarget::default_for_non_linux(platform)
        .or_else(|| {
            crate::unified_update::InstallerTarget::for_platform(
                platform,
                crate::unified_update::PackageFormat::Deb,
            )
        });
    record.source_sha = Some("a".repeat(40));
    record.source_manifest_sha256 = Some("b".repeat(64));
    record.pending = Some(PendingInstall {
        environment_id: "environment".into(),
        operation_id: Some("operation".into()),
        started_at_ms: 100,
    });
    record.phase = DownloadPhase::InstallingOrHandedOff;
    record
}
fn observation(outcome: UpgradeOutcome) -> UpgradeObservation {
    UpgradeObservation {
        environment_id: "environment".into(),
        operation_id: "operation".into(),
        version: "1.2.3".into(),
        source_sha: "a".repeat(40),
        manifest_sha256: "b".repeat(64),
        outcome,
    }
}
fn build(version: &str) -> webcodex_core::desktop_runtime_contract::MachineBuildInfo {
    let mut build: webcodex_core::desktop_runtime_contract::MachineBuildInfo = serde_json::from_value(serde_json::json!({"schema_version":1,"binary":"webcodex-desktop","version":"1.2.3","git_commit":null,"git_dirty":false,"built_at":null,"target":"x86_64-unknown-linux-gnu","architecture":"x86_64","desktop_runtime_contract":{"min_generation":1,"max_generation":1}})).unwrap();
    build.version = version.into();
    build.git_commit = Some("a".repeat(40));
    build.git_dirty = Some(false);
    build
}

#[test]
fn handoff_never_means_installed_without_version_and_core_commit() {
    let record = record();
    let current = build("1.2.2");
    assert_eq!(
        reconcile(&record, None, Some(&current), 101),
        Reconciliation::RecoveryRequired
    );
    assert_eq!(
        reconcile(
            &record,
            Some(&observation(UpgradeOutcome::Pending)),
            Some(&current),
            101
        ),
        Reconciliation::InProgress
    );
    assert_eq!(
        reconcile(
            &record,
            Some(&observation(UpgradeOutcome::Pending)),
            Some(&current),
            2_000_000
        ),
        Reconciliation::RecoveryRequired
    );
    assert_eq!(
        reconcile(
            &record,
            Some(&observation(UpgradeOutcome::Committed)),
            Some(&current),
            101
        ),
        Reconciliation::RecoveryRequired
    );
    assert_eq!(
        reconcile(
            &record,
            Some(&observation(UpgradeOutcome::Committed)),
            Some(&build("1.2.3")),
            101
        ),
        Reconciliation::Installed
    );
    assert_eq!(
        reconcile(
            &record,
            Some(&observation(UpgradeOutcome::RolledBack)),
            Some(&current),
            101
        ),
        Reconciliation::Restored
    );
}

#[test]
fn changed_owner_operation_source_or_dirty_build_cannot_clear_pending() {
    for changed in ["owner", "operation", "source", "manifest"] {
        let mut observed = observation(UpgradeOutcome::Pending);
        match changed {
            "owner" => observed.environment_id = "other".into(),
            "operation" => observed.operation_id = "other".into(),
            "source" => observed.source_sha = "c".repeat(40),
            _ => observed.manifest_sha256 = "c".repeat(64),
        }
        assert_eq!(
            reconcile(&record(), Some(&observed), Some(&build("1.2.2")), 101),
            Reconciliation::RecoveryRequired
        );
    }
    let mut dirty = build("1.2.3");
    dirty.git_dirty = Some(true);
    assert_eq!(
        reconcile(
            &record(),
            Some(&observation(UpgradeOutcome::Committed)),
            Some(&dirty),
            101
        ),
        Reconciliation::RecoveryRequired
    );
}

#[test]
fn superseding_committed_generation_requires_manual_reconciliation() {
    let mut observed = observation(UpgradeOutcome::Committed);
    observed.version = "1.2.4".into();
    observed.operation_id = "newer-operation".into();
    assert_eq!(
        reconcile(&record(), Some(&observed), Some(&build("1.2.4")), 101),
        Reconciliation::RecoveryRequired
    );
}

#[test]
fn repeated_target_with_different_operation_never_clears_pending() {
    let mut observed = observation(UpgradeOutcome::Committed);
    observed.operation_id = "replacement-operation".into();
    assert_eq!(
        reconcile(&record(), Some(&observed), Some(&build("1.2.3")), 101),
        Reconciliation::RecoveryRequired
    );
}

#[test]
fn verified_installed_disk_identity_owns_committed_reconciliation() {
    let old_running_desktop = build("1.2.2");
    let verified_disk_desktop = build("1.2.3");
    assert_ne!(old_running_desktop.version, verified_disk_desktop.version);
    assert_eq!(
        reconcile(
            &record(),
            Some(&observation(UpgradeOutcome::Committed)),
            Some(&verified_disk_desktop),
            101
        ),
        Reconciliation::Installed
    );
    assert_eq!(
        reconcile(
            &record(),
            Some(&observation(UpgradeOutcome::Committed)),
            None,
            101
        ),
        Reconciliation::RecoveryRequired
    );
}

#[test]
fn unacknowledged_windows_spawn_cannot_attach_a_later_same_candidate_operation() {
    let mut record = record();
    record.pending.as_mut().unwrap().operation_id = None;
    for outcome in [
        UpgradeOutcome::Pending,
        UpgradeOutcome::Committed,
        UpgradeOutcome::RolledBack,
    ] {
        assert_eq!(
            reconcile(
                &record,
                Some(&observation(outcome)),
                Some(&build("1.2.3")),
                101
            ),
            Reconciliation::RecoveryRequired
        );
    }
}

#[tokio::test]
async fn loaded_manager_install_rejects_externally_pending_handoff_before_writes() {
    struct CountingLauncher(std::sync::atomic::AtomicUsize);
    impl LaunchAdapter for CountingLauncher {
        fn supported(&self, _: unified::InstallerTarget) -> bool {
            true
        }
        fn launch<'a>(
            &'a self,
            _: LaunchRequest<'a>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LaunchOutcome> + Send + 'a>>
        {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Box::pin(async { LaunchOutcome::Unknown })
        }
    }
    let temp = crate::test_tempdir().unwrap();
    let manager = UpdateManager::with_environment_root(
        temp.path().join("desktop"),
        temp.path().join("absent-environment"),
    );
    manager
        .download_now(
            None,
            false,
            true,
            InstallationKind::Managed,
            None,
            &CancellationSignal::new(),
        )
        .await
        .unwrap();
    let mut pending = record();
    pending.version = Some("99.0.0".into());
    pending.sha256 = Some("c".repeat(64));
    pending.verified_at_ms = Some(100);
    pending.pending.as_mut().unwrap().operation_id = Some(uuid::Uuid::new_v4().to_string());
    manager.change(|state| {
        *state = pending.clone();
        state.pending = None;
        state.phase = DownloadPhase::Available;
    });
    let other = PrivateUpdateCache::open(manager.root.clone()).unwrap();
    let bytes = serde_json::to_vec(&pending).unwrap();
    {
        let _lock = other.lock().unwrap();
        other.write("update-state.json", &bytes).unwrap();
    }
    let identity = unified::CandidateIdentity {
        version: pending.version.clone().unwrap(),
        target: pending.target.unwrap(),
        source_sha: pending.source_sha.clone().unwrap(),
        manifest_sha256: pending.source_manifest_sha256.clone().unwrap(),
        installer_sha256: pending.sha256.clone().unwrap(),
    };
    let context = InstallContext {
        environment_root: manager.environment_root.clone(),
        environment_id: "environment".into(),
        binaries: crate::RuntimeBinaries {
            cli: temp.path().join("webcodex"),
            server: temp.path().join("webcodex-server"),
            runner: temp.path().join("webcodex-runner"),
        },
        desktop: temp.path().join("webcodex-desktop"),
        build: build("98.0.0"),
        target: identity.target,
    };
    let target = UpgradeTarget {
        environment_id: context.environment_id.clone(),
        manifest_sha256: identity.manifest_sha256.clone(),
        operation_id: None,
    };
    let launcher = CountingLauncher(std::sync::atomic::AtomicUsize::new(0));
    let result = manager
        .install_checked(&context, &identity, &target, true, &launcher)
        .await;
    assert_eq!(
        other.read("update-state.json", 24 * 1024).unwrap().unwrap(),
        bytes
    );
    assert_eq!(result.unwrap_err(), UpdateError::RecoveryRequired);
    assert_eq!(launcher.0.load(std::sync::atomic::Ordering::Relaxed), 0);
}

fn guarded_fixture() -> (
    tempfile::TempDir,
    std::sync::Arc<UpdateManager>,
    crate::EnvironmentStore,
    UpgradeTarget,
) {
    let directory = crate::test_tempdir().unwrap();
    let store = crate::EnvironmentStore::open(directory.path().join("environment")).unwrap();
    let manager = UpdateManager::with_environment_root(
        directory.path().join("desktop"),
        store.root().to_path_buf(),
    );
    let target = UpgradeTarget {
        environment_id: "environment".into(),
        manifest_sha256: "b".repeat(64),
        operation_id: Some(uuid::Uuid::new_v4().to_string()),
    };
    let saved = crate::EnvironmentRecord {
        schema_version: 1,
        environment_id: target.environment_id.clone(),
        request: crate::SetupRequest {
            service_scope: crate::service::ServiceScope::System,
            mode: crate::EnvironmentMode::Join,
            server_url: "http://127.0.0.1:1".into(),
            project: None,
            runner: None,
            runner_display_name: None,
            account: crate::current_account().unwrap(),
            binaries: crate::RuntimeBinaries {
                cli: store.root().join("webcodex"),
                server: store.root().join("webcodex-server"),
                runner: store.root().join("webcodex-runner"),
            },
        },
        username: None,
        runner_client_id: None,
        projects: vec![],
        configured: true,
    };
    let _setup_lock = store.lock().unwrap();
    store.save_environment(&saved).unwrap();
    store.write_json("upgrade.json", &serde_json::json!({
        "schema_version":1,"operation_id":target.operation_id,"phase":"rolled_back",
        "record":saved,"services":[],"all_services":[],"inventory_complete":true,
        "programs":[],"data":null,"data_snapshot":null,
        "candidate":{"version":"1.2.3","source_sha":"a".repeat(40),
            "platform":"linux-x64","source_workflow_run_id":1,
            "source_workflow_ref":"yyjeqhc/webcodex/.github/workflows/release-build.yml@main",
            "manifest_sha256":target.manifest_sha256,"root":store.root(),"artifacts":{}}
    })).unwrap();
    let cache = PrivateUpdateCache::open(manager.root.clone()).unwrap();
    let _cache_lock = cache.lock().unwrap();
    let mut pending = record();
    pending.pending.as_mut().unwrap().operation_id = target.operation_id.clone();
    manager.change(|state| *state = pending);
    manager.persist(&cache).unwrap();
    (directory, manager, store, target)
}

#[tokio::test]
async fn guarded_terminal_reconciliation_clears_only_exact_rolled_back_pending() {
    let (_directory, manager, store, target) = guarded_fixture();
    let before_journal = std::fs::read(store.root().join("upgrade.json")).unwrap();
    manager.reconcile_pending_guarded(&target).await.unwrap();
    let cache = PrivateUpdateCache::open_existing(manager.root.clone())
        .unwrap()
        .unwrap();
    let state: UpdateRecord =
        serde_json::from_slice(&cache.read("update-state.json", 24 * 1024).unwrap().unwrap())
            .unwrap();
    assert!(state.pending.is_none());
    assert_eq!(state.phase, DownloadPhase::Failed);
    assert_eq!(state.error_kind, Some(UpdateError::UpgradeRolledBack));
    assert!(state.cancelled);
    assert_eq!(
        std::fs::read(store.root().join("upgrade.json")).unwrap(),
        before_journal
    );
    manager.reconcile_pending_guarded(&target).await.unwrap();
}

#[tokio::test]
async fn guarded_reconciliation_rejects_stale_bindings_before_any_saved_cleanup() {
    let (_directory, manager, store, target) = guarded_fixture();
    let state_file = manager.root.join("update-state.json");
    let before = std::fs::read(&state_file).unwrap();
    let version = manager.root.join("1.2.3");
    crate::storage::ensure_private_directory(&version).unwrap();
    let marker = version.join("installer.part");
    std::fs::write(&marker, b"retained-private-candidate").unwrap();
    for stale in [
        UpgradeTarget {
            environment_id: "other-environment".into(),
            ..target.clone()
        },
        UpgradeTarget {
            manifest_sha256: "c".repeat(64),
            ..target.clone()
        },
        UpgradeTarget {
            operation_id: Some(uuid::Uuid::new_v4().to_string()),
            ..target.clone()
        },
        UpgradeTarget {
            operation_id: None,
            ..target.clone()
        },
    ] {
        assert!(manager.reconcile_pending_guarded(&stale).await.is_err());
        assert_eq!(std::fs::read(&state_file).unwrap(), before);
        assert_eq!(
            std::fs::read(&marker).unwrap(),
            b"retained-private-candidate"
        );
    }
    let mut journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.root().join("upgrade.json")).unwrap()).unwrap();
    journal["operation_id"] = serde_json::json!(uuid::Uuid::new_v4().to_string());
    store.write_json("upgrade.json", &journal).unwrap();
    assert!(manager.reconcile_pending_guarded(&target).await.is_err());
    assert_eq!(std::fs::read(&state_file).unwrap(), before);
    journal["operation_id"] = serde_json::json!(target.operation_id);
    journal["phase"] = serde_json::json!("snapshot_ready");
    store.write_json("upgrade.json", &journal).unwrap();
    assert!(manager.reconcile_pending_guarded(&target).await.is_err());
    assert_eq!(std::fs::read(&state_file).unwrap(), before);
}

#[tokio::test]
async fn guarded_reconciliation_never_provisions_missing_cache_root_or_fences() {
    let directory = crate::test_tempdir().unwrap();
    let data = directory.path().join("absent-desktop");
    let environment = directory.path().join("absent-environment");
    let manager = UpdateManager::with_environment_root(data.clone(), environment.clone());
    let target = UpgradeTarget {
        environment_id: "environment".into(),
        manifest_sha256: "b".repeat(64),
        operation_id: Some(uuid::Uuid::new_v4().to_string()),
    };
    manager.reconcile_pending_guarded(&target).await.unwrap();
    assert!(!data.exists());
    assert!(!environment.exists());
    let cache = PrivateUpdateCache::open(manager.root.clone()).unwrap();
    assert!(manager.reconcile_pending_guarded(&target).await.is_err());
    assert!(!cache.root().join("update.lock").exists());
    let _lock = cache.lock().unwrap();
    let mut pending = record();
    pending.pending.as_mut().unwrap().operation_id = target.operation_id.clone();
    manager.change(|state| *state = pending);
    manager.persist(&cache).unwrap();
    drop(_lock);
    let before = std::fs::read(manager.root.join("update-state.json")).unwrap();
    assert!(manager.reconcile_pending_guarded(&target).await.is_err());
    assert_eq!(
        std::fs::read(manager.root.join("update-state.json")).unwrap(),
        before
    );
    assert!(!environment.exists());
}
