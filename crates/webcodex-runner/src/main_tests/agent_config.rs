use super::*;

#[test]
fn agent_config_defaults_transport_to_websocket_without_quic_section() {
    // No transport field and no [quic] section: default stays websocket.
    let toml = r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
"#;
    let cfg: AgentConfig = toml::from_str(toml).unwrap();
    assert!(cfg.transport.is_none());
    assert!(cfg.quic.is_none());
    assert_eq!(effective_transport(&cfg), TRANSPORT_WEBSOCKET);
    assert_eq!(
        cfg.websocket_connect_timeout_secs,
        default_websocket_connect_timeout_secs()
    );
    assert_eq!(
        auto_transport_plan(&cfg),
        vec![TRANSPORT_WEBSOCKET, TRANSPORT_POLLING]
    );
    assert_eq!(cfg.mcp_gateway.request_timeout_secs, 30);
    assert!(cfg.mcp_gateway.providers.is_empty());
}

#[test]
fn agent_config_rejects_zero_websocket_connect_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
websocket_connect_timeout_secs = 0
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(
        err.contains("websocket_connect_timeout_secs must be > 0"),
        "{err}"
    );
}

#[test]
fn agent_config_rejects_relative_temporary_projects_root() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
temporary_projects_root = "temporary"
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(
        err.contains("temporary_projects_root must be a non-empty absolute path"),
        "{err}"
    );
}

#[test]
fn agent_config_accepts_transport_quic_with_quic_section() {
    let toml = r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
transport = "quic"

[quic]
server_addr = "v4.example.test:8443"
server_name = "v4.example.test"
"#;
    let cfg: AgentConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.transport.as_deref(), Some("quic"));
    let quic = cfg.quic.expect("quic section");
    assert_eq!(quic.server_addr, "v4.example.test:8443");
    assert_eq!(quic.server_name, "v4.example.test");
    // Defaults applied.
    assert_eq!(quic.alpn, "webcodex-runner/1");
    assert_eq!(quic.connect_timeout_secs, 10);
    assert_eq!(quic.keepalive_interval_secs, 20);
}

#[test]
fn agent_config_accepts_transport_auto() {
    let toml = r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
transport = "auto"
"#;
    let cfg: AgentConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.transport.as_deref(), Some(TRANSPORT_AUTO));
    assert_eq!(effective_transport(&cfg), TRANSPORT_AUTO);
    assert_eq!(
        auto_transport_plan(&cfg),
        vec![TRANSPORT_WEBSOCKET, TRANSPORT_POLLING]
    );
}

#[test]
fn auto_transport_plan_tries_quic_then_websocket_then_polling() {
    let mut cfg = test_config(PathBuf::from("/tmp/x"));
    cfg.transport = Some(TRANSPORT_AUTO.to_string());
    cfg.quic = Some(quic_client_config());
    assert_eq!(
        auto_transport_plan(&cfg),
        vec![TRANSPORT_QUIC, TRANSPORT_WEBSOCKET, TRANSPORT_POLLING]
    );
}

#[test]
fn strict_quic_transport_still_requires_quic_section() {
    let mut cfg = test_config(PathBuf::from("/tmp/x"));
    cfg.transport = Some(TRANSPORT_QUIC.to_string());
    let err = resolve_quic_config(&cfg).unwrap_err();
    assert!(err.contains("transport=quic requires a [quic] section"));
    assert_eq!(effective_transport(&cfg), TRANSPORT_QUIC);
}

#[test]
fn resolve_quic_config_errors_when_section_missing() {
    let mut cfg = test_config(PathBuf::from("/tmp/x"));
    cfg.transport = Some("quic".to_string());
    let err = resolve_quic_config(&cfg).unwrap_err();
    assert!(err.contains("[quic]"), "err was: {err}");
}

