use super::{sessions, RecoveryKind, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::client_window::{peer_id_from_window_key, peer_window_key_prefix, ClientWindow};
use serde_json::{json, Value};

const PEER_RECENT_WINDOW_MS: i64 = 10 * 60 * 1_000;
const PEER_AWARENESS_LIMIT: usize = 3;
const PEER_MESSAGE_PROJECTION_LIMIT: usize = 4;

fn normalize_peer_message(
    message: String,
    tags: Vec<String>,
) -> Result<(String, Vec<String>), String> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("peer message must not be empty".to_string());
    }
    if message.chars().count() > sessions::MAX_MESSAGE_CHARS {
        return Err(format!(
            "peer message exceeds {} chars",
            sessions::MAX_MESSAGE_CHARS
        ));
    }
    if tags.len() > sessions::MAX_MESSAGE_TAGS {
        return Err(format!(
            "peer message tags exceed {} items",
            sessions::MAX_MESSAGE_TAGS
        ));
    }
    let mut normalized_tags = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_string();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > sessions::MAX_MESSAGE_TAG_CHARS {
            return Err(format!(
                "peer message tag exceeds {} chars",
                sessions::MAX_MESSAGE_TAG_CHARS
            ));
        }
        if !normalized_tags.iter().any(|existing| existing == &tag) {
            normalized_tags.push(tag);
        }
    }
    Ok((message, normalized_tags))
}

impl ToolRuntime {
    pub(crate) fn post_peer_message_tool(
        &self,
        peer_id: String,
        kind: sessions::SessionMessageKind,
        message: String,
        tags: Vec<String>,
        priority: sessions::SessionMessagePriority,
        requires_ack: bool,
        delivery_key: Option<String>,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
        trusted_recording_session_id: Option<&str>,
        trusted_recording_session_project: Option<&str>,
    ) -> ToolResult {
        let (message, tags) = match normalize_peer_message(message, tags) {
            Ok(normalized) => normalized,
            Err(message) => return peer_error("invalid_peer_message", &message),
        };
        let delivery = match delivery_key {
            Some(delivery_key) => {
                let delivery_key = delivery_key.trim().to_string();
                if delivery_key.is_empty()
                    || delivery_key.chars().count() > sessions::MAX_MESSAGE_DELIVERY_KEY_CHARS
                {
                    return peer_error(
                        "invalid_delivery_key",
                        "delivery_key must contain 1..=128 characters",
                    );
                }
                Some(webcodex_store::PeerMessageDelivery { delivery_key })
            }
            None => None,
        };
        let Some(sender_window) = window else {
            return peer_error(
                "peer_identity_unavailable",
                "peer messaging requires a stable client window identity",
            );
        };
        let Ok((principal_kind, principal_id)) = super::runtime_observation_principal(auth) else {
            return peer_error(
                "peer_principal_unavailable",
                "peer messaging requires a stable authenticated principal",
            );
        };
        let Some(target_prefix) = peer_window_key_prefix(&peer_id) else {
            return peer_error(
                "invalid_peer_id",
                "peer_id must be a canonical wc_peer_* window identity",
            );
        };
        let Some(window_db) = self.window_activity_db.as_ref() else {
            return peer_error(
                "peer_collaboration_unavailable",
                "peer collaboration storage is unavailable",
            );
        };
        let target_window_key = match window_db.resolve_peer_window_prefix(
            &principal_kind,
            &principal_id,
            target_prefix,
        ) {
            Ok(Some(window_key)) => window_key,
            Ok(None) => {
                return peer_error(
                    "peer_not_found",
                    "peer_id is not known for the current principal",
                )
            }
            Err(_) => {
                return peer_error(
                    "peer_collaboration_unavailable",
                    "peer collaboration lookup failed",
                )
            }
        };
        if target_window_key == sender_window.key() {
            return peer_error("peer_self_target", "peer_id refers to the current window");
        }
        let Some(db) = self.communication_db.as_ref() else {
            return peer_error(
                "peer_collaboration_unavailable",
                "peer collaboration storage is unavailable",
            );
        };
        let sender_peer_id = sender_window.peer_id();
        let input = webcodex_store::NewPeerMessage {
            principal_kind,
            principal_id,
            sender_window_key: sender_window.key().to_string(),
            recipient_window_key: target_window_key,
            sender_peer_id: sender_peer_id.clone(),
            recipient_peer_id: peer_id.clone(),
            kind: kind.as_str().to_string(),
            priority: priority_name(priority).to_string(),
            message,
            tags,
            requires_ack,
            sender_session_id: trusted_recording_session_id.map(str::to_string),
            sender_project: trusted_recording_session_project.map(str::to_string),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        };
        let replay_safe_delivery = delivery.is_some();
        match db.post_peer_message_with_delivery(input, delivery) {
            Ok(webcodex_store::PeerMessageDeliveryOutcome::Delivered {
                message,
                replayed,
                state_changed,
            }) => ToolResult::ok(json!({
                "success": true,
                "message_id": message.message_id,
                "sender_peer_id": sender_peer_id,
                "recipient_peer_id": peer_id,
                "requires_ack": message.requires_ack,
                "replayed": replayed,
                "state_changed": state_changed,
            })),
            Ok(webcodex_store::PeerMessageDeliveryOutcome::DeliveryKeyConflict) => {
                ToolResult::err_with_output(
                    "delivery_key_conflict",
                    json!({
                        "error_kind": "delivery_key_conflict",
                        "failure_kind": "conflict",
                        "dispatch_certainty": "not_started",
                        "state_changed": false,
                    }),
                )
            }
            Err(_) if replay_safe_delivery => peer_delivery_persistence_error(),
            Err(_) => peer_error(
                "peer_collaboration_unavailable",
                "peer message persistence failed",
            ),
        }
    }

