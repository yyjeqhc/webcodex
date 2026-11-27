//! Process-local active-Job index owned with the canonical Job records.
//!
//! Only the mutation methods below expose mutable records. They mark exact dirty
//! IDs; the registry unlock boundary reconciles those IDs before publishing its
//! next snapshot. There is deliberately no DerefMut to the underlying map, so a
//! new insertion/removal/mutation path cannot silently bypass index maintenance.
//! This index is not authority: readers recheck lifecycle, visibility and access.

use crate::state::ShellJobRecord;
use std::collections::{HashMap, HashSet};
use std::ops::Deref;

#[derive(Debug, Default)]
pub(crate) struct JobRecords {
    records: HashMap<String, ShellJobRecord>,
    active: HashSet<String>,
    dirty: HashSet<String>,
}

impl Deref for JobRecords {
    type Target = HashMap<String, ShellJobRecord>;
    fn deref(&self) -> &Self::Target {
        &self.records
    }
}

impl JobRecords {
    pub(crate) fn insert(&mut self, id: String, job: ShellJobRecord) -> Option<ShellJobRecord> {
        debug_assert_eq!(id, job.job_id);
        self.dirty.insert(id.clone());
        self.records.insert(id, job)
    }

    pub(crate) fn insert_if_absent(&mut self, job: ShellJobRecord) {
        if !self.records.contains_key(&job.job_id) {
            self.insert(job.job_id.clone(), job);
        }
    }

    pub(crate) fn get_mut(&mut self, id: &str) -> Option<&mut ShellJobRecord> {
        let job = self.records.get_mut(id)?;
        self.dirty.insert(id.to_string());
        Some(job)
    }

    pub(crate) fn remove(&mut self, id: &str) -> Option<ShellJobRecord> {
        self.active.remove(id);
        self.dirty.remove(id);
        self.records.remove(id)
    }

    /// O(changed Jobs), run under the same mutex as the canonical records. A
    /// same-lock aggregate also flushes pending changes before selecting IDs.
    pub(crate) fn reconcile_active(&mut self) {
        for id in self.dirty.drain() {
            if self
                .records
                .get(&id)
                .is_some_and(|job| job.lifecycle.is_active())
            {
                self.active.insert(id);
            } else {
                self.active.remove(&id);
            }
        }
    }

    pub(crate) fn active_ids(&mut self) -> Vec<String> {
        self.reconcile_active();
        self.active.iter().cloned().collect()
    }
}
