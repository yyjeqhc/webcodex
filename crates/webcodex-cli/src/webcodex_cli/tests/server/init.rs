use super::super::support::*;

#[test]
fn runner_init_writes_valid_toml_and_refuses_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let output = tmp.path().join("runner.toml");
    let opts = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://v4.example.test/",
        "--token",
        "agent_fake_test_token",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--display-name",
        "Alice Laptop",
        "--allowed-root",
        "/srv/projects",
        "--output",
        output.to_str().unwrap(),
    ]))
    .unwrap();
    let msg = run_runner_init(opts).unwrap();
    assert!(msg.contains("runner.toml"));

    // Refuse overwrite without --overwrite.
    let opts2 = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://v4.example.test/",
        "--token",
        "agent_fake_test_token",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        output.to_str().unwrap(),
    ]))
    .unwrap();
    let err = run_runner_init(opts2).unwrap_err();
    assert!(err.contains("already exists"));
}

#[test]
fn runner_init_stdout_output_contains_token_only_once() {
    let opts = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--token",
        "agent_fake_stdout_token",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        "-",
    ]))
    .unwrap();
    let content = run_runner_init(opts).unwrap();
    assert_eq!(content.matches("agent_fake_stdout_token").count(), 1);
}

#[cfg(unix)]
#[test]
fn runner_init_writes_0600_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let output = tmp.path().join("runner.toml");
    let opts = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--token",
        "agent_fake_perms_token",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        output.to_str().unwrap(),
    ]))
    .unwrap();
    run_runner_init(opts).unwrap();
    let mode = std::fs::metadata(&output).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn runner_init_token_file_and_env_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let token_file = tmp.path().join("agent.token");
    std::fs::write(&token_file, "agent_fake_file_token\n").unwrap();
    let opts = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--token-file",
        token_file.to_str().unwrap(),
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        "-",
    ]))
    .unwrap();
    let content = run_runner_init(opts).unwrap();
    assert!(content.contains("agent_fake_file_token"));

    let _guard = env_test_guard();
    let _env = EnvGuard::new().set("WEBCODEX_AGENT_TOKEN", "agent_fake_env_token");
    let opts = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        "-",
    ]))
    .unwrap();
    let content = run_runner_init(opts).unwrap();
    assert!(content.contains("agent_fake_env_token"));
}

#[test]
fn runner_init_empty_tokens_are_rejected() {
    let opts = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--token",
        "   ",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        "-",
    ]))
    .unwrap();
    let err = run_runner_init(opts).unwrap_err();
    assert!(err.contains("--token cannot be empty"), "{err}");
}

/// Unix-only: asserts the historical `$HOME` default for empty allowed_roots.
/// Windows uses `USERPROFILE` instead (see `webcodex_runner_config::paths`).
#[cfg(unix)]
#[test]
fn runner_init_allows_empty_allowed_roots_with_home_default() {
    let _guard = env_test_guard();
    let home = std::env::var_os("HOME");
    if home.is_some() {
        let opts = parse_cli_runner_init(&args(&[
            "--server-url",
            "https://v4.example.test",
            "--token",
            "agent_fake_home_token",
            "--client-id",
            "alice-laptop",
            "--owner",
            "alice",
            "--output",
            "-",
        ]))
        .unwrap();
        let content = run_runner_init(opts).unwrap();
        let home = std::env::var_os("HOME").unwrap();
        assert!(content.contains(&home.to_string_lossy().to_string()));
    }
}

#[test]
fn server_init_parse_defaults() {
    let opts = parse_server_init(&args(&[])).unwrap();
    assert_eq!(opts.listen, "127.0.0.1:8080");
    if is_effective_root() {
        assert_eq!(opts.data_dir, PathBuf::from("/var/lib/webcodex"));
        assert_eq!(opts.env_file, PathBuf::from("/etc/webcodex/webcodex.env"));
    } else {
        assert!(opts.data_dir.ends_with(".local/share/webcodex"));
        assert!(opts.env_file.ends_with(".config/webcodex/webcodex.env"));
    }
    assert!(!opts.overwrite);
    assert!(!opts.json);
}

