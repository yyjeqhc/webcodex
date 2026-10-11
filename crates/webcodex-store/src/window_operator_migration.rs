//! Startup-only repair of retained Operator history. Read bounded pages rather
//! than whole mailboxes; each message and all of its replies move atomically.
use crate::window_collaboration::{
    oauth_window_operator_principal, NewWindowModelReply, NewWindowOperatorMessage,
};
use crate::{Database, StoreDomain};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

const PAGE_ROWS: i64 = 64;

impl Database {
    /// Automatically repair the retained startup snapshot before expired grant
    /// cleanup. Keyset progress also passes unattributable/conflicting rows, so
    /// they cannot starve later messages. No message/reply bodies are logged.
    /// Work scales with retained history, while allocations are page-bounded.
    pub fn migrate_legacy_managed_oauth_operator_messages(&self) -> anyhow::Result<usize> {
        let last_rowid: i64 = self.lock_connection(StoreDomain::Communication).query_row(
            "SELECT COALESCE(MAX(rowid),0) FROM window_operator_messages",
            [],
            |row| row.get(0),
        )?;
        let mut after = 0;
        let mut migrated = 0;
        loop {
            let candidates: Vec<(i64, String)> = {
                let conn = self.lock_connection(StoreDomain::Communication);
                let mut stmt = conn.prepare(
                    "SELECT rowid,message_id FROM window_operator_messages
                     WHERE principal_kind='oauth2' AND rowid>?1 AND rowid<=?2 ORDER BY rowid LIMIT ?3",
                )?;
                let rows = stmt.query_map(params![after, last_rowid, PAGE_ROWS], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            if candidates.is_empty() {
                break;
            }
            for (rowid, message_id) in candidates {
                after = rowid;
                migrated += usize::from(self.migrate_legacy_operator_message(&message_id)?);
            }
        }
        Ok(migrated)
    }

    fn migrate_legacy_operator_message(&self, message_id: &str) -> anyhow::Result<bool> {
        let mut conn = self.lock_connection(StoreDomain::Communication);
        // Attribution, collision checks and mutations share one writer snapshot.
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let original = tx
            .query_row(
                "SELECT principal_id,recipient_window_key,context_session_id,context_project,
                    kind,priority,message,tags_json,requires_ack,created_at_ms,delivery_key_hash
             FROM window_operator_messages WHERE message_id=?1 AND principal_kind='oauth2'",
                [message_id],
                |row| {
                    Ok((
                        NewWindowOperatorMessage {
                            principal_kind: "oauth2".into(),
                            principal_id: row.get(0)?,
                            recipient_window_key: row.get(1)?,
                            context_session_id: row.get(2)?,
                            context_project: row.get(3)?,
                            kind: row.get(4)?,
                            priority: row.get(5)?,
                            message: row.get(6)?,
                            tags: vec![],
                            requires_ack: row.get(8)?,
                            created_at_ms: row.get(9)?,
                        },
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(10)?,
                    ))
                },
            )
            .optional()?;
        let Some((mut message, tags, delivery_key)) = original else {
            return Ok(false);
        };
        let Some((kind, principal)) = oauth_window_operator_principal(
            &tx,
            &message.recipient_window_key,
            &message.principal_id,
        )?
        else {
            return Ok(false);
        };
        if kind != "oauth2-window-operator" {
            return Ok(false);
        }
        let Ok(tags) = serde_json::from_str(&tags) else {
            return Ok(false);
        };
        let old_principal = std::mem::replace(&mut message.principal_id, principal);
        message.principal_kind = kind;
        message.tags = tags;
        let collision: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM window_operator_messages
             WHERE principal_kind=?1 AND principal_id=?2 AND delivery_key_hash=?3)",
            params![message.principal_kind, message.principal_id, delivery_key],
            |row| row.get(0),
        )?;
        if collision {
            return Ok(false);
        }

        let mut after = String::new();
        loop {
            let replies: Vec<(String, String, i64, String)> = {
                let mut stmt = tx.prepare(
                    "SELECT message_id,message,created_at_ms,delivery_key_hash
                     FROM window_model_replies WHERE principal_kind='oauth2' AND principal_id=?1
                       AND window_key=?2 AND reply_to_message_id=?3 AND message_id>?4
                     ORDER BY message_id LIMIT ?5",
                )?;
                let rows = stmt.query_map(
                    params![
                        old_principal,
                        message.recipient_window_key,
                        message_id,
                        after,
                        PAGE_ROWS
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            if replies.is_empty() {
                break;
            }
            for (reply_id, body, created_at_ms, key) in replies {
                let collision: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM window_model_replies
                     WHERE principal_kind=?1 AND principal_id=?2 AND delivery_key_hash=?3)",
                    params![message.principal_kind, message.principal_id, key],
                    |row| row.get(0),
                )?;
                // Dropping the transaction rolls back earlier reply pages too.
                if collision {
                    return Ok(false);
                }
                let reply = NewWindowModelReply {
                    principal_kind: message.principal_kind.clone(),
                    principal_id: message.principal_id.clone(),
                    window_key: message.recipient_window_key.clone(),
                    reply_to_message_id: message_id.into(),
                    message: body,
                    created_at_ms,
                };
                let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&reply)?));
                tx.execute("UPDATE window_model_replies SET principal_kind=?1,principal_id=?2,delivery_payload_hash=?3 WHERE message_id=?4",
                    params![reply.principal_kind, reply.principal_id, hash, reply_id])?;
                after = reply_id;
            }
        }
        let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&message)?));
        tx.execute("UPDATE window_operator_messages SET principal_kind=?1,principal_id=?2,delivery_payload_hash=?3 WHERE message_id=?4",
            params![message.principal_kind, message.principal_id, hash, message_id])?;
        tx.commit()?;
        Ok(true)
    }
}
