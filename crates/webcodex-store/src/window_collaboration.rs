use crate::{Database, PeerProjectionRollback, StoreDomain};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Shared read-only projection for the Console and MCP App.
#[derive(Debug, Clone, Serialize)]
pub struct WindowCollaborationMessage {
    pub message_id: String,
    pub source: String,
    pub direction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<String>,
    pub message: String,
    pub created_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_project: Option<String>,
    pub requires_ack: bool,
    pub first_projected_at_ms: Option<i64>,
    pub first_ack_observed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewWindowOperatorMessage {
    pub principal_kind: String,
    pub principal_id: String,
    pub recipient_window_key: String,
    pub context_session_id: Option<String>,
    pub context_project: Option<String>,
    pub kind: String,
    pub priority: String,
    pub message: String,
    pub tags: Vec<String>,
    pub requires_ack: bool,
    #[serde(skip)]
    pub created_at_ms: i64,
}

pub enum WindowOperatorDeliveryOutcome {
    Delivered { message_id: String, replayed: bool },
    DeliveryKeyConflict,
}

#[derive(Default)]
pub struct WindowOperatorAttention {
    pub messages: Vec<WindowCollaborationMessage>,
    pub accepted_ack_ids: Vec<String>,
    pub projection_rollbacks: Vec<PeerProjectionRollback>,
}

fn transcript_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WindowCollaborationMessage> {
    Ok(WindowCollaborationMessage {
        message_id: row.get(0)?,
        source: row.get(1)?,
        direction: row.get(2)?,
        peer_id: row.get(3)?,
        message: row.get(4)?,
        created_at_ms: row.get(5)?,
        context_session_id: row.get(6)?,
        context_project: row.get(7)?,
        requires_ack: row.get(8)?,
        first_projected_at_ms: row.get(9)?,
        first_ack_observed_at_ms: row.get(10)?,
    })
}

const OPERATOR_SELECT: &str = "SELECT message_id, 'operator', 'inbound', NULL, message,
    created_at_ms, context_session_id, context_project, requires_ack,
    first_projected_at_ms, first_ack_observed_at_ms FROM window_operator_messages";

impl Database {
    pub fn window_has_session_context(
        &self,
        kind: &str,
        principal: &str,
        window: &str,
        session: &str,
    ) -> anyhow::Result<bool> {
        let conn = self.lock_connection(StoreDomain::WindowActivity);
        Ok(conn.query_row("SELECT 1 FROM action_event_workflow_links l JOIN action_events e ON e.event_id=l.event_id
            WHERE e.principal_correlation_kind=?1 AND e.principal_correlation_id=?2 AND e.client_window_key=?3
            AND l.workflow_session_id=?4 LIMIT 1", params![kind,principal,window,session], |_| Ok(())).optional()?.is_some())
    }

    pub fn post_window_operator_message(
        &self,
        input: NewWindowOperatorMessage,
        delivery_key: &str,
    ) -> anyhow::Result<WindowOperatorDeliveryOutcome> {
        anyhow::ensure!(
            !delivery_key.trim().is_empty() && delivery_key.chars().count() <= 128,
            "invalid delivery key"
        );
        anyhow::ensure!(
            !input.message.trim().is_empty() && input.message.chars().count() <= 8000,
            "invalid message"
        );
        let key_hash = format!("{:x}", Sha256::digest(delivery_key.as_bytes()));
        // Includes the exact recipient and optional contexts; the timestamp is not retry identity.
        let payload_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&input)?));
        let mut conn = self.lock_connection(StoreDomain::Communication);
        let tx = conn.transaction()?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT message_id, delivery_payload_hash FROM window_operator_messages
             WHERE principal_kind=?1 AND principal_id=?2 AND delivery_key_hash=?3",
                params![input.principal_kind, input.principal_id, key_hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((message_id, previous)) = existing {
            return Ok(if previous == payload_hash {
                WindowOperatorDeliveryOutcome::Delivered {
                    message_id,
                    replayed: true,
                }
            } else {
                WindowOperatorDeliveryOutcome::DeliveryKeyConflict
            });
        }
        let message_id = format!("wc_msg_{}", webcodex_core::compact::random_suffix::<12>());
        tx.execute("INSERT INTO window_operator_messages (
            message_id, principal_kind, principal_id, recipient_window_key, context_session_id,
            context_project, kind, priority, message, tags_json, requires_ack, created_at_ms,
            delivery_key_hash, delivery_payload_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![message_id,input.principal_kind,input.principal_id,input.recipient_window_key,
                input.context_session_id,input.context_project,input.kind,input.priority,input.message,
                serde_json::to_string(&input.tags)?,input.requires_ack,input.created_at_ms,key_hash,payload_hash])?;
        tx.execute(
            "DELETE FROM window_operator_messages WHERE rowid IN (
            SELECT rowid FROM window_operator_messages WHERE principal_kind=?1 AND principal_id=?2
            ORDER BY created_at_ms DESC, message_id DESC LIMIT -1 OFFSET 512)",
            params![input.principal_kind, input.principal_id],
        )?;
        tx.commit()?;
        Ok(WindowOperatorDeliveryOutcome::Delivered {
            message_id,
            replayed: false,
        })
    }

