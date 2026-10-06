use super::{ToolResult, ToolRuntime};
use crate::auth::{AuthContext, SCOPE_SESSION_COLLABORATE};
use crate::client_window::ClientWindow;
use serde_json::json;
use sha2::{Digest, Sha256};

pub(crate) const TOOL_CALL_WINDOW_REPLY_FIELD: &str = "window_reply";
pub(crate) const MAX_WINDOW_REPLY_CHARS: usize = 8_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolCallWindowReply {
    pub(crate) reply_to_message_id: String,
    pub(crate) message: String,
}

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

fn valid_window_message_kind(value: &str) -> bool {
    matches!(
        value,
        "note"
            | "proposal"
            | "question"
            | "answer"
            | "decision"
            | "risk"
            | "progress"
            | "guidance"
            | "todo"
    )
}

fn valid_window_message_priority(value: &str) -> bool {
    matches!(value, "low" | "normal" | "high")
}

impl ToolRuntime {
    #[cfg(test)]
    pub(crate) async fn post_window_operator_message(
        &self,
        target_window_key: &str,
        context_session_id: Option<&str>,
        context_project: Option<&str>,
        message: String,
        delivery_key: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.post_window_operator_message_with_options(
            target_window_key,
            context_session_id,
            context_project,
            "guidance",
            "normal",
            true,
            message,
            delivery_key,
            auth,
        )
        .await
    }

    pub(crate) async fn post_window_operator_message_with_options(
        &self,
        target_window_key: &str,
        context_session_id: Option<&str>,
        context_project: Option<&str>,
        message_kind: &str,
        priority: &str,
        requires_ack: bool,
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
        self.post_window_operator_message_with_options_for_principal(
            target_window_key,
            context_session_id,
            context_project,
            message_kind,
            priority,
            requires_ack,
            message,
            delivery_key,
            &kind,
            &principal,
            auth,
        )
        .await
    }

