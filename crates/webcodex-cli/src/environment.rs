//! Input/output adapter only. All deployment decisions live in the shared Core.
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use webcodex_environment::*;

mod cloudflare;
mod update;

const USAGE: &str = "webcodex environment <COMMAND>\n\nconfigure [--create | --join URL] [--runner] [--runner-name NAME] [--project PATH | --no-project]\n          [--scope user|system]\n          [--code-stdin | --token-file PATH] [--new-pairing-code]\nresume    [--code-stdin] [--new-pairing-code] [--token-file PATH]\ninvite\nadd-project PATH [--code-stdin] [--new-pairing-code]\nremove-project PROJECT_ID\nstatus|doctor\npaths|backup-manifest (read-only metadata; no files or restore)\nstart|stop|restart <server|runner|tunnel> [--profile PROFILE]\nrepair-credential runner\nrepair-user-credential [--token-file PATH]\nuninstall-service <server|runner|tunnel> [--profile PROFILE]\nconfigure-tunnel [PROFILE] [--provider openai|cloudflare_named|cloudflare_quick]\n                 [--host embedded|standalone] [--name NAME] [--autostart true|false]\n                 [--expected-revision N] [--credentials-file PATH | --token-file PATH]\n                 [--public-origin HTTPS_ORIGIN --tunnel-id ID] [--ingress-port PORT]\ncloudflare-status|cloudflare-start|cloudflare-stop PROFILE\ncloudflare-oauth PROFILE --redirect-uri URL [--scopes JSON_ARRAY] [--replace]\ntunnel-status [PROFILE]\ntunnel-host PROFILE --host embedded|standalone\nremove-tunnel [PROFILE] [--expected-revision N]\nupdate status|check|download|apply|resume|rollback (use update --help)\nupgrade-preflight|upgrade-prepare --candidate-dir PATH [--development-build]\nupgrade-finish|upgrade-rollback\ninstaller-authorize --upgrade-receipt PATH --candidate-dir PATH\ninstaller-apply --upgrade-receipt PATH --candidate-dir PATH --installer-file PATH --installer-target TARGET (OS authorization required)\ninstaller-verify --candidate-dir PATH [--installer-target TARGET]\ninstaller-verify-same --candidate-dir PATH --expected-runtime-dir PATH [--installer-target TARGET]\ninstaller-classify --expected-runtime-dir PATH (Windows)\npackage-upgrade-preflight|package-upgrade-prepare|package-upgrade-verify --candidate-dir PATH --expected-runtime-dir PATH (Windows)\npackage-upgrade-finish|package-upgrade-rollback --expected-runtime-dir PATH (Windows)\ninstaller-finish|installer-cancel\n\nPublic environment commands accept --json and --environment-dir PATH.\nInstaller finalization uses only the fixed owner authorization.\nAdvanced: --bin-dir PATH (configure only).\n--runner enables local work without requiring an initial project.\nNew environments default to user services; saved environments retain their manager.\nUser scope: Linux systemd user manager (linger-dependent), macOS login LaunchAgent, Windows signed-in user task.\nSystem scope: boot services with explicit OS authorization; no automatic fallback.\nSame-machine creation issues separate local credentials automatically, without pairing input.\nViewer-only uses a user credential; pairing codes are only for Runner machines.\nOpenAI credentials use hidden input or protected JSON with tunnel_id and api_key.\nNamed Cloudflare uses protected JSON with tunnel_id, public_origin and token, or --token-file plus identity/origin options. Quick Tunnel takes no credentials or saved origin.\nNew Cloudflare profiles default to embedded Server ownership; OpenAI retains its standalone default.\nExisting Cloudflare edits require --expected-revision. Separate Cloudflare services must be selected for startup before installation; stop and uninstall before deselecting them.\nCloudflare OAuth configuration returns a newly issued client secret once; repeat configuration retains the existing secret. Use --replace explicitly to recover a lost secret or change the callback; replacement revokes the previous client authorization.\nRuntime Join: configure --join URL --runner --no-project (hidden terminal pairing input, or --code-stdin).\nJoining starts only this machine’s Runner; it never creates or starts the central Server.\n";
#[derive(Default)]
struct Input {
    command: String,
    create: bool,
    join: Option<String>,
    project: Option<PathBuf>,
    no_project: bool,
    runner: bool,
    runner_name: Option<String>,
    scope: Option<service::ServiceScope>,
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
    tunnel_host: Option<TunnelHostMode>,
    tunnel_provider: Option<crate::ServerTunnelProvider>,
    public_origin: Option<String>,
    tunnel_id: Option<String>,
    name: Option<String>,
    autostart: Option<bool>,
    expected_revision: Option<u64>,
    ingress_port: Option<u16>,
    redirect_uri: Option<String>,
    scopes: Option<Vec<String>>,
    replace: bool,
    upgrade_receipt: Option<PathBuf>,
    installer_file: Option<PathBuf>,
    installer_target: Option<String>,
    expected_runtime_dir: Option<PathBuf>,
    upgrade_target_file: Option<PathBuf>,
    operation_id: Option<String>,
    operation_id_output: bool,
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
            "--create" => input.create = true,
            "--join" => input.join = Some(value(&mut iter)?),
            "--project" => input.project = Some(PathBuf::from(value(&mut iter)?)),
            "--no-project" => input.no_project = true,
            "--runner" => input.runner = true,
            "--runner-name" => input.runner_name = Some(value(&mut iter)?),
            "--scope" => input.scope = Some(value(&mut iter)?.parse()?),
            "--environment-dir" => input.directory = Some(PathBuf::from(value(&mut iter)?)),
            "--bin-dir" => input.bin_dir = Some(PathBuf::from(value(&mut iter)?)),
            "--candidate-dir" => input.candidate_dir = Some(PathBuf::from(value(&mut iter)?)),
            "--token-file" => input.token_file = Some(PathBuf::from(value(&mut iter)?)),
            "--credentials-file" => input.credentials_file = Some(PathBuf::from(value(&mut iter)?)),
            "--profile" => input.profile = Some(value(&mut iter)?),
            "--provider" => {
                input.tunnel_provider =
                    Some(crate::ServerTunnelProvider::parse(&value(&mut iter)?)?)
            }
            "--public-origin" => input.public_origin = Some(value(&mut iter)?),
            "--tunnel-id" => input.tunnel_id = Some(value(&mut iter)?),
            "--name" => input.name = Some(value(&mut iter)?),
            "--autostart" => {
                input.autostart = Some(
                    value(&mut iter)?
                        .parse()
                        .map_err(|_| "--autostart must be true or false")?,
                )
            }
            "--expected-revision" => {
                input.expected_revision = Some(
                    value(&mut iter)?
                        .parse()
                        .map_err(|_| "--expected-revision must be a positive integer")?,
                )
            }
            "--ingress-port" => {
                input.ingress_port = Some(
                    value(&mut iter)?
                        .parse()
                        .map_err(|_| "--ingress-port must be a nonzero port number")?,
                )
            }
            "--redirect-uri" => input.redirect_uri = Some(value(&mut iter)?),
            "--replace" => input.replace = true,
            "--scopes" => {
                input.scopes = Some(
                    serde_json::from_str(&value(&mut iter)?)
                        .map_err(|_| "--scopes must be a JSON string array")?,
                )
            }
            "--host" => {
                input.tunnel_host = Some(match value(&mut iter)?.as_str() {
                    "embedded" => TunnelHostMode::Embedded,
                    "standalone" => TunnelHostMode::Standalone,
                    _ => return Err("--host must be embedded or standalone".into()),
                })
            }
            "--upgrade-target-file" => {
                input.upgrade_target_file = Some(PathBuf::from(value(&mut iter)?))
            }
            "--operation-id" => input.operation_id = Some(value(&mut iter)?),
            "--operation-id-output" => input.operation_id_output = true,
            "--upgrade-receipt" => input.upgrade_receipt = Some(PathBuf::from(value(&mut iter)?)),
            "--installer-file" => input.installer_file = Some(PathBuf::from(value(&mut iter)?)),
            "--installer-target" => input.installer_target = Some(value(&mut iter)?),
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
    if input.scope.is_some() && input.command != "configure" {
        return Err("--scope selects a new environment during configure; resume/control use its saved scope".into());
    }
    if input.runner && input.command != "configure" {
        return Err("--runner applies only to environment configure".into());
    }
    if input.runner_name.is_some() && input.command != "configure" {
        return Err("--runner-name applies only to environment configure".into());
    }
    if input.runner_name.is_some() && !input.runner && input.project.is_none() {
        return Err("--runner-name requires --runner or --project PATH".into());
    }
    webcodex_core::runner_protocol::validate_optional_runner_field(
        &input.runner_name,
        "display_name",
    )?;
    if input.development_build
        && !matches!(
            input.command.as_str(),
            "upgrade-preflight" | "upgrade-prepare"
        )
    {
        return Err("--development-build applies only to upgrade candidate verification".into());
    }
    if input.installer_file.is_some() && input.command != "installer-apply" {
        return Err("--installer-file applies only to verified unified installer handoff".into());
    }
    if input.tunnel_host.is_some()
        && !matches!(input.command.as_str(), "configure-tunnel" | "tunnel-host")
    {
        return Err("--host applies only to configure-tunnel or tunnel-host".into());
    }
    if input.credentials_file.is_some() && input.command != "configure-tunnel" {
        return Err("--credentials-file applies only to configure-tunnel".into());
    }
    cloudflare::validate_input(&input)?;
    if input.upgrade_target_file.is_some() {
        if !matches!(
            input.command.as_str(),
            "upgrade-prepare"
                | "upgrade-finish"
                | "upgrade-rollback"
                | "installer-verify"
                | "installer-classify"
        ) || input.directory.is_none()
            || input.development_build
            || input.upgrade_receipt.is_some()
        {
            return Err("Guarded installer handoff requires an explicit Environment and a supported command".into());
        }
        if input.operation_id_output != (input.command == "upgrade-prepare")
            || input.operation_id.is_some()
                != matches!(
                    input.command.as_str(),
                    "upgrade-finish" | "upgrade-rollback" | "installer-verify"
                )
            || input.operation_id_output && input.json
        {
            return Err(
                "Guarded installer handoff requires an exact operation for follow-up commands"
                    .into(),
            );
        }
        if input
            .operation_id
            .as_ref()
            .is_some_and(|id| !uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == *id))
        {
            return Err("Invalid guarded installer operation".into());
        }
    } else if input.operation_id.is_some() || input.operation_id_output {
        return Err("Guarded installer operation options require --upgrade-target-file".into());
    }
    Ok(input)
}