#[test]
fn server_init_writes_env_file_and_0600_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("etc/webcodex.env");
    let data_dir = tmp.path().join("data");
    let opts = parse_server_init(&args(&[
        "--listen",
        "127.0.0.1:9090",
        "--data-dir",
        data_dir.to_str().unwrap(),
        "--env-file",
        env_file.to_str().unwrap(),
        "--public-url",
        "https://example.test/",
    ]))
    .unwrap();
    let output = run_server_init(opts).unwrap();
    assert!(data_dir.is_dir(), "server init must create WEBCODEX_DATA");
    assert!(output.contains("WebCodex Server configured."), "{output}");
    assert!(output.contains("Data:"), "{output}");
    assert!(output.contains("Next:"), "{output}");
    let foreground = crate::webcodex_cli::shell_command(&[
        "webcodex".to_string(),
        "server".to_string(),
        "run".to_string(),
        "--env-file".to_string(),
        env_file.to_string_lossy().into_owned(),
    ]);
    assert!(output.contains(&foreground), "{output}");
    if cfg!(target_os = "linux") && is_effective_root() {
        let install = crate::webcodex_cli::shell_command(&[
            "webcodex".to_string(),
            "server".to_string(),
            "install".to_string(),
            "--env-file".to_string(),
            env_file.to_string_lossy().into_owned(),
            "--working-directory".to_string(),
            data_dir.to_string_lossy().into_owned(),
        ]);
        assert!(output.contains(&install), "{output}");
    } else {
        assert!(!output.contains("webcodex server install"), "{output}");
    }
    let content = std::fs::read_to_string(&env_file).unwrap();
    assert!(content.contains("WEBCODEX_ADDR=127.0.0.1:9090\n"));
    assert!(content.contains(&format!("WEBCODEX_DATA={}\n", data_dir.display())));
    assert!(content.contains("WEBCODEX_TOKEN=wc_boot_"));
    assert!(content.contains("WEBCODEX_PUBLIC_URL=https://example.test\n"));
    assert!(content.contains("WEBCODEX_OAUTH2_ENABLED=true\n"));
    assert!(content.contains("WEBCODEX_OAUTH2_ISSUER=https://example.test\n"));
    assert!(content.contains("WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE=true\n"));
    assert!(content.contains("WEBCODEX_SHARED_KEY_ENABLED=true\n"));
    let token = parse_env_content_value(&content, "WEBCODEX_TOKEN").unwrap();
    assert!(!output.contains(&token));
    assert!(!output.contains("token prefix:"), "{output}");
    assert!(!output.contains("shared key:"), "{output}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&env_file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn server_init_refuses_overwrite_unless_requested() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    std::fs::write(&env_file, "WEBCODEX_TOKEN=old\n").unwrap();
    let mut opts = parse_server_init(&args(&[
        "--env-file",
        env_file.to_str().unwrap(),
        "--data-dir",
        tmp.path().to_str().unwrap(),
    ]))
    .unwrap();
    let err = run_server_init(opts.clone()).unwrap_err();
    assert!(err.contains("already exists"));
    opts.overwrite = true;
    run_server_init(opts).unwrap();
    let content = std::fs::read_to_string(&env_file).unwrap();
    assert!(content.contains("WEBCODEX_ADDR="));
    assert!(content.contains("WEBCODEX_TOKEN=old"));
    assert!(content.contains("WEBCODEX_SHARED_KEY_ENABLED=true"));
    assert!(!content.contains("WEBCODEX_OAUTH2_ENABLED=true"));
    assert!(!content.contains("WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE=true"));
}

#[test]
fn server_init_json_output_does_not_include_full_token() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    let opts = parse_server_init(&args(&[
        "--env-file",
        env_file.to_str().unwrap(),
        "--data-dir",
        tmp.path().to_str().unwrap(),
        "--json",
    ]))
    .unwrap();
    let output = run_server_init(opts).unwrap();
    let content = std::fs::read_to_string(&env_file).unwrap();
    let token = parse_env_content_value(&content, "WEBCODEX_TOKEN").unwrap();
    assert!(!output.contains(&token));
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["wrote_env_file"], true);
    assert!(json["token_prefix"]
        .as_str()
        .unwrap()
        .starts_with("wc_boot"));
    assert!(json.get("token").is_none());
}

#[cfg(windows)]
#[test]
fn server_init_secret_env_file_has_protected_windows_dacl() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("config/webcodex.env");
    let data_dir = tmp.path().join("data");
    let opts = parse_server_init(&args(&[
        "--listen",
        "127.0.0.1:0",
        "--env-file",
        env_file.to_str().unwrap(),
        "--data-dir",
        data_dir.to_str().unwrap(),
        "--json",
    ]))
    .unwrap();
    let output = run_server_init(opts).unwrap();
    let content = std::fs::read_to_string(&env_file).unwrap();
    let token = parse_env_content_value(&content, "WEBCODEX_TOKEN").unwrap();
    assert!(!output.contains(&token));
    let json: Value = serde_json::from_str(&output).unwrap();
    let next_steps = json["next_steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(next_steps.contains("server run --env-file"), "{next_steps}");
    assert!(!next_steps.contains("server install"), "{next_steps}");

    let sddl = crate::webcodex_cli::system::windows_dacl_sddl(&env_file).unwrap();
    assert!(sddl.starts_with("D:P"), "DACL must be protected: {sddl}");
    assert!(sddl.contains(";;;SY)"), "SYSTEM access must remain: {sddl}");
    assert_eq!(
        sddl.matches("(A;").count(),
        2,
        "only the current user and SYSTEM should have allow ACEs: {sddl}"
    );
    assert!(
        !sddl.contains(";;;WD)"),
        "Everyone must not retain read access: {sddl}"
    );
    assert!(
        !sddl.contains(";;;BU)"),
        "Builtin Users must not retain read access: {sddl}"
    );
    assert!(
        !sddl.contains(";;;BA)"),
        "Builtin Administrators must not receive a direct allow ACE: {sddl}"
    );
}

#[test]
fn server_init_rejects_legacy_full_token_stdout_mode() {
    let result = parse_server_init(&args(&["--output", "-"]));
    assert_eq!(result.unwrap_err(), "unknown server init flag: --output");
    assert!(!server_init_usage().contains("full WEBCODEX_TOKEN"));
}
