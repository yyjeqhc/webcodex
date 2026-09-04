use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use webcodex_core::runner_protocol::ShellJobValidationStep;
use webcodex_runner_registry::RunnerAccess;

pub type ConnectorHostFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Stable, non-secret owner identity projected by root authentication policy.
///
/// The Connector runtime treats this value as opaque. It never interprets
/// root authentication-mechanism categories and never receives plaintext credentials.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectorPrincipalId(String);

impl ConnectorPrincipalId {
    pub fn new(stable_subject: String) -> Result<Self, String> {
        if stable_subject.trim().is_empty() {
            return Err("connector principal id must not be empty".to_string());
        }
        Ok(Self(stable_subject))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorPermission {
    RuntimeRead,
    ProjectRead,
    ProjectWrite,
    JobRun,
}

impl ConnectorPermission {
    pub fn for_capability(capability: &str) -> Self {
        match capability {
            "files_read" | "files_search" | "code_navigate" | "code_impact" | "task_review"
            | "task_list" | "task_resume" => Self::ProjectRead,
            "edits_apply" | "task_finish" => Self::ProjectWrite,
            "checks_run" | "commands_run" | "task_cancel" => Self::JobRun,
            _ => Self::RuntimeRead,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConnectorPermissions {
    pub runtime_read: bool,
    pub project_read: bool,
    pub project_write: bool,
    pub job_run: bool,
}

impl ConnectorPermissions {
    pub fn allows(self, permission: ConnectorPermission) -> bool {
        match permission {
            ConnectorPermission::RuntimeRead => self.runtime_read,
            ConnectorPermission::ProjectRead => self.project_read,
            ConnectorPermission::ProjectWrite => self.project_write,
            ConnectorPermission::JobRun => self.job_run,
        }
    }
}

/// Connector-facing authority projection for one authenticated request.
///
/// `bootstrap` is the project-owner bypass. `global_admin` is deliberately a
/// separate visibility bit and never implies project-grant access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorAccess {
    pub principal: ConnectorPrincipalId,
    pub project_grant_id: Option<String>,
    pub bootstrap: bool,
    pub global_admin: bool,
    pub permissions: ConnectorPermissions,
    pub runner_access: RunnerAccess,
}

impl ConnectorAccess {
    pub fn project_access_allowed(&self, expected_project_grant_id: &str) -> bool {
        self.bootstrap || self.project_grant_id.as_deref() == Some(expected_project_grant_id)
    }

    pub fn allows(&self, permission: ConnectorPermission) -> bool {
        self.permissions.allows(permission)
    }
}

/// Opaque, already-domain-separated transport window binding.
///
/// Root owns raw headers/cookies/session ids and supplies only this hashed
/// identity. Connector uses it solely for explicit continuation binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorWindowId {
    key: String,
    source: String,
}

impl ConnectorWindowId {
    pub fn new(key: String, source: String) -> Result<Self, String> {
        if key.trim().is_empty() || source.trim().is_empty() {
            return Err("connector window key and source must not be empty".to_string());
        }
        Ok(Self { key, source })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Immutable authority metadata used only by the Connector approval state
/// machine. The permission-rule engine remains root-owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorExecutionAuthority {
    pub auto_authorize: bool,
    pub mode: String,
    pub source: String,
    pub resolved_rule: String,
}

#[derive(Clone)]
pub struct ConnectorCallContext {
    pub access: ConnectorAccess,
    pub execution_authority: ConnectorExecutionAuthority,
    pub host: Arc<dyn ConnectorExecutionHost>,
}

#[derive(Debug, Clone)]
pub struct ConnectorCallOutcome {
    pub ok: bool,
    pub body: Value,
    pub http_status: u16,
    pub required_permission: Option<ConnectorPermission>,
    pub protocol_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorTransport {
    Api,
    Mcp,
}

#[derive(Debug, Clone)]
pub struct ConnectorToolRequest {
    pub tool_name: String,
    pub arguments: Value,
    pub transport: ConnectorTransport,
}

#[derive(Debug, Clone)]
pub enum ConnectorToolFailure {
    Permission {
        required: Option<ConnectorPermission>,
        message: String,
    },
    InvalidArguments(String),
    Tool {
        error: Option<String>,
        output: Value,
    },
    Adapter(String),
}

#[derive(Debug, Clone)]
pub struct ConnectorProjectRegistration {
    pub client_id: String,
    pub project_id: String,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConnectorJobRequest {
    pub project: String,
    pub command: String,
    pub timeout_secs: u64,
    pub cwd: Option<String>,
    pub validation_steps: Vec<ShellJobValidationStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorJobSubmission {
    pub job_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorJobHostError {
    Rejected(Option<String>),
    Adapter(String),
    OutcomeUnknown(Option<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorSemanticCheck {
    Format,
    Check,
    Test,
}

impl ConnectorSemanticCheck {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Format => "format",
            Self::Check => "check",
            Self::Test => "test",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorRecipeId {
    Rust,
    Node,
    Python,
    Go,
}

impl ConnectorRecipeId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Node => "node",
            Self::Python => "python",
            Self::Go => "go",
        }
    }
}

/// Root policy adapter used only for operations whose canonical implementation
/// still requires root ToolRuntime or Job authority. Validation planning and
/// evidence classification are package-local and intentionally absent.
pub trait ConnectorExecutionHost: Send + Sync {
    fn invoke_tool(
        &self,
        request: ConnectorToolRequest,
    ) -> ConnectorHostFuture<'_, Result<Value, ConnectorToolFailure>>;

    fn register_isolated_project(
        &self,
        request: ConnectorProjectRegistration,
    ) -> ConnectorHostFuture<'_, Result<(), ConnectorJobHostError>>;

    fn start_execution_job(
        &self,
        request: ConnectorJobRequest,
    ) -> ConnectorHostFuture<'_, Result<ConnectorJobSubmission, ConnectorJobHostError>>;

    fn stop_execution_job(
        &self,
        project: String,
        job_id: String,
    ) -> ConnectorHostFuture<'_, Result<(), ConnectorJobHostError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_admin_does_not_imply_project_grant_access() {
        let access = ConnectorAccess {
            principal: ConnectorPrincipalId::new("user:admin".to_string()).unwrap(),
            project_grant_id: None,
            bootstrap: false,
            global_admin: true,
            permissions: ConnectorPermissions {
                runtime_read: true,
                project_read: true,
                project_write: true,
                job_run: true,
            },
            runner_access: RunnerAccess {
                global_visibility: true,
                owner_bypass: false,
                username: Some("admin".to_string()),
                group: None,
            },
        };
        assert!(!access.project_access_allowed("wc_pgrant_project"));
        assert!(!access.runner_access.owner_bypass);
    }
}
