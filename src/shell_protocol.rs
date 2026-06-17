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

fn default_agent_request_kind() -> String {
    "run_shell".to_string()
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
pub struct ShellAgentProjectSummary {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub path: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub git_head: Option<String>,
    #[serde(default)]
    pub git_dirty: Option<bool>,
    pub updated_at: i64,
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
    #[serde(default)]
    pub projects: Option<Vec<ShellAgentProjectSummary>>,
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
    #[serde(default)]
    pub projects: Vec<ShellAgentProjectSummary>,
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
    #[serde(default)]
    pub projects: Option<Vec<ShellAgentProjectSummary>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientProjectsRequest {
    pub client_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellClientProjectsResponse {
    pub success: bool,
    pub client_id: String,
    pub projects: Vec<ShellAgentProjectSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentShellRequest {
    pub request_id: String,
    pub client_id: String,
    #[serde(default = "default_agent_request_kind")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
    #[serde(default)]
    pub create_dirs: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentJobUpdateRequest {
    pub client_id: String,
    pub job_id: String,
    #[serde(default)]
    pub request_id: Option<String>,
    pub status: String,
    #[serde(default)]
    pub stdout_chunk: Option<String>,
    #[serde(default)]
    pub stderr_chunk: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellAgentJobUpdateResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellFileOpRequest {
    pub op: String,
    pub client_id: String,
    pub path: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
    #[serde(default)]
    pub expected_sha256: Option<String>,
    #[serde(default)]
    pub create_dirs: bool,
    #[serde(default = "default_wait_timeout_secs")]
    pub wait_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellFileOpResponse {
    pub success: bool,
    pub op: String,
    pub request_id: String,
    pub client_id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobCodexMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runtime_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobOpRequest {
    pub op: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub job_id: Option<String>,
    #[serde(default)]
    pub since_stdout_line: Option<usize>,
    #[serde(default)]
    pub since_stderr_line: Option<usize>,
    #[serde(default)]
    pub tail_lines: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<ShellJobCodexMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobInfo {
    pub job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub command_preview: String,
    pub status: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<ShellJobCodexMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobOpResponse {
    pub success: bool,
    pub op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stdout_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stderr_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
