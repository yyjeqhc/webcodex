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
    /// Optional human-readable name. Phase 2 user model.
    #[serde(default)]
    pub display_name: Option<String>,
    /// `"admin"` or `"user"`. Defaults to `"user"` for legacy rows.
    #[serde(default = "default_user_role")]
    pub role: String,
    /// Optional unix timestamp marking when the user was disabled. Mirrors
    /// the legacy `disabled` flag (disabled != 0 implies disabled_at is set).
    #[serde(default)]
    pub disabled_at: Option<i64>,
    /// Optional unix timestamp of the most recent user metadata update.
    #[serde(default)]
    pub updated_at: Option<i64>,
}

fn default_user_role() -> String {
    "user".to_string()
}

impl UserRecord {
    /// True when the user is disabled (disabled flag set or disabled_at present).
    pub fn is_disabled(&self) -> bool {
        self.disabled != 0 || self.disabled_at.is_some()
    }

    /// True when the user holds the admin role.
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// The kind of an API key. Phase 3 introduces agent tokens that are bound to
/// an owner username and an allowed `client_id`, and may only be used on agent
/// transport endpoints. Existing Phase 2 personal API tokens default to
/// `user`.
pub const TOKEN_KIND_USER: &str = "user";
pub const TOKEN_KIND_AGENT: &str = "agent";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub key_prefix: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
    /// Space-separated scope list (e.g. `"runtime:read project:write"`).
    /// Empty string means no scopes. Phase 2 token model.
    #[serde(default)]
    pub scopes: String,
    /// Optional unix timestamp after which the token must no longer
    /// authenticate. `None` means the token never expires.
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// Phase 3 token kind: `"user"` (default) or `"agent"`. Agent tokens are
    /// bound to an owner username (via `user_id`) and an `allowed_client_id`,
    /// and may only authorize agent transport endpoints. Older rows that
    /// predate the column default to `"user"` via the DB schema and the
    /// [`ApiKeyRecord::kind`] helper.
    #[serde(default)]
    pub kind: String,
    /// Phase 3: the `client_id` an agent token is bound to. Required for
    /// agent tokens; `None` for user tokens. An agent token may only
    /// register/poll/result/job_update for a client_id that matches this
    /// value.
    #[serde(default)]
    pub allowed_client_id: Option<String>,
}

impl ApiKeyRecord {
    /// Parse the stored scope string into an ordered, deduplicated list.
    pub fn scopes_vec(&self) -> Vec<String> {
        self.scopes.split_whitespace().map(str::to_string).collect()
    }

    /// True when the token has been explicitly revoked.
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    /// True when the token has expired relative to `now` (unix seconds).
    pub fn is_expired(&self, now: i64) -> bool {
        self.expires_at.is_some_and(|exp| now >= exp)
    }

    /// The token kind, normalized to `"user"` when unset (legacy rows).
    pub fn kind(&self) -> &str {
        if self.kind.is_empty() {
            TOKEN_KIND_USER
        } else {
            self.kind.as_str()
        }
    }

    /// True when this is an agent token (kind == "agent").
    pub fn is_agent_token(&self) -> bool {
        self.kind() == TOKEN_KIND_AGENT
    }

    /// True when this is a user token (kind == "user", the default).
    pub fn is_user_token(&self) -> bool {
        self.kind() == TOKEN_KIND_USER
    }

    /// The `allowed_client_id` bound to an agent token. `None` for user tokens.
    pub fn allowed_client_id(&self) -> Option<&str> {
        self.allowed_client_id.as_deref()
    }
}

impl Default for ApiKeyRecord {
    /// Convenience default used by tests that build a record field-by-field;
    /// production code constructs the struct explicitly. Defaults `kind` to
    /// `"user"` and `allowed_client_id` to `None`.
    fn default() -> Self {
        Self {
            id: String::new(),
            user_id: String::new(),
            name: String::new(),
            key_prefix: String::new(),
            created_at: 0,
            last_used_at: None,
            revoked_at: None,
            scopes: String::new(),
            expires_at: None,
            kind: TOKEN_KIND_USER.to_string(),
            allowed_client_id: None,
        }
    }
}
