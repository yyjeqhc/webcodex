//! Session message board: post / list / resolve and discussion aggregates.

use super::model::{
    ListSessionMessagesFilter, PostSessionMessageInput, SessionDiscussionSummary, SessionInboxHint,
    SessionMessage, SessionMessageError, SessionMessageStatus, DEFAULT_MAX_MESSAGES_PER_SESSION,
    DEFAULT_MESSAGE_LIST_LIMIT, MAX_MESSAGE_LIST_LIMIT, MESSAGE_ID_PREFIX,
};
use super::query::{
    build_discussion_summary, build_inbox_hint, validate_message_tags, validate_message_text,
    validate_resolution_text,
};
use super::store::SessionStore;
use super::util::now_ts;

impl SessionStore {
    pub(crate) fn post_message(
        &self,
        input: PostSessionMessageInput,
    ) -> Result<SessionMessage, SessionMessageError> {
        let message = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.touch(&input.session_id);
            let Some(record) = inner.sessions.get_mut(&input.session_id) else {
                return Err(SessionMessageError::UnknownSession);
            };
            let message = validate_message_text(input.message)?;
            let tags = validate_message_tags(input.tags)?;
            if let Some(reply_to) = input.reply_to.as_deref() {
                let found = record
                    .messages
                    .iter()
                    .any(|message| message.message_id == reply_to);
                if !found {
                    return Err(SessionMessageError::UnknownMessage);
                }
            }
            let now = now_ts();
            let message = SessionMessage {
                message_id: format!("{MESSAGE_ID_PREFIX}{}", uuid::Uuid::new_v4().simple()),
                session_id: input.session_id.clone(),
                created_at: now,
                kind: input.kind,
                status: SessionMessageStatus::Open,
                priority: input.priority,
                message,
                tags,
                reply_to: input.reply_to,
                resolved_at: None,
                resolution: None,
            };
            record.updated_at = now;
            record.messages.push_back(message.clone());
            while record.messages.len() > DEFAULT_MAX_MESSAGES_PER_SESSION {
                record.messages.pop_front();
            }
            message
        };
        self.persist_after_mutation();
        Ok(message)
    }

    pub(crate) fn list_messages(
        &self,
        session_id: &str,
        filter: ListSessionMessagesFilter,
    ) -> Result<Vec<SessionMessage>, SessionMessageError> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        let Some(record) = inner.sessions.get(session_id) else {
            return Err(SessionMessageError::UnknownSession);
        };
        let limit = filter
            .limit
            .unwrap_or(DEFAULT_MESSAGE_LIST_LIMIT)
            .clamp(0, MAX_MESSAGE_LIST_LIMIT);
        Ok(record
            .messages
            .iter()
            .filter(|message| filter.kind.is_none_or(|kind| message.kind == kind))
            .filter(|message| filter.status.is_none_or(|status| message.status == status))
            .rev()
            .take(limit)
            .cloned()
            .collect())
    }

    pub(crate) fn resolve_message(
        &self,
        session_id: &str,
        message_id: &str,
        resolution: Option<String>,
    ) -> Result<SessionMessage, SessionMessageError> {
        let message = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.touch(session_id);
            let Some(record) = inner.sessions.get_mut(session_id) else {
                return Err(SessionMessageError::UnknownSession);
            };
            let Some(message) = record
                .messages
                .iter_mut()
                .find(|message| message.message_id == message_id)
            else {
                return Err(SessionMessageError::UnknownMessage);
            };
            let resolution = match resolution {
                Some(value) => Some(validate_resolution_text(value)?),
                None => None,
            };
            if message.status == SessionMessageStatus::Open {
                message.status = SessionMessageStatus::Resolved;
                message.resolved_at = Some(now_ts());
            }
            if resolution.is_some() {
                message.resolution = resolution;
            }
            record.updated_at = now_ts();
            message.clone()
        };
        self.persist_after_mutation();
        Ok(message)
    }

    pub(crate) fn discussion_summary(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<SessionDiscussionSummary, SessionMessageError> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        let Some(record) = inner.sessions.get(session_id) else {
            return Err(SessionMessageError::UnknownSession);
        };
        let limit = limit
            .unwrap_or(DEFAULT_MESSAGE_LIST_LIMIT)
            .clamp(0, MAX_MESSAGE_LIST_LIMIT);
        Ok(build_discussion_summary(record, limit))
    }

    pub(crate) fn inbox_hint(&self, session_id: &str) -> Option<SessionInboxHint> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        let record = inner.sessions.get(session_id)?;
        build_inbox_hint(record)
    }
}
