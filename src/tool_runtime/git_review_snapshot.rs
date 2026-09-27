use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::auth::AuthContext;

#[cfg(test)]
use super::git_committed::CommittedGitScope;
use super::session_context::workflow_session_authority_fingerprint;
use super::{ToolResult, ToolRuntime};

const REVIEW_SNAPSHOT_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_REVIEW_SNAPSHOTS: usize = 32;
const MAX_REVIEW_SNAPSHOTS_PER_CALLER: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GitReviewScope {
    Workspace,
    Committed {
        requested_base: String,
        requested_head: String,
        merge_base: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GitReviewSourceIdentity {
    Workspace {
        head_commit: Option<String>,
        frozen_tree: String,
        status_fingerprint: String,
    },
    Committed {
        requested_base: String,
        requested_head: String,
        merge_base: String,
    },
}

impl GitReviewSourceIdentity {
    pub(crate) fn presentation_value(&self) -> Value {
        match self {
            Self::Workspace {
                head_commit,
                frozen_tree,
                status_fingerprint,
            } => json!({
                "kind": "workspace",
                "head_commit": head_commit,
                "frozen_tree": frozen_tree,
                "status_fingerprint": status_fingerprint,
            }),
            Self::Committed {
                requested_base,
                requested_head,
                merge_base,
            } => json!({
                "kind": "committed",
                "requested_base": requested_base,
                "requested_head": requested_head,
                "merge_base": merge_base,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GitReviewSnapshot {
    pub(crate) snapshot_id: String,
    pub(crate) caller_fingerprint: String,
    pub(crate) project: String,
    pub(crate) session_id: Option<String>,
    pub(crate) scope: GitReviewScope,
    pub(crate) source: GitReviewSourceIdentity,
    pub(crate) projection_identity: Value,
    pub(crate) summary: Value,
    pub(crate) signals: Value,
    pub(crate) diff_page: Value,
    pub(crate) coverage_partial: bool,
    pub(crate) metadata_complete: bool,
    expires_at: Instant,
}

impl GitReviewSnapshot {
    pub(crate) fn new(
        caller_fingerprint: String,
        project: String,
        session_id: Option<String>,
        scope: GitReviewScope,
        source: GitReviewSourceIdentity,
        projection_identity: Value,
        summary: Value,
        signals: Value,
        diff_page: Value,
        coverage_partial: bool,
        metadata_complete: bool,
    ) -> Self {
        let snapshot_id = review_snapshot_id(
            &caller_fingerprint,
            &project,
            session_id.as_deref(),
            &source,
            &projection_identity,
        );
        Self {
            snapshot_id,
            caller_fingerprint,
            project,
            session_id,
            scope,
            source,
            projection_identity,
            summary,
            signals,
            diff_page,
            coverage_partial,
            metadata_complete,
            expires_at: Instant::now() + REVIEW_SNAPSHOT_TTL,
        }
    }

    pub(crate) fn matches_identity(
        &self,
        caller_fingerprint: &str,
        project: &str,
        session_id: Option<&str>,
    ) -> bool {
        self.caller_fingerprint == caller_fingerprint
            && self.project == project
            && self.session_id.as_deref() == session_id
    }
}

#[derive(Default)]
struct GitReviewSnapshotRegistry {
    snapshots: VecDeque<GitReviewSnapshot>,
}

impl GitReviewSnapshotRegistry {
    fn prune(&mut self) {
        let now = Instant::now();
        self.snapshots.retain(|snapshot| snapshot.expires_at > now);
    }

    fn insert_or_get(&mut self, snapshot: GitReviewSnapshot) -> GitReviewSnapshot {
        self.prune();
        if let Some(index) = self.snapshots.iter().position(|existing| {
            existing.snapshot_id == snapshot.snapshot_id
                && existing.matches_identity(
                    &snapshot.caller_fingerprint,
                    &snapshot.project,
                    snapshot.session_id.as_deref(),
                )
        }) {
            self.snapshots[index] = snapshot.clone();
            return snapshot;
        }
        while self
            .snapshots
            .iter()
            .filter(|existing| existing.caller_fingerprint == snapshot.caller_fingerprint)
            .count()
            >= MAX_REVIEW_SNAPSHOTS_PER_CALLER
        {
            if let Some(index) = self
                .snapshots
                .iter()
                .position(|existing| existing.caller_fingerprint == snapshot.caller_fingerprint)
            {
                self.snapshots.remove(index);
            }
        }
        while self.snapshots.len() >= MAX_REVIEW_SNAPSHOTS {
            self.snapshots.pop_front();
        }
        self.snapshots.push_back(snapshot.clone());
        snapshot
    }

    fn get(
        &mut self,
        snapshot_id: &str,
        caller_fingerprint: &str,
        project: &str,
        session_id: Option<&str>,
    ) -> Option<GitReviewSnapshot> {
        self.prune();
        self.snapshots
            .iter()
            .find(|snapshot| {
                snapshot.snapshot_id == snapshot_id
                    && snapshot.matches_identity(caller_fingerprint, project, session_id)
            })
            .cloned()
    }

    fn latest_workspace(
        &mut self,
        caller_fingerprint: &str,
        project: &str,
        session_id: Option<&str>,
    ) -> Option<GitReviewSnapshot> {
        self.prune();
        self.snapshots
            .iter()
            .rev()
            .find(|snapshot| {
                matches!(snapshot.scope, GitReviewScope::Workspace)
                    && snapshot.matches_identity(caller_fingerprint, project, session_id)
            })
            .cloned()
    }
}

fn review_snapshots() -> &'static Mutex<GitReviewSnapshotRegistry> {
    static REGISTRY: OnceLock<Mutex<GitReviewSnapshotRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(GitReviewSnapshotRegistry::default()))
}

fn review_snapshot_id(
    caller_fingerprint: &str,
    project: &str,
    session_id: Option<&str>,
    source: &GitReviewSourceIdentity,
    projection_identity: &Value,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.git-review-snapshot.v1\0");
    for value in [caller_fingerprint, project, session_id.unwrap_or("")] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.update(
        serde_json::to_vec(&source.presentation_value())
            .expect("review source identity serializes"),
    );
    hasher.update(
        serde_json::to_vec(projection_identity).expect("review projection identity serializes"),
    );
    format!("wc_grs_{:x}", hasher.finalize())
}

#[cfg(test)]
pub(crate) fn committed_source_identity(scope: &CommittedGitScope) -> GitReviewSourceIdentity {
    GitReviewSourceIdentity::Committed {
        requested_base: scope.requested_base.clone(),
        requested_head: scope.requested_head.clone(),
        merge_base: scope.merge_base.clone(),
    }
}

pub(crate) fn caller_fingerprint(auth: Option<&AuthContext>) -> Result<String, ToolResult> {
    workflow_session_authority_fingerprint(auth)
        .map_err(|_| ToolResult::err("git review snapshot authority unavailable"))
}

pub(crate) fn insert_snapshot(snapshot: GitReviewSnapshot) -> GitReviewSnapshot {
    review_snapshots()
        .lock()
        .expect("Git review snapshot registry mutex poisoned")
        .insert_or_get(snapshot)
}

pub(crate) fn get_snapshot(
    snapshot_id: &str,
    caller_fingerprint: &str,
    project: &str,
    session_id: Option<&str>,
) -> Option<GitReviewSnapshot> {
    review_snapshots()
        .lock()
        .expect("Git review snapshot registry mutex poisoned")
        .get(snapshot_id, caller_fingerprint, project, session_id)
}

pub(crate) fn latest_workspace_snapshot(
    caller_fingerprint: &str,
    project: &str,
    session_id: Option<&str>,
) -> Option<GitReviewSnapshot> {
    review_snapshots()
        .lock()
        .expect("Git review snapshot registry mutex poisoned")
        .latest_workspace(caller_fingerprint, project, session_id)
}

pub(crate) fn workspace_snapshot_complete_for_closeout(
    snapshot: &GitReviewSnapshot,
    include_diff: bool,
) -> bool {
    if !snapshot.metadata_complete || snapshot.coverage_partial {
        return false;
    }
    if !include_diff {
        return true;
    }
    let full_paths = snapshot
        .projection_identity
        .get("paths")
        .map(|value| value.is_null() || value.as_array().is_some_and(Vec::is_empty))
        .unwrap_or(false);
    let diff_complete = snapshot.diff_page.is_object()
        && snapshot.diff_page.get("truncated").and_then(Value::as_bool) == Some(false)
        && snapshot.diff_page.get("has_more").and_then(Value::as_bool) == Some(false);
    full_paths && diff_complete
}

impl ToolRuntime {
    pub(crate) async fn workspace_review_source_identity(
        &self,
        project: &str,
    ) -> Result<GitReviewSourceIdentity, ToolResult> {
        let (head_commit, frozen_tree, status_fingerprint) =
            self.freeze_workspace_git_state(project, true).await?;
        let status_fingerprint = status_fingerprint
            .ok_or_else(|| ToolResult::err("workspace review status identity unavailable"))?;
        Ok(GitReviewSourceIdentity::Workspace {
            head_commit,
            frozen_tree,
            status_fingerprint,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(caller: &str, project: &str, tree: &str) -> GitReviewSnapshot {
        GitReviewSnapshot::new(
            caller.to_string(),
            project.to_string(),
            Some("wc_sess_test".to_string()),
            GitReviewScope::Workspace,
            GitReviewSourceIdentity::Workspace {
                head_commit: Some("a".repeat(40)),
                frozen_tree: tree.to_string(),
                status_fingerprint: "0".repeat(64),
            },
            json!({"paths": [], "max_hunks": 24}),
            json!({"files_changed": 1}),
            json!([]),
            json!({"truncated": false, "has_more": false}),
            false,
            true,
        )
    }

    #[test]
    fn workspace_snapshot_identity_changes_with_frozen_tree() {
        let first = snapshot("caller", "project", &"1".repeat(40));
        let second = snapshot("caller", "project", &"2".repeat(40));
        assert_ne!(first.snapshot_id, second.snapshot_id);
        assert_ne!(first.source, second.source);
    }

    #[test]
    fn committed_identity_pins_requested_and_resolved_commits() {
        let scope = CommittedGitScope {
            requested_base: "1".repeat(40),
            requested_head: "2".repeat(40),
            merge_base: "3".repeat(40),
            base_is_ancestor: false,
            commit_count: 2,
            files_changed: 1,
            insertions: 1,
            deletions: 0,
            binary_files: 0,
        };
        assert_eq!(
            committed_source_identity(&scope).presentation_value(),
            json!({
                "kind": "committed",
                "requested_base": "1".repeat(40),
                "requested_head": "2".repeat(40),
                "merge_base": "3".repeat(40),
            })
        );
    }

    #[test]
    fn closeout_reuse_requires_complete_full_workspace_projection_for_diff() {
        let mut candidate = snapshot("caller", "project", &"9".repeat(40));
        assert!(workspace_snapshot_complete_for_closeout(&candidate, true));
        candidate.coverage_partial = true;
        assert!(!workspace_snapshot_complete_for_closeout(&candidate, false));
        candidate.coverage_partial = false;
        candidate.projection_identity["paths"] = json!(["src/lib.rs"]);
        assert!(!workspace_snapshot_complete_for_closeout(&candidate, true));
        assert!(workspace_snapshot_complete_for_closeout(&candidate, false));

        candidate.projection_identity["paths"] = json!([]);
        candidate.diff_page["truncated"] = json!(true);
        assert!(!workspace_snapshot_complete_for_closeout(&candidate, true));
        assert!(workspace_snapshot_complete_for_closeout(&candidate, false));

        candidate.diff_page["truncated"] = json!(false);
        candidate.diff_page["has_more"] = json!(true);
        assert!(!workspace_snapshot_complete_for_closeout(&candidate, true));
        assert!(workspace_snapshot_complete_for_closeout(&candidate, false));
    }

    #[test]
    fn reinserting_same_source_refreshes_snapshot_payload() {
        let mut first = snapshot("caller-refresh", "project", &"8".repeat(40));
        first.signals = json!([{"name": "before"}]);
        let first = insert_snapshot(first);
        let mut refreshed = snapshot("caller-refresh", "project", &"8".repeat(40));
        assert_eq!(first.snapshot_id, refreshed.snapshot_id);
        refreshed.signals = json!([{"name": "after"}]);
        let refreshed = insert_snapshot(refreshed);
        let loaded = get_snapshot(
            &refreshed.snapshot_id,
            "caller-refresh",
            "project",
            Some("wc_sess_test"),
        )
        .unwrap();
        assert_eq!(loaded.signals, json!([{"name": "after"}]));
    }

    #[test]
    fn registry_is_authority_and_session_fenced() {
        let stored = insert_snapshot(snapshot("caller-a", "project", &"4".repeat(40)));
        assert!(get_snapshot(
            &stored.snapshot_id,
            "caller-a",
            "project",
            Some("wc_sess_test")
        )
        .is_some());
        assert!(get_snapshot(
            &stored.snapshot_id,
            "caller-b",
            "project",
            Some("wc_sess_test")
        )
        .is_none());
        assert!(get_snapshot(
            &stored.snapshot_id,
            "caller-a",
            "project",
            Some("wc_sess_other")
        )
        .is_none());
    }
}