#[test]
fn resolve_quic_config_errors_when_server_addr_or_name_missing() {
    let mut cfg = test_config(PathBuf::from("/tmp/x"));
    cfg.transport = Some("quic".to_string());

    // Missing server_addr.
    cfg.quic = Some(QuicClientConfig {
        server_addr: "  ".to_string(),
        server_name: "v4.example.test".to_string(),
        alpn: default_quic_alpn(),
        connect_timeout_secs: 10,
        keepalive_interval_secs: 20,
    });
    let err = resolve_quic_config(&cfg).unwrap_err();
    assert!(err.contains("server_addr"), "err was: {err}");

    // Missing server_name.
    cfg.quic = Some(QuicClientConfig {
        server_addr: "v4.example.test:8443".to_string(),
        server_name: String::new(),
        alpn: default_quic_alpn(),
        connect_timeout_secs: 10,
        keepalive_interval_secs: 20,
    });
    let err = resolve_quic_config(&cfg).unwrap_err();
    assert!(err.contains("server_name"), "err was: {err}");
}

#[test]
fn resolve_quic_config_accepts_valid_section() {
    let mut cfg = test_config(PathBuf::from("/tmp/x"));
    cfg.transport = Some("quic".to_string());
    cfg.quic = Some(quic_client_config());
    let resolved = resolve_quic_config(&cfg).unwrap();
    assert_eq!(resolved.server_addr, "v4.example.test:8443");
    assert_eq!(resolved.server_name, "v4.example.test");
}

#[test]
fn resolve_quic_server_addrs_accepts_hostname_port() {
    let addrs = resolve_quic_server_addrs("localhost:8443").unwrap();
    assert!(addrs.iter().any(|addr| addr.port() == 8443));
}

#[test]
fn resolve_quic_server_addrs_rejects_missing_port() {
    let err = resolve_quic_server_addrs("localhost").unwrap_err();
    assert!(err.contains("failed to resolve"), "err was: {err}");
}

#[test]
fn quic_client_bind_addr_matches_remote_address_family() {
    let v4: SocketAddr = "127.0.0.1:8443".parse().unwrap();
    let v6: SocketAddr = "[::1]:8443".parse().unwrap();
    assert!(quic_client_bind_addr_for(v4).is_ipv4());
    assert!(quic_client_bind_addr_for(v6).is_ipv6());
}

#[test]
fn agent_cli_help_and_version_exit_before_runtime() {
    match parse_agent_args(["--help"]).unwrap() {
        AgentCliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("Usage: webcodex-runner"));
            assert!(!stdout.contains("webcodex-runner init"));
            assert!(stderr.is_empty());
        }
        other => panic!("expected help exit, got {other:?}"),
    }
    match parse_agent_args(["--version"]).unwrap() {
        AgentCliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            assert_eq!(code, 0);
            assert!(stdout.starts_with(&format!(
                "webcodex-runner {} (commit ",
                env!("CARGO_PKG_VERSION")
            )));
            assert!(stdout.trim_end().ends_with(')'));
            assert_ne!(
                stdout,
                format!("webcodex-runner {}\n", env!("CARGO_PKG_VERSION"))
            );
            assert!(stderr.is_empty());
        }
        other => panic!("expected version exit, got {other:?}"),
    }
}

#[test]
fn agent_cli_has_no_init_alias() {
    let error = parse_agent_args(["init"]).unwrap_err();
    assert!(error.contains("unknown argument: init"));
}

#[test]
fn agent_version_output_includes_build_metadata() {
    match parse_agent_args(["-V"]).unwrap() {
        AgentCliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("commit "));
            assert!(stdout.starts_with("webcodex-runner "));
            assert!(stderr.is_empty());
        }
        other => panic!("expected version exit, got {other:?}"),
    }
}

#[test]
fn agent_cli_legacy_runtime_args_are_preserved() {
    let action = parse_agent_args(["--config", "/tmp/agent.toml", "--once"]).unwrap();
    assert_eq!(
        action,
        AgentCliAction::Run {
            config_path: PathBuf::from("/tmp/agent.toml"),
            once: true,
        }
    );
}

#[test]
fn agent_cli_profile_derives_default_config_path() {
    let action = parse_agent_args(["--profile", "special"]).unwrap();
    assert_eq!(
        action,
        AgentCliAction::Run {
            config_path: client_profile_agent_config("special").unwrap(),
            once: false,
        }
    );
}

#[test]
fn agent_cli_explicit_config_overrides_profile() {
    let action = parse_agent_args(["--profile", "special", "--config", "/tmp/agent.toml"]).unwrap();
    assert_eq!(
        action,
        AgentCliAction::Run {
            config_path: PathBuf::from("/tmp/agent.toml"),
            once: false,
        }
    );
}

