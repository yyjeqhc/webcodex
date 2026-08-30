use super::support::*;

#[test]
fn bare_interactive_repo_defaults_to_share_only_on_supported_desktop_unix() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let nested = repo.join("src/nested");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir_all(&nested).unwrap();

    assert_eq!(
        first_run_default_args(Vec::new(), Some(&nested), "linux", true),
        vec!["share".to_string()]
    );
    assert_eq!(
        first_run_default_args(Vec::new(), Some(&nested), "macos", true),
        vec!["share".to_string()]
    );
    assert!(first_run_default_args(Vec::new(), Some(&nested), "windows", true).is_empty());
    assert!(first_run_default_args(Vec::new(), Some(&nested), "linux", false).is_empty());
    assert!(first_run_default_args(Vec::new(), Some(temp.path()), "linux", true).is_empty());

    let explicit = vec!["--help".to_string()];
    assert_eq!(
        first_run_default_args(explicit.clone(), Some(&nested), "linux", true),
        explicit
    );
}

#[test]
fn bare_interactive_linked_worktree_marker_also_defaults_to_share() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("worktree");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join(".git"), "gitdir: /tmp/example\n").unwrap();
    assert_eq!(
        first_run_default_args(Vec::new(), Some(&repo), "linux", true),
        vec!["share".to_string()]
    );
}

#[test]
fn cli_help_and_version_exit_before_dispatch() {
    match cli_action(["--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("Usage: webcodex"));
        }
        other => panic!("expected help exit, got {other:?}"),
    }
    match cli_action(["--version"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.starts_with(&format!("webcodex {} (commit ", env!("CARGO_PKG_VERSION"))));
            assert!(stdout.trim_end().ends_with(')'));
            assert_ne!(stdout, format!("webcodex {}\n", env!("CARGO_PKG_VERSION")));
        }
        other => panic!("expected version exit, got {other:?}"),
    }
}

#[test]
fn cli_version_output_includes_build_metadata() {
    match cli_action(["-V"]) {
        CliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("commit "));
            assert!(stdout.starts_with("webcodex "));
            assert!(stderr.is_empty());
        }
        other => panic!("expected version exit, got {other:?}"),
    }
}

#[test]
fn project_doctor_and_hosted_connect_dispatch() {
    assert!(matches!(
        cli_action(["project", "register", "--config", "/tmp/agent.toml", "/tmp/repo"]),
        CliAction::ProjectRegister(opts)
            if opts.config == std::path::PathBuf::from("/tmp/agent.toml")
                && opts.project == std::path::PathBuf::from("/tmp/repo")
                && !opts.json
    ));
    assert!(matches!(
        cli_action(["doctor"]),
        CliAction::Project(args) if args == ["doctor"]
    ));
    assert!(matches!(
        cli_action(["share", "--tunnel", "none"]),
        CliAction::Project(args) if args == ["share", "--tunnel", "none"]
    ));
    assert!(matches!(
        cli_action(["connect", "https://example.test", "--key", "shared-secret"]),
        CliAction::Connect(_)
    ));
}

#[test]
fn webcodex_cli_help_mentions_management_commands() {
    match cli_action(["--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("pairing create"));
            assert!(stdout.contains("Full daily setup:"), "{stdout}");
            assert!(
                stdout.matches("service lifecycle is Linux-only").count() >= 2,
                "{stdout}"
            );
            assert!(stdout.contains("Quick trial:"), "{stdout}");
            assert!(
                stdout.contains("Temporarily share one project; ends when the command exits"),
                "{stdout}"
            );
            assert!(stdout.contains("client enroll"));
            // The token actions are now listed once per group rather than one
            // line per action, but every action must still appear.
            for action in [
                "create-local",
                "generate",
                "register-hash",
                "list",
                "revoke",
            ] {
                assert!(
                    stdout.contains(action),
                    "help no longer mentions token action {action}"
                );
            }
            assert!(stdout.contains("tokens create|"));
            assert!(stdout.contains("agent-tokens create|"));
            assert!(
                stdout.contains("runner init|install|run|start|stop|restart|status|logs|uninstall")
            );
        }
        other => panic!("expected help exit, got {other:?}"),
    }
}

