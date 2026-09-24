use std::collections::{BTreeMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::auth::AuthContext;

use super::helpers::validate_project_relative_path;
use super::session_context::{
    session_project_mismatch_result, unknown_session_result,
    workflow_session_authority_fingerprint, SessionProjectMismatch,
};
use super::{ToolResult, ToolRuntime};

// Work Result is the primary task card and users commonly inspect the final
// result well after closeout. Snapshot metadata is tightly bounded below, so keep
// the immutable per-file view alive for a full day instead of the former 5-minute
// transient presentation window.
const CHANGES_SNAPSHOT_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_CHANGES_SNAPSHOTS: usize = 32;
const MAX_CHANGES_SNAPSHOTS_PER_CALLER: usize = 8;
const MAX_CHANGES_FILES: usize = 24;
const MAX_CHANGES_PATH_CHARS: usize = 1024;
const CHANGES_METADATA_SOURCE_BYTES: usize = 32 * 1024;
const CHANGES_DIFF_MAX_BYTES: usize = 48 * 1024;
const CHANGES_DIFF_MAX_LINES: usize = 1200;
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

#[derive(Debug, Clone)]
struct ChangesSnapshot {
    snapshot_id: String,
    caller_fingerprint: String,
    project: String,
    session_id: String,
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
        self.caller_fingerprint == caller_fingerprint
            && self.project == project
            && self.session_id == session_id
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
            "files_returned": self.files.len(),
            "files_truncated": self.files_truncated,
            "files": self.files.iter().map(ChangesFileMetadata::to_value).collect::<Vec<_>>(),
        })
    }
}

#[derive(Default)]
struct ChangesSnapshotRegistry {
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
        if let Some(existing) = self
            .snapshots
            .iter()
            .find(|candidate| {
                candidate.matches_attempt(
                    &snapshot.caller_fingerprint,
                    &snapshot.project,
                    &snapshot.session_id,
                    &snapshot.attempt_key,
                )
            })
            .cloned()
        {
            return existing;
        }
        while self
            .snapshots
            .iter()
            .filter(|candidate| candidate.caller_fingerprint == snapshot.caller_fingerprint)
            .count()
            >= MAX_CHANGES_SNAPSHOTS_PER_CALLER
        {
            if let Some(index) = self
                .snapshots
                .iter()
                .position(|candidate| candidate.caller_fingerprint == snapshot.caller_fingerprint)
            {
                self.snapshots.remove(index);
            } else {
                break;
            }
        }
        while self.snapshots.len() >= MAX_CHANGES_SNAPSHOTS {
            self.snapshots.pop_front();
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

fn changes_snapshots() -> &'static Mutex<ChangesSnapshotRegistry> {
    static REGISTRY: OnceLock<Mutex<ChangesSnapshotRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(ChangesSnapshotRegistry::default()))
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
changes_git_untracked_tmp=
changes_git_cleanup() {
  rm -f -- "$changes_git_overlay" "$changes_git_filter_keys"
  if [ -n "$changes_git_tmp_index" ]; then rm -f -- "$changes_git_tmp_index"; fi
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
        Ok(changes_snapshots()
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
        if let Some(snapshot) = changes_snapshots()
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
            snapshot_id,
            caller_fingerprint,
            project: project.to_string(),
            session_id: summary.session_id.clone(),
            attempt_key: attempt_key.to_string(),
            baseline_tree: baseline_tree.to_string(),
            final_tree,
            totals,
            files,
            files_truncated,
            expires_at: Instant::now() + CHANGES_SNAPSHOT_TTL,
        };
        let snapshot = changes_snapshots()
            .lock()
            .expect("Changes snapshot registry mutex poisoned")
            .insert_or_get(snapshot);

        Ok(Some(snapshot.presentation_value()))
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
            .authorize_exact_changes_context(&project, &session_id, "changes_file_diff", auth)
            .await
        {
            Ok(context) => context,
            Err(result) => return result,
        };
        if !valid_changes_path(&path) {
            return changes_identity_error("changes_snapshot_path_invalid");
        }

        let snapshot = changes_snapshots()
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
            return Err(unknown_session_result(session_id));
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

    async fn freeze_final_workspace_tree(&self, project: &str) -> Result<String, ToolResult> {
        // A private temporary index snapshots HEAD plus the complete current
        // workspace without touching the real index/ref/worktree. Custom Git
        // clean/process filters and fsmonitor are neutralized because this is a
        // read-authority presentation path, not repository-configured execution.
        // `git add` may still write immutable blobs/trees to the object database;
        // the resulting tree is intentionally unreachable presentation state.
        let script = format!(
            r#"set -eu
LC_ALL=C; export LC_ALL
GIT_TERMINAL_PROMPT=0; export GIT_TERMINAL_PROMPT
umask 077
{safe_config_setup}
changes_git_tmp_index=$(mktemp "${{TMPDIR:-/tmp}}/webcodex-changes-index.XXXXXX")
rm -f "$changes_git_tmp_index"
if changes_git rev-parse --verify HEAD >/dev/null 2>&1; then
  GIT_INDEX_FILE="$changes_git_tmp_index" changes_git read-tree HEAD
else
  GIT_INDEX_FILE="$changes_git_tmp_index" changes_git read-tree --empty
fi
GIT_INDEX_FILE="$changes_git_tmp_index" changes_git add -A -- .
GIT_INDEX_FILE="$changes_git_tmp_index" changes_git write-tree
"#,
            safe_config_setup = CHANGES_GIT_SAFE_CONFIG_SETUP,
        );
        let output = self
            .run_project_internal_posix_script_capture(project, script.to_string(), 60, None)
            .await
            .map_err(|error| changes_runtime_error("changes_snapshot_failed", error))?;
        if output.exit_code != Some(0) || output.stdout_truncated {
            return Err(changes_runtime_error(
                "changes_snapshot_failed",
                "Git could not freeze the final workspace tree",
            ));
        }
        let tree = output.stdout.trim();
        if !valid_git_object_id(tree) {
            return Err(changes_runtime_error(
                "changes_snapshot_failed",
                "Git returned an invalid frozen workspace tree id",
            ));
        }
        Ok(tree.to_string())
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
        for mut file in statuses.into_iter().take(MAX_CHANGES_FILES) {
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
    validate_project_relative_path(path).is_ok()
}

fn parse_name_status_z(source: &str) -> Vec<ChangesFileMetadata> {
    let fields = complete_nul_prefix(source).split('\0').collect::<Vec<_>>();
    let mut index = 0;
    let mut files = Vec::new();
    while index < fields.len() && files.len() < MAX_CHANGES_FILES {
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
            caller_fingerprint: caller.to_string(),
            project: "agent:runner:project".to_string(),
            session_id: "session".to_string(),
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

        let stats = parse_numstat_z("1\t2\tsrc/a.rs\03\t4\tsrc/partial");
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

        let stats = parse_numstat_z("1\t2\tsrc/a.rs\03\t4\tline\nbreak.rs\05\t6\tC:drive.rs\0");
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
