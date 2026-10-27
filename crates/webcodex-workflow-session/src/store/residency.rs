//! Non-destructive materialized working-set policy, under the Session store lock.
//! SQLite Cold entries hold metadata and an exact committed revision. Only clean
//! state can leave RAM; failed writes and later dirty generations stay resident.
use super::*;
use crate::persistence::cold_session_from_record;

impl SessionStore {
    /// Update retained evidence without duplicating Hot/Cold ownership handling.
    /// The exact target hydrates before a mutation, including late evidence for
    /// Closed history. Persistence remains the caller's duty.
    pub(super) fn update_retained_record<T>(
        &self,
        session_id: &str,
        value: T,
        update: impl FnOnce(&mut SessionRecord, T) -> bool,
    ) -> bool {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        if !inner.prepare_record_mutation(session_id) {
            return false;
        }
        let Some(record) = inner
            .sessions
            .get_mut(session_id)
            .and_then(StoredSession::hot_mut)
        else {
            return false;
        };
        let updated = update(record, value);
        let closed = !record.lifecycle.allows_mutation();
        drop(inner);
        if updated && closed {
            self.coldify_closed_session(session_id);
        }
        updated
    }
}

impl SessionStoreInner {
    /// Prepare only the exact mutation target. Reads use an ephemeral
    /// snapshot instead and therefore cannot promote a history scan into Hot RAM.
    /// Missing/Closed targets are left to their original semantic checks.
    pub(super) fn prepare_record_mutation(&mut self, session_id: &str) -> bool {
        let Some(StoredSession::Cold(cold)) = self.sessions.get(session_id) else {
            return true;
        };
        let Some(record) = materialize_cold_session(cold, self.recent_event_limit) else {
            tracing::warn!("canonical Session payload unavailable; mutation rejected");
            return false;
        };
        self.sessions
            .replace_residency(session_id.to_owned(), StoredSession::Hot(record));
        self.touch(session_id);
        self.compact_hot_sessions(session_id);
        true
    }

    /// Reuse the existing access order rather than adding a CLOCK/SLRU index.
    /// This runs only on admission, not every event. No full-record borrow can
    /// race the transition because both are protected by the same store mutex.
    pub(super) fn compact_hot_sessions(&mut self, protected: &str) {
        if self.persistence.is_some() {
            // Only successfully committed, unchanged records can leave RAM.
            // Active lifecycle never pins history. Dirty data survives I/O errors.
            let cache_bytes = self.cache_target_bytes;
            let mut bytes: usize = self
                .sessions
                .values()
                .filter_map(StoredSession::hot)
                .map(resident_bytes)
                .sum();
            let candidates = self
                .lru
                .iter()
                .filter(|id| id.as_str() != protected)
                .cloned()
                .collect::<Vec<_>>();
            for id in candidates {
                if self.sessions.dirty.contains_key(&id) {
                    continue;
                }
                let Some(record) = self.sessions.get(&id).and_then(StoredSession::hot) else {
                    continue;
                };
                if bytes <= cache_bytes && record.lifecycle.allows_mutation() {
                    continue;
                }
                let Some(cold) = record.durable.clone() else {
                    continue;
                };
                bytes = bytes.saturating_sub(resident_bytes(record));
                self.sessions
                    .replace_residency(id, StoredSession::Cold(cold));
            }
            return;
        }
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
            match cold_session_from_record(record, self.recent_event_limit) {
                Ok(cold) => {
                    self.sessions
                        .replace_residency(id, StoredSession::Cold(cold));
                    remaining -= 1;
                }
                Err(_) => tracing::warn!("Session compaction failed; retaining Hot data"),
            }
        }
    }
}

#[cfg(test)]
mod tests;

// A conservative working-set weight, not a process RSS measurement. Include
// event/message bodies rather than counting every Session as an equal object.
fn resident_bytes(record: &SessionRecord) -> usize {
    let events = record
        .events
        .iter()
        .map(|event| {
            std::mem::size_of::<SessionEvent>()
                + event.tool_name.len()
                + event.session_id.len()
                + event.instruction.as_ref().map_or(0, String::len)
                + event.changed_paths.iter().map(String::len).sum::<usize>()
                + event.observed_paths.iter().map(String::len).sum::<usize>()
                + event
                    .input_summary
                    .as_ref()
                    .map_or(0, |value| value.to_string().len())
                + event
                    .validation_output_summary
                    .as_ref()
                    .map_or(0, |value| value.to_string().len())
        })
        .sum::<usize>();
    let messages = record
        .messages
        .iter()
        .map(|message| std::mem::size_of::<SessionMessage>() + message.message.len())
        .sum::<usize>();
    (std::mem::size_of::<SessionRecord>() + events + messages).saturating_mul(2)
}
