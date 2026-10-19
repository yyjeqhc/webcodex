//! Parsed commands with only the options needed by their execution path.
//!
//! Setup, provider configuration and installer authorization retain their current
//! adapter while command families migrate. No raw Input reaches these handlers.
use super::*;
use std::collections::BTreeSet;

pub(super) enum Command {
    Inventory(InventoryCommand),
    Control(ControlCommand),
    Other(Box<Input>),
}

pub(super) struct InventoryCommand {
    pub(super) directory: Option<PathBuf>,
    pub(super) output: InventoryOutput,
}

pub(super) enum InventoryOutput {
    Paths { json: bool },
    BackupManifest,
}

pub(super) struct EnvironmentOptions {
    pub(super) directory: Option<PathBuf>,
    pub(super) json: bool,
}

pub(super) struct ExplicitTunnel {
    pub(super) directory: PathBuf,
    pub(super) profile: String,
    pub(super) json: bool,
}

pub(super) enum ServiceTarget {
    Server,
    Runner,
    Tunnel {
        profile: String,
        expected_revision: Option<u64>,
    },
}

pub(super) struct ServiceCommand {
    pub(super) environment: EnvironmentOptions,
    pub(super) target: ServiceTarget,
    pub(super) operation: ServiceOperation,
}

pub(super) enum ControlCommand {
    Service(ServiceCommand),
    DiagnoseTunnel(ExplicitTunnel),
    RecoverTunnel {
        target: ExplicitTunnel,
        request: TunnelRecoveryRequest,
    },
    SetTunnelHost {
        environment: EnvironmentOptions,
        profile: String,
        mode: TunnelHostMode,
    },
}

// Reuse the options recorded by the lexical parser; do not rescan argv or
// infer supplied options from defaults. Other command families keep their
// existing validation until their own typed adapter is migrated.
pub(super) fn validate_options(input: &Input, options: &BTreeSet<&str>) -> Result<(), String> {
    let (allowed, error): (&[&str], &str) = match input.command.as_str() {
        "paths" | "backup-manifest" => (
            &["--environment-dir", "--json"],
            "paths and backup-manifest accept only --environment-dir and --json",
        ),
        "start" | "stop" | "restart" | "repair-credential" | "uninstall-service" => (
            &["--environment-dir", "--json", "--profile", "--expected-revision"],
            "Service control accepts only a component, --profile, --expected-revision, --environment-dir and --json",
        ),
        "tunnel-host" => (
            &["--environment-dir", "--json", "--host"],
            "tunnel-host accepts only PROFILE, --host, --environment-dir and --json",
        ),
        "tunnel-diagnose" => (
            &["--environment-dir", "--json"],
            "tunnel-diagnose accepts only PROFILE, --environment-dir and --json",
        ),
        "recover-tunnel" => (
            &[
                "--environment-dir",
                "--json",
                "--apply",
                "--accept-uncertain-effects",
                "--expected-environment-id",
                "--expected-revision",
                "--expected-run-id",
            ],
            "recover-tunnel accepts only PROFILE, --environment-dir, --json and its recovery confirmation options",
        ),
        _ => return Ok(()),
    };
    if options.iter().any(|option| !allowed.contains(option))
        || matches!(input.command.as_str(), "paths" | "backup-manifest") && input.operand.is_some()
    {
        return Err(error.into());
    }
    Ok(())
}

