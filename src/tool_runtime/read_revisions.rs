//! Process-local model-facing handles for exact file-read snapshots.
//!
//! A read revision is a process-local model-facing handle for one exact file
//! snapshot. Runtime uses the same handle for guarded edits and for generated
//! read_files continuations, while authoritative filesystem evidence remains the
//! owning Runner's full-file SHA from reads and SHA guards on writes.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use uuid::Uuid;

pub(crate) const MAX_JSON_SAFE_INTEGER: u64 = (1_u64 << 53) - 1;
const READ_REVISION_COUNTER_BITS: u32 = 12;
const READ_REVISION_COUNTER_MASK: u64 = (1_u64 << READ_REVISION_COUNTER_BITS) - 1;
const READ_REVISION_EPOCH_MASK: u64 = (1_u64 << (53 - READ_REVISION_COUNTER_BITS)) - 1;
pub(crate) const READ_REVISION_HARD_BOUND: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ReadRevisionTarget {
    pub project_id: String,
    pub path: String,
    pub client_id: String,
    pub runner_instance_id: String,
    /// Runner-native project root used only as an internal retargeting fence.
    /// It is never projected to the model.
    pub project_root: String,
    pub root_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Eq)]
struct ReadRevisionSnapshot {
    target: ReadRevisionTarget,
    sha256: String,
}

impl PartialEq for ReadRevisionSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target && self.sha256 == other.sha256
    }
}

impl Hash for ReadRevisionSnapshot {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.target.project_id.hash(state);
        self.target.path.hash(state);
        self.target.client_id.hash(state);
        self.target.runner_instance_id.hash(state);
        self.target.project_root.hash(state);
        self.target.root_fingerprint.hash(state);
        self.sha256.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadRevisionLookupError {
    Unknown,
    ProjectMismatch,
    PathMismatch,
    OwnerMismatch,
}

struct ReadRevisionState {
    epoch: u64,
    next_counter: u64,
    by_revision: HashMap<u64, ReadRevisionSnapshot>,
    by_snapshot: HashMap<ReadRevisionSnapshot, u64>,
    insertion_order: VecDeque<u64>,
}

pub(crate) struct ReadRevisionRegistry {
    hard_bound: usize,
    state: Mutex<ReadRevisionState>,
}

impl std::fmt::Debug for ReadRevisionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadRevisionRegistry")
            .field("hard_bound", &self.hard_bound)
            .finish_non_exhaustive()
    }
}

impl Default for ReadRevisionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadRevisionRegistry {
    pub(crate) fn new() -> Self {
        Self::with_epoch_and_bound(random_epoch(None), READ_REVISION_HARD_BOUND)
    }

    fn with_epoch_and_bound(epoch: u64, hard_bound: usize) -> Self {
        debug_assert!((1..=READ_REVISION_EPOCH_MASK).contains(&epoch));
        debug_assert!((1..=READ_REVISION_COUNTER_MASK as usize).contains(&hard_bound));
        Self {
            hard_bound,
            state: Mutex::new(ReadRevisionState {
                epoch,
                next_counter: 1,
                by_revision: HashMap::new(),
                by_snapshot: HashMap::new(),
                insertion_order: VecDeque::new(),
            }),
        }
    }

    pub(crate) fn observe(&self, target: ReadRevisionTarget, sha256: impl Into<String>) -> u64 {
        let snapshot = ReadRevisionSnapshot {
            target,
            sha256: sha256.into(),
        };
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(revision) = state.by_snapshot.get(&snapshot).copied() {
            return revision;
        }
        if state.next_counter > READ_REVISION_COUNTER_MASK {
            rotate_epoch(&mut state);
        }
        while state.by_revision.len() >= self.hard_bound {
            let Some(evicted_revision) = state.insertion_order.pop_front() else {
                break;
            };
            if let Some(evicted_snapshot) = state.by_revision.remove(&evicted_revision) {
                state.by_snapshot.remove(&evicted_snapshot);
            }
        }
        let revision = (state.epoch << READ_REVISION_COUNTER_BITS) | state.next_counter;
        state.next_counter += 1;
        debug_assert!((1..=MAX_JSON_SAFE_INTEGER).contains(&revision));
        state.by_revision.insert(revision, snapshot.clone());
        state.by_snapshot.insert(snapshot, revision);
        state.insertion_order.push_back(revision);
        revision
    }

    pub(crate) fn resolve(
        &self,
        revision: u64,
        expected: &ReadRevisionTarget,
    ) -> Result<String, ReadRevisionLookupError> {
        if revision == 0 || revision > MAX_JSON_SAFE_INTEGER {
            return Err(ReadRevisionLookupError::Unknown);
        }
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let snapshot = state
            .by_revision
            .get(&revision)
            .ok_or(ReadRevisionLookupError::Unknown)?;
        if snapshot.target.project_id != expected.project_id {
            return Err(ReadRevisionLookupError::ProjectMismatch);
        }
        if snapshot.target.path != expected.path {
            return Err(ReadRevisionLookupError::PathMismatch);
        }
        if snapshot.target.client_id != expected.client_id
            || snapshot.target.runner_instance_id != expected.runner_instance_id
            || snapshot.target.project_root != expected.project_root
            || snapshot.target.root_fingerprint != expected.root_fingerprint
        {
            return Err(ReadRevisionLookupError::OwnerMismatch);
        }
        Ok(snapshot.sha256.clone())
    }
}

