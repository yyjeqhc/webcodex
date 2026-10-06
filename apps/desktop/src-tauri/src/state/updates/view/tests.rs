use super::*;
#[test]
fn observations_are_bounded_and_do_not_fill_missing_identity() {
    let v = observed(
        "webcodex-runner",
        &serde_json::json!({"version":"1.2.3", "build_git_commit":"a".repeat(81), "build_git_dirty":null}),
    );
    assert_eq!(v.version.as_deref(), Some("1.2.3"));
    assert!(v.git_commit.is_none());
    assert!(v.git_dirty.is_none());
    let missing = observed(
        "webcodex-server",
        &serde_json::json!({"version":"bad\nbody"}),
    );
    assert!(missing.version.is_none());
}
#[test]
fn remote_hosts_cannot_be_observed_as_local_server() {
    assert!(loopback("http://127.0.0.1:8080"));
    assert!(!loopback("https://server.example"));
}

#[test]
fn update_service_inventory_matches_core_tunnel_ownership() {
    let profile = |host_mode, installed| webcodex_environment::TunnelRecord {
        profile_id: "profile".into(),
        name: "Profile".into(),
        host_mode,
        autostart: true,
        revision: 1,
        runtime_revision: 1,
        installed,
        started: installed,
    };
    assert!(standalone_tunnel_service(&profile(
        webcodex_environment::TunnelHostMode::Standalone,
        true,
    )));
    assert!(!standalone_tunnel_service(&profile(
        webcodex_environment::TunnelHostMode::Standalone,
        false,
    )));
    // Embedded Tunnel lifecycle belongs to the Server, which is already in the
    // upgrade service inventory. It must not require a nonexistent Tunnel service.
    assert!(!standalone_tunnel_service(&profile(
        webcodex_environment::TunnelHostMode::Embedded,
        false,
    )));
}
fn fence() -> UpdateConfirmation {
    serde_json::from_value(serde_json::json!({"candidate":{"version":"0.5.0","target":{"platform":"linux-x64","format":"deb"},"source_sha":"a".repeat(40),"manifest_sha256":"b".repeat(64),"installer_sha256":"c".repeat(64)},"target":{"environment_id":"local-env","manifest_sha256":"b".repeat(64),"operation_id":null},"selection_revision":7,"services":[],"service_inventory_complete":true})).unwrap()
}
#[test]
fn confirmation_cannot_retarget_environment_revision_or_candidate() {
    let mut selected = fence();
    assert!(confirmation_matches(
        &selected,
        Some("local-env"),
        7,
        "0.5.0"
    ));
    assert!(!confirmation_matches(&selected, None, 7, "0.5.0"));
    assert!(!confirmation_matches(
        &selected,
        Some("changed"),
        7,
        "0.5.0"
    ));
    assert!(!confirmation_matches(
        &selected,
        Some("local-env"),
        8,
        "0.5.0"
    ));
    assert!(!confirmation_matches(
        &selected,
        Some("local-env"),
        7,
        "0.6.0"
    ));
    selected.target.manifest_sha256 = "d".repeat(64);
    assert!(!confirmation_matches(
        &selected,
        Some("local-env"),
        7,
        "0.5.0"
    ));
}
#[test]
fn arbitrary_server_strings_are_never_presented_as_build_identity() {
    for canary in [
        "private-error-body",
        "x".repeat(40).as_str(),
        "1.2.3 secret",
        "1.2.3+private-canary",
        "1.2.3-private-canary",
    ] {
        let projected = observed(
            "webcodex-server",
            &serde_json::json!({"version":canary,"build_git_commit":canary}),
        );
        assert!(projected.version.is_none());
        assert!(projected.git_commit.is_none());
    }
    let projected = observed(
        "webcodex-runner",
        &serde_json::json!({"version":"1.2.3","build_git_commit":"a".repeat(40)}),
    );
    assert_eq!(projected.version.as_deref(), Some("1.2.3"));
    assert_eq!(
        projected.git_commit.as_deref(),
        Some("a".repeat(40).as_str())
    );
}
#[test]
fn native_service_observations_do_not_adopt_foreign_or_unknown_processes() {
    use webcodex_environment::service::{Ownership, ServiceStatus};
    let mut status = ServiceStatus {
        id: "hidden-service-path".into(),
        ownership: Ownership::Owned,
        installed: true,
        enabled: None,
        running: Some(true),
        detail: Some("private-detail".into()),
    };
    assert_eq!(service_state(&status), "running");
    status.running = Some(false);
    assert_eq!(service_state(&status), "stopped");
    status.ownership = Ownership::Foreign;
    assert_eq!(service_state(&status), "unknown");
    status.ownership = Ownership::Unknown;
    assert_eq!(service_state(&status), "unknown");
    status.ownership = Ownership::Absent;
    status.installed = false;
    assert_eq!(service_state(&status), "absent");
}
#[test]
fn terminal_operation_never_becomes_the_identity_of_fresh_prepare() {
    let mut upgrade: webcodex_environment::UpgradeStatus = serde_json::from_value(serde_json::json!({"schema_version":1,"environment_id":"local-env","operation_id":"old-op","version":"0.5.0","source_sha":"a".repeat(40),"manifest_sha256":"b".repeat(64),"phase":"committed","files":["desktop"],"services":[],"service_inventory_complete":true})).unwrap();
    assert!(confirmation_operation(Some(&upgrade)).is_none());
    upgrade.phase = webcodex_environment::UpgradePhase::RolledBack;
    assert!(confirmation_operation(Some(&upgrade)).is_none());
    upgrade.phase = webcodex_environment::UpgradePhase::Restoring;
    assert_eq!(
        confirmation_operation(Some(&upgrade)).as_deref(),
        Some("old-op")
    );
}
#[test]
fn committed_journal_with_unknown_or_mismatched_files_does_not_claim_restart() {
    let upgrade: webcodex_environment::UpgradeStatus = serde_json::from_value(serde_json::json!({"schema_version":1,"environment_id":"local-env","operation_id":"old-op","version":"0.5.0","source_sha":"a".repeat(40),"manifest_sha256":"b".repeat(64),"phase":"committed","files":["desktop"],"services":[],"service_inventory_complete":true})).unwrap();
    let mut desktop = unknown("webcodex-desktop", "unknown");
    assert!(!restart_required(
        Some(&upgrade),
        Some("local-env"),
        &[],
        &desktop
    ));
    let mut installed = crate::commands::get_desktop_build_info();
    installed.version = "0.5.0".into();
    installed.git_commit = Some("a".repeat(40));
    installed.git_dirty = Some(false);
    assert!(!restart_required(
        Some(&upgrade),
        Some("local-env"),
        &[installed.clone()],
        &desktop
    ));
    desktop.state = "observed";
    desktop.version = Some("0.4.0".into());
    desktop.git_commit = Some("d".repeat(40));
    assert!(restart_required(
        Some(&upgrade),
        Some("local-env"),
        &[installed.clone()],
        &desktop
    ));
    assert!(!restart_required(
        Some(&upgrade),
        Some("other-env"),
        &[installed.clone()],
        &desktop
    ));
    installed.git_dirty = Some(true);
    assert!(!restart_required(
        Some(&upgrade),
        Some("local-env"),
        &[installed],
        &desktop
    ));
}

