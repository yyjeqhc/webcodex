use super::support::*;

/// Deterministic per-user environment so default path derivation is stable
/// and platform-independent. `XDG_*` wins over home on every platform in the
/// shared `webcodex_runner_config::paths` policy, so setting them pins the
/// derived paths without depending on the ambient HOME/USERPROFILE.
fn deterministic_user_env() -> EnvGuard {
    EnvGuard::new()
        .set("HOME", "/home/tester")
        .set("XDG_CONFIG_HOME", "/tmp/test-xdg")
        .set("XDG_STATE_HOME", "/tmp/test-state")
        .set("USERPROFILE", "C:\\Users\\tester")
        .set("APPDATA", "C:\\Users\\tester\\AppData\\Roaming")
        .set("LOCALAPPDATA", "C:\\Users\\tester\\AppData\\Local")
}

#[test]
fn client_output_dir_for_profile_uses_clients_subdir() {
    let base = PathBuf::from("/tmp/wc-base");
    assert_eq!(
        client_output_dir_for_profile(&base, "alice-laptop"),
        PathBuf::from("/tmp/wc-base/clients/alice-laptop")
    );
}

#[test]
fn runner_init_defaults_to_client_id_profile_paths() {
    let _guard = env_test_guard();
    let _env = deterministic_user_env();
    let opts = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://example.test",
        "--token",
        "agent_fake_token",
        "--client-id",
        "special-container",
        "--owner",
        "alice",
    ]))
    .unwrap();
    assert_eq!(
        opts.output,
        client_profile_runner_config("special-container").unwrap()
    );
    assert_eq!(
        opts.projects_dir,
        client_profile_projects_dir("special-container").unwrap()
    );
}

#[test]
fn runner_init_profile_overrides_client_id_profile_paths() {
    let _guard = env_test_guard();
    let _env = deterministic_user_env();
    let opts = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://example.test",
        "--token",
        "agent_fake_token",
        "--client-id",
        "special-container",
        "--profile",
        "special",
        "--owner",
        "alice",
    ]))
    .unwrap();
    assert_eq!(
        opts.output,
        client_profile_runner_config("special").unwrap()
    );
    assert_eq!(
        opts.projects_dir,
        client_profile_projects_dir("special").unwrap()
    );
}

#[test]
fn runner_init_explicit_output_and_projects_dir_win() {
    let opts = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://example.test",
        "--token",
        "agent_fake_token",
        "--client-id",
        "special-container",
        "--profile",
        "special",
        "--owner",
        "alice",
        "--output",
        "/tmp/a.toml",
        "--projects-dir",
        "/tmp/projects.d",
    ]))
    .unwrap();
    assert_eq!(opts.output, PathBuf::from("/tmp/a.toml"));
    assert_eq!(opts.projects_dir, PathBuf::from("/tmp/projects.d"));
}

#[cfg(unix)]
#[test]
fn user_scope_runner_config_keeps_legacy_only_and_rejects_dual_names() {
    let _guard = env_test_guard();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = tmp.path().to_str().unwrap();
    let _env = EnvGuard::new()
        .set("HOME", tmp_path)
        .set("XDG_CONFIG_HOME", tmp_path);
    let config_dir = tmp.path().join("webcodex");
    std::fs::create_dir_all(&config_dir).unwrap();

    assert_eq!(
        runner_config_for_scope(ServiceScope::User, None).unwrap(),
        config_dir.join("runner.toml")
    );
    std::fs::write(config_dir.join("agent.toml"), "legacy = true\n").unwrap();
    assert_eq!(
        runner_config_for_scope(ServiceScope::User, None).unwrap(),
        config_dir.join("agent.toml")
    );
    std::fs::write(config_dir.join("runner.toml"), "current = true\n").unwrap();
    let error = runner_config_for_scope(ServiceScope::User, None).unwrap_err();
    assert!(error.contains("runner.toml"));
    assert!(error.contains("agent.toml"));
    assert!(error.contains("refusing to guess"));
}

#[test]
fn runner_init_explicit_output_without_profile_preserves_legacy_projects_dir() {
    let opts = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://example.test",
        "--token",
        "agent_fake_token",
        "--client-id",
        "client id with spaces",
        "--owner",
        "alice",
        "--output",
        "/tmp/a.toml",
    ]))
    .unwrap();
    assert_eq!(opts.output, PathBuf::from("/tmp/a.toml"));
    assert_eq!(opts.projects_dir, PathBuf::from(DEFAULT_INIT_PROJECTS_DIR));
}

