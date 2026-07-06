use super::support::*;

#[test]
fn ops_help_entrypoints_print_usage() {
    let cases: &[(&[&str], &[&str])] = &[
        (
            &["ops", "--help"],
            &[
                "Usage: webcodex-cli ops <COMMAND>",
                "status",
                "agents",
                "projects",
                "smoke-preflight",
                "--server-url URL",
                "--token TOKEN",
            ],
        ),
        (
            &["ops", "status", "--help"],
            &[
                "Usage: webcodex-cli ops status",
                "--server-url URL",
                "--url URL",
                "--env-file PATH",
                "--token-file PATH",
                "--token TOKEN",
                "--json",
            ],
        ),
        (
            &["ops", "agents", "--help"],
            &[
                "Usage: webcodex-cli ops agents",
                "--server-url URL",
                "--url URL",
                "--env-file PATH",
                "--token-file PATH",
                "--token TOKEN",
                "--json",
            ],
        ),
        (
            &["ops", "projects", "--help"],
            &[
                "Usage: webcodex-cli ops projects",
                "--server-url URL",
                "--url URL",
                "--env-file PATH",
                "--token-file PATH",
                "--token TOKEN",
                "--json",
            ],
        ),
        (
            &["ops", "smoke-preflight", "--help"],
            &[
                "Usage: webcodex-cli ops smoke-preflight",
                "--project PROJECT_ID",
                "--server-url URL",
                "--url URL",
                "--env-file PATH",
                "--token-file PATH",
                "--token TOKEN",
                "--json",
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
fn top_level_help_mentions_ops() {
    let out = cli_exit(["--help"]).unwrap();
    assert!(out.contains("ops status|agents|projects|smoke-preflight"));
}

#[test]
fn ops_unknown_subcommand_is_clear() {
    match cli_action(["ops", "unknown"]) {
        CliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            assert_eq!(code, 2);
            assert!(stdout.is_empty());
            assert!(stderr.contains("unknown ops subcommand: unknown"));
        }
        other => panic!("expected unknown ops subcommand exit, got {other:?}"),
    }
}

#[test]
fn ops_common_flags_parse_without_printing_token() {
    match cli_action([
        "ops",
        "status",
        "--url",
        "http://runtime.example",
        "--env-file",
        "/tmp/webcodex.env",
        "--token-file",
        "/tmp/token",
        "--token",
        "secret-token-value",
        "--json",
    ]) {
        CliAction::Ops(OpsCommand::Status(opts)) => {
            assert_eq!(opts.server_url, "http://runtime.example");
            assert_eq!(
                opts.env_file.as_deref(),
                Some(Path::new("/tmp/webcodex.env"))
            );
            assert_eq!(opts.token_file.as_deref(), Some(Path::new("/tmp/token")));
            assert_eq!(opts.token.as_deref(), Some("secret-token-value"));
            assert!(opts.json);
        }
        other => panic!("expected ops status action, got {other:?}"),
    }
}

#[test]
fn ops_smoke_preflight_requires_project() {
    match cli_action(["ops", "smoke-preflight", "--json"]) {
        CliAction::Exit { code, stderr, .. } => {
            assert_eq!(code, 2);
            assert!(stderr.contains("--project is required"));
        }
        other => panic!("expected missing project exit, got {other:?}"),
    }
}

#[test]
fn ops_parser_errors_do_not_leak_token_value() {
    let secret = "secret-token-value";
    match cli_action(["ops", "status", "--token", secret, "--bad-flag"]) {
        CliAction::Exit { code, stderr, .. } => {
            assert_eq!(code, 2);
            assert!(stderr.contains("unknown ops status flag: --bad-flag"));
            assert!(!stderr.contains(secret));
        }
        other => panic!("expected parser error, got {other:?}"),
    }
}
