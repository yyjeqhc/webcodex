use super::*;

fn fixture(root: &Path, phase: Phase) -> UpgradeJournal {
    let mut journal = super::super::tests::fixture(root, phase);
    journal.record.request.account = crate::current_account().unwrap();
    journal
}
fn saved(phase: Phase) -> (tempfile::TempDir, EnvironmentStore, UpgradeJournal) {
    let directory = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(directory.path().join("environment")).unwrap();
    let journal = fixture(store.root(), phase);
    let _lock = store.lock().unwrap();
    store.save_environment(&journal.record).unwrap();
    save(&store, &journal).unwrap();
    (directory, store, journal)
}

#[test]
fn every_persisted_phase_remains_distinct_and_terminal_history_survives_without_cache() {
    let (_directory, store, mut journal) = saved(Phase::Prepared);
    for (phase, expected) in [
        (Phase::Prepared, UpgradePhase::Prepared),
        (Phase::Stopping, UpgradePhase::Stopping),
        (Phase::Stopped, UpgradePhase::Stopped),
        (Phase::SnapshotReady, UpgradePhase::SnapshotReady),
        (Phase::Verifying, UpgradePhase::Verifying),
        (Phase::Committed, UpgradePhase::Committed),
        (Phase::Restoring, UpgradePhase::Restoring),
        (Phase::RolledBack, UpgradePhase::RolledBack),
        (Phase::RecoveryRequired, UpgradePhase::RecoveryRequired),
    ] {
        journal.phase = phase;
        save(&store, &journal).unwrap();
        let status = upgrade_status(&store).unwrap().unwrap();
        assert_eq!(status.schema_version, 1);
        assert_eq!(status.phase, expected);
        assert_eq!(status.operation_id, journal.operation_id);
        assert!(!store.root().join("updates").exists());
    }
}

#[test]
fn projection_uses_only_allowlisted_component_metadata_and_never_private_canaries() {
    let (_directory, store, mut journal) = saved(Phase::SnapshotReady);
    let canary = "PRIVATE_PATH_ARGV_ENV_RECEIPT_CANARY";
    journal.candidate.root = PathBuf::from(canary);
    journal.candidate.source_workflow_ref = canary.into();
    journal.record.username = Some(canary.into());
    journal.record.request.server_url = canary.into();
    journal.record.request.binaries.cli = PathBuf::from(canary);
    journal.data = Some(PathBuf::from(canary));
    journal.data_snapshot = Some(PathBuf::from(canary));
    journal.snapshot_digest = Some(canary.into());
    journal.programs = vec![ProgramBackup {
        target: PathBuf::from(canary),
        backup: PathBuf::from(canary),
        sha256: canary.into(),
        name: "webcodex".into(),
    }];
    journal.desktop = Some(DesktopBackup {
        target: PathBuf::from(canary),
        backup: PathBuf::from(canary),
        sha256: canary.into(),
        directory_modes_sha256: canary.into(),
    });
    journal.all_services = vec![ServiceSpec {
        scope: ServiceScope::User,
        id: canary.into(),
        component: Component::Runner,
        program: PathBuf::from(canary),
        args: vec![canary.into()],
        working_directory: PathBuf::from(canary),
        account: crate::service::ServiceAccount::WindowsVirtual {
            name: canary.into(),
        },
        config_identity: canary.into(),
        env_file: Some(PathBuf::from(canary)),
        environment: BTreeMap::from([(canary.into(), canary.into())]),
        linux_socket: None,
    }];
    store.save_environment(&journal.record).unwrap();
    save(&store, &journal).unwrap();
    store
        .write_json(PREPARED_RECEIPT, &json!({"private": canary}))
        .unwrap();
    let status = upgrade_status(&store).unwrap().unwrap();
    let serialized = serde_json::to_string(&status).unwrap();
    assert!(!serialized.contains(canary));
    assert!(serialized.len() <= MAX_STATUS_BYTES);
    assert_eq!(
        status.files,
        vec![UpgradeFileComponent::Cli, UpgradeFileComponent::Desktop]
    );
    assert_eq!(
        status.services,
        vec![UpgradeServiceComponent {
            component: UpgradeServiceKind::Runner,
            scope: ServiceScope::User,
        }]
    );
    assert!(status.service_inventory_complete);
    // Unrecognized names fail closed instead of copying arbitrary journal data.
    journal.programs[0].name = canary.into();
    save(&store, &journal).unwrap();
    assert!(upgrade_status(&store).is_err());
}

