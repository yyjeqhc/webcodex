use super::*;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex, OnceLock, Weak};
use tempfile::TempDir;
use webcodex_core::plugin::{
    PluginContent, PluginGatewayResponsePayload, PLUGIN_MAX_ARGUMENT_BYTES,
};

static FAKE_PLUGIN: OnceLock<Mutex<Weak<FakeBinary>>> = OnceLock::new();

struct FakeBinary {
    _temp: TempDir,
    path: PathBuf,
}

fn fake_binary() -> Arc<FakeBinary> {
    let cache = FAKE_PLUGIN.get_or_init(|| Mutex::new(Weak::new()));
    let mut cached = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(binary) = cached.upgrade() {
        return binary;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = temp
        .path()
        .join(format!("webcodex-plugin-fake{}", env::consts::EXE_SUFFIX));
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/webcodex_runner/fake_plugin.rs");
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let result = Command::new(rustc)
        .arg("--edition=2021")
        .arg("--crate-name=webcodex_plugin_fake")
        .arg(source)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let binary = Arc::new(FakeBinary {
        _temp: temp,
        path: output,
    });
    *cached = Arc::downgrade(&binary);
    binary
}

struct Fixture {
    manager: Arc<PluginManager>,
    marker: PathBuf,
    provider: StartupPluginProvider,
    _fake: Arc<FakeBinary>,
    _temp: TempDir,
}

impl Fixture {
    fn new(scenario: &str, timeout_secs: u64) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("marker.log");
        let fake = fake_binary();
        let provider = PluginProviderConfig {
            id: "fake".to_string(),
            name: "Fake Plugin".to_string(),
            command: fake.path.to_string_lossy().into_owned(),
            args: vec![scenario.to_string(), marker.to_string_lossy().into_owned()],
            cwd: Some(temp.path().to_string_lossy().into_owned()),
            profile: None,
            timeout_secs: Some(timeout_secs),
        };
        let config = runner_config(
            PluginConfig {
                request_timeout_secs: timeout_secs,
                providers: vec![provider],
            },
            ShellConfig::default(),
            temp.path(),
        );
        let manager = Arc::new(PluginManager::new(&config, temp.path().join("runner.toml")));
        let provider = manager.startup_catalog().into_iter().next().unwrap();
        Self {
            manager,
            marker,
            provider,
            _fake: fake,
            _temp: temp,
        }
    }

    fn list(&self) -> PluginGatewayResponse {
        self.manager.handle(PluginGatewayRequest::ToolsList {
            plane: PluginPlane::Startup,
            provider_id: self.provider.provider_id.clone(),
            provider_instance_id: self.provider.provider_instance_id.clone(),
        })
    }

    fn call(&self) -> PluginGatewayResponse {
        self.call_with_arguments(json!({"value":"hello"}))
    }

    fn call_with_arguments(&self, arguments: Value) -> PluginGatewayResponse {
        let schema = self.provider.tools[0].schema_observation();
        self.manager.handle(PluginGatewayRequest::ToolsCall {
            plane: PluginPlane::Startup,
            provider_id: self.provider.provider_id.clone(),
            provider_instance_id: self.provider.provider_instance_id.clone(),
            name: "echo".to_string(),
            arguments,
            expected_schema: schema,
        })
    }

    fn marker_count(&self, value: &str) -> usize {
        fs::read_to_string(&self.marker)
            .unwrap_or_default()
            .lines()
            .filter(|line| *line == value)
            .count()
    }

    fn marker_pid(&self, prefix: &str) -> Option<u32> {
        fs::read_to_string(&self.marker)
            .ok()?
            .lines()
            .find_map(|line| line.strip_prefix(prefix)?.parse().ok())
    }
}

fn maximum_bounded_arguments() -> Value {
    let empty = json!({"value":""});
    let overhead = serde_json::to_vec(&empty).unwrap().len();
    let arguments = json!({"value":"x".repeat(PLUGIN_MAX_ARGUMENT_BYTES - overhead)});
    assert_eq!(
        serde_json::to_vec(&arguments).unwrap().len(),
        PLUGIN_MAX_ARGUMENT_BYTES
    );
    arguments
}

fn wait_until(timeout: Duration, condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    condition()
}

