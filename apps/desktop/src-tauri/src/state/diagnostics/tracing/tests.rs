use super::*;
use webcodex_environment::{
    service::ServiceScope, EnvironmentMode, EnvironmentRecord, RuntimeBinaries, SetupRequest,
};

struct Fixture {
    _temp: tempfile::TempDir,
    store: EnvironmentStore,
    runtime: StoredRuntime,
    record: EnvironmentRecord,
}

impl Fixture {
    fn new() -> Self {
        #[cfg(target_os = "macos")]
        let temp = tempfile::Builder::new()
            .tempdir_in(std::env::temp_dir().canonicalize().unwrap())
            .unwrap();
        #[cfg(not(target_os = "macos"))]
        let temp = tempfile::tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let record = EnvironmentRecord {
            schema_version: 1,
            environment_id: "fixture-environment".into(),
            request: SetupRequest {
                runner_display_name: None,
                service_scope: ServiceScope::User,
                mode: EnvironmentMode::Create {
                    listen: "127.0.0.1:1".into(),
                },
                server_url: "http://127.0.0.1:1".into(),
                project: None,
                runner: Some(false),
                account: current_account().unwrap(),
                binaries: RuntimeBinaries {
                    cli: temp.path().join("cli"),
                    server: temp.path().join("server"),
                    runner: temp.path().join("runner"),
                },
            },
            username: Some("fixture".into()),
            runner_client_id: None,
            projects: vec![],
            configured: true,
        };
        store.save_environment(&record).unwrap();
        // Use the native private writer to create the fixture ACL on Windows,
        // then replace its contents without altering that ACL. No real profile.
        let server = EnvironmentStore::open(store.root().join("server")).unwrap();
        server.save_environment(&record).unwrap();
        let path = server.root().join("webcodex.env");
        std::fs::write(
            server.root().join("environment.json"),
            "WEBCODEX_TOKEN=fixture-private-canary\nCUSTOM_SETTING=keep\n",
        )
        .unwrap();
        std::fs::rename(server.root().join("environment.json"), &path).unwrap();
        let runtime = StoredRuntime {
            server_url: record.request.server_url.clone(),
            server_env_file: Some(path),
            runner_config: None,
            user_token_file: None,
            runner_client_id: None,
            project_id: None,
            runtime_project_id: None,
        };
        Self {
            _temp: temp,
            store,
            runtime,
            record,
        }
    }
    fn target(&self) -> DesktopResult<ManagedTraceTarget> {
        persistent_trace_target(
            self.store.clone(),
            &self.record.environment_id,
            &self.runtime,
            |spec| {
                assert_eq!(spec.component, Component::Server);
                assert_eq!(spec.scope, ServiceScope::User);
                Ok(true)
            },
        )
    }
    fn contents(&self) -> String {
        std::fs::read_to_string(self.runtime.server_env_file.as_ref().unwrap()).unwrap()
    }
}

#[test]
fn current_mode_uses_the_overview_effective_config_and_never_guesses_off() {
    assert_eq!(
        effective_trace_mode(
            &serde_json::json!({"effective_config":{"tool_request_trace_mode":"metadata"}})
        ),
        Some(TraceMode::Metadata)
    );
    assert_eq!(
        effective_trace_mode(
            &serde_json::json!({"effective_config":{"tool_request_trace_mode":"full"}})
        ),
        Some(TraceMode::Full)
    );
    for missing in [
        serde_json::json!({}),
        serde_json::json!({"tool_request_trace_mode":"off"}),
        serde_json::json!({"effective_config":{"tool_request_trace_mode":"invalid"}}),
    ] {
        assert_eq!(effective_trace_mode(&missing), None);
    }
}

#[test]
fn owned_user_service_tracing_is_editable_and_keeps_credentials_and_setup_fences() {
    let f = Fixture::new();
    let target = f.target().unwrap();
    let initial = target.inspect().unwrap();
    assert!(initial.can_edit && initial.can_restart);
    assert!(
        f.store.lock().is_err(),
        "setup/installer may not retarget an admitted edit"
    );
    let original = f.contents();
    assert_eq!(
        target
            .update(TraceMode::Full, &initial.revision, false)
            .unwrap_err()
            .code,
        "full_trace_confirmation_required"
    );
    assert_eq!(f.contents(), original);
    let next = target
        .update(TraceMode::Metadata, &initial.revision, false)
        .unwrap();
    assert!(webcodex_environment::read_secret(&target.path)
        .unwrap()
        .expose()
        .contains("fixture-private-canary"));
    assert_eq!(next.configured_mode, Some(TraceMode::Metadata));
    assert!(next.restart_required);
    assert!(
        next.effective_mode.is_none(),
        "a file save is not running-process evidence"
    );
    assert!(f.contents().contains(&original));
    assert!(f
        .contents()
        .contains("WEBCODEX_TOOL_REQUEST_TRACE=metadata"));
    assert_eq!(
        target
            .update(TraceMode::Off, &initial.revision, false)
            .unwrap_err()
            .code,
        "server_environment_changed"
    );
    assert!(!serde_json::to_string(&next)
        .unwrap()
        .contains("fixture-private-canary"));
    drop(target);
    assert!(f.store.lock().is_ok());
}

