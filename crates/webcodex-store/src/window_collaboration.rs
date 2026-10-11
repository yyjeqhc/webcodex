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
    pub reply_to_message_id: Option<String>,
    pub kind: String,
    pub priority: String,
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

#[derive(Debug, Clone, Serialize)]
pub struct NewWindowModelReply {
    pub principal_kind: String,
    pub principal_id: String,
    pub window_key: String,
    pub reply_to_message_id: String,
    pub message: String,
    #[serde(skip)]
    pub created_at_ms: i64,
}

pub enum WindowModelReplyDeliveryOutcome {
    Delivered { message_id: String, replayed: bool },
    DeliveryKeyConflict,
    ReplyTargetNotFound,
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

pub(super) fn transcript_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<WindowCollaborationMessage> {
    Ok(WindowCollaborationMessage {
        message_id: row.get(0)?,
        source: row.get(1)?,
        direction: row.get(2)?,
        peer_id: row.get(3)?,
        message: row.get(4)?,
        created_at_ms: row.get(5)?,
        reply_to_message_id: row.get(6)?,
        kind: row.get(7)?,
        priority: row.get(8)?,
        context_session_id: row.get(9)?,
        context_project: row.get(10)?,
        requires_ack: row.get(11)?,
        first_projected_at_ms: row.get(12)?,
        first_ack_observed_at_ms: row.get(13)?,
    })
}

const OPERATOR_SELECT: &str = "SELECT message_id, 'operator', 'inbound', NULL, message,
    created_at_ms, NULL, kind, priority, context_session_id, context_project, requires_ack,
    first_projected_at_ms, first_ack_observed_at_ms FROM window_operator_messages";

/// Stable, domain-separated Window Operator mailbox identity for managed OAuth
/// credentials. The Window key is a separate required predicate on every read,
/// ACK, write and reply; neither component is authority by itself.
pub fn managed_oauth_operator_principal(
    user_id: &str,
    oauth_client_id: &str,
) -> Option<(String, String)> {
    if user_id.trim().is_empty() || oauth_client_id.trim().is_empty() {
        return None;
    }
    let mut digest = Sha256::new();
    digest.update(b"webcodex.window-operator.oauth-managed.v1\0");
    for part in [user_id, oauth_client_id] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    Some((
        "oauth2-window-operator".to_string(),
        format!("{:x}", digest.finalize()),
    ))
}

impl Database {
    /// Move provably owned legacy OAuth Operator mailboxes to a stable
    /// user+OAuth-client namespace, preserving message ids, projection/ACK
    /// counters, replies, and replay keys. This runs before expired OAuth
    /// token cleanup when the database is opened. Other credential kinds and
    /// ambiguous/missing provenance stay untouched (fail closed).
    ///
    /// Bound the startup batch: any remainder is eligible on the next open.
    /// Neither this migration nor a Window key grants model-access authority.
    pub fn migrate_legacy_managed_oauth_operator_messages(&self) -> anyhow::Result<usize> {
        let candidates: Vec<(String, String)> = {
            let conn = self.lock_connection(StoreDomain::Communication);
            let mut stmt = conn.prepare(
                "SELECT DISTINCT principal_id, recipient_window_key
                 FROM window_operator_messages WHERE principal_kind='oauth2'
                 ORDER BY recipient_window_key, principal_id LIMIT 4096",
            )?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut migrated = 0;
        for (legacy_token_id, window) in candidates {
            let Some((user, client)) =
                self.oauth_managed_window_recipient(&window, &legacy_token_id)?
            else {
                continue;
            };
            let Some((stable_kind, stable_id)) = managed_oauth_operator_principal(&user, &client)
            else {
                continue;
            };
            let mut conn = self.lock_connection(StoreDomain::Communication);
            let tx = conn.transaction()?;
            let legacy_rows: Vec<(
                String,
                Option<String>,
                Option<String>,
                String,
                String,
                String,
                String,
                bool,
                i64,
                String,
            )> = {
                let mut stmt = tx.prepare(
                    "SELECT message_id,context_session_id,context_project,kind,priority,
                            message,tags_json,requires_ack,created_at_ms,delivery_key_hash
                     FROM window_operator_messages
                     WHERE principal_kind='oauth2' AND principal_id=?1
                       AND recipient_window_key=?2",
                )?;
                let rows = stmt.query_map(params![legacy_token_id, window], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                    ))
                })?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            for (
                message_id,
                context_session_id,
                context_project,
                kind,
                priority,
                message,
                tags_json,
                requires_ack,
                created_at_ms,
                delivery_key_hash,
            ) in legacy_rows
            {
                let Ok(tags) = serde_json::from_str::<Vec<String>>(&tags_json) else {
                    continue;
                };
                let new_operator = NewWindowOperatorMessage {
                    principal_kind: stable_kind.clone(),
                    principal_id: stable_id.clone(),
                    recipient_window_key: window.clone(),
                    context_session_id,
                    context_project,
                    kind,
                    priority,
                    message,
                    tags,
                    requires_ack,
                    created_at_ms,
                };
                let payload_hash =
                    format!("{:x}", Sha256::digest(serde_json::to_vec(&new_operator)?));
                let operator_collision: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM window_operator_messages
                     WHERE principal_kind=?1 AND principal_id=?2
                       AND delivery_key_hash=?3 AND message_id<>?4)",
                    params![stable_kind, stable_id, delivery_key_hash, message_id],
                    |row| row.get(0),
                )?;
                if operator_collision {
                    continue;
                }
                let reply_rows: Vec<(String, String, i64, String)> = {
                    let mut stmt = tx.prepare(
                        "SELECT message_id,message,created_at_ms,delivery_key_hash
                         FROM window_model_replies WHERE principal_kind='oauth2'
                           AND principal_id=?1 AND window_key=?2
                           AND reply_to_message_id=?3",
                    )?;
                    let rows = stmt
                        .query_map(params![legacy_token_id, window, message_id], |row| {
                            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                        })?;
                    rows.collect::<rusqlite::Result<_>>()?
                };
                let mut migrated_replies = Vec::with_capacity(reply_rows.len());
                let mut conflict = false;
                for (reply_id, reply_body, reply_created_at_ms, reply_key_hash) in reply_rows {
                    let replay_collision: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM window_model_replies
                         WHERE principal_kind=?1 AND principal_id=?2
                           AND delivery_key_hash=?3 AND message_id<>?4)",
                        params![stable_kind, stable_id, reply_key_hash, reply_id],
                        |row| row.get(0),
                    )?;
                    if replay_collision {
                        conflict = true;
                        break;
                    }
                    let stable_reply = NewWindowModelReply {
                        principal_kind: stable_kind.clone(),
                        principal_id: stable_id.clone(),
                        window_key: window.clone(),
                        reply_to_message_id: message_id.clone(),
                        message: reply_body,
                        created_at_ms: reply_created_at_ms,
                    };
                    let reply_payload_hash =
                        format!("{:x}", Sha256::digest(serde_json::to_vec(&stable_reply)?));
                    migrated_replies.push((reply_id, reply_payload_hash));
                }
                if conflict {
                    continue;
                }
                tx.execute(
                    "UPDATE window_operator_messages
                     SET principal_kind=?1, principal_id=?2, delivery_payload_hash=?3
                     WHERE message_id=?4 AND principal_kind='oauth2' AND principal_id=?5
                       AND recipient_window_key=?6",
                    params![
                        stable_kind,
                        stable_id,
                        payload_hash,
                        message_id,
                        legacy_token_id,
                        window
                    ],
                )?;
                for (reply_id, reply_payload_hash) in migrated_replies {
                    tx.execute(
                        "UPDATE window_model_replies
                         SET principal_kind=?1, principal_id=?2, delivery_payload_hash=?3
                         WHERE message_id=?4 AND principal_kind='oauth2' AND principal_id=?5
                           AND window_key=?6 AND reply_to_message_id=?7",
                        params![
                            stable_kind,
                            stable_id,
                            reply_payload_hash,
                            reply_id,
                            legacy_token_id,
                            window,
                            message_id
                        ],
                    )?;
                }
                migrated += 1;
            }
            tx.commit()?;
        }
        Ok(migrated)
    }

    /// Read-only, Window-fenced attribution of an OAuth access-token observation.
    /// Console already authorizes the exact Window before using this lookup.
    /// Historic tokens may have been pruned, so use only server-authored ActionAudit
    /// user/client fields for an exact (Window, access-token id) fallback.
    /// A missing or contradictory binding never grants a different principal.
    pub fn oauth_managed_window_recipient(
        &self,
        window: &str,
        access_token_id: &str,
    ) -> anyhow::Result<Option<(String, String)>> {
        let conn = self.lock_connection(StoreDomain::WindowActivity);
        let grant: Option<(String, Option<String>, String)> = conn
            .query_row(
                "SELECT subject_kind, user_id, client_id FROM oauth_access_tokens WHERE id=?1",
                params![access_token_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((kind, user, client)) = grant {
            if kind != "managed_user" {
                return Ok(None);
            }
            return Ok(user
                .filter(|user| !user.trim().is_empty() && !client.trim().is_empty())
                .map(|user| (user, client)));
        }
        let mut stmt = conn.prepare(
            "SELECT DISTINCT principal_user_id, oauth_client_id FROM action_events
             WHERE client_window_key=?1 AND principal_correlation_kind='oauth2'
             AND principal_correlation_id=?2 AND principal_kind='oauth2'
             AND principal_user_id IS NOT NULL AND principal_user_id != ''
             AND oauth_client_id IS NOT NULL AND oauth_client_id != ''
             LIMIT 2",
        )?;
        let mut rows = stmt.query(params![window, access_token_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let identity: (String, String) = (row.get(0)?, row.get(1)?);
        Ok(rows.next()?.is_none().then_some(identity))
    }

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
        tx.commit()?;
        Ok(WindowOperatorDeliveryOutcome::Delivered {
            message_id,
            replayed: false,
        })
    }

    pub fn post_window_model_reply(
        &self,
        input: NewWindowModelReply,
        delivery_key: &str,
    ) -> anyhow::Result<WindowModelReplyDeliveryOutcome> {
        anyhow::ensure!(
            !delivery_key.trim().is_empty() && delivery_key.chars().count() <= 128,
            "invalid delivery key"
        );
        anyhow::ensure!(
            !input.message.trim().is_empty() && input.message.chars().count() <= 8000,
            "invalid message"
        );
        anyhow::ensure!(
            webcodex_core::workflow_session_contract::is_valid_session_message_id(
                &input.reply_to_message_id
            ),
            "invalid reply target"
        );
        let key_hash = format!("{:x}", Sha256::digest(delivery_key.as_bytes()));
        let payload_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&input)?));
        let mut conn = self.lock_connection(StoreDomain::Communication);
        let tx = conn.transaction()?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT message_id, delivery_payload_hash FROM window_model_replies
                 WHERE principal_kind=?1 AND principal_id=?2 AND delivery_key_hash=?3",
                params![input.principal_kind, input.principal_id, key_hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((message_id, previous)) = existing {
            return Ok(if previous == payload_hash {
                WindowModelReplyDeliveryOutcome::Delivered {
                    message_id,
                    replayed: true,
                }
            } else {
                WindowModelReplyDeliveryOutcome::DeliveryKeyConflict
            });
        }
        let target_exists = tx
            .query_row(
                "SELECT 1 FROM window_operator_messages
                 WHERE message_id=?1 AND principal_kind=?2 AND principal_id=?3
                 AND recipient_window_key=?4 AND first_projected_at_ms IS NOT NULL LIMIT 1",
                params![
                    input.reply_to_message_id,
                    input.principal_kind,
                    input.principal_id,
                    input.window_key
                ],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !target_exists {
            return Ok(WindowModelReplyDeliveryOutcome::ReplyTargetNotFound);
        }
        let message_id = format!("wc_msg_{}", webcodex_core::compact::random_suffix::<12>());
        tx.execute(
            "INSERT INTO window_model_replies (
                message_id, principal_kind, principal_id, window_key, reply_to_message_id,
                message, created_at_ms, delivery_key_hash, delivery_payload_hash
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                message_id,
                input.principal_kind,
                input.principal_id,
                input.window_key,
                input.reply_to_message_id,
                input.message,
                input.created_at_ms,
                key_hash,
                payload_hash
            ],
        )?;
        tx.commit()?;
        Ok(WindowModelReplyDeliveryOutcome::Delivered {
            message_id,
            replayed: false,
        })
    }

    pub fn list_window_model_replies(
        &self,
        kind: &str,
        principal: &str,
        window: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<WindowCollaborationMessage>> {
        let conn = self.lock_connection(StoreDomain::Communication);
        let mut stmt = conn.prepare(
            "SELECT r.message_id, 'window', 'outbound', NULL, r.message, r.created_at_ms,
                    r.reply_to_message_id, 'answer', 'normal',
                    o.context_session_id, o.context_project, 0, NULL, NULL
             FROM window_model_replies r
             LEFT JOIN window_operator_messages o
               ON o.message_id=r.reply_to_message_id
              AND o.principal_kind=r.principal_kind
              AND o.principal_id=r.principal_id
              AND o.recipient_window_key=r.window_key
             WHERE r.principal_kind=?1 AND r.principal_id=?2 AND r.window_key=?3
             ORDER BY r.created_at_ms DESC, r.message_id DESC LIMIT ?4",
        )?;
        let mut messages = stmt
            .query_map(
                params![kind, principal, window, limit.clamp(1, 512) as i64],
                transcript_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        messages.reverse();
        Ok(messages)
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
            message, created_at_ms, NULL, kind, priority,
            CASE WHEN sender_window_key=?3 THEN sender_session_id ELSE NULL END,
            CASE WHEN sender_window_key=?3 THEN sender_project ELSE NULL END, requires_ack,
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
        self.window_collaboration_page(kind, principal, window, limit, None)?
            .ok_or_else(|| anyhow::anyhow!("unavailable transcript cursor"))
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
        self.project_window_operator_attention(
            kind,
            principal,
            window,
            ack_ids,
            now,
            limit,
            None,
            |_| true,
        )?
        .ok_or_else(|| anyhow::anyhow!("required projection unavailable"))
    }

    pub fn project_window_operator_attention(
        &self,
        kind: &str,
        principal: &str,
        window: &str,
        ack_ids: &[String],
        now: i64,
        limit: usize,
        deadline: Option<std::time::Instant>,
        accept: impl FnOnce(&WindowOperatorAttention) -> bool,
    ) -> anyhow::Result<Option<WindowOperatorAttention>> {
        let deadline = if ack_ids.is_empty() { deadline } else { None };
        self.with_projection_connection(StoreDomain::Communication,deadline,|conn| {
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
            let mut stmt = tx.prepare(include_str!("window_operator_attention.sql"))?;
            batch.messages = stmt
                .query_map(
                    params![kind, principal, window, limit.clamp(1, 8) as i64],
                    transcript_row,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        let mut preview=batch.messages.clone();
        for message in &mut preview { message.first_projected_at_ms.get_or_insert(now); }
        if !accept(&WindowOperatorAttention { messages:preview, accepted_ack_ids:batch.accepted_ack_ids.clone(), projection_rollbacks:Vec::new() }) { batch.messages.clear(); }
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
        crate::optional_projection::check_deadline(deadline)?;
        tx.commit()?;
        Ok(batch)
        })
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
