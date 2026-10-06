use super::*;
use std::collections::BTreeMap;
use std::process::Command;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("webcodex-mcp-appstate-{}", uuid::Uuid::new_v4()));
        Self(root)
    }
    fn app(&self) -> AppState {
        AppState::new(self.0.clone(), self.0.join("resources")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn request(revision: u64) -> McpProviderRequest {
    McpProviderRequest {
        id: None,
        expected_revision: revision,
        name: "Local MCP".into(),
        command: std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        args: Vec::new(),
        cwd: None,
        enabled: true,
        env: BTreeMap::from([("PASSWORD".into(), Some("appstate-fixture-secret".into()))]),
    }
}
fn fixture_process() -> Command {
    #[cfg(unix)]
    {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "cat >/dev/null"]);
        command
    }
    #[cfg(windows)]
    {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::In.ReadToEnd() | Out-Null",
        ]);
        command
    }
}

#[tokio::test]
async fn missing_enabled_mcp_fails_before_setup_and_preserves_user_profile() {
    let fixture = Fixture::new();
    let app = fixture.app();
    let program = fixture.0.join("optional-provider.exe");
    std::fs::create_dir_all(&fixture.0).unwrap();
    std::fs::write(&program, b"fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut input = request(0);
    input.command = program.to_string_lossy().into_owned();
    let saved = app.save_mcp_provider(input).await.unwrap();
    let bytes = std::fs::read(fixture.0.join("mcp-providers.json")).unwrap();
    std::fs::remove_file(program).unwrap();
    let error = app.configure_local_setup(None).await.unwrap_err();
    assert_eq!(error.code, "mcp_provider_executable_unavailable");
    assert_eq!(
        error.details.as_ref().unwrap()["provider_id"],
        saved.mcp_providers.profiles[0].id
    );
    assert!(!error.message.contains("appstate-fixture-secret"));
    assert!(app.supervisor.lock().await.keys().is_empty());
    assert_ne!(app.get_state().readiness.server, ServerReadiness::Starting);
    assert_ne!(
        app.get_state().readiness.runner,
        RunnerReadiness::Connecting
    );
    assert_eq!(app.get_state().runtime_error.as_ref().unwrap(), &error);
    assert_eq!(
        std::fs::read(fixture.0.join("mcp-providers.json")).unwrap(),
        bytes
    );
    assert_eq!(
        fixture.app().get_state().mcp_providers.profiles,
        saved.mcp_providers.profiles
    );
}

#[tokio::test]
async fn explicit_disable_keeps_profile_and_removes_only_its_saved_runner_entry() {
    let fixture = Fixture::new();
    let app = fixture.app();
    let saved = app.save_mcp_provider(request(0)).await.unwrap();
    let id = saved.mcp_providers.profiles[0].id.clone();
    let mut disable = request(1);
    disable.id = Some(id.clone());
    disable.enabled = false;
    disable.command = fixture
        .0
        .join("deleted-provider.exe")
        .to_string_lossy()
        .into_owned();
    disable.env.insert("PASSWORD".into(), None);
    app.save_mcp_provider(disable).await.unwrap();
    let runner = fixture.0.join("runner.toml");
    std::fs::write(&runner,format!("client_id='fixture'\nserver_url='http://127.0.0.1:1'\noperator_setting='keep'\n[[mcp.providers]]\nid='{id}'\nname='Local MCP'\nexecutable='missing-provider-fixture'\n")).unwrap();
    {
        let mut slot = app.core.lock().await;
        let core = slot.as_mut().unwrap();
        core.config.runtime = Some(StoredRuntime {
            server_url: "http://127.0.0.1:1".into(),
            runner_config: Some(runner.clone()),
            runner_client_id: Some("fixture".into()),
            server_env_file: None,
            user_token_file: None,
            project_id: None,
            runtime_project_id: None,
        });
        core.preflight_providers().unwrap();
        core.reconcile_saved_providers().await.unwrap();
    }
    assert!(!std::fs::read_to_string(&runner)
        .unwrap()
        .contains("missing-provider-fixture"));
    assert!(std::fs::read_to_string(&runner)
        .unwrap()
        .contains("operator_setting='keep'"));
    let profile = fixture.app().get_state().mcp_providers.profiles.remove(0);
    assert_eq!(profile.id, id);
    assert!(!profile.enabled);
    assert_eq!(profile.env_keys, ["PASSWORD"]);
    assert!(app.supervisor.lock().await.keys().is_empty());
}

#[tokio::test]
async fn failed_resume_cannot_restore_a_stale_starting_baseline() {
    for kind in [
        DesktopOperationKind::RuntimeResume,
        DesktopOperationKind::RuntimeSwitch,
        DesktopOperationKind::EnvironmentMigration,
        DesktopOperationKind::LocalSetup,
    ] {
        let fixture = Fixture::new();
        let app = fixture.app();
        {
            let mut slot = app.core.lock().await;
            let core = slot.as_mut().unwrap();
            core.snapshot.readiness.server = ServerReadiness::Starting;
            core.snapshot.readiness.runner = RunnerReadiness::Connecting;
            core.snapshot.readiness.exposure = ExposureReadiness::Starting;
            core.publish_snapshot();
        }
        let (operation, cancellation, core, baseline) =
            app.begin_operation(kind, true).await.unwrap();
        let error = DesktopError::new(
            "fixture_start_failed",
            "Server could not start",
            "Retry after fixing the configuration",
        );
        app.finish_operation(operation, cancellation, core, baseline, Err(error.clone()))
            .await
            .unwrap_err();
        let state = app.get_state();
        assert_eq!(state.readiness.server, ServerReadiness::Error);
        assert_eq!(state.readiness.runner, RunnerReadiness::Error);
        assert_eq!(state.readiness.exposure, ExposureReadiness::Error);
        assert!(!state.readiness.runtime_ready);
        assert_eq!(state.runtime_error, Some(error));
        assert!(state.current_operation.is_none());
    }
}

#[tokio::test]
async fn saving_updating_and_removing_mcp_never_restart_any_runtime_or_connection_process() {
    let fixture = Fixture::new();
    let app = fixture.app();
    let keys = [
        ProcessKey::LocalServer,
        ProcessKey::LocalRunner,
        ProcessKey::RegularTunnel(crate::connection_id::TunnelProfileId::new()),
        ProcessKey::RegularTunnel(crate::connection_id::TunnelProfileId::new()),
    ];
    let mut original = Vec::new();
    for key in keys {
        app.supervisor
            .lock()
            .await
            .spawn_owned(key, fixture_process(), false)
            .await
            .unwrap();
        original.push(app.supervisor.lock().await.snapshot(key).unwrap());
    }
    let saved = app.save_mcp_provider(request(0)).await.unwrap();
    assert!(saved.mcp_providers.restart_required);
    let id = saved.mcp_providers.profiles[0].id.clone();
    let mut edit = request(1);
    edit.id = Some(id.clone());
    edit.name = "Renamed MCP".into();
    edit.env.insert("PASSWORD".into(), None);
    app.save_mcp_provider(edit).await.unwrap();
    for process in &original {
        let now = app.supervisor.lock().await.snapshot(process.kind).unwrap();
        assert_eq!(now.generation, process.generation);
        assert_eq!(now.pid, process.pid);
        assert_eq!(now.phase, ProcessPhase::Running);
    }
    assert!(!serde_json::to_string(&app.get_state())
        .unwrap()
        .contains("appstate-fixture-secret"));
    assert!(!serde_json::to_string(&app.activity())
        .unwrap()
        .contains("appstate-fixture-secret"));
    let restarted = fixture.app();
    assert_eq!(
        restarted.get_state().mcp_providers.profiles[0].name,
        "Renamed MCP"
    );
    app.remove_mcp_provider(id, 2).await.unwrap();
    assert!(fixture.app().get_state().mcp_providers.profiles.is_empty());
    for process in &original {
        assert_eq!(
            app.supervisor
                .lock()
                .await
                .snapshot(process.kind)
                .unwrap()
                .pid,
            process.pid
        );
    }
    app.supervisor.lock().await.stop_all().await;
}

#[tokio::test]
async fn coding_agent_appstate_save_keeps_service_generations_and_other_capabilities_unchanged() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(&fixture.0).unwrap();
    let app = fixture.app();
    let path = fixture.0.join("runner.toml");
    let config = "# identity comment\nserver_url = \"http://127.0.0.1:1\"\nclient_id = \"mini\"\ntoken = \"fixture-token\"\n[ssh_resources.static]\ntarget = \"operator-alias\"\n";
    std::fs::write(&path, config).unwrap();
    let registry_sentinel = fixture.0.join("managed-ssh-registry-sentinel");
    std::fs::write(&registry_sentinel, "fixture-registry-unchanged").unwrap();
    let runtime = crate::models::StoredRuntime {
        server_url: "http://127.0.0.1:1".into(),
        server_env_file: None,
        runner_config: Some(path.clone()),
        user_token_file: None,
        runner_client_id: Some("mini".into()),
        project_id: None,
        runtime_project_id: None,
    };
    app.core.lock().await.as_mut().unwrap().config.runtime = Some(runtime.clone());
    let target = crate::webcodex::settings::SettingsTarget {
        config_path: path.clone(),
        client_id: "mini".into(),
        server_url: runtime.server_url.clone(),
    };
    let mut processes = Vec::new();
    for key in [
        ProcessKey::LocalServer,
        ProcessKey::LocalRunner,
        ProcessKey::RegularTunnel(crate::connection_id::TunnelProfileId::new()),
    ] {
        app.supervisor
            .lock()
            .await
            .spawn_owned(key, fixture_process(), false)
            .await
            .unwrap();
        processes.push(app.supervisor.lock().await.snapshot(key).unwrap());
    }
    let mcp = app
        .save_mcp_provider(request(0))
        .await
        .unwrap()
        .mcp_providers;
    let mut add = crate::coding_agents::tests::request("pi", 0);
    add.target = target.clone();
    let saved = app.save_coding_agent(add).await.unwrap();
    assert!(saved.coding_agents.restart_required);
    assert_eq!(saved.mcp_providers.revision, mcp.revision);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        config,
        "Save must not apply active config"
    );
    let mut edit = crate::coding_agents::tests::request("pi", 1);
    edit.target = target.clone();
    edit.previous_id = Some("pi".into());
    edit.profile.name = "Pi Local".into();
    app.save_coding_agent(edit).await.unwrap();
    {
        let slot = app.core.lock().await;
        let core = slot.as_ref().unwrap();
        crate::webcodex::settings::reconcile_acp(&runtime, &core.coding_agents, true).unwrap();
        crate::webcodex::settings::reconcile_mcp(&runtime, &core.mcp_providers).unwrap();
        crate::webcodex::settings::reconcile_acp(&runtime, &core.coding_agents, false).unwrap();
    }
    let applied = std::fs::read_to_string(&path).unwrap();
    for expected in [
        "# identity comment",
        "fixture-token",
        "operator-alias",
        "Pi Local",
        "Local MCP",
    ] {
        assert!(applied.contains(expected), "missing {expected}");
    }
    app.remove_coding_agent(crate::coding_agents::CodingAgentRemove {
        target,
        expected_revision: 2,
        provider_id: "pi".into(),
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        applied,
        "Removal must also await restart"
    );
    assert_eq!(
        std::fs::read_to_string(registry_sentinel).unwrap(),
        "fixture-registry-unchanged"
    );
    assert_eq!(app.get_state().mcp_providers.revision, mcp.revision);
    for process in processes {
        let observed = app.supervisor.lock().await.snapshot(process.kind).unwrap();
        assert_eq!(observed.pid, process.pid);
        assert_eq!(observed.generation, process.generation);
        assert_eq!(observed.phase, ProcessPhase::Running);
    }
    app.supervisor.lock().await.stop_all().await;
}