    pub(crate) fn add_peer_collaboration_projection(
        &self,
        result: &mut ToolResult,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
        project: Option<&str>,
        ack_message_ids: &[String],
    ) {
        let Some(window) = window else {
            return;
        };
        if !result.output.is_object() {
            return;
        }
        let Ok((principal_kind, principal_id)) = super::runtime_observation_principal(auth) else {
            return;
        };
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut peer_messages = None;
        let mut message_rollbacks = Vec::new();
        if let Some(db) = self.communication_db.as_ref() {
            if let Ok(batch) = db.take_peer_attention(
                &principal_kind,
                &principal_id,
                window.key(),
                ack_message_ids,
                now_ms,
                PEER_MESSAGE_PROJECTION_LIMIT,
            ) {
                message_rollbacks = batch.projection_rollbacks;
                if !batch.messages.is_empty() || !batch.accepted_ack_ids.is_empty() {
                    let messages = batch
                        .messages
                        .into_iter()
                        .map(|message| {
                            json!({
                                "message_id": message.message_id,
                                "from_peer_id": message.sender_peer_id,
                                "kind": message.kind,
                                "priority": message.priority,
                                "message": message.message,
                                "tags": message.tags,
                                "requires_ack": message.requires_ack,
                                "created_at_ms": message.created_at_ms,
                                "projection_count": message.projection_count,
                            })
                        })
                        .collect::<Vec<_>>();
                    peer_messages = Some(json!({
                        "messages": messages,
                        "ack": {
                            "accepted_count": batch.accepted_ack_ids.len(),
                            "accepted_ids": batch.accepted_ack_ids,
                        }
                    }));
                }
            }
        }

        let mut peer_awareness = None;
        let mut discovery_rowids = Vec::new();
        if let (Some(project), Some(db)) = (project, self.window_activity_db.as_ref()) {
            if let Ok(peers) = db.take_new_recent_project_peers(
                &principal_kind,
                &principal_id,
                window.key(),
                project,
                now_ms.saturating_sub(PEER_RECENT_WINDOW_MS),
                now_ms,
                PEER_AWARENESS_LIMIT,
            ) {
                let mut projected_peers = Vec::new();
                for peer in peers {
                    if let Some(rowid) = peer.discovery_rowid {
                        discovery_rowids.push(rowid);
                    }
                    if let Some(peer_id) = peer_id_from_window_key(&peer.client_window_key) {
                        projected_peers.push(json!({
                            "peer_id": peer_id,
                            "source": peer.client_window_source,
                            "last_meaningful_activity_at_ms": peer.last_meaningful_activity_at_ms,
                        }));
                    }
                }
                if !projected_peers.is_empty() {
                    peer_awareness = Some(json!({
                        "self_peer_id": window.peer_id(),
                        "project": project,
                        "recent_window_secs": PEER_RECENT_WINDOW_MS / 1_000,
                        "new_peers": projected_peers,
                        "semantics": "recent_same_project_activity_not_liveness",
                    }));
                }
            }
        }

        let output = result
            .output
            .as_object_mut()
            .expect("peer collaboration requires object ToolResult output");
        if let Some(peer_messages) = peer_messages {
            output.insert("peer_messages".to_string(), peer_messages);
        }
        if let Some(peer_awareness) = peer_awareness {
            output.insert("peer_awareness".to_string(), peer_awareness);
        }

        let oversized = crate::json_measurement::serialized_json_len(result).is_ok_and(|bytes| {
            bytes > webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES
        });
        if !oversized {
            return;
        }
        if let Some(output) = result.output.as_object_mut() {
            output.remove("peer_awareness");
            output.remove("peer_messages");
        }
        if let Some(db) = self.communication_db.as_ref() {
            if let Err(error) = db.rollback_peer_attention(
                &principal_kind,
                &principal_id,
                window.key(),
                now_ms,
                &message_rollbacks,
            ) {
                tracing::warn!(
                    ?error,
                    "failed to roll back oversized Peer message projection"
                );
            }
        }
        if let (Some(project), Some(db)) = (project, self.window_activity_db.as_ref()) {
            if let Err(error) = db.rollback_peer_discoveries(
                &principal_kind,
                &principal_id,
                window.key(),
                project,
                &discovery_rowids,
            ) {
                tracing::warn!(
                    ?error,
                    "failed to roll back oversized Peer awareness projection"
                );
            }
        }
    }

