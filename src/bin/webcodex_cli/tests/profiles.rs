use super::support::*;

#[test]
fn client_output_dir_for_profile_uses_clients_subdir() {
    let base = PathBuf::from("/tmp/wc-base");
    assert_eq!(
        client_output_dir_for_profile(&base, "alice-laptop"),
        PathBuf::from("/tmp/wc-base/clients/alice-laptop")
    );
}

#[test]
fn client_enroll_parse_defaults_to_client_id_profile() {
    let opts = parse_client_enroll(&args(&[
        "--server-url",
        "https://example.test",
        "--pairing-code",
        "wc_pair_fake",
        "--client-id",
        "alice-laptop",
    ]))
    .unwrap();
    let default_dir = default_client_output_dir_for_profile("alice-laptop");
    assert_eq!(opts.output_dir, default_dir);
    assert_eq!(opts.agent_config, opts.output_dir.join("agent.toml"));
    assert_eq!(opts.projects_dir, opts.output_dir.join("projects.d"));
    assert_eq!(opts.transport, TRANSPORT_WEBSOCKET);
    assert!(!opts.overwrite);
}

#[test]
fn client_enroll_parse_uses_explicit_profile_for_default_output_dir() {
    let opts = parse_client_enroll(&args(&[
        "--server-url",
        "https://example.test",
        "--pairing-code",
        "wc_pair_fake",
        "--client-id",
        "alice-laptop",
        "--profile",
        "special",
    ]))
    .unwrap();
    assert_eq!(
        opts.output_dir,
        default_client_output_dir_for_profile("special")
    );
    assert_eq!(opts.agent_config, opts.output_dir.join("agent.toml"));
    assert_eq!(opts.projects_dir, opts.output_dir.join("projects.d"));
}

#[test]
fn client_enroll_parse_output_dir_overrides_profile_default() {
    let opts = parse_client_enroll(&args(&[
        "--server-url",
        "https://example.test",
        "--pairing-code",
        "wc_pair_fake",
        "--client-id",
        "alice-laptop",
        "--profile",
        "special",
        "--output-dir",
        "/tmp/wc",
    ]))
    .unwrap();
    assert_eq!(opts.output_dir, PathBuf::from("/tmp/wc"));
    assert_eq!(opts.agent_config, PathBuf::from("/tmp/wc/agent.toml"));
    assert_eq!(opts.projects_dir, PathBuf::from("/tmp/wc/projects.d"));
}

#[test]
fn client_enroll_parse_output_dir_does_not_derive_profile_from_client_id() {
    let opts = parse_client_enroll(&args(&[
        "--server-url",
        "https://example.test",
        "--pairing-code",
        "wc_pair_fake",
        "--client-id",
        "alice laptop",
        "--output-dir",
        "/tmp/wc",
    ]))
    .unwrap();
    assert_eq!(opts.output_dir, PathBuf::from("/tmp/wc"));
    assert_eq!(opts.agent_config, PathBuf::from("/tmp/wc/agent.toml"));
    assert_eq!(opts.projects_dir, PathBuf::from("/tmp/wc/projects.d"));
}

#[test]
fn client_enroll_parse_agent_config_and_projects_dir_override_defaults() {
    let opts = parse_client_enroll(&args(&[
        "--server-url",
        "https://example.test",
        "--pairing-code",
        "wc_pair_fake",
        "--client-id",
        "alice-laptop",
        "--profile",
        "special",
        "--agent-config",
        "/tmp/custom-agent.toml",
        "--projects-dir",
        "/tmp/custom-projects.d",
    ]))
    .unwrap();
    assert_eq!(
        opts.output_dir,
        default_client_output_dir_for_profile("special")
    );
    assert_eq!(opts.agent_config, PathBuf::from("/tmp/custom-agent.toml"));
    assert_eq!(opts.projects_dir, PathBuf::from("/tmp/custom-projects.d"));
}

#[test]
fn client_enroll_rejects_unsafe_profiles() {
    for profile in [
        "",
        "   ",
        ".",
        "..",
        "../x",
        "a/b",
        r"a\b",
        "has space",
        "ümlaut",
    ] {
        let err = parse_client_enroll(&args(&[
            "--server-url",
            "https://example.test",
            "--pairing-code",
            "wc_pair_fake",
            "--client-id",
            "alice-laptop",
            "--profile",
            profile,
        ]))
        .unwrap_err();
        assert_eq!(err, CLIENT_PROFILE_ERROR);
    }
}

#[test]
fn client_enroll_rejects_unsafe_default_client_id_profile() {
    let err = parse_client_enroll(&args(&[
        "--server-url",
        "https://example.test",
        "--pairing-code",
        "wc_pair_fake",
        "--client-id",
        "alice laptop",
    ]))
    .unwrap_err();
    assert_eq!(err, CLIENT_PROFILE_ERROR);
}

