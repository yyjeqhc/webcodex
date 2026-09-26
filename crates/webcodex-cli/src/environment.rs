//! Input/output adapter only. All deployment decisions live in the shared Core.
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use webcodex_environment::*;

const USAGE: &str = "webcodex environment <COMMAND>\n\nconfigure [--create | --join URL] [--project PATH | --no-project]\n          [--code-stdin | --token-file PATH] [--new-pairing-code]\nresume    [--code-stdin] [--new-pairing-code] [--token-file PATH]\ninvite\nadd-project PATH [--code-stdin] [--new-pairing-code]\nremove-project PROJECT_ID\nstatus|doctor\nstart|stop|restart <server|runner|tunnel> [--profile PROFILE]\nrepair-credential runner\nrepair-user-credential [--token-file PATH]\nuninstall-service <server|runner|tunnel> [--profile PROFILE]\nconfigure-tunnel [PROFILE] [--credentials-file PATH]\ntunnel-status [PROFILE]\nremove-tunnel [PROFILE]\nupgrade-preflight|upgrade-prepare --candidate-dir PATH [--development-build]\nupgrade-finish|upgrade-rollback\nmigrate-legacy-runner --join URL --project PATH --token-file PATH [--profile PROFILE] (Linux)\nmigrate-legacy-server --user NAME --token-file PATH --listen ORIGINAL_ADDR [--server-url URL] (Linux)\ninstaller-authorize --upgrade-receipt PATH --candidate-dir PATH\ninstaller-verify --candidate-dir PATH\ninstaller-verify-same --candidate-dir PATH --expected-runtime-dir PATH\ninstaller-finish|installer-cancel\n\nPublic environment commands accept --json and --environment-dir PATH.\nInstaller finalization uses only the fixed owner authorization.\nAdvanced: --bin-dir PATH (configure and explicit migration).\nViewer-only uses a user credential; pairing codes are only for project machines.\nTunnel credentials use hidden input or a protected JSON file with tunnel_id and api_key.\n";

#[derive(Default)]
struct Input {
    command: String,
    create: bool,
    join: Option<String>,
    project: Option<PathBuf>,
    no_project: bool,
    directory: Option<PathBuf>,
    bin_dir: Option<PathBuf>,
    token_file: Option<PathBuf>,
    code_stdin: bool,
    new_code: bool,
    json: bool,
    operand: Option<String>,
    candidate_dir: Option<PathBuf>,
    development_build: bool,
    credentials_file: Option<PathBuf>,
    profile: Option<String>,
    upgrade_receipt: Option<PathBuf>,
    expected_runtime_dir: Option<PathBuf>,
    username: Option<String>,
    listen: Option<String>,
    server_url: Option<String>,
}

fn parse(args: &[String]) -> Result<Input, String> {
    let mut input = Input {
        command: args.first().cloned().unwrap_or_else(|| "configure".into()),
        ..Default::default()
    };
    let mut iter = args.iter().skip(1);
    let mut options = std::collections::BTreeSet::new();
    while let Some(arg) = iter.next() {
        if arg.starts_with('-') && !options.insert(arg.as_str()) {
            return Err("Environment options must not be repeated".into());
        }
        let value =
            |iter: &mut std::iter::Skip<std::slice::Iter<'_, String>>| -> Result<String, String> {
                iter.next()
                    .cloned()
                    .ok_or_else(|| "Missing option value".into())
            };
        match arg.as_str() {
            "--user" => input.username = Some(value(&mut iter)?),
            "--listen" => input.listen = Some(value(&mut iter)?),
            "--server-url" => input.server_url = Some(value(&mut iter)?),
            "--create" => input.create = true,
            "--join" => input.join = Some(value(&mut iter)?),
            "--project" => input.project = Some(PathBuf::from(value(&mut iter)?)),
            "--no-project" => input.no_project = true,
            "--environment-dir" => input.directory = Some(PathBuf::from(value(&mut iter)?)),
            "--bin-dir" => input.bin_dir = Some(PathBuf::from(value(&mut iter)?)),
            "--candidate-dir" => input.candidate_dir = Some(PathBuf::from(value(&mut iter)?)),
            "--token-file" => input.token_file = Some(PathBuf::from(value(&mut iter)?)),
            "--credentials-file" => input.credentials_file = Some(PathBuf::from(value(&mut iter)?)),
            "--profile" => input.profile = Some(value(&mut iter)?),
            "--upgrade-receipt" => input.upgrade_receipt = Some(PathBuf::from(value(&mut iter)?)),
            "--expected-runtime-dir" => {
                input.expected_runtime_dir = Some(PathBuf::from(value(&mut iter)?))
            }
            "--code-stdin" => input.code_stdin = true,
            "--new-pairing-code" => input.new_code = true,
            "--json" => input.json = true,
            "--development-build" => input.development_build = true,
            _ if !arg.starts_with('-') && input.operand.is_none() => {
                input.operand = Some(arg.clone())
            }
            _ => {
                return Err(
                    "Unknown or repeated environment argument; use environment --help".into(),
                )
            }
        }
    }
    if input.create && input.join.is_some() || input.project.is_some() && input.no_project {
        return Err("Choose exactly one environment action and one project option".into());
    }
    if input.code_stdin && input.token_file.is_some() {
        return Err(
            "Provide the credential for the selected path: a pairing code or a user token".into(),
        );
    }
    if input.development_build
        && !matches!(
            input.command.as_str(),
            "upgrade-preflight" | "upgrade-prepare"
        )
    {
        return Err("--development-build applies only to upgrade candidate verification".into());
    }
    if input.credentials_file.is_some() && input.command != "configure-tunnel" {
        return Err("--credentials-file applies only to configure-tunnel".into());
    }
    if (input.username.is_some() || input.listen.is_some() || input.server_url.is_some())
        && input.command != "migrate-legacy-server"
    {
        return Err(
            "--user, --listen and --server-url apply only to explicit legacy Server migration"
                .into(),
        );
    }
    Ok(input)
}

