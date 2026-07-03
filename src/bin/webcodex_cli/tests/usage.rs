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
fn webcodex_cli_help_mentions_pairing_client_and_doctor() {
    match cli_action(["--help"]) {
        CliAction::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("pairing create"));
            assert!(stdout.contains("client enroll"));
            assert!(stdout.contains("doctor"));
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

#[test]
fn top_level_usage_mentions_connect_and_server_up() {
    let out = cli_exit(["--help"]).unwrap();
    assert!(out.contains("connect"));
    assert!(out.contains("server up"));
}