fn runner_config(
    plugins: PluginConfig,
    shell: ShellConfig,
    project_registry_dir: &Path,
) -> RunnerConfig {
    use super::super::config::{
        default_websocket_connect_timeout_secs, AcpConfig, McpGatewayConfig, RunnerPolicy,
        SshConfig, ToolProvidersConfig,
    };
    RunnerConfig {
        server_url: "http://127.0.0.1:8000".to_string(),
        token: "test-token".to_string(),
        client_id: "plugin-test".to_string(),
        display_name: None,
        owner: Some("alice".to_string()),
        hostname: None,
        host_context: None,
        project_registry_dir: Some(project_registry_dir.to_path_buf()),
        legacy_projects_dir: None,
        deprecated_temporary_projects_root: None,
        poll_interval_ms: 1000,
        capabilities: None,
        max_concurrent_jobs: None,
        policy: RunnerPolicy::default(),
        transport: None,
        websocket_connect_timeout_secs: default_websocket_connect_timeout_secs(),
        quic: None,
        shell,
        ssh: SshConfig::default(),
        tool_providers: ToolProvidersConfig::default(),
        mcp_gateway: McpGatewayConfig::default(),
        plugins,
        acp: AcpConfig::default(),
    }
}

#[test]
fn startup_is_eager_persistent_and_reused() {
    let fixture = Fixture::new("normal", 2);
    assert_eq!(fixture.marker_count("start"), 1);
    assert_eq!(fixture.marker_count("initialize"), 1);
    assert_eq!(
        fixture.marker_count("list"),
        1,
        "startup admission must list eagerly"
    );
    assert_eq!(fixture.provider.status, "ready");
    assert_eq!(fixture.provider.tools.len(), 1);

    assert!(fixture.list().error.is_none());
    for expected in ["call-1", "call-2"] {
        let response = fixture.call();
        let Some(PluginGatewayResponsePayload::ToolResult { result }) = response.payload else {
            panic!("missing result: {:?}", response.error);
        };
        assert_eq!(
            result.content,
            vec![PluginContent::Text {
                text: expected.to_string()
            }]
        );
    }
    assert_eq!(
        fixture.marker_count("start"),
        1,
        "provider must never silently respawn"
    );
    assert_eq!(fixture.marker_count("call"), 2);
}

#[test]
fn bad_version_and_invalid_startup_provider_do_not_block_manager() {
    for (scenario, code) in [
        ("bad_version", "plugin_protocol_version_mismatch"),
        ("invalid_tools", "plugin_tools_list_invalid"),
        ("malformed", "plugin_malformed_json"),
        ("oversized_message", "plugin_message_too_large"),
    ] {
        let fixture = Fixture::new(scenario, 2);
        assert_eq!(fixture.provider.status, "failed", "{scenario}");
        assert_eq!(
            fixture.provider.error_code.as_deref(),
            Some(code),
            "{scenario}"
        );
        assert!(fixture.provider.tools.is_empty());
    }
}

#[test]
fn startup_secondary_admission_stays_separate_from_runtime_provider_health() {
    let fixture = Fixture::new("check_startup_large_schema", 2);
    assert_eq!(fixture.provider.status, "ready_secondary");
    assert_eq!(
        fixture.provider.error_code.as_deref(),
        Some("first_class_catalog_too_large")
    );
    assert!(fixture.provider.tools.is_empty());

    let response = fixture.manager.handle(PluginGatewayRequest::ProvidersList);
    let Some(PluginGatewayResponsePayload::Providers { providers, .. }) = response.payload else {
        panic!("missing provider view: {:?}", response.error);
    };
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].status, "ready");
    assert_eq!(providers[0].plane, PluginPlane::Startup);
    assert_eq!(providers[0].startup_direct_tool_count, 0);
    assert_eq!(providers[0].error_code, None);
}

