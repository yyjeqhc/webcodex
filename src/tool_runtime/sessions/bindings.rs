//! Current-session bindings: bind / unbind / lookup.
//!
//! Bindings are process-local control metadata, not durable ledger state.

use super::model::{CurrentSessionKey, SessionSummary, DEFAULT_SUMMARY_LIMIT};
use super::store::SessionStore;

impl SessionStore {
    pub(crate) fn bind_current_session(
        &self,
        key: CurrentSessionKey,
        session_id: &str,
    ) -> Option<SessionSummary> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        let summary = inner.summary(session_id, Some(DEFAULT_SUMMARY_LIMIT))?;
        inner
            .current_sessions
            .insert(key, session_id.trim().to_string());
        Some(summary)
    }

    pub(crate) fn current_session(&self, key: &CurrentSessionKey) -> Option<SessionSummary> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        let session_id = inner.current_sessions.get(key).cloned()?;
        inner.touch(&session_id);
        match inner.summary(&session_id, Some(DEFAULT_SUMMARY_LIMIT)) {
            Some(summary) => Some(summary),
            None => {
                inner.current_sessions.remove(key);
                None
            }
        }
    }

    pub(crate) fn current_session_id(&self, key: &CurrentSessionKey) -> Option<String> {
        self.current_session(key).map(|summary| summary.session_id)
    }

    pub(crate) fn unbind_current_session(&self, key: &CurrentSessionKey) -> bool {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.current_sessions.remove(key).is_some()
    }
}