#[test]
fn aggregate_local_projection_enforces_a_cap_including_adapter_fields() {
    let mut status = LocalUpdateStatus {
        view: UpdateView {
            schema_version: 1,
            download: Default::default(),
            installed: vec![],
            candidate: None,
            candidate_components: vec![],
            upgrade: None,
            blockers: vec![],
            restart_required: false,
        },
        environment_id: None,
        selection_revision: 0,
        installed_observed: false,
        installed_checked_at_ms: None,
        running: vec![unknown("webcodex-desktop", "unknown")],
        confirmation: None,
        observation_error: false,
    };
    assert!(bounded_local_status(status.clone()).is_ok());
    status.running[0].version =
        Some("x".repeat(webcodex_environment::unified_update::MAX_UPDATE_VIEW_BYTES));
    assert_eq!(
        bounded_local_status(status).unwrap_err().code,
        "update_action_unavailable"
    );
}

#[test]
fn file_identity_survives_protocol_difference_but_never_byte_change() {
    let probe = |code: &str| runtime_selection::BinaryProbe {
        name: "webcodex-server".into(),
        present: Some(true),
        startup_check: runtime_selection::BinaryStartupCheck::Passed,
        metadata: Some(crate::commands::get_desktop_build_info()),
        sha256: Some("a".repeat(64)),
        error_code: Some(code.into()),
        diagnostics: None,
    };
    assert!(verified_file_build(probe("runtime_contract_incompatible")).is_some());
    assert!(verified_file_build(probe("binary_architecture_mismatch")).is_some());
    assert!(verified_file_build(probe("runtime_candidate_changed")).is_none());
    assert!(verified_file_build(probe("private-diagnostic-canary")).is_none());
}