#[test]
fn agent_cli_rejects_unsafe_profile() {
    let err = parse_agent_args(["--profile", "../x"]).unwrap_err();
    assert_eq!(err, CLIENT_PROFILE_ERROR);
}

#[test]
fn empty_tokens_config_parser_accepts_empty_and_whitespace_token() {
    for token in ["", "   "] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
                &path,
                format!(
                    "server_url = \"http://127.0.0.1:8000\"\ntoken = \"{}\"\nclient_id = \"open-agent\"\n[policy]\nallow_cwd_anywhere = true\n",
                    token
                ),
            )
            .unwrap();

        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.token, token);
        assert_eq!(non_empty_token(&cfg.token), None);
    }
}

#[test]
fn agent_config_host_context_is_normalized_closed_and_restart_scoped() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"
projects_dir = "projects.d"

[host_context]
role = " server_host "
runtime = " Prefer this Runner for Server-host operations. "
service = "Use the ordinary host-local service mechanism."

[policy]
allow_cwd_anywhere = true
"#,
    )
    .unwrap();
    let cfg = load_config(&path).unwrap();
    let context = cfg.host_context.as_ref().expect("host context");
    assert_eq!(context.role.as_deref(), Some("server_host"));
    assert_eq!(
        context.runtime.as_deref(),
        Some("Prefer this Runner for Server-host operations.")
    );

    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"
projects_dir = "projects.d"
[host_context]
role = "server_host"
arbitrary = "not allowed"
[policy]
allow_cwd_anywhere = true
"#,
    )
    .unwrap();
    let err = load_config(&path).unwrap_err();
    assert!(err.contains("failed to parse config"), "{err}");
    assert!(err.contains("arbitrary"), "{err}");
}

#[test]
fn agent_config_without_shell_section_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true
"#,
    )
    .unwrap();

    let cfg = load_config(&path).unwrap();
    assert_eq!(cfg.shell, ShellConfig::default());
    assert_eq!(cfg.shell.max_persistent_shells, 8);
    assert_eq!(cfg.shell.persistent_shell_idle_timeout_secs, 30 * 60);
}

#[test]
fn agent_config_persistent_shell_limits_are_validated() {
    let mut shell = ShellConfig {
        max_persistent_shells: 0,
        ..Default::default()
    };
    assert!(validate_shell_config(&shell)
        .unwrap_err()
        .contains("max_persistent_shells"));

    shell.max_persistent_shells = 65;
    assert!(validate_shell_config(&shell)
        .unwrap_err()
        .contains("max_persistent_shells"));

    shell.max_persistent_shells = 8;
    shell.persistent_shell_idle_timeout_secs = 0;
    assert!(validate_shell_config(&shell)
        .unwrap_err()
        .contains("persistent_shell_idle_timeout_secs"));

    shell.persistent_shell_idle_timeout_secs = 86_401;
    assert!(validate_shell_config(&shell)
        .unwrap_err()
        .contains("persistent_shell_idle_timeout_secs"));
}

#[test]
fn agent_config_loads_named_ssh_resources_without_authentication_material() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[ssh.resources.tmp]
host = "tmp"
default_cwd = "/opt/webcodex-edge"

[ssh.resources.no_default]
host = "ops-alias"
"#,
    )
    .unwrap();

    let cfg = load_config(&path).unwrap();
    let tmp = cfg.ssh.resources.get("tmp").unwrap();
    assert_eq!(tmp.host, "tmp");
    assert_eq!(tmp.default_cwd.as_deref(), Some("/opt/webcodex-edge"));
    assert_eq!(
        cfg.ssh
            .resources
            .get("no_default")
            .and_then(|resource| resource.default_cwd.as_deref()),
        None
    );
}

#[test]
fn agent_config_shell_profiles_parse() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
default_profile = "rust"

[shell.profiles.rust]
description = "Rust development tools"
program = "sh"
args = ["-c"]
init_script = '''
export RUST_BACKTRACE=1
'''

[shell.profiles.rust.env]
PATH = "/root/.cargo/bin:/usr/bin:/bin"
CARGO_HOME = "/root/.cargo"
RUSTUP_HOME = "/root/.rustup"