#[tokio::test]
async fn tunnel_configuration_edit_does_not_change_runner_generation_or_mcp_desired_revision() {
    let fixture = Fixture::new();
    let app = fixture.app();
    app.save_mcp_provider(request(0)).await.unwrap();
    app.supervisor
        .lock()
        .await
        .spawn_owned(ProcessKey::LocalRunner, fixture_process(), false)
        .await
        .unwrap();
    let runner = app
        .supervisor
        .lock()
        .await
        .snapshot(ProcessKey::LocalRunner)
        .unwrap();
    let state = app
        .save_tunnel_profile(crate::tunnel_config::TunnelProfileRequest {
            id: None,
            name: "Personal".into(),
            tunnel_id: format!("tunnel_{}", uuid::Uuid::new_v4().simple()),
            api_key: Some("independent-tunnel-fixture".into()),
            autostart: true,
            host_mode: webcodex_environment::TunnelHostMode::Standalone,
            expected_revision: None,
        })
        .await
        .unwrap();
    let connection = &state.connections.profiles[0].config;
    app.save_tunnel_profile(crate::tunnel_config::TunnelProfileRequest {
        id: Some(connection.id.clone()),
        name: "Work".into(),
        tunnel_id: connection.tunnel_id.clone().unwrap(),
        api_key: Some("replacement-tunnel-fixture".into()),
        autostart: true,
        host_mode: webcodex_environment::TunnelHostMode::Standalone,
        expected_revision: Some(connection.revision),
    })
    .await
    .unwrap();
    assert_eq!(
        app.supervisor
            .lock()
            .await
            .snapshot(ProcessKey::LocalRunner)
            .unwrap()
            .generation,
        runner.generation
    );
    assert_eq!(app.get_state().mcp_providers.revision, 1);
    assert!(!serde_json::to_string(&app.get_state())
        .unwrap()
        .contains("replacement-tunnel-fixture"));
    app.supervisor.lock().await.stop_all().await;
}