    pub fn list_window_operator_messages(
        &self,
        kind: &str,
        principal: &str,
        window: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<WindowCollaborationMessage>> {
        let conn = self.lock_connection(StoreDomain::Communication);
        let mut stmt = conn.prepare(&format!(
            "{OPERATOR_SELECT} WHERE principal_kind=?1 AND principal_id=?2
            AND recipient_window_key=?3 ORDER BY created_at_ms DESC, message_id DESC LIMIT ?4"
        ))?;
        let mut messages = stmt
            .query_map(
                params![kind, principal, window, limit.clamp(1, 512) as i64],
                transcript_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        messages.reverse();
        Ok(messages)
    }

    pub fn list_window_peer_messages(
        &self,
        kind: &str,
        principal: &str,
        window: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<WindowCollaborationMessage>> {
        let conn = self.lock_connection(StoreDomain::Communication);
        let mut stmt = conn.prepare("SELECT message_id, 'peer',
            CASE WHEN recipient_window_key=?3 THEN 'inbound' ELSE 'outbound' END,
            CASE WHEN recipient_window_key=?3 THEN sender_peer_id ELSE recipient_peer_id END,
            message, created_at_ms, sender_session_id, sender_project, requires_ack,
            first_projected_at_ms, first_ack_observed_at_ms FROM window_peer_messages
            WHERE principal_kind=?1 AND principal_id=?2 AND (recipient_window_key=?3 OR sender_window_key=?3)
            ORDER BY created_at_ms DESC, message_id DESC LIMIT ?4")?;
        let mut messages = stmt
            .query_map(
                params![kind, principal, window, limit.clamp(1, 512) as i64],
                transcript_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        messages.reverse();
        Ok(messages)
    }

    pub fn window_collaboration_transcript(
        &self,
        kind: &str,
        principal: &str,
        window: &str,
        limit: usize,
    ) -> anyhow::Result<(Vec<WindowCollaborationMessage>, bool)> {
        let limit = limit.clamp(1, 100);
        let mut messages =
            self.list_window_operator_messages(kind, principal, window, limit + 1)?;
        messages.extend(self.list_window_peer_messages(kind, principal, window, limit + 1)?);
        messages.sort_by(|a, b| {
            (a.created_at_ms, &a.message_id).cmp(&(b.created_at_ms, &b.message_id))
        });
        let truncated = messages.len() > limit;
        messages.drain(..messages.len().saturating_sub(limit));
        Ok((messages, truncated))
    }

    pub fn take_window_operator_attention(
        &self,
        kind: &str,
        principal: &str,
        window: &str,
        ack_ids: &[String],
        now: i64,
        limit: usize,
    ) -> anyhow::Result<WindowOperatorAttention> {
        let mut conn = self.lock_connection(StoreDomain::Communication);
        let tx = conn.transaction()?;
        let mut batch = WindowOperatorAttention::default();
        for id in ack_ids.iter().take(8) {
            if tx.execute("UPDATE window_operator_messages SET first_ack_observed_at_ms=COALESCE(first_ack_observed_at_ms,?5)
                WHERE message_id=?1 AND principal_kind=?2 AND principal_id=?3 AND recipient_window_key=?4
                AND requires_ack=1 AND first_projected_at_ms IS NOT NULL", params![id,kind,principal,window,now])? > 0 {
                batch.accepted_ack_ids.push(id.clone());
            }
        }
        {
            let mut stmt = tx.prepare(&format!(
                "{OPERATOR_SELECT} WHERE principal_kind=?1 AND principal_id=?2
                AND recipient_window_key=?3 AND first_ack_observed_at_ms IS NULL
                AND (first_projected_at_ms IS NULL OR requires_ack=1)
                ORDER BY (first_projected_at_ms IS NULL) DESC, created_at_ms, message_id LIMIT ?4"
            ))?;
            batch.messages = stmt
                .query_map(
                    params![kind, principal, window, limit.clamp(1, 8) as i64],
                    transcript_row,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        for message in &mut batch.messages {
            batch.projection_rollbacks.push(tx.query_row(
                "SELECT last_projected_at_ms, projection_count
                FROM window_operator_messages WHERE message_id=?1",
                params![message.message_id],
                |row| {
                    Ok(PeerProjectionRollback {
                        message_id: message.message_id.clone(),
                        first_projected_at_ms: message.first_projected_at_ms,
                        last_projected_at_ms: row.get(0)?,
                        projection_count: row.get::<_, i64>(1)? as usize,
                    })
                },
            )?);
            tx.execute("UPDATE window_operator_messages SET first_projected_at_ms=COALESCE(first_projected_at_ms,?2),
                last_projected_at_ms=?2, projection_count=projection_count+1 WHERE message_id=?1",params![message.message_id,now])?;
            message.first_projected_at_ms.get_or_insert(now);
        }
        tx.commit()?;
        Ok(batch)
    }

    pub fn rollback_window_operator_attention(
        &self,
        kind: &str,
        principal: &str,
        window: &str,
        now: i64,
        rollbacks: &[PeerProjectionRollback],
    ) -> anyhow::Result<()> {
        let mut conn = self.lock_connection(StoreDomain::Communication);
        let tx = conn.transaction()?;
        for r in rollbacks {
            tx.execute("UPDATE window_operator_messages SET first_projected_at_ms=?5,last_projected_at_ms=?6,projection_count=?7
                WHERE message_id=?1 AND principal_kind=?2 AND principal_id=?3 AND recipient_window_key=?4
                AND last_projected_at_ms=?8 AND projection_count=?9", params![r.message_id,kind,principal,window,
                    r.first_projected_at_ms,r.last_projected_at_ms,r.projection_count as i64,now,(r.projection_count+1) as i64])?;
        }
        tx.commit()?;
        Ok(())
    }
}