#[test]
fn runner_init_rejects_unsafe_profile() {
    let err = parse_cli_runner_init(&args(&[
        "--server-url",
        "https://example.test",
        "--token",
        "agent_fake_token",
        "--client-id",
        "special-container",
        "--profile",
        "../x",
        "--owner",
        "alice",
    ]))
    .unwrap_err();
    assert_eq!(err, CLIENT_PROFILE_ERROR);
}

/// Unix-only: derives systemd service paths, which require Unix
/// absolute-path semantics (`/etc/systemd/system/...`). On Windows the
/// Runner service feature fails closed instead.
#[cfg(unix)]
#[test]
fn runner_status_profile_derives_config_and_token_paths() {
    let opts = parse_runner_status(&args(&["--profile", "special", "--scope", "system"])).unwrap();
    assert_eq!(opts.scope, ServiceScope::System);
    assert_eq!(
        opts.config,
        runner_config_for_scope(ServiceScope::System, Some("special")).unwrap()
    );
    assert_eq!(
        opts.service_file,
        runner_service_file_for_scope(ServiceScope::System, Some("special")).unwrap()
    );
    assert_eq!(
        opts.user_token_file,
        Some(client_profile_user_token_file_for_scope(ServiceScope::System, "special").unwrap())
    );
    assert_eq!(
        opts.agent_token_file,
        Some(client_profile_agent_token_file_for_scope(ServiceScope::System, "special").unwrap())
    );
    assert!(opts.local_state_dir.is_none());
}

/// Unix-only: derives systemd service paths, which require Unix
/// absolute-path semantics (`/etc/systemd/system/...`). On Windows the
/// Runner service feature fails closed instead.
#[cfg(unix)]
#[test]
fn runner_status_explicit_paths_win_and_no_profile_uses_canonical_default() {
    let opts = parse_runner_status(&args(&[
        "--profile",
        "special",
        "--scope",
        "system",
        "--config",
        "/tmp/agent.toml",
        "--user-token-file",
        "/tmp/user-token",
        "--agent-token-file",
        "/tmp/agent-token",
    ]))
    .unwrap();
    assert_eq!(opts.config, PathBuf::from("/tmp/agent.toml"));
    assert_eq!(opts.user_token_file, Some(PathBuf::from("/tmp/user-token")));
    assert_eq!(
        opts.agent_token_file,
        Some(PathBuf::from("/tmp/agent-token"))
    );

    let default = parse_runner_status(&args(&["--scope", "system"])).unwrap();
    assert_eq!(default.config, PathBuf::from("/etc/webcodex/runner.toml"));
    assert_eq!(
        default.service_file,
        PathBuf::from("/etc/systemd/system/webcodex-runner.service")
    );
    assert_eq!(default.user_token_file, None);
    assert_eq!(default.agent_token_file, None);
}

/// Unix-only: derives systemd service paths, which require Unix
/// absolute-path semantics (`/etc/systemd/system/...`). On Windows the
/// Runner service feature fails closed instead.
#[cfg(unix)]
#[test]
fn runner_install_service_profile_derives_config_and_service_file() {
    let opts = parse_runner_install_service(&args(&[
        "--profile",
        "special",
        "--scope",
        "system",
        "--user",
        "webcodex",
        "--working-directory",
        "/srv/webcodex",
        "--bin",
        "/opt/webcodex/bin/webcodex-runner",
        "--dry-run",
    ]))
    .unwrap();
    assert_eq!(
        opts.config,
        runner_config_for_scope(ServiceScope::System, Some("special")).unwrap()
    );
    assert_eq!(
        opts.service_file,
        runner_service_file_for_scope(ServiceScope::System, Some("special")).unwrap()
    );
    let unit = render_runner_systemd_unit(&opts).unwrap();
    assert!(unit.contains(
        "ExecStart=\"/opt/webcodex/bin/webcodex-runner\" \"--config\" \"/etc/webcodex/clients/special/runner.toml\""
    ));
}

