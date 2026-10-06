//! Terminal adapter for the shared unified updater. No Desktop or GUI authority.
use std::io::IsTerminal;
use std::path::PathBuf;
use webcodex_environment::unified_update as unified;
use webcodex_environment::{EnvironmentStore, NativeEnvironment, UpgradeTarget};

mod launcher;

const USAGE: &str = "webcodex environment update <COMMAND>\n\nstatus\ncheck\ndownload --version VERSION\napply --version VERSION --yes\nresume --operation-id ID --yes\nrollback --operation-id ID --yes\n\nAll commands accept --environment-dir PATH and --json.\nLinux apply/resume/rollback require an interactive terminal and system sudo authorization.\nOnly an existing official unified installation is eligible; bare Runner, source, npm and custom Runtime installations use the manual release path.\nmacOS and Windows headless application is not supported by this entry point.\nThis updates only this machine; remote Runners are unchanged.\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Status,
    Check,
    Download,
    Apply,
    Resume,
    Rollback,
}
impl Command {
    fn affects_services(self) -> bool {
        matches!(self, Self::Apply | Self::Resume | Self::Rollback)
    }
}

fn terminal_matches(command: Command, phase: webcodex_environment::UpgradePhase) -> bool {
    match command {
        Command::Apply | Command::Resume => phase == webcodex_environment::UpgradePhase::Committed,
        Command::Rollback => phase == webcodex_environment::UpgradePhase::RolledBack,
        _ => false,
    }
}

#[derive(Debug)]
struct Input {
    command: Command,
    environment_dir: Option<PathBuf>,
    version: Option<String>,
    operation_id: Option<String>,
    yes: bool,
    json: bool,
}

fn parse(args: &[String]) -> Result<Input, &'static str> {
    let command = match args.first().map(String::as_str) {
        Some("status") => Command::Status,
        Some("check") => Command::Check,
        Some("download") => Command::Download,
        Some("apply") => Command::Apply,
        Some("resume") => Command::Resume,
        Some("rollback") => Command::Rollback,
        _ => return Err("invalid_update_command"),
    };
    let mut input = Input {
        command,
        environment_dir: None,
        version: None,
        operation_id: None,
        yes: false,
        json: false,
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if !seen.insert(arg.as_str()) {
            return Err("repeated_update_option");
        }
        match arg.as_str() {
            "--environment-dir" => {
                input.environment_dir =
                    Some(PathBuf::from(iter.next().ok_or("missing_update_option")?))
            }
            "--version" => {
                input.version = Some(iter.next().ok_or("missing_update_option")?.clone())
            }
            "--operation-id" => {
                input.operation_id = Some(iter.next().ok_or("missing_update_option")?.clone())
            }
            "--yes" => input.yes = true,
            "--json" => input.json = true,
            _ => return Err("invalid_update_option"),
        }
    }
    let version_command = matches!(command, Command::Download | Command::Apply);
    let operation_command = matches!(command, Command::Resume | Command::Rollback);
    if version_command != input.version.is_some()
        || operation_command != input.operation_id.is_some()
        || (!command.affects_services() && input.yes)
    {
        return Err("invalid_update_options_for_command");
    }
    if input
        .version
        .as_deref()
        .is_some_and(|value| !unified::stable_version(value))
    {
        return Err("invalid_update_version");
    }
    if input
        .operation_id
        .as_deref()
        .is_some_and(|value| uuid::Uuid::parse_str(value).is_err())
    {
        return Err("invalid_update_operation_id");
    }
    Ok(input)
}

/// Called before resolving/opening any store or preparing a transaction.
fn admit_effect(input: &Input, linux: bool, tty: bool) -> Result<(), &'static str> {
    if !input.command.affects_services() {
        return Ok(());
    }
    if !linux {
        return Err("headless_platform_not_supported");
    }
    if !input.yes {
        return Err("explicit_confirmation_required");
    }
    if !tty {
        return Err("interactive_terminal_required");
    }
    Ok(())
}

fn failure(kind: &str, json: bool) -> String {
    if json {
        serde_json::json!({"schema_version":1,"ok":false,"error_kind":kind}).to_string()
    } else {
        match kind {
            "headless_platform_not_supported" => "Headless application is supported only on Linux. Use the existing native Desktop update or documented manual installer on this platform.",
            "explicit_confirmation_required" => "Confirm the exact update with --yes before applying, continuing or restoring.",
            "interactive_terminal_required" => "This operation requires a Linux terminal (for example SSH with a TTY) and system sudo authorization. No services were changed. Non-interactive sessions can use status, check and download.",
            _ => "The update request could not be admitted. Use environment update --help and review the local update state before retrying.",
        }.into()
    }
}

