//! Private project setup, registration, and local-state ownership.

use super::{
    agent_runtime_available, LocalTaskState, ProductError, ProjectCommandOptions, ReadinessFact,
    SetupReport,
};
use crate::agent_init::{
    generated_agent_config_toml, AgentInitOptions, DEFAULT_POLL_INTERVAL_MS, TRANSPORT_WEBSOCKET,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const CONFIG_VERSION: u32 = 1;

#[derive(Debug)]
pub(super) struct ProjectPaths {
    pub(super) state: PathBuf,
    pub(super) data: PathBuf,
    pub(super) cache: PathBuf,
    pub(super) cargo_target: PathBuf,
    pub(super) credentials: PathBuf,
    pub(super) projects: PathBuf,
    pub(super) runs: PathBuf,
    pub(super) results: PathBuf,
    pub(super) logs: PathBuf,
    pub(super) config: PathBuf,
    pub(super) bootstrap_key: PathBuf,
    pub(super) connector_key: PathBuf,
    pub(super) agent_config: PathBuf,
}

impl ProjectPaths {
    fn new(state: PathBuf) -> Self {
        let credentials = state.join("credentials");
        let agent = state.join("agent");
        let cache = state.join("cache");
        Self {
            data: state.join("data"),
            cargo_target: cache.join("cargo-target"),
            cache,
            projects: agent.join("projects.d"),
            runs: state.join("runs"),
            results: state.join("results"),
            logs: state.join("logs"),
            config: state.join("project.toml"),
            bootstrap_key: credentials.join("bootstrap-key"),
            connector_key: credentials.join("connector-key"),
            agent_config: agent.join("agent.toml"),
            credentials,
            state,
        }
    }

    fn create(&self) -> Result<(), ProductError> {
        for path in [
            &self.state,
            &self.data,
            &self.cache,
            &self.cargo_target,
            &self.credentials,
            &self.projects,
            &self.runs,
            &self.results,
            &self.logs,
        ] {
            create_private_dir(path)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
enum LocalProjectState {
    NotConfigured,
    Configured {
        config: ProjectConfig,
        paths: ProjectPaths,
    },
    Invalid {
        config: Option<ProjectConfig>,
        paths: ProjectPaths,
        error: ProductError,
    },
    WorkspaceUnavailable {
        config: Option<ProjectConfig>,
        paths: Option<ProjectPaths>,
        error: ProductError,
    },
}

impl LocalProjectState {
    fn invalid(config: Option<ProjectConfig>, paths: ProjectPaths, error: ProductError) -> Self {
        Self::Invalid {
            config,
            paths,
            error,
        }
    }

    fn workspace_unavailable(
        config: Option<ProjectConfig>,
        paths: Option<ProjectPaths>,
        error: ProductError,
    ) -> Self {
        Self::WorkspaceUnavailable {
            config,
            paths,
            error,
        }
    }
}

#[derive(Debug)]
pub(super) struct LocalReadiness {
    pub(super) config: Option<ProjectConfig>,
    pub(super) paths: Option<ProjectPaths>,
    pub(super) setup_present: bool,
    pub(super) can_probe_remote: bool,
    pub(super) findings: Vec<ReadinessFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ProjectConfig {
    pub(super) version: u32,
    pub(super) project_name: String,
    pub(super) root: PathBuf,
    pub(super) profile: String,
    pub(super) port: u16,
    pub(super) logical_project_id: String,
    pub(super) workspace_id: String,
    pub(super) executor_project_id: String,
    pub(super) executor_client_id: String,
}

impl ProjectConfig {
    pub(super) fn resolve(
        options: &ProjectCommandOptions,
    ) -> Result<(Self, ProjectPaths), ProductError> {
        validate_profile(&options.profile)?;
        let root = discover_project_root(&options.root)?;
        let identity = project_identity(&root);
        let project_name = safe_slug(&root);
        let executor_project_id = format!("{project_name}-{}", &identity[..10]);
        let state = resolve_state_path(options, &executor_project_id)?;
        let grant_id = project_grant_identity(&root, &options.profile, &state);
        let port_seed = u16::from_str_radix(&identity[..4], 16).unwrap_or_default();
        let port = 20_000 + (port_seed % 20_000);
        Ok((
            Self {
                version: CONFIG_VERSION,
                project_name,
                root,
                profile: options.profile.clone(),
                port,
                logical_project_id: format!("wc_proj_{}", &identity[..20]),
                workspace_id: format!("wc_ws_{}", &identity[20..40]),
                executor_project_id,
                executor_client_id: format!("local-{}-{}", &identity[..8], &grant_id[10..18]),
            },
            ProjectPaths::new(state),
        ))
    }

    pub(super) fn server_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub(super) fn runtime_project_id(&self) -> String {
        format!(
            "agent:{}:{}",
            self.executor_client_id, self.executor_project_id
        )
    }

    pub(super) fn project_grant_id(&self, paths: &ProjectPaths) -> String {
        project_grant_identity(&self.root, &self.profile, &paths.state)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectRegistration {
    id: String,
    path: String,
    name: String,
    kind: String,
    allow_patch: bool,
    disabled: bool,
}

pub(crate) fn resolve_local_task_state(
    root: &Path,
    profile: &str,
    state_dir: Option<&Path>,
) -> Result<LocalTaskState, String> {
    let options = ProjectCommandOptions {
        root: root.to_path_buf(),
        profile: profile.to_string(),
        state_dir: state_dir.map(Path::to_path_buf),
        json: false,
    };
    let (config, paths) = ProjectConfig::resolve(&options).map_err(|error| error.message)?;
    Ok(LocalTaskState {
        root: config.root,
        state: paths.state,
        data: paths.data,
        runs: paths.runs,
        projects: paths.projects,
        cargo_target: paths.cargo_target,
        logical_project_id: config.logical_project_id,
    })
}

pub(crate) fn setup(options: &ProjectCommandOptions) -> Result<SetupReport, ProductError> {
    let (expected, paths) = ProjectConfig::resolve(options)?;
    let config = match read_toml_optional::<ProjectConfig>(&paths.config)? {
        Some(existing) => {
            validate_product_config(&expected, &existing)?;
            existing
        }
        None => expected,
    };
    validate_existing_agent(&config, &paths)?;
    validate_existing_registration(&config, &paths)?;
    paths.create()?;

    let mut changed = Vec::new();
    let connector_key = if paths.connector_key.is_file() {
        read_project_credential(&paths.connector_key)?
    } else {
        if paths.agent_config.exists() {
            return Err(ProductError::new(
                "project_registration_invalid",
                "existing Agent configuration conflicts with missing authentication material",
                Some("Restore the existing authentication material or remove this incomplete profile, then run webcodex setup."),
            ));
        }
        let value = format!(
            "webcodex_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        write_new_private(&paths.connector_key, format!("{value}\n").as_bytes())?;
        changed.push("Connection".to_string());
        value
    };
    if paths.agent_config.exists() {
        validate_agent_credential(&config, &paths, &connector_key)?;
    }
    if !paths.bootstrap_key.is_file() {
        let value = format!(
            "wc_bootstrap_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        write_new_private(&paths.bootstrap_key, format!("{value}\n").as_bytes())?;
        if !changed.iter().any(|item| item == "Connection") {
            changed.push("Connection".to_string());
        }
    } else {
        let _ = read_private_value(&paths.bootstrap_key)?;
    }

    if !paths.agent_config.is_file() {
        let content = generated_agent_config_toml(&AgentInitOptions {
            server_url: config.server_url(),
            token: Some(connector_key.clone()),
            token_file: None,
            client_id: config.executor_client_id.clone(),
            owner: "local-owner".to_string(),
            display_name: Some(format!("{} local Agent", config.project_name)),
            transport: TRANSPORT_WEBSOCKET.to_string(),
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            projects_dir: paths.projects.clone(),
            output: paths.agent_config.clone(),
            allowed_roots: vec![config.root.clone(), paths.runs.clone(), paths.cache.clone()],
            allow_cwd_anywhere: false,
            overwrite: false,
        })
        .map_err(|message| {
            ProductError::new(
                "project_registration_invalid",
                format!("could not generate Agent configuration: {message}"),
                Some("Correct the reported configuration issue, then run webcodex setup."),
            )
        })?;
        write_new_private(&paths.agent_config, content.as_bytes())?;
        changed.push("Agent".to_string());
    }

    let project_path = registration_path(&config, &paths);
    if !project_path.is_file() {
        let registration = expected_registration(&config);
        let content = toml::to_string_pretty(&registration).map_err(|error| {
            ProductError::new(
                "project_registration_invalid",
                format!("could not serialize project registration: {error}"),
                Some("Run webcodex setup again after correcting the local state error."),
            )
        })?;
        write_new_private(&project_path, content.as_bytes())?;
        changed.push("Project registration".to_string());
    }

    if !paths.config.is_file() {
        let content = toml::to_string_pretty(&config).map_err(|error| {
            ProductError::new(
                "project_registration_invalid",
                format!("could not serialize project configuration: {error}"),
                Some("Run webcodex setup again after correcting the local state error."),
            )
        })?;
        write_new_private(&paths.config, content.as_bytes())?;
        if !changed.iter().any(|item| item == "Connection") {
            changed.insert(0, "Connection".to_string());
        }
    }
    validate_existing_agent(&config, &paths)?;
    validate_agent_credential(&config, &paths, &connector_key)?;
    validate_existing_registration(&config, &paths)?;
    let connection_url = config.server_url();
    Ok(SetupReport {
        project: config.project_name,
        connection_url,
        status: if changed.is_empty() {
            "already_configured".to_string()
        } else {
            "configured".to_string()
        },
        changed,
        next_action: "webcodex doctor".to_string(),
    })
}

pub(super) fn local_readiness(options: &ProjectCommandOptions) -> LocalReadiness {
    match local_project_state(options) {
        LocalProjectState::NotConfigured => LocalReadiness {
            config: None,
            paths: None,
            setup_present: false,
            can_probe_remote: false,
            findings: vec![ReadinessFact::fail(
                "Setup",
                "project_not_configured",
                "No WebCodex setup was found for this project.",
                "webcodex setup",
            )],
        },
        LocalProjectState::Invalid {
            config,
            paths,
            error,
        } => LocalReadiness {
            config,
            paths: Some(paths),
            setup_present: true,
            can_probe_remote: false,
            findings: vec![ReadinessFact::fail(
                if error.code == "project_credential_invalid" {
                    "Authentication"
                } else {
                    "Setup"
                },
                &error.code,
                error.message,
                error
                    .next_action
                    .as_deref()
                    .unwrap_or("Resolve the invalid private project state."),
            )],
        },
        LocalProjectState::WorkspaceUnavailable {
            config,
            paths,
            error,
        } => LocalReadiness {
            config,
            paths,
            setup_present: true,
            can_probe_remote: false,
            findings: vec![ReadinessFact::fail(
                "Workspace",
                "workspace_unavailable",
                error.message,
                error
                    .next_action
                    .as_deref()
                    .unwrap_or("Restore the configured Git workspace."),
            )],
        },
        LocalProjectState::Configured { config, paths } => configured_readiness(config, paths),
    }
}

fn configured_readiness(config: ProjectConfig, paths: ProjectPaths) -> LocalReadiness {
    let mut findings = [
        (
            "Config",
            "config_readable",
            "Project configuration is readable.",
        ),
        (
            "Connection",
            "server_url_valid",
            "The loopback server URL is valid.",
        ),
        (
            "Authentication",
            "authentication_configured",
            "The project credential is present and protected.",
        ),
        (
            "Project",
            "project_registered",
            "Project registration points to the current project.",
        ),
        (
            "Workspace",
            "workspace_available",
            "The Git workspace is available.",
        ),
        (
            "Git",
            "git_workspace_available",
            "Git workspace discovery is valid.",
        ),
    ]
    .into_iter()
    .map(|(name, code, summary)| ReadinessFact::pass(name, code, summary))
    .collect::<Vec<_>>();
    if agent_runtime_available() {
        findings.push(ReadinessFact::pass(
            "Agent runtime",
            "agent_runtime_available",
            "The local Agent runtime is available.",
        ));
    } else {
        findings.push(ReadinessFact::fail(
            "Agent runtime",
            "required_capability_unavailable",
            "The local Agent executable is unavailable.",
            "Install all WebCodex binaries, then retry.",
        ));
    }
    LocalReadiness {
        config: Some(config),
        paths: Some(paths),
        setup_present: true,
        can_probe_remote: true,
        findings,
    }
}

fn local_project_state(options: &ProjectCommandOptions) -> LocalProjectState {
    let state = match state_path_for_readiness(options) {
        Ok(state) => state,
        Err(error) if error.code == "project_not_configured" => {
            return LocalProjectState::NotConfigured;
        }
        Err(error) => {
            return LocalProjectState::workspace_unavailable(None, None, error);
        }
    };
    let paths = ProjectPaths::new(state);
    if !paths.state.exists() {
        return LocalProjectState::NotConfigured;
    }
    if !paths.state.is_dir() {
        return LocalProjectState::invalid(
            None,
            paths,
            invalid_registration("the WebCodex project state path is not a directory"),
        );
    }
    if !paths.config.exists() {
        return if contains_setup_state(&paths) {
            LocalProjectState::invalid(
                None,
                paths,
                invalid_registration(
                    "WebCodex project state exists but its registration is incomplete",
                ),
            )
        } else {
            LocalProjectState::NotConfigured
        };
    }
    let config = match read_toml::<ProjectConfig>(&paths.config) {
        Ok(config) => config,
        Err(error) => return LocalProjectState::invalid(None, paths, error),
    };
    let expected = match ProjectConfig::resolve(options) {
        Ok((expected, _)) => expected,
        Err(error) if error.code == "workspace_unavailable" => {
            return LocalProjectState::workspace_unavailable(Some(config), Some(paths), error);
        }
        Err(error) => return LocalProjectState::invalid(Some(config), paths, error),
    };
    let validation = validate_product_config(&expected, &config)
        .and_then(|_| validate_existing_agent(&config, &paths))
        .and_then(|_| validate_existing_registration(&config, &paths))
        .and_then(|_| read_project_credential(&paths.connector_key))
        .and_then(|credential| validate_agent_credential(&config, &paths, &credential))
        .and_then(|_| read_private_value(&paths.bootstrap_key).map(|_| ()));
    if let Err(error) = validation {
        return LocalProjectState::invalid(Some(config), paths, error);
    }
    LocalProjectState::Configured { config, paths }
}

fn contains_setup_state(paths: &ProjectPaths) -> bool {
    paths.agent_config.exists()
        || paths.connector_key.exists()
        || paths.bootstrap_key.exists()
        || paths.projects.exists()
        || paths.data.exists()
}

fn invalid_registration(message: &str) -> ProductError {
    ProductError::new(
        "project_registration_invalid",
        message,
        Some("Resolve the invalid private state, then run webcodex setup."),
    )
}

pub(super) fn validate_product_config(
    expected: &ProjectConfig,
    actual: &ProjectConfig,
) -> Result<(), ProductError> {
    for (field, same) in [
        ("version", actual.version == CONFIG_VERSION),
        ("project root", actual.root == expected.root),
        ("profile", actual.profile == expected.profile),
        (
            "project identity",
            actual.logical_project_id == expected.logical_project_id,
        ),
    ] {
        if !same {
            return Err(ProductError::new(
                "project_registration_invalid",
                format!("existing project configuration conflicts in field '{field}'"),
                Some("Use the state directory belonging to this project or resolve the conflict manually."),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_existing_agent(
    config: &ProjectConfig,
    paths: &ProjectPaths,
) -> Result<(), ProductError> {
    if !paths.agent_config.exists() {
        return Ok(());
    }
    let value: toml::Value = read_toml(&paths.agent_config)?;
    let expected = [
        ("server_url", config.server_url()),
        ("client_id", config.executor_client_id.clone()),
        (
            "projects_dir",
            paths.projects.to_string_lossy().into_owned(),
        ),
    ];
    for (field, expected) in expected {
        if value.get(field).and_then(toml::Value::as_str) != Some(expected.as_str()) {
            return Err(ProductError::new(
                "project_registration_invalid",
                format!("existing Agent configuration conflicts in field '{field}'"),
                Some(
                    "Resolve the existing configuration conflict; WebCodex will not overwrite it.",
                ),
            ));
        }
    }
    if value
        .get("token")
        .and_then(toml::Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(ProductError::new(
            "project_registration_invalid",
            "existing Agent configuration conflicts in field 'authentication'",
            Some("Restore the existing authentication material; WebCodex will not overwrite it."),
        ));
    }
    Ok(())
}

pub(super) fn validate_agent_credential(
    config: &ProjectConfig,
    paths: &ProjectPaths,
    credential: &str,
) -> Result<(), ProductError> {
    let _ = read_private_value_with_code(&paths.agent_config, "project_credential_invalid")?;
    let value: toml::Value = read_toml(&paths.agent_config)?;
    let agent_credential = value
        .get("token")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| {
            ProductError::new(
                "project_credential_invalid",
                "the Agent project credential is missing or invalid",
                Some("Restore the private credential or explicitly rotate the project setup."),
            )
        })?;
    let verifier =
        crate::auth::ProjectCredentialVerifier::new(config.project_grant_id(paths), credential)
            .map_err(|_| {
                ProductError::new(
                    "project_credential_invalid",
                    "the configured project credential is invalid",
                    Some("Restore the private credential or explicitly rotate the project setup."),
                )
            })?;
    verifier
        .authenticate(agent_credential)
        .map(|_| ())
        .ok_or_else(|| {
            ProductError::new(
            "project_credential_invalid",
            "the Connector and Agent project credentials do not match",
            Some("Restore the matching private credential or explicitly rotate the project setup."),
        )
        })
}

pub(super) fn validate_existing_registration(
    config: &ProjectConfig,
    paths: &ProjectPaths,
) -> Result<(), ProductError> {
    let path = registration_path(config, paths);
    if !path.exists() {
        return Ok(());
    }
    let actual: ProjectRegistration = read_toml(&path)?;
    let expected = expected_registration(config);
    if actual.id != expected.id {
        return registration_conflict("id");
    }
    if actual.path != expected.path {
        return registration_conflict("path");
    }
    if actual.disabled || !actual.allow_patch {
        return registration_conflict("coding access");
    }
    Ok(())
}

fn registration_conflict(field: &str) -> Result<(), ProductError> {
    Err(ProductError::new(
        "project_registration_invalid",
        format!("existing project registration conflicts in field '{field}'"),
        Some("Resolve the existing registration conflict; WebCodex will not overwrite it."),
    ))
}

fn expected_registration(config: &ProjectConfig) -> ProjectRegistration {
    ProjectRegistration {
        id: config.executor_project_id.clone(),
        path: config.root.to_string_lossy().into_owned(),
        name: config.project_name.clone(),
        kind: "auto".to_string(),
        allow_patch: true,
        disabled: false,
    }
}

fn registration_path(config: &ProjectConfig, paths: &ProjectPaths) -> PathBuf {
    paths
        .projects
        .join(format!("{}.toml", config.executor_project_id))
}

pub(super) fn validate_profile(profile: &str) -> Result<(), ProductError> {
    if profile.is_empty()
        || matches!(profile, "." | "..")
        || !profile
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ProductError::new(
            "project_registration_invalid",
            "profile may only contain ASCII letters, digits, '-', '_', and '.'",
            Some("Choose a valid profile name, then retry."),
        ));
    }
    Ok(())
}

fn discover_project_root(input: &Path) -> Result<PathBuf, ProductError> {
    let canonical = input.canonicalize().map_err(|_| {
        ProductError::new(
            "workspace_unavailable",
            "the project path is unavailable",
            Some("Run webcodex setup from an accessible Git project."),
        )
    })?;
    if !canonical.is_dir() {
        return Err(ProductError::new(
            "workspace_unavailable",
            "the project path is not a directory",
            Some("Run webcodex setup from a Git project directory."),
        ));
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&canonical)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|_| {
            ProductError::new(
                "workspace_unavailable",
                "Git is unavailable",
                Some("Install Git, then run webcodex setup."),
            )
        })?;
    if !output.status.success() {
        return Err(ProductError::new(
            "workspace_unavailable",
            "the selected directory is not a supported Git project",
            Some("Run webcodex setup from a Git project directory."),
        ));
    }
    let discovered = String::from_utf8_lossy(&output.stdout).trim().to_string();
    PathBuf::from(discovered).canonicalize().map_err(|_| {
        ProductError::new(
            "workspace_unavailable",
            "the discovered Git root is unavailable",
            Some("Resolve the Git workspace path, then retry."),
        )
    })
}

fn project_identity(root: &Path) -> String {
    format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()))
}

fn project_grant_identity(root: &Path, profile: &str, state: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-project-grant-v1\0");
    hasher.update(root.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(profile.as_bytes());
    hasher.update(b"\0");
    hasher.update(state.to_string_lossy().as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("wc_pgrant_{}", &digest[..24])
}

fn safe_slug(root: &Path) -> String {
    let slug = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['-', '.'])
        .to_string();
    if slug.is_empty() {
        "project".to_string()
    } else {
        slug
    }
}

fn resolve_state_path(
    options: &ProjectCommandOptions,
    executor_project_id: &str,
) -> Result<PathBuf, ProductError> {
    match &options.state_dir {
        Some(path) if path.is_absolute() => Ok(path.clone()),
        Some(path) => std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|_| {
                ProductError::new(
                    "workspace_unavailable",
                    "cannot resolve the requested state directory",
                    Some("Use an accessible absolute --state-dir."),
                )
            }),
        None => Ok(default_state_base()?
            .join(&options.profile)
            .join(executor_project_id)),
    }
}

fn state_path_for_readiness(options: &ProjectCommandOptions) -> Result<PathBuf, ProductError> {
    if let Some(path) = &options.state_dir {
        return Ok(if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()
                .map_err(|_| {
                    ProductError::new(
                        "workspace_unavailable",
                        "cannot resolve the requested state directory",
                        Some("Use an accessible absolute --state-dir."),
                    )
                })?
                .join(path)
        });
    }
    let root = discover_project_root(&options.root)?;
    let identity = project_identity(&root);
    let project_id = format!("{}-{}", safe_slug(&root), &identity[..10]);
    Ok(default_state_base()?
        .join(&options.profile)
        .join(project_id))
}

fn default_state_base() -> Result<PathBuf, ProductError> {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("webcodex/projects"));
    }
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ProductError::new(
                "project_not_configured",
                "HOME is unavailable and no --state-dir was provided",
                Some("Set HOME or pass an absolute --state-dir."),
            )
        })?;
    Ok(PathBuf::from(home).join(".local/state/webcodex/projects"))
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ProductError> {
    let content = std::fs::read_to_string(path).map_err(|_| {
        ProductError::new(
            "project_registration_invalid",
            "a WebCodex configuration file is unreadable",
            Some("Restore readable private state, then retry."),
        )
    })?;
    toml::from_str(&content).map_err(|_| {
        ProductError::new(
            "project_registration_invalid",
            "a WebCodex configuration file is invalid",
            Some("Resolve the invalid configuration; WebCodex will not overwrite it."),
        )
    })
}

pub(super) fn read_toml_optional<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Option<T>, ProductError> {
    if path.is_file() {
        read_toml(path).map(Some)
    } else {
        Ok(None)
    }
}

pub(super) fn read_private_value(path: &Path) -> Result<String, ProductError> {
    read_private_value_with_code(path, "project_registration_invalid")
}

pub(super) fn read_project_credential(path: &Path) -> Result<String, ProductError> {
    let value = read_private_value_with_code(path, "project_credential_invalid")?;
    crate::auth::validate_project_credential(&value).map_err(|_| {
        ProductError::new(
            "project_credential_invalid",
            "the configured project credential is invalid",
            Some("Restore the private credential or explicitly rotate the project setup."),
        )
    })?;
    Ok(value)
}

fn read_private_value_with_code(path: &Path, code: &str) -> Result<String, ProductError> {
    crate::auth::read_protected_secret(path)
        .and_then(|value| {
            (!value.is_empty())
                .then_some(value)
                .ok_or_else(|| "private authentication material is empty".to_string())
        })
        .map_err(|message| {
            ProductError::new(
                code,
                message,
                Some("Restore protected private authentication material, then retry."),
            )
        })
}

pub(super) fn create_private_dir(path: &Path) -> Result<(), ProductError> {
    std::fs::create_dir_all(path).map_err(|_| {
        ProductError::new(
            "project_registration_invalid",
            "WebCodex could not create its private state directory",
            Some("Check local filesystem permissions, then retry."),
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(|_| {
            ProductError::new(
                "project_registration_invalid",
                "WebCodex could not protect its private state directory",
                Some("Check local filesystem permissions, then retry."),
            )
        })?;
    }
    Ok(())
}

fn write_new_private(path: &Path, content: &[u8]) -> Result<(), ProductError> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| {
        ProductError::new(
            "project_registration_invalid",
            "WebCodex refused to overwrite existing project state",
            Some("Resolve the existing state conflict, then retry."),
        )
    })?;
    file.write_all(content).map_err(|_| {
        ProductError::new(
            "project_registration_invalid",
            "WebCodex could not write private project state",
            Some("Check local filesystem permissions, then retry."),
        )
    })
}
