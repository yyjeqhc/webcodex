//! Session message board: post / list / resolve and discussion aggregates.
//!
//! All message-map mutations go through `SessionStoreInner` helpers.

use super::model::{
    ListSessionMessagesFilter, PostSessionMessageInput, SessionDiscussionSummary, SessionInboxHint,
    SessionMessage, SessionMessageError, DEFAULT_MESSAGE_LIST_LIMIT, MAX_MESSAGE_LIST_LIMIT,
};
use super::query::{build_discussion_summary, build_inbox_hint};
use super::store::SessionStore;

impl SessionStore {
    pub(crate) fn post_message(
        &self,
        input: PostSessionMessageInput,
    ) -> Result<SessionMessage, SessionMessageError> {
        let message = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.post_message(input)?
        };
        self.persist_after_mutation();
        Ok(message)
    }

    pub(crate) fn list_messages(
        &self,
        session_id: &str,
        filter: ListSessionMessagesFilter,
    ) -> Result<Vec<SessionMessage>, SessionMessageError> {
        self.with_record_for_query(session_id, |record, _| {
            let limit = filter
                .limit
                .unwrap_or(DEFAULT_MESSAGE_LIST_LIMIT)
                .clamp(0, MAX_MESSAGE_LIST_LIMIT);
            record
                .messages
                .iter()
                .filter(|message| filter.kind.is_none_or(|kind| message.kind == kind))
                .filter(|message| filter.status.is_none_or(|status| message.status == status))
                .rev()
                .take(limit)
                .map(|message| message.as_ref().clone())
                .collect()
        })
        .ok_or(SessionMessageError::UnknownSession)
    }

    pub(crate) fn resolve_message(
        &self,
        session_id: &str,
        message_id: &str,
        resolution: Option<String>,
    ) -> Result<SessionMessage, SessionMessageError> {
        let message = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.resolve_message(session_id, message_id, resolution)?
        };
        self.persist_after_mutation();
        Ok(message)
    }

    pub(crate) fn discussion_summary(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<SessionDiscussionSummary, SessionMessageError> {
        self.with_record_for_query(session_id, |record, _| {
            let limit = limit
                .unwrap_or(DEFAULT_MESSAGE_LIST_LIMIT)
                .clamp(0, MAX_MESSAGE_LIST_LIMIT);
            build_discussion_summary(record, limit)
        })
        .ok_or(SessionMessageError::UnknownSession)
    }

    pub(crate) fn inbox_hint(&self, session_id: &str) -> Option<SessionInboxHint> {
        self.with_record_for_query(session_id, |record, _| build_inbox_hint(record))
            .flatten()
    }
}