#[derive(serde::Serialize)]
struct Status {
    schema_version: u16,
    ok: bool,
    environment_id: Option<String>,
    local_server: Option<bool>,
    local_runner: Option<bool>,
    view: unified::UpdateView,
    latest: Option<unified::ReleaseNotice>,
    error_kind: Option<String>,
    headless_apply_supported: bool,
}

fn encode(status: &Status, json: bool) -> Result<String, String> {
    let bytes = serde_json::to_vec(status).map_err(|_| failure("status_unavailable", json))?;
    if bytes.len() > unified::MAX_UPDATE_VIEW_BYTES {
        return Err(failure("status_too_large", json));
    }
    if json {
        return String::from_utf8(bytes).map_err(|_| failure("status_unavailable", true));
    }
    let mut lines = vec![
        format!("Installation: {:?}", status.view.download.installation),
        format!("Download: {:?}", status.view.download.phase),
    ];
    for component in &status.view.installed {
        lines.push(format!(
            "{} installed: {}",
            component.binary,
            component
                .build
                .as_ref()
                .map(|build| format!(
                    "{} / {} / dirty={:?}",
                    build.version,
                    build.git_commit.as_deref().unwrap_or("unknown"),
                    build.git_dirty
                ))
                .unwrap_or_else(|| "unknown".into())
        ));
    }
    if let Some(candidate) = &status.view.candidate {
        lines.push(format!(
            "Verified candidate: {} / {} / {}",
            candidate.version,
            candidate.source_sha,
            candidate.target.as_str()
        ));
    }
    if let Some(latest) = &status.latest {
        lines.push(format!("Latest stable release: {}", latest.version));
    }
    if let Some(upgrade) = &status.view.upgrade {
        lines.push(format!(
            "Upgrade: {:?} / operation {}",
            upgrade.phase, upgrade.operation_id
        ));
        lines.push("An installer handoff is not installation success. Ambiguous operations require the existing Environment recovery path; do not launch another installer.".into());
    }
    if !status.view.blockers.is_empty() {
        lines.push(format!("Update conditions: {:?}", status.view.blockers));
    }
    if let Some(error) = &status.error_kind {
        lines.push(format!("Action error: {error}"));
        lines.push(match error.as_str() {
            "active_tasks" => "Running work prevents replacement. Wait for a suitable window, then explicitly review status and apply again.",
            "task_observation_unavailable" => "Task status could not be confirmed. Restore the saved Server connection and review status before applying.",
            "manual_recovery_required" => "The current installation result is uncertain. Review environment update status/upgrade-preflight and the documented owner recovery procedure. This command will not dispatch another installer or guess that rollback is safe.",
            "upgrade_rolled_back" => "The attempted update did not complete. Core verified that the previous installation was restored; review status before retrying.",
            "headless_dependencies_unavailable" => "The selected installed package format requires trusted system sudo and its existing package tools. Install the missing distribution dependencies; the updater does not switch package managers.",
            "authorization_required" => "System authorization was not obtained. Services were not prepared by this request; explicitly retry after reviewing status.",
            _ => "Review the saved update state and use the documented manual path when this installation is unsupported.",
        }.into());
    }
    if status.view.download.installation != unified::InstallationKind::Managed {
        lines.push("Automatic replacement requires an existing official unified installation. Preserve this installation type and use the documented manual release path.".into());
    }
    lines.push("Only this machine is updated; remote Runner installations are unchanged.".into());
    Ok(lines.join("\n"))
}

fn update_error(error: unified::UpdateError) -> String {
    // The shared enum is the only serialized error source. Never include
    // process stdout/stderr, arguments, private file paths or parse excerpts.
    serde_json::to_value(error)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "status_unavailable".into())
}

pub(super) async fn run(args: &[String]) -> Result<String, String> {
    let json = args.iter().any(|arg| arg == "--json");
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return Ok(USAGE.into());
    }
    let input = parse(args).map_err(|kind| failure(kind, json))?;
    // No cache, ownership, process probe or service effect precedes this gate.
    admit_effect(
        &input,
        cfg!(target_os = "linux"),
        std::io::stdin().is_terminal(),
    )
    .map_err(|kind| failure(kind, json))?;
    let root = match &input.environment_dir {
        Some(path) => {
            super::absolute(path).map_err(|_| failure("environment_unavailable", json))?
        }
        None => webcodex_environment::default_environment_dir()
            .map_err(|_| failure("environment_unavailable", json))?,
    };
    let data =
        unified::default_desktop_data_dir().map_err(|error| failure(&update_error(error), json))?;
    run_at(input, root, data).await
}