    pub(crate) async fn post_window_operator_message_with_options_for_principal(
        &self,
        target_window_key: &str,
        context_session_id: Option<&str>,
        context_project: Option<&str>,
        message_kind: &str,
        priority: &str,
        requires_ack: bool,
        message: String,
        delivery_key: String,
        recipient_principal_kind: &str,
        recipient_principal_id: &str,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !auth.is_some_and(|a| a.has_scope(SCOPE_SESSION_COLLABORATE)) {
            return ToolResult::err("collaboration scope required");
        }
        if !valid_window_message_kind(message_kind) || !valid_window_message_priority(priority) {
            return ToolResult::err_with_output(
                "invalid Window collaboration metadata",
                json!({
                    "failure_kind": "invalid_arguments",
                    "state_changed": false,
                    "retry_same_delivery": false,
                    "dispatch_certainty": "not_started",
                }),
            );
        }
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
                .authorize_session_target(session, "send_work_result_message", auth)
                .await
                .is_err()
            {
                return invalid_window_context(
                    "session_context_unavailable",
                    "Session context is no longer available",
                );
            }
            let related = self.window_activity_db.as_ref().is_some_and(|db| {
                db.window_has_session_context(
                    recipient_principal_kind,
                    recipient_principal_id,
                    target_window_key,
                    session,
                )
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
            principal_kind: recipient_principal_kind.to_string(),
            principal_id: recipient_principal_id.to_string(),
            recipient_window_key: target_window_key.to_string(),
            context_session_id: context_session_id.map(str::to_string),
            context_project: canonical_context_project,
            kind: message_kind.to_string(),
            priority: priority.to_string(),
            message,
            tags: Vec::new(),
            requires_ack,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        };
        match db.post_window_operator_message(input, &delivery_key) {
            Ok(webcodex_store::WindowOperatorDeliveryOutcome::Delivered {message_id,replayed}) =>
                ToolResult::ok(json!({
                    "success": true,
                    "message_id": message_id,
                    "replayed": replayed,
                    "state_changed": !replayed,
                    "kind": message_kind,
                    "priority": priority,
                    "requires_ack": requires_ack
                })),
            Ok(webcodex_store::WindowOperatorDeliveryOutcome::DeliveryKeyConflict) =>
                ToolResult::err_with_output("delivery_key_conflict",json!({"failure_kind":"conflict","state_changed":false,"dispatch_certainty":"not_started"})),
            Err(_) => ToolResult::err_with_output("Operator message persistence outcome is unknown",
                json!({"failure_kind":"outcome_unknown","state_changed":null,"retry_same_delivery":true}))
                .with_recovery(super::RecoveryKind::RetrySame),
        }
    }

    pub(crate) fn add_window_model_reply_sidecar(
        &self,
        result: &mut ToolResult,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
        reply: Option<&ToolCallWindowReply>,
    ) {
        let Some(reply) = reply else {
            return;
        };
        if !result.output.is_object() {
            return;
        }
        let reply_result = if !auth.is_some_and(|a| a.has_scope(SCOPE_SESSION_COLLABORATE)) {
            json!({
                "success": false,
                "failure_kind": "insufficient_scope",
                "state_changed": false
            })
        } else if let (Some(window), Some(db), Ok((kind, principal))) = (
            window,
            self.communication_db.as_ref(),
            super::runtime_observation_principal(auth),
        ) {
            let message_digest = format!("{:x}", Sha256::digest(reply.message.as_bytes()));
            let delivery_key = format!(
                "window-reply:{}:{}",
                reply.reply_to_message_id,
                &message_digest[..24]
            );
            let input = webcodex_store::NewWindowModelReply {
                principal_kind: kind,
                principal_id: principal,
                window_key: window.key().to_string(),
                reply_to_message_id: reply.reply_to_message_id.clone(),
                message: reply.message.clone(),
                created_at_ms: chrono::Utc::now().timestamp_millis(),
            };
            match db.post_window_model_reply(input, &delivery_key) {
                Ok(webcodex_store::WindowModelReplyDeliveryOutcome::Delivered {
                    message_id,
                    replayed,
                }) => json!({
                    "success": true,
                    "message_id": message_id,
                    "reply_to": reply.reply_to_message_id,
                    "replayed": replayed,
                    "state_changed": !replayed
                }),
                Ok(webcodex_store::WindowModelReplyDeliveryOutcome::ReplyTargetNotFound) => json!({
                    "success": false,
                    "failure_kind": "invalid_context",
                    "error_kind": "operator_message_unavailable",
                    "state_changed": false
                }),
                Ok(webcodex_store::WindowModelReplyDeliveryOutcome::DeliveryKeyConflict) => json!({
                    "success": false,
                    "failure_kind": "conflict",
                    "state_changed": false
                }),
                Err(error) => {
                    tracing::warn!(?error, "Window model reply persistence outcome is unknown");
                    json!({
                        "success": false,
                        "failure_kind": "outcome_unknown",
                        "state_changed": null
                    })
                }
            }
        } else {
            json!({
                "success": false,
                "failure_kind": "unavailable",
                "state_changed": false
            })
        };
        result.output["window_reply"] = reply_result;
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
        let (Some(window), Ok((kind, principal))) =
            (window_key, super::runtime_observation_principal(auth))
        else {
            return unavailable();
        };
        self.window_collaboration_for_principal(window, &kind, &principal, limit)
    }

    pub(crate) fn window_collaboration_for_principal(
        &self,
        window: &str,
        recipient_principal_kind: &str,
        recipient_principal_id: &str,
        limit: usize,
    ) -> serde_json::Value {
        self.window_collaboration_page_for_principal(
            window,
            recipient_principal_kind,
            recipient_principal_id,
            limit,
            None,
        )
    }

    pub(crate) fn window_collaboration_page_for_principal(
        &self,
        window: &str,
        kind: &str,
        principal: &str,
        limit: usize,
        before: Option<&str>,
    ) -> serde_json::Value {
        let unavailable = || json!({"available":false,"can_send":false,"messages":[]});
        let Some(db) = self.communication_db.as_ref() else {
            return unavailable();
        };
        match db.window_collaboration_page(kind, principal, window, limit, before) {
            Ok(Some((messages, truncated))) => {
                let next_before = if truncated {
                    messages.first().map(|message| message.message_id.clone())
                } else {
                    None
                };
                // Opaque cache namespace only; a cursor/scope never grants authority.
                let history_scope = format!(
                    "{:x}",
                    Sha256::digest(serde_json::to_vec(&(kind, principal, window)).unwrap())
                );
                json!({"available":true,"can_send":true,"returned":messages.len(),"messages":messages,
                    "truncated":truncated,"next_before":next_before,"history_scope":history_scope})
            }
            Ok(None) => {
                json!({"available":false,"can_send":false,"messages":[],"error_kind":"history_cursor_unavailable"})
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
        self.add_window_operator_projection_until(
            result,
            auth,
            window,
            ack_ids,
            super::optional_enrichment::deadline(),
        );
    }

    pub(crate) fn add_window_operator_projection_until(
        &self,
        result: &mut ToolResult,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
        ack_ids: &[String],
        deadline: std::time::Instant,
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
        let started = std::time::Instant::now();
        let value = |batch: &webcodex_store::WindowOperatorAttention| json!({"messages": batch.messages, "ack": {"accepted_ids": batch.accepted_ack_ids}});
        let outcome = match db.project_window_operator_attention(
            &kind,
            &principal,
            window.key(),
            ack_ids,
            chrono::Utc::now().timestamp_millis(),
            4,
            Some(deadline),
            |batch| super::optional_enrichment::fits(result, "operator_messages", value(batch)),
        ) {
            Ok(Some(batch)) => {
                if !batch.messages.is_empty() || !batch.accepted_ack_ids.is_empty() {
                    let value = value(&batch);
                    if super::optional_enrichment::fits(result, "operator_messages", value.clone())
                    {
                        result.output["operator_messages"] = value;
                    }
                }
                "completed"
            }
            Ok(None) => super::optional_enrichment::omitted(deadline),
            Err(_) => "store_error",
        };
        crate::tool_request_trace::record_phase_latency("operator_attention", started, outcome);
    }
}
