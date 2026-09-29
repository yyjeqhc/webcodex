//! Non-authoritative access order, separate from Session identity/lifecycle.
//!
//! Exact touches and removal cost O(log n); closed-history eviction never scans
//! Active records. The owning store supplies lifecycle under its existing lock.
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Default)]
pub(super) struct SessionRecency {
    positions: HashMap<String, u64>,
    ordered: BTreeMap<u64, String>,
    closed: BTreeSet<u64>,
    next: u64,
}

impl SessionRecency {
    pub(super) fn touch(&mut self, id: &str, is_closed: bool) {
        self.remove(id);
        if self.next == u64::MAX {
            // Extremely rare local counter rollover. Preserve access order and
            // eviction eligibility; this is not a durable revision or identity.
            let ordered = std::mem::take(&mut self.ordered);
            let closed = std::mem::take(&mut self.closed);
            self.positions.clear();
            self.next = 0;
            for (position, id) in ordered {
                self.touch(&id, closed.contains(&position));
            }
        }
        let position = self.next;
        self.next += 1;
        self.positions.insert(id.to_owned(), position);
        self.ordered.insert(position, id.to_owned());
        if is_closed {
            self.closed.insert(position);
        }
    }

    pub(super) fn remove(&mut self, id: &str) {
        if let Some(position) = self.positions.remove(id) {
            self.ordered.remove(&position);
            self.closed.remove(&position);
        }
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &String> {
        self.ordered.values()
    }

    pub(super) fn closed_count(&self) -> usize {
        self.closed.len()
    }

    pub(super) fn oldest_closed(&self) -> Option<&str> {
        self.closed
            .first()
            .and_then(|position| self.ordered.get(position))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests;
