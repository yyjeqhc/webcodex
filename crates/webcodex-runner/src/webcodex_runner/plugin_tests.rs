use super::*;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tempfile::TempDir;
use webcodex_core::plugin::{PluginContent, PluginGatewayResponsePayload};

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
        let schema = self.provider.tools[0].schema_observation();
        self.manager.handle(PluginGatewayRequest::ToolsCall {
            plane: PluginPlane::Startup,
            provider_id: self.provider.provider_id.clone(),
            provider_instance_id: self.provider.provider_instance_id.clone(),
            name: "echo".to_string(),
            arguments: json!({"value":"hello"}),
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
    let second = fixture.call();
    assert_eq!(second.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(second.error.as_ref().unwrap().code, "plugin_provider_busy");
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

fn write_runner_toml(
    path: &Path,
    project_registry_dir: &Path,
    executable: &Path,
    marker: &Path,
    scenario: &str,
) {
    fn quoted(value: &Path) -> String {
        format!("{:?}", value.to_string_lossy().as_ref())
    }
    fs::write(
        path,
        format!(
            "server_url = \"http://127.0.0.1:8000\"\ntoken = \"test-token\"\nclient_id = \"plugin-test\"\nproject_registry_dir = {}\n\n[plugins]\nrequest_timeout_secs = 2\n\n[[plugins.providers]]\nid = \"fake\"\nname = \"Fake Plugin\"\ncommand = {}\nargs = [\"{}\", {}]\ncwd = {}\n",
            quoted(project_registry_dir),
            quoted(executable),
            scenario,
            quoted(marker),
            quoted(project_registry_dir),
        ),
    )
    .unwrap();
}
