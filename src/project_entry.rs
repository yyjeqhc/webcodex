//! Canonical project onboarding and readiness application service.
//!
//! Configuration lives outside the Git checkout. CLI, Connector HTTP, and the
//! Browser console project the same structured readiness facts; none parse
//! human-formatted output to decide whether coding is ready.

#[path = "project_entry_setup.rs"]
mod setup_service;

use setup_service::{
    create_private_dir, local_readiness, read_private_value, read_project_credential,
    read_toml_optional, validate_agent_credential, validate_existing_agent,
    validate_existing_registration, validate_product_config, validate_profile, ProjectConfig,
};
pub(crate) use setup_service::{resolve_local_task_state, setup};

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};

const DEFAULT_PROFILE: &str = "personal";
const START_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectCommandOptions {
    pub root: PathBuf,
    pub profile: String,
    pub state_dir: Option<PathBuf>,
    pub json: bool,
}

impl ProjectCommandOptions {
    pub(crate) fn current() -> Result<Self, String> {
        Ok(Self {
            root: std::env::current_dir().map_err(|error| format!("cannot read cwd: {error}"))?,
            profile: DEFAULT_PROFILE.to_string(),
            state_dir: None,
            json: false,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProductError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub user_action_required: bool,
    pub next_action: Option<String>,
}

impl ProductError {
    fn new(code: &str, message: impl Into<String>, next_action: Option<&str>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            retryable: matches!(code, "server_unreachable" | "agent_offline"),
            user_action_required: true,
            next_action: next_action.map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SetupReport {
    pub project: String,
    pub connection_url: String,
    pub status: String,
    pub changed: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReadinessStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ReadinessFact {
    pub name: String,
    pub status: ReadinessStatus,
    pub code: String,
    pub summary: String,
    pub next_action: Option<String>,
}

impl ReadinessFact {
    fn pass(name: &str, code: &str, summary: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: ReadinessStatus::Pass,
            code: code.to_string(),
            summary: summary.into(),
            next_action: None,
        }
    }

    fn fail(name: &str, code: &str, summary: impl Into<String>, action: &str) -> Self {
        Self {
            name: name.to_string(),
            status: ReadinessStatus::Fail,
            code: code.to_string(),
            summary: summary.into(),
            next_action: Some(action.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProjectReadiness {
    pub project: Option<String>,
    pub connection: String,
    pub agent: String,
    pub capabilities: String,
    pub ready: bool,
    pub next_action: Option<String>,
    pub findings: Vec<ReadinessFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RemoteProbe {
    Unreachable,
    CredentialRejected,
    Ready,
    AgentOffline,
    ProjectMissing,
    RequiredCapabilityMissing,
    StructuredValidationMissing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalTaskState {
    pub root: PathBuf,
    pub state: PathBuf,
    pub data: PathBuf,
    pub runs: PathBuf,
    pub projects: PathBuf,
    pub cargo_target: PathBuf,
    pub logical_project_id: String,
}

pub(crate) fn parse_options(
    args: &[String],
    command: &str,
) -> Result<ProjectCommandOptions, String> {
    let mut options = ProjectCommandOptions::current()?;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = |index: &mut usize| -> Result<String, String> {
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))
        };
        match flag {
            "--root" => options.root = PathBuf::from(value(&mut index)?),
            "--profile" => options.profile = value(&mut index)?,
            "--state-dir" => options.state_dir = Some(PathBuf::from(value(&mut index)?)),
            "--json" if command != "agent start" => options.json = true,
            "--help" | "-h" => return Err("help requested".to_string()),
            _ => return Err(format!("unknown {command} option '{flag}'")),
        }
        index += 1;
    }
    validate_profile(&options.profile).map_err(|error| error.message)?;
    Ok(options)
}

pub(crate) fn usage() -> &'static str {
    "Usage: webcodex setup [--root PATH] [--profile NAME] [--state-dir PATH] [--json]\n\
       webcodex doctor [--root PATH] [--profile NAME] [--state-dir PATH] [--json]\n\
       webcodex status [--root PATH] [--profile NAME] [--state-dir PATH] [--json]\n\
       webcodex agent start [--root PATH] [--profile NAME] [--state-dir PATH]\n\n\
Run setup in a local Git project. It writes private WebCodex state outside the\n\
checkout and never starts services, modifies Git content, or opens a network\n\
port. `agent start` is the explicit foreground runtime step.\n"
}

pub(crate) fn readiness_with_probe(
    options: &ProjectCommandOptions,
    remote: RemoteProbe,
) -> ProjectReadiness {
    let local = local_readiness(options);
    let config = local.config;
    let paths = local.paths;
    let setup_present = local.setup_present;
    let can_probe_remote = local.can_probe_remote;
    let mut findings = local.findings;
    let project = config.as_ref().map(|config| config.project_name.clone());
    let mut connection = if can_probe_remote {
        "configured".to_string()
    } else if setup_present {
        "invalid".to_string()
    } else {
        "not configured".to_string()
    };
    let mut agent = "unknown".to_string();
    let mut capabilities = "not_ready".to_string();
    let local_complete = can_probe_remote
        && paths.as_ref().is_some_and(|paths| {
            paths.agent_config.is_file()
                && paths.connector_key.is_file()
                && paths.bootstrap_key.is_file()
        })
        && !findings
            .iter()
            .any(|finding| finding.status == ReadinessStatus::Fail);
    if can_probe_remote {
        let runtime = runtime_readiness(project.clone(), remote);
        connection = runtime.connection;
        agent = runtime.agent;
        capabilities = runtime.capabilities;
        findings.extend(runtime.findings);
    }
    if local_complete {
        findings.push(ReadinessFact::pass(
            "Setup",
            "setup_complete",
            "Project setup is complete.",
        ));
    }
    let ready =
        local_complete && connection == "connected" && agent == "online" && capabilities == "ready";
    if ready {
        findings.push(ReadinessFact::pass(
            "Coding access",
            "ready",
            "Coding access is ready.",
        ));
    }
    let next_action = if ready {
        None
    } else {
        findings
            .iter()
            .find(|finding| finding.status == ReadinessStatus::Fail)
            .and_then(|finding| finding.next_action.clone())
            .or_else(|| Some("webcodex agent start".to_string()))
    };
    ProjectReadiness {
        project,
        connection,
        agent,
        capabilities,
        ready,
        next_action,
        findings,
    }
}

pub(crate) fn runtime_readiness(project: Option<String>, probe: RemoteProbe) -> ProjectReadiness {
    let mut findings = vec![ReadinessFact::pass(
        "Connection",
        "server_reachable",
        "WebCodex is reachable.",
    )];
    let (connection, agent, capabilities) = match probe {
        RemoteProbe::Unreachable => {
            findings[0] = ReadinessFact::fail(
                "Connection",
                "server_unreachable",
                "WebCodex is not reachable.",
                "Run webcodex agent start, then retry.",
            );
            ("unreachable", "unknown", "not_ready")
        }
        RemoteProbe::CredentialRejected => {
            findings.push(ReadinessFact::fail(
                "Authentication",
                "project_credential_rejected",
                "WebCodex rejected the configured project credential.",
                "Restore the matching private credential or explicitly rotate the project setup.",
            ));
            ("connected", "unknown", "not_ready")
        }
        RemoteProbe::AgentOffline => {
            findings.push(ReadinessFact::fail(
                "Agent",
                "agent_offline",
                "The local Agent is offline.",
                "Run webcodex agent start.",
            ));
            ("connected", "offline", "not_ready")
        }
        RemoteProbe::ProjectMissing => {
            findings.push(ReadinessFact::fail(
                "Project",
                "project_registration_invalid",
                "The Agent registration does not contain this project.",
                "Stop the Agent, run webcodex setup, then start it again.",
            ));
            ("connected", "online", "not_ready")
        }
        RemoteProbe::RequiredCapabilityMissing => {
            findings.push(ReadinessFact::fail(
                "Capabilities",
                "required_capability_unavailable",
                "The local Agent is missing a required coding capability.",
                "Upgrade the WebCodex Agent and restart it.",
            ));
            ("connected", "online", "not_ready")
        }
        RemoteProbe::StructuredValidationMissing => {
            findings.push(ReadinessFact::fail(
                "Capabilities",
                "structured_validation_unavailable",
                "Structured validation is unavailable.",
                "Upgrade the WebCodex Agent and restart it.",
            ));
            ("connected", "online", "not_ready")
        }
        RemoteProbe::Ready => {
            findings.push(ReadinessFact::pass(
                "Agent",
                "agent_online",
                "The local Agent is online.",
            ));
            findings.push(ReadinessFact::pass(
                "Project",
                "project_registered",
                "The current project is registered.",
            ));
            findings.push(ReadinessFact::pass(
                "Capabilities",
                "required_capabilities_available",
                "Required coding capabilities are available.",
            ));
            findings.push(ReadinessFact::pass(
                "Structured validation",
                "structured_validation_available",
                "Structured validation is available.",
            ));
            ("connected", "online", "ready")
        }
    };
    ProjectReadiness {
        project,
        connection: connection.to_string(),
        agent: agent.to_string(),
        capabilities: capabilities.to_string(),
        ready: probe == RemoteProbe::Ready,
        next_action: (probe != RemoteProbe::Ready).then(|| "webcodex doctor".to_string()),
        findings,
    }
}

pub(crate) async fn collect_readiness(options: &ProjectCommandOptions) -> ProjectReadiness {
    let local = local_readiness(options);
    if !local.can_probe_remote {
        return readiness_with_probe(options, RemoteProbe::Unreachable);
    }
    let (Some(config), Some(paths)) = (local.config, local.paths) else {
        return readiness_with_probe(options, RemoteProbe::Unreachable);
    };
    let key = match read_project_credential(&paths.connector_key) {
        Ok(key) => key,
        Err(_) => return readiness_with_probe(options, RemoteProbe::Unreachable),
    };
    let url = format!("{}/api/connector/readiness", config.server_url());
    let response = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(1))
        .timeout(Duration::from_secs(3))
        .build()
        .ok();
    let Some(client) = response else {
        return readiness_with_probe(options, RemoteProbe::Unreachable);
    };
    let remote = match client
        .post(url)
        .bearer_auth(key)
        .json(&serde_json::json!({}))
        .send()
        .await
    {
        Ok(response)
            if matches!(
                response.status(),
                reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
            ) =>
        {
            return readiness_with_probe(options, RemoteProbe::CredentialRejected);
        }
        Ok(response) if response.status().is_success() => {
            response.json::<ProjectReadiness>().await.ok()
        }
        _ => None,
    };
    match remote {
        Some(remote) => {
            let probe = remote_probe_from_readiness(&remote);
            readiness_with_probe(options, probe)
        }
        None => readiness_with_probe(options, RemoteProbe::Unreachable),
    }
}

pub(crate) fn render_setup_text(report: &SetupReport) -> String {
    let state = if report.status == "already_configured" {
        "already configured"
    } else {
        "configured"
    };
    format!(
        "Project: {}\nConnection: {state} at {}\nAgent: configured, not running\nCapabilities: ready after Agent starts\n\nNext:\n  {}\n",
        report.project, report.connection_url, report.next_action
    )
}

pub(crate) fn render_doctor_text(readiness: &ProjectReadiness) -> String {
    let mut output = format!(
        "WebCodex doctor — {}\n",
        readiness.project.as_deref().unwrap_or("current project")
    );
    for finding in &readiness.findings {
        let status = match finding.status {
            ReadinessStatus::Pass => "pass",
            ReadinessStatus::Warn => "warn",
            ReadinessStatus::Fail => "fail",
        };
        output.push_str(&format!(
            "[{status}] {}: {}\n",
            finding.name, finding.summary
        ));
    }
    output.push_str(if readiness.ready {
        "\nVerdict: Ready\n"
    } else {
        "\nVerdict: Needs action\n"
    });
    if let Some(action) = &readiness.next_action {
        output.push_str(&format!("Next:\n  {action}\n"));
    }
    output
}

pub(crate) fn render_status_text(readiness: &ProjectReadiness) -> String {
    let mut output = format!(
        "Project: {}\nConnection: {}\nAgent: {}\nCapabilities: {}\nCoding access: {}\n",
        readiness.project.as_deref().unwrap_or("not configured"),
        readiness.connection,
        readiness.agent,
        readiness.capabilities,
        if readiness.ready {
            "ready"
        } else {
            "not ready"
        }
    );
    if let Some(action) = &readiness.next_action {
        output.push_str(&format!("\nNext:\n  {action}\n"));
    }
    output
}

pub(crate) fn render_error(error: &ProductError, json: bool) -> String {
    if json {
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": false,
            "error": error
        }))
        .unwrap_or_else(|_| format!("{{\"ok\":false,\"error\":{{\"code\":\"{}\"}}}}", error.code))
    } else {
        let mut output = format!("{}: {}", error.code, error.message);
        if let Some(action) = &error.next_action {
            output.push_str(&format!("\nNext:\n  {action}"));
        }
        output
    }
}

pub(crate) async fn start_agent(options: &ProjectCommandOptions) -> Result<(), ProductError> {
    let readiness = readiness_with_probe(options, RemoteProbe::Unreachable);
    if readiness
        .findings
        .iter()
        .any(|finding| finding.code == "project_not_configured")
    {
        return Err(ProductError::new(
            "project_not_configured",
            "the current project has not been set up",
            Some("Run webcodex setup."),
        ));
    }
    let (expected, paths) = ProjectConfig::resolve(options)?;
    let config = read_toml_optional::<ProjectConfig>(&paths.config)?.ok_or_else(|| {
        ProductError::new(
            "project_not_configured",
            "the current project has not been set up",
            Some("Run webcodex setup."),
        )
    })?;
    validate_product_config(&expected, &config)?;
    validate_existing_agent(&config, &paths)?;
    validate_existing_registration(&config, &paths)?;
    if TcpListener::bind(("127.0.0.1", config.port)).is_err() {
        return Err(ProductError::new(
            "server_unreachable",
            "the configured loopback port is already in use",
            Some("Stop the conflicting process, then run webcodex agent start."),
        ));
    }
    let agent_binary = locate_agent_binary().ok_or_else(|| {
        ProductError::new(
            "required_capability_unavailable",
            "the WebCodex Agent executable is unavailable",
            Some("Install all WebCodex binaries, then run webcodex doctor."),
        )
    })?;
    let bootstrap = read_private_value(&paths.bootstrap_key)?;
    let connector_key = read_project_credential(&paths.connector_key)?;
    validate_agent_credential(&config, &paths, &connector_key)?;
    let server_binary = std::env::current_exe().map_err(|error| {
        ProductError::new(
            "required_capability_unavailable",
            format!("cannot locate the WebCodex executable: {error}"),
            Some("Reinstall WebCodex, then retry."),
        )
    })?;
    let server_log = open_log(&paths.logs.join("server.log"))?;
    let server_error = server_log.try_clone().map_err(io_error)?;
    let mut server_command = Command::new(server_binary);
    server_command
        .arg("serve")
        .current_dir(&paths.state)
        .env_remove("WEBCODEX_ENV_FILE")
        .env("WEBCODEX_ADDR", format!("127.0.0.1:{}", config.port))
        .env("WEBCODEX_DATA", &paths.data)
        .env("WEBCODEX_TOKEN", bootstrap)
        .env("WEBCODEX_SHARED_KEY_ENABLED", "false")
        .env("WEBCODEX_ALLOW_ANONYMOUS", "false")
        .env("WEBCODEX_PUBLIC_URL", config.server_url())
        .env("WEBCODEX_OAUTH2_ENABLED", "false")
        .env("WEBCODEX_QUIC_ENABLED", "false")
        .env("WEBCODEX_CONNECTOR_SURFACE", "task-v1")
        .env(
            "WEBCODEX_CONNECTOR_PROJECT_GRANT_ID",
            config.project_grant_id(&paths),
        )
        .env("WEBCODEX_PROJECT_CREDENTIAL_FILE", &paths.connector_key)
        .env("WEBCODEX_CONNECTOR_PROJECT_ID", &config.logical_project_id)
        .env("WEBCODEX_CONNECTOR_PROJECT_NAME", &config.project_name)
        .env("WEBCODEX_CONNECTOR_WORKSPACE_ID", &config.workspace_id)
        .env(
            "WEBCODEX_CONNECTOR_EXECUTOR_PROJECT",
            config.runtime_project_id(),
        )
        .env("WEBCODEX_CONNECTOR_EXECUTOR_ROOT", &config.root)
        .env("WEBCODEX_CONNECTOR_RUNS_ROOT", &paths.runs)
        .env("WEBCODEX_CONNECTOR_RESULTS_ROOT", &paths.results)
        .env("WEBCODEX_CONNECTOR_PROJECTS_DIR", &paths.projects)
        .env("WEBCODEX_CONNECTOR_PROFILE", &config.profile)
        .stdout(Stdio::from(server_log))
        .stderr(Stdio::from(server_error))
        .kill_on_drop(true);
    let mut server = server_command.spawn().map_err(|_| {
        ProductError::new(
            "server_unreachable",
            "WebCodex could not start",
            Some("Run webcodex doctor."),
        )
    })?;
    wait_for_server(&mut server, &config.server_url(), &connector_key).await?;

    let agent_log = open_log(&paths.logs.join("agent.log"))?;
    let agent_error = agent_log.try_clone().map_err(io_error)?;
    let mut agent = Command::new(agent_binary)
        .arg("--config")
        .arg(&paths.agent_config)
        .current_dir(&paths.state)
        .env_remove("WEBCODEX_TOKEN")
        .env_remove("WEBCODEX_AGENT_TOKEN")
        .stdout(Stdio::from(agent_log))
        .stderr(Stdio::from(agent_error))
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| {
            ProductError::new(
                "agent_offline",
                "the local Agent could not start",
                Some("Run webcodex doctor."),
            )
        })?;
    wait_for_ready(&mut server, &mut agent, options).await?;
    println!(
        "Project: {}\nConnection: connected at {}\nAgent: online\nCoding access: ready\n\nPress Ctrl-C to stop.",
        config.project_name,
        config.server_url()
    );
    tokio::select! {
        _ = tokio::signal::ctrl_c() => Ok(()),
        status = server.wait() => Err(ProductError::new(
            "server_unreachable",
            format!("WebCodex stopped unexpectedly ({:?})", status.ok()),
            Some("Run webcodex doctor."),
        )),
        status = agent.wait() => Err(ProductError::new(
            "agent_offline",
            format!("the local Agent stopped unexpectedly ({:?})", status.ok()),
            Some("Run webcodex doctor."),
        )),
    }
}

fn remote_probe_from_readiness(readiness: &ProjectReadiness) -> RemoteProbe {
    for (code, probe) in [
        (
            "structured_validation_unavailable",
            RemoteProbe::StructuredValidationMissing,
        ),
        (
            "required_capability_unavailable",
            RemoteProbe::RequiredCapabilityMissing,
        ),
        ("project_registration_invalid", RemoteProbe::ProjectMissing),
        ("agent_offline", RemoteProbe::AgentOffline),
    ] {
        if readiness
            .findings
            .iter()
            .any(|finding| finding.code == code && finding.status == ReadinessStatus::Fail)
        {
            return probe;
        }
    }
    if readiness.ready {
        RemoteProbe::Ready
    } else {
        RemoteProbe::Unreachable
    }
}

fn locate_agent_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("WEBCODEX_AGENT_BIN").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    let current = std::env::current_exe().ok()?;
    let parent = current.parent()?;
    for candidate in [
        parent.join(executable_name("webcodex-agent")),
        parent
            .parent()
            .map(|path| path.join(executable_name("webcodex-agent")))
            .unwrap_or_default(),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(executable_name("webcodex-agent")))
        .find(|candidate| candidate.is_file())
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn agent_runtime_available() -> bool {
    #[cfg(test)]
    {
        true
    }
    #[cfg(not(test))]
    {
        locate_agent_binary().is_some()
    }
}

fn open_log(path: &Path) -> Result<File, ProductError> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| {
            ProductError::new(
                "workspace_unavailable",
                "WebCodex could not open its local log",
                Some("Check local filesystem permissions, then retry."),
            )
        })
}