#[test]
fn schema_change_is_not_started_and_never_dispatches_effect() {
    let fixture = Fixture::new("schema_change", 2);
    let response = fixture.call();
    assert_eq!(response.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(
        response.error.as_ref().unwrap().code,
        "plugin_schema_changed"
    );
    assert_eq!(fixture.marker_count("call"), 0);
}

#[test]
fn schema_preflight_and_effect_share_one_provider_timeout_budget() {
    let fixture = Fixture::new("split_timeout", 1);
    let response = fixture.call();
    assert_eq!(response.dispatch_state, PluginDispatchState::OutcomeUnknown);
    assert_eq!(response.error.as_ref().unwrap().code, "plugin_timeout");
    assert_eq!(
        fixture.marker_count("call"),
        1,
        "the effect starts only with the time remaining after schema preflight"
    );
}

#[test]
fn blocking_stdin_write_respects_total_deadline_and_retires_provider_tree() {
    let fixture = Fixture::new("block_after_preflight_tree", 1);
    let started = Instant::now();
    let response = fixture.call_with_arguments(maximum_bounded_arguments());
    let elapsed = started.elapsed();

    assert_eq!(response.dispatch_state, PluginDispatchState::OutcomeUnknown);
    assert_eq!(response.error.as_ref().unwrap().code, "plugin_timeout");
    assert!(
        elapsed < Duration::from_millis(2500),
        "blocked stdin write exceeded the provider deadline plus termination slack: {elapsed:?}"
    );
    assert_eq!(fixture.marker_count("stdin-blocked"), 1);
    assert_eq!(fixture.marker_count("call"), 0);

    let descendant_pid = fixture
        .marker_pid("descendant-pid:")
        .expect("fixture descendant pid");
    assert!(wait_until(Duration::from_secs(1), || {
        !crate::job_manager_tests::process_running(descendant_pid)
    }));

    let retired = fixture.call();
    assert_eq!(retired.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(
        retired.error.as_ref().unwrap().code,
        "plugin_provider_unavailable"
    );

    let shutdown_started = Instant::now();
    fixture.manager.shutdown();
    assert!(
        shutdown_started.elapsed() < Duration::from_millis(1500),
        "shutdown of an already-retired blocked writer must remain bounded"
    );
}

#[test]
fn shutdown_terminates_process_tree_while_effectful_stdin_write_is_blocked() {
    let fixture = Fixture::new("block_after_preflight_tree", 10);
    let manager = Arc::clone(&fixture.manager);
    let provider = fixture.provider.clone();
    let arguments = maximum_bounded_arguments();
    let (sender, receiver) = mpsc::channel();
    let request = std::thread::spawn(move || {
        let response = manager.handle(PluginGatewayRequest::ToolsCall {
            plane: PluginPlane::Startup,
            provider_id: provider.provider_id.clone(),
            provider_instance_id: provider.provider_instance_id.clone(),
            name: "echo".to_string(),
            arguments,
            expected_schema: provider.tools[0].schema_observation(),
        });
        let _ = sender.send(response);
    });

    assert!(wait_until(Duration::from_secs(2), || {
        fixture.marker_count("stdin-blocked") == 1
    }));
    assert!(wait_until(Duration::from_secs(2), || {
        fixture.marker_pid("descendant-pid:").is_some()
    }));
    // Give the writer worker an opportunity to enter OS pipe backpressure after
    // the preflight response has been delivered.
    std::thread::sleep(Duration::from_millis(50));

    let shutdown_started = Instant::now();
    fixture.manager.shutdown();
    let shutdown_elapsed = shutdown_started.elapsed();
    assert!(
        shutdown_elapsed < Duration::from_millis(2500),
        "shutdown waited on the request/session lock: {shutdown_elapsed:?}"
    );

    let response = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("blocked request must be released by process-tree termination");
    request.join().unwrap();
    assert_eq!(response.dispatch_state, PluginDispatchState::OutcomeUnknown);
    assert!(matches!(
        response.error.as_ref().map(|error| error.code.as_str()),
        Some("plugin_manager_stopping") | Some("plugin_stdin_failed") | Some("plugin_eof")
    ));
    let descendant_pid = fixture.marker_pid("descendant-pid:").unwrap();
    assert!(wait_until(Duration::from_secs(1), || {
        !crate::job_manager_tests::process_running(descendant_pid)
    }));
}

#[test]
fn crash_after_effect_send_is_outcome_unknown_and_instance_is_retired() {
    let fixture = Fixture::new("crash", 2);
    let first = fixture.call();
    assert_eq!(first.dispatch_state, PluginDispatchState::OutcomeUnknown);
    assert_eq!(first.error.as_ref().unwrap().code, "plugin_eof");
    let second = fixture.call();
    assert_eq!(second.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(
        second.error.as_ref().unwrap().code,
        "plugin_provider_unavailable"
    );
    assert_eq!(fixture.marker_count("start"), 1);
    assert_eq!(fixture.marker_count("call"), 1);
}

#[test]
fn unsupported_result_is_completed_and_retires_protocol_broken_instance() {
    let fixture = Fixture::new("bad_result", 2);
    let response = fixture.call();
    assert_eq!(response.dispatch_state, PluginDispatchState::Completed);
    assert_eq!(
        response.error.as_ref().unwrap().code,
        "plugin_result_invalid"
    );
    assert_eq!(
        fixture.list().error.as_ref().unwrap().code,
        "plugin_provider_unavailable"
    );
}

#[test]
fn stderr_is_diagnostic_only_and_never_enters_catalog_or_result() {
    let fixture = Fixture::new("stderr", 2);
    let catalog = serde_json::to_string(&fixture.manager.startup_catalog()).unwrap();
    assert!(!catalog.contains("diagnostic-only-secret-looking-stderr"));
    let result = serde_json::to_string(&fixture.call()).unwrap();
    assert!(!result.contains("diagnostic-only-secret-looking-stderr"));
}

#[test]
fn provider_busy_is_not_started() {
    let fixture = Fixture::new("slow", 2);
    let manager = Arc::clone(&fixture.manager);
    let provider = fixture.provider.clone();
    let first = std::thread::spawn(move || {
        manager.handle(PluginGatewayRequest::ToolsCall {
            plane: PluginPlane::Startup,
            provider_id: provider.provider_id.clone(),
            provider_instance_id: provider.provider_instance_id.clone(),
            name: "echo".to_string(),
            arguments: json!({"value":"first"}),
            expected_schema: provider.tools[0].schema_observation(),
        })
    });
    for _ in 0..100 {
        if fixture.marker_count("call") == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let discovery = fixture.list();
    assert_eq!(discovery.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(
        discovery.error.as_ref().unwrap().code,
        "plugin_provider_busy"
    );
    let second_call = fixture.call();
    assert_eq!(second_call.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(
        second_call.error.as_ref().unwrap().code,
        "plugin_provider_busy"
    );
    assert!(first.join().unwrap().error.is_none());
}

#[test]
fn prepared_environment_reuses_shell_env_default_profile_and_clears_sensitive_values() {
    use super::super::config::ShellProfileConfig;
    use std::collections::BTreeMap;
    let _guard = crate::tests::test_env_lock();
    let _env = crate::tests::EnvGuard::new().set("WEBCODEX_AGENT_TOKEN", "must-not-leak");
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("marker.log");
    let fake = fake_binary();
    let mut shell = ShellConfig::default();
    shell
        .env
        .insert("WEBCODEX_PLUGIN_TEST_ENV".to_string(), "base".to_string());
    shell.default_profile = Some("plugin".to_string());
    shell.profiles = BTreeMap::from([(
        "plugin".to_string(),
        ShellProfileConfig {
            env: BTreeMap::from([(
                "WEBCODEX_PLUGIN_TEST_ENV".to_string(),
                "profile-ready".to_string(),
            )]),
            ..ShellProfileConfig::default()
        },
    )]);
    let plugins = PluginConfig {
        request_timeout_secs: 2,
        providers: vec![PluginProviderConfig {
            id: "fake".to_string(),
            name: "Fake Plugin".to_string(),
            command: fake.path.to_string_lossy().into_owned(),
            args: vec![
                "execution_context".to_string(),
                marker.to_string_lossy().into_owned(),
                temp.path().to_string_lossy().into_owned(),
            ],
            cwd: Some(temp.path().to_string_lossy().into_owned()),
            profile: None,
            timeout_secs: None,
        }],
    };
    let config = runner_config(plugins, shell, temp.path());
    let manager = PluginManager::new(&config, temp.path().join("runner.toml"));
    assert_eq!(manager.startup_catalog()[0].status, "ready");
    let markers = fs::read_to_string(marker).unwrap();
    assert!(markers.contains("profile-env-ok"));
    assert!(markers.contains("sensitive-env-cleared"));
    assert!(markers.contains("cwd-ok"));
}

#[test]
fn bare_plugin_command_resolves_from_prepared_path_with_explicit_profile() {
    use super::super::config::ShellProfileConfig;
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let fake = fake_binary();
    let command_name = format!("wc-plugin-fixture{}", env::consts::EXE_SUFFIX);
    let installed = bin.join(&command_name);
    fs::copy(&fake.path, &installed).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&installed).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&installed, permissions).unwrap();
    }
    let marker = temp.path().join("marker.log");
    let mut shell = ShellConfig::default();
    shell.path_prepend.push(bin);
    shell.profiles.insert(
        "native".to_string(),
        ShellProfileConfig {
            env: BTreeMap::from([(
                "WEBCODEX_PLUGIN_TEST_ENV".to_string(),
                "profile-ready".to_string(),
            )]),
            ..ShellProfileConfig::default()
        },
    );
    let plugins = PluginConfig {
        request_timeout_secs: 2,
        providers: vec![PluginProviderConfig {
            id: "fake".to_string(),
            name: "Fake Plugin".to_string(),
            command: command_name,
            args: vec!["normal".to_string(), marker.to_string_lossy().into_owned()],
            cwd: Some(temp.path().to_string_lossy().into_owned()),
            profile: Some("native".to_string()),
            timeout_secs: None,
        }],
    };
    let config = runner_config(plugins, shell, temp.path());
    let manager = PluginManager::new(&config, temp.path().join("runner.toml"));
    assert_eq!(manager.startup_catalog()[0].status, "ready");
    assert_eq!(
        fs::read_to_string(marker)
            .unwrap_or_default()
            .lines()
            .filter(|line| *line == "start")
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn plugin_profile_init_script_is_captured_into_native_child_environment() {
    use super::super::config::ShellProfileConfig;
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("marker.log");
    let fake = fake_binary();
    let mut shell = ShellConfig::default();
    shell.profiles.insert(
        "native".to_string(),
        ShellProfileConfig {
            init_script: Some("export WEBCODEX_PLUGIN_TEST_ENV=profile-ready".to_string()),
            ..ShellProfileConfig::default()
        },
    );
    let plugins = PluginConfig {
        request_timeout_secs: 2,
        providers: vec![PluginProviderConfig {
            id: "fake".to_string(),
            name: "Fake Plugin".to_string(),
            command: fake.path.to_string_lossy().into_owned(),
            args: vec![
                "execution_context".to_string(),
                marker.to_string_lossy().into_owned(),
                temp.path().to_string_lossy().into_owned(),
            ],
            cwd: Some(temp.path().to_string_lossy().into_owned()),
            profile: Some("native".to_string()),
            timeout_secs: None,
        }],
    };
    let config = runner_config(plugins, shell, temp.path());
    let manager = PluginManager::new(&config, temp.path().join("runner.toml"));
    assert_eq!(manager.startup_catalog()[0].status, "ready");
    let markers = fs::read_to_string(marker).unwrap();
    assert!(markers.contains("profile-env-ok"));
}

#[test]
fn direct_startup_catalog_remains_frozen_across_dynamic_reload() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("marker.log");
    let fake = fake_binary();
    let startup_plugins = PluginConfig {
        request_timeout_secs: 2,
        providers: vec![PluginProviderConfig {
            id: "fake".to_string(),
            name: "Fake Plugin".to_string(),
            command: fake.path.to_string_lossy().into_owned(),
            args: vec!["normal".to_string(), marker.to_string_lossy().into_owned()],
            cwd: Some(temp.path().to_string_lossy().into_owned()),
            profile: None,
            timeout_secs: None,
        }],
    };
    let config_path = temp.path().join("runner.toml");
    write_runner_toml(&config_path, temp.path(), &fake.path, &marker, "normal");
    let config = runner_config(startup_plugins, ShellConfig::default(), temp.path());
    let manager = PluginManager::new(&config, config_path);
    let startup = manager.startup_catalog();
    let startup_instance = startup[0].provider_instance_id.clone();
    let reloaded = manager.handle(PluginGatewayRequest::Reload);
    let Some(PluginGatewayResponsePayload::Reloaded {
        providers,
        first_class_restart_required,
        ..
    }) = reloaded.payload
    else {
        panic!("reload failed: {:?}", reloaded.error);
    };
    assert!(first_class_restart_required);
    assert_ne!(providers[0].provider_instance_id, startup_instance);
    assert_eq!(
        manager.startup_catalog(),
        startup,
        "startup catalog must be immutable"
    );

    let direct = manager.handle(PluginGatewayRequest::ToolsList {
        plane: PluginPlane::Startup,
        provider_id: "fake".to_string(),
        provider_instance_id: startup_instance,
    });
    assert!(direct.error.is_none());
}

#[test]
fn concurrent_reload_is_busy_while_existing_dynamic_calls_continue_and_later_reload_wins() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("marker.log");
    let release = marker.with_extension("release");
    let fake = fake_binary();
    let startup_plugins = PluginConfig {
        request_timeout_secs: 2,
        providers: vec![PluginProviderConfig {
            id: "fake".to_string(),
            name: "Fake Plugin".to_string(),
            command: fake.path.to_string_lossy().into_owned(),
            args: vec!["normal".to_string(), marker.to_string_lossy().into_owned()],
            cwd: Some(temp.path().to_string_lossy().into_owned()),
            profile: None,
            timeout_secs: None,
        }],
    };
    let config_path = temp.path().join("runner.toml");
    write_runner_toml(&config_path, temp.path(), &fake.path, &marker, "normal");
    let config = runner_config(startup_plugins, ShellConfig::default(), temp.path());
    let manager = Arc::new(PluginManager::new(&config, config_path.clone()));
    let startup = manager.startup_catalog();
    let expected_schema = startup[0].tools[0].schema_observation();

    let first_reload = manager.handle(PluginGatewayRequest::Reload);
    let Some(PluginGatewayResponsePayload::Reloaded { providers, .. }) = first_reload.payload
    else {
        panic!("initial dynamic reload failed: {:?}", first_reload.error);
    };
    let dynamic_v1 = providers[0].provider_instance_id.clone();

    let _ = fs::remove_file(&release);
    write_runner_toml_with_timeout(
        &config_path,
        temp.path(),
        &fake.path,
        &marker,
        "reload_block_list",
        10,
    );
    let reload_manager = Arc::clone(&manager);
    let reload_a = std::thread::spawn(move || reload_manager.handle(PluginGatewayRequest::Reload));
    assert!(wait_until(Duration::from_secs(2), || {
        fs::read_to_string(&marker)
            .unwrap_or_default()
            .lines()
            .any(|line| line == "reload-blocked")
    }));

    // A has already read the old config and is blocked preparing its candidate.
    // Publish a different config while A owns the reload gate.
    write_runner_toml_with_timeout(
        &config_path,
        temp.path(),
        &fake.path,
        &marker,
        "reload_new",
        2,
    );
    let busy_started = Instant::now();
    let busy = manager.handle(PluginGatewayRequest::Reload);
    assert_eq!(busy.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(busy.error.as_ref().unwrap().code, "plugin_reload_busy");
    assert!(
        busy_started.elapsed() < Duration::from_secs(1),
        "a second reload must fail busy instead of waiting for candidate preparation"
    );

    let current = manager.handle(PluginGatewayRequest::ProvidersList);
    let Some(PluginGatewayResponsePayload::Providers { providers, .. }) = current.payload else {
        panic!("current dynamic provider list failed: {:?}", current.error);
    };
    assert_eq!(providers[0].provider_instance_id, dynamic_v1);
    let call_while_reloading = manager.handle(PluginGatewayRequest::ToolsCall {
        plane: PluginPlane::Effective,
        provider_id: "fake".to_string(),
        provider_instance_id: dynamic_v1.clone(),
        name: "echo".to_string(),
        arguments: json!({"value":"during-reload"}),
        expected_schema: expected_schema.clone(),
    });
    assert!(
        call_while_reloading.error.is_none(),
        "candidate preparation must not hold the dynamic-state lock or block the current provider"
    );
    assert_eq!(manager.startup_catalog(), startup);

    fs::write(&release, b"release").unwrap();
    let completed_a = reload_a.join().unwrap();
    let Some(PluginGatewayResponsePayload::Reloaded { providers, .. }) = completed_a.payload else {
        panic!("reload A failed after release: {:?}", completed_a.error);
    };
    let dynamic_a = providers[0].provider_instance_id.clone();
    assert_ne!(dynamic_a, dynamic_v1);
    assert_eq!(
        fs::read_to_string(&marker)
            .unwrap_or_default()
            .lines()
            .filter(|line| *line == "reload-new-start")
            .count(),
        0,
        "busy reload B must not have started a candidate from the new config"
    );

    let reload_c = manager.handle(PluginGatewayRequest::Reload);
    let Some(PluginGatewayResponsePayload::Reloaded { providers, .. }) = reload_c.payload else {
        panic!("later reload C failed: {:?}", reload_c.error);
    };
    let dynamic_c = providers[0].provider_instance_id.clone();
    assert_ne!(dynamic_c, dynamic_a);
    assert_eq!(
        fs::read_to_string(&marker)
            .unwrap_or_default()
            .lines()
            .filter(|line| *line == "reload-new-start")
            .count(),
        1,
        "reload C must read and prepare the new config after A releases the gate"
    );
    assert_eq!(manager.startup_catalog(), startup);

    let final_view = manager.handle(PluginGatewayRequest::ProvidersList);
    let Some(PluginGatewayResponsePayload::Providers { providers, .. }) = final_view.payload else {
        panic!("final provider list failed: {:?}", final_view.error);
    };
    assert_eq!(providers[0].provider_instance_id, dynamic_c);
    manager.shutdown();
}

#[test]
fn shutdown_during_blocked_reload_does_not_wait_for_gate_or_allow_late_commit() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("marker.log");
    let fake = fake_binary();
    let startup_plugins = PluginConfig {
        request_timeout_secs: 2,
        providers: vec![PluginProviderConfig {
            id: "fake".to_string(),
            name: "Fake Plugin".to_string(),
            command: fake.path.to_string_lossy().into_owned(),
            args: vec!["normal".to_string(), marker.to_string_lossy().into_owned()],
            cwd: Some(temp.path().to_string_lossy().into_owned()),
            profile: None,
            timeout_secs: None,
        }],
    };
    let config_path = temp.path().join("runner.toml");
    write_runner_toml_with_timeout(
        &config_path,
        temp.path(),
        &fake.path,
        &marker,
        "reload_block_list",
        10,
    );
    let config = runner_config(startup_plugins, ShellConfig::default(), temp.path());
    let manager = Arc::new(PluginManager::new(&config, config_path));
    let reload_manager = Arc::clone(&manager);
    let reload = std::thread::spawn(move || reload_manager.handle(PluginGatewayRequest::Reload));
    assert!(wait_until(Duration::from_secs(2), || {
        fs::read_to_string(&marker)
            .unwrap_or_default()
            .lines()
            .any(|line| line == "reload-blocked")
    }));

    let shutdown_started = Instant::now();
    manager.shutdown();
    assert!(
        shutdown_started.elapsed() < Duration::from_millis(1500),
        "shutdown must not wait for the reload gate held by candidate preparation"
    );
    let reload_response = reload.join().unwrap();
    assert_eq!(
        reload_response.dispatch_state,
        PluginDispatchState::NotStarted
    );
    assert_eq!(
        reload_response.error.as_ref().unwrap().code,
        "plugin_manager_stopping"
    );
    assert_eq!(
        fs::read_to_string(&marker)
            .unwrap_or_default()
            .lines()
            .filter(|line| *line == "reload-released")
            .count(),
        0,
        "the blocked candidate must terminate from manager stopping rather than reaching commit"
    );
}

#[test]
fn failed_dynamic_candidate_keeps_previous_instance_and_removal_is_tombstoned() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("marker.log");
    let fake = fake_binary();
    let startup_plugins = PluginConfig {
        request_timeout_secs: 2,
        providers: vec![PluginProviderConfig {
            id: "fake".to_string(),
            name: "Fake Plugin".to_string(),
            command: fake.path.to_string_lossy().into_owned(),
            args: vec!["normal".to_string(), marker.to_string_lossy().into_owned()],
            cwd: Some(temp.path().to_string_lossy().into_owned()),
            profile: None,
            timeout_secs: None,
        }],
    };
    let config_path = temp.path().join("runner.toml");
    write_runner_toml(&config_path, temp.path(), &fake.path, &marker, "normal");
    let config = runner_config(startup_plugins, ShellConfig::default(), temp.path());
    let manager = PluginManager::new(&config, config_path.clone());
    let startup_instance = manager.startup_catalog()[0].provider_instance_id.clone();

    let first_reload = manager.handle(PluginGatewayRequest::Reload);
    let Some(PluginGatewayResponsePayload::Reloaded { providers, .. }) = first_reload.payload
    else {
        panic!("first dynamic reload failed: {:?}", first_reload.error);
    };
    let dynamic_instance = providers[0].provider_instance_id.clone();
    assert_ne!(dynamic_instance, startup_instance);

    write_runner_toml(
        &config_path,
        temp.path(),
        &fake.path,
        &marker,
        "bad_version",
    );
    let failed_reload = manager.handle(PluginGatewayRequest::Reload);
    let Some(PluginGatewayResponsePayload::Reloaded {
        providers,
        failures,
        ..
    }) = failed_reload.payload
    else {
        panic!(
            "failed candidate reload response missing: {:?}",
            failed_reload.error
        );
    };
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].provider_id, "fake");
    assert_eq!(
        providers[0].provider_instance_id, dynamic_instance,
        "a failed candidate must not destroy the previous working dynamic instance"
    );

    write_runner_toml_without_plugins(&config_path, temp.path());
    let removal = manager.handle(PluginGatewayRequest::Reload);
    let Some(PluginGatewayResponsePayload::Reloaded { providers, .. }) = removal.payload else {
        panic!("removal reload failed: {:?}", removal.error);
    };
    assert!(
        providers.is_empty(),
        "removed provider must disappear from effective dynamic view"
    );
    let effective_fallback = manager.handle(PluginGatewayRequest::ToolsList {
        plane: PluginPlane::Effective,
        provider_id: "fake".to_string(),
        provider_instance_id: startup_instance.clone(),
    });
    assert_eq!(
        effective_fallback
            .error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("stale_plugin_provider"),
        "dynamic removal tombstone must prevent fallback to startup for plugin_tool"
    );

    let direct_startup = manager.handle(PluginGatewayRequest::ToolsList {
        plane: PluginPlane::Startup,
        provider_id: "fake".to_string(),
        provider_instance_id: startup_instance,
    });
    assert!(
        direct_startup.error.is_none(),
        "dynamic removal must not mutate the frozen direct startup plane"
    );
}

