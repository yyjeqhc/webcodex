use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::auth::AuthContext;

use super::helpers::validate_project_relative_path;
use super::session_context::{
    absent_workflow_session_result, session_project_mismatch_result,
    workflow_session_authority_fingerprint, SessionProjectMismatch,
};
use super::{ToolResult, ToolRuntime};

// Work Result is the primary task card and users commonly inspect the final
// result well after closeout. Snapshot metadata is tightly bounded below, so keep
// the immutable per-file view alive for a full day instead of the former 5-minute
// transient presentation window.
const CHANGES_SNAPSHOT_TTL: Duration = Duration::from_secs(24 * 60 * 60);
// Sealed results keep their original quota. Explicit live inspection gets a
// small independent quota, so browsing cannot consume final-result retention.
const MAX_WORKSPACE_SNAPSHOTS: usize = 8;
const MAX_WORKSPACE_SNAPSHOTS_PER_CALLER: usize = 2;
const MAX_CHANGES_SNAPSHOTS: usize = 32;
const MAX_CHANGES_SNAPSHOTS_PER_CALLER: usize = 8;
const MAX_CHANGES_FILES: usize = 24;
const MAX_STORED_CHANGES_FILES: usize = 2_000;
const MAX_CHANGES_PATH_CHARS: usize = 1024;
const CHANGES_METADATA_SOURCE_BYTES: usize = 32 * 1024;
const CHANGES_DIFF_MAX_BYTES: usize = 48 * 1024;
const CHANGES_DIFF_MAX_LINES: usize = 1200;
const CHANGES_CONTENT_PAGE_BYTES: usize = 32 * 1024;
const CHANGES_CONTENT_MAX_BYTES: usize = 256 * 1024;
const CHANGES_PDF_PAGE_BYTES: usize = 128 * 1024;
const CHANGES_PDF_MAX_BYTES: usize = 20 * 1024 * 1024;
const CHANGES_SESSION_SUMMARY_LIMIT: usize = 0;
const SOURCE_BYTES_MARKER: &str = "WEBCODEX_CHANGES_SOURCE_BYTES=";
const DIFF_BYTES_MARKER: &str = "WEBCODEX_CHANGES_DIFF_BYTES=";
const DIFF_LINES_MARKER: &str = "WEBCODEX_CHANGES_DIFF_LINES=";

#[derive(Debug, Clone)]
struct ChangesFileMetadata {
    path: String,
    previous_path: Option<String>,
    kind: &'static str,
    additions: Option<u64>,
    deletions: Option<u64>,
    binary: Option<bool>,
}

