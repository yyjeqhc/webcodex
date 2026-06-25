use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageKind {
    Text,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub channel: String,
    pub kind: MessageKind,
    pub title: Option<String>,
    pub text: Option<String>,
    pub file_name: Option<String>,
    pub file_path: Option<String>,
    pub file_size: Option<i64>,
    pub mime_type: Option<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Channel {
    pub name: String,
    pub display_name: String,
    pub message_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandAuditRecord {
    pub id: String,
    pub project: String,
    pub command: String,
    pub command_text: Option<String>,
    pub reason: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub approved_at: Option<i64>,
    pub executed_at: Option<i64>,
    pub exit_code: Option<i32>,
    pub stdout_tail: Option<String>,
    pub stderr_tail: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexGoalRecord {
    pub id: String,
    pub project: String,
    pub title: String,
    pub summary: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub closed_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentSpecRecord {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub auth_token: String,
    pub openapi_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentModelProfileRecord {
    pub id: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: Option<f64>,
    pub max_rounds: Option<usize>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSessionRecord {
    pub session_id: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub closed_at: Option<i64>,
    pub first_event_at: Option<i64>,
    pub last_event_at: Option<i64>,
    pub total_actions: i64,
    pub success_count: i64,
    pub failed_count: i64,
    pub timeout_or_unknown_count: i64,
    pub warning_count: i64,
    pub total_duration_ms: i64,
    pub changed_files_count: i64,
    pub job_ids_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionEventRecord {
    pub event_id: String,
    pub session_id: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration_ms: i64,
    pub endpoint: String,
    pub operation: Option<String>,
    pub action_name: String,
    pub project: Option<String>,
    pub status: String,
    pub http_status: Option<i64>,
    pub error_summary: Option<String>,
    pub warning_summary: Option<String>,
    pub changed_files_json: String,
    pub ids_json: String,
    pub summary_json: String,
    pub request_bytes: Option<i64>,
    pub response_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: String,
    pub username: String,
    pub created_at: i64,
    pub disabled: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub key_prefix: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
}
