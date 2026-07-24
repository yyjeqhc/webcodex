use super::support::*;

#[test]
fn cli_help_and_version_exit_before_dispatch() {
    match cli_action(["--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("Usage: webcodex-cli"));
        }
        other => panic!("expected help exit, got {other:?}"),
    }
    match cli_action(["--version"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.starts_with(&format!(
                "webcodex-cli {} (commit ",
                env!("CARGO_PKG_VERSION")
            )));
            assert!(stdout.trim_end().ends_with(')'));
            assert_ne!(
                stdout,
                format!("webcodex-cli {}\n", env!("CARGO_PKG_VERSION"))
            );
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
            assert!(stdout.starts_with("webcodex-cli "));
            assert!(stderr.is_empty());
        }
        other => panic!("expected version exit, got {other:?}"),
    }
}

#[test]
fn removed_onboarding_and_doctor_commands_do_not_dispatch() {
    for command in ["connect", "doctor"] {
        match cli_action([command]) {
            CliAction::Exit {
                code: 2, stderr, ..
            } => assert!(stderr.contains("unknown command"), "{stderr}"),
            other => panic!("{command} unexpectedly dispatched: {other:?}"),
        }
    }
}

#[test]
fn webcodex_cli_help_mentions_management_commands() {
    match cli_action(["--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("pairing create"));
            assert!(stdout.contains("client enroll"));
            assert!(stdout.contains("token generate"));
            assert!(stdout.contains("token create-local"));
            assert!(stdout.contains("token register-hash"));
            assert!(stdout.contains("agent-token create-local"));
            assert!(stdout.contains("agent-token register-hash"));
            assert!(stdout.contains("agent init/install-service/status"));
        }
        other => panic!("expected help exit, got {other:?}"),
    }
}

#[test]
fn common_help_entrypoints_smoke() {
    let cases: &[(&[&str], &[&str])] = &[
        (
            &["--help"],
            &[
                "Usage: webcodex-cli <COMMAND>",
                "Commands:",
                "server up",
                "setup single-user",
            ],
        ),
        (
            &["server", "--help"],
            &[
                "Usage: webcodex-cli server <COMMAND>",
                "Commands:",
                "up",
                "init",
                "install-service",
                "status",
            ],
        ),
        (
            &["setup", "--help"],
            &[
                "Usage: webcodex-cli <COMMAND>",
                "Commands:",
                "setup single-user",
                "Common flags",
                "--server-url URL",
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
fn webcodex_cli_agent_help_mentions_new_subcommands() {
    match cli_action(["agent", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("install-service"));
            assert!(stdout.contains("status"));
            assert!(stdout.contains("init"));
        }
        other => panic!("expected help exit, got {other:?}"),
    }
    match cli_action(["agent", "install-service", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("--config PATH"));
            assert!(stdout.contains("--bin PATH"));
            assert!(stdout.contains("Tokens are never inlined"));
        }
        other => panic!("expected help exit, got {other:?}"),
    }
    match cli_action(["agent", "status", "--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("--user-token-file PATH"));
            assert!(stdout.contains("--agent-token-file PATH"));
            assert!(stdout.contains("no tokens"));
        }
        other => panic!("expected help exit, got {other:?}"),
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