impl ChangesFileMetadata {
    fn to_value(&self) -> Value {
        let mut value = Map::new();
        value.insert("path".to_string(), json!(self.path));
        if let Some(previous_path) = self.previous_path.as_ref() {
            value.insert("previous_path".to_string(), json!(previous_path));
        }
        value.insert("kind".to_string(), json!(self.kind));
        value.insert(
            "additions".to_string(),
            self.additions.map_or(Value::Null, Value::from),
        );
        value.insert(
            "deletions".to_string(),
            self.deletions.map_or(Value::Null, Value::from),
        );
        if let Some(binary) = self.binary {
            value.insert("binary".to_string(), json!(binary));
        }
        Value::Object(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotRetention {
    SealedResult,
    WorkspaceInspection,
}

impl SnapshotRetention {
    fn limits(self) -> (usize, usize) {
        match self {
            Self::SealedResult => (MAX_CHANGES_SNAPSHOTS, MAX_CHANGES_SNAPSHOTS_PER_CALLER),
            Self::WorkspaceInspection => {
                (MAX_WORKSPACE_SNAPSHOTS, MAX_WORKSPACE_SNAPSHOTS_PER_CALLER)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ChangesSnapshot {
    retention: SnapshotRetention,
    snapshot_id: String,
    caller_fingerprint: String,
    project: String,
    session_id: Option<String>,
    attempt_key: String,
    baseline_tree: String,
    final_tree: String,
    totals: ChangesTotals,
    files: Vec<ChangesFileMetadata>,
    files_truncated: bool,
    expires_at: Instant,
}

impl ChangesSnapshot {
    fn matches_identity(&self, caller_fingerprint: &str, project: &str, session_id: &str) -> bool {
        self.matches_context(caller_fingerprint, project, Some(session_id))
    }

    fn matches_context(&self, caller: &str, project: &str, session: Option<&str>) -> bool {
        self.caller_fingerprint == caller
            && self.project == project
            && self.session_id.as_deref() == session
    }

    fn matches_attempt(
        &self,
        caller_fingerprint: &str,
        project: &str,
        session_id: &str,
        attempt_key: &str,
    ) -> bool {
        self.matches_identity(caller_fingerprint, project, session_id)
            && self.attempt_key == attempt_key
    }

    fn presentation_value(&self) -> Value {
        json!({
            "snapshot_id": self.snapshot_id,
            "files_changed": self.totals.files,
            "additions": self.totals.additions,
            "deletions": self.totals.deletions,
            "files_total": self.totals.files,
            "files_returned": self.files.len().min(MAX_CHANGES_FILES),
            "files_truncated": self.files_truncated || self.files.len() > MAX_CHANGES_FILES,
            "files": self.files.iter().take(MAX_CHANGES_FILES).map(ChangesFileMetadata::to_value).collect::<Vec<_>>(),
        })
    }
}

#[derive(Default)]
pub(super) struct ChangesSnapshotRegistry {
    snapshots: VecDeque<ChangesSnapshot>,
}

impl ChangesSnapshotRegistry {
    fn prune(&mut self, now: Instant) {
        self.snapshots.retain(|snapshot| snapshot.expires_at > now);
    }

    fn get_for_attempt(
        &mut self,
        caller_fingerprint: &str,
        project: &str,
        session_id: &str,
        attempt_key: &str,
    ) -> Option<ChangesSnapshot> {
        self.prune(Instant::now());
        self.snapshots
            .iter()
            .find(|snapshot| {
                snapshot.matches_attempt(caller_fingerprint, project, session_id, attempt_key)
            })
            .cloned()
    }

    fn insert_or_get(&mut self, snapshot: ChangesSnapshot) -> ChangesSnapshot {
        let now = Instant::now();
        self.prune(now);
        let retention = snapshot.retention;
        let (global_limit, caller_limit) = retention.limits();
        if let Some(existing) = self
            .snapshots
            .iter()
            .find(|candidate| {
                candidate.matches_context(
                    &snapshot.caller_fingerprint,
                    &snapshot.project,
                    snapshot.session_id.as_deref(),
                ) && candidate.attempt_key == snapshot.attempt_key
            })
            .cloned()
        {
            return existing;
        }
        while self
            .snapshots
            .iter()
            .filter(|candidate| {
                candidate.retention == retention
                    && candidate.caller_fingerprint == snapshot.caller_fingerprint
            })
            .count()
            >= caller_limit
        {
            if let Some(index) = self.snapshots.iter().position(|candidate| {
                candidate.retention == retention
                    && candidate.caller_fingerprint == snapshot.caller_fingerprint
            }) {
                self.snapshots.remove(index);
            } else {
                break;
            }
        }
        while self
            .snapshots
            .iter()
            .filter(|candidate| candidate.retention == retention)
            .count()
            >= global_limit
        {
            if let Some(index) = self
                .snapshots
                .iter()
                .position(|candidate| candidate.retention == retention)
            {
                self.snapshots.remove(index);
            } else {
                break;
            }
        }
        self.snapshots.push_back(snapshot.clone());
        snapshot
    }

    #[cfg(test)]
    fn insert(&mut self, snapshot: ChangesSnapshot) {
        let _ = self.insert_or_get(snapshot);
    }

    fn get(&mut self, snapshot_id: &str) -> Option<ChangesSnapshot> {
        self.prune(Instant::now());
        self.snapshots
            .iter()
            .find(|snapshot| snapshot.snapshot_id == snapshot_id)
            .cloned()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ChangesTotals {
    files: u64,
    additions: u64,
    deletions: u64,
}

/// Keep Final Changes under read/presentation authority even when repository
/// configuration defines executable clean/process filters or an fsmonitor hook.
/// The command-scope overlay preserves ordinary Git config (autocrlf, sparse
/// checkout, ignores, etc.) while replacing only execution-bearing filters with
/// identity/no-op behavior for this observation.
pub(super) const CHANGES_GIT_SAFE_CONFIG_SETUP: &str = r#"changes_git_overlay=$(mktemp "${TMPDIR:-/tmp}/webcodex-changes-config.XXXXXX")
changes_git_filter_keys=$(mktemp "${TMPDIR:-/tmp}/webcodex-changes-filter-keys.XXXXXX")
changes_git_tmp_index=
changes_git_review_status_tmp=
changes_git_untracked_tmp=
changes_git_cleanup() {
  rm -f -- "$changes_git_overlay" "$changes_git_filter_keys"
  if [ -n "$changes_git_tmp_index" ]; then rm -f -- "$changes_git_tmp_index"; fi
  if [ -n "$changes_git_review_status_tmp" ]; then rm -f -- "$changes_git_review_status_tmp"; fi
  if [ -n "$changes_git_untracked_tmp" ]; then rm -f -- "$changes_git_untracked_tmp"; fi
}
trap changes_git_cleanup 0 HUP INT TERM
: >"$changes_git_overlay"
set +e
git config --null --name-only --get-regexp '^filter\.' >"$changes_git_filter_keys"
changes_git_filter_status=$?
set -e
if [ "$changes_git_filter_status" -ne 0 ] && [ "$changes_git_filter_status" -ne 1 ]; then
  exit 71
fi
if [ -s "$changes_git_filter_keys" ]; then
  CHANGES_GIT_OVERLAY="$changes_git_overlay" xargs -0 -n 1 sh -c '
    key=$1
    case "$key" in
      *.clean) value=cat ;;
      *.process) value= ;;
      *.required) value=false ;;
      *) exit 0 ;;
    esac
    git config --file "$CHANGES_GIT_OVERLAY" "$key" "$value"
  ' sh <"$changes_git_filter_keys"
fi
changes_git() {
  git -c include.path="$changes_git_overlay" -c core.fsmonitor=false "$@"
}
"#;

/// Build the same bounded workspace observation for direct and compound reviews.
pub(in crate::tool_runtime) fn workspace_freeze_command(capture_review_status: bool) -> String {
    // A private temporary index snapshots HEAD plus the complete current
    // workspace without touching the real index/ref/worktree. Custom Git
    // clean/process filters and fsmonitor are neutralized because this is a
    // read-authority observation path, not repository-configured execution.
    // `git add` may still write immutable blobs/trees to the object database;
    // the resulting tree is intentionally unreachable observation state.
    // Scope GIT_INDEX_FILE to a subshell: macOS sh (Bash 3.2) retains inline
    // assignments before function calls, which would make the subsequent
    // review status read this temporary index instead of the real index.
    format!(
        r#"set -eu
LC_ALL=C; export LC_ALL
GIT_TERMINAL_PROMPT=0; export GIT_TERMINAL_PROMPT
umask 077
{safe_config_setup}
changes_git_tmp_index=$(mktemp "${{TMPDIR:-/tmp}}/webcodex-changes-index.XXXXXX")
rm -f "$changes_git_tmp_index"
head=""
if changes_git rev-parse --verify HEAD >/dev/null 2>&1; then
  head=$(changes_git rev-parse --verify HEAD)
fi
tree=$(
  set -e
  GIT_INDEX_FILE="$changes_git_tmp_index"; export GIT_INDEX_FILE
  if [ -n "$head" ]; then
    changes_git read-tree "$head"
  else
    changes_git read-tree --empty
  fi
  changes_git add -A -- .
  changes_git write-tree
)
printf 'WEBCODEX_WORKSPACE_HEAD=%s\nWEBCODEX_WORKSPACE_TREE=%s\n' "$head" "$tree"
{review_status}
"#,
        safe_config_setup = CHANGES_GIT_SAFE_CONFIG_SETUP,
        // Porcelain v2 includes branch identity, staged object ids, modes,
        // conflicts and untracked classification. The frozen worktree alone
        // cannot fence metadata/diffs after staging or a same-HEAD switch.
        // Keep this in the same Runner request and do not refresh the real index.
        review_status = if capture_review_status {
            r#"changes_git_review_status_tmp=$(mktemp "${TMPDIR:-/tmp}/webcodex-review-status.XXXXXX")
changes_git --no-optional-locks status --porcelain=v2 --branch --untracked-files=all --ignore-submodules=none >"$changes_git_review_status_tmp"
status_fingerprint=$(changes_git hash-object --no-filters -- "$changes_git_review_status_tmp")
printf 'WEBCODEX_WORKSPACE_STATUS=%s\n' "$status_fingerprint""#
        } else {
            ""
        },
    )
}

pub(in crate::tool_runtime) fn parse_workspace_freeze(
    stdout: &str,
    capture_review_status: bool,
) -> Result<(Option<String>, String, Option<String>), ToolResult> {
    let mut head = None;
    let mut tree = None;
    let mut status_fingerprint = None;
    for line in stdout.lines() {
        if let Some(value) = line.strip_prefix("WEBCODEX_WORKSPACE_HEAD=") {
            if !value.is_empty() {
                if !valid_git_object_id(value) {
                    return Err(changes_runtime_error(
                        "changes_snapshot_failed",
                        "Git returned an invalid workspace HEAD id",
                    ));
                }
                head = Some(value.to_string());
            }
        } else if let Some(value) = line.strip_prefix("WEBCODEX_WORKSPACE_TREE=") {
            tree = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("WEBCODEX_WORKSPACE_STATUS=") {
            if !valid_git_object_id(value) {
                return Err(changes_runtime_error(
                    "changes_snapshot_failed",
                    "Git returned an invalid review status fingerprint",
                ));
            }
            status_fingerprint = Some(value.to_string());
        }
    }
    let Some(tree) = tree else {
        return Err(changes_runtime_error(
            "changes_snapshot_failed",
            "Git did not return a frozen workspace tree id",
        ));
    };
    if !valid_git_object_id(&tree) {
        return Err(changes_runtime_error(
            "changes_snapshot_failed",
            "Git returned an invalid frozen workspace tree id",
        ));
    }
    if capture_review_status && status_fingerprint.is_none() {
        return Err(changes_runtime_error(
            "changes_snapshot_failed",
            "Git did not return workspace review status",
        ));
    }
    Ok((head, tree, status_fingerprint))
}

impl ToolRuntime {
    /// Cheap closeout eligibility probe. It intentionally does not freeze a
    /// snapshot or generate diff bodies: only the explicit presentation call
    /// pays the temporary-index/write-tree cost.
    pub(crate) async fn final_changes_presentation_needed(
        &self,
        project: &str,
        summary: &super::sessions::SessionSummary,
    ) -> Result<bool, String> {
        if !summary.repository_edit_observed {
            return Ok(false);
        }
        let Some(baseline_tree) = summary.git_baseline_tree.as_deref() else {
            return Ok(false);
        };
        if !valid_git_object_id(baseline_tree) {
            return Err("Workflow Session has an invalid Git baseline tree".to_string());
        }

        let script = format!(
            r#"set -eu
LC_ALL=C; export LC_ALL
GIT_TERMINAL_PROMPT=0; export GIT_TERMINAL_PROMPT
umask 077
{safe_config_setup}
set +e
changes_git --no-pager diff --quiet --no-ext-diff --no-textconv {baseline_tree} -- .
diff_status=$?
set -e
if [ "$diff_status" -eq 1 ]; then
  exit 10
fi
if [ "$diff_status" -ne 0 ]; then
  exit 20
fi
changes_git_untracked_tmp=$(mktemp "${{TMPDIR:-/tmp}}/webcodex-changes-untracked.XXXXXX")
changes_git ls-files --others --exclude-standard -z -- . >"$changes_git_untracked_tmp"
if [ -s "$changes_git_untracked_tmp" ]; then
  exit 10
fi
exit 0
"#,
            safe_config_setup = CHANGES_GIT_SAFE_CONFIG_SETUP,
        );
        let output = self
            .run_project_internal_posix_script_capture(project, script, 30, None)
            .await
            .map_err(|error| format!("Changes closeout probe failed: {error}"))?;
        match output.exit_code {
            Some(0) => Ok(false),
            Some(10) => Ok(true),
            other => Err(format!(
                "Changes closeout probe could not compare the Session baseline to the current workspace (exit_code={other:?})"
            )),
        }
    }

    pub(super) async fn seal_work_result_changes_for_closeout(
        &self,
        project: &str,
        summary: &super::sessions::SessionSummary,
        auth: Option<&AuthContext>,
    ) -> Result<Option<Value>, ToolResult> {
        let Some(attempt_key) = self
            .sessions
            .retained_task_instruction_event_id_at(&summary.session_id, summary.events_total)
        else {
            return Ok(None);
        };
        self.freeze_work_result_changes(project, summary, &attempt_key, auth)
            .await
    }

    pub(super) fn sealed_work_result_changes(
        &self,
        project: &str,
        summary: &super::sessions::SessionSummary,
        auth: Option<&AuthContext>,
    ) -> Result<Option<Value>, ToolResult> {
        let Some(attempt_key) = self
            .sessions
            .retained_task_instruction_event_id_at(&summary.session_id, summary.events_total)
        else {
            return Ok(None);
        };
        let caller_fingerprint = workflow_session_authority_fingerprint(auth)
            .map_err(|_| changes_identity_error("session_authority_denied"))?;
        Ok(self
            .changes_snapshots
            .lock()
            .expect("Changes snapshot registry mutex poisoned")
            .get_for_attempt(
                &caller_fingerprint,
                project,
                &summary.session_id,
                &attempt_key,
            )
            .map(|snapshot| snapshot.presentation_value()))
    }

    /// Seal or reuse the immutable final-changes domain for one non-blocking
    /// coding closeout. The caller has independently authorized this exact Project
    /// and Session; neither a card nor a snapshot is authority. Repeated reads for
    /// the same attempt reuse the same frozen identity.
    pub(super) async fn freeze_work_result_changes(
        &self,
        project: &str,
        summary: &super::sessions::SessionSummary,
        attempt_key: &str,
        auth: Option<&AuthContext>,
    ) -> Result<Option<Value>, ToolResult> {
        let Some(baseline_tree) = summary.git_baseline_tree.as_deref() else {
            return Ok(None);
        };
        if !summary.repository_edit_observed {
            return Ok(None);
        }
        if !valid_git_object_id(baseline_tree) {
            return Err(changes_unavailable("session_git_baseline_invalid"));
        }
        let caller_fingerprint = workflow_session_authority_fingerprint(auth)
            .map_err(|_| changes_identity_error("session_authority_denied"))?;
        if let Some(snapshot) = self
            .changes_snapshots
            .lock()
            .expect("Changes snapshot registry mutex poisoned")
            .get_for_attempt(
                &caller_fingerprint,
                project,
                &summary.session_id,
                attempt_key,
            )
        {
            return Ok(Some(snapshot.presentation_value()));
        }
        let final_tree = self.freeze_final_workspace_tree(project).await?;
        if final_tree == baseline_tree {
            return Ok(None);
        }
        let (totals, files, files_truncated) = self
            .changes_metadata(project, baseline_tree, &final_tree)
            .await?;
        if totals.files == 0 {
            return Ok(None);
        }

        let snapshot_id = changes_snapshot_id(
            &caller_fingerprint,
            project,
            &summary.session_id,
            attempt_key,
            baseline_tree,
            &final_tree,
        );
        let snapshot = ChangesSnapshot {
            retention: SnapshotRetention::SealedResult,
            snapshot_id,
            caller_fingerprint,
            project: project.to_string(),
            session_id: Some(summary.session_id.clone()),
            attempt_key: attempt_key.to_string(),
            baseline_tree: baseline_tree.to_string(),
            final_tree,
            totals,
            files,
            files_truncated,
            expires_at: Instant::now() + CHANGES_SNAPSHOT_TTL,
        };
        let snapshot = self
            .changes_snapshots
            .lock()
            .expect("Changes snapshot registry mutex poisoned")
            .insert_or_get(snapshot);

        Ok(Some(snapshot.presentation_value()))
    }

    /// Shared lazy UI reader: exact caller/Project/optional Session, immutable
    /// source, bounded pages, and the same safe per-file diff producer as finals.
    pub(crate) async fn work_result_files(
        &self,
        project: String,
        session_id: Option<String>,
        request: webcodex_tool_contracts::WorkResultFilesRequest,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if request.offset > MAX_STORED_CHANGES_FILES
            || (request.view.is_some() && request.path.is_none())
            || (request.view.is_none() && request.byte_offset != 0)
            || request.byte_offset
                >= match request.view {
                    Some(webcodex_tool_contracts::WorkResultFileView::Pdf) => CHANGES_PDF_MAX_BYTES,
                    _ => CHANGES_CONTENT_MAX_BYTES,
                }
            || request
                .path
                .as_deref()
                .is_some_and(|path| !valid_changes_path(path))
            || (request.path.is_some() && (request.snapshot_id.is_none() || request.offset != 0))
            || (request.snapshot_id.is_none() && (session_id.is_some() || request.offset != 0))
        {
            return changes_identity_error("changes_page_invalid");
        }
        let caller = match workflow_session_authority_fingerprint(auth) {
            Ok(caller) => caller,
            Err(_) => return changes_identity_error("session_authority_denied"),
        };
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) if resolved.resolved_id == project => resolved,
            _ => return changes_identity_error("changes_project_not_exact"),
        };
        if let Some(session) = session_id.as_deref() {
            if let Err(result) = self
                .authorize_exact_changes_context(&project, session, "read_changed_file_diff", auth)
                .await
            {
                return result;
            }
        }
        let snapshot = if let Some(id) = request.snapshot_id {
            let found = self
                .changes_snapshots
                .lock()
                .expect("Changes registry")
                .get(&id);
            match found {
                Some(snapshot)
                    if snapshot.matches_context(&caller, &project, session_id.as_deref()) =>
                {
                    snapshot
                }
                _ => return changes_identity_error("changes_snapshot_unavailable"),
            }
        } else {
            let (head, tree, _) = match self
                .freeze_workspace_git_state(&resolved.resolved_id, false)
                .await
            {
                Ok(value) => value,
                Err(result) => return result,
            };
            let baseline = match head {
                Some(head) => head,
                None => {
                    let empty = self
                        .run_project_internal_posix_script_capture(
                            &project,
                            "printf '' | git hash-object -t tree --stdin".into(),
                            10,
                            None,
                        )
                        .await;
                    match empty {
                        Ok(output)
                            if output.exit_code == Some(0)
                                && valid_git_object_id(output.stdout.trim()) =>
                        {
                            output.stdout.trim().to_string()
                        }
                        _ => return changes_identity_error("changes_snapshot_failed"),
                    }
                }
            };
            let (totals, files, files_truncated) =
                match self.changes_metadata(&project, &baseline, &tree).await {
                    Ok(value) => value,
                    Err(result) => return result,
                };
            let attempt_key = format!("workspace:{baseline}:{tree}");
            let snapshot = ChangesSnapshot {
                retention: SnapshotRetention::WorkspaceInspection,
                snapshot_id: changes_snapshot_id(
                    &caller,
                    &project,
                    "",
                    &attempt_key,
                    &baseline,
                    &tree,
                ),
                caller_fingerprint: caller,
                project: project.clone(),
                session_id: None,
                attempt_key,
                baseline_tree: baseline,
                final_tree: tree,
                totals,
                files,
                files_truncated,
                expires_at: Instant::now() + CHANGES_SNAPSHOT_TTL,
            };
            self.changes_snapshots
                .lock()
                .expect("Changes registry")
                .insert_or_get(snapshot)
        };
        if let Some(path) = request.path {
            let Some(file) = snapshot.files.iter().find(|file| file.path == path) else {
                return changes_identity_error("changes_snapshot_path_not_allowed");
            };
            if let Some(view) = request.view {
                let page = match view {
                    webcodex_tool_contracts::WorkResultFileView::Content => {
                        self.frozen_changes_file_content(&snapshot, file, request.byte_offset)
                            .await
                    }
                    webcodex_tool_contracts::WorkResultFileView::Pdf => {
                        self.frozen_changes_file_pdf(&snapshot, file, request.byte_offset)
                            .await
                    }
                };
                return match page {
                    Ok(content) => ToolResult::ok(json!({"work_result_files": content})),
                    Err(result) => result,
                };
            }
            let diff = match self.frozen_changes_file_diff(&snapshot, file).await {
                Ok(diff) => diff,
                Err(result) => return result,
            };
            return ToolResult::ok(json!({"work_result_files": {
                "project": project, "session_id": session_id, "snapshot_id": snapshot.snapshot_id,
                "path": path, "diff": diff.text, "truncated": diff.truncated,
            }}));
        }
        if request.offset > snapshot.files.len() {
            return changes_identity_error("changes_page_invalid");
        }
        let end = request
            .offset
            .saturating_add(MAX_CHANGES_FILES)
            .min(snapshot.files.len());
        ToolResult::ok(json!({"work_result_files": {
            "project": project, "session_id": session_id, "snapshot_id": snapshot.snapshot_id,
            "offset": request.offset, "next_offset": (end < snapshot.files.len()).then_some(end),
            "files_total": snapshot.totals.files, "source_truncated": snapshot.files_truncated,
            "files": snapshot.files[request.offset..end].iter().map(ChangesFileMetadata::to_value).collect::<Vec<_>>(),
        }}))
    }

    pub(crate) async fn changes_file_diff(
        &self,
        project: String,
        session_id: String,
        snapshot_id: String,
        path: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let (resolved_project, _summary, caller_fingerprint) = match self
            .authorize_exact_changes_context(&project, &session_id, "read_changed_file_diff", auth)
            .await
        {
            Ok(context) => context,
            Err(result) => return result,
        };
        if !valid_changes_path(&path) {
            return changes_identity_error("changes_snapshot_path_invalid");
        }

        let snapshot = self
            .changes_snapshots
            .lock()
            .expect("Changes snapshot registry mutex poisoned")
            .get(&snapshot_id);
        let Some(snapshot) = snapshot else {
            return changes_identity_error("changes_snapshot_unavailable");
        };
        if !snapshot.matches_identity(&caller_fingerprint, &resolved_project, &session_id) {
            return changes_identity_error("changes_snapshot_identity_mismatch");
        }
        let Some(file) = snapshot.files.iter().find(|file| file.path == path) else {
            return changes_identity_error("changes_snapshot_path_not_allowed");
        };

        let diff = match self.frozen_changes_file_diff(&snapshot, file).await {
            Ok(diff) => diff,
            Err(result) => return result,
        };
        ToolResult::ok(json!({
            "changes_file_diff": {
                "version": 1,
                "project": resolved_project,
                "session_id": session_id,
                "snapshot_id": snapshot_id,
                "path": file.path,
                "previous_path": file.previous_path,
                "kind": file.kind,
                "binary": file.binary,
                "diff": diff.text,
                "bytes_total": diff.bytes_total,
                "bytes_returned": diff.text.len(),
                "lines_total": diff.lines_total,
                "lines_returned": diff.text.lines().count(),
                "truncated": diff.truncated,
            }
        }))
    }

    async fn authorize_exact_changes_context(
        &self,
        project: &str,
        session_id: &str,
        tool_name: &'static str,
        auth: Option<&AuthContext>,
    ) -> Result<(String, super::sessions::SessionSummary, String), ToolResult> {
        self.authorize_session_target(session_id, tool_name, auth)
            .await?;
        let resolved = self
            .resolve_project_input_for_auth(project, auth)
            .await
            .map_err(|error| error.into_tool_result())?;
        if project.trim() != resolved.resolved_id {
            return Err(ToolResult::err_with_output(
                "Changes presentation requires the exact complete runtime project id",
                json!({
                    "error_kind": "changes_project_not_exact",
                    "failure_kind": "invalid_arguments",
                    "state_changed": false,
                }),
            ));
        }
        let Some(summary) = self
            .sessions
            .summary(session_id, Some(CHANGES_SESSION_SUMMARY_LIMIT))
        else {
            return Err(absent_workflow_session_result(
                &self.sessions,
                session_id,
                auth,
            ));
        };
        if summary.project.as_deref() != Some(resolved.resolved_id.as_str()) {
            return Err(session_project_mismatch_result(
                session_id,
                tool_name,
                &SessionProjectMismatch {
                    session_project: summary
                        .project
                        .clone()
                        .unwrap_or_else(|| "<unscoped>".to_string()),
                    request_project: resolved.resolved_id,
                },
            ));
        }
        let caller_fingerprint = workflow_session_authority_fingerprint(auth)
            .map_err(|_| changes_identity_error("session_authority_denied"))?;
        Ok((resolved.resolved_id, summary, caller_fingerprint))
    }

    pub(crate) async fn freeze_workspace_git_state(
        &self,
        project: &str,
        capture_review_status: bool,
    ) -> Result<(Option<String>, String, Option<String>), ToolResult> {
        let output = self
            .run_project_internal_posix_script_capture(
                project,
                workspace_freeze_command(capture_review_status),
                60,
                None,
            )
            .await
            .map_err(|error| changes_runtime_error("changes_snapshot_failed", error))?;
        if output.exit_code != Some(0) || output.stdout_truncated {
            return Err(changes_runtime_error(
                "changes_snapshot_failed",
                "Git could not freeze the workspace tree",
            ));
        }
        parse_workspace_freeze(&output.stdout, capture_review_status)
    }

    pub(crate) async fn freeze_final_workspace_tree(
        &self,
        project: &str,
    ) -> Result<String, ToolResult> {
        self.freeze_workspace_git_state(project, false)
            .await
            .map(|(_, tree, _)| tree)
    }
    async fn changes_metadata(
        &self,
        project: &str,
        baseline_tree: &str,
        final_tree: &str,
    ) -> Result<(ChangesTotals, Vec<ChangesFileMetadata>, bool), ToolResult> {
        let shortstat = self
            .run_project_internal_posix_script_capture(
                project,
                format!(
                    "LC_ALL=C git --no-pager diff --shortstat --no-ext-diff --no-textconv --find-renames {baseline_tree} {final_tree} -- ."
                ),
                30,
                None,
            )
            .await
            .map_err(|error| changes_runtime_error("changes_metadata_failed", error))?;
        if shortstat.exit_code != Some(0) || shortstat.stdout_truncated {
            return Err(changes_runtime_error(
                "changes_metadata_failed",
                "Git could not summarize the frozen Changes snapshot",
            ));
        }
        let totals = parse_shortstat(&shortstat.stdout);

        let (name_status, status_source_truncated) = self
            .bounded_tree_diff_source(
                project,
                baseline_tree,
                final_tree,
                "--name-status -z --find-renames",
            )
            .await?;
        let (numstat, numstat_source_truncated) = self
            .bounded_tree_diff_source(
                project,
                baseline_tree,
                final_tree,
                "--numstat -z --find-renames",
            )
            .await?;
        let statuses = parse_name_status_z(&name_status);
        let stats = parse_numstat_z(&numstat);
        let mut files = Vec::new();
        for mut file in statuses.into_iter().take(MAX_STORED_CHANGES_FILES) {
            if let Some(stat) = stats.get(&file.path) {
                file.additions = stat.additions;
                file.deletions = stat.deletions;
                file.binary = Some(stat.binary);
            }
            files.push(file);
        }
        let files_truncated = status_source_truncated
            || numstat_source_truncated
            || totals.files > files.len() as u64;
        Ok((totals, files, files_truncated))
    }

    async fn bounded_tree_diff_source(
        &self,
        project: &str,
        baseline_tree: &str,
        final_tree: &str,
        mode: &'static str,
    ) -> Result<(String, bool), ToolResult> {
        debug_assert!(valid_git_object_id(baseline_tree));
        debug_assert!(valid_git_object_id(final_tree));
        let script = format!(
            r#"set -eu
LC_ALL=C; export LC_ALL
umask 077
tmp=$(mktemp "${{TMPDIR:-/tmp}}/webcodex-changes-meta.XXXXXX")
trap 'rm -f "$tmp"' 0 HUP INT TERM
git --no-pager diff --no-ext-diff --no-textconv {mode} {baseline_tree} {final_tree} -- . >"$tmp"
bytes=$(wc -c <"$tmp" | tr -d '[:space:]')
printf '{SOURCE_BYTES_MARKER}%s\n' "$bytes" >&2
dd if="$tmp" bs=1 count={CHANGES_METADATA_SOURCE_BYTES} 2>/dev/null
"#
        );
        let output = self
            .run_project_internal_posix_script_capture(project, script, 30, None)
            .await
            .map_err(|error| changes_runtime_error("changes_metadata_failed", error))?;
        if output.exit_code != Some(0) {
            return Err(changes_runtime_error(
                "changes_metadata_failed",
                "Git could not enumerate the frozen Changes snapshot",
            ));
        }
        let source_bytes =
            marker_u64(&output.stderr, SOURCE_BYTES_MARKER).unwrap_or(output.stdout.len() as u64);
        let truncated = output.stdout_truncated || source_bytes > output.stdout.len() as u64;
        Ok((output.stdout, truncated))
    }

    async fn frozen_changes_file_content(
        &self,
        snapshot: &ChangesSnapshot,
        file: &ChangesFileMetadata,
        byte_offset: usize,
    ) -> Result<Value, ToolResult> {
        let unavailable = |reason: &str| {
            json!({
                "project": snapshot.project, "session_id": snapshot.session_id,
                "snapshot_id": snapshot.snapshot_id, "path": file.path,
                "view": "content", "unavailable_reason": reason,
            })
        };
        if file.kind == "deleted" {
            return Ok(unavailable("deleted"));
        }
        if file.binary == Some(true) {
            return Ok(unavailable("binary"));
        }
        let object = match self.frozen_changes_file_blob(snapshot, file).await? {
            FrozenFileBlob::Object(object) => object,
            FrozenFileBlob::Unavailable(reason) => return Ok(unavailable(reason)),
        };
        // Base64 is internal Runner transport only, preserving bytes across
        // shell charset normalization. Three lookahead bytes cover a UTF-8
        // scalar at a page boundary; no whole-file temporary is created.
        let count = (CHANGES_CONTENT_MAX_BYTES - byte_offset).min(CHANGES_CONTENT_PAGE_BYTES) + 3;
        let script = format!(
            r#"set -eu
bytes=$(git cat-file -s {object})
printf '%s\n' "$bytes"
git cat-file blob {object} | dd bs=1 skip={byte_offset} count={count} 2>/dev/null | base64
"#,
        );
        let output = self
            .run_project_internal_posix_script_capture(&snapshot.project, script, 30, None)
            .await
            .map_err(|error| changes_runtime_error("changes_file_content_failed", error))?;
        if output.exit_code != Some(0) || output.stdout_truncated || output.stderr_truncated {
            return Err(changes_identity_error("changes_file_content_failed"));
        }
        let Some((size, encoded)) = output.stdout.split_once('\n') else {
            return Err(changes_identity_error("changes_file_content_failed"));
        };
        let bytes_total = size
            .parse::<usize>()
            .map_err(|_| changes_identity_error("changes_file_content_failed"))?;
        if byte_offset > bytes_total {
            return Err(changes_identity_error("changes_page_invalid"));
        }
        let encoded = encoded
            .chars()
            .filter(|ch| !ch.is_ascii_whitespace())
            .collect::<String>();
        let raw = STANDARD
            .decode(encoded)
            .map_err(|_| changes_identity_error("changes_file_content_failed"))?;
        if raw.len() != (bytes_total - byte_offset).min(count) {
            return Err(changes_identity_error("changes_file_content_failed"));
        }
        if raw.contains(&0) {
            return Ok(unavailable("binary"));
        }
        let text = match std::str::from_utf8(&raw) {
            Ok(text) => text,
            Err(error) if error.error_len().is_none() && byte_offset + raw.len() < bytes_total => {
                std::str::from_utf8(&raw[..error.valid_up_to()]).expect("validated UTF-8 prefix")
            }
            Err(_) => return Ok(unavailable("non_utf8")),
        };
        let page_limit = (CHANGES_CONTENT_MAX_BYTES - byte_offset).min(CHANGES_CONTENT_PAGE_BYTES);
        let mut end = text.len().min(page_limit);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let content = &text[..end];
        let next = byte_offset + end;
        let complete = next == bytes_total;
        let limited = !complete
            && next + text[end..].chars().next().map_or(1, char::len_utf8)
                > CHANGES_CONTENT_MAX_BYTES;
        if !complete && end == 0 && !limited {
            return Err(changes_identity_error("changes_file_content_failed"));
        }
        Ok(json!({
            "project": snapshot.project, "session_id": snapshot.session_id,
            "snapshot_id": snapshot.snapshot_id, "path": file.path, "view": "content",
            "byte_offset": byte_offset, "bytes_total": bytes_total, "content": content,
            "next_byte_offset": (!complete && !limited).then_some(next),
            "complete": complete, "limited": limited,
        }))
    }

    async fn frozen_changes_file_blob(
        &self,
        snapshot: &ChangesSnapshot,
        file: &ChangesFileMetadata,
    ) -> Result<FrozenFileBlob, ToolResult> {
        // Resolve exactly one immutable entry; never read a live filesystem path
        // or execute filters. Check the exact path before using its object id.
        let script = format!(
            "git --no-pager ls-tree -z {} -- {}",
            snapshot.final_tree,
            shell_single_quote(&format!(":(literal){}", file.path))
        );
        let entry = self
            .run_project_internal_posix_script_capture(&snapshot.project, script, 30, None)
            .await
            .map_err(|error| changes_runtime_error("changes_file_content_failed", error))?;
        if entry.exit_code != Some(0) || entry.stdout_truncated || entry.stderr_truncated {
            return Err(changes_identity_error("changes_file_content_failed"));
        }
        let Some((header, path)) = entry.stdout.split_once('\t') else {
            return Err(changes_identity_error("changes_file_content_failed"));
        };
        if path.strip_suffix('\0') != Some(file.path.as_str()) {
            return Err(changes_identity_error("changes_file_content_failed"));
        }
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 || !valid_git_object_id(fields[2]) {
            return Err(changes_identity_error("changes_file_content_failed"));
        }
        match fields[0] {
            "120000" => return Ok(FrozenFileBlob::Unavailable("symlink")),
            "160000" => return Ok(FrozenFileBlob::Unavailable("submodule")),
            "100644" | "100755" if fields[1] == "blob" => {}
            _ => return Ok(FrozenFileBlob::Unavailable("unsupported_file_type")),
        }
        Ok(FrozenFileBlob::Object(fields[2].to_string()))
    }

    async fn frozen_changes_file_pdf(
        &self,
        snapshot: &ChangesSnapshot,
        file: &ChangesFileMetadata,
        byte_offset: usize,
    ) -> Result<Value, ToolResult> {
        let unavailable = |reason: &str| {
            json!({
                "project": snapshot.project, "session_id": snapshot.session_id,
                "snapshot_id": snapshot.snapshot_id, "path": file.path,
                "view": "pdf", "unavailable_reason": reason,
            })
        };
        if file.kind == "deleted" {
            return Ok(unavailable("deleted"));
        }
        let object = match self.frozen_changes_file_blob(snapshot, file).await? {
            FrozenFileBlob::Object(object) => object,
            FrozenFileBlob::Unavailable(reason) => return Ok(unavailable(reason)),
        };
        // Validate size and signature before streaming a bounded segment. Read
        // only the immutable Git blob, including for a renamed/untracked PDF.
        // 128 KiB of bytes fits Runner's bounded text capture after Base64.
        let script = format!(
            r#"set -eu
bytes=$(git cat-file -s {object})
printf '%s\n' "$bytes"
if [ "$bytes" -eq 0 ] || [ "$bytes" -gt {CHANGES_PDF_MAX_BYTES} ]; then exit 0; fi
magic=$(git cat-file blob {object} | dd bs=1 count=5 2>/dev/null | base64)
if [ "$magic" != 'JVBERi0=' ]; then printf 'not_pdf\n'; exit 0; fi
printf 'pdf\n'
git cat-file blob {object} | dd bs=4096 skip={block} 2>/dev/null | dd bs=1 skip={remainder} count={CHANGES_PDF_PAGE_BYTES} 2>/dev/null | base64
"#,
            block = byte_offset / 4096,
            remainder = byte_offset % 4096,
        );
        let output = self
            .run_project_internal_posix_script_capture(&snapshot.project, script, 30, None)
            .await
            .map_err(|error| changes_runtime_error("changes_file_content_failed", error))?;
        if output.exit_code != Some(0) || output.stdout_truncated || output.stderr_truncated {
            return Err(changes_identity_error("changes_file_content_failed"));
        }
        let Some((size, encoded)) = output.stdout.split_once('\n') else {
            return Err(changes_identity_error("changes_file_content_failed"));
        };
        let bytes_total = size
            .parse::<usize>()
            .map_err(|_| changes_identity_error("changes_file_content_failed"))?;
        if bytes_total > CHANGES_PDF_MAX_BYTES {
            return Ok(unavailable("too_large"));
        }
        if bytes_total == 0 {
            return Ok(unavailable("not_pdf"));
        }
        if byte_offset >= bytes_total {
            return Err(changes_identity_error("changes_page_invalid"));
        }
        let Some((kind, encoded)) = encoded.split_once('\n') else {
            return Err(changes_identity_error("changes_file_content_failed"));
        };
        if kind == "not_pdf" {
            return Ok(unavailable("not_pdf"));
        }
        if kind != "pdf" {
            return Err(changes_identity_error("changes_file_content_failed"));
        }
        let encoded = encoded
            .chars()
            .filter(|ch| !ch.is_ascii_whitespace())
            .collect::<String>();
        let raw = STANDARD
            .decode(encoded)
            .map_err(|_| changes_identity_error("changes_file_content_failed"))?;
        if raw.len() != (bytes_total - byte_offset).min(CHANGES_PDF_PAGE_BYTES) {
            return Err(changes_identity_error("changes_file_content_failed"));
        }
        let next = byte_offset + raw.len();
        let complete = next == bytes_total;
        Ok(json!({
            "project": snapshot.project, "session_id": snapshot.session_id,
            "snapshot_id": snapshot.snapshot_id, "path": file.path, "view": "pdf",
            "byte_offset": byte_offset, "bytes_total": bytes_total,
            "content_base64": STANDARD.encode(raw),
            "next_byte_offset": (!complete).then_some(next), "complete": complete,
        }))
    }

    async fn frozen_changes_file_diff(
        &self,
        snapshot: &ChangesSnapshot,
        file: &ChangesFileMetadata,
    ) -> Result<FrozenFileDiff, ToolResult> {
        let mut pathspecs = Vec::new();
        if let Some(previous_path) = file.previous_path.as_ref() {
            pathspecs.push(shell_single_quote(&format!(":(literal){previous_path}")));
        }
        pathspecs.push(shell_single_quote(&format!(":(literal){}", file.path)));
        let pathspecs = pathspecs.join(" ");
        let script = format!(
            r#"set -eu
LC_ALL=C; export LC_ALL
umask 077
tmp=$(mktemp "${{TMPDIR:-/tmp}}/webcodex-changes-diff.XXXXXX")
trap 'rm -f "$tmp"' 0 HUP INT TERM
git --no-pager diff --no-ext-diff --no-textconv --find-renames --unified=3 {} {} -- {pathspecs} >"$tmp"
bytes=$(wc -c <"$tmp" | tr -d '[:space:]')
lines=$(wc -l <"$tmp" | tr -d '[:space:]')
printf '{DIFF_BYTES_MARKER}%s\n{DIFF_LINES_MARKER}%s\n' "$bytes" "$lines" >&2
head -n {CHANGES_DIFF_MAX_LINES} "$tmp" | dd bs=1 count={CHANGES_DIFF_MAX_BYTES} 2>/dev/null
"#,
            snapshot.baseline_tree, snapshot.final_tree,
        );
        let output = self
            .run_project_internal_posix_script_capture(&snapshot.project, script, 30, None)
            .await
            .map_err(|error| changes_runtime_error("changes_file_diff_failed", error))?;
        if output.exit_code != Some(0) {
            return Err(changes_runtime_error(
                "changes_file_diff_failed",
                "Git could not read the frozen file diff",
            ));
        }
        let bytes_total =
            marker_u64(&output.stderr, DIFF_BYTES_MARKER).unwrap_or(output.stdout.len() as u64);
        let lines_total = marker_u64(&output.stderr, DIFF_LINES_MARKER)
            .unwrap_or(output.stdout.lines().count() as u64);
        // Runner text decoding may expand a clipped multibyte sequence. Bound
        // the returned UTF-8 representation too, not only the source Git bytes.
        let mut text = output.stdout;
        let text_bounded = bound_frozen_diff_text(&mut text);
        let truncated = output.stdout_truncated
            || text_bounded
            || bytes_total > text.len() as u64
            || lines_total > text.lines().count() as u64;
        Ok(FrozenFileDiff {
            text,
            bytes_total,
            lines_total,
            truncated,
        })
    }
}

fn changes_snapshot_id(
    caller_fingerprint: &str,
    project: &str,
    session_id: &str,
    attempt_key: &str,
    baseline_tree: &str,
    final_tree: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.work-result.sealed-changes.v1\0");
    for value in [
        caller_fingerprint,
        project,
        session_id,
        attempt_key,
        baseline_tree,
        final_tree,
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    let digest = hasher.finalize();
    let suffix = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("wc_changes_snapshot_{suffix}")
}

fn bound_frozen_diff_text(text: &mut String) -> bool {
    if text.len() <= CHANGES_DIFF_MAX_BYTES {
        return false;
    }
    let mut end = CHANGES_DIFF_MAX_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    true
}

#[derive(Debug)]
enum FrozenFileBlob {
    Object(String),
    Unavailable(&'static str),
}

#[derive(Debug)]
struct FrozenFileDiff {
    text: String,
    bytes_total: u64,
    lines_total: u64,
    truncated: bool,
}

#[derive(Debug, Clone, Copy)]
struct Numstat {
    additions: Option<u64>,
    deletions: Option<u64>,
    binary: bool,
}

fn parse_shortstat(source: &str) -> ChangesTotals {
    let mut totals = ChangesTotals::default();
    let tokens = source.split_whitespace().collect::<Vec<_>>();
    for pair in tokens.windows(2) {
        let value = pair[0].trim_end_matches(',').parse::<u64>().ok();
        let Some(value) = value else { continue };
        if pair[1].starts_with("file") {
            totals.files = value;
        } else if pair[1].starts_with("insertion") {
            totals.additions = value;
        } else if pair[1].starts_with("deletion") {
            totals.deletions = value;
        }
    }
    totals
}

fn complete_nul_prefix(source: &str) -> &str {
    if source.ends_with('\0') {
        source
    } else {
        source
            .rfind('\0')
            .map(|index| &source[..=index])
            .unwrap_or_default()
    }
}

fn valid_changes_path(path: &str) -> bool {
    if path.is_empty()
        || path == "."
        || path.chars().count() > MAX_CHANGES_PATH_CHARS
        || path.chars().any(char::is_control)
        || path.starts_with('/')
        || path.starts_with('\\')
    {
        return false;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return false;
    }
    if path.split(['/', '\\']).any(|component| component == "..") {
        return false;
    }
    !crate::sensitive_paths::is_secret_path(path) && validate_project_relative_path(path).is_ok()
}

fn parse_name_status_z(source: &str) -> Vec<ChangesFileMetadata> {
    let fields = complete_nul_prefix(source).split('\0').collect::<Vec<_>>();
    let mut index = 0;
    let mut files = Vec::new();
    while index < fields.len() && files.len() < MAX_STORED_CHANGES_FILES {
        let status = fields[index];
        index += 1;
        if status.is_empty() {
            break;
        }
        let code = status.as_bytes().first().copied().unwrap_or_default();
        let (previous_path, path) = if matches!(code, b'R' | b'C') {
            if index + 1 >= fields.len() {
                break;
            }
            let previous = fields[index];
            let current = fields[index + 1];
            index += 2;
            (Some(previous), current)
        } else {
            if index >= fields.len() {
                break;
            }
            let current = fields[index];
            index += 1;
            (None, current)
        };
        if !valid_changes_path(path)
            || previous_path.is_some_and(|previous| !valid_changes_path(previous))
        {
            continue;
        }
        let kind = match code {
            b'A' => "added",
            b'D' => "deleted",
            b'R' => "renamed",
            b'C' => "added",
            _ => "modified",
        };
        files.push(ChangesFileMetadata {
            path: path.to_string(),
            previous_path: previous_path.map(str::to_string),
            kind,
            additions: None,
            deletions: None,
            binary: None,
        });
    }
    files
}

fn parse_numstat_z(source: &str) -> BTreeMap<String, Numstat> {
    let fields = complete_nul_prefix(source).split('\0').collect::<Vec<_>>();
    let mut index = 0;
    let mut stats = BTreeMap::new();
    while index < fields.len() {
        let header = fields[index];
        index += 1;
        if header.is_empty() {
            break;
        }
        let mut columns = header.splitn(3, '\t');
        let additions = columns.next().unwrap_or_default();
        let deletions = columns.next().unwrap_or_default();
        let path_field = columns.next().unwrap_or_default();
        let path = if path_field.is_empty() {
            if index + 1 >= fields.len() {
                break;
            }
            index += 1; // previous path
            let current = fields[index];
            index += 1;
            current
        } else {
            path_field
        };
        if !valid_changes_path(path) {
            continue;
        }
        let binary = additions == "-" || deletions == "-";
        stats.insert(
            path.to_string(),
            Numstat {
                additions: (!binary).then(|| additions.parse::<u64>().ok()).flatten(),
                deletions: (!binary).then(|| deletions.parse::<u64>().ok()).flatten(),
                binary,
            },
        );
    }
    stats
}

fn marker_u64(stderr: &str, marker: &str) -> Option<u64> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix(marker)?.trim().parse::<u64>().ok())
}

fn valid_git_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_fixture(id: &str, caller: &str) -> ChangesSnapshot {
        ChangesSnapshot {
            snapshot_id: id.to_string(),
            retention: SnapshotRetention::SealedResult,
            caller_fingerprint: caller.to_string(),
            project: "agent:runner:project".to_string(),
            session_id: Some("session".to_string()),
            attempt_key: format!("attempt-{id}"),
            baseline_tree: "a".repeat(40),
            final_tree: "b".repeat(40),
            totals: ChangesTotals::default(),
            files: Vec::new(),
            files_truncated: false,
            expires_at: Instant::now() + CHANGES_SNAPSHOT_TTL,
        }
    }

    #[test]
    fn changes_registry_is_shared_by_clones_but_isolated_between_runtimes() {
        let runtime = ToolRuntime::new_for_tests();
        let clone = runtime.clone();
        let independent = ToolRuntime::new_for_tests();
        runtime
            .changes_snapshots
            .lock()
            .unwrap()
            .insert(snapshot_fixture("runtime-only", "caller"));
        assert!(clone
            .changes_snapshots
            .lock()
            .unwrap()
            .get("runtime-only")
            .is_some());
        assert!(independent
            .changes_snapshots
            .lock()
            .unwrap()
            .get("runtime-only")
            .is_none());
        for index in 0..MAX_CHANGES_SNAPSHOTS * 2 {
            independent
                .changes_snapshots
                .lock()
                .unwrap()
                .insert(snapshot_fixture(&format!("other-{index}"), "caller"));
        }
        assert!(clone
            .changes_snapshots
            .lock()
            .unwrap()
            .get("runtime-only")
            .is_some());
        let weak = std::sync::Arc::downgrade(&runtime.changes_snapshots);
        drop(runtime);
        assert!(weak.upgrade().is_some());
        drop(clone);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn frozen_changes_snapshot_possession_does_not_replace_caller_project_or_session_authority() {
        let snapshot = snapshot_fixture("known-snapshot", "caller");
        assert!(snapshot.matches_identity("caller", "agent:runner:project", "session"));
        assert!(!snapshot.matches_identity("other", "agent:runner:project", "session"));
        assert!(!snapshot.matches_identity("caller", "agent:runner:other", "session"));
        assert!(!snapshot.matches_identity("caller", "agent:runner:project", "other"));
    }

    #[test]
    fn frozen_changes_registry_expires_without_refresh_extending_its_lifetime() {
        let mut registry = ChangesSnapshotRegistry::default();
        let current = snapshot_fixture("current", "caller");
        let expires_at = current.expires_at;
        registry.insert(current);
        assert_eq!(registry.get("current").unwrap().expires_at, expires_at);
        assert!(registry.get("missing").is_none());
        let mut expired = snapshot_fixture("expired", "caller");
        expired.expires_at = Instant::now() - Duration::from_secs(1);
        registry.insert(expired);
        assert!(registry.get("expired").is_none());
        registry.prune(expires_at);
        assert!(registry.get("current").is_none());
        assert!(registry.snapshots.is_empty());
    }

    #[test]
    fn frozen_changes_registry_keeps_per_caller_and_process_bounds() {
        let mut registry = ChangesSnapshotRegistry::default();
        for index in 0..=MAX_CHANGES_SNAPSHOTS_PER_CALLER {
            registry.insert(snapshot_fixture(&format!("same-{index}"), "caller"));
        }
        assert_eq!(registry.snapshots.len(), MAX_CHANGES_SNAPSHOTS_PER_CALLER);
        assert!(registry.get("same-0").is_none());
        assert!(registry.get("same-1").is_some());
        for index in 0..=MAX_CHANGES_SNAPSHOTS {
            registry.insert(snapshot_fixture(
                &format!("other-{index}"),
                &format!("caller-{index}"),
            ));
        }
        assert_eq!(registry.snapshots.len(), MAX_CHANGES_SNAPSHOTS);
        assert!(registry.get("other-0").is_none());
        assert!(registry.get("other-1").is_some());
        assert!(registry.get("same-1").is_none());
    }

    fn workspace_snapshot_fixture(id: &str, caller: &str) -> ChangesSnapshot {
        ChangesSnapshot {
            retention: SnapshotRetention::WorkspaceInspection,
            session_id: None,
            ..snapshot_fixture(id, caller)
        }
    }

    #[test]
    fn live_inspection_pressure_never_evicts_a_sealed_result_for_the_same_caller() {
        let mut registry = ChangesSnapshotRegistry::default();
        let sealed = snapshot_fixture("sealed", "caller");
        let expiry = sealed.expires_at;
        registry.insert(sealed);
        for index in 0..(MAX_CHANGES_SNAPSHOTS_PER_CALLER + 4) {
            registry.insert(workspace_snapshot_fixture(
                &format!("live-{index}"),
                "caller",
            ));
        }
        assert_eq!(registry.get("sealed").unwrap().expires_at, expiry);
        assert_eq!(
            registry.snapshots.len(),
            MAX_WORKSPACE_SNAPSHOTS_PER_CALLER + 1
        );
        assert!(registry.get("live-0").is_none());
        assert!(registry
            .get_for_attempt(
                "caller",
                "agent:runner:project",
                "session",
                "attempt-sealed"
            )
            .is_some());
    }

    #[test]
    fn global_retention_quotas_are_independent_bounded_and_bidirectional() {
        let mut registry = ChangesSnapshotRegistry::default();
        for index in 0..MAX_CHANGES_SNAPSHOTS {
            registry.insert(snapshot_fixture(
                &format!("sealed-{index}"),
                &format!("caller-{index}"),
            ));
        }
        for index in 0..=MAX_WORKSPACE_SNAPSHOTS {
            registry.insert(workspace_snapshot_fixture(
                &format!("live-{index}"),
                &format!("caller-{index}"),
            ));
        }
        assert_eq!(
            registry.snapshots.len(),
            MAX_CHANGES_SNAPSHOTS + MAX_WORKSPACE_SNAPSHOTS
        );
        for index in 0..MAX_CHANGES_SNAPSHOTS {
            assert!(registry.get(&format!("sealed-{index}")).is_some());
        }
        assert!(registry.get("live-0").is_none());
        assert!(registry.get("live-1").is_some());
        registry.insert(snapshot_fixture("sealed-new", "new-caller"));
        assert!(registry.get("sealed-0").is_none());
        assert!(registry.get("live-1").is_some());
        assert_eq!(
            registry.snapshots.len(),
            MAX_CHANGES_SNAPSHOTS + MAX_WORKSPACE_SNAPSHOTS
        );
    }

    #[test]
    fn live_snapshot_replay_neither_extends_ttl_nor_spends_another_slot() {
        let mut registry = ChangesSnapshotRegistry::default();
        let live = workspace_snapshot_fixture("live", "caller");
        let expires_at = live.expires_at;
        registry.insert(live);
        let replay = registry.insert_or_get(workspace_snapshot_fixture("live", "caller"));
        assert_eq!(replay.expires_at, expires_at);
        assert_eq!(registry.snapshots.len(), 1);
        assert!(!replay.matches_context("foreign", "agent:runner:project", None));
        assert!(!replay.matches_context("caller", "agent:runner:project", Some("session")));
        registry.prune(expires_at);
        assert!(registry.get("live").is_none());
    }

    #[test]
    fn frozen_changes_diff_budget_bounds_returned_utf8_not_only_source_bytes() {
        let mut text = format!("{}汉", "a".repeat(CHANGES_DIFF_MAX_BYTES - 1));
        assert!(bound_frozen_diff_text(&mut text));
        assert_eq!(text.len(), CHANGES_DIFF_MAX_BYTES - 1);
        assert!(text.is_char_boundary(text.len()));
        assert!(!bound_frozen_diff_text(&mut text));
        let mut exact = "a".repeat(CHANGES_DIFF_MAX_BYTES);
        assert!(!bound_frozen_diff_text(&mut exact));
        assert_eq!(exact.len(), CHANGES_DIFF_MAX_BYTES);
    }

    #[test]
    fn nul_metadata_parsers_ignore_incomplete_bounded_tail() {
        let files = parse_name_status_z("M\0src/a.rs\0A\0src/partial");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/a.rs");

        let stats = parse_numstat_z("1\t2\tsrc/a.rs\x003\t4\tsrc/partial");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats["src/a.rs"].additions, Some(1));
        assert_eq!(stats["src/a.rs"].deletions, Some(2));
    }

    #[test]
    fn changes_paths_match_the_app_safe_identity_boundary() {
        for safe in ["src/a.rs", "leading space.rs", "name\\part.rs"] {
            assert!(valid_changes_path(safe), "{safe:?}");
        }
        for unsafe_path in [
            "line\nbreak.rs",
            "tab\tname.rs",
            "/absolute.rs",
            "\\rooted.rs",
            "C:drive.rs",
            "../outside.rs",
            "src/../outside.rs",
            ".",
        ] {
            assert!(!valid_changes_path(unsafe_path), "{unsafe_path:?}");
        }

        let files =
            parse_name_status_z("M\0src/a.rs\0M\0line\nbreak.rs\0M\0C:drive.rs\0M\0\\rooted.rs\0");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/a.rs");

        let stats = parse_numstat_z("1\t2\tsrc/a.rs\x003\t4\tline\nbreak.rs\x005\t6\tC:drive.rs\0");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats["src/a.rs"].additions, Some(1));
        assert_eq!(stats["src/a.rs"].deletions, Some(2));
    }

    #[test]
    fn nul_metadata_parsers_preserve_rename_and_binary_truth() {
        let files = parse_name_status_z("R100\0old.rs\0new.rs\0M\0blob.bin\0");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].kind, "renamed");
        assert_eq!(files[0].previous_path.as_deref(), Some("old.rs"));
        assert_eq!(files[0].path, "new.rs");

        let stats = parse_numstat_z("2\t1\t\0old.rs\0new.rs\0-\t-\tblob.bin\0");
        assert_eq!(stats["new.rs"].additions, Some(2));
        assert_eq!(stats["new.rs"].deletions, Some(1));
        assert!(!stats["new.rs"].binary);
        assert!(stats["blob.bin"].binary);
        assert_eq!(stats["blob.bin"].additions, None);
        assert_eq!(stats["blob.bin"].deletions, None);
    }
}

fn changes_unavailable(reason: &'static str) -> ToolResult {
    ToolResult::err_with_output(
        "Frozen final changes are not available for this Workflow Session",
        json!({
            "error_kind": "changes_not_available",
            "reason": reason,
            "state_changed": false,
        }),
    )
}

fn changes_identity_error(error_kind: &'static str) -> ToolResult {
    ToolResult::err_with_output(
        "Changes snapshot identity is invalid or unavailable",
        json!({
            "error_kind": error_kind,
            "failure_kind": "invalid_arguments",
            "state_changed": false,
        }),
    )
}

fn changes_runtime_error(error_kind: &'static str, message: impl Into<String>) -> ToolResult {
    ToolResult::err_with_output(
        message.into(),
        json!({
            "error_kind": error_kind,
            "failure_kind": "operation_failed",
            "state_changed": false,
        }),
    )
}