#[test]
fn agent_init_defaults_to_client_id_profile_paths() {
    let opts = parse_cli_agent_init(&args(&[
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
        client_profile_agent_config("special-container")
    );
    assert_eq!(
        opts.projects_dir,
        client_profile_projects_dir("special-container")
    );
}

#[test]
fn agent_init_profile_overrides_client_id_profile_paths() {
    let opts = parse_cli_agent_init(&args(&[
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
    assert_eq!(opts.output, client_profile_agent_config("special"));
    assert_eq!(opts.projects_dir, client_profile_projects_dir("special"));
}

#[test]
fn agent_init_explicit_output_and_projects_dir_win() {
    let opts = parse_cli_agent_init(&args(&[
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

#[test]
fn agent_init_explicit_output_without_profile_preserves_legacy_projects_dir() {
    let opts = parse_cli_agent_init(&args(&[
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
fn agent_init_rejects_unsafe_profile() {
    let err = parse_cli_agent_init(&args(&[
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

#[test]
fn agent_status_profile_derives_config_and_token_paths() {
    let opts = parse_agent_status(&args(&["--profile", "special"])).unwrap();
    assert_eq!(opts.config, client_profile_agent_config("special"));
    assert_eq!(
        opts.user_token_file,
        Some(client_profile_user_token_file("special"))
    );
    assert_eq!(
        opts.agent_token_file,
        Some(client_profile_agent_token_file("special"))
    );
}

#[test]
fn agent_status_explicit_paths_win_and_no_profile_keeps_legacy_default() {
    let opts = parse_agent_status(&args(&[
        "--profile",
        "special",
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

    let legacy = parse_agent_status(&args(&[])).unwrap();
    assert_eq!(legacy.config, PathBuf::from("/etc/webcodex/agent.toml"));
    assert_eq!(legacy.user_token_file, None);
    assert_eq!(legacy.agent_token_file, None);
}

#[test]
fn agent_install_service_profile_derives_config_and_service_file() {
    let opts = parse_agent_install_service(&args(&[
        "--profile",
        "special",
        "--bin",
        "/opt/webcodex/bin/webcodex-agent",
        "--dry-run",
    ]))
    .unwrap();
    assert_eq!(opts.config, client_profile_agent_config("special"));
    assert_eq!(opts.service_file, client_profile_service_file("special"));
    let unit = render_agent_systemd_unit(&opts);
    assert!(unit.contains(
        "ExecStart=/opt/webcodex/bin/webcodex-agent --config /etc/webcodex/clients/special/agent.toml"
    ));
}

#[test]
fn agent_install_service_explicit_paths_win_and_rejects_unsafe_profile() {
    let opts = parse_agent_install_service(&args(&[
        "--profile",
        "special",
        "--config",
        "/tmp/agent.toml",
        "--service-file",
        "/tmp/webcodex-agent.service",
        "--bin",
        "/opt/webcodex/bin/webcodex-agent",
    ]))
    .unwrap();
    assert_eq!(opts.config, PathBuf::from("/tmp/agent.toml"));
    assert_eq!(
        opts.service_file,
        PathBuf::from("/tmp/webcodex-agent.service")
    );

    let err = parse_agent_install_service(&args(&[
        "--profile",
        "../x",
        "--bin",
        "/opt/webcodex/bin/webcodex-agent",
    ]))
    .unwrap_err();
    assert_eq!(err, CLIENT_PROFILE_ERROR);
}

#[test]
fn doctor_profile_derives_agent_config_and_token_paths() {
    let opts = parse_doctor(&args(&["--profile", "special"])).unwrap();
    assert_eq!(
        opts.agent_config,
        Some(client_profile_agent_config("special"))
    );
    assert_eq!(
        opts.user_token_file,
        Some(client_profile_user_token_file("special"))
    );
    assert_eq!(
        opts.agent_token_file,
        Some(client_profile_agent_token_file("special"))
    );
}

#[test]
fn doctor_explicit_paths_win_and_no_profile_keeps_legacy_behavior() {
    let opts = parse_doctor(&args(&[
        "--profile",
        "special",
        "--agent-config",
        "/tmp/agent.toml",
        "--user-token-file",
        "/tmp/user-token",
        "--agent-token-file",
        "/tmp/agent-token",
    ]))
    .unwrap();
    assert_eq!(opts.agent_config, Some(PathBuf::from("/tmp/agent.toml")));
    assert_eq!(opts.user_token_file, Some(PathBuf::from("/tmp/user-token")));
    assert_eq!(
        opts.agent_token_file,
        Some(PathBuf::from("/tmp/agent-token"))
    );

    let legacy = parse_doctor(&args(&[])).unwrap();
    assert_eq!(legacy.agent_config, None);
    assert_eq!(legacy.user_token_file, None);
    assert_eq!(legacy.agent_token_file, None);
}

#[test]
fn doctor_rejects_unsafe_profile() {
    let err = parse_doctor(&args(&["--profile", "a/b"])).unwrap_err();
    assert_eq!(err, CLIENT_PROFILE_ERROR);
}