fn write_runner_toml(
    path: &Path,
    project_registry_dir: &Path,
    executable: &Path,
    marker: &Path,
    scenario: &str,
) {
    write_runner_toml_with_timeout(path, project_registry_dir, executable, marker, scenario, 2);
}

fn write_runner_toml_with_timeout(
    path: &Path,
    project_registry_dir: &Path,
    executable: &Path,
    marker: &Path,
    scenario: &str,
    request_timeout_secs: u64,
) {
    fn quoted(value: &Path) -> String {
        format!("{:?}", value.to_string_lossy().as_ref())
    }
    fs::write(
        path,
        format!(
            "server_url = \"http://127.0.0.1:8000\"\ntoken = \"test-token\"\nclient_id = \"plugin-test\"\nproject_registry_dir = {}\n\n[plugins]\nrequest_timeout_secs = {request_timeout_secs}\n\n[[plugins.providers]]\nid = \"fake\"\nname = \"Fake Plugin\"\ncommand = {}\nargs = [\"{}\", {}]\ncwd = {}\n",
            quoted(project_registry_dir),
            quoted(executable),
            scenario,
            quoted(marker),
            quoted(project_registry_dir),
        ),
    )
    .unwrap();
}

fn write_runner_toml_without_plugins(path: &Path, project_registry_dir: &Path) {
    fn quoted(value: &Path) -> String {
        format!("{:?}", value.to_string_lossy().as_ref())
    }
    fs::write(
        path,
        format!(
            "server_url = \"http://127.0.0.1:8000\"\ntoken = \"test-token\"\nclient_id = \"plugin-test\"\nproject_registry_dir = {}\n\n[plugins]\nrequest_timeout_secs = 2\n",
            quoted(project_registry_dir),
        ),
    )
    .unwrap();
}
