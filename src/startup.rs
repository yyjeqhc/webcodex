use crate::{admin_cli, build_info, hosted_connect};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ServerCliAction {
    Run,
    Connect(hosted_connect::HostedConnectOptions),
    Admin(admin_cli::AdminCliCommand),
    Exit {
        code: i32,
        stdout: String,
        stderr: String,
    },
}

fn server_usage() -> String {
    format!(
        "Usage: webcodex [OPTIONS]\n       webcodex connect <TARGET> --via <INGRESS> [OPTIONS]\n       webcodex <ADMIN-COMMAND>\n\n\
Commands:\n\
  connect            Open the current project and connect a hosted client\n\
  serve              Run the HTTP runtime (internal/advanced mode)\n\n\
Options:\n\
  -h, --help       Print help and exit\n\
  -V, --version    Print version and exit\n\n\
{}\
Environment:\n\
  WEBCODEX_ENV_FILE      Load environment variables from this file\n\
  WEBCODEX_TOKEN         Bearer token for protected API endpoints\n\
  WEBCODEX_ADDR          Listen address, default 0.0.0.0:8080\n\
  WEBCODEX_DATA          Data directory, default ./data\n\
  WEBCODEX_PUBLIC_URL    Public URL reported to clients\n\
  WEBCODEX_ALLOW_ANONYMOUS  Allow anonymous GPT/MCP and client access (--open). \
Default off; only safe on localhost/trusted LAN/temporary demos.\n\
  WEBCODEX_QUIC_ENABLED  Enable QUIC agent transport (default off)\n\
  WEBCODEX_QUIC_LISTEN   QUIC UDP listen addr, default 0.0.0.0:8443\n\
  WEBCODEX_QUIC_CERT     PEM cert path for the QUIC listener\n\
  WEBCODEX_QUIC_KEY      PEM key path for the QUIC listener\n\
  WEBCODEX_QUIC_ALPN     QUIC ALPN, default webcodex-agent/1\n",
        admin_cli::usage()
    )
}

