//! Durable transcript reads, independent of the bounded model attention queue.
//! Each source is an indexed keyset page; merge at most six bounded source pages,
//! never deserialize a complete conversation or use a shifting OFFSET cursor.
use super::window_collaboration::{transcript_row, WindowCollaborationMessage};
use crate::{Database, StoreDomain};
use rusqlite::{params, OptionalExtension};

fn sources() -> Vec<(String, &'static str)> {
    let mut sources = vec![
        (
            "SELECT message_id, 'operator' AS source, 'inbound' AS direction, NULL AS peer_id,
            message, created_at_ms, NULL AS reply_to_message_id, kind, priority,
            context_session_id, context_project, requires_ack,
            first_projected_at_ms, first_ack_observed_at_ms
         FROM window_operator_messages
         WHERE principal_kind=?1 AND principal_id=?2 AND recipient_window_key=?3"
                .into(),
            "created_at_ms, message_id",
        ),
        (
            "SELECT r.message_id, 'window', 'outbound', NULL, r.message, r.created_at_ms,
            r.reply_to_message_id, 'answer', 'normal', o.context_session_id, o.context_project,
            0, NULL, NULL
         FROM window_model_replies r LEFT JOIN window_operator_messages o
            ON o.message_id=r.reply_to_message_id AND o.principal_kind=r.principal_kind
            AND o.principal_id=r.principal_id AND o.recipient_window_key=r.window_key
         WHERE r.principal_kind=?1 AND r.principal_id=?2 AND r.window_key=?3"
                .into(),
            "r.created_at_ms, r.message_id",
        ),
    ];
    // Fixed internal table names, never interpolated caller input. Separate
    // inbound/outbound queries preserve indexed order even for long histories.
    for table in ["window_peer_messages", "window_peer_message_history"] {
        for (direction, key, peer, context, extra) in [
            (
                "inbound",
                "recipient_window_key",
                "sender_peer_id",
                "NULL, NULL",
                "",
            ),
            (
                "outbound",
                "sender_window_key",
                "recipient_peer_id",
                "sender_session_id, sender_project",
                " AND recipient_window_key<>?3",
            ),
        ] {
            sources.push((
                format!(
                    "SELECT message_id, 'peer', '{direction}', {peer}, message, created_at_ms,
                    NULL, kind, priority, {context}, requires_ack,
                    first_projected_at_ms, first_ack_observed_at_ms
                 FROM {table} WHERE principal_kind=?1 AND principal_id=?2 AND {key}=?3{extra}"
                ),
                "created_at_ms, message_id",
            ));
        }
    }
    sources
}

impl Database {
    /// The cursor is an exact message identity in this principal and Window,
    /// not authority. None result means missing/foreign cursor, not empty history.
    pub fn window_collaboration_page(
        &self,
        kind: &str,
        principal: &str,
        window: &str,
        limit: usize,
        before: Option<&str>,
    ) -> anyhow::Result<Option<(Vec<WindowCollaborationMessage>, bool)>> {
        let conn = self.lock_connection(StoreDomain::Communication);
        // One read snapshot also protects against another process moving peer rows
        // between the delivery table and archive while these six pages merge.
        let snapshot = conn.unchecked_transaction()?;
        let conn = &snapshot;
        let sources = sources();
        let (at, id) = if let Some(before) = before {
            let union = sources
                .iter()
                .map(|(sql, _)| sql.as_str())
                .collect::<Vec<_>>()
                .join(" UNION ALL ");
            let cursor: Option<(i64, String)> = conn.query_row(
                &format!("SELECT created_at_ms, message_id FROM ({union}) WHERE message_id=?4 LIMIT 1"),
                params![kind, principal, window, before], |row| Ok((row.get(0)?, row.get(1)?)),
            ).optional()?;
            let Some(cursor) = cursor else {
                return Ok(None);
            };
            cursor
        } else {
            (i64::MAX, "~".into())
        };
        let limit = limit.clamp(1, 100);
        let mut messages = Vec::with_capacity((limit + 1) * sources.len());
        for (sql, order) in sources {
            let query = format!(
                "{sql} AND ({order}) < (?4, ?5) ORDER BY {} DESC, {} DESC LIMIT ?6",
                order.split(", ").next().unwrap(),
                order.split(", ").nth(1).unwrap()
            );
            let mut statement = conn.prepare(&query)?;
            messages.extend(
                statement
                    .query_map(
                        params![kind, principal, window, at, id, (limit + 1) as i64],
                        transcript_row,
                    )?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            );
        }
        messages.sort_by(|a, b| {
            (b.created_at_ms, &b.message_id).cmp(&(a.created_at_ms, &a.message_id))
        });
        // Bound encoded results as well as row count. Never truncate a body:
        // stop at a whole-message boundary and continue using its exact cursor.
        const PAGE_BYTES: usize = 128 * 1024;
        let mut bytes = 2usize;
        let mut take = 0;
        for message in messages.iter().take(limit) {
            let size = serde_json::to_vec(message)?.len() + 1;
            anyhow::ensure!(
                size + 2 <= PAGE_BYTES,
                "stored message exceeds history page budget"
            );
            if bytes + size > PAGE_BYTES {
                break;
            }
            bytes += size;
            take += 1;
        }
        let truncated = messages.len() > take;
        messages.truncate(take);
        messages.reverse();
        Ok(Some((messages, truncated)))
    }
}
