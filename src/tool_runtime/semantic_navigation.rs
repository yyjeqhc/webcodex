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
use serde::Serialize;
use std::time::Duration;
use tokio::time::Instant;

pub(crate) const DEFAULT_SEMANTIC_NAVIGATION_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

const RUST_LANGUAGE: &str = "rust";
const RUST_ANALYZER_SERVER: &str = "rust-analyzer";
const SEMANTIC_NAVIGATION_TOOLS: [&str; 4] = [
    "lsp_status",
    "document_symbols",
    "goto_definition",
    "find_references",
];
const SEMANTIC_NAVIGATION_PREFERRED_FLOW: [&str; 5] = [
    "document_symbols",
    "goto_definition",
    "find_references",
    "read_file",
    "search_project_text",
];
const SEMANTIC_NAVIGATION_LIMITATIONS: [&str; 5] = [
    "rust_only",
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
    ProjectNotAgentBacked,
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
            server: Some(RUST_ANALYZER_SERVER),
            position_encoding: None,
            tools: SEMANTIC_NAVIGATION_TOOLS.to_vec(),
            preferred_flow: Vec::new(),
            limitations: SEMANTIC_NAVIGATION_LIMITATIONS.to_vec(),
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
            limitations: SEMANTIC_NAVIGATION_LIMITATIONS.to_vec(),
            reason_code: Some(SemanticNavigationReasonCode::RustNotDetected),
        }
    }

    fn from_lsp_status(
        result: LspStatusResult,
        expected_project_id: &str,
    ) -> Result<Self, SemanticNavigationReasonCode> {
        if result.project != expected_project_id {
            return Err(SemanticNavigationReasonCode::MalformedAgentResult);
        }
        if !result
            .detected_languages
            .iter()
            .any(|language| language == RUST_LANGUAGE)
        {
            return Ok(Self::rust_not_detected());
        }
        let Some(server) = result
            .servers
            .iter()
            .find(|entry| entry.language == RUST_LANGUAGE && entry.server == RUST_ANALYZER_SERVER)
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
            language: Some(RUST_LANGUAGE),
            server: Some(RUST_ANALYZER_SERVER),
            position_encoding,
            tools: SEMANTIC_NAVIGATION_TOOLS.to_vec(),
            preferred_flow: if recommended {
                SEMANTIC_NAVIGATION_PREFERRED_FLOW.to_vec()
            } else {
                Vec::new()
            },
            limitations: SEMANTIC_NAVIGATION_LIMITATIONS.to_vec(),
            reason_code,
        })
    }
}

impl ToolRuntime {
    pub(crate) async fn probe_semantic_navigation_for_startup(
        &self,
        resolved: &ResolvedProject,
    ) -> SemanticNavigationStartupSummary {
        if !resolved.config.is_agent() {
            return SemanticNavigationStartupSummary::unsupported(
                SemanticNavigationStartupStatus::NotApplicable,
                SemanticNavigationReasonCode::ProjectNotAgentBacked,
            );
        }
        let client_id = match resolved.config.agent_client_id() {
            Ok(client_id) => client_id.to_string(),
            Err(_) => {
                return SemanticNavigationStartupSummary::unsupported(
                    SemanticNavigationStartupStatus::AgentUnavailable,
                    SemanticNavigationReasonCode::AgentNotConnected,
                )
            }
        };
        let Some(client) = self.shell_clients.get_client_view(&client_id).await else {
            return SemanticNavigationStartupSummary::unsupported(
                SemanticNavigationStartupStatus::AgentUnavailable,
                SemanticNavigationReasonCode::AgentNotConnected,
            );
        };
        if !client.connected {
            return SemanticNavigationStartupSummary::unsupported(
                SemanticNavigationStartupStatus::AgentUnavailable,
                SemanticNavigationReasonCode::AgentNotConnected,
            );
        }
        if !client.capabilities.lsp_read_only_navigation {
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
            Ok(Err(error)) => {
                let lower = error.to_ascii_lowercase();
                if lower.contains("unknown shell client") || lower.contains("not connected") {
                    return SemanticNavigationStartupSummary::unsupported(
                        SemanticNavigationStartupStatus::AgentUnavailable,
                        SemanticNavigationReasonCode::AgentNotConnected,
                    );
                }
                if lower.contains("does not support") {
                    return SemanticNavigationStartupSummary::unsupported(
                        SemanticNavigationStartupStatus::AgentCapabilityUnavailable,
                        SemanticNavigationReasonCode::LspCapabilityNotAdvertised,
                    );
                }
                return SemanticNavigationStartupSummary::supported_failure(
                    SemanticNavigationStartupStatus::ProbeFailed,
                    SemanticNavigationReasonCode::StatusProbeFailed,
                );
            }
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
    fn non_agent_summary_is_bounded_and_not_applicable() {
        let summary = SemanticNavigationStartupSummary::unsupported(
            SemanticNavigationStartupStatus::NotApplicable,
            SemanticNavigationReasonCode::ProjectNotAgentBacked,
        );
        let value = serde_json::to_value(summary).unwrap();
        assert_eq!(value["status"], "not_applicable");
        assert_eq!(value["reason_code"], "project_not_agent_backed");
        assert_eq!(value["tools"], serde_json::json!([]));
        assert_eq!(value["preferred_flow"], serde_json::json!([]));
        assert_eq!(value["limitations"], serde_json::json!([]));
    }
}