/// Unix-only: derives systemd service paths, which require Unix
/// absolute-path semantics (`/etc/systemd/system/...`). On Windows the
/// Runner service feature fails closed instead.
#[cfg(unix)]
#[test]
fn runner_install_service_explicit_paths_win_and_rejects_unsafe_profile() {
    let opts = parse_runner_install_service(&args(&[
        "--profile",
        "special",
        "--scope",
        "system",
        "--user",
        "webcodex",
        "--working-directory",
        "/srv/webcodex",
        "--config",
        "/tmp/agent.toml",
        "--service-file",
        "/tmp/webcodex-runner.service",
        "--bin",
        "/opt/webcodex/bin/webcodex-runner",
    ]))
    .unwrap();
    assert_eq!(opts.config, PathBuf::from("/tmp/agent.toml"));
    assert_eq!(
        opts.service_file,
        PathBuf::from("/tmp/webcodex-runner.service")
    );

    let err = parse_runner_install_service(&args(&[
        "--profile",
        "../x",
        "--scope",
        "system",
        "--user",
        "webcodex",
        "--working-directory",
        "/srv/webcodex",
        "--bin",
        "/opt/webcodex/bin/webcodex-runner",
    ]))
    .unwrap_err();
    assert_eq!(err, CLIENT_PROFILE_ERROR);
}

/// Unix-only: asserts the historical XDG/HOME directory semantics (`~/.config`
/// and `~/.config/systemd/user` layouts with Unix path spelling). Windows has
/// its own directory policy (APPDATA/LOCALAPPDATA) covered in
/// `webcodex_runner_config::paths` tests.
#[cfg(unix)]
#[test]
fn runner_service_scope_parsing_defaults_and_paths_are_deterministic() {
    let _guard = env_test_guard();
    let _env = EnvGuard::new()
        .set("HOME", "/home/alice")
        .set("XDG_CONFIG_HOME", "/tmp/alice-config");

    let user = parse_runner_install_service_with_identity(
        &args(&["--bin", "/opt/webcodex/bin/webcodex-runner", "--dry-run"]),
        false,
    )
    .unwrap();
    assert_eq!(user.scope, ServiceScope::User);
    assert_eq!(
        user.config,
        PathBuf::from("/tmp/alice-config/webcodex/runner.toml")
    );
    assert_eq!(
        user.service_file,
        PathBuf::from("/tmp/alice-config/systemd/user/webcodex-runner.service")
    );
    assert_eq!(user.working_directory, PathBuf::from("/home/alice"));
    assert!(!user.root_runner);

    let root_error = parse_runner_install_service_with_identity(
        &args(&["--bin", "/opt/webcodex/bin/webcodex-runner", "--dry-run"]),
        true,
    )
    .unwrap_err();
    assert!(root_error.contains("would run as root"), "{root_error}");

    let root = parse_runner_install_service_with_identity(
        &args(&[
            "--bin",
            "/opt/webcodex/bin/webcodex-runner",
            "--allow-root-runner",
            "--dry-run",
        ]),
        true,
    )
    .unwrap();
    assert_eq!(root.scope, ServiceScope::System);
    assert_eq!(root.config, PathBuf::from("/etc/webcodex/runner.toml"));
    assert_eq!(
        root.service_file,
        PathBuf::from("/etc/systemd/system/webcodex-runner.service")
    );
    assert!(root.root_runner);

    let root_user_error = parse_runner_install_service_with_identity(
        &args(&[
            "--scope",
            "user",
            "--bin",
            "/opt/webcodex/bin/webcodex-runner",
        ]),
        true,
    )
    .unwrap_err();
    assert!(
        root_user_error.contains("would run as root"),
        "{root_user_error}"
    );
}

/// Unix-only: asserts the historical `$HOME/.config` layout with Unix path
/// spelling.
#[cfg(unix)]
#[test]
fn user_scope_falls_back_to_home_and_profile_paths() {
    let _guard = env_test_guard();
    let _env = EnvGuard::new()
        .set("HOME", "/home/bob")
        .remove("XDG_CONFIG_HOME");

    let opts = parse_runner_install_service_with_identity(
        &args(&[
            "--scope",
            "user",
            "--profile",
            "work",
            "--bin",
            "/opt/webcodex/bin/webcodex-runner",
            "--dry-run",
        ]),
        false,
    )
    .unwrap();
    assert_eq!(
        opts.config,
        PathBuf::from("/home/bob/.config/webcodex/clients/work/runner.toml")
    );
    assert_eq!(
        opts.service_file,
        PathBuf::from("/home/bob/.config/systemd/user/webcodex-runner-work.service")
    );
}