[shell.profiles.py-venv]
description = "Project-local Python virtual environment"
program = "bash"
args = ["-lc"]
init_script = '''
source .venv/bin/activate
'''
"#,
    )
    .unwrap();

    let cfg = load_config(&path).unwrap();
    assert_eq!(cfg.shell.default_profile.as_deref(), Some("rust"));
    let rust = cfg.shell.profiles.get("rust").unwrap();
    assert_eq!(rust.description.as_deref(), Some("Rust development tools"));
    assert_eq!(rust.program.as_deref(), Some("sh"));
    assert_eq!(rust.args.as_ref().unwrap(), &vec!["-c".to_string()]);
    assert_eq!(
        rust.env.get("CARGO_HOME").map(String::as_str),
        Some("/root/.cargo")
    );
    assert!(rust
        .init_script
        .as_deref()
        .unwrap()
        .contains("RUST_BACKTRACE=1"));
    assert!(cfg.shell.profiles.contains_key("py-venv"));
}

#[test]
fn agent_config_shell_default_profile_must_exist() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
default_profile = "missing"

[shell.profiles.rust]
program = "sh"
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.contains("default_profile"), "{err}");
    assert!(err.contains("missing"), "{err}");
}

#[test]
fn agent_config_shell_profile_name_must_be_safe() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell.profiles."bad/name"]
program = "sh"
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.contains("shell profile name"), "{err}");
    assert!(err.contains("slash"), "{err}");
}

#[test]
fn agent_config_shell_profile_type_errors_are_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell.profiles.rust]
args = "-c"
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.contains("failed to parse config"), "{err}");
    assert!(err.contains("args"), "{err}");
}

#[test]
fn agent_config_shell_profile_env_type_errors_are_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell.profiles.rust.env]
PATH = ["/root/.cargo/bin"]
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.contains("failed to parse config"), "{err}");
    assert!(err.contains("env"), "{err}");
}

#[test]
fn agent_config_shell_errors_do_not_include_init_script_body() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    let secret = "DO_NOT_LEAK_THIS_INLINE_SCRIPT_BODY";
    std::fs::write(
        &path,
        format!(
            r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
default_profile = "missing"

[shell.profiles.rust]
init_script = '''
export SECRET={}
'''
"#,
            secret
        ),
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.contains("default_profile"), "{err}");
    assert!(!err.contains(secret), "{err}");
}

#[test]
fn agent_project_toml_parse_sorts_hook_names() {
    let project = parse_agent_project_toml(
        r#"
id = "webcodex"
path = "/root/git/webcodex"
kind = "rust"
shell_profile = "rust"

[hooks]
precommit = ["cargo test"]
doctor = ["git status --short"]
"#,
    )
    .unwrap();
    let summary = agent_project_summary(&project, 123456, false);
    assert_eq!(summary.id, "webcodex");
    assert_eq!(summary.name.as_deref(), Some("webcodex"));
    assert_eq!(summary.path, "/root/git/webcodex");
    assert_eq!(summary.kind.as_deref(), Some("rust"));
    assert_eq!(summary.hooks, vec!["doctor", "precommit"]);
    assert_eq!(summary.updated_at, 123456);
    assert_eq!(summary.git_branch, None);
    assert_eq!(project.shell_profile.as_deref(), Some("rust"));
}

#[test]
fn agent_project_toml_rejects_invalid_id() {
    let err = parse_agent_project_toml(
        r#"
id = "bad id"
path = "/tmp/webcodex"
"#,
    )
    .unwrap_err();
    assert!(err.contains("ASCII letters"));
}

#[test]
fn agent_project_toml_hints_when_server_projects_format_is_used() {
    let err = parse_agent_project_toml(
        r#"
[projects.smoke]
path = "/root/webcodex-smoke"
"#,
    )
    .unwrap_err();
    assert!(err.contains("missing field"), "{err}");
    assert!(err.contains("server projects.toml"), "{err}");
    assert!(
        err.contains("Agent projects.d files must use top-level fields"),
        "{err}"
    );
    assert!(err.contains("id = \"smoke\""), "{err}");
    assert!(err.contains("path = \"/path/to/repo\""), "{err}");
}

#[test]
fn agent_project_toml_rejects_invalid_shell_profile() {
    let err = parse_agent_project_toml(
        r#"
id = "demo"
path = "/tmp/webcodex"
shell_profile = "../rust"
"#,
    )
    .unwrap_err();
    assert!(err.contains("project.shell_profile"), "{err}");
}