pub(crate) async fn run(args: &[String]) -> Result<String, String> {
    run_inner(args).await.map_err(|error| {
        if args.iter().any(|arg| arg == "--json")
            && serde_json::from_str::<serde_json::Value>(&error).is_err()
        {
            serde_json::json!({"ok":false,"error":error}).to_string()
        } else {
            error
        }
    })
}

async fn run_inner(args: &[String]) -> Result<String, String> {
    #[cfg(unix)]
    if args.first().map(String::as_str) == Some("__installer-child") {
        if args.len() != 4
            || args[1] != "3"
            || !matches!(args[2].as_str(), "finish" | "rollback")
            || !Path::new(&args[3]).is_absolute()
        {
            return Err("Invalid internal installer request".into());
        }
        run_installer_upgrade_child(3, &args[2], Path::new(&args[3]))
            .await
            .map_err(|error| error.to_string())?;
        return Ok(String::new());
    }
    if args.first().map(String::as_str) == Some("__service-operation") {
        if args.len() != 3 {
            return Err("Invalid internal service request".into());
        }
        run_privileged_service_request(Path::new(&args[1]), Path::new(&args[2]))
            .map_err(|e| e.to_string())?;
        return Ok(String::new());
    }
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return Ok(USAGE.into());
    }
    let mut input = parse(args)?;
    #[cfg(unix)]
    if input.command == "installer-finish" {
        if args.iter().skip(1).any(|arg| arg != "--json") {
            return Err("installer-finish accepts only --json; its original environment is fixed by the owner authorization".into());
        }
        finish_authorized_installation()
            .await
            .map_err(|error| error.to_string())?;
        return Ok("{\"installation_finished\":true}".into());
    }
    if input.command == "installer-verify-same" {
        let candidate = absolute(
            input
                .candidate_dir
                .as_deref()
                .ok_or("Specify --candidate-dir PATH")?,
        )?;
        let runtime = absolute(
            input
                .expected_runtime_dir
                .as_deref()
                .ok_or("Specify --expected-runtime-dir PATH")?,
        )?;
        verify_same_installed_package(&candidate, &runtime)
            .await
            .map_err(|error| error.to_string())?;
        return Ok("{\"ok\":true,\"same\":true}".into());
    }
    // A system installer must never accidentally configure root's environment.
    // Owner-bound receipt handling happens before opening any default Store.
    if matches!(
        input.command.as_str(),
        "installer-authorize" | "installer-verify" | "installer-cancel"
    ) {
        if input.command == "installer-cancel" {
            cancel_installer_authorization().map_err(|error| error.to_string())?;
            return Ok("{\"authorization_removed\":true}".into());
        }
        let candidate = absolute(
            input
                .candidate_dir
                .as_deref()
                .ok_or("Specify --candidate-dir PATH")?,
        )?;
        let receipt = if input.command == "installer-authorize" {
            let receipt = absolute(
                input
                    .upgrade_receipt
                    .as_deref()
                    .ok_or("Specify the original user's --upgrade-receipt PATH")?,
            )?;
            authorize_prepared_installation(&receipt, &candidate).await
        } else if let Some(receipt) = input.upgrade_receipt.as_deref() {
            verify_prepared_installation(&absolute(receipt)?, &candidate).await
        } else if cfg!(unix)
            && current_account()
                .map_err(|error| error.to_string())?
                .identity
                == "0"
        {
            verify_installer_authorization(&candidate).await
        } else {
            let root = input
                .directory
                .as_ref()
                .map(|path| absolute(path))
                .unwrap_or_else(|| default_environment_dir().map_err(|error| error.to_string()))?;
            verify_prepared_installation(&root.join("upgrade-prepared.json"), &candidate).await
        }
        .map_err(|error| error.to_string())?;
        if let Some(directory) = input.expected_runtime_dir.as_deref() {
            verify_installer_targets(&receipt, &absolute(directory)?)
                .map_err(|error| error.to_string())?;
        }
        return serde_json::to_string_pretty(&receipt)
            .map_err(|_| "Could not encode installer receipt".into());
    }
    let root = input
        .directory
        .take()
        .map(Ok)
        .unwrap_or_else(default_environment_dir)
        .map_err(|e| e.to_string())?;
    let store = EnvironmentStore::open(absolute(&root)?).map_err(|e| e.to_string())?;
    let mut core = EnvironmentSetup::new(NativeEnvironment::new().map_err(|e| e.to_string())?);
    match input.command.as_str() {
        "upgrade-preflight" | "upgrade-prepare" => {
            let candidate = input.candidate_dir.ok_or("Specify --candidate-dir PATH")?;
            let result = match (input.command.as_str(), input.development_build) {
                ("upgrade-preflight", false) => {
                    core.backend.upgrade_preflight(&store, &candidate).await
                }
                ("upgrade-preflight", true) => {
                    core.backend
                        .upgrade_preflight_development(&store, &candidate)
                        .await
                }
                (_, false) => core.backend.upgrade_prepare(&store, &candidate).await,
                (_, true) => {
                    core.backend
                        .upgrade_prepare_development(&store, &candidate)
                        .await
                }
            };
            match result {
                Ok(result) if result.ready => serde_json::to_string_pretty(&result)
                    .map_err(|_| "Could not encode upgrade preflight".into()),
                Ok(result) => Err(serde_json::to_string(&result).unwrap_or_default()),
                Err(diagnostic) => {
                    Err(serde_json::json!({"ready":false,"diagnostics":[diagnostic]}).to_string())
                }
            }
        }
        "upgrade-finish" | "upgrade-rollback" => {
            if input.command == "upgrade-finish" {
                core.backend.upgrade_finish(&store).await
            } else {
                core.backend.upgrade_rollback(&store).await
            }
            .map_err(|e| e.to_string())?;
            Ok("{\"ready\":true}".into())
        }
        #[cfg(target_os = "linux")]
        "migrate-legacy-server" => {
            if input.create
                || input.join.is_some()
                || input.project.is_some()
                || input.code_stdin
                || input.new_code
            {
                return Err("Legacy Server migration preserves its original listener and credentials; use --listen, --user and --token-file".into());
            }
            let listen = input
                .listen
                .as_deref()
                .ok_or("Specify the original --listen ADDRESS")?;
            let socket: std::net::SocketAddr = listen
                .parse()
                .map_err(|_| "The original listen address is invalid")?;
            let reachable = if socket.ip().is_unspecified() {
                std::net::SocketAddr::new(
                    if socket.is_ipv6() {
                        std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
                    } else {
                        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                    },
                    socket.port(),
                )
            } else {
                socket
            };
            let default_url = format!("http://{reachable}");
            let request = SetupRequest {
                mode: EnvironmentMode::Create {
                    listen: listen.into(),
                },
                server_url: canonical_server_url(
                    input.server_url.as_deref().unwrap_or(&default_url),
                )
                .map_err(|e| e.to_string())?,
                project: None,
                account: current_account().map_err(|e| e.to_string())?,
                binaries: discover_binaries(input.bin_dir.as_deref())?,
            };
            let legacy = LegacyCliServerInput {
                username: input.username.ok_or("Specify the original --user NAME")?,
                user_token_file: absolute(
                    input
                        .token_file
                        .as_deref()
                        .ok_or("Specify the original user's --token-file PATH")?,
                )?,
            };
            let result = migrate_legacy_cli_system_server(
                &store,
                request,
                legacy,
                &SetupSecrets::default(),
                progress_sink(input.json),
            )
            .await
            .map_err(|e| e.to_string())?;
            render(&result, input.json)
        }
        #[cfg(target_os = "linux")]
        "migrate-legacy-runner" => {
            if input.create || input.no_project || input.code_stdin || input.new_code {
                return Err("Legacy migration preserves the original Runner and requires --join URL --project PATH --token-file PATH".into());
            }
            let request = SetupRequest {
                mode: EnvironmentMode::Join,
                server_url: canonical_server_url(
                    input
                        .join
                        .as_deref()
                        .ok_or("Specify the original --join URL")?,
                )
                .map_err(|e| e.to_string())?,
                project: Some(
                    input
                        .project
                        .as_deref()
                        .ok_or("Specify an original --project PATH")?
                        .canonicalize()
                        .map_err(|_| "The original project is unavailable")?,
                ),
                account: current_account().map_err(|e| e.to_string())?,
                binaries: discover_binaries(input.bin_dir.as_deref())?,
            };
            let legacy = LegacyCliRunnerInput {
                profile: input.profile,
                user_token_file: absolute(
                    input
                        .token_file
                        .as_deref()
                        .ok_or("Specify the original user's --token-file PATH")?,
                )?,
            };
            let result = migrate_legacy_cli_user_runner(
                &store,
                request,
                legacy,
                &SetupSecrets::default(),
                progress_sink(input.json),
            )
            .await
            .map_err(|e| e.to_string())?;
            render(&result, input.json)
        }
        "status" | "doctor" => {
            let result = if input.command == "doctor" {
                core.backend.doctor(&store).await
            } else {
                core.backend.status(&store).await
            }
            .map_err(|e| e.to_string())?;
            render(&result, input.json)
        }
        "invite" => {
            let code = core
                .backend
                .invite(&store)
                .await
                .map_err(|e| e.to_string())?;
            // This explicit command is the only intentional disclosure of an invitation.
            let server = store
                .load_environment()
                .map_err(|e| e.to_string())?
                .ok_or("Environment changed while creating the invitation")?
                .request
                .server_url;
            if input.json {
                Ok(serde_json::json!({"server_url":server,"pairing_code":code.expose(),"expires_in_seconds":600}).to_string())
            } else {
                Ok(format!(
                    "Server: {server}\nOne-time pairing code: {}\nExpires in 10 minutes.",
                    code.expose()
                ))
            }
        }
        "configure" | "resume" => {
            let interactive = std::io::stdin().is_terminal();
            let request = if input.command == "resume" {
                store
                    .load_journal()
                    .map_err(|e| e.to_string())?
                    .ok_or("No saved setup to resume")?
                    .environment
                    .request
            } else {
                if !input.create && input.join.is_none() {
                    if !interactive {
                        return Err("Specify --create or --join URL".into());
                    }
                    let choice = prompt(
                        "Create a WebCodex environment or join an existing one? [create/join]: ",
                    )?;
                    match choice.trim() {
                        "create" => input.create = true,
                        "join" => input.join = Some(prompt("Server address: ")?),
                        _ => return Err("Choose create or join".into()),
                    }
                }
                if input.project.is_none() && !input.no_project {
                    if !interactive {
                        return Err("Specify --project PATH or --no-project".into());
                    }
                    let path = prompt("Project folder on this machine (leave empty to skip): ")?;
                    if path.is_empty() {
                        input.no_project = true;
                    } else {
                        input.project = Some(PathBuf::from(path));
                    }
                }
                let project = input
                    .project
                    .as_ref()
                    .map(|path| {
                        path.canonicalize()
                            .map_err(|_| "Project folder does not exist".to_string())
                    })
                    .transpose()?;
                let server_url =
                    canonical_server_url(input.join.as_deref().unwrap_or("http://127.0.0.1:8080"))
                        .map_err(|e| e.to_string())?;
                let binaries = discover_binaries(input.bin_dir.as_deref())?;
                SetupRequest {
                    mode: if input.create {
                        EnvironmentMode::Create {
                            listen: "127.0.0.1:8080".into(),
                        }
                    } else {
                        EnvironmentMode::Join
                    },
                    server_url,
                    project,
                    account: current_account().map_err(|e| e.to_string())?,
                    binaries,
                }
            };
            let mut secrets = SetupSecrets {
                replacement_pairing_code: input.new_code,
                ..Default::default()
            };
            if input.code_stdin && (request.local_server() || !request.local_runner()) {
                return Err("Runner pairing is only used when joining with a local project".into());
            }
            if input.token_file.is_some() && (request.local_server() || request.local_runner()) {
                return Err(
                    "--token-file is for joining a viewer with an existing user credential".into(),
                );
            }
            if let Some(path) = input.token_file {
                secrets.user_token =
                    Some(read_secret(&absolute(&path)?).map_err(|e| e.to_string())?);
            }
            if input.code_stdin {
                secrets.pairing_code = Some(Secret::new(read_stdin_secret()?));
            }
            if interactive && !request.local_server() {
                if request.local_runner()
                    && !store.root().join("runner.toml").exists()
                    && !store.root().join("enrollment-recovery.json").exists()
                    && secrets.pairing_code.is_none()
                {
                    secrets.pairing_code =
                        Some(Secret::new(secret_prompt("One-time pairing code: ")?));
                } else if !request.local_runner()
                    && !store.root().join("webcodex-user-token").exists()
                    && secrets.user_token.is_none()
                {
                    secrets.user_token = Some(Secret::new(secret_prompt(
                        "User access credential (not a Runner pairing code): ",
                    )?));
                }
            }
            let json = input.json;
            let result = core
                .configure(&store, request, &secrets, progress_sink(json))
                .await
                .map_err(|e| e.to_string())?;
            render(&result, json)
        }
        "repair-user-credential" => {
            if input.create
                || input.join.is_some()
                || input.project.is_some()
                || input.code_stdin
                || input.new_code
                || input.operand.is_some()
            {
                return Err("User credential repair preserves the saved Server and user; supply --token-file or use hidden input".into());
            }
            let record = store
                .load_environment()
                .map_err(|e| e.to_string())?
                .ok_or("No saved environment is available")?;
            let token = match input.token_file {
                Some(path) => read_secret(&absolute(&path)?).map_err(|e| e.to_string())?,
                None if std::io::stdin().is_terminal() => {
                    Secret::new(secret_prompt("Existing Server user credential: ")?)
                }
                None => return Err("Use --token-file PATH or interactive hidden input".into()),
            };
            core.backend
                .repair_user_credential(&store, &record.environment_id, &token)
                .await
                .map_err(|e| e.to_string())?;
            Ok(if input.json {
                "{\"credential_saved\":true}".into()
            } else {
                "Server user credential restored; Runner identity and services retained".into()
            })
        }
        "start" | "stop" | "restart" | "repair-credential" | "uninstall-service" => {
            let component = match input.operand.as_deref() {
                Some("server") => service::Component::Server,
                Some("runner") => service::Component::Runner,
                Some("tunnel") => service::Component::Tunnel,
                _ => return Err("Specify server, runner or tunnel".into()),
            };
            let operation = match input.command.as_str() {
                "start" => ServiceOperation::Start,
                "stop" => ServiceOperation::Stop,
                "repair-credential" => ServiceOperation::UpdateCredential,
                "uninstall-service" => ServiceOperation::Uninstall,
                _ => ServiceOperation::Restart,
            };
            if input.profile.is_some() && component != service::Component::Tunnel {
                return Err("--profile identifies a Tunnel profile".into());
            }
            let status = if component == service::Component::Tunnel {
                let profile = input.profile.as_deref().unwrap_or("default");
                if operation == ServiceOperation::Start {
                    core.backend.configure_tunnel(&store, profile, None).await
                } else {
                    core.backend
                        .control_tunnel(&store, profile, operation)
                        .await
                }
            } else {
                core.backend
                    .control_service(&store, component, operation)
                    .await
            }
            .map_err(|e| e.to_string())?;
            if input.json {
                serde_json::to_string_pretty(&status)
                    .map_err(|_| "Could not encode service status".into())
            } else {
                Ok(format!("{}: running={:?}", status.id, status.running))
            }
        }
        "configure-tunnel" => {
            let profile = input.operand.as_deref().unwrap_or("default");
            let credentials = if let Some(path) = input.credentials_file {
                Some(read_tunnel_credentials(&absolute(&path)?)?)
            } else if tunnel_profiles(&store)
                .map_err(|e| e.to_string())?
                .iter()
                .any(|entry| entry.profile_id == profile)
            {
                None
            } else if std::io::stdin().is_terminal() {
                Some(TunnelCredentials {
                    tunnel_id: Secret::new(secret_prompt("Existing ChatGPT Tunnel ID: ")?),
                    api_key: Secret::new(secret_prompt("Existing Tunnel API credential: ")?),
                })
            } else {
                return Err(
                    "Supply the protected --credentials-file for this Tunnel profile".into(),
                );
            };
            let status = core
                .backend
                .configure_tunnel(&store, profile, credentials.as_ref())
                .await
                .map_err(|e| e.to_string())?;
            if input.json {
                serde_json::to_string_pretty(&status)
                    .map_err(|_| "Could not encode Tunnel status".into())
            } else {
                Ok(format!("{}: Tunnel and local MCP are ready", status.id))
            }
        }
        "tunnel-status" => {
            let status = core
                .backend
                .tunnel_status(&store, input.operand.as_deref().unwrap_or("default"))
                .map_err(|e| e.to_string())?;
            if input.json {
                serde_json::to_string_pretty(&status)
                    .map_err(|_| "Could not encode Tunnel status".into())
            } else {
                Ok(format!(
                    "{}: running={:?}, Tunnel ready={}, local MCP ready={}",
                    status.service_status.id,
                    status.service_status.running,
                    status.tunnel_ready,
                    status.local_mcp_ready
                ))
            }
        }
        "remove-tunnel" => {
            core.backend
                .remove_tunnel(&store, input.operand.as_deref().unwrap_or("default"))
                .await
                .map_err(|e| e.to_string())?;
            Ok(if input.json {
                "{\"removed\":true}".into()
            } else {
                "Tunnel profile removed".into()
            })
        }
        "remove-project" => {
            let project = input.operand.ok_or("Specify the local project ID")?;
            let result = core
                .backend
                .remove_project(&store, &project, None)
                .await
                .map_err(|e| e.to_string())?;
            render(&result, input.json)
        }
        "add-project" => {
            let project = input.operand.ok_or("Specify the new project folder")?;
            let mut secrets = SetupSecrets {
                replacement_pairing_code: input.new_code,
                ..Default::default()
            };
            if input.code_stdin {
                secrets.pairing_code = Some(Secret::new(read_stdin_secret()?));
            }
            let existing = store
                .load_environment()
                .map_err(|e| e.to_string())?
                .ok_or("Configure an environment first")?;
            if existing.runner_client_id.is_none()
                && !existing.request.local_server()
                && secrets.pairing_code.is_none()
                && std::io::stdin().is_terminal()
            {
                secrets.pairing_code = Some(Secret::new(secret_prompt(
                    "One-time pairing code for this project machine: ",
                )?));
            }
            let result = core
                .enable_runner(
                    &store,
                    PathBuf::from(project),
                    &secrets,
                    progress_sink(input.json),
                )
                .await
                .map_err(|e| e.to_string())?;
            render(&result, input.json)
        }
        _ => Err(USAGE.into()),
    }
}