fn io_error(_: std::io::Error) -> ProductError {
    ProductError::new(
        "workspace_unavailable",
        "WebCodex could not prepare local process output",
        Some("Check local filesystem permissions, then retry."),
    )
}

async fn wait_for_server(
    server: &mut Child,
    base_url: &str,
    key: &str,
) -> Result<(), ProductError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(1))
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| {
            ProductError::new(
                "server_unreachable",
                "WebCodex readiness client could not start",
                Some("Run webcodex doctor."),
            )
        })?;
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if server.try_wait().ok().flatten().is_some() {
            break;
        }
        let response = client
            .post(format!("{base_url}/api/connector/readiness"))
            .bearer_auth(key)
            .json(&serde_json::json!({}))
            .send()
            .await;
        if response.is_ok_and(|response| response.status().is_success()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(ProductError::new(
        "server_unreachable",
        "WebCodex did not become reachable",
        Some("Run webcodex doctor."),
    ))
}

async fn wait_for_ready(
    server: &mut Child,
    agent: &mut Child,
    options: &ProjectCommandOptions,
) -> Result<(), ProductError> {
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if server.try_wait().ok().flatten().is_some() {
            return Err(ProductError::new(
                "server_unreachable",
                "WebCodex stopped during startup",
                Some("Run webcodex doctor."),
            ));
        }
        if agent.try_wait().ok().flatten().is_some() {
            return Err(ProductError::new(
                "agent_offline",
                "the local Agent stopped during startup",
                Some("Run webcodex doctor."),
            ));
        }
        if collect_readiness(options).await.ready {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Err(ProductError::new(
        "agent_offline",
        "the local Agent did not become ready",
        Some("Run webcodex doctor."),
    ))
}

#[cfg(test)]
#[path = "project_entry_tests.rs"]
mod tests;
