//! Runtime tool execution result envelope.

use serde::Serialize;
use serde_json::Value;

pub use webcodex_core::runtime_contract::{
    ContinuationCarrier, ContinuationKind, ContinuationSemantics, GeneratedFollowUpKind,
    CONTINUATION_CARRIER_VALUES, CONTINUATION_KIND_VALUES, GENERATED_FOLLOW_UP_KIND_VALUES,
    RECOVERY_KIND_VALUES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryKind {
    FixInput,
    RetrySame,
    Reobserve,
    Reconcile,
    Wait,
    UserAction,
    NoAction,
}

impl RecoveryKind {
    pub const fn as_str(self) -> &'static str {
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

/// Parser-ready advisory expression of one possible next tool call. It carries
/// no authority, never executes by itself, and is not a retry, cursor, or
/// idempotency identity. follow_up_kind is the Host execution posture: only
/// mechanically_followable may be followed without a new model decision, while
/// fallback_recovery must remain an explicit recovery/detail/dependency path.
/// Domain producers remain responsible for bounding and validating arguments.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuggestedToolCall {
    pub follow_up_kind: GeneratedFollowUpKind,
    pub tool: &'static str,
    pub arguments: Value,
}

impl SuggestedToolCall {
    pub fn mechanically_followable(tool: &'static str, arguments: Value) -> Self {
        Self {
            follow_up_kind: GeneratedFollowUpKind::MechanicallyFollowable,
            tool,
            arguments,
        }
    }

    pub fn fallback_recovery(tool: &'static str, arguments: Value) -> Self {
        Self {
            follow_up_kind: GeneratedFollowUpKind::FallbackRecovery,
            tool,
            arguments,
        }
    }

    pub fn to_value(self) -> Value {
        serde_json::to_value(self).expect("SuggestedToolCall serialization is infallible")
    }
}
#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub success: bool,
    /// Main payload - always a JSON object so both MCP
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

    pub fn with_recovery(mut self, recovery_kind: RecoveryKind) -> Self {
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
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn continuation_vocabulary_is_closed_unique_and_separate_from_recovery() {
        assert_eq!(
            CONTINUATION_KIND_VALUES,
            ["page", "batch", "observe", "checkpoint", "refine"]
        );
        assert_eq!(
            CONTINUATION_CARRIER_VALUES,
            [
                "position",
                "index",
                "opaque_token",
                "observation_token",
                "revision",
                "none"
            ]
        );
        let kinds = [
            ContinuationKind::Page,
            ContinuationKind::Batch,
            ContinuationKind::Observe,
            ContinuationKind::Checkpoint,
            ContinuationKind::Refine,
        ];
        assert_eq!(
            kinds.map(ContinuationKind::as_str),
            CONTINUATION_KIND_VALUES
        );
        let carriers = [
            ContinuationCarrier::Position,
            ContinuationCarrier::Index,
            ContinuationCarrier::OpaqueToken,
            ContinuationCarrier::ObservationToken,
            ContinuationCarrier::Revision,
            ContinuationCarrier::None,
        ];
        assert_eq!(
            carriers.map(ContinuationCarrier::as_str),
            CONTINUATION_CARRIER_VALUES
        );
        assert_eq!(
            RECOVERY_KIND_VALUES,
            [
                "fix_input",
                "retry_same",
                "reobserve",
                "reconcile",
                "wait",
                "user_action",
                "none"
            ],
            "T2 must not create or mutate the existing failure-recovery vocabulary"
        );
    }

    #[test]
    fn suggested_tool_call_has_explicit_host_follow_up_posture() {
        let call = SuggestedToolCall::fallback_recovery(
            "observe_jobs",
            json!({"items": [{"job_id": "job-1"}], "wait_secs": 30}),
        )
        .to_value();
        assert_eq!(call["follow_up_kind"], "fallback_recovery");
        assert_eq!(call["tool"], "observe_jobs");
        assert_eq!(call["arguments"]["items"][0]["job_id"], "job-1");
        assert_eq!(call.as_object().unwrap().len(), 3);
        assert!(call.get("authority").is_none());
        assert!(call.get("retry_token").is_none());
        assert!(call.get("continuation_token").is_none());

        let paging =
            SuggestedToolCall::mechanically_followable("git_log", json!({"project": "demo"}))
                .to_value();
        assert_eq!(paging["follow_up_kind"], "mechanically_followable");
    }
    #[test]
    fn recovery_metadata_is_bounded_and_never_decorates_success() {
        let success = ToolResult::ok(json!({"value": true})).with_recovery(RecoveryKind::Reobserve);
        assert!(success.output.get("recovery_kind").is_none());
        assert!(success.output.get("recovery_tool").is_none());

        let secret = "PRIVATE_FREE_FORM_BODY";
        let failure = ToolResult::err_with_output(secret, json!({"message": secret}))
            .with_recovery(RecoveryKind::Reobserve);
        assert_eq!(failure.output["recovery_kind"], "reobserve");
        assert!(failure.output.get("recovery_tool").is_none());
        assert!(!failure.output["recovery_kind"]
            .as_str()
            .unwrap()
            .contains(secret));
    }
}