async fn run_at(input: Input, root: PathBuf, data: PathBuf) -> Result<String, String> {
    let json = input.json;
    let manager = unified::UpdateManager::with_environment_root(data, root.clone());
    let (mut kind, mut context, builds) = unified::assess_headless_installation(root.clone())
        .await
        .map_err(|error| failure(&update_error(error), json))?;
    if let Some(installation) = &context {
        let caller = webcodex_build_info::machine_build_info("webcodex");
        if std::env::current_exe()
            .ok()
            .and_then(|path| path.canonicalize().ok())
            .as_ref()
            != Some(&installation.binaries.cli)
            || caller.git_dirty != Some(false)
            || caller.git_commit != installation.build.git_commit
            || caller.version != installation.build.version
        {
            kind = unified::InstallationKind::SourceBuild;
            context = None;
        }
    }
    manager.set_installation(kind);
    let initial = manager
        .status_view(&builds)
        .map_err(|error| failure(&update_error(error), json))?;
    let store = EnvironmentStore::open_existing(root.clone())
        .map_err(|_| failure("environment_unavailable", json))?;
    let record = store
        .as_ref()
        .map(EnvironmentStore::load_environment)
        .transpose()
        .map_err(|_| failure("environment_unavailable", json))?
        .flatten();
    let record = record.filter(|record| {
        record.request.account.identity
            == webcodex_environment::current_account()
                .map(|account| account.identity)
                .unwrap_or_default()
            && uuid::Uuid::parse_str(&record.environment_id).is_ok()
    });
    let mut status = Status {
        schema_version: 1,
        ok: true,
        environment_id: record.as_ref().map(|record| record.environment_id.clone()),
        local_server: record.as_ref().map(|record| record.request.local_server()),
        local_runner: record.as_ref().map(|record| record.request.local_runner()),
        view: initial,
        latest: None,
        error_kind: None,
        headless_apply_supported: cfg!(target_os = "linux")
            && kind == unified::InstallationKind::Managed
            && context.as_ref().is_some_and(|context| {
                unified::LaunchAdapter::supported(&launcher::TerminalLauncher, context.target)
            }),
    };
    let action: Result<(), String> = match input.command {
        Command::Status => Ok(()),
        Command::Check => match unified::fetch_latest().await {
            Ok(latest) => {
                status.latest = latest;
                Ok(())
            }
            Err(error) => Err(update_error(error)),
        },
        Command::Download => {
            let version = input.version.as_deref().unwrap();
            let notice = unified::ReleaseNotice {
                version: version.into(),
                runtime_version: version.into(),
                release_url: format!(
                    "https://github.com/{}/releases/tag/v{version}",
                    unified::OFFICIAL_REPOSITORY
                ),
                compatibility: unified::UpdateCompatibility::Unknown,
            };
            let cancellation = unified::CancellationSignal::new();
            let download = manager.download_now(
                Some(notice),
                false,
                true,
                kind,
                unified::detected_installer_target(),
                &cancellation,
            );
            tokio::pin!(download);
            let result = tokio::select! {
                result = &mut download => result,
                _ = tokio::signal::ctrl_c() => {
                    cancellation.cancel();
                    manager.cancel_download(true);
                    download.await
                }
            };
            result.map(|_| ()).map_err(update_error)
        }
        Command::Apply => {
            apply(
                &manager,
                context.as_ref(),
                &status.view,
                input.version.as_deref().unwrap(),
            )
            .await
        }
        Command::Resume | Command::Rollback if context.is_none() => {
            Err("unsupported_installation".into())
        }
        Command::Resume | Command::Rollback => {
            recover(
                store.as_ref(),
                &status.view,
                input.operation_id.as_deref().unwrap(),
                input.command,
            )
            .await
        }
    };
    if let Err(error) = action {
        status.ok = false;
        status.error_kind = Some(error);
    }
    if input.command.affects_services() && status.ok {
        if let Err(error) = reconcile_effect(&manager, &input, &status).await {
            status.ok = false;
            status.error_kind = Some(error);
        }
    }
    // Reobserve authoritative outcome after a download, helper exit or recovery;
    // do not turn a Started acknowledgement into completed installation.
    if !matches!(input.command, Command::Status | Command::Check) {
        let refreshed_builds = unified::assess_headless_installation(root)
            .await
            .map(|(_, _, builds)| builds)
            .unwrap_or_default();
        match manager.status_view(&refreshed_builds) {
            Ok(view) => status.view = view,
            Err(error) => {
                status.ok = false;
                status.error_kind = Some(update_error(error));
            }
        }
    }
    match status.error_kind.as_deref() {
        Some("active_tasks") => {
            status.view.download.can_install = false;
            status
                .view
                .blockers
                .push(unified::UpdateBlocker::ActiveTasks);
        }
        Some("task_observation_unavailable") => {
            status.view.download.can_install = false;
            status
                .view
                .blockers
                .push(unified::UpdateBlocker::TaskObservationUnavailable);
        }
        _ => {}
    }
    let output = encode(&status, json)?;
    if status.ok {
        Ok(output)
    } else {
        Err(output)
    }
}