pub(crate) fn server_cli_action<I, S>(args: I) -> ServerCliAction
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    if args.is_empty() {
        return ServerCliAction::Run;
    }
    if args[0] == "connect" {
        if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
            return ServerCliAction::Exit {
                code: 0,
                stdout: hosted_connect::usage().to_string(),
                stderr: String::new(),
            };
        }
        return match hosted_connect::parse(&args[1..]) {
            Ok(opts) => ServerCliAction::Connect(opts),
            Err(e) => ServerCliAction::Exit {
                code: 2,
                stdout: String::new(),
                stderr: format!("{}\n\n{}", e, hosted_connect::usage()),
            },
        };
    }
    if args.len() == 1 && args[0] == "serve" {
        return ServerCliAction::Run;
    }
    if admin_cli::is_admin_group(&args[0]) {
        return match admin_cli::parse_admin_cli(&args) {
            Ok(cmd) => ServerCliAction::Admin(cmd),
            Err(e) => ServerCliAction::Exit {
                code: 2,
                stdout: String::new(),
                stderr: format!("{}\n", e),
            },
        };
    }
    if args.len() == 1 {
        match args[0].as_str() {
            "--help" | "-h" => {
                return ServerCliAction::Exit {
                    code: 0,
                    stdout: server_usage(),
                    stderr: String::new(),
                };
            }
            "--version" | "-V" => {
                return ServerCliAction::Exit {
                    code: 0,
                    stdout: build_info::version_output("webcodex"),
                    stderr: String::new(),
                };
            }
            _ => {}
        }
    }
    ServerCliAction::Exit {
        code: 2,
        stdout: String::new(),
        stderr: format!(
            "unknown argument(s): {}\n{}",
            args.join(" "),
            server_usage()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_cli_output<I, S>(args: I) -> Result<Option<String>, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        match server_cli_action(args) {
            ServerCliAction::Run => Ok(None),
            ServerCliAction::Connect(_) => Ok(None),
            ServerCliAction::Admin(_) => Ok(None),
            ServerCliAction::Exit {
                code: 0, stdout, ..
            } => Ok(Some(stdout)),
            ServerCliAction::Exit { stderr, .. } => Err(stderr),
        }
    }

    #[test]
    fn server_cli_help_mentions_env_vars() {
        let output = server_cli_output(["--help"]).unwrap().unwrap();
        assert!(output.contains("Usage: webcodex"));
        for key in [
            "WEBCODEX_ENV_FILE",
            "WEBCODEX_TOKEN",
            "WEBCODEX_ADDR",
            "WEBCODEX_DATA",
            "WEBCODEX_PUBLIC_URL",
            "WEBCODEX_ALLOW_ANONYMOUS",
            "WEBCODEX_QUIC_ENABLED",
            "WEBCODEX_QUIC_LISTEN",
            "WEBCODEX_QUIC_CERT",
            "WEBCODEX_QUIC_KEY",
            "WEBCODEX_QUIC_ALPN",
        ] {
            assert!(output.contains(key), "help missing {key}");
        }
    }

    #[test]
    fn server_cli_short_help_and_version_exit_before_startup() {
        assert!(server_cli_output(["-h"])
            .unwrap()
            .unwrap()
            .contains("Usage: webcodex"));
        for output in [
            server_cli_output(["--version"]).unwrap().unwrap(),
            server_cli_output(["-V"]).unwrap().unwrap(),
        ] {
            assert!(output.starts_with(&format!("webcodex {} (commit ", env!("CARGO_PKG_VERSION"))));
            assert_ne!(output, format!("webcodex {}\n", env!("CARGO_PKG_VERSION")));
        }
        assert!(server_cli_output(["connect", "--help"])
            .unwrap()
            .unwrap()
            .contains("connect <TARGET>"));
    }

    #[test]
    fn server_cli_rejects_unknown_arguments() {
        assert!(server_cli_output(["--bogus"])
            .unwrap_err()
            .contains("unknown argument"));
    }

    #[test]
    fn server_help_mentions_expected_env_vars() {
        let action = server_cli_action(["--help"]);
        let ServerCliAction::Exit {
            code,
            stdout,
            stderr,
        } = action
        else {
            panic!("expected help to exit");
        };
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("Usage: webcodex"));
        for key in [
            "WEBCODEX_ENV_FILE",
            "WEBCODEX_TOKEN",
            "WEBCODEX_ADDR",
            "WEBCODEX_DATA",
            "WEBCODEX_PUBLIC_URL",
            "WEBCODEX_ALLOW_ANONYMOUS",
            "WEBCODEX_QUIC_ENABLED",
            "WEBCODEX_QUIC_LISTEN",
            "WEBCODEX_QUIC_CERT",
            "WEBCODEX_QUIC_KEY",
            "WEBCODEX_QUIC_ALPN",
        ] {
            assert!(stdout.contains(key), "usage missing {key}");
        }
    }

    #[test]
    fn version_output_includes_build_commit_or_unknown() {
        let action = server_cli_action(["-V"]);
        let ServerCliAction::Exit {
            code,
            stdout,
            stderr,
        } = action
        else {
            panic!("expected version to exit");
        };
        assert_eq!(code, 0);
        assert!(stdout.starts_with(&format!("webcodex {} (commit ", env!("CARGO_PKG_VERSION"))));
        assert!(stdout.trim_end().ends_with(')'));
        assert_ne!(stdout, format!("webcodex {}\n", env!("CARGO_PKG_VERSION")));
        assert!(stderr.is_empty());
    }

    #[test]
    fn server_no_args_runs_normally() {
        assert_eq!(
            server_cli_action(std::iter::empty::<&str>()),
            ServerCliAction::Run
        );
        assert_eq!(server_cli_action(["serve"]), ServerCliAction::Run);
    }
}