fn render(result: &SetupResult, json: bool) -> Result<String, String> {
    if json {
        return serde_json::to_string_pretty(result)
            .map_err(|_| "Could not encode environment status".into());
    }
    let mut lines = vec![
        format!("Server: {}", result.environment.request.server_url),
        format!("Configuration saved: {}", result.environment.configured),
        format!("Server reachable: {}", result.observation.server_reachable),
        format!("User authenticated: {}", result.observation.authenticated),
    ];
    if let Some(online) = result.observation.runner_online {
        lines.push(format!("Runner online: {online}"));
        lines.push(format!(
            "Projects registered and visible: {}",
            result.observation.projects_visible.len()
        ));
    }
    if let Some(fleet) = &result.observation.fleet {
        let display = |value: &str| {
            value
                .chars()
                .filter(|character| !character.is_control())
                .take(4096)
                .collect::<String>()
        };
        lines.push(format!("Authorized Runners: {}", fleet.runners.len()));
        for runner in &fleet.runners {
            lines.push(format!(
                "  {}: connected={}, status={}, GUI available={}",
                display(&runner.client_id),
                runner.connected,
                display(runner.status.as_deref().unwrap_or("unknown")),
                runner.computer_session_availability.unwrap_or(false)
            ));
        }
        lines.push(format!(
            "Authorized projects: {} (available={}, truncated={})",
            fleet.projects.len(),
            fleet.projects_available,
            fleet.projects_truncated
        ));
        for project in &fleet.projects {
            lines.push(format!(
                "  {} on {}: connected={} remote path={}",
                display(&project.id),
                display(&project.client_id),
                project.connected,
                display(project.path.as_deref().unwrap_or("unreported"))
            ));
        }
    }
    lines.extend(
        result
            .observation
            .diagnostics
            .iter()
            .map(ToString::to_string),
    );
    Ok(lines.join("\n"))
}