pub(super) fn parse(args: &[String]) -> Result<Command, String> {
    let input = super::parse(args)?;
    Ok(match input.command.as_str() {
        "paths" | "backup-manifest" => Command::Inventory(InventoryCommand {
            directory: input.directory,
            output: if input.command == "paths" {
                InventoryOutput::Paths { json: input.json }
            } else {
                InventoryOutput::BackupManifest
            },
        }),
        "start" | "stop" | "restart" | "repair-credential" | "uninstall-service" => {
            let target = match input.operand.as_deref() {
                Some("server") if input.profile.is_none() => ServiceTarget::Server,
                Some("runner") if input.profile.is_none() => ServiceTarget::Runner,
                Some("server" | "runner") => {
                    return Err("--profile identifies a Tunnel profile".into())
                }
                Some("tunnel") => ServiceTarget::Tunnel {
                    profile: input.profile.unwrap_or_else(|| "default".into()),
                    expected_revision: input.expected_revision,
                },
                _ => return Err("Specify server, runner or tunnel".into()),
            };
            let operation = match input.command.as_str() {
                "start" => ServiceOperation::Start,
                "stop" => ServiceOperation::Stop,
                "repair-credential" => ServiceOperation::UpdateCredential,
                "uninstall-service" => ServiceOperation::Uninstall,
                _ => ServiceOperation::Restart,
            };
            Command::Control(ControlCommand::Service(ServiceCommand {
                environment: EnvironmentOptions {
                    directory: input.directory,
                    json: input.json,
                },
                target,
                operation,
            }))
        }
        "tunnel-host" => Command::Control(ControlCommand::SetTunnelHost {
            environment: EnvironmentOptions {
                directory: input.directory,
                json: input.json,
            },
            profile: input.operand.ok_or("Specify an exact Tunnel profile")?,
            mode: input
                .tunnel_host
                .ok_or("Specify --host embedded or standalone")?,
        }),
        "tunnel-diagnose" => Command::Control(ControlCommand::DiagnoseTunnel(ExplicitTunnel {
            directory: input
                .directory
                .ok_or("tunnel-diagnose requires an explicit PROFILE and --environment-dir PATH")?,
            profile: input
                .operand
                .ok_or("tunnel-diagnose requires an explicit PROFILE and --environment-dir PATH")?,
            json: input.json,
        })),
        "recover-tunnel" => Command::Control(ControlCommand::RecoverTunnel {
            target: ExplicitTunnel {
                directory: input
                    .directory
                    .ok_or("recover-tunnel requires PROFILE and --environment-dir PATH")?,
                profile: input
                    .operand
                    .ok_or("recover-tunnel requires PROFILE and --environment-dir PATH")?,
                json: input.json,
            },
            // This remains the authoritative Core request. The lexical boundary
            // retains its existing apply, confirmation and exact run-ID checks.
            request: TunnelRecoveryRequest {
                apply: input.apply_recovery,
                accept_uncertain_effects: input.accept_uncertain_effects,
                expected_revision: input.expected_revision,
                expected_environment_id: input.expected_environment_id,
                expected_run_id: input.expected_run_id,
            },
        }),
        _ => Command::Other(Box::new(input)),
    })
}

