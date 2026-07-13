//! Session message board: post / list / resolve and discussion aggregates.
//!
//! All message-map mutations go through `SessionStoreInner` helpers.

use super::model::{
    ListSessionMessagesFilter, PostSessionMessageInput, SessionDiscussionSummary, SessionInboxHint,
    SessionMessage, SessionMessageError,
};
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
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.list_messages(session_id, filter)
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
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.discussion_summary(session_id, limit)
    }

    pub(crate) fn inbox_hint(&self, session_id: &str) -> Option<SessionInboxHint> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.inbox_hint(session_id)
    }
}