#[test]
fn project_register_and_login_project_help_prioritize_user_language() {
    let project_help = cli_exit(["project", "register", "--help"]).unwrap();
    assert!(project_help.contains("Add one existing project to a Runner configuration"));
    assert!(project_help.contains("Advanced: projects_dir"));
    assert!(project_help.contains("allowed_roots"));
    let login_help = cli_exit(["login", "--help"]).unwrap();
    assert!(login_help.contains("one-time login code"));
    assert!(login_help.contains("--project PATH"));
    assert!(login_help.contains("projects may be added later"));

    assert!(matches!(
        cli_action([
            "login",
            "https://example.test",
            "--code",
            "wc_pair_example",
            "--allowed-root",
            "/tmp",
            "--project",
            "/tmp/repo",
        ]),
        CliAction::Login(opts)
            if opts.allowed_roots == vec![std::path::PathBuf::from("/tmp")]
                && opts.project == Some(std::path::PathBuf::from("/tmp/repo"))
    ));
}

#[test]
fn foreground_run_banner_describes_lifetime_without_false_readiness() {
    let server = foreground_run_banner("Server");
    assert!(server.contains("Starting WebCodex Server in the foreground."));
    assert!(server.contains("Keep this terminal open."));
    assert!(server.contains("Ctrl-C stops the Server."));
    assert!(!server.contains("Server is running"));

    let runner = foreground_run_banner("Runner");
    assert!(runner.contains("Starting WebCodex Runner in the foreground."));
    assert!(runner.contains("Ctrl-C stops the Runner."));
    assert!(!runner.contains("Runner connected"));
}

#[test]
fn common_help_entrypoints_smoke() {
    let cases: &[(&[&str], &[&str])] = &[
        (
            &["--help"],
            &[
                "Usage: webcodex [COMMAND]",
                "Full daily setup:",
                "share",
                "Quick trial:",
                "connect",
                "server init|install|run|start|stop|restart|status|logs|uninstall",
                "setup single-user",
            ],
        ),
        (
            &["server", "--help"],
            &[
                "Usage: webcodex server <COMMAND>",
                "Commands:",
                "init",
                "install",
                "run",
                "uninstall",
            ],
        ),
        (
            &["runner", "--help"],
            &[
                "Usage: webcodex runner <COMMAND>",
                "Linux systemd Runner service",
                "foreground (all supported platforms)",
                "Linux systemd service commands",
            ],
        ),
    ];

    for (args, expected) in cases {
        let out = cli_exit(args.iter().copied())
            .unwrap_or_else(|err| panic!("expected {args:?} help to exit successfully: {err}"));
        for needle in *expected {
            assert!(
                out.contains(needle),
                "help for {args:?} did not contain {needle:?}\n{out}"
            );
        }
    }
}

#[test]
fn top_level_help_prioritizes_first_run_without_hiding_advanced_commands() {
    let out = cli_exit(["--help"]).unwrap();
    let full = out.find("Full daily setup:").unwrap();
    let server = out.find("server init|install|run").unwrap();
    let pairing = out.find("pairing create").unwrap();
    let login = out.find("\nlogin").unwrap();
    let quick = out.find("Quick trial:").unwrap();
    let share = out.find("\nshare").unwrap();
    let advanced = out.find("Operator / advanced:").unwrap();
    assert!(full < server && server < pairing && pairing < login);
    assert!(login < quick && quick < share && share < advanced);
    assert!(out.contains("(no command)"));
    assert!(out.contains("Interactive Git repo shortcut for `share` on Linux/macOS"));
    assert!(out.contains("Temporarily share one project; ends when the command exits"));
    assert!(out.contains("agent-tokens create|"));
}

#[test]
fn unified_project_and_auth_commands_dispatch() {
    for args in [
        &["setup", "--help"][..],
        &["status", "--help"][..],
        &["doctor", "--help"][..],
        &["task", "--help"][..],
        &["run", "--help"][..],
        &["share", "--help"][..],
    ] {
        assert!(matches!(
            cli_action(args.iter().copied()),
            CliAction::Project(_)
        ));
    }
    match cli_action(["auth", "status", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("Usage: webcodex auth status"));
        }
        other => panic!("expected auth status help, got {other:?}"),
    }
}