async fn reconcile_effect(
    manager: &unified::UpdateManager,
    input: &Input,
    initial: &Status,
) -> Result<(), String> {
    let next = manager.status_view(&[]).map_err(update_error)?;
    let Some(operation) = next.upgrade.as_ref() else {
        return Err("manual_recovery_required".into());
    };
    let target_matches = Some(operation.environment_id.as_str())
        == initial.environment_id.as_deref()
        && match input.command {
            Command::Apply => initial.view.candidate.as_ref().is_some_and(|candidate| {
                candidate.version == operation.version
                    && candidate.manifest_sha256 == operation.manifest_sha256
            }),
            Command::Resume | Command::Rollback => {
                initial.view.upgrade.as_ref().is_some_and(|selected| {
                    selected.operation_id == operation.operation_id
                        && selected.manifest_sha256 == operation.manifest_sha256
                        && input.operation_id.as_deref() == Some(selected.operation_id.as_str())
                })
            }
            _ => false,
        };
    if !target_matches {
        return Err("operation_changed".into());
    }
    if !terminal_matches(input.command, operation.phase) {
        return Err(
            if input.command == Command::Apply
                && operation.phase == webcodex_environment::UpgradePhase::RolledBack
            {
                "upgrade_rolled_back".into()
            } else {
                "manual_recovery_required".into()
            },
        );
    }
    manager
        .reconcile_pending_guarded(&UpgradeTarget {
            environment_id: operation.environment_id.clone(),
            manifest_sha256: operation.manifest_sha256.clone(),
            operation_id: Some(operation.operation_id.clone()),
        })
        .await
        .map_err(update_error)
}

async fn apply(
    manager: &unified::UpdateManager,
    context: Option<&unified::InstallContext>,
    view: &unified::UpdateView,
    version: &str,
) -> Result<(), String> {
    let context = context.ok_or("unsupported_installation")?;
    let candidate = view
        .candidate
        .as_ref()
        .filter(|candidate| candidate.version == version)
        .ok_or("candidate_not_verified")?;
    if view.download.pending_install {
        return Err("manual_recovery_required".into());
    }
    let target = UpgradeTarget {
        environment_id: context.environment_id.clone(),
        manifest_sha256: candidate.manifest_sha256.clone(),
        operation_id: None,
    };
    let launcher = launcher::TerminalLauncher;
    if !unified::LaunchAdapter::supported(&launcher, candidate.target) {
        return Err("headless_dependencies_unavailable".into());
    }
    let store = EnvironmentStore::open_existing(context.environment_root.clone())
        .map_err(|_| "environment_unavailable")?
        .ok_or("environment_not_configured")?;
    let backend = NativeEnvironment::new().map_err(|_| "task_observation_unavailable")?;
    match backend
        .upgrade_task_count(&store, &context.environment_id)
        .await
    {
        Ok(0) => {}
        Ok(_) => return Err("active_tasks".into()),
        Err(_) => return Err("task_observation_unavailable".into()),
    }
    #[cfg(target_os = "linux")]
    {
        unified::verify_installed_update_cli(&context.binaries.cli).map_err(update_error)?;
        // Obtain normal terminal authorization before pausing owned services.
        // Only the narrow trusted helper is elevated during the later handoff.
        let status = tokio::process::Command::new("/usr/bin/sudo")
            .arg("--validate")
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .status()
            .await
            .map_err(|_| "authorization_required")?;
        if !status.success() {
            return Err("authorization_required".into());
        }
    }
    match manager
        .install_checked(context, candidate, &target, true, &launcher)
        .await
        .map_err(update_error)?
    {
        unified::LaunchOutcome::Started => Ok(()),
        unified::LaunchOutcome::NotStarted(error) => Err(update_error(error)),
        unified::LaunchOutcome::Unknown | unified::LaunchOutcome::StartedWithOperation(_) => {
            Err("manual_recovery_required".into())
        }
    }
}

