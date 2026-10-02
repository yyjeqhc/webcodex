//! Runner process entry: argument parsing, startup and exit only.
mod webcodex_runner;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;
use webcodex_build_info as build_info;
use webcodex_runner::config::{
    client_profile_runner_config, default_config_path, load_config, validate_client_profile,
};
use webcodex_runner::transport::{registration::process_started_at, run_runner};

#[derive(Debug, Clone, PartialEq, Eq)]
enum RunnerCliAction {
    Run {
        config_path: PathBuf,
        once: bool,
        stop_on_stdin_eof: bool,
        computer_session_dir: Option<PathBuf>,
    },
    ComputerSessionHelper {
        session_state_dir: PathBuf,
    },
    Exit {
        code: i32,
        stdout: String,
        stderr: String,
    },
}

fn usage() -> &'static str {
    "Usage: webcodex-runner [--config PATH] [--once] [--stop-on-stdin-eof] [--computer-session-dir PATH]\n\n\
     Options:\n\
       -h, --help                 Print help and exit\n\
       -V, --version              Print version and exit\n\
       -c, --config PATH          Runner config path for normal runtime\n\
       --profile NAME             Client config profile for default config path\n\
       --once                     Complete one successful poll, then exit (polling transport)\n\
       --stop-on-stdin-eof        Stop when the invoking parent closes stdin\n\n\
       --computer-session-dir PATH  Route Computer calls to login-session helper\n\
       --computer-session-helper --session-state-dir PATH  Run login-session helper\n\n\
     With --profile, the default config path is derived under\n\
     /etc/webcodex/clients/<profile> for root or\n\
     ~/.config/webcodex/clients/<profile> for non-root users. Explicit\n\
     --config overrides the profile-derived default.\n\n\
     Environment:\n\
       WEBCODEX_RUNNER_CONFIG     default config path override\n\
     Example runner.toml:\n\
       server_url = \"https://v4.yyjeqhc.cn\"\n\
       token = \"...\"\n\
       client_id = \"xrh\"\n\
       display_name = \"XRH\"\n\
       owner = \"yyjeqhc\"\n\
       project_registry_dir = \"/root/.config/webcodex/project-registry\"\n\
       poll_interval_ms = 1000\n\
\n\
       [policy]\n\
       allow_raw_shell = true\n\
       allow_cwd_anywhere = true\n\
       max_timeout_secs = 3600\n\
       max_output_bytes = 262144\n"
}

fn parse_args() -> Result<RunnerCliAction, String> {
    parse_runner_args(std::env::args().skip(1))
}

#[cfg(windows)]
fn parse_service_runner_args(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    if args.len() != 4
        || args[0] != "--config"
        || args[2] != "--computer-session-dir"
        || args[1].is_empty()
        || args[3].is_empty()
    {
        return Err("Runner service requires --config PATH --computer-session-dir PATH".into());
    }
    Ok((PathBuf::from(&args[1]), PathBuf::from(&args[3])))
}

fn parse_runner_args<I, S>(args: I) -> Result<RunnerCliAction, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    if args
        .first()
        .is_some_and(|arg| arg == "--computer-session-helper")
    {
        if args.len() != 3 || args[1] != "--session-state-dir" || args[2].is_empty() {
            return Err(
                "computer session helper requires only --session-state-dir PATH".to_string(),
            );
        }
        return Ok(RunnerCliAction::ComputerSessionHelper {
            session_state_dir: PathBuf::from(&args[2]),
        });
    }
    if args.len() == 1 {
        match args[0].as_str() {
            "--build-info-json" => {
                return Ok(RunnerCliAction::Exit {
                    code: 0,
                    stdout: build_info::build_info_json("webcodex-runner"),
                    stderr: String::new(),
                });
            }
            "--help" | "-h" => {
                return Ok(RunnerCliAction::Exit {
                    code: 0,
                    stdout: usage().to_string(),
                    stderr: String::new(),
                });
            }
            "--version" | "-V" => {
                return Ok(RunnerCliAction::Exit {
                    code: 0,
                    stdout: build_info::version_output("webcodex-runner"),
                    stderr: String::new(),
                });
            }
            _ => {}
        }
    }
    let runner_config_env = std::env::var("WEBCODEX_RUNNER_CONFIG").ok();
    let mut config_path: Option<PathBuf> = None;
    let mut profile: Option<String> = None;
    let mut once = false;
    let mut stop_on_stdin_eof = false;
    let mut computer_session_dir = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                return Ok(RunnerCliAction::Exit {
                    code: 0,
                    stdout: usage().to_string(),
                    stderr: String::new(),
                });
            }
            "--version" | "-V" => {
                return Ok(RunnerCliAction::Exit {
                    code: 0,
                    stdout: build_info::version_output("webcodex-runner"),
                    stderr: String::new(),
                });
            }
            "--once" => once = true,
            "--stop-on-stdin-eof" => stop_on_stdin_eof = true,
            "--computer-session-dir" => {
                let Some(path) = args.next() else {
                    return Err("--computer-session-dir requires a path".to_string());
                };
                computer_session_dir = Some(PathBuf::from(path));
            }
            "--config" | "-c" => {
                let Some(path) = args.next() else {
                    return Err("--config requires a path".to_string());
                };
                config_path = Some(PathBuf::from(path));
            }
            "--profile" => {
                let Some(value) = args.next() else {
                    return Err("--profile requires a value".to_string());
                };
                profile = Some(value);
            }
            _ => return Err(format!("unknown argument: {}\n{}", arg, usage())),
        }
    }
    let profile = profile
        .as_deref()
        .map(validate_client_profile)
        .transpose()?;
    let config_path = if let Some(config_path) = config_path {
        config_path
    } else if let Some(profile) = profile {
        client_profile_runner_config(&profile)?
    } else {
        runner_config_env
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(default_config_path)?
    };
    Ok(RunnerCliAction::Run {
        config_path,
        once,
        stop_on_stdin_eof,
        computer_session_dir,
    })
}

fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let service = match webcodex_environment::runtime_entry::split_windows_service_args(&raw_args) {
        Ok(service) => service,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    if let Some((name, args)) = service {
        #[cfg(windows)]
        {
            let (config_path, computer_session_dir) = match parse_service_runner_args(&args) {
                Ok(paths) => paths,
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(2);
                }
            };
            let result =
                webcodex_environment::service::runtime::run_windows_service(&name, move |stop| {
                    let log_dir = config_path
                        .parent()
                        .ok_or("Runner config has no parent directory")?;
                    let mut service_log = webcodex_environment::service::ServiceLogGuard::open(
                        log_dir,
                        webcodex_environment::service::Component::Runner,
                    )?;
                    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
                    let _ = tracing_subscriber::fmt()
                        .with_env_filter(
                            EnvFilter::try_from_default_env()
                                .unwrap_or_else(|_| EnvFilter::new("info")),
                        )
                        .try_init();
                    let cfg = load_config(&config_path)?;
                    let result = run_runner(
                        cfg,
                        config_path,
                        false,
                        false,
                        Some(computer_session_dir),
                        Some(stop),
                    );
                    if result.is_ok() {
                        service_log.stopped()?;
                    }
                    result
                });
            if let Err(error) = result {
                eprintln!("webcodex-runner service failed: {error}");
                std::process::exit(1);
            }
            return;
        }
        #[cfg(not(windows))]
        {
            let _ = (name, args);
            unreachable!("service prefix rejected on non-Windows");
        }
    }
    if let Some(code) = webcodex_runner::detached_job::maybe_run_internal_mode(raw_args.iter()) {
        std::process::exit(code);
    }
    // Pin the process start timestamp before any transport work so register
    // payloads report real process identity even after reconnect loops.
    let _ = process_started_at();
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    let action = match parse_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };
    let (config_path, once, stop_on_stdin_eof, computer_session_dir) = match action {
        RunnerCliAction::Run {
            config_path,
            once,
            stop_on_stdin_eof,
            computer_session_dir,
        } => (config_path, once, stop_on_stdin_eof, computer_session_dir),
        RunnerCliAction::ComputerSessionHelper { session_state_dir } => {
            if let Err(error) = webcodex_runner::computer_session::run_helper(&session_state_dir) {
                eprintln!("{error}");
                std::process::exit(2);
            }
            return;
        }
        RunnerCliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
            std::process::exit(code);
        }
    };
    #[cfg(target_os = "macos")]
    let mut service_log = match webcodex_environment::service::ServiceLogGuard::from_managed_env(
        webcodex_environment::service::Component::Runner,
    ) {
        Ok(log) => log,
        Err(error) => {
            eprintln!("Runner service lifecycle log unavailable: {error}");
            std::process::exit(2);
        }
    };
    let cfg = match load_config(&config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            #[cfg(target_os = "macos")]
            drop(service_log);
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };
    if cfg.token.trim().is_empty() {
        eprintln!(
            "webcodex-runner warning: agent token is empty; connecting without Authorization; the server must be started with --open"
        );
    }
    if let Err(e) = run_runner(
        cfg,
        config_path,
        once,
        stop_on_stdin_eof,
        computer_session_dir,
        #[cfg(windows)]
        None,
    ) {
        #[cfg(target_os = "macos")]
        drop(service_log);
        eprintln!("webcodex-runner failed: {}", e);
        std::process::exit(1);
    }
    #[cfg(target_os = "macos")]
    if let Some(log) = service_log.as_mut() {
        let _ = log.stopped();
    }
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