#[test]
fn service_preview_is_allowlisted_owned_and_has_no_definition_fields() {
    use webcodex_environment::{
        service::{Ownership, ServiceScope, ServiceStatus},
        UpgradeServiceKind,
    };
    let mut status = ServiceStatus {
        id: "private-id".into(),
        ownership: Ownership::Owned,
        installed: true,
        enabled: None,
        running: Some(false),
        detail: Some("private-definition-canary".into()),
    };
    let mut projected = Vec::new();
    assert!(include_service(
        &mut projected,
        UpgradeServiceKind::Runner,
        ServiceScope::User,
        &status
    ));
    assert!(include_service(
        &mut projected,
        UpgradeServiceKind::Runner,
        ServiceScope::User,
        &status
    ));
    assert_eq!(projected.len(), 1);
    assert_eq!(
        serde_json::to_value(&projected).unwrap(),
        serde_json::json!([{"component":"runner","scope":"user"}])
    );
    status.ownership = Ownership::Foreign;
    assert!(!include_service(
        &mut projected,
        UpgradeServiceKind::Server,
        ServiceScope::System,
        &status
    ));
    status.ownership = Ownership::Owned;
    status.running = None;
    assert!(!include_service(
        &mut projected,
        UpgradeServiceKind::Tunnel,
        ServiceScope::System,
        &status
    ));
}

#[test]
fn source_and_custom_installations_cannot_gain_eligibility_from_read_view() {
    let mut download = DownloadStatus {
        phase: DownloadPhase::ReadyToInstall,
        can_install: true,
        installation: InstallationKind::Managed,
        ..Default::default()
    };
    assert!(install_eligible(&download, true, true, &[]));
    for manual in [
        InstallationKind::SourceBuild,
        InstallationKind::UnmanagedInstallation,
    ] {
        download.installation = manual;
        assert!(!install_eligible(&download, true, true, &[]));
    }
    download.installation = InstallationKind::Managed;
    assert!(!install_eligible(&download, false, true, &[]));
    assert!(!install_eligible(&download, true, false, &[]));
    assert!(!install_eligible(
        &download,
        true,
        true,
        &[UpdateBlocker::ActiveTasks]
    ));
    download.can_install = false;
    assert!(!install_eligible(&download, true, true, &[]));
}

#[test]
fn external_environment_changes_invalidate_the_whole_local_observation() {
    use webcodex_environment::{
        service::ServiceScope, EnvironmentMode, EnvironmentRecord, LocalAccount, ProjectRecord,
        RuntimeBinaries, SetupRequest,
    };
    let before = EnvironmentRecord {
        schema_version: 1,
        environment_id: "saved-env".into(),
        request: SetupRequest {
            service_scope: ServiceScope::System,
            mode: EnvironmentMode::Create {
                listen: "127.0.0.1:8080".into(),
            },
            server_url: "http://127.0.0.1:8080".into(),
            project: None,
            runner: Some(true),
            runner_display_name: None,
            account: LocalAccount {
                name: "fixture-owner".into(),
                identity: "fixture-owner-id".into(),
                home: "/fixture/home".into(),
            },
            binaries: RuntimeBinaries {
                cli: "/fixture/bin/webcodex".into(),
                server: "/fixture/bin/webcodex-server".into(),
                runner: "/fixture/bin/webcodex-runner".into(),
            },
        },
        username: Some("saved-user".into()),
        runner_client_id: Some("saved-runner".into()),
        projects: vec![],
        configured: true,
    };
    assert!(saved_environment_unchanged(
        Some(&before),
        Some(&before.clone())
    ));
    assert!(saved_environment_unchanged(None, None));
    assert!(!saved_environment_unchanged(Some(&before), None));
    assert!(!saved_environment_unchanged(None, Some(&before)));
    let changes: [fn(&mut EnvironmentRecord); 12] = [
        |record| record.environment_id = "replacement-env".into(),
        |record| record.request.mode = EnvironmentMode::Join,
        |record| record.request.runner = Some(false),
        |record| record.request.service_scope = ServiceScope::User,
        |record| record.request.server_url = "https://changed.example".into(),
        |record| record.request.binaries.cli = "/other/webcodex".into(),
        |record| record.request.account.identity = "other-owner-id".into(),
        |record| record.runner_client_id = Some("replacement-runner".into()),
        |record| record.configured = false,
        |record| record.username = Some("different-user".into()),
        |record| {
            record.projects.push(ProjectRecord {
                id: "new-project".into(),
                path: "/fixture/project".into(),
            })
        },
        |record| record.schema_version = 2,
    ];
    for change in changes {
        let mut after = before.clone();
        change(&mut after);
        assert!(!saved_environment_unchanged(Some(&before), Some(&after)));
    }
}