fn configure_tunnel_host_mode(
    requested: Option<TunnelHostMode>,
    existing: Option<TunnelHostMode>,
) -> TunnelHostMode {
    requested.or(existing).unwrap_or(TunnelHostMode::Standalone)
}

pub(crate) async fn run(args: &[String]) -> Result<String, String> {
    // Setup and update branches carry large domain futures. Keep the public
    // adapter's future small on the default executor/test thread stack.
    Box::pin(run_inner(args)).await.map_err(|error| {
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
    if args.first().map(String::as_str) == Some("update") {
        return update::run(&args[1..]).await;
    }
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
    if matches!(input.command.as_str(), "paths" | "backup-manifest") {
        let mut options = args.iter().skip(1);
        while let Some(option) = options.next() {
            match option.as_str() {
                "--json" => {}
                "--environment-dir" => {
                    options.next().ok_or("Missing environment directory")?;
                }
                _ => {
                    return Err(
                        "paths and backup-manifest accept only --environment-dir and --json".into(),
                    )
                }
            }
        }
        let root = match input.directory.as_deref() {
            Some(path) => absolute(path)?,
            None => default_environment_dir().map_err(|e| e.to_string())?,
        };
        let mut inventory = inspect_environment_paths(&root, None);
        let mut build = webcodex_build_info::machine_build_info("webcodex");
        build.version = env!("CARGO_PKG_VERSION").into();
        inventory.builds.push(BuildObservation {
            source: BuildSource::EntryPoint,
            build: safe_build(&build),
        });
        recompute_revision(&mut inventory);
        if input.command == "backup-manifest" {
            return serde_json::to_string_pretty(&build_backup_manifest(&inventory))
                .map_err(|_| "Could not serialize backup manifest".into());
        }
        if input.json {
            return serde_json::to_string_pretty(&inventory)
                .map_err(|_| "Could not serialize path inventory".into());
        }
        let mut lines = vec![
            "WebCodex locations (configured metadata; running-effective locations are unconfirmed)"
                .to_string(),
        ];
        for entry in inventory.roots.iter().chain(&inventory.entries) {
            let status = serde_json::to_value(entry.status)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "unconfirmed".into());
            let location = entry
                .configured_path
                .as_deref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "no local path".into());
            lines.push(format!("{}: {} ({})", entry.id, location, status));
        }
        for issue in &inventory.issues {
            lines.push(format!("Issue: {}", issue.code));
        }
        return Ok(lines.join("\n"));
    }
    let guarded_handoff = input
        .upgrade_target_file
        .as_deref()
        .map(|path| {
            webcodex_environment::unified_update::WindowsHandoff::from_request_file(path)
                .map_err(|_| "Invalid guarded installer handoff".to_owned())
        })
        .transpose()?;
    if let (Some(handoff), Some(operation)) = (&guarded_handoff, &input.operation_id) {
        if handoff
            .prepared_operation()
            .map_err(|_| "Invalid guarded installer acknowledgement")?
            .as_deref()
            != Some(operation)
            || handoff
                .accepted_operation()
                .map_err(|_| "Invalid guarded installer acknowledgement")?
                .as_deref()
                != Some(operation)
        {
            return Err(
                "Guarded installer follow-up does not match the acknowledged operation".into(),
            );
        }
    }
    let guarded_target = guarded_handoff.as_ref().map(|handoff| {
        let mut target = handoff.selected_target().clone();
        if let Some(operation) = &input.operation_id {
            target.operation_id = Some(operation.clone());
        }
        target
    });
    if input.command == "installer-apply" {
        #[cfg(unix)]
        {
            use webcodex_environment::unified_update::{
                apply_verified_installer, InstallerLaunchNotice, UpdateError,
            };
            let mut options = args.iter().skip(1);
            while let Some(option) = options.next() {
                match option.as_str() {
                    "--json" => {},
                    "--upgrade-receipt" | "--candidate-dir" | "--installer-file" | "--installer-target" => {
                        options.next().ok_or("Missing installer handoff argument")?;
                    }
                    _ => return Err("installer-apply accepts only an owner receipt, candidate directory, verified installer file and exact installer target".into()),
                }
            }
            let receipt = absolute(
                &input
                    .upgrade_receipt
                    .take()
                    .ok_or("--upgrade-receipt is required")?,
            )?;
            let candidate = absolute(
                &input
                    .candidate_dir
                    .take()
                    .ok_or("--candidate-dir is required")?,
            )?;
            let installer = absolute(
                &input
                    .installer_file
                    .take()
                    .ok_or("--installer-file is required")?,
            )?;
            let target = webcodex_environment::unified_update::InstallerTarget::parse(
                input
                    .installer_target
                    .as_deref()
                    .ok_or("--installer-target is required")?,
            )
            .ok_or("invalid --installer-target")?;
            let emit = |notice: &InstallerLaunchNotice| {
                if let Ok(mut bytes) = serde_json::to_vec(notice) {
                    bytes.push(b'\n');
                    let mut stdout = std::io::stdout().lock();
                    // Losing the Desktop acknowledgement must not abort an
                    // already-running package transaction or its recovery.
                    let _ = stdout.write_all(&bytes);
                    let _ = stdout.flush();
                }
            };
            let mut acknowledged = false;
            let result =
                apply_verified_installer(&receipt, &candidate, &installer, target, |notice| {
                    acknowledged = true;
                    emit(&notice);
                })
                .await;
            if !acknowledged {
                emit(&InstallerLaunchNotice::not_started(
                    result
                        .as_ref()
                        .err()
                        .copied()
                        .unwrap_or(UpdateError::RecoveryRequired),
                ));
            }
            return result
                .map(|()| String::new())
                .map_err(|kind| serde_json::json!({"ok":false,"error_kind":kind}).to_string());
        }
        #[cfg(not(unix))]
        return Err(
            "Windows upgrade orchestration belongs to the verified unified NSIS installer".into(),
        );
    }
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
        let package_target = input
            .installer_target
            .as_deref()
            .map(|value| {
                webcodex_environment::unified_update::InstallerTarget::parse(value)
                    .ok_or("Invalid installer package target")
            })
            .transpose()?;
        verify_same_installed_package(&candidate, &runtime, package_target)
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
        let package_target = match input.installer_target.as_deref() {
            Some(value) => Some(
                webcodex_environment::unified_update::InstallerTarget::parse(value)
                    .ok_or("Invalid installer package target")?,
            ),
            None => None,
        };
        let receipt = if input.command == "installer-authorize" {
            let receipt = absolute(
                input
                    .upgrade_receipt
                    .as_deref()
                    .ok_or("Specify the original user's --upgrade-receipt PATH")?,
            )?;
            authorize_prepared_installation_for_target(&receipt, &candidate, package_target).await
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
        if let Some(target) = &guarded_target {
            if receipt.environment_id != target.environment_id
                || receipt.manifest_sha256 != target.manifest_sha256
                || Some(receipt.operation_id.as_str()) != target.operation_id.as_deref()
            {
                return Err(
                    "Guarded installer receipt does not match the selected operation".into(),
                );
            }
        }
        if input.command == "installer-verify" {
            verify_installer_package_target(&receipt, package_target)
                .map_err(|error| error.to_string())?;
        }
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
    let store = if guarded_target.is_some() {
        EnvironmentStore::open_existing(absolute(&root)?)
            .map_err(|e| e.to_string())?
            .ok_or("Selected Environment is no longer available")?
    } else {
        EnvironmentStore::open(absolute(&root)?).map_err(|e| e.to_string())?
    };
    if input.command == "installer-classify" || input.command.starts_with("package-upgrade-") {
        let runtime = absolute(
            input
                .expected_runtime_dir
                .as_deref()
                .ok_or("Specify --expected-runtime-dir PATH")?,
        )?;
        if input.command == "installer-classify" {
            let kind = windows_package::classify(&store, &runtime)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(serde_json::json!({"kind":kind}).to_string());
        }
        match input.command.as_str() {
            "package-upgrade-preflight" | "package-upgrade-prepare" | "package-upgrade-verify" => {
                let candidate = absolute(
                    input
                        .candidate_dir
                        .as_deref()
                        .ok_or("Specify --candidate-dir PATH")?,
                )?;
                match input.command.as_str() {
                    "package-upgrade-preflight" => {
                        let result = windows_package::preflight(&store, &candidate, &runtime)
                            .await
                            .map_err(|e| e.to_string())?;
                        if !result.ready {
                            return Err(serde_json::to_string(&result).unwrap_or_default());
                        }
                    }
                    "package-upgrade-prepare" => {
                        windows_package::prepare(&store, &candidate, &runtime)
                            .await
                            .map_err(|e| e.to_string())?
                    }
                    _ => {
                        windows_package::verify(&store, &candidate, &runtime)
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
            "package-upgrade-finish" => windows_package::finish(&store, &runtime)
                .await
                .map_err(|e| e.to_string())?,
            "package-upgrade-rollback" => {
                windows_package::rollback(&store, &runtime).map_err(|e| e.to_string())?
            }
            _ => return Err("Unknown package upgrade command".into()),
        }
        return Ok("{\"ready\":true}".into());
    }
    let mut core = EnvironmentSetup::new(NativeEnvironment::new().map_err(|e| e.to_string())?);
    match input.command.as_str() {
        "upgrade-preflight" | "upgrade-prepare" => {
            let candidate = input.candidate_dir.ok_or("Specify --candidate-dir PATH")?;
            if let (Some(handoff), Some(target)) = (&guarded_handoff, &guarded_target) {
                let (result, receipt) = core
                    .backend
                    .upgrade_prepare_guarded_with_receipt(&store, &candidate, target)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut exact = target.clone();
                exact.operation_id = Some(receipt.operation_id.clone());
                if handoff.report_prepared(&receipt).is_err()
                    || handoff
                        .wait_for_acceptance(&receipt.operation_id)
                        .await
                        .is_err()
                {
                    // This preparing process owns the operation. The launching
                    // adapter never races an unknown outer installer with rollback.
                    core.backend
                        .upgrade_rollback_guarded(&store, &exact)
                        .await
                        .map_err(|e| e.to_string())?;
                    return Err("Guarded installer handoff was not acknowledged".into());
                }
                if !result.ready {
                    return Err("Guarded installer preparation was rejected".into());
                }
                return Ok(receipt.operation_id);
            }
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
            if let Some(target) = &guarded_target {
                if input.command == "upgrade-finish" {
                    core.backend.upgrade_finish_guarded(&store, target).await
                } else {
                    core.backend.upgrade_rollback_guarded(&store, target).await
                }
            } else if input.command == "upgrade-finish" {
                core.backend.upgrade_finish(&store).await
            } else {
                core.backend.upgrade_rollback(&store).await
            }
            .map_err(|e| e.to_string())?;
            Ok("{\"ready\":true}".into())
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
                if input.project.is_none() && !input.no_project && !input.runner {
                    if !interactive {
                        return Err("Specify --runner, --project PATH, or --no-project".into());
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
                configure_request(
                    &input,
                    resolve_service_scope(&store, input.scope).map_err(|e| e.to_string())?,
                    project,
                    current_account().map_err(|e| e.to_string())?,
                    discover_binaries(input.bin_dir.as_deref())?,
                )?
            };
            let mut secrets = SetupSecrets {
                replacement_pairing_code: input.new_code,
                ..Default::default()
            };
            if input.code_stdin && (request.local_server() || !request.local_runner()) {
                return Err("Runner pairing is only used when joining with a local Runner".into());
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
                let environment_id = store
                    .load_environment()
                    .map_err(|error| error.to_string())?
                    .ok_or("No saved environment is available")?
                    .environment_id;
                let saved = tunnel_profiles(&store)
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .find(|entry| entry.profile_id == profile);
                if saved
                    .as_ref()
                    .is_some_and(|entry| entry.provider != TunnelProvider::Openai)
                {
                    if input.expected_revision.is_some_and(|revision| saved.as_ref().is_none_or(|entry| entry.revision != revision)) {
                        return Err("The Cloudflare profile changed; reload it before controlling its service".into());
                    }
                    if saved
                        .as_ref()
                        .is_some_and(|entry| entry.host_mode == TunnelHostMode::Embedded)
                        && matches!(operation, ServiceOperation::Start | ServiceOperation::Stop)
                    {
                        return cloudflare::run_profile_action(
                            &store,
                            &core.backend,
                            profile,
                            if operation == ServiceOperation::Start {
                                "start"
                            } else {
                                "stop"
                            },
                            input.expected_revision,
                            input.json,
                        )
                        .await;
                    }
                    let observed = cloudflare_tunnel_profile(&store, profile)
                        .map_err(|error| error.to_string())?;
                    if saved.as_ref().is_none_or(|entry| {
                        entry.revision != observed.revision
                            || entry.provider != observed.provider
                            || entry.host_mode != observed.host_mode
                            || entry.effective_configuration_id().as_deref()
                                != Some(observed.configuration_id.as_str())
                    }) {
                        return Err("The Cloudflare profile changed; reload it before controlling its service".into());
                    }
                    core.backend
                        .control_cloudflare_tunnel(&store, &environment_id, &observed, operation)
                        .await
                } else if operation == ServiceOperation::Start {
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
        "cloudflare-status" | "cloudflare-start" | "cloudflare-stop" | "cloudflare-oauth" => {
            cloudflare::run_control(&store, &core.backend, &input).await
        }
        "configure-tunnel" => {
            let profile = input.operand.as_deref().unwrap_or("default");
            if cloudflare::selected_provider(&store, &input)? != crate::ServerTunnelProvider::Openai
            {
                return cloudflare::configure(&store, &core.backend, &input).await;
            }
            let profiles = tunnel_profiles(&store).map_err(|e| e.to_string())?;
            let existing = profiles.iter().find(|entry| entry.profile_id == profile);
            let credentials = if let Some(path) = input.credentials_file {
                Some(read_tunnel_credentials(&absolute(&path)?)?)
            } else if existing.is_some() {
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
            let host_mode = configure_tunnel_host_mode(
                input.tunnel_host,
                existing.map(|entry| entry.host_mode),
            );
            let result = core
                .backend
                .configure_tunnel_profile(
                    &store,
                    profile,
                    input.name.as_deref(),
                    host_mode,
                    input.autostart,
                    input.expected_revision,
                    credentials.as_ref(),
                    host_mode == TunnelHostMode::Standalone,
                )
                .await
                .map_err(|e| e.to_string())?;
            if input.json {
                serde_json::to_string_pretty(&result)
                    .map_err(|_| "Could not encode Tunnel status".into())
            } else {
                Ok(match result.next_action {
                    TunnelConfigurationNextAction::None
                        if result.profile.host_mode == TunnelHostMode::Embedded =>
                    {
                        format!(
                            "{profile}: saved for WebCodex Server; no Server restart is required"
                        )
                    }
                    TunnelConfigurationNextAction::None => format!(
                        "{}: Tunnel and local MCP are ready",
                        result.owner_status.id
                    ),
                    TunnelConfigurationNextAction::StartServer => format!(
                        "{profile}: saved for WebCodex Server; start the Server to supervise it"
                    ),
                    TunnelConfigurationNextAction::RestartServer => format!(
                        "{profile}: saved for WebCodex Server; restart the Server once after configuring all profiles"
                    ),
                    TunnelConfigurationNextAction::StartStandalone => format!(
                        "{profile}: saved; start the separate Tunnel service explicitly"
                    ),
                    TunnelConfigurationNextAction::RestartStandalone => format!(
                        "{profile}: saved; restart the separate Tunnel service explicitly"
                    ),
                })
            }
        }
        "tunnel-host" => {
            let profile = input
                .operand
                .as_deref()
                .ok_or("Specify an exact Tunnel profile")?;
            let mode = input
                .tunnel_host
                .ok_or("Specify --host embedded or standalone")?;
            core.backend
                .set_tunnel_host(&store, profile, mode)
                .map_err(|error| error.to_string())?;
            if input.json {
                Ok(serde_json::json!({"profile_id":profile,"host_mode":mode,"restart_required":true}).to_string())
            } else {
                Ok(format!(
                    "{profile}: host={mode:?}; start the selected owner explicitly"
                ))
            }
        }
        "tunnel-status" => {
            let profile = input.operand.as_deref().unwrap_or("default");
            if tunnel_profiles(&store)
                .map_err(|error| error.to_string())?
                .iter()
                .any(|entry| {
                    entry.profile_id == profile && entry.provider != TunnelProvider::Openai
                })
            {
                return cloudflare::run_profile_action(
                    &store,
                    &core.backend,
                    profile,
                    "status",
                    input.expected_revision,
                    input.json,
                )
                .await;
            }
            let status = core
                .backend
                .tunnel_status(&store, input.operand.as_deref().unwrap_or("default"))
                .map_err(|e| e.to_string())?;
            if input.json {
                serde_json::to_string_pretty(&status)
                    .map_err(|_| "Could not encode Tunnel status".into())
            } else {
                Ok(format!(
                    "{} (Tunnel {}): host={:?}, autostart={}, running={:?}, Tunnel ready={}, local MCP ready={}, Server restart required={}",
                    status.profile_id,
                    status.tunnel_id,
                    status.host_mode,
                    status.autostart,
                    status.service_status.running,
                    status.tunnel_ready,
                    status.local_mcp_ready,
                    status.server_restart_required
                ))
            }
        }
        "remove-tunnel" => {
            core.backend
                .remove_tunnel_at_revision(
                    &store,
                    input.operand.as_deref().unwrap_or("default"),
                    input.expected_revision,
                )
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
                    Some(PathBuf::from(project)),
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

fn configure_request(
    input: &Input,
    service_scope: service::ServiceScope,
    project: Option<PathBuf>,
    account: LocalAccount,
    binaries: RuntimeBinaries,
) -> Result<SetupRequest, String> {
    let (mode, url) = match (input.create, input.join.as_deref()) {
        (true, None) => (
            EnvironmentMode::Create {
                listen: "127.0.0.1:8080".into(),
            },
            "http://127.0.0.1:8080",
        ),
        (false, Some(url)) => (EnvironmentMode::Join, url),
        _ => return Err("Choose exactly one of --create or --join URL".into()),
    };
    Ok(SetupRequest {
        service_scope,
        mode,
        server_url: canonical_server_url(url).map_err(|e| e.to_string())?,
        project,
        runner: input.runner.then_some(true),
        runner_display_name: input.runner_name.clone(),
        account,
        binaries,
    })
}

fn observed_boolean(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "not observed",
    }
}

fn render(result: &SetupResult, json: bool) -> Result<String, String> {
    if json {
        return serde_json::to_string_pretty(result)
            .map_err(|_| "Could not encode environment status".into());
    }
    let mut lines = vec![
        format!(
            "Service scope: {} ({})",
            result.environment.request.service_scope.as_str(),
            result.environment.request.service_scope.lifecycle()
        ),
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
    if let Some(local) = &result.observation.local {
        if let Some(linger) = local.linger_enabled {
            lines.push(format!("Continue after Linux logout (linger): {linger}"));
        }
        lines.push(format!(
            "User credential file: {}",
            local.user_credential_file
        ));
        for row in &local.components {
            let name = row
                .profile
                .as_ref()
                .map(|p| format!("{} [{p}]", row.component))
                .unwrap_or_else(|| row.component.clone());
            if let Some(service) = &row.service {
                lines.push(format!(
                    "{name}: ownership={:?}, running={}, enabled={}",
                    service.ownership,
                    observed_boolean(service.running),
                    observed_boolean(service.enabled)
                ));
            } else {
                lines.push(format!("{name}: status unobserved"));
            }
            if row.component == "tunnel" {
                lines.push(format!(
                    "  Control plane ready: {}; local MCP ready: {}",
                    observed_boolean(row.tunnel_ready),
                    observed_boolean(row.local_mcp_ready)
                ));
            }
            if let Some(error) = &row.diagnostic {
                lines.push(error.to_string());
            }
        }
        lines.push(match local.local_connection_ready {
            Some(ready) => format!(
                "Ready for ChatGPT connection: {} (local evidence only)",
                observed_boolean(Some(ready))
            ),
            None => "Remote Server exposure: not checked by local environment status".into(),
        });
        lines.push("ChatGPT tool scan/use: not observed by environment status".into());
        if local.profiles_truncated {
            lines.push("Tunnel profiles truncated at 16; inspect exact profiles separately".into());
        }
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
