use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ErrorComponent {
    Runtime,
    Server,
    Runner,
    Service,
    Tunnel,
    Auth,
    Project,
    Environment,
    Migration,
    Config,
    Desktop,
    Unknown,
}

impl ErrorComponent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Server => "server",
            Self::Runner => "runner",
            Self::Service => "service",
            Self::Tunnel => "tunnel",
            Self::Auth => "auth",
            Self::Project => "project",
            Self::Environment => "environment",
            Self::Migration => "migration",
            Self::Config => "config",
            Self::Desktop => "desktop",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse_safe(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "runtime" => Some(Self::Runtime),
            "server" => Some(Self::Server),
            "runner" => Some(Self::Runner),
            "service" => Some(Self::Service),
            "tunnel" => Some(Self::Tunnel),
            "auth" => Some(Self::Auth),
            "project" => Some(Self::Project),
            "environment" => Some(Self::Environment),
            "migration" => Some(Self::Migration),
            "config" => Some(Self::Config),
            "desktop" => Some(Self::Desktop),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKind {
    AutoRecovering,
    UserAction,
    ManualCheck,
    ConfirmationRequired,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum SafeRecoveryAction {
    WaitAutoRecovery,
    Retry,
    RefreshState,
    StartRuntime,
    StartServer,
    RestartServer,
    StartRunner,
    RestartRunner,
    RepairRunnerCredential,
    RepairUserCredential,
    CheckConnection,
    OpenNetworkSettings,
    OpenRuntimeSettings,
    OpenDiagnostics,
    OpenActivity,
    OpenProjects,
    OpenSetup,
    InspectServiceOwnership,
    ResolveMigration,
    ObserveBeforeRetry,
    RestoreConfiguration,
    ManageProviders,
    #[serde(rename = "none")]
    NoAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorDiagnostic {
    pub component: ErrorComponent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    pub recovery_kind: RecoveryKind,
    pub safe_action: SafeRecoveryAction,
    pub retry_allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_sequence: Option<u64>,
}

impl ErrorDiagnostic {
    pub fn classify(code: &str) -> Self {
        use ErrorComponent::*;
        use RecoveryKind::*;
        use SafeRecoveryAction::*;

        let (component, recovery_kind, safe_action, retry_allowed) = match code {
            // 1. Environment & runtime dependency missing / incompatible
            "binaries_missing"
            | "binary_missing"
            | "runtime_unavailable"
            | "runtime_incompatible"
            | "runtime_missing"
            | "runtime_binary_missing"
            | "runtime_binary_invalid"
            | "runtime_binary_unexecutable"
            | "runtime_directory_invalid"
            | "runtime_directory_missing"
            | "runtime_probe_failed"
            | "runtime_protocol_incompatible"
            | "selected_runtime_not_active"
            | "webcodex_command_start_failed"
            | "service_missingprerequisite"
            | "upgrade_package_flavor"
            | "installer_package_target"
            | "upgrade_package_ownership" => (Runtime, UserAction, OpenRuntimeSettings, false),

            // 2 & 3. Server start failure, exit, unreachable, port occupied
            "server_port_in_use"
            | "local_port_unavailable"
            | "listen_in_use"
            | "listen_address_in_use"
            | "port_in_use" => (Server, UserAction, OpenDiagnostics, false),
            "server_start_failed" | "server_exited" => (Server, UserAction, StartServer, true),
            "server_not_ready"
            | "server_unreachable"
            | "workspace_server_unreachable"
            | "server_identity_contract" => (Server, UserAction, StartServer, true),
            "server_not_owned" | "server_binding_conflict" | "server_admin_required" => {
                (Server, ManualCheck, InspectServiceOwnership, false)
            }

            // 4. Server HTTP request failures & workspace API failures
            // These originate from HTTP POST handlers, including non-idempotent
            // setup and workspace operations. An error response alone does not
            // prove the write was unapplied. Offer diagnostics, not blind replay.
            "server_request_failed" | "workspace_unavailable" => {
                (Server, ManualCheck, OpenDiagnostics, false)
            }
            "server_conflict" => (Server, ConfirmationRequired, ObserveBeforeRetry, false),
            "response_limit" | "response_invalid" => (Server, ManualCheck, OpenDiagnostics, false),

            // 5. Runner offline, process failure, ownership / identity mismatch
            "runner_offline" => (Runner, AutoRecovering, WaitAutoRecovery, true),
            "runner_not_ready" | "runner_start_failed" | "runner_exited" => {
                (Runner, UserAction, StartRunner, true)
            }
            "runner_not_owned"
            | "runner_identity_mismatch"
            | "runner_identity"
            | "enrollment_binding_conflict" => {
                (Runner, ManualCheck, InspectServiceOwnership, false)
            }
            "runner_enrollment_required" | "local_runner_required" => {
                (Runner, UserAction, OpenSetup, false)
            }
            "runner_removal_explicit" => (Runner, ManualCheck, OpenRuntimeSettings, false),
            "runner_settings_changed" => (Runner, UserAction, RefreshState, false),

            // 6. Windows/macOS service management, ownership, credentials, elevation
            "service_foreignservice"
            | "service_ownershipunknown"
            | "service_owner_conflict"
            | "service_scope_conflict" => (Service, ManualCheck, InspectServiceOwnership, false),
            "service_outcomeunknown" => (Service, ManualCheck, ObserveBeforeRetry, false),
            "service_credentialinvalid"
            | "service_credentialrequired"
            | "service_logonfailure"
            | "credential_repair_unavailable" => {
                (Service, UserAction, RepairRunnerCredential, false)
            }
            "service_elevationrequired"
            | "service_elevationcancelled"
            | "service_permissiondenied"
            | "elevation_required"
            | "elevation_cancelled" => (Service, ConfirmationRequired, OpenRuntimeSettings, false),
            "service_installfailed"
            | "service_startfailed"
            | "service_stopfailed"
            | "service_restartfailed"
            | "service_uninstallfailed"
            | "service_notinstalled"
            | "service_unavailable"
            | "service_platformunsupported"
            | "component_not_configured" => (Service, UserAction, OpenDiagnostics, true),

            // 7. Legacy migration conflicts / interruption
            "migration_required"
            | "migration_conflict"
            | "migration_target_conflict"
            | "migration_recovery_required"
            | "migration_restored"
            | "legacy_owner_unknown"
            | "legacy_owner_changed"
            | "legacy_owner_active"
            | "legacy_process_changed"
            | "legacy_tunnel_owner"
            | "legacy_tunnel_id"
            | "legacy_tunnel_credentials"
            | "legacy_runner_layout_unsupported"
            | "legacy_runner_configuration" => (Migration, ManualCheck, ResolveMigration, false),
            "legacy_stop_unconfirmed" => (Migration, ManualCheck, ObserveBeforeRetry, false),

            // 8 & 9. Tunnel network, credentials, ownership, Cloudflare controls
            "tunnel_credentials" | "tunnel_token_invalid" | "cloudflare_authentication_failed" => {
                (Tunnel, UserAction, CheckConnection, false)
            }
            "tunnel_owner" | "tunnel_binding_conflict" | "tunnel_catalog_conflict" => {
                (Tunnel, ManualCheck, InspectServiceOwnership, false)
            }
            "tunnel_server_owned" | "tunnel_host_busy" => {
                (Tunnel, ConfirmationRequired, OpenRuntimeSettings, false)
            }
            "cloudflare_ingress_not_applied" => {
                (Tunnel, ConfirmationRequired, RestartServer, false)
            }
            "cloudflare_proxy_invalid" => (Tunnel, UserAction, OpenNetworkSettings, false),
            "cloudflare_storage_unavailable"
            | "cloudflare_verification_failed"
            | "cloudflare_version_unsupported"
            | "cloudflare_platform_unsupported"
            | "cloudflare_extraction_failed" => (Tunnel, UserAction, OpenDiagnostics, false),
            "cloudflare_download_failed"
            | "cloudflare_network_unavailable"
            | "cloudflare_start_failed"
            | "cloudflare_binary_unavailable"
            | "cloudflare_discovery_unavailable"
            | "cloudflare_forwarding_timeout"
            | "cloudflare_forwarding_mismatch" => (Tunnel, UserAction, CheckConnection, true),
            "cloudflare_environment_required" => (Tunnel, UserAction, OpenSetup, false),
            "cloudflare_revision_stale" | "tunnel_revision_stale" => {
                (Tunnel, ConfirmationRequired, CheckConnection, false)
            }
            "cloudflare_server_unreachable"
            | "cloudflare_control_unavailable"
            | "cloudflare_control_required"
            | "tunnel_proxy_invalid"
            | "tunnel_unavailable"
            | "tunnel_start_failed"
            | "tunnel_apply_failed"
            | "tunnel_profile_missing"
            | "regular_tunnel_not_configured"
            | "quick_share_start_failed"
            | "quick_share_unavailable" => (Tunnel, UserAction, CheckConnection, true),
            "tunnel_config_invalid" => (Config, UserAction, OpenNetworkSettings, false),

            // 10. Configuration expired, corrupt, revision/fence mismatch
            "configuration_migration_failed" | "desktop_state_corrupt" => {
                (Config, ConfirmationRequired, RestoreConfiguration, false)
            }
            "environment_changed"
            | "environment_conflict"
            | "journal_missing"
            | "runtime_selection_changed"
            | "diagnostic_identity_changed"
            | "server_environment_changed"
            | "inventory_changed"
            | "project_revision_changed"
            | "project_operation_conflict"
            | "mcp_provider_changed"
            | "coding_agent_changed"
            | "managed_instruction_conflict" => (Config, ConfirmationRequired, RefreshState, false),
            "server_environment_invalid"
            | "server_environment_duplicate_key"
            | "server_environment_unavailable"
            | "trace_mode_invalid"
            | "desktop_data_dir_invalid"
            | "desktop_data_dir_unavailable"
            | "state_io" => (Config, ManualCheck, OpenDiagnostics, false),
            "not_configured" | "setup_not_found" => (Environment, UserAction, OpenSetup, false),
            "runtime_switch_jobs_confirmation_required"
            | "full_trace_confirmation_required"
            | "project_confirmation_required" => {
                (Desktop, ConfirmationRequired, ObserveBeforeRetry, false)
            }

            // 11. OAuth / authentication / authorization
            "credential_repair_failed" => (Auth, ManualCheck, OpenDiagnostics, false),
            "authentication_required"
            | "workspace_authentication_required"
            | "user_credential_required"
            | "pairing_code_required" => (Auth, UserAction, RepairUserCredential, false),
            "permission_denied"
            | "workspace_permission_denied"
            | "user_identity_conflict"
            | "pairing_user_conflict" => (Auth, ManualCheck, OpenDiagnostics, false),
            "pairing_recovery_required" => (Auth, ManualCheck, ObserveBeforeRetry, false),

            // 12. Background operation contention
            "desktop_operation_busy"
            | "desktop_state_unavailable"
            | "desktop_operation_not_cancellable" => {
                (Desktop, AutoRecovering, WaitAutoRecovery, true)
            }
            "desktop_operation_not_current" => (Desktop, UserAction, RefreshState, false),

            // 13. Uncertain outcomes / interrupted writes / unconfirmed stops
            "operation_uncertain"
            | "response_uncertain"
            | "process_stop_unconfirmed"
            | "project_reconcile_required"
            | "runner_config_reconcile_required"
            | "server_environment_write_unconfirmed"
            | "support_bundle_write_unconfirmed"
            | "inventory_write_unconfirmed"
            | "inventory_export_path_unconfirmed"
            | "update_outcome_uncertain"
            | "desktop_operation_cancelled" => (Desktop, ManualCheck, ObserveBeforeRetry, false),

            // 14. Project missing, not loaded, inaccessible
            "project_not_loaded" => (Project, UserAction, OpenProjects, true),
            "project_unavailable"
            | "project_path"
            | "project_path_invalid"
            | "project_identity"
            | "project_inventory"
            | "local_project_required" => (Project, UserAction, OpenProjects, false),

            // Providers (MCP / Coding agents)
            "mcp_provider_invalid"
            | "mcp_provider_command_unavailable"
            | "mcp_provider_capacity_exceeded"
            | "coding_agent_invalid"
            | "coding_agent_executable_unavailable" => {
                (Runtime, UserAction, ManageProviders, false)
            }

            // General runtime readiness / CLI command failures
            "runtime_not_ready" => (Runtime, UserAction, StartRuntime, true),
            "webcodex_command_timeout" => (Runtime, UserAction, OpenActivity, true),
            "webcodex_command_failed"
            | "webcodex_command_input_failed"
            | "machine_event_overflow" => (Runtime, ManualCheck, OpenDiagnostics, false),

            _ => {
                if code.starts_with("service_") {
                    (Service, ManualCheck, OpenDiagnostics, false)
                } else if code.starts_with("migration_") || code.starts_with("legacy_") {
                    (Migration, ManualCheck, ResolveMigration, false)
                } else if code.starts_with("cloudflare_") || code.starts_with("tunnel_") {
                    (Tunnel, UserAction, CheckConnection, true)
                } else if code.starts_with("oauth_") || code.starts_with("auth_") {
                    (Auth, UserAction, RepairUserCredential, false)
                } else if code.starts_with("project_") {
                    (Project, UserAction, OpenProjects, false)
                } else if code.starts_with("runner_") {
                    (Runner, UserAction, OpenDiagnostics, false)
                } else if code.starts_with("server_") {
                    (Server, UserAction, OpenDiagnostics, false)
                } else if code.starts_with("runtime_") || code.starts_with("upgrade_") {
                    (Runtime, UserAction, OpenRuntimeSettings, false)
                } else if code.starts_with("workspace_") {
                    (Server, UserAction, RefreshState, true)
                } else if code.starts_with("inventory_")
                    || code.starts_with("support_bundle_")
                    || code.starts_with("diagnostic_")
                {
                    (Desktop, ManualCheck, OpenDiagnostics, false)
                } else {
                    (Unknown, ManualCheck, OpenDiagnostics, false)
                }
            }
        };

        Self {
            component,
            stage: None,
            reason_code: None,
            recovery_kind,
            safe_action,
            retry_allowed,
            http_status: None,
            exit_code: None,
            operation_id: None,
            generation: None,
            resource_id: None,
            activity_sequence: None,
        }
    }
}

pub fn sanitize_safe_identifier(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() < 2 || trimmed.len() > 96 {
        return None;
    }
    if !trimmed.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
    }) {
        return None;
    }
    if [
        // Error codes and phase identifiers legitimately contain words such as
        // "token", "credential", and "authorization". Reject real credential
        // markers, not these generic metadata words.
        "secret",
        "password",
        "bearer",
        "cookie",
        "api_key",
        "private_key",
        "session_id",
        "wc_pat",
        "wc_agent",
        "wc_boot",
        "wc_acct",
    ]
    .into_iter()
    .any(|sensitive| trimmed.contains(sensitive))
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn sanitize_resource_or_operation_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 96 {
        return None;
    }
    if !trimmed
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if [
        "wc_pat_",
        "wc_agent_",
        "wc_boot_",
        "wc_acct_",
        "bearer",
        "secret",
        "password",
        "token",
    ]
    .into_iter()
    .any(|sensitive| lower.contains(sensitive))
    {
        return None;
    }
    Some(trimmed.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopError {
    pub code: String,
    pub message: String,
    pub next_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<ErrorDiagnostic>,
}

impl DesktopError {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        next_action: impl Into<String>,
    ) -> Self {
        let code = code.into();
        let diagnostic = ErrorDiagnostic::classify(&code);
        Self {
            code,
            message: message.into(),
            next_action: next_action.into(),
            details: None,
            diagnostic: Some(diagnostic),
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    fn ensure_diagnostic(&mut self) -> &mut ErrorDiagnostic {
        let code = self.code.clone();
        self.diagnostic
            .get_or_insert_with(|| ErrorDiagnostic::classify(&code))
    }

    pub fn diagnostic_ref(&self) -> Option<&ErrorDiagnostic> {
        self.diagnostic.as_ref()
    }

    pub fn component(&self) -> ErrorComponent {
        self.diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.component)
            .unwrap_or_else(|| ErrorDiagnostic::classify(&self.code).component)
    }

    pub fn with_component(mut self, component: ErrorComponent) -> Self {
        self.ensure_diagnostic().component = component;
        self
    }

    pub fn with_stage(mut self, stage: &str) -> Self {
        if let Some(safe) = sanitize_safe_identifier(stage) {
            self.ensure_diagnostic().stage = Some(safe);
        }
        self
    }

    pub fn with_reason_code(mut self, reason_code: &str) -> Self {
        if let Some(safe) = sanitize_safe_identifier(reason_code) {
            if safe != self.code {
                self.ensure_diagnostic().reason_code = Some(safe);
            }
        }
        self
    }

    pub fn with_recovery(
        mut self,
        recovery_kind: RecoveryKind,
        safe_action: SafeRecoveryAction,
        retry_allowed: bool,
    ) -> Self {
        let diagnostic = self.ensure_diagnostic();
        diagnostic.recovery_kind = recovery_kind;
        diagnostic.safe_action = safe_action;
        diagnostic.retry_allowed = retry_allowed;
        self
    }

    pub fn with_http_status(mut self, status: u16) -> Self {
        if !(100..=599).contains(&status) {
            return self;
        }
        let code = self.code.clone();
        let diagnostic = self.ensure_diagnostic();
        diagnostic.http_status = Some(status);
        if diagnostic.reason_code.is_none() {
            let reason = match status {
                400 => Some("http_bad_request"),
                401 => Some("http_unauthorized"),
                403 => Some("http_forbidden"),
                404 => Some("http_not_found"),
                408 => Some("http_timeout"),
                409 => Some("http_conflict"),
                429 => Some("http_rate_limited"),
                502 => Some("http_bad_gateway"),
                503 => Some("http_service_unavailable"),
                504 => Some("http_gateway_timeout"),
                500..=599 => Some("http_server_error"),
                300..=399 => Some("http_unexpected_redirect"),
                400..=499 => Some("http_client_error"),
                _ => None,
            };
            if let Some(reason) = reason {
                if reason != code {
                    diagnostic.reason_code = Some(reason.to_string());
                }
            }
        }
        if matches!(
            code.as_str(),
            "server_request_failed" | "workspace_unavailable" | "cloudflare_control_unavailable"
        ) {
            match status {
                400 => {
                    diagnostic.recovery_kind = RecoveryKind::UserAction;
                    diagnostic.safe_action = SafeRecoveryAction::OpenDiagnostics;
                    diagnostic.retry_allowed = false;
                }
                401 | 403 => {
                    diagnostic.component = ErrorComponent::Auth;
                    diagnostic.recovery_kind = RecoveryKind::UserAction;
                    diagnostic.safe_action = SafeRecoveryAction::RepairUserCredential;
                    diagnostic.retry_allowed = false;
                }
                409 => {
                    diagnostic.recovery_kind = RecoveryKind::ConfirmationRequired;
                    diagnostic.safe_action = SafeRecoveryAction::ObserveBeforeRetry;
                    diagnostic.retry_allowed = false;
                }
                503 => {
                    // A read-only observation is allowed, but the original POST
                    // must not be replayed without reconciling its effects.
                    diagnostic.recovery_kind = RecoveryKind::AutoRecovering;
                    diagnostic.safe_action = SafeRecoveryAction::RefreshState;
                    diagnostic.retry_allowed = false;
                }
                500..=599 => {
                    diagnostic.recovery_kind = RecoveryKind::ManualCheck;
                    diagnostic.safe_action = SafeRecoveryAction::OpenDiagnostics;
                    diagnostic.retry_allowed = false;
                }
                _ => {}
            }
        }
        self
    }

    pub fn with_exit_code(mut self, exit_code: i32) -> Self {
        self.ensure_diagnostic().exit_code = Some(exit_code);
        self
    }

    pub fn with_operation_context(
        mut self,
        operation_id: Option<&str>,
        generation: Option<u64>,
        resource_id: Option<&str>,
        activity_sequence: Option<u64>,
    ) -> Self {
        let diagnostic = self.ensure_diagnostic();
        if diagnostic.operation_id.is_none() {
            diagnostic.operation_id = operation_id.and_then(sanitize_resource_or_operation_id);
        }
        if diagnostic.generation.is_none() {
            diagnostic.generation = generation;
        }
        if diagnostic.resource_id.is_none() {
            diagnostic.resource_id = resource_id.and_then(sanitize_resource_or_operation_id);
        }
        if diagnostic.activity_sequence.is_none() {
            diagnostic.activity_sequence = activity_sequence;
        }
        self
    }

    /// Explicit allowlist projection for diagnostic reports and support bundles.
    /// Never exports freeform message text, paths, request bodies, or arbitrary details.
    pub fn safe_summary_value(&self) -> Value {
        let diag = self
            .diagnostic
            .clone()
            .unwrap_or_else(|| ErrorDiagnostic::classify(&self.code));
        let mut map = serde_json::Map::new();
        map.insert("code".into(), json!(self.code));
        map.insert("component".into(), json!(diag.component));
        map.insert("recovery_kind".into(), json!(diag.recovery_kind));
        map.insert("safe_action".into(), json!(diag.safe_action));
        map.insert("retry_allowed".into(), json!(diag.retry_allowed));
        if let Some(stage) = diag.stage {
            map.insert("stage".into(), json!(stage));
        }
        if let Some(reason_code) = diag.reason_code {
            map.insert("reason_code".into(), json!(reason_code));
        }
        if let Some(http_status) = diag.http_status {
            map.insert("http_status".into(), json!(http_status));
        }
        if let Some(exit_code) = diag.exit_code {
            map.insert("exit_code".into(), json!(exit_code));
        }
        if let Some(operation_id) = diag.operation_id {
            map.insert("operation_id".into(), json!(operation_id));
        }
        if let Some(activity_sequence) = diag.activity_sequence {
            map.insert("activity_sequence".into(), json!(activity_sequence));
        }
        Value::Object(map)
    }
}

impl Display for DesktopError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for DesktopError {}

pub type DesktopResult<T> = Result<T, DesktopError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_http_400_and_503_keep_distinct_structured_diagnostics() {
        let bad_request = DesktopError::new(
            "server_request_failed",
            "Local user creation failed: The Server rejected the request parameters (HTTP 400)",
            "Check setup inputs",
        )
        .with_stage("user_create")
        .with_http_status(400);
        let unavailable = DesktopError::new(
            "server_request_failed",
            "The Server is temporarily unavailable (HTTP 503)",
            "Wait and check Server status",
        )
        .with_stage("user_create")
        .with_http_status(503);

        let d400 = bad_request.diagnostic_ref().unwrap();
        let d503 = unavailable.diagnostic_ref().unwrap();
        assert_eq!(d400.http_status, Some(400));
        assert_eq!(d400.reason_code.as_deref(), Some("http_bad_request"));
        assert_eq!(d400.recovery_kind, RecoveryKind::UserAction);
        assert!(!d400.retry_allowed);

        assert_eq!(d503.http_status, Some(503));
        assert_eq!(
            d503.reason_code.as_deref(),
            Some("http_service_unavailable")
        );
        assert_eq!(d503.recovery_kind, RecoveryKind::AutoRecovering);
        assert_eq!(d503.safe_action, SafeRecoveryAction::RefreshState);
        assert!(!d503.retry_allowed, "503 does not prove POST had no effect");
        assert_eq!(d400.stage.as_deref(), Some("user_create"));
        assert_eq!(d503.stage.as_deref(), Some("user_create"));
    }

    #[test]
    fn http_post_failures_require_reconciliation_before_replay() {
        for code in [
            "server_request_failed",
            "workspace_unavailable",
            "cloudflare_control_unavailable",
        ] {
            let base = DesktopError::new(code, "POST rejected", "Inspect");
            if code != "cloudflare_control_unavailable" {
                assert!(!base.diagnostic_ref().unwrap().retry_allowed);
            }
            for status in [400, 409, 500, 503] {
                let error = base.clone().with_http_status(status);
                let diagnostic = error.diagnostic_ref().unwrap();
                assert!(!diagnostic.retry_allowed, "{code}: HTTP {status}");
                assert_ne!(diagnostic.safe_action, SafeRecoveryAction::Retry);
            }
        }
    }

    #[test]
    fn ownership_and_uncertain_errors_forbid_automatic_retry() {
        for code in [
            "service_foreignservice",
            "service_ownershipunknown",
            "tunnel_owner",
            "runner_not_owned",
            "operation_uncertain",
            "response_uncertain",
            "service_outcomeunknown",
            "process_stop_unconfirmed",
            "project_reconcile_required",
            "migration_conflict",
            "cloudflare_revision_stale",
        ] {
            let error = DesktopError::new(code, "test", "inspect");
            let diag = error.diagnostic_ref().unwrap();
            assert!(!diag.retry_allowed, "{code} must not allow automatic retry");
            assert_ne!(
                diag.safe_action,
                SafeRecoveryAction::Retry,
                "{code} must not recommend blind retry"
            );
        }
    }

    #[test]
    fn diagnostic_identifiers_may_name_token_or_credential_failure_without_leaking_values() {
        for name in [
            "credential_repair_failed",
            "tunnel_token_invalid",
            "oauth_authorization_failed",
        ] {
            assert_eq!(sanitize_safe_identifier(name).as_deref(), Some(name));
        }
        for sensitive in [
            "wc_pat_canary_value",
            "wc_agent_canary_value",
            "password_secret",
        ] {
            assert!(sanitize_safe_identifier(sensitive).is_none());
        }
    }

    #[test]
    fn safe_identifiers_and_summary_reject_secrets_and_paths() {
        let error = DesktopError::new(
            "server_request_failed",
            "Bearer wc_pat_canary_secret_value",
            "Check C:\\Users\\secret\\token",
        )
        .with_stage("user_create")
        .with_reason_code("wc_pat_leaked_token")
        .with_operation_context(
            Some("desktop-operation-42"),
            Some(9),
            Some("wc_pat_resource_leak"),
            Some(17),
        );
        let diag = error.diagnostic_ref().unwrap();
        assert_eq!(diag.stage.as_deref(), Some("user_create"));
        assert_eq!(diag.reason_code, None);
        assert_eq!(diag.operation_id.as_deref(), Some("desktop-operation-42"));
        assert_eq!(diag.resource_id, None);

        let summary = serde_json::to_string(&error.safe_summary_value()).unwrap();
        assert!(!summary.contains("wc_pat_"));
        assert!(!summary.contains("canary"));
        assert!(!summary.contains("Users"));
        assert!(summary.contains("user_create"));
        assert!(summary.contains("desktop-operation-42"));
    }
}
