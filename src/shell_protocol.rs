use serde::{Deserialize, Serialize};

fn default_shell_true() -> bool {
    true
}

fn default_timeout_secs() -> u64 {
    120
}

fn default_wait_timeout_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientCapabilities {
    #[serde(default = "default_shell_true")]
    pub shell: bool,
    #[serde(default)]
    pub file_read: bool,
    #[serde(default)]
    pub file_write: bool,
    #[serde(default)]
    pub git: bool,
    #[serde(default)]
    pub jobs: bool,
}

impl Default for ShellClientCapabilities {
    fn default() -> Self {
        Self {
            shell: true,
            file_read: false,
            file_write: false,
            git: false,
            jobs: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientRegisterRequest {
    pub client_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub capabilities: Option<ShellClientCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientView {
    pub client_id: String,
    pub display_name: Option<String>,
    pub owner: Option<String>,
    pub hostname: Option<String>,
    pub status: String,
    pub connected: bool,
    pub last_seen: i64,
    pub capabilities: ShellClientCapabilities,
    pub pending_requests: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellClientsResponse {
    pub success: bool,
    pub clients: Vec<ShellClientView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellClientRegisterResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<ShellClientView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRunRequest {
    pub client_id: String,
    #[serde(default)]
    pub cwd: Option<String>,
    pub command: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_wait_timeout_secs")]
    pub wait_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRunResponse {
    pub success: bool,
    pub request_id: String,
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub command_preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentPollRequest {
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentShellRequest {
    pub request_id: String,
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub command: String,
    pub timeout_secs: u64,
    pub requested_by: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellAgentPollResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<ShellAgentShellRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentResultRequest {
    pub client_id: String,
    pub request_id: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellAgentResultResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
