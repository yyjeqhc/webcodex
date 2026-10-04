//! Non-destructive materialized working-set policy, under the Session store lock.
//! Cold bytes are owned in RAM, so dirty data and in-flight Job/Agent identities
//! are not discarded or delegated to an older disk image. Active is not a pin.
use super::*;
use crate::persistence::cold_session_from_record;

impl SessionStoreInner {
    /// Prepare only the exact Active mutation target. Reads use an ephemeral
    /// snapshot instead and therefore cannot promote a history scan into Hot RAM.
    /// Missing/Closed targets are left to their original semantic checks.
    pub(super) fn prepare_active_session(&mut self, session_id: &str) -> bool {
        let Some(StoredSession::Cold(cold)) = self.sessions.get(session_id) else {
            return true;
        };
        if !cold.lifecycle.allows_mutation() {
            return true;
        }
        let Some(record) = materialize_cold_session(cold, self.max_events_per_session) else {
            tracing::warn!("canonical Active Session payload unavailable; mutation rejected");
            return false;
        };
        self.sessions
            .insert(session_id.to_owned(), StoredSession::Hot(record));
        self.touch(session_id);
        self.compact_hot_sessions(session_id);
        true
    }

    /// Reuse the existing access order rather than adding a CLOCK/SLRU index.
    /// This runs only on admission, not every event. No full-record borrow can
    /// race the transition because both are protected by the same store mutex.
    pub(super) fn compact_hot_sessions(&mut self, protected: &str) {
        let target = self.hot_session_capacity_target.max(1);
        let hot = self
            .sessions
            .values()
            .filter(|entry| entry.hot().is_some())
            .count();
        if hot <= target {
            return;
        }
        let candidates: Vec<_> = self
            .lru
            .iter()
            .filter(|id| id.as_str() != protected)
            .filter(|id| {
                self.sessions
                    .get(*id)
                    .is_some_and(|entry| entry.hot().is_some())
            })
            .cloned()
            .collect();
        let mut remaining = hot - target;
        for id in candidates {
            if remaining == 0 {
                break;
            }
            let Some(record) = self.sessions.get(&id).and_then(StoredSession::hot) else {
                continue;
            };
            match cold_session_from_record(record, self.max_events_per_session) {
                Ok(cold) => {
                    self.sessions.insert(id, StoredSession::Cold(cold));
                    remaining -= 1;
                }
                Err(_) => tracing::warn!("Session compaction failed; retaining Hot data"),
            }
        }
    }
}

#[cfg(test)]
mod tests;
