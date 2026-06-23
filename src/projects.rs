use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Executor {
    #[default]
    Local,
    Ssh,
    Agent,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProjectChecks {
    pub fmt: Option<String>,
    pub test: Option<String>,
    pub build: Option<String>,
    pub e2e: Option<String>,
    pub full: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProjectConfig {
    pub path: String,
    #[serde(default)]
    pub executor: Executor,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub ssh_hosts: Vec<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default = "default_true")]
    pub allow_patch: bool,
    #[serde(default)]
    pub allow_command_requests: bool,
    #[serde(default)]
    pub allow_raw_command_requests: bool,
    #[serde(default)]
    pub default_apply_patch_backend: Option<String>,
    #[serde(default)]
    pub allowed_checks: Vec<String>,
    pub checks: Option<ProjectChecks>,
    #[serde(default)]
    pub commands: HashMap<String, String>,
    #[serde(default)]
    pub hooks: HashMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SshConfig {
    #[serde(default)]
    pub control_master: bool,
}

#[derive(Debug, Deserialize)]
pub struct ProjectsConfig {
    #[serde(default)]
    pub ssh: Option<SshConfig>,
    pub projects: HashMap<String, ProjectConfig>,
}

#[derive(Debug, Clone)]
pub struct ProjectsState {
    pub config: Option<Arc<ProjectsConfig>>,
    pub load_error: Option<String>,
    pub config_path: String,
}

impl ProjectsState {
    pub fn loaded(config: ProjectsConfig, config_path: String) -> Self {
        Self {
            config: Some(Arc::new(config)),
            load_error: None,
            config_path,
        }
    }

    pub fn failed(error: String, config_path: String) -> Self {
        Self {
            config: None,
            load_error: Some(error),
            config_path,
        }
    }
}

impl ProjectsConfig {
    pub fn config_path_from_env() -> String {
        std::env::var("PROJECTS_CONFIG").unwrap_or_else(|_| "./projects.toml".to_string())
    }

    pub fn load() -> Result<Self, String> {
        let config_path = Self::config_path_from_env();
        let path = Path::new(&config_path);
        if !path.exists() {
            return Err(format!(
                "Projects config not found at '{}'. Set PROJECTS_CONFIG env var or create projects.toml",
                config_path
            ));
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read projects config: {}", e))?;
        toml::from_str(&content).map_err(|e| format!("Failed to parse projects config: {}", e))
    }

    pub fn available_project_names(&self) -> Vec<String> {
        let mut names = self.projects.keys().cloned().collect::<Vec<_>>();
        names.sort();
        names
    }

    pub fn get_project(&self, name: &str) -> Result<&ProjectConfig, String> {
        self.projects.get(name).ok_or_else(|| {
            format!(
                "Unknown project '{}'. Available projects: {}",
                name,
                self.available_project_names().join(", ")
            )
        })
    }
}

impl ProjectConfig {
    pub fn root(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    pub fn allow_patch(&self) -> bool {
        self.allow_patch
    }

    pub fn is_check_allowed(&self, suite: &str) -> bool {
        self.effective_allowed_checks().iter().any(|c| c == suite)
    }

    pub fn get_check_command(&self, suite: &str) -> Result<String, String> {
        let checks = self
            .checks
            .as_ref()
            .ok_or("No checks configured for this project")?;
        match suite {
            "fmt" => checks.fmt.clone(),
            "test" => checks.test.clone(),
            "build" => checks.build.clone(),
            "e2e" => checks.e2e.clone(),
            "full" => checks.full.clone(),
            _ => None,
        }
        .ok_or_else(|| format!("Check '{}' has no command configured", suite))
    }

    pub fn configured_check_names(&self) -> Vec<String> {
        let Some(checks) = &self.checks else {
            return Vec::new();
        };
        let mut names = Vec::new();
        if checks.fmt.is_some() {
            names.push("fmt".to_string());
        }
        if checks.test.is_some() {
            names.push("test".to_string());
        }
        if checks.build.is_some() {
            names.push("build".to_string());
        }
        if checks.e2e.is_some() {
            names.push("e2e".to_string());
        }
        if checks.full.is_some() {
            names.push("full".to_string());
        }
        names
    }

    pub fn effective_allowed_checks(&self) -> Vec<String> {
        let mut names = if self.allowed_checks.is_empty() {
            self.configured_check_names()
        } else {
            self.allowed_checks.clone()
        };
        names.sort();
        names.dedup();
        names
    }

    pub fn checks_enabled(&self) -> bool {
        !self.effective_allowed_checks().is_empty()
    }

    pub fn is_ssh(&self) -> bool {
        self.executor == Executor::Ssh
    }

    pub fn is_agent(&self) -> bool {
        self.executor == Executor::Agent
    }

    pub fn agent_client_id(&self) -> Result<&str, String> {
        self.client_id
            .as_deref()
            .map(str::trim)
            .filter(|client_id| !client_id.is_empty())
            .ok_or_else(|| "Agent executor requires client_id".to_string())
    }

    /// Build ordered SSH endpoints, preserving legacy host compatibility.
    pub fn ssh_targets(&self) -> Vec<String> {
        let mut hosts = Vec::new();
        if let Some(host) = self.host.as_ref().filter(|h| !h.trim().is_empty()) {
            hosts.push(host.trim().to_string());
        }
        for host in &self.ssh_hosts {
            let host = host.trim();
            if !host.is_empty() && !hosts.iter().any(|h| h == host) {
                hosts.push(host.to_string());
            }
        }
        hosts
            .into_iter()
            .map(|host| match &self.user {
                Some(user) if !user.trim().is_empty() => format!("{}@{}", user, host),
                _ => host,
            })
            .collect()
    }
}
pub fn canonicalize_and_verify(path: &Path, project_root: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Path does not exist: {}", e))?;
    let canonical_root = project_root
        .canonicalize()
        .map_err(|e| format!("Project root does not exist: {}", e))?;
    if !canonical.starts_with(&canonical_root) {
        return Err("Path is outside project directory".to_string());
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::ProjectsConfig;

    #[test]
    fn concise_project_config_defaults_patch_and_checks() {
        let cfg: ProjectsConfig = toml::from_str(
            r#"
            [projects.demo]
            path = "/tmp/demo"

            [projects.demo.checks]
            test = "cargo test"
            build = "cargo build"
            "#,
        )
        .unwrap();
        let project = cfg.projects.get("demo").unwrap();
        assert!(project.allow_patch());
        assert_eq!(
            project.effective_allowed_checks(),
            vec!["build".to_string(), "test".to_string()]
        );
        assert!(project.is_check_allowed("test"));
        assert!(project.is_check_allowed("build"));
    }

    #[test]
    fn explicit_allowed_checks_still_narrows_configured_checks() {
        let cfg: ProjectsConfig = toml::from_str(
            r#"
            [projects.demo]
            path = "/tmp/demo"
            allow_patch = false
            allowed_checks = ["test"]

            [projects.demo.checks]
            fmt = "cargo fmt --check"
            test = "cargo test"
            "#,
        )
        .unwrap();
        let project = cfg.projects.get("demo").unwrap();
        assert!(!project.allow_patch());
        assert_eq!(project.effective_allowed_checks(), vec!["test".to_string()]);
        assert!(!project.is_check_allowed("fmt"));
        assert!(project.is_check_allowed("test"));
    }

    #[test]
    fn agent_project_config_parses_client_id() {
        let cfg: ProjectsConfig = toml::from_str(
            r#"
            [projects.demo]
            executor = "agent"
            client_id = "oe"
            path = "/tmp/demo"

            [projects.demo.commands]
            status = "git status --short"
            "#,
        )
        .unwrap();
        let project = cfg.projects.get("demo").unwrap();
        assert_eq!(project.executor, super::Executor::Agent);
        assert_eq!(project.agent_client_id().unwrap(), "oe");
        assert!(!project.is_ssh());
        assert!(project.is_agent());
    }

    #[test]
    fn project_hooks_default_to_empty() {
        let cfg: ProjectsConfig = toml::from_str(
            r#"
            [projects.demo]
            path = "/tmp/demo"
            "#,
        )
        .unwrap();
        let project = cfg.projects.get("demo").unwrap();
        assert!(project.hooks.is_empty());
    }

    #[test]
    fn project_hooks_parse_command_vectors() {
        let cfg: ProjectsConfig = toml::from_str(
            r#"
            [projects.demo]
            path = "/tmp/demo"

            [projects.demo.hooks]
            doctor = ["git status --short", "git log --oneline -5"]
            precommit = ["cargo fmt --check", "cargo test"]
            "#,
        )
        .unwrap();
        let project = cfg.projects.get("demo").unwrap();
        assert_eq!(
            project.hooks.get("doctor").unwrap(),
            &vec![
                "git status --short".to_string(),
                "git log --oneline -5".to_string()
            ]
        );
        assert_eq!(
            project.hooks.get("precommit").unwrap(),
            &vec!["cargo fmt --check".to_string(), "cargo test".to_string()]
        );
    }
}
