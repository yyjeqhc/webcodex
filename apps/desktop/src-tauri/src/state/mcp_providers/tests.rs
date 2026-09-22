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
            expected_revision: None,
        })
        .await
        .unwrap();
    let connection = &state.connections.profiles[0].config;
    app.save_tunnel_profile(crate::tunnel_config::TunnelProfileRequest {
        id: Some(connection.id),
        name: "Work".into(),
        tunnel_id: connection.tunnel_id.clone().unwrap(),
        api_key: Some("replacement-tunnel-fixture".into()),
        autostart: true,
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