fn read_tunnel_credentials(path: &Path) -> Result<TunnelCredentials, String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct InputCredentials {
        tunnel_id: String,
        api_key: String,
    }
    let content = read_secret(path).map_err(|error| error.to_string())?;
    let credentials: InputCredentials = serde_json::from_str(content.expose())
        .map_err(|_| "Tunnel credential file must contain only tunnel_id and api_key strings")?;
    Ok(TunnelCredentials {
        tunnel_id: Secret::new(credentials.tunnel_id),
        api_key: Secret::new(credentials.api_key),
    })
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|_| "Current directory unavailable")?
            .join(path))
    }
}
fn discover_binaries(directory: Option<&Path>) -> Result<RuntimeBinaries, String> {
    let exe = std::env::current_exe().map_err(|_| "Could not resolve the installed CLI")?;
    let explicit_directory = directory
        .map(|path| {
            path.canonicalize()
                .map_err(|_| "The runtime directory is unavailable")
        })
        .transpose()?;
    let directory = explicit_directory
        .as_deref()
        .unwrap_or_else(|| exe.parent().unwrap_or(Path::new("/")));
    let name = |stem: &str| directory.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX));
    Ok(RuntimeBinaries {
        cli: if explicit_directory.is_some() {
            name("webcodex")
        } else {
            exe.clone()
        },
        server: name("webcodex-server"),
        runner: name("webcodex-runner"),
    })
}
fn prompt(message: &str) -> Result<String, String> {
    use std::io::{BufRead, Read};
    eprint!("{message}");
    std::io::stderr()
        .flush()
        .map_err(|_| "Prompt unavailable")?;
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(8193)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "Input unavailable")?;
    let value = String::from_utf8(bytes).map_err(|_| "Input must be UTF-8")?;
    if value.len() > 8192 {
        return Err("Input exceeds its bound".into());
    }
    Ok(value.trim().to_string())
}