fn random_epoch(exclude: Option<u64>) -> u64 {
    loop {
        let bytes = Uuid::new_v4().as_u128();
        let epoch = ((bytes ^ (bytes >> 64)) as u64) & READ_REVISION_EPOCH_MASK;
        if epoch != 0 && Some(epoch) != exclude {
            return epoch;
        }
    }
}

fn rotate_epoch(state: &mut ReadRevisionState) {
    state.epoch = random_epoch(Some(state.epoch));
    state.next_counter = 1;
    state.by_revision.clear();
    state.by_snapshot.clear();
    state.insertion_order.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(project: &str, path: &str) -> ReadRevisionTarget {
        ReadRevisionTarget {
            project_id: project.to_string(),
            path: path.to_string(),
            client_id: "runner-a".to_string(),
            runner_instance_id: "instance-a".to_string(),
            project_root: "/runner/project".to_string(),
            root_fingerprint: Some("root-a".to_string()),
        }
    }

    #[test]
    fn same_snapshot_reuses_revision_and_changed_snapshot_advances() {
        let registry = ReadRevisionRegistry::with_epoch_and_bound(17, 8);
        let a = registry.observe(target("agent:r:p", "src/a.rs"), "sha-a");
        let same = registry.observe(target("agent:r:p", "src/a.rs"), "sha-a");
        let changed = registry.observe(target("agent:r:p", "src/a.rs"), "sha-b");
        assert_eq!(a, same);
        assert_ne!(a, changed);
        assert_eq!(changed, a + 1);
    }

    #[test]
    fn revisions_are_strictly_project_path_and_owner_bound() {
        let registry = ReadRevisionRegistry::with_epoch_and_bound(23, 8);
        let original = target("agent:r:p", "src/a.rs");
        let revision = registry.observe(original.clone(), "sha-a");

        let mut wrong_project = original.clone();
        wrong_project.project_id = "agent:r:other".into();
        assert_eq!(
            registry.resolve(revision, &wrong_project),
            Err(ReadRevisionLookupError::ProjectMismatch)
        );
        let mut wrong_path = original.clone();
        wrong_path.path = "src/b.rs".into();
        assert_eq!(
            registry.resolve(revision, &wrong_path),
            Err(ReadRevisionLookupError::PathMismatch)
        );
        let mut wrong_owner = original.clone();
        wrong_owner.runner_instance_id = "instance-b".into();
        assert_eq!(
            registry.resolve(revision, &wrong_owner),
            Err(ReadRevisionLookupError::OwnerMismatch)
        );
        assert_eq!(registry.resolve(revision, &original).unwrap(), "sha-a");
    }

    #[test]
    fn same_content_on_different_paths_never_retargets() {
        let registry = ReadRevisionRegistry::with_epoch_and_bound(31, 8);
        let a = registry.observe(target("agent:r:p", "a.rs"), "same-sha");
        let b = registry.observe(target("agent:r:p", "b.rs"), "same-sha");
        assert_ne!(a, b);
        assert_eq!(
            registry.resolve(a, &target("agent:r:p", "b.rs")),
            Err(ReadRevisionLookupError::PathMismatch)
        );
    }

    #[test]
    fn new_runtime_epoch_does_not_accept_old_revision() {
        let old = ReadRevisionRegistry::with_epoch_and_bound(41, 8);
        let revision = old.observe(target("agent:r:p", "a.rs"), "sha-a");
        let restarted = ReadRevisionRegistry::with_epoch_and_bound(42, 8);
        let replacement = restarted.observe(target("agent:r:p", "a.rs"), "sha-a");
        assert_ne!(revision, replacement);
        assert_eq!(
            restarted.resolve(revision, &target("agent:r:p", "a.rs")),
            Err(ReadRevisionLookupError::Unknown)
        );
    }

    #[test]
    fn eviction_makes_old_revision_unknown() {
        let registry = ReadRevisionRegistry::with_epoch_and_bound(51, 2);
        let first = registry.observe(target("agent:r:p", "a.rs"), "sha-a");
        registry.observe(target("agent:r:p", "b.rs"), "sha-b");
        registry.observe(target("agent:r:p", "c.rs"), "sha-c");
        assert_eq!(
            registry.resolve(first, &target("agent:r:p", "a.rs")),
            Err(ReadRevisionLookupError::Unknown)
        );
    }

    #[test]
    fn every_revision_is_positive_and_json_safe() {
        let registry = ReadRevisionRegistry::with_epoch_and_bound(READ_REVISION_EPOCH_MASK, 8);
        for index in 0..8 {
            let revision = registry.observe(
                target("agent:r:p", &format!("src/{index}.rs")),
                format!("sha-{index}"),
            );
            assert!((1..=MAX_JSON_SAFE_INTEGER).contains(&revision));
        }
    }
}