impl InventoryCommand {
    pub(super) fn run(self) -> Result<String, String> {
        let root = match self.directory.as_deref() {
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
        if matches!(self.output, InventoryOutput::BackupManifest) {
            return serde_json::to_string_pretty(&build_backup_manifest(&inventory))
                .map_err(|_| "Could not serialize backup manifest".into());
        }
        if matches!(self.output, InventoryOutput::Paths { json: true }) {
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
        Ok(lines.join("\n"))
    }
}

impl EnvironmentOptions {
    fn open(&self) -> Result<EnvironmentStore, String> {
        let root = match self.directory.as_deref() {
            Some(path) => absolute(path)?,
            None => absolute(&default_environment_dir().map_err(|error| error.to_string())?)?,
        };
        EnvironmentStore::open(root).map_err(|error| error.to_string())
    }
}

impl ExplicitTunnel {
    fn open_existing(&self) -> Result<EnvironmentStore, String> {
        EnvironmentStore::open_existing(absolute(&self.directory)?)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Selected Environment is no longer available".into())
    }
}

impl ServiceCommand {
    async fn run(self) -> Result<String, String> {
        let store = self.environment.open()?;
        let backend = NativeEnvironment::new().map_err(|error| error.to_string())?;
        let operation = self.operation;
        let status = match self.target {
                ServiceTarget::Tunnel { profile, expected_revision } => {
                let profile = profile.as_str();
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
                    if expected_revision.is_some_and(|revision| saved.as_ref().is_none_or(|entry| entry.revision != revision)) {
                        return Err("The Cloudflare profile changed; reload it before controlling its service".into());
                    }
                    if saved
                        .as_ref()
                        .is_some_and(|entry| entry.host_mode == TunnelHostMode::Embedded)
                        && matches!(operation, ServiceOperation::Start | ServiceOperation::Stop)
                    {
                        return cloudflare::run_profile_action(
                            &store,
                            &backend,
                            profile,
                            if operation == ServiceOperation::Start {
                                "start"
                            } else {
                                "stop"
                            },
                            expected_revision,
                            self.environment.json,
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
                    backend
                        .control_cloudflare_tunnel(&store, &environment_id, &observed, operation)
                        .await
                } else if operation == ServiceOperation::Start {
                    backend.configure_tunnel(&store, profile, None).await
                } else {
                    backend
                        .control_tunnel(&store, profile, operation)
                        .await
                }
                }
                ServiceTarget::Server => backend.control_service(&store, service::Component::Server, operation).await,
                ServiceTarget::Runner => backend.control_service(&store, service::Component::Runner, operation).await,
            }
            .map_err(|e| e.to_string())?;
        if self.environment.json {
            serde_json::to_string_pretty(&status)
                .map_err(|_| "Could not encode service status".into())
        } else {
            Ok(format!("{}: running={:?}", status.id, status.running))
        }
    }
}

impl ControlCommand {
    pub(super) async fn run(self) -> Result<String, String> {
        match self {
            Self::Service(command) => command.run().await,
            Self::SetTunnelHost {
                environment,
                profile,
                mode,
            } => {
                let store = environment.open()?;
                let backend = NativeEnvironment::new().map_err(|error| error.to_string())?;
                backend
                    .set_tunnel_host(&store, &profile, mode)
                    .map_err(|error| error.to_string())?;
                if environment.json {
                    Ok(serde_json::json!({"profile_id":profile,"host_mode":mode,"restart_required":true}).to_string())
                } else {
                    Ok(format!(
                        "{profile}: host={mode:?}; start the selected owner explicitly"
                    ))
                }
            }
            Self::RecoverTunnel { target, request } => {
                let store = target.open_existing()?;
                let backend = NativeEnvironment::new().map_err(|error| error.to_string())?;
                let status = backend
                    .recover_tunnel(&store, &target.profile, request)
                    .await
                    .map_err(|e| e.to_string())?;
                if target.json {
                    serde_json::to_string_pretty(&status)
                        .map_err(|_| "Could not encode recovery diagnosis".into())
                } else {
                    Ok(format!(
                        "{}: {}; Environment={}, revision={}, run={}\n{}",
                        status.profile_id,
                        status.status,
                        status.environment_id.as_deref().unwrap_or("unconfigured"),
                        status.revision,
                        status.run_id.as_deref().unwrap_or("unavailable"),
                        status.next_action
                    ))
                }
            }
            Self::DiagnoseTunnel(target) => {
                let store = target.open_existing()?;
                let backend = NativeEnvironment::new().map_err(|error| error.to_string())?;
                let status = backend
                    .diagnose_tunnel(&store, &target.profile)
                    .map_err(|e| e.to_string())?;
                if target.json {
                    serde_json::to_string_pretty(&status)
                        .map_err(|_| "Could not encode Tunnel diagnostics".into())
                } else {
                    Ok(format!("{}: observed state={:?}, readiness={}, tunnel_ready={}, local_mcp_ready={}, diagnostic={}\n{}",
                status.profile_id, status.observed_state, status.readiness_status, status.tunnel_ready, status.local_mcp_ready,
                status.diagnostic.as_deref().unwrap_or("none"), status.next_action))
                }
            }
        }
    }
}
