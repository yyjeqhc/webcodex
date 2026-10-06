//! Retained workspace-read state, independent of request/view timing policy.
//!
//! Revisions bind an exact target plus SHA, not a filesystem generation. Cache
//! reuse still probes the owning Runner for fresh full-file SHA evidence; writes
//! do not proactively invalidate either store. Authorization stays at callers.

use super::files::ProjectFileReader;
use super::read_cache::ReadCache;
use super::read_revisions::{ReadRevisionLookupError, ReadRevisionRegistry, ReadRevisionTarget};
use super::{ResolvedProject, ToolResult};
use std::sync::Arc;
use tokio::time::Instant;

#[derive(Default)]
pub(crate) struct WorkspaceReadRuntime {
    revisions: ReadRevisionRegistry,
    cache: Arc<ReadCache>,
}

impl WorkspaceReadRuntime {
    pub(super) fn revision_target(
        resolved: &ResolvedProject,
        path: &str,
        runner_instance_id: &str,
    ) -> ReadRevisionTarget {
        ReadRevisionTarget::from_resolved(resolved, path, runner_instance_id)
    }

    pub(crate) fn observe_revision(
        &self,
        target: ReadRevisionTarget,
        sha256: impl Into<String>,
    ) -> u64 {
        self.revisions.observe(target, sha256)
    }

    /// Resolves only retained identity/SHA. The Runner must still enforce the SHA
    /// at the actual read/write boundary, including changes by external processes.
    pub(super) fn resolve_revision(
        &self,
        revision: u64,
        target: &ReadRevisionTarget,
    ) -> Result<String, ReadRevisionLookupError> {
        self.revisions.resolve(revision, target)
    }

    pub(super) async fn read_project_snapshot(
        &self,
        reader: &ProjectFileReader,
        resolved: &ResolvedProject,
        runner_project_id: &str,
        runner_instance_id: &str,
        path: String,
        start_line: Option<usize>,
        limit: Option<usize>,
        expected_sha256: Option<&str>,
        deadline: Instant,
    ) -> ToolResult {
        self.cache
            .read_project_snapshot(
                reader,
                resolved,
                runner_project_id,
                runner_instance_id,
                path,
                start_line,
                limit,
                expected_sha256,
                deadline,
            )
            .await
    }
}

#[cfg(test)]
#[path = "tests/workspace_reads.rs"]
mod tests;
