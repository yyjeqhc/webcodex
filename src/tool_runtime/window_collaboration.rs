use super::{ToolResult, ToolRuntime};
use crate::auth::{AuthContext, SCOPE_SESSION_COLLABORATE};
use crate::client_window::ClientWindow;
use serde_json::json;

fn invalid_window_context(error_kind: &str, message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message,
        json!({
            "failure_kind": "invalid_context",
            "error_kind": error_kind,
            "state_changed": false,
            "retry_same_delivery": false,
            "dispatch_certainty": "not_started",
        }),
    )
}

impl ToolRuntime {
    pub(crate) async fn post_window_operator_message(
        &self,
        target_window_key: &str,
        context_session_id: Option<&str>,
        context_project: Option<&str>,
        message: String,
        delivery_key: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !auth.is_some_and(|a| a.has_scope(SCOPE_SESSION_COLLABORATE)) {
            return ToolResult::err("collaboration scope required");
        }
        let Ok((kind, principal)) = super::runtime_observation_principal(auth) else {
            return ToolResult::err("collaboration principal unavailable");
        };
        if message.trim().is_empty()
            || message.chars().count() > super::sessions::MAX_MESSAGE_CHARS
            || delivery_key.trim().is_empty()
            || delivery_key.chars().count() > 128
        {
            return ToolResult::err("invalid message or delivery_key");
        }
        let canonical_context_project = if let Some(session) = context_session_id {
            if !webcodex_core::workflow_session_contract::is_valid_session_id(session) {
                return invalid_window_context(
                    "invalid_session_context",
                    "exact Session context identity required",
                );
            }
            if self
                .authorize_session_target(session, "work_result_send_message", auth)
                .await
                .is_err()
            {
                return invalid_window_context(
                    "session_context_unavailable",
                    "Session context is no longer available",
                );
            }
            let related = self.window_activity_db.as_ref().is_some_and(|db| {
                db.window_has_session_context(&kind, &principal, target_window_key, session)
                    .unwrap_or(false)
            });
            if !related {
                return invalid_window_context(
                    "session_context_unlinked",
                    "Session context is not explicitly linked to this Window",
                );
            }
            let Some(summary) = self.sessions.summary(session, Some(1)) else {
                return invalid_window_context(
                    "session_context_unavailable",
                    "Session context is no longer available",
                );
            };
            if context_project.is_some_and(|project| summary.project.as_deref() != Some(project)) {
                return invalid_window_context(
                    "session_project_mismatch",
                    "Session context Project mismatch",
                );
            }
            summary.project
        } else {
            context_project.map(str::to_string)
        };
        let Some(db) = self.communication_db.as_ref() else {
            return ToolResult::err("Window collaboration unavailable");
        };
        let input = webcodex_store::NewWindowOperatorMessage {
            principal_kind: kind,
            principal_id: principal,
            recipient_window_key: target_window_key.to_string(),
            context_session_id: context_session_id.map(str::to_string),
            context_project: canonical_context_project,
            kind: "guidance".into(),
            priority: "normal".into(),
            message,
            tags: Vec::new(),
            requires_ack: true,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        };
        match db.post_window_operator_message(input, &delivery_key) {
            Ok(webcodex_store::WindowOperatorDeliveryOutcome::Delivered {message_id,replayed}) =>
                ToolResult::ok(json!({"success":true,"message_id":message_id,"replayed":replayed,"state_changed":!replayed})),
            Ok(webcodex_store::WindowOperatorDeliveryOutcome::DeliveryKeyConflict) =>
                ToolResult::err_with_output("delivery_key_conflict",json!({"failure_kind":"conflict","state_changed":false,"dispatch_certainty":"not_started"})),
            Err(_) => ToolResult::err_with_output("Operator message persistence outcome is unknown",
                json!({"failure_kind":"outcome_unknown","state_changed":null,"retry_same_delivery":true}))
                .with_recovery(super::RecoveryKind::RetrySame),
        }
    }

    pub(crate) fn window_collaboration(
        &self,
        window_key: Option<&str>,
        auth: Option<&AuthContext>,
        limit: usize,
    ) -> serde_json::Value {
        let unavailable = || json!({"available":false,"can_send":false,"messages":[]});
        if !auth.is_some_and(|a| a.has_scope(SCOPE_SESSION_COLLABORATE)) {
            return unavailable();
        }
        let (Some(window), Some(db), Ok((kind, principal))) = (
            window_key,
            self.communication_db.as_ref(),
            super::runtime_observation_principal(auth),
        ) else {
            return unavailable();
        };
        match db.window_collaboration_transcript(&kind, &principal, window, limit) {
            Ok((messages, truncated)) => {
                json!({"available":true,"can_send":true,"returned":messages.len(),"messages":messages,"truncated":truncated})
            }
            Err(_) => unavailable(),
        }
    }

    pub(crate) fn add_window_operator_projection(
        &self,
        result: &mut ToolResult,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
        ack_ids: &[String],
    ) {
        let (Some(window), Some(db), Ok((kind, principal))) = (
            window,
            self.communication_db.as_ref(),
            super::runtime_observation_principal(auth),
        ) else {
            return;
        };
        if !result.output.is_object() {
            return;
        }
        let now = chrono::Utc::now().timestamp_millis();
        let Ok(batch) =
            db.take_window_operator_attention(&kind, &principal, window.key(), ack_ids, now, 4)
        else {
            return;
        };
        if batch.messages.is_empty() && batch.accepted_ack_ids.is_empty() {
            return;
        }
        result.output["operator_messages"] =
            json!({"messages":batch.messages,"ack":{"accepted_ids":batch.accepted_ack_ids}});
        if crate::json_measurement::serialized_json_len(result).is_ok_and(|bytes| {
            bytes > webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES
        }) {
            result
                .output
                .as_object_mut()
                .unwrap()
                .remove("operator_messages");
            if let Err(error) = db.rollback_window_operator_attention(
                &kind,
                &principal,
                window.key(),
                now,
                &batch.projection_rollbacks,
            ) {
                tracing::warn!(?error, "failed to roll back oversized Operator attention");
            }
        }
    }
}