    /// Decorate an MCP CallToolResult produced by a specialized fast path that bypasses the
    /// canonical ToolRuntime kernel. Standard WebCodex structuredContent keeps Peer overlays
    /// inside `output`; native Plugin/MCP/SSH structuredContent receives the same fields at its
    /// root. `structuredContent` is the canonical model-readable channel for both shapes.
    pub(crate) fn add_peer_collaboration_to_mcp_call_result(
        &self,
        call_result: &mut Value,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
        project: Option<&str>,
        ack_message_ids: &[String],
    ) {
        let Some(structured) = call_result
            .get_mut("structuredContent")
            .and_then(Value::as_object_mut)
        else {
            return;
        };

        let standard_tool_result = structured.get("success").is_some_and(Value::is_boolean)
            && structured.get("output").is_some_and(Value::is_object);
        if standard_tool_result {
            let success = structured
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let error = structured
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string);
            let output = structured
                .get_mut("output")
                .map(std::mem::take)
                .unwrap_or_else(|| json!({}));
            let mut result = ToolResult {
                success,
                output,
                error,
            };
            self.add_peer_collaboration_projection(
                &mut result,
                auth,
                window,
                project,
                ack_message_ids,
            );

            structured.insert("output".to_string(), result.output);
            return;
        }

        let output = Value::Object(std::mem::take(structured));
        let mut result = ToolResult::ok(output);
        self.add_peer_collaboration_projection(&mut result, auth, window, project, ack_message_ids);

        if let Value::Object(output) = result.output {
            *structured = output;
        }
    }
}

fn priority_name(priority: sessions::SessionMessagePriority) -> &'static str {
    match priority {
        sessions::SessionMessagePriority::Low => "low",
        sessions::SessionMessagePriority::Normal => "normal",
        sessions::SessionMessagePriority::High => "high",
    }
}

fn peer_delivery_persistence_error() -> ToolResult {
    ToolResult::err_with_output(
        "peer message persistence outcome is unknown",
        json!({
            "error_kind": "peer_message_persistence_uncertain",
            "failure_kind": "outcome_unknown",
            "state_changed": null,
            "retry_same_delivery": true,
        }),
    )
    .with_recovery(RecoveryKind::RetrySame)
}

fn peer_error(kind: &str, message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message,
        json!({
            "failure_kind": kind,
            "dispatch_certainty": "not_started",
        }),
    )
}

#[cfg(test)]
#[test]
fn keyed_peer_persistence_error_projects_outcome_unknown_retry_same() {
    let result = peer_delivery_persistence_error();
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert!(result.output["state_changed"].is_null());
    assert_eq!(result.output["retry_same_delivery"], true);
    assert_eq!(result.output["recovery_kind"], "retry_same");
}

#[cfg(test)]
pub(crate) fn peer_recent_window_ms_for_tests() -> i64 {
    PEER_RECENT_WINDOW_MS
}