#[test]
fn unowned_remote_system_or_changed_targets_cannot_edit_the_server_environment() {
    let f = Fixture::new();
    let original = f.contents();
    assert_eq!(
        persistent_trace_target(
            f.store.clone(),
            "other-environment",
            &f.runtime,
            |_| panic!("stale identity")
        )
        .err()
        .unwrap()
        .code,
        "diagnostic_identity_changed"
    );
    assert_eq!(
        persistent_trace_target(
            f.store.clone(),
            &f.record.environment_id,
            &f.runtime,
            |_| Ok(false)
        )
        .err()
        .unwrap()
        .code,
        "server_not_owned"
    );
    let mut runtime = f.runtime.clone();
    runtime.server_env_file = Some(f.store.root().join("foreign.env"));
    assert_eq!(
        persistent_trace_target(
            f.store.clone(),
            &f.record.environment_id,
            &runtime,
            |_| panic!("foreign path")
        )
        .err()
        .unwrap()
        .code,
        "server_environment_not_managed"
    );
    let mut record = f.record.clone();
    record.request.account.identity = "other-account".into();
    f.store.save_environment(&record).unwrap();
    assert_eq!(
        f.target().err().unwrap().code,
        "diagnostic_identity_changed"
    );
    record = f.record.clone();
    record.request.mode = EnvironmentMode::Join;
    f.store.save_environment(&record).unwrap();
    assert_eq!(f.target().err().unwrap().code, "server_not_owned");
    record = f.record.clone();
    record.request.service_scope = ServiceScope::System;
    f.store.save_environment(&record).unwrap();
    assert_eq!(
        f.target().err().unwrap().code,
        "trace_system_service_read_only"
    );
    assert_eq!(f.contents(), original);
}

#[test]
fn identical_file_bytes_do_not_allow_an_old_draft_to_retarget_another_environment() {
    let mut f = Fixture::new();
    let old = f.target().unwrap().inspect().unwrap().revision;
    f.record.environment_id = "replacement-environment".into();
    f.store.save_environment(&f.record).unwrap();
    let target = f.target().unwrap();
    assert_ne!(target.inspect().unwrap().revision, old);
    let original = f.contents();
    assert_eq!(
        target
            .update(TraceMode::Metadata, &old, false)
            .unwrap_err()
            .code,
        "server_environment_changed"
    );
    assert_eq!(f.contents(), original);
}

#[cfg(unix)]
#[test]
fn linked_server_directory_is_rejected_before_a_configuration_write() {
    let f = Fixture::new();
    let server = f.store.root().join("server");
    let other = f._temp.path().join("foreign-server");
    std::fs::rename(&server, &other).unwrap();
    std::os::unix::fs::symlink(&other, &server).unwrap();
    assert_eq!(f.target().err().unwrap().code, "unsafe_path");
    assert!(!std::fs::read_to_string(other.join("webcodex.env"))
        .unwrap()
        .contains("WEBCODEX_TOOL_REQUEST_TRACE"));
}

#[test]
fn configuration_navigation_is_independent_of_edit_authority() {
    let f = Fixture::new();
    let expected = f.store.root().join("server/webcodex.env");
    let original = f.contents();
    let mut record = f.record.clone();
    record.request.service_scope = ServiceScope::System;
    f.store.save_environment(&record).unwrap();
    assert_eq!(
        f.target().err().unwrap().code,
        "trace_system_service_read_only"
    );
    assert_eq!(
        configuration_location(true, Some(&f.runtime), &expected).unwrap(),
        expected.parent().unwrap()
    );
    record.request.service_scope = ServiceScope::User;
    f.store.save_environment(&record).unwrap();
    assert!(
        persistent_trace_target(f.store.clone(), &record.environment_id, &f.runtime, |_| Ok(
            false
        ))
        .is_err()
    );
    assert!(configuration_location(true, Some(&f.runtime), &expected).is_ok());
    assert!(configuration_location(false, Some(&f.runtime), &expected).is_err());
    let mut foreign = f.runtime.clone();
    foreign.server_env_file = Some(f.store.root().join("foreign.env"));
    assert!(configuration_location(true, Some(&foreign), &expected).is_err());
    assert_eq!(f.contents(), original);
}

