//! Mutation generations are registered under the same mutex as domain state.
//! A durability read can therefore fence an observed version without inventing
//! a mutation or racing a later background notification.
use super::*;
use std::ops::Deref;

#[derive(Debug, Default)]
pub(super) struct SessionRecords {
    pub(super) records: HashMap<String, StoredSession>,
    pub generation: u64,
    pub dirty: BTreeMap<String, u64>,
}

impl Deref for SessionRecords {
    type Target = HashMap<String, StoredSession>;
    fn deref(&self) -> &Self::Target {
        &self.records
    }
}

impl SessionRecords {
    pub fn restored(records: HashMap<String, StoredSession>) -> Self {
        Self {
            records,
            ..Self::default()
        }
    }

    fn mark(&mut self, id: &str) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("Session mutation generation exhausted");
        self.dirty.insert(id.to_owned(), self.generation);
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut StoredSession> {
        if !self.records.contains_key(id) {
            return None;
        }
        self.mark(id);
        self.records.get_mut(id)
    }

    pub fn insert(&mut self, id: String, mut value: StoredSession) -> Option<StoredSession> {
        if let StoredSession::Hot(record) = &mut value {
            if record.durable.is_none() {
                record.pending_events = record.events.clone();
            }
        }
        self.mark(&id);
        self.records.insert(id, value)
    }

    /// Residency is not a domain mutation and must never schedule persistence.
    pub fn replace_residency(&mut self, id: String, value: StoredSession) {
        self.records.insert(id, value);
    }

    pub fn acknowledge(
        &mut self,
        id: &str,
        revision: u64,
        pending: &[Arc<SessionEvent>],
        cold: ColdSessionRecord,
    ) {
        if let Some(StoredSession::Hot(record)) = self.records.get_mut(id) {
            for event in pending {
                if record
                    .pending_events
                    .front()
                    .is_some_and(|current| Arc::ptr_eq(current, event))
                {
                    record.pending_events.pop_front();
                } else {
                    break;
                }
            }
            record.durable = Some(cold);
        }
        if self.dirty.get(id) == Some(&revision) {
            self.dirty.remove(id);
        }
    }
}