#[cfg(test)]
mod tests;
fn read_stdin_secret() -> Result<String, String> {
    use std::io::Read;
    let mut value = String::new();
    std::io::stdin()
        .take(8193)
        .read_to_string(&mut value)
        .map_err(|_| "Credential input unavailable")?;
    if value.len() > 8192 || value.trim().is_empty() {
        return Err("Credential input is empty or too large".into());
    }
    Ok(value.trim().to_string())
}
fn secret_prompt(message: &str) -> Result<String, String> {
    #[cfg(unix)]
    {
        struct Restore(libc::termios);
        impl Drop for Restore {
            fn drop(&mut self) {
                unsafe {
                    libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &self.0);
                }
                eprintln!();
            }
        }
        let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, original.as_mut_ptr()) } != 0 {
            return Err(
                "Secure terminal input unavailable; use --token-file or --code-stdin".into(),
            );
        }
        let original = unsafe { original.assume_init() };
        let _restore = Restore(original);
        let mut hidden = original;
        hidden.c_lflag &= !libc::ECHO;
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &hidden) } != 0 {
            return Err("Could not hide credential input".into());
        }
        prompt(message)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::*;
        let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let mut mode = 0;
        if unsafe { GetConsoleMode(input, &mut mode) } == 0
            || unsafe { SetConsoleMode(input, mode & !ENABLE_ECHO_INPUT) } == 0
        {
            return Err("Secure console input unavailable".into());
        }
        let value = prompt(message);
        unsafe {
            SetConsoleMode(input, mode);
        }
        eprintln!();
        value
    }
}

fn progress_sink(json: bool) -> impl FnMut(SetupProgress) {
    move |progress| {
        if json {
            eprintln!("{}", serde_json::json!({"progress": progress}));
        } else {
            eprintln!("{:?}: {:?}", progress.step, progress.state);
        }
    }
}
