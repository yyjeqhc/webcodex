//! Current-session bindings: bind / unbind / lookup.
//!
//! Bindings are process-local control metadata, not durable ledger state.
//! All mutations go through `SessionStoreInner` helpers.

use super::model::{CurrentSessionKey, SessionSummary};
use super::store::SessionStore;

impl SessionStore {
    pub(crate) fn bind_current_session(
        &self,
        key: CurrentSessionKey,
        session_id: &str,
    ) -> Option<SessionSummary> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.bind_current(key, session_id)
    }

    pub(crate) fn current_session(&self, key: &CurrentSessionKey) -> Option<SessionSummary> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.current_session(key)
    }

    pub(crate) fn current_session_id(&self, key: &CurrentSessionKey) -> Option<String> {
        self.current_session(key).map(|summary| summary.session_id)
    }

    pub(crate) fn unbind_current_session(&self, key: &CurrentSessionKey) -> bool {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.unbind_current(key)
    }
}
