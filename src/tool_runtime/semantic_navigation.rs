//! Compact project-specific semantic navigation capability for coding startup.
//!
//! The startup probe uses only the typed agent `Status` operation. It never
//! enters public ToolCall dispatch, starts a language server, or exposes the
//! raw agent transport/result envelope.

use super::lsp_tools::agent_local_project_id;
use super::project_resolution::ResolvedProject;
use super::ToolRuntime;
use crate::lsp_bridge::{
    parse_agent_lsp_result_envelope, AgentLspPayload, AgentLspRequest, LspAvailabilityStatus,
    LspStatusResult,
};
use crate::shell_client::{EnqueueLspError, RunnerFeature};
use serde::Serialize;
use std::time::Duration;
use tokio::time::Instant;

pub(crate) const DEFAULT_SEMANTIC_NAVIGATION_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

const RUST_LANGUAGE: &str = "rust";
const RUST_ANALYZER_SERVER: &str = "rust-analyzer";
const GO_LANGUAGE: &str = "go";
const GOPLS_SERVER: &str = "gopls";
const SEMANTIC_NAVIGATION_TOOLS: [&str; 7] = [
    "lsp_status",
    "document_symbols",
    "goto_definition",
    "find_references",
    "document_diagnostics",
    "hover",
    "workspace_symbols",
];
const SEMANTIC_NAVIGATION_PREFERRED_FLOW: [&str; 6] = [
    "document_symbols",
    "goto_definition",
    "find_references",
    "hover",
    "read_file",
    "search_project_text",
];
const RUST_SEMANTIC_NAVIGATION_LIMITATIONS: [&str; 5] = [
    "rust_only",
    "read_only",
    "workspace_only",
    "no_dependency_navigation",
    "full_text_sync_only",
];
const GO_SEMANTIC_NAVIGATION_LIMITATIONS: [&str; 5] = [
    "go_only",
    "read_only",
    "workspace_only",
    "no_dependency_navigation",
    "full_text_sync_only",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SemanticNavigationStartupStatus {
    Running,
    Available,
    Initializing,
    Crashed,
    Unavailable,
    NotApplicable,
    AgentUnavailable,
    AgentCapabilityUnavailable,
    ProbeTimeout,
    ProbeFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SemanticNavigationReasonCode {
    RustNotDetected,
    AgentNotConnected,
    LspCapabilityNotAdvertised,
    ServerCrashed,
    ServerUnavailable,
    StatusProbeTimedOut,
    StatusProbeFailed,
    MalformedAgentResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SemanticNavigationStartupSummary {
    supported: bool,
    available: bool,
    recommended: bool,
    status: SemanticNavigationStartupStatus,
    language: Option<&'static str>,
    server: Option<&'static str>,
    position_encoding: Option<String>,
    tools: Vec<&'static str>,
    preferred_flow: Vec<&'static str>,
    limitations: Vec<&'static str>,
    reason_code: Option<SemanticNavigationReasonCode>,
}

impl SemanticNavigationStartupSummary {
    fn unsupported(
        status: SemanticNavigationStartupStatus,
        reason_code: SemanticNavigationReasonCode,
    ) -> Self {
        Self {
            supported: false,
            available: false,
            recommended: false,
            status,
            language: None,
            server: None,
            position_encoding: None,
            tools: Vec::new(),
            preferred_flow: Vec::new(),
            limitations: Vec::new(),
            reason_code: Some(reason_code),
        }
    }

    fn supported_failure(
        status: SemanticNavigationStartupStatus,
        reason_code: SemanticNavigationReasonCode,
    ) -> Self {
        Self {
            supported: true,
            available: false,
            recommended: false,
            status,
            language: None,
            // The status probe did not yield a usable language selection. Keep
            // the legacy Rust provider projection unchanged for compatibility.
            server: Some(RUST_ANALYZER_SERVER),
            position_encoding: None,
            tools: SEMANTIC_NAVIGATION_TOOLS.to_vec(),
            preferred_flow: Vec::new(),
            limitations: RUST_SEMANTIC_NAVIGATION_LIMITATIONS.to_vec(),
            reason_code: Some(reason_code),
        }
    }

    fn rust_not_detected() -> Self {
        Self {
            supported: true,
            available: false,
            recommended: false,
            status: SemanticNavigationStartupStatus::NotApplicable,
            language: None,
            server: Some(RUST_ANALYZER_SERVER),
            position_encoding: None,
            tools: Vec::new(),
            preferred_flow: Vec::new(),
            limitations: RUST_SEMANTIC_NAVIGATION_LIMITATIONS.to_vec(),
            reason_code: Some(SemanticNavigationReasonCode::RustNotDetected),
        }
    }

    fn from_enqueue_error(error: &EnqueueLspError) -> Self {
        match error {
            EnqueueLspError::UnknownClient { .. } | EnqueueLspError::ClientOffline { .. } => {
                Self::unsupported(
                    SemanticNavigationStartupStatus::AgentUnavailable,
                    SemanticNavigationReasonCode::AgentNotConnected,
                )
            }
            EnqueueLspError::UnsupportedCapability { .. } => Self::unsupported(
                SemanticNavigationStartupStatus::AgentCapabilityUnavailable,
                SemanticNavigationReasonCode::LspCapabilityNotAdvertised,
            ),
            EnqueueLspError::InvalidRequest { .. } | EnqueueLspError::QueueFull { .. } => {
                Self::supported_failure(
                    SemanticNavigationStartupStatus::ProbeFailed,
                    SemanticNavigationReasonCode::StatusProbeFailed,
                )
            }
        }
    }

    fn from_lsp_status(
        result: LspStatusResult,
        expected_project_id: &str,
    ) -> Result<Self, SemanticNavigationReasonCode> {
        if result.project != expected_project_id {
            return Err(SemanticNavigationReasonCode::MalformedAgentResult);
        }
        // Preserve the pre-existing Rust-first behavior in mixed workspaces.
        // Go is additive: a Go-only workspace selects gopls, while existing
        // Python/TypeScript-only startup behavior remains unchanged.
        let (language, server_name, limitations) = if result
            .detected_languages
            .iter()
            .any(|language| language == RUST_LANGUAGE)
        {
            (
                RUST_LANGUAGE,
                RUST_ANALYZER_SERVER,
                RUST_SEMANTIC_NAVIGATION_LIMITATIONS.as_slice(),
            )
        } else if result
            .detected_languages
            .iter()
            .any(|language| language == GO_LANGUAGE)
        {
            (
                GO_LANGUAGE,
                GOPLS_SERVER,
                GO_SEMANTIC_NAVIGATION_LIMITATIONS.as_slice(),
            )
        } else {
            return Ok(Self::rust_not_detected());
        };
        let Some(server) = result
            .servers
            .iter()
            .find(|entry| entry.language == language && entry.server == server_name)
        else {
            return Err(SemanticNavigationReasonCode::MalformedAgentResult);
        };
        if server
            .position_encoding
            .as_deref()
            .is_some_and(|encoding| !matches!(encoding, "utf-8" | "utf-16" | "utf-32"))
        {
            return Err(SemanticNavigationReasonCode::MalformedAgentResult);
        }

        let (available, recommended, status, reason_code, position_encoding) = match server.status {
            LspAvailabilityStatus::Running => (
                true,
                true,
                SemanticNavigationStartupStatus::Running,
                None,
                server.position_encoding.clone(),
            ),
            LspAvailabilityStatus::Available => (
                true,
                true,
                SemanticNavigationStartupStatus::Available,
                None,
                None,
            ),
            LspAvailabilityStatus::Initializing => (
                true,
                false,
                SemanticNavigationStartupStatus::Initializing,
                None,
                None,
            ),
            // A crashed slot restarts on the next request, so navigation is
            // still worth offering — but only while the agent reports the
            // executable itself as available. Hardcoding `true` here would
            // advertise navigation after the binary was removed.
            LspAvailabilityStatus::Crashed => (
                server.available,
                false,
                SemanticNavigationStartupStatus::Crashed,
                Some(SemanticNavigationReasonCode::ServerCrashed),
                None,
            ),
            LspAvailabilityStatus::Unavailable => (
                false,
                false,
                SemanticNavigationStartupStatus::Unavailable,
                Some(SemanticNavigationReasonCode::ServerUnavailable),
                None,
            ),
        };

        Ok(Self {
            supported: true,
            available,
            recommended,
            status,
            language: Some(language),
            server: Some(server_name),
            position_encoding,
            tools: SEMANTIC_NAVIGATION_TOOLS.to_vec(),
            preferred_flow: if recommended {
                SEMANTIC_NAVIGATION_PREFERRED_FLOW.to_vec()
            } else {
                Vec::new()
            },
            limitations: limitations.to_vec(),
            reason_code,
        })
    }
}

impl ToolRuntime {
    pub(crate) async fn probe_semantic_navigation_for_startup(
        &self,
        resolved: &ResolvedProject,
    ) -> SemanticNavigationStartupSummary {
        let client_id = resolved.config.client_id.clone();
        let Some(client) = self
            .shell_clients
            .get_client_semantic_view(&client_id)
            .await
        else {
            return SemanticNavigationStartupSummary::unsupported(
                SemanticNavigationStartupStatus::AgentUnavailable,
                SemanticNavigationReasonCode::AgentNotConnected,
            );
        };
        if !client.view.connected {
            return SemanticNavigationStartupSummary::unsupported(
                SemanticNavigationStartupStatus::AgentUnavailable,
                SemanticNavigationReasonCode::AgentNotConnected,
            );
        }
        if !client.supports(RunnerFeature::LspReadOnlyNavigation) {
            return SemanticNavigationStartupSummary::unsupported(
                SemanticNavigationStartupStatus::AgentCapabilityUnavailable,
                SemanticNavigationReasonCode::LspCapabilityNotAdvertised,
            );
        }
        let Some(agent_project_id) = agent_local_project_id(&resolved.resolved_id) else {
            return SemanticNavigationStartupSummary::supported_failure(
                SemanticNavigationStartupStatus::ProbeFailed,
                SemanticNavigationReasonCode::StatusProbeFailed,
            );
        };

        let deadline = Instant::now() + self.semantic_navigation_probe_timeout;
        let payload = AgentLspPayload {
            project_id: agent_project_id.to_string(),
            request: AgentLspRequest::Status,
        };
        let timeout_secs = self.semantic_navigation_probe_timeout.as_secs().max(1);
        let enqueued = tokio::time::timeout_at(
            deadline,
            self.shell_clients.enqueue_lsp(
                client_id,
                payload,
                "coding_startup_probe".to_string(),
                timeout_secs,
            ),
        )
        .await;
        let (request_id, receiver) = match enqueued {
            Err(_) => {
                return SemanticNavigationStartupSummary::supported_failure(
                    SemanticNavigationStartupStatus::ProbeTimeout,
                    SemanticNavigationReasonCode::StatusProbeTimedOut,
                )
            }
            Ok(Err(error)) => return SemanticNavigationStartupSummary::from_enqueue_error(&error),
            Ok(Ok(request)) => request,
        };

        let response = match tokio::time::timeout_at(deadline, receiver).await {
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                return SemanticNavigationStartupSummary::supported_failure(
                    SemanticNavigationStartupStatus::ProbeTimeout,
                    SemanticNavigationReasonCode::StatusProbeTimedOut,
                );
            }
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                return SemanticNavigationStartupSummary::supported_failure(
                    SemanticNavigationStartupStatus::ProbeFailed,
                    SemanticNavigationReasonCode::StatusProbeFailed,
                );
            }
            Ok(Ok(response)) => response,
        };
        if !response.success || response.error.is_some() || response.exit_code != Some(0) {
            return SemanticNavigationStartupSummary::supported_failure(
                SemanticNavigationStartupStatus::ProbeFailed,
                SemanticNavigationReasonCode::StatusProbeFailed,
            );
        }
        let Some(stdout) = response.stdout.as_deref() else {
            return SemanticNavigationStartupSummary::supported_failure(
                SemanticNavigationStartupStatus::ProbeFailed,
                SemanticNavigationReasonCode::MalformedAgentResult,
            );
        };
        let envelope = match parse_agent_lsp_result_envelope(stdout) {
            Ok(envelope) => envelope,
            Err(_) => {
                return SemanticNavigationStartupSummary::supported_failure(
                    SemanticNavigationStartupStatus::ProbeFailed,
                    SemanticNavigationReasonCode::MalformedAgentResult,
                )
            }
        };
        if !envelope.success {
            return SemanticNavigationStartupSummary::supported_failure(
                SemanticNavigationStartupStatus::ProbeFailed,
                SemanticNavigationReasonCode::StatusProbeFailed,
            );
        }
        let Some(result) = envelope.result else {
            return SemanticNavigationStartupSummary::supported_failure(
                SemanticNavigationStartupStatus::ProbeFailed,
                SemanticNavigationReasonCode::MalformedAgentResult,
            );
        };
        let result = match serde_json::from_value::<LspStatusResult>(result) {
            Ok(result) => result,
            Err(_) => {
                return SemanticNavigationStartupSummary::supported_failure(
                    SemanticNavigationStartupStatus::ProbeFailed,
                    SemanticNavigationReasonCode::MalformedAgentResult,
                )
            }
        };
        match SemanticNavigationStartupSummary::from_lsp_status(result, agent_project_id) {
            Ok(summary) => summary,
            Err(reason_code) => SemanticNavigationStartupSummary::supported_failure(
                SemanticNavigationStartupStatus::ProbeFailed,
                reason_code,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp_bridge::LspServerStatusEntry;

    fn crashed_status(executable_available: bool) -> LspStatusResult {
        LspStatusResult {
            project: "demo".to_string(),
            detected_languages: vec!["rust".to_string()],
            servers: vec![LspServerStatusEntry {
                language: "rust".to_string(),
                server: "rust-analyzer".to_string(),
                available: executable_available,
                running: false,
                status: LspAvailabilityStatus::Crashed,
                source: None,
                position_encoding: None,
            }],
            warnings: Vec::new(),
        }
    }

    #[test]
    fn crashed_available_follows_agent_reported_executable_availability() {
        // Executable still present: the slot restarts on demand.
        let summary =
            SemanticNavigationStartupSummary::from_lsp_status(crashed_status(true), "demo")
                .unwrap();
        let value = serde_json::to_value(summary).unwrap();
        assert_eq!(value["status"], "crashed");
        assert_eq!(value["available"], true);
        assert_eq!(value["recommended"], false);
        assert_eq!(value["reason_code"], "server_crashed");

        // Executable removed after the crash: navigation must not be offered.
        let summary =
            SemanticNavigationStartupSummary::from_lsp_status(crashed_status(false), "demo")
                .unwrap();
        let value = serde_json::to_value(summary).unwrap();
        assert_eq!(value["status"], "crashed");
        assert_eq!(value["available"], false);
        assert_eq!(value["reason_code"], "server_crashed");
    }

    #[test]
    fn go_status_selects_gopls_without_changing_rust_first_precedence() {
        let go = LspStatusResult {
            project: "demo".to_string(),
            detected_languages: vec!["go".to_string()],
            servers: vec![LspServerStatusEntry {
                language: "go".to_string(),
                server: "gopls".to_string(),
                available: true,
                running: false,
                status: LspAvailabilityStatus::Available,
                source: None,
                position_encoding: None,
            }],
            warnings: Vec::new(),
        };
        let summary = SemanticNavigationStartupSummary::from_lsp_status(go, "demo").unwrap();
        let value = serde_json::to_value(summary).unwrap();
        assert_eq!(value["language"], "go");
        assert_eq!(value["server"], "gopls");
        assert_eq!(value["available"], true);
        assert!(value["limitations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "go_only"));

        let mixed = LspStatusResult {
            project: "demo".to_string(),
            detected_languages: vec!["rust".to_string(), "go".to_string()],
            servers: vec![
                LspServerStatusEntry {
                    language: "go".to_string(),
                    server: "gopls".to_string(),
                    available: true,
                    running: false,
                    status: LspAvailabilityStatus::Available,
                    source: None,
                    position_encoding: None,
                },
                LspServerStatusEntry {
                    language: "rust".to_string(),
                    server: "rust-analyzer".to_string(),
                    available: true,
                    running: false,
                    status: LspAvailabilityStatus::Available,
                    source: None,
                    position_encoding: None,
                },
            ],
            warnings: Vec::new(),
        };
        let summary = SemanticNavigationStartupSummary::from_lsp_status(mixed, "demo").unwrap();
        let value = serde_json::to_value(summary).unwrap();
        assert_eq!(value["language"], "rust");
        assert_eq!(value["server"], "rust-analyzer");
    }

    #[test]
    fn enqueue_error_classification_uses_variants_not_display_text() {
        let cases = [
            (
                EnqueueLspError::UnknownClient {
                    client_id: "agent does not support navigation".to_string(),
                },
                SemanticNavigationStartupStatus::AgentUnavailable,
                SemanticNavigationReasonCode::AgentNotConnected,
            ),
            (
                EnqueueLspError::UnsupportedCapability {
                    client_id: "unknown shell client wording".to_string(),
                    capability:
                        crate::shell_protocol::SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION,
                },
                SemanticNavigationStartupStatus::AgentCapabilityUnavailable,
                SemanticNavigationReasonCode::LspCapabilityNotAdvertised,
            ),
            (
                EnqueueLspError::QueueFull {
                    client_id: "does not support".to_string(),
                    limit: 256,
                },
                SemanticNavigationStartupStatus::ProbeFailed,
                SemanticNavigationReasonCode::StatusProbeFailed,
            ),
        ];

        for (error, expected_status, expected_reason) in cases {
            let displayed = error.to_string();
            let summary = SemanticNavigationStartupSummary::from_enqueue_error(&error);
            assert_eq!(summary.status, expected_status, "{displayed}");
            assert_eq!(summary.reason_code, Some(expected_reason), "{displayed}");
        }
    }
}