#[test]
fn status_queries_never_create_roots_or_fences_and_busy_or_unreadable_state_fails_closed() {
    let directory = crate::test_tempdir().unwrap();
    let absent = directory.path().join("absent").join("environment");
    assert!(upgrade_status_at(&absent).unwrap().is_none());
    assert!(!directory.path().join("absent").exists());
    let store = EnvironmentStore::open(directory.path().join("existing")).unwrap();
    assert!(upgrade_status(&store).is_err());
    assert!(!store.root().join("setup.lock").exists());
    let (_directory, store, journal) = saved(Phase::Committed);
    let lock = store.lock().unwrap();
    assert_eq!(upgrade_status(&store).unwrap_err().code, "setup_busy");
    drop(lock);
    store
        .write_json("upgrade.json", &json!({"invalid": true}))
        .unwrap();
    assert!(upgrade_status(&store).is_err());
    save(&store, &journal).unwrap();
    std::fs::remove_file(store.root().join("environment.json")).unwrap();
    assert!(upgrade_status(&store).is_err());
}

#[tokio::test]
async fn stale_guarded_targets_have_no_journal_or_receipt_effects() {
    let (_directory, store, journal) = saved(Phase::SnapshotReady);
    let before = std::fs::read(store.root().join("upgrade.json")).unwrap();
    let mut backend = NativeEnvironment::new().unwrap();
    let target = UpgradeTarget {
        environment_id: journal.record.environment_id.clone(),
        manifest_sha256: journal.candidate.manifest_sha256.clone(),
        operation_id: Some(journal.operation_id.clone()),
    };
    for stale in [
        UpgradeTarget {
            environment_id: "different-environment".into(),
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
        assert_eq!(
            backend
                .upgrade_prepare_guarded(&store, Path::new("/absent-candidate"), &stale)
                .await
                .unwrap_err()
                .code,
            "upgrade_target_changed"
        );
        assert_eq!(
            backend
                .upgrade_prepare_guarded_with_receipt(
                    &store,
                    Path::new("/absent-candidate"),
                    &stale
                )
                .await
                .unwrap_err()
                .code,
            "upgrade_target_changed"
        );
        assert_eq!(
            backend
                .upgrade_finish_guarded(&store, &stale)
                .await
                .unwrap_err()
                .code,
            "upgrade_target_changed"
        );
        assert_eq!(
            backend
                .upgrade_rollback_guarded(&store, &stale)
                .await
                .unwrap_err()
                .code,
            "upgrade_target_changed"
        );
        assert_eq!(
            std::fs::read(store.root().join("upgrade.json")).unwrap(),
            before
        );
        assert!(!store.root().join(PREPARED_RECEIPT).exists());
        assert!(!store.root().join("upgrade-backups").exists());
    }
    let _lock = store.lock().unwrap();
    assert_eq!(
        validate_target_under_lock(&store, &target, Some(&journal), Some(&"c".repeat(64)))
            .unwrap_err()
            .code,
        "upgrade_target_changed"
    );
}

#[test]
fn fresh_prepare_never_reuses_an_unfinished_operation_and_final_retries_stay_bound() {
    let (_directory, store, mut journal) = saved(Phase::Prepared);
    let fresh = UpgradeTarget {
        environment_id: journal.record.environment_id.clone(),
        manifest_sha256: journal.candidate.manifest_sha256.clone(),
        operation_id: None,
    };
    let _lock = store.lock().unwrap();
    assert!(validate_target_under_lock(
        &store,
        &fresh,
        Some(&journal),
        Some(&fresh.manifest_sha256)
    )
    .is_err());
    for phase in [Phase::Committed, Phase::RolledBack] {
        journal.phase = phase;
        validate_target_under_lock(&store, &fresh, Some(&journal), Some(&fresh.manifest_sha256))
            .unwrap();
        let bound = UpgradeTarget {
            operation_id: Some(journal.operation_id.clone()),
            ..fresh.clone()
        };
        validate_target_under_lock(&store, &bound, Some(&journal), None).unwrap();
        assert!(validate_target_under_lock(
            &store,
            &bound,
            Some(&journal),
            Some(&bound.manifest_sha256)
        )
        .is_err());
        assert!(validate_target_under_lock(&store, &bound, None, None).is_err());
    }
}

#[test]
fn identity_strings_are_bounded_before_serialization() {
    let directory = crate::test_tempdir().unwrap();
    let mut journal = fixture(directory.path(), Phase::Prepared);
    journal.record.environment_id = "x".repeat(MAX_STATUS_BYTES);
    assert!(project(&journal, &journal.record).is_err());
    journal.record.environment_id = "env-fixture".into();
    journal.candidate.version = format!("1.0.{}", "1".repeat(MAX_STATUS_BYTES));
    assert!(project(&journal, &journal.record).is_err());
}

#[cfg(unix)]
#[test]
fn an_absent_root_cannot_hide_an_unsafe_existing_ancestor() {
    let directory = crate::test_tempdir().unwrap();
    let real = directory.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let link = directory.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(upgrade_status_at(&link.join("absent-environment")).is_err());
    assert!(!real.join("absent-environment").exists());
}

#[tokio::test]
async fn headless_recovery_checks_the_current_phase_inside_the_effect_lock() {
    let (_directory, store, mut journal) = saved(Phase::Prepared);
    let mut backend = NativeEnvironment::new().unwrap();
    let target = UpgradeTarget {
        environment_id: journal.record.environment_id.clone(),
        manifest_sha256: journal.candidate.manifest_sha256.clone(),
        operation_id: Some(journal.operation_id.clone()),
    };
    for phase in [
        Phase::SnapshotReady,
        Phase::Verifying,
        Phase::Restoring,
        Phase::RecoveryRequired,
    ] {
        // Simulate a phase transition after the caller selected the operation.
        journal.phase = phase;
        save(&store, &journal).unwrap();
        let before = std::fs::read(store.root().join("upgrade.json")).unwrap();
        assert_eq!(
            backend
                .upgrade_finish_headless_guarded(&store, &target)
                .await
                .unwrap_err()
                .code,
            "upgrade_manual_recovery"
        );
        assert_eq!(
            backend
                .upgrade_rollback_headless_guarded(&store, &target)
                .await
                .unwrap_err()
                .code,
            "upgrade_manual_recovery"
        );
        assert_eq!(
            std::fs::read(store.root().join("upgrade.json")).unwrap(),
            before
        );
    }
    journal.phase = Phase::Prepared;
    journal.replacement_started = true;
    save(&store, &journal).unwrap();
    assert_eq!(
        backend
            .upgrade_rollback_headless_guarded(&store, &target)
            .await
            .unwrap_err()
            .code,
        "upgrade_manual_recovery"
    );
    journal.replacement_started = false;
    save(&store, &journal).unwrap();
    store
        .write_json(PREPARED_RECEIPT, &prepared_receipt(store.root(), &journal))
        .unwrap();
    assert_eq!(
        backend
            .upgrade_rollback_headless_guarded(&store, &target)
            .await
            .unwrap_err()
            .code,
        "upgrade_manual_recovery"
    );
    std::fs::remove_file(store.root().join(PREPARED_RECEIPT)).unwrap();
    let program = store.root().join("program");
    std::fs::write(&program, b"original").unwrap();
    journal.programs.push(ProgramBackup {
        target: program.clone(),
        backup: store.root().join("program-backup"),
        sha256: digest(&program).unwrap(),
        name: "webcodex".into(),
    });
    save(&store, &journal).unwrap();
    let before = std::fs::read(store.root().join("upgrade.json")).unwrap();
    std::fs::write(&program, b"replacement-may-be-active").unwrap();
    assert_eq!(
        backend
            .upgrade_rollback_headless_guarded(&store, &target)
            .await
            .unwrap_err()
            .code,
        "upgrade_manual_recovery"
    );
    assert_eq!(
        std::fs::read(store.root().join("upgrade.json")).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read(&program).unwrap(),
        b"replacement-may-be-active"
    );
    journal.programs.clear();
    for phase in [Phase::Prepared, Phase::Stopping, Phase::Stopped] {
        journal.phase = phase;
        save(&store, &journal).unwrap();
        assert_eq!(
            backend
                .upgrade_finish_headless_guarded(&store, &target)
                .await
                .unwrap_err()
                .code,
            "upgrade_manual_recovery"
        );
        // Empty service/file inventory is intentionally used: this exercises
        // admission and durable decisions without invoking any host service.
        backend
            .upgrade_rollback_headless_guarded(&store, &target)
            .await
            .unwrap();
        let status = upgrade_status(&store).unwrap().unwrap();
        assert_eq!(status.phase, UpgradePhase::RolledBack);
    }
    for phase in [Phase::Committed, Phase::RolledBack] {
        journal.phase = phase;
        save(&store, &journal).unwrap();
        let before = std::fs::read(store.root().join("upgrade.json")).unwrap();
        if phase == Phase::Committed {
            backend
                .upgrade_finish_headless_guarded(&store, &target)
                .await
                .unwrap();
            assert_eq!(
                backend
                    .upgrade_rollback_headless_guarded(&store, &target)
                    .await
                    .unwrap_err()
                    .code,
                "upgrade_manual_recovery"
            );
        } else {
            backend
                .upgrade_rollback_headless_guarded(&store, &target)
                .await
                .unwrap();
        }
        assert_eq!(
            std::fs::read(store.root().join("upgrade.json")).unwrap(),
            before
        );
    }
}

#[tokio::test]
async fn task_count_rejects_stale_unconfigured_or_foreign_records_before_credentials_or_network() {
    let (_directory, store, mut journal) = saved(Phase::Prepared);
    let backend = NativeEnvironment::new().unwrap();
    // No credential exists and the fixture endpoint is unreachable. These
    // failures must happen before either credential lookup or an HTTP request.
    assert_eq!(
        backend
            .upgrade_task_count(&store, "other-env")
            .await
            .unwrap_err()
            .code,
        "upgrade_target_changed"
    );
    journal.record.configured = false;
    store.save_environment(&journal.record).unwrap();
    assert_eq!(
        backend
            .upgrade_task_count(&store, &journal.record.environment_id)
            .await
            .unwrap_err()
            .code,
        "upgrade_tasks_unknown"
    );
    journal.record.configured = true;
    journal.record.request.account.identity = "foreign-owner".into();
    store.save_environment(&journal.record).unwrap();
    assert_eq!(
        backend
            .upgrade_task_count(&store, &journal.record.environment_id)
            .await
            .unwrap_err()
            .code,
        "upgrade_owner_unknown"
    );
    assert!(!store.root().join("webcodex-user-token").exists());
    std::fs::remove_file(store.root().join("setup.lock")).unwrap();
    assert!(backend
        .upgrade_task_count(&store, &journal.record.environment_id)
        .await
        .is_err());
    assert!(!store.root().join("setup.lock").exists());
}

#[test]
fn task_count_rechecks_the_complete_saved_target_after_reading() {
    let directory = crate::test_tempdir().unwrap();
    let before = fixture(directory.path(), Phase::Prepared).record;
    verify_task_record_unchanged(&before, &before).unwrap();
    let mut changed = before.clone();
    changed.request.server_url = "http://127.0.0.1:2".into();
    assert_eq!(
        verify_task_record_unchanged(&before, &changed)
            .unwrap_err()
            .code,
        "upgrade_target_changed"
    );
    changed = before.clone();
    changed.runner_client_id = Some("different-runner".into());
    assert!(verify_task_record_unchanged(&before, &changed).is_err());
    changed = before.clone();
    changed.environment_id = "replacement-environment".into();
    assert!(verify_task_record_unchanged(&before, &changed).is_err());
    changed = before.clone();
    changed.request.account.identity = "replacement-owner".into();
    assert!(verify_task_record_unchanged(&before, &changed).is_err());
    changed = before.clone();
    changed.configured = false;
    assert!(verify_task_record_unchanged(&before, &changed).is_err());
}

#[test]
fn task_counts_preserve_create_join_and_viewer_semantics_and_the_u64_bound() {
    let directory = crate::test_tempdir().unwrap();
    let mut record = fixture(directory.path(), Phase::Prepared).record;
    record.request.mode = EnvironmentMode::Create {
        listen: "127.0.0.1:1".into(),
    };
    for count in [0, 1, u64::MAX] {
        assert_eq!(
            task_count_from_responses(&record, &json!({"jobs":{"active_count":count}}), None)
                .unwrap(),
            count
        );
        assert_eq!(
            task_count_from_responses(
                &record,
                &json!({"output":{"jobs":{"active_count":count}}}),
                None
            )
            .unwrap(),
            count
        );
    }
    for invalid in [
        json!(null),
        json!(-1),
        json!(1.5),
        json!("0"),
        json!("PRIVATE_RESPONSE_CANARY"),
    ] {
        let error =
            task_count_from_responses(&record, &json!({"jobs":{"active_count":invalid}}), None)
                .unwrap_err();
        assert_eq!(error.code, "upgrade_tasks_unknown");
        assert!(!error.to_string().contains("PRIVATE_RESPONSE_CANARY"));
    }
    let oversized: Value =
        serde_json::from_str("{\"jobs\":{\"active_count\":18446744073709551616}}").unwrap();
    assert!(task_count_from_responses(&record, &oversized, None).is_err());
    assert!(task_count_from_responses(&record, &json!({}), None).is_err());
    record.request.mode = EnvironmentMode::Join;
    record.runner_client_id = Some("saved-runner".into());
    assert_eq!(
        task_count_from_responses(
            &record,
            &json!({"jobs":{"active_count":999}}),
            Some(&json!({"active_jobs":3}))
        )
        .unwrap(),
        3
    );
    assert!(task_count_from_responses(&record, &json!({}), None).is_err());
    assert!(
        task_count_from_responses(&record, &json!({}), Some(&json!({"active_jobs":null}))).is_err()
    );
    record.runner_client_id = None;
    assert_eq!(
        task_count_from_responses(&record, &json!({}), None).unwrap(),
        0
    );
}
