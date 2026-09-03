//! Runtime tool execution result envelope.

use serde::Serialize;
use serde_json::Value;

pub(crate) use webcodex_core::runtime_contract::RECOVERY_KIND_VALUES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryKind {
    FixInput,
    RetrySame,
    Reobserve,
    Reconcile,
    Wait,
    UserAction,
    NoAction,
}

impl RecoveryKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::FixInput => "fix_input",
            Self::RetrySame => "retry_same",
            Self::Reobserve => "reobserve",
            Self::Reconcile => "reconcile",
            Self::Wait => "wait",
            Self::UserAction => "user_action",
            Self::NoAction => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryTool {
    ListJobs,
    ComputerFindElements,
    ComputerListWindows,
    ComputerListApplications,
    ComputerListDisplays,
    ComputerSnapshotDisplay,
    ReadProjectArtifactMetadata,
}

impl RecoveryTool {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ListJobs => "list_jobs",
            Self::ComputerFindElements => "computer_find_elements",
            Self::ComputerListWindows => "computer_list_windows",
            Self::ComputerListApplications => "computer_list_applications",
            Self::ComputerListDisplays => "computer_list_displays",
            Self::ComputerSnapshotDisplay => "computer_snapshot_display",
            Self::ReadProjectArtifactMetadata => "read_project_artifact_metadata",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub success: bool,
    /// Main payload - always a JSON object so both MCP and GPT Actions
    /// can forward it verbatim.
    pub output: Value,
    /// Optional human-readable error when success == false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(output: Value) -> Self {
        Self {
            success: true,
            output,
            error: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            output: Value::Null,
            error: Some(msg.into()),
        }
    }

    pub fn err_with_output(msg: impl Into<String>, output: Value) -> Self {
        Self {
            success: false,
            output,
            error: Some(msg.into()),
        }
    }

    pub(crate) fn with_recovery(
        mut self,
        recovery_kind: RecoveryKind,
        recovery_tool: Option<RecoveryTool>,
    ) -> Self {
        if self.success {
            return self;
        }
        let Some(output) = self.output.as_object_mut() else {
            return self;
        };
        output.insert(
            "recovery_kind".to_string(),
            Value::String(recovery_kind.as_str().to_string()),
        );
        if let Some(recovery_tool) = recovery_tool {
            output.insert(
                "recovery_tool".to_string(),
                Value::String(recovery_tool.as_str().to_string()),
            );
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recovery_metadata_is_bounded_and_never_decorates_success() {
        let success = ToolResult::ok(json!({"value": true})).with_recovery(
            RecoveryKind::Reobserve,
            Some(RecoveryTool::ComputerListWindows),
        );
        assert!(success.output.get("recovery_kind").is_none());
        assert!(success.output.get("recovery_tool").is_none());

        let secret = "PRIVATE_FREE_FORM_BODY";
        let failure = ToolResult::err_with_output(secret, json!({"message": secret}))
            .with_recovery(
                RecoveryKind::Reobserve,
                Some(RecoveryTool::ComputerListWindows),
            );
        assert_eq!(failure.output["recovery_kind"], "reobserve");
        assert_eq!(failure.output["recovery_tool"], "computer_list_windows");
        assert!(!failure.output["recovery_kind"]
            .as_str()
            .unwrap()
            .contains(secret));
        assert!(!failure.output["recovery_tool"]
            .as_str()
            .unwrap()
            .contains(secret));
    }
}