async fn recover(
    store: Option<&EnvironmentStore>,
    view: &unified::UpdateView,
    operation_id: &str,
    command: Command,
) -> Result<(), String> {
    let store = store.ok_or("environment_not_configured")?;
    let operation = view
        .upgrade
        .as_ref()
        .filter(|operation| operation.operation_id == operation_id)
        .ok_or("operation_changed")?;
    let target = UpgradeTarget {
        environment_id: operation.environment_id.clone(),
        manifest_sha256: operation.manifest_sha256.clone(),
        operation_id: Some(operation_id.into()),
    };
    let mut backend = NativeEnvironment::new().map_err(|_| "recovery_unavailable")?;
    let result = match command {
        Command::Resume => {
            backend
                .upgrade_finish_headless_guarded(store, &target)
                .await
        }
        Command::Rollback => {
            backend
                .upgrade_rollback_headless_guarded(store, &target)
                .await
        }
        _ => return Err("invalid_update_command".into()),
    };
    result.map_err(|diagnostic| match diagnostic.code.as_str() {
        "upgrade_target_changed" => "operation_changed".into(),
        "upgrade_manual_recovery" => "manual_recovery_required".into(),
        _ => "recovery_unavailable".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(value: &str) -> Result<Input, &'static str> {
        parse(
            &value
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        )
    }
    #[test]
    fn parser_binds_only_required_public_targets() {
        for command in [
            "status",
            "check",
            "download --version 1.2.3",
            "apply --version 1.2.3 --yes",
            "resume --operation-id 11111111-1111-4111-8111-111111111111 --yes",
            "rollback --operation-id 11111111-1111-4111-8111-111111111111 --yes",
        ] {
            assert!(input(command).is_ok(), "{command}");
        }
        for command in [
            "status --version 1.2.3",
            "download",
            "check --yes",
            "apply --version 1.2.3 --version 1.2.4",
            "apply --version 1.2.3 --candidate-dir arbitrary",
            "resume --operation-id arbitrary",
            "apply --version 1.2.3-alpha",
            "status --url https://example.org",
        ] {
            assert!(input(command).is_err(), "{command}");
        }
    }
    #[test]
    fn non_tty_and_non_linux_effects_fail_before_store_access() {
        for command in [
            "apply --version 1.2.3 --yes",
            "resume --operation-id 11111111-1111-4111-8111-111111111111 --yes",
            "rollback --operation-id 11111111-1111-4111-8111-111111111111 --yes",
        ] {
            let request = input(command).unwrap();
            assert_eq!(
                admit_effect(&request, true, false),
                Err("interactive_terminal_required")
            );
            assert_eq!(
                admit_effect(&request, false, true),
                Err("headless_platform_not_supported")
            );
            assert!(admit_effect(&request, true, true).is_ok());
        }
        for command in ["status", "check", "download --version 1.2.3"] {
            assert!(admit_effect(&input(command).unwrap(), false, false).is_ok());
        }
        assert_eq!(
            admit_effect(&input("apply --version 1.2.3").unwrap(), true, true),
            Err("explicit_confirmation_required")
        );
    }
    #[test]
    fn effect_success_requires_the_terminal_state_requested_by_the_command() {
        use webcodex_environment::UpgradePhase;
        assert!(terminal_matches(Command::Apply, UpgradePhase::Committed));
        assert!(!terminal_matches(Command::Apply, UpgradePhase::RolledBack));
        assert!(terminal_matches(Command::Resume, UpgradePhase::Committed));
        assert!(!terminal_matches(Command::Resume, UpgradePhase::RolledBack));
        assert!(terminal_matches(
            Command::Rollback,
            UpgradePhase::RolledBack
        ));
        assert!(!terminal_matches(
            Command::Rollback,
            UpgradePhase::Committed
        ));
    }

    #[tokio::test]
    async fn absent_status_does_not_create_environment_or_desktop_cache() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("unconfigured");
        let data = temp.path().join("desktop-app-data");
        let output = run_at(input("status --json").unwrap(), root.clone(), data.clone())
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert!(value["environment_id"].is_null());
        assert!(value["latest"].is_null());
        assert!(value["view"]["candidate"].is_null());
        assert!(value["view"]["upgrade"].is_null());
        assert!(!root.exists());
        assert!(!data.exists());
        assert!(output.len() <= unified::MAX_UPDATE_VIEW_BYTES);
    }
    #[tokio::test]
    async fn errors_do_not_echo_private_canary_arguments() {
        let args = ["apply", "--api-key", "private-canary-secret", "--json"].map(str::to_owned);
        let output = run(&args).await.unwrap_err();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["ok"], false);
        assert!(!output.contains("private-canary-secret"));
    }
}