// Runs on Windows too, using the native private-file writer/ACL fixture.
fn local_core(f: &Fixture) -> DesktopCore {
    let data = f._temp.path().join("desktop");
    let mut core = DesktopCore::new(data.clone(), data.join("resources")).unwrap();
    let local = EnvironmentStore::open(data.join("runtime/local")).unwrap();
    local.save_environment(&f.record).unwrap();
    let path = local.root().join("webcodex.env");
    std::fs::rename(local.root().join("environment.json"), &path).unwrap();
    std::fs::write(&path, "WEBCODEX_TOOL_REQUEST_TRACE=off\n").unwrap();
    core.config.persistent_environment = None;
    core.config.runtime = Some(StoredRuntime {
        server_env_file: Some(path),
        ..f.runtime.clone()
    });
    core.config.topology = Some(RuntimeTopology {
        experience: Experience::Full,
        server: ServerTopology::Local,
        runner: RunnerTopology::Local,
        exposure: Exposure::None,
        enrollment: Enrollment::ManagedPairing,
    });
    core
}

#[test]
fn desktop_child_server_tracing_is_editable_without_a_persistent_environment() {
    let f = Fixture::new();
    let core = local_core(&f);
    let mut server = crate::process::ProcessSnapshot {
        kind: ProcessKey::LocalServer,
        generation: 1,
        phase: crate::process::ProcessPhase::Running,
        pid: Some(123),
        exit_code: None,
        owned_by_desktop: true,
    };
    let target = core.local_trace_target(Some(server.clone())).unwrap();
    let trace = target.inspect().unwrap();
    assert!(trace.can_edit);
    assert!(trace.can_restart);
    assert_eq!(trace.error_code, None);
    let saved = target
        .update(TraceMode::Metadata, &trace.revision, false)
        .unwrap();
    assert_eq!(saved.configured_mode, Some(TraceMode::Metadata));
    server.owned_by_desktop = false;
    assert_eq!(
        core.local_trace_target(Some(server)).err().unwrap().code,
        "server_not_owned"
    );
}

#[tokio::test]
async fn stopped_server_config_edit_does_not_require_runtime_switch_authority() {
    let f = Fixture::new();
    let mut core = local_core(&f);
    // Port availability governs starting/replacing a Server, not saving its file.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    core.config.runtime.as_mut().unwrap().server_url =
        format!("http://{}", listener.local_addr().unwrap());
    assert!(core.runtime_switch_authority().await.is_err());
    let target = core.managed_trace_target().await.unwrap();
    let trace = target.inspect().unwrap();
    assert!(trace.can_edit);
    assert!(!trace.can_restart);
    assert_eq!(trace.error_code, None);
    target
        .update(TraceMode::Metadata, &trace.revision, false)
        .unwrap();
    core.config.runtime.as_mut().unwrap().server_env_file = f.runtime.server_env_file.clone();
    assert_eq!(
        core.managed_trace_target().await.err().unwrap().code,
        "server_environment_not_managed"
    );
    core.config.topology.as_mut().unwrap().server = ServerTopology::Remote {
        url: "https://example.invalid".into(),
    };
    assert_eq!(
        core.managed_trace_target().await.err().unwrap().code,
        "server_not_owned"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn local_trace_rejects_non_private_files_and_linked_ancestors() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let core = local_core(&f);
    let path = core
        .config
        .runtime
        .as_ref()
        .unwrap()
        .server_env_file
        .as_ref()
        .unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(core.managed_trace_target().await.is_err());
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let parent = path.parent().unwrap();
    let other = core.data_dir.join("other");
    std::fs::rename(parent, &other).unwrap();
    std::os::unix::fs::symlink(other, parent).unwrap();
    assert!(core.managed_trace_target().await.is_err());
}

#[cfg(windows)]
#[tokio::test]
#[ignore = "Desktop Windows real-process lane: supervised PowerShell children"]
async fn desktop_real_process_windows_owned_children_allow_local_tracing() {
    let f = Fixture::new();
    let core = local_core(&f);
    let mut supervisor = core.supervisor.lock().await;
    for key in [ProcessKey::LocalServer, ProcessKey::LocalRunner] {
        let mut command = std::process::Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$null = [Console]::In.ReadToEnd()",
        ]);
        // Mirror the production stdin liveness lease; the supervisor drains
        // both output pipes and closes stdin when stopping each child.
        if let Err(error) = supervisor.spawn_owned(key, command, false).await {
            supervisor.stop(ProcessKey::LocalServer).await;
            panic!("fixture start failed: {}", error.code);
        }
    }
    drop(supervisor);
    let result = core
        .managed_trace_target()
        .await
        .and_then(|target| target.inspect());
    core.stop_process(ProcessKey::LocalRunner).await;
    core.stop_process(ProcessKey::LocalServer).await;
    let trace = result.unwrap();
    assert!(trace.can_edit && trace.can_restart);
    assert_eq!(trace.error_code, None);
}