/// Unix-only: derives systemd service paths, which require Unix
/// absolute-path semantics (`/etc/systemd/system/...`). On Windows the
/// Runner service feature fails closed instead.
#[cfg(unix)]
#[test]
fn runner_service_scope_rejects_invalid_and_conflicting_flags() {
    let bin = "/opt/webcodex/bin/webcodex-runner";
    let invalid = parse_runner_install_service_with_identity(
        &args(&["--scope", "session", "--bin", bin]),
        false,
    )
    .unwrap_err();
    assert_eq!(invalid, "--scope must be 'user' or 'system'");

    for flags in [
        vec!["--scope", "user", "--user", "alice"],
        vec!["--scope", "user", "--group", "alice"],
    ] {
        let mut values = flags;
        values.extend(["--bin", bin]);
        let error = parse_runner_install_service_with_identity(&args(&values), false).unwrap_err();
        assert!(error.contains("valid only with --scope system"), "{error}");
    }

    let system_root = parse_runner_install_service_with_identity(
        &args(&["--scope", "system", "--bin", bin]),
        false,
    )
    .unwrap_err();
    assert!(system_root.contains("would run as root"), "{system_root}");

    let unnecessary_opt_in = parse_runner_install_service_with_identity(
        &args(&[
            "--scope",
            "system",
            "--user",
            "alice",
            "--working-directory",
            "/home/alice",
            "--allow-root-runner",
            "--bin",
            bin,
        ]),
        false,
    )
    .unwrap_err();
    assert!(
        unnecessary_opt_in.contains("only valid"),
        "{unnecessary_opt_in}"
    );
}

/// Unix-only: derives systemd service paths, which require Unix
/// absolute-path semantics (`/etc/systemd/system/...`). On Windows the
/// Runner service feature fails closed instead.
#[cfg(unix)]
#[test]
fn explicit_service_paths_override_defaults_but_wrong_scope_paths_are_rejected() {
    let user = parse_runner_install_service_with_identity(
        &args(&[
            "--scope",
            "user",
            "--config",
            "/tmp/custom-agent.toml",
            "--service-file",
            "/tmp/webcodex-runner-custom.service",
            "--working-directory",
            "/tmp",
            "--bin",
            "/opt/webcodex/bin/webcodex-runner",
        ]),
        false,
    )
    .unwrap();
    assert_eq!(user.config, PathBuf::from("/tmp/custom-agent.toml"));
    assert_eq!(
        user.service_file,
        PathBuf::from("/tmp/webcodex-runner-custom.service")
    );

    let error = parse_runner_install_service_with_identity(
        &args(&[
            "--scope",
            "user",
            "--service-file",
            "/etc/systemd/system/webcodex-runner.service",
            "--bin",
            "/opt/webcodex/bin/webcodex-runner",
        ]),
        false,
    )
    .unwrap_err();
    assert!(error.contains("user scope cannot write"), "{error}");

    let error = parse_runner_install_service_with_identity(
        &args(&[
            "--scope",
            "user",
            "--service-file",
            "/etc/webcodex-runner.service",
            "--bin",
            "/opt/webcodex/bin/webcodex-runner",
        ]),
        false,
    )
    .unwrap_err();
    assert!(error.contains("user scope cannot write"), "{error}");

    let error = parse_runner_service_action(
        "start",
        &args(&[
            "--scope",
            "system",
            "--service-file",
            "/home/alice/.config/systemd/user/webcodex-runner.service",
        ]),
    )
    .unwrap_err();
    assert!(error.contains("system scope cannot write"), "{error}");

    let error = parse_runner_service_action(
        "start",
        &args(&[
            "--scope",
            "user",
            "--service-file",
            "/home/alice/.config/systemd/user/../../../../etc/systemd/system/webcodex-runner.service",
        ]),
    )
    .unwrap_err();
    assert!(error.contains("cannot contain '..'"), "{error}");
}