#[test]
fn webcodex_cli_runner_help_mentions_lifecycle_subcommands() {
    match cli_action(["runner", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            for command in [
                "init",
                "install",
                "run",
                "start",
                "stop",
                "restart",
                "status",
                "logs",
                "uninstall",
            ] {
                assert!(
                    stdout.contains(command),
                    "runner help missing {command}: {stdout}"
                );
            }
            assert!(stdout.contains("webcodex run"));
        }
        other => panic!("expected help exit, got {other:?}"),
    }
    match cli_action(["runner", "init", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("Usage: webcodex runner init"));
            assert!(stdout.contains("Stable Runner client id"));
            assert!(stdout.contains("Human-readable Runner name"));
            assert!(stdout.contains("agent.toml"));
        }
        other => panic!("expected Runner init help exit, got {other:?}"),
    }
    match cli_action(["runner", "install", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("--config PATH"));
            assert!(stdout.contains("--bin PATH"));
            assert!(stdout.contains("--scope user|system"));
            assert!(stdout.contains("--allow-root-runner"));
            assert!(stdout.contains("default: user for non-root"));
            assert!(stdout.contains("Tokens are never inlined"));
        }
        other => panic!("expected help exit, got {other:?}"),
    }
    match cli_action(["runner", "restart", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("--bin PATH"));
            assert!(stdout.contains("hosted profile"));
            assert!(stdout.contains("does not accept --bin"));
        }
        other => panic!("expected help exit, got {other:?}"),
    }
    match cli_action(["runner", "status", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("--user-token-file PATH"));
            assert!(stdout.contains("--agent-token-file PATH"));
            assert!(stdout.contains("--scope user|system"));
            assert!(stdout.contains("--service-file PATH"));
            assert!(stdout.contains("Runner config path"));
            assert!(stdout.contains("agent.toml"));
            assert!(stdout.contains("no tokens"));
        }
        other => panic!("expected help exit, got {other:?}"),
    }
}

#[test]
fn runner_namespace_owns_all_lifecycle_commands_and_agent_is_unknown() {
    let top_help = cli_exit(["--help"]).unwrap();
    assert!(top_help.contains("runner init|install|run|start|stop|restart|status|logs|uninstall"));
    assert!(!top_help.contains("  agent init|install|run|start|stop|restart|status|logs|uninstall"));

    for command in [
        "init",
        "install",
        "run",
        "start",
        "stop",
        "restart",
        "status",
        "logs",
        "uninstall",
    ] {
        match cli_action(["runner", command, "--help"]) {
            CliAction::Exit { code, stdout, .. } => {
                assert_eq!(code, 0, "runner {command} help must dispatch");
                assert!(
                    stdout.contains("webcodex runner"),
                    "runner {command} help did not stay in Runner namespace: {stdout}"
                );
            }
            other => panic!("runner {command} did not dispatch to lifecycle help: {other:?}"),
        }

        match cli_action(["agent", command]) {
            CliAction::Exit {
                code,
                stdout,
                stderr,
            } => {
                assert_eq!(code, 2);
                assert!(stdout.is_empty());
                assert!(
                    stderr.starts_with("unknown command: agent\n"),
                    "legacy agent namespace was still recognized for {command}: {stderr}"
                );
            }
            other => panic!("legacy agent {command} produced a lifecycle action: {other:?}"),
        }
    }
}

#[test]
fn client_enroll_help_documents_profile_and_output_dir_precedence() {
    let help = client_enroll_usage();
    assert!(help.contains("--profile NAME"));
    assert!(help.contains("/etc/webcodex/clients/<profile>"));
    assert!(help.contains("~/.config/webcodex/clients/<profile>"));
    assert!(help.contains("Explicit --output-dir overrides"));
}