#[test]
fn missing_projects_dir_returns_empty_list() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("missing-projects.d");
    let projects = load_agent_project_summaries_from_dir(&missing);
    assert!(projects.is_empty());
}

#[test]
fn phase_e2_max_concurrent_jobs_normalizes_to_inventory_capacity() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    assert_eq!(DEFAULT_MAX_CONCURRENT_JOBS, 4);
    assert_eq!(max_concurrent_jobs(&cfg), DEFAULT_MAX_CONCURRENT_JOBS);

    cfg.max_concurrent_jobs = Some(0);
    assert_eq!(max_concurrent_jobs(&cfg), 1);

    cfg.max_concurrent_jobs = Some(1);
    assert_eq!(max_concurrent_jobs(&cfg), 1);

    cfg.max_concurrent_jobs = Some(4);
    assert_eq!(max_concurrent_jobs(&cfg), 4);

    cfg.max_concurrent_jobs = Some(8);
    assert_eq!(max_concurrent_jobs(&cfg), 8);

    cfg.max_concurrent_jobs = Some(64);
    assert_eq!(max_concurrent_jobs(&cfg), 64);

    cfg.max_concurrent_jobs = Some(65);
    assert_eq!(max_concurrent_jobs(&cfg), 64);

    cfg.max_concurrent_jobs = Some(128);
    assert_eq!(max_concurrent_jobs(&cfg), 64);

    cfg.max_concurrent_jobs = Some(usize::MAX);
    assert_eq!(max_concurrent_jobs(&cfg), 64);
}

#[test]
fn phase_e2_polling_dispatch_and_job_execution_concurrency_defaults_are_independent() {
    assert_eq!(POLLING_DISPATCH_MAX_IN_FLIGHT, 2);
    assert_eq!(DEFAULT_MAX_CONCURRENT_JOBS, 4);
    assert_ne!(POLLING_DISPATCH_MAX_IN_FLIGHT, DEFAULT_MAX_CONCURRENT_JOBS);
}

#[test]
fn agent_config_accepts_static_literal_mcp_gateway_provider() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"

[mcp]
request_timeout_secs = 7

[[mcp.providers]]
id = "local-tools"
name = "Local tools"
executable = "/usr/bin/example-mcp"
args = ["--stdio", "$HOME", "$(id)"]
timeout_secs = 5
"#,
    )
    .unwrap();

    let config = load_config(&path).unwrap();
    assert_eq!(config.mcp_gateway.request_timeout_secs, 7);
    assert_eq!(config.mcp_gateway.providers.len(), 1);
    assert_eq!(config.mcp_gateway.providers[0].timeout_secs, Some(5));
    assert_eq!(
        config.mcp_gateway.providers[0].args,
        ["--stdio", "$HOME", "$(id)"]
    );
}

#[test]
fn agent_config_mcp_gateway_provider_timeout_defaults_to_gateway_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"

[mcp]
request_timeout_secs = 11

[[mcp.providers]]
id = "inherits"
name = "Inherits"
executable = "/usr/bin/example-mcp"
"#,
    )
    .unwrap();

    let config = load_config(&path).unwrap();
    assert_eq!(config.mcp_gateway.request_timeout_secs, 11);
    assert_eq!(config.mcp_gateway.providers[0].timeout_secs, None);
}

#[test]
fn agent_config_rejects_unsafe_or_ambiguous_mcp_gateway_identity() {
    for (providers, expected) in [
        (
            r#"
[[mcp.providers]]
id = "local"
name = "Local"
executable = "relative-mcp"
"#,
            "executable must be an absolute path",
        ),
        (
            r#"
[[mcp.providers]]
id = "bad-timeout"
name = "Bad timeout"
executable = "/usr/bin/example-mcp"
timeout_secs = 121
"#,
            "timeout_secs must be between 1 and 120",
        ),
        (
            r#"
[[mcp.providers]]
id = "duplicate"
name = "First"
executable = "/usr/bin/first"

[[mcp.providers]]
id = "duplicate"
name = "Second"
executable = "/usr/bin/second"
"#,
            "provider ids must be unique",
        ),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            format!(
                r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"

[mcp]
request_timeout_secs = 30
{providers}
"#
            ),
        )
        .unwrap();
        let error = load_config(&path).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}