/// Unix-only: derives systemd service paths, which require Unix
/// absolute-path semantics (`/etc/systemd/system/...`). On Windows the
/// Runner service feature fails closed instead.
#[cfg(unix)]
#[test]
fn explicit_scope_selects_systemd_while_omitted_scope_preserves_hosted_profile() {
    let _guard = env_test_guard();
    let _env = deterministic_user_env();
    let implicit = parse_runner_service_action("start", &args(&["--profile", "hosted"])).unwrap();
    assert!(implicit.local_profile.is_some());

    let explicit = parse_runner_service_action(
        "start",
        &args(&["--profile", "hosted", "--scope", "system"]),
    )
    .unwrap();
    assert_eq!(explicit.scope, ServiceScope::System);
    assert!(explicit.local_profile.is_none());
}

#[cfg(unix)]
#[test]
fn hosted_profile_runner_bin_override_is_narrow_and_explicit() {
    let _guard = env_test_guard();
    let _env = deterministic_user_env();
    let bin = "/tmp/webcodex-dev-runner";

    let restart =
        parse_runner_service_action("restart", &args(&["--profile", "hosted", "--bin", bin]))
            .unwrap();
    assert_eq!(
        restart
            .local_profile
            .as_ref()
            .and_then(|local| local.runner_bin.as_deref()),
        Some(std::path::Path::new(bin))
    );

    for (command, values, expected) in [
        (
            "start",
            vec!["--profile", "hosted", "--scope", "user", "--bin", bin],
            "hosted connect profiles",
        ),
        ("restart", vec!["--bin", bin], "requires --profile"),
        (
            "start",
            vec!["--profile", "hosted", "--bin", bin],
            "valid only with `webcodex runner restart",
        ),
    ] {
        let error = parse_runner_service_action(command, &args(&values)).unwrap_err();
        assert!(error.contains(expected), "{command}: {error}");
    }
}

/// Unix-only: derives systemd service paths, which require Unix
/// absolute-path semantics (`/etc/systemd/system/...`). On Windows the
/// Runner service feature fails closed instead.
#[cfg(unix)]
#[test]
fn omitted_scope_hosted_status_keeps_xdg_profile_paths_for_root() {
    let _guard = env_test_guard();
    let _env = deterministic_user_env();

    let opts = parse_runner_status_with_identity(&args(&["--profile", "hosted"]), true).unwrap();
    assert_eq!(opts.scope, ServiceScope::System);
    assert_eq!(
        opts.config,
        PathBuf::from("/tmp/test-xdg/webcodex/clients/hosted/runner.toml")
    );
    assert_eq!(
        opts.user_token_file,
        Some(PathBuf::from(
            "/tmp/test-xdg/webcodex/clients/hosted/webcodex-user-token"
        ))
    );
    assert_eq!(
        opts.agent_token_file,
        Some(PathBuf::from(
            "/tmp/test-xdg/webcodex/clients/hosted/webcodex-runner-token"
        ))
    );
    assert!(opts.local_state_dir.is_some());
}

/// Unix-only: derives systemd service paths, which require Unix
/// absolute-path semantics (`/etc/systemd/system/...`). On Windows the
/// Runner service feature fails closed instead.
#[cfg(unix)]
#[test]
fn every_runner_service_action_accepts_scope_and_service_file() {
    let service_file = "/home/alice/.config/systemd/user/webcodex-runner-work.service";
    for command in ["start", "stop", "restart"] {
        let parsed = parse_runner_service_action(
            command,
            &args(&["--scope", "user", "--service-file", service_file]),
        )
        .unwrap();
        assert_eq!(parsed.scope, ServiceScope::User);
        assert_eq!(parsed.service_file, PathBuf::from(service_file));
        assert_eq!(parsed.unit, "webcodex-runner-work.service");
    }

    let logs = parse_runner_service_action(
        "logs",
        &args(&[
            "--scope",
            "user",
            "--service-file",
            service_file,
            "--lines",
            "25",
        ]),
    )
    .unwrap();
    assert!(matches!(
        logs.kind,
        ServiceActionKind::Logs { lines: 25, .. }
    ));

    let uninstall = parse_runner_service_action(
        "uninstall",
        &args(&[
            "--scope",
            "user",
            "--service-file",
            service_file,
            "--confirm",
        ]),
    )
    .unwrap();
    assert!(matches!(
        uninstall.kind,
        ServiceActionKind::Uninstall { confirm: true }
    ));
}