#[test]
fn singular_and_plural_token_groups_dispatch_identically() {
    // `tokens create-local` used to reach the admin parser, which has no such
    // action, so it failed with "unknown admin command" while the documented
    // `token create-local` worked. Both spellings now take the same path.
    for group in ["token", "tokens", "agent-token", "agent-tokens"] {
        match cli_action([group, "create-local", "--help"]) {
            CliAction::Exit { stdout, stderr, .. } => {
                let text = format!("{stdout}{stderr}");
                assert!(
                    text.contains("create-local"),
                    "{group} create-local was not recognized: {text}"
                );
                assert!(
                    !text.contains("unknown admin command"),
                    "{group} create-local still falls through to the admin parser: {text}"
                );
            }
            other => panic!("expected an exit for {group}, got {other:?}"),
        }
    }
}

#[test]
fn admin_token_actions_still_reach_the_admin_parser_under_both_spellings() {
    for group in ["token", "tokens"] {
        match cli_action([
            group,
            "list",
            "--server-url",
            "https://example.test",
            "--username",
            "alice",
        ]) {
            CliAction::Admin(_) => {}
            other => panic!("expected admin dispatch for {group} list, got {other:?}"),
        }
    }
}

#[test]
fn usage_lists_one_canonical_spelling_per_group() {
    // The old help text listed `user/users`, `token`, and `tokens` as separate
    // commands, which is what made the surface look twice its real size.
    match cli_action(["--help"]) {
        CliAction::Exit { stdout, .. } => {
            for canonical in [
                "users create|list",
                "tokens create|",
                "agent-tokens create|",
            ] {
                assert!(
                    stdout.contains(canonical),
                    "help is missing {canonical}: {stdout}"
                );
            }
            assert!(
                !stdout.contains("user/users"),
                "help still advertises both spellings: {stdout}"
            );
        }
        other => panic!("expected help exit, got {other:?}"),
    }
}

#[test]
fn help_moves_client_enroll_to_advanced_and_keeps_login_as_the_primary_entry() {
    match cli_action(["--help"]) {
        CliAction::Exit { stdout, .. } => {
            assert!(stdout.contains("login"), "help missing login: {stdout}");
            assert!(
                stdout.contains("client enroll"),
                "help missing client enroll: {stdout}"
            );
            assert!(
                stdout.contains("Operator / advanced"),
                "help missing advanced section: {stdout}"
            );
            // client enroll must sit under the advanced section, after it.
            let advanced = stdout.find("Operator / advanced").unwrap();
            let enroll = stdout.find("client enroll").unwrap();
            assert!(
                enroll > advanced,
                "client enroll is not under Advanced: {stdout}"
            );
        }
        other => panic!("expected help exit, got {other:?}"),
    }
}

#[test]
fn pairing_create_help_marks_client_id_optional_and_explains_matching_device() {
    match cli_action(["pairing", "create", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(
                stdout.contains("[--client-id CLIENT_ID]"),
                "optional client id is missing from synopsis: {stdout}"
            );
            assert!(stdout.contains("let the login device claim it"), "{stdout}");
            assert!(stdout.contains("same --device value"), "{stdout}");
        }
        other => panic!("expected help exit, got {other:?}"),
    }
}

#[test]
fn login_help_describes_root_and_non_root_default_directories() {
    match cli_action(["login", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("root /etc/webcodex"), "{stdout}");
            assert!(stdout.contains("non-root ~/.config/webcodex"), "{stdout}");
        }
        other => panic!("expected help exit, got {other:?}"),
    }
}

#[test]
fn login_print_mcp_config_and_json_are_mutually_exclusive() {
    // `--json --print-mcp-config` together is a parse-time error.
    match cli_action([
        "login",
        "https://example.com",
        "--code",
        "wc_pair_x",
        "--json",
        "--print-mcp-config",
    ]) {
        CliAction::Exit { code, stderr, .. } => {
            assert_eq!(code, 2);
            assert!(
                stderr.contains("mutually exclusive"),
                "expected mutual-exclusion error, got {stderr}"
            );
        }
        other => panic!("expected a parse error, got {other:?}"),
    }
    // `--print-mcp-config` alone dispatches to Login with the flag set.
    match cli_action([
        "login",
        "https://example.com",
        "--code",
        "wc_pair_x",
        "--print-mcp-config",
    ]) {
        CliAction::Login(opts) => {
            assert!(opts.print_mcp_config);
            assert!(!opts.json);
        }
        other => panic!("expected Login dispatch, got {other:?}"),
    }
}
