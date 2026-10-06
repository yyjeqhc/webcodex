//! Runtime-local presentation retention, not execution or durable authority.
//!
//! ToolRuntime owns one Arc of this service; its clones share retention while
//! independent runtimes/restarts start empty. Each cache keeps its own lock,
//! budget, TTL, and identity rules. No lock spans a Runner call or an await.
//! Callers still authorize the exact Project/Session and verify source freshness.
//! Read revisions, validation fences, and continuation signing remain separate.

use super::changes::{ChangesSnapshot, ChangesSnapshotRegistry};
use super::git_review_snapshot::{GitReviewSnapshot, GitReviewSnapshotRegistry};
use super::work_result_workspace::WorkResultWorkspaceCache;
use std::sync::Mutex;

#[derive(Default)]
pub(super) struct PresentationRuntime {
    review_snapshots: Mutex<GitReviewSnapshotRegistry>,
    changes_snapshots: Mutex<ChangesSnapshotRegistry>,
    workspace_cache: WorkResultWorkspaceCache,
}

impl PresentationRuntime {
    pub(super) fn insert_review_snapshot(&self, snapshot: GitReviewSnapshot) -> GitReviewSnapshot {
        self.review_snapshots
            .lock()
            .expect("Git review snapshot registry mutex poisoned")
            .insert_or_get(snapshot)
    }

    pub(super) fn review_snapshot(
        &self,
        snapshot_id: &str,
        caller_fingerprint: &str,
        project: &str,
        session_id: Option<&str>,
    ) -> Option<GitReviewSnapshot> {
        self.review_snapshots
            .lock()
            .expect("Git review snapshot registry mutex poisoned")
            .get(snapshot_id, caller_fingerprint, project, session_id)
    }

    pub(super) fn latest_workspace_review_snapshot(
        &self,
        caller_fingerprint: &str,
        project: &str,
        session_id: Option<&str>,
    ) -> Option<GitReviewSnapshot> {
        self.review_snapshots
            .lock()
            .expect("Git review snapshot registry mutex poisoned")
            .latest_workspace(caller_fingerprint, project, session_id)
    }

    pub(super) fn insert_changes_snapshot(&self, snapshot: ChangesSnapshot) -> ChangesSnapshot {
        self.changes_snapshots
            .lock()
            .expect("Changes snapshot registry mutex poisoned")
            .insert_or_get(snapshot)
    }

    /// Lookup alone is not authority. The reader must check the returned
    /// snapshot's caller/Project/Session before producing any file content.
    pub(super) fn changes_snapshot(&self, snapshot_id: &str) -> Option<ChangesSnapshot> {
        self.changes_snapshots
            .lock()
            .expect("Changes snapshot registry mutex poisoned")
            .get(snapshot_id)
    }

    pub(super) fn changes_for_attempt(
        &self,
        caller_fingerprint: &str,
        project: &str,
        session_id: &str,
        attempt_key: &str,
    ) -> Option<ChangesSnapshot> {
        self.changes_snapshots
            .lock()
            .expect("Changes snapshot registry mutex poisoned")
            .get_for_attempt(caller_fingerprint, project, session_id, attempt_key)
    }

    pub(super) fn workspace_cache(&self) -> &WorkResultWorkspaceCache {
        &self.workspace_cache
    }
}
