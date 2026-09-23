use std::collections::BTreeMap;
#[cfg(test)]
use std::path::Path;
use std::time::Duration;

use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};

use webcodex_core::runtime_contract::DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES;

use super::super::helpers::{
    decode_git_quoted_path, shell_escape_simple, validate_project_relative_path,
};
use super::super::tool_result::{SuggestedToolCall, ToolResult};
use super::super::ToolRuntime;
use super::diff_hunks::{
    git_diff_file_hunk_count, git_diff_files_have_omitted_lines, parse_git_diff_hunks,
    sparsify_complete_diff_files, DEFAULT_MAX_HUNKS, DEFAULT_MAX_HUNK_LINES, MAX_MAX_HUNK_LINES,
};
use super::shared::{
    parse_fixed_decimal, parse_optional_bool, parse_optional_usize, parse_status_result_field,
    strip_wire_lf,
};
use crate::runner_protocol::ShellRunRequest;
use crate::tool_runtime::sessions::{SessionEvent, SessionSummary};

#[cfg(test)]
pub(crate) const SHOW_CHANGES_SENTINEL: &str = "@@WEBCODEX_SHOW_CHANGES_SEP@@";
const SHOW_CHANGES_BLOCK_TRAILER_BYTES: usize = 30;
const SHOW_CHANGES_BLOCK_MAGIC: &[u8; 6] = b"WCSF1:";

const SHOW_CHANGES_DEFAULT_MAX_HUNKS: usize = 20;
const SHOW_CHANGES_MAX_HUNKS: usize = 100;
const SHOW_CHANGES_DEFAULT_MAX_HUNK_LINES: usize = 80;
const SHOW_CHANGES_MAX_HUNK_LINES: usize = 240;
// Keep overview context materially below the default per-hunk line ceiling so
// ordinary small mid-file edits remain decision-complete in show_changes.
const SHOW_CHANGES_DIFF_CONTEXT_LINES: usize = 20;
const SHOW_CHANGES_SESSION_SIGNAL_EVENT_LIMIT: usize = 30;
const SHOW_CHANGES_MAX_SESSION_EVENT_LIMIT: usize = 200;
/// Maximum number of changed-file records `show_changes` emits on the
/// production side. The total count stays exact (all entries are counted); only
/// the returned records are bounded so a multi-thousand-file status never
/// overflows ordinary Runner result-retention headroom.
pub(crate) const SHOW_CHANGES_MAX_STATUS_FILES: usize = 200;
/// Production-side stdout budget for the whole `show_changes` command. The
/// command is constructed so its worst-case raw stdout stays under this value,
/// which is itself below the ordinary Runner per-stream result-retention default
/// of 256 KiB, with room for protocol framing and error text. That retention
/// bound is not the polling/WebSocket/QUIC wire ceiling. Bounding happens in the
/// command itself, never by relying on retained-tail truncation.
pub(crate) const SHOW_CHANGES_OUTPUT_BUDGET_BYTES: usize = 224 * 1024;
/// Reserved protocol space inside the output budget. The observation/result
/// metadata frames and the diff metadata frame must always remain complete even
/// when an individual segment overflows its own budget, so the script carves
/// this many bytes out of [`SHOW_CHANGES_OUTPUT_BUDGET_BYTES`] before splitting
/// the rest across the status records, HEAD metadata, diff stat, and diff
/// hunks. Sized to comfortably cover all metadata bodies, fixed-length trailers,
/// and `printf` overhead in the worst case.
const SHOW_CHANGES_PROTOCOL_RESERVE_BYTES: usize = 8 * 1024;
/// Independent byte budget for the emitted status records (file entries only;
/// the branch header and the status result frame are always preserved). The
/// status byte budget is enforced *in addition to* [`SHOW_CHANGES_MAX_STATUS_FILES`]:
/// a record is only returned when both the count limit and the byte budget have
/// room, and an overlong path skips its record and reports a structured reason.
pub(crate) const SHOW_CHANGES_STATUS_BYTES: usize = 96 * 1024;
/// Byte budget for the `git diff --stat` segment.
pub(crate) const SHOW_CHANGES_DIFF_STAT_BYTES: usize = 24 * 1024;
/// Byte budget for the per-file `git diff --numstat` segment used only to
/// enrich already-bounded changed-file records with additions/deletions. The
/// segment is parsed server-side and is never exposed as raw presentation text.
pub(crate) const SHOW_CHANGES_NUMSTAT_BYTES: usize = 24 * 1024;
/// Byte budget for the HEAD metadata segment (`git log -1`).
pub(crate) const SHOW_CHANGES_HEAD_BYTES: usize = 8 * 1024;
/// Byte budget for the emitted diff hunks segment (hunk bodies, file headers,
/// and preambles). Hunks are additionally bounded by `max_hunks` and
/// `max_hunk_lines`; this byte budget guarantees a single pathological diff
/// line cannot overflow the global budget.
pub(crate) const SHOW_CHANGES_DIFF_BYTES: usize = 48 * 1024;
const SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_FILES: usize = 5;
const SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_BYTES: u64 = 8192;
const SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_LINES: usize = 40;

// The per-segment byte budgets are each independently bounded in the
// production script; their sum plus the fixed protocol reserve must fit within
// the command output budget, so the script's raw stdout is provably at or
// under `SHOW_CHANGES_OUTPUT_BUDGET_BYTES` whenever every segment's metadata
// frame is present. This compile-time check pins that invariant: changing any
// segment budget (or the reserve) without shrinking the sum below the budget
// fails to build, rather than silently overflowing its producer budget.
const _: () = assert!(
    SHOW_CHANGES_STATUS_BYTES
        + SHOW_CHANGES_HEAD_BYTES
        + SHOW_CHANGES_DIFF_STAT_BYTES
        + SHOW_CHANGES_NUMSTAT_BYTES
        + SHOW_CHANGES_DIFF_BYTES
        + SHOW_CHANGES_PROTOCOL_RESERVE_BYTES
        <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShowChangesStatusObservationKind {
    Observed,
    NonGit,
    CommandFailed,
    OutputUnavailable,
}

impl ShowChangesStatusObservationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::NonGit => "non_git",
            Self::CommandFailed => "command_failed",
            Self::OutputUnavailable => "output_unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShowChangesStatusObservation {
    kind: ShowChangesStatusObservationKind,
    reason_code: Option<&'static str>,
    pub(crate) exit_code: Option<i32>,
    repository_probe: &'static str,
    repository_probe_exit_code: Option<i32>,
    git_available: bool,
}

impl ShowChangesStatusObservation {
    fn observed(exit_code: Option<i32>) -> Self {
        Self {
            kind: ShowChangesStatusObservationKind::Observed,
            reason_code: None,
            exit_code,
            repository_probe: "inside_worktree",
            repository_probe_exit_code: Some(0),
            git_available: true,
        }
    }

    pub(crate) fn status_observed(&self) -> bool {
        self.kind == ShowChangesStatusObservationKind::Observed
    }

    fn non_git(&self) -> bool {
        self.kind == ShowChangesStatusObservationKind::NonGit
    }

    pub(crate) fn as_json(&self) -> Value {
        json!({
            "status": self.kind.as_str(),
            "reason_code": self.reason_code,
            "exit_code": self.exit_code,
            "repository_probe": self.repository_probe,
            "repository_probe_exit_code": self.repository_probe_exit_code,
        })
    }
}

fn git_status_failure_reason(stderr: &str) -> &'static str {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("bad config")
        || lower.contains("invalid untracked files mode")
        || lower.contains("invalid value for") && lower.contains("config")
    {
        "git_status_config_error"
    } else if lower.contains("permission denied")
        || lower.contains("operation not permitted")
        || lower.contains("access is denied")
    {
        "git_status_permission_denied"
    } else {
        "git_status_command_failed"
    }
}

pub(crate) fn parse_show_changes_status_observation(
    status_stdout: &str,
    status_result_stdout: &str,
    stderr: &str,
) -> ShowChangesStatusObservation {
    let exit_code = parse_status_result_field(status_result_stdout, "status_exit")
        .and_then(|value| value.parse::<i32>().ok());
    let repository_probe = parse_status_result_field(status_result_stdout, "repository_probe");
    let repository_probe_exit_code =
        parse_status_result_field(status_result_stdout, "repository_probe_exit")
            .and_then(|value| value.parse::<i32>().ok());
    let has_reliable_header = status_stdout
        .lines()
        .any(|line| parse_status_header(line).is_some());
    let command_failure_reason = git_status_failure_reason(stderr);

    match (exit_code, repository_probe) {
        (Some(0), Some("inside_worktree")) if has_reliable_header => {
            ShowChangesStatusObservation::observed(Some(0))
        }
        (Some(0), Some("inside_worktree")) => ShowChangesStatusObservation {
            kind: ShowChangesStatusObservationKind::OutputUnavailable,
            reason_code: Some("git_status_header_unavailable"),
            exit_code: Some(0),
            repository_probe: "inside_worktree",
            repository_probe_exit_code,
            git_available: true,
        },
        (Some(code), Some("outside_worktree")) if code != 0 => ShowChangesStatusObservation {
            kind: ShowChangesStatusObservationKind::NonGit,
            reason_code: Some("not_a_git_repository"),
            exit_code: Some(code),
            repository_probe: "outside_worktree",
            repository_probe_exit_code,
            git_available: false,
        },
        (Some(code), Some("inside_worktree")) if code != 0 => ShowChangesStatusObservation {
            kind: ShowChangesStatusObservationKind::CommandFailed,
            reason_code: Some(command_failure_reason),
            exit_code: Some(code),
            repository_probe: "inside_worktree",
            repository_probe_exit_code,
            git_available: true,
        },
        (Some(code), Some("unavailable")) if code != 0 => ShowChangesStatusObservation {
            kind: ShowChangesStatusObservationKind::CommandFailed,
            reason_code: Some(command_failure_reason),
            exit_code: Some(code),
            repository_probe: "unavailable",
            repository_probe_exit_code,
            git_available: false,
        },
        _ => ShowChangesStatusObservation {
            kind: ShowChangesStatusObservationKind::OutputUnavailable,
            reason_code: Some("git_status_result_unavailable"),
            exit_code,
            repository_probe: "unavailable",
            repository_probe_exit_code,
            git_available: false,
        },
    }
}

/// Build the graceful-degradation payload returned by `show_changes` when the
/// project is not a git repository. Git-backed status/diff is reported as
/// unavailable without dumping git's noisy stderr/usage; the session
/// sub-summary is still layered on by the caller via
/// `apply_show_changes_session`.
#[cfg(test)]
pub(crate) fn non_git_show_changes_payload(
    project: &str,
    exit_code: Option<i32>,
    include_diff: bool,
) -> serde_json::Value {
    non_git_show_changes_payload_with_observation(
        project,
        include_diff,
        ShowChangesStatusObservation {
            kind: ShowChangesStatusObservationKind::NonGit,
            reason_code: Some("not_a_git_repository"),
            exit_code,
            repository_probe: "outside_worktree",
            repository_probe_exit_code: exit_code,
            git_available: false,
        },
    )
}

fn non_git_show_changes_payload_with_observation(
    project: &str,
    include_diff: bool,
    observation: ShowChangesStatusObservation,
) -> serde_json::Value {
    let mut payload = json!({
        "project": project,
        "git_available": false,
        "non_git_project": true,
        "git_error": "not a git repository; git-backed diff unavailable",
        "branch": null,
        "upstream_status": "unobserved",
        "upstream_reason_code": "non_git_project",
        "upstream": null,
        "ahead": null,
        "behind": null,
        "head": {
            "commit": null,
            "short": null,
            "summary": null,
        },
        "status_observation": observation.as_json(),
        "clean": null,
        "counts": {
            "modified": 0,
            "added": 0,
            "deleted": 0,
            "renamed": 0,
            "copied": 0,
            "untracked": 0,
            "conflicted": null,
            "staged": 0,
            "unstaged": 0,
        },
        "files": [],
        "files_total": null,
        "files_returned": 0,
        "files_truncated": false,
        "files_limit": SHOW_CHANGES_MAX_STATUS_FILES,
        "transport_safe": false,
        "output_budget_bytes": SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "output_truncated": false,
        "truncation_reasons": [],
        "diff_stat": "",
        "diff_exit": null,
        "diff_status": diff_status_json(None),
        "diff_stat_exit": null,
        "diff_stat_status": {
            "status": "output_unavailable", "exit_code": null, "reason_code": "git_unavailable",
        },
        "head_exit": null,
        "warnings": [],
        "suggested_next_actions": [
            "git-backed status/diff is not applicable; project is not a git repository",
        ],
        "session": null,
        "exit_code": observation.exit_code,
        "stderr": "",
    });
    if include_diff {
        payload["untracked_previews"] = json!([]);
        payload["untracked_previews_truncated"] = json!(false);
    }
    set_show_changes_verdict(&mut payload);
    payload
}

/// Build the read-only, production-bounded `show_changes` command.
///
/// Unlike an unbounded `git status` capture, the status is *streamed* line by
/// line through a POSIX `while read` loop that:
///   * passes the branch header (`## ...`) through verbatim,
///   * emits at most `status_files_limit` changed-file records,
///   * counts every entry and classifies every entry by porcelain XY code so
///     `files_total` and all per-category counts stay exact even when the
///     returned records are truncated,
///   * captures the real `git status` exit code from the final producer line,
///   * runs the explicit repository probe only when status failed.
///
/// Each data/metadata pair ends in a fixed 30-byte `WCSF1` trailer containing
/// the pair kind and exact wire lengths. The parser walks backward by those
/// lengths, so frame bodies are never scanned for delimiter text. The status
/// metadata carries `status_exit`, `repository_probe`, `repository_probe_exit`, the
/// `files_total/files_returned/files_truncated/files_limit` metadata, and the
/// exact per-category counts
/// (`modified/added/deleted/renamed/copied/untracked/conflicted/staged/unstaged`).
///
/// When `include_diff` is set, the `git diff` is likewise bounded in the command
/// by `max_hunks` hunks and `max_hunk_lines` lines per hunk before it leaves the
/// producer, so ordinary Runner/Server result retention does not need to drop
/// the first selected hunks. A trailing diff metadata frame reports the returned count,
/// emitted bytes, and independent count/line/byte truncation flags.
///
/// The shell script is held in a raw string with literal placeholders and
/// interpolated with `str::replace`, so nested `${...}` expansions stay
/// literal and never collide with Rust `format!` brace escaping.
///
/// No Python/Node helper or external temp script is used; only POSIX `sh`,
/// `git`, `printf`, and `grep`. No project worktree is mutated.
pub(crate) fn show_changes_command(
    include_diff: bool,
    max_hunks: usize,
    max_hunk_lines: usize,
) -> String {
    let status_files_limit = SHOW_CHANGES_MAX_STATUS_FILES;
    let diff_part = if include_diff {
        r#"; {
               git diff --unified=__DIFF_CONTEXT_LINES__; printf 'diff_exit=%s\n' "$?";
             } | {
               hc=0; in_hunk=0; lc=0; diff_exit_raw=; diff_bytes=0; file_buf=; stop_emit=0;
               trunc_count=0; trunc_lines=0; trunc_bytes=0; trunc_bytes_in_hunk=0; have=0; pending=;
               while IFS= read -r dl; do
                 next=$dl; dl=$pending; pending=$next;
                 if [ "$have" = 0 ]; then have=1; continue; fi;
                 case "$dl" in
                   '@@ '*)
                     if [ "$stop_emit" = 1 ]; then in_hunk=0; file_buf=; continue; fi;
                     if [ "$hc" -ge __HUNK_LIMIT__ ]; then trunc_count=1; stop_emit=1; in_hunk=0; file_buf=; continue; fi;
                     preamble_len=${#file_buf}; header_len=$((${#dl}+1)); candidate_len=$((preamble_len+header_len));
                     if [ "$((diff_bytes + candidate_len))" -gt __DIFF_BYTE_BUDGET__ ]; then trunc_bytes=1; stop_emit=1; in_hunk=0; file_buf=; continue; fi;
                     if [ -n "$file_buf" ]; then printf '%s' "$file_buf"; file_buf=; fi;
                     hc=$((hc+1)); lc=1; in_hunk=1; printf '%s\n' "$dl"; diff_bytes=$((diff_bytes+candidate_len)) ;;
                   *)
                     case "$dl" in
                       'diff --git '*)
                         in_hunk=0; file_buf=;
                         if [ "$stop_emit" = 0 ]; then
                           line_len=$((${#dl}+1));
                           if [ "$((diff_bytes + line_len))" -gt __DIFF_BYTE_BUDGET__ ]; then trunc_bytes=1; stop_emit=1; else file_buf="$dl
"; fi;
                         fi;
                         continue ;;
                     esac;
                     if [ "$in_hunk" = 1 ]; then
                       if [ "$stop_emit" = 1 ]; then continue; fi;
                       if [ "$lc" -ge __LINE_LIMIT__ ]; then trunc_lines=1;
                       else
                         line_len=$((${#dl}+1));
                         if [ "$((diff_bytes + line_len))" -gt __DIFF_BYTE_BUDGET__ ]; then trunc_bytes=1; trunc_bytes_in_hunk=1; stop_emit=1;
                         else printf '%s\n' "$dl"; lc=$((lc+1)); diff_bytes=$((diff_bytes+line_len)); fi;
                       fi;
                     elif [ "$stop_emit" = 0 ]; then
                       line_len=$((${#dl}+1)); file_len=${#file_buf};
                       if [ "$((diff_bytes + file_len + line_len))" -gt __DIFF_BYTE_BUDGET__ ]; then trunc_bytes=1; stop_emit=1; file_buf=;
                       else file_buf="$file_buf$dl
"; fi;
                     fi ;;
                 esac;
               done;
               case "$pending" in diff_exit=*) diff_exit_raw=${pending#diff_exit=} ;; *) diff_exit_raw= ;; esac;
               if [ "$stop_emit" = 0 ] && [ -n "$file_buf" ]; then printf '%s' "$file_buf"; diff_bytes=$((diff_bytes+${#file_buf})); fi;
               diff_wire_bytes=$diff_bytes; diff_frame_bytes=$diff_wire_bytes;
               if [ "$diff_frame_bytes" -gt 0 ]; then diff_frame_bytes=$((diff_frame_bytes-1)); fi;
               dm=$(printf 'diff_exit=%s\ndiff_hunks_returned=%s\ndiff_hunks_truncated=%s\ndiff_trunc_hunk_count=%s\ndiff_trunc_hunk_lines=%s\ndiff_trunc_bytes=%s\ndiff_trunc_bytes_in_hunk=%s\ndiff_bytes=%s' \
                 "$diff_exit_raw" "$hc" "$((trunc_count || trunc_lines || trunc_bytes))" "$trunc_count" "$trunc_lines" "$trunc_bytes" "$trunc_bytes_in_hunk" "$diff_frame_bytes");
               printf '%s\n' "$dm"; printf 'WCSF1:D:%010d:%010d\n' "$diff_wire_bytes" "$(( ${#dm}+1 ))";
             }"#
        .replace("__HUNK_LIMIT__", &max_hunks.to_string())
        .replace("__LINE_LIMIT__", &max_hunk_lines.to_string())
        .replace(
            "__DIFF_CONTEXT_LINES__",
            &SHOW_CHANGES_DIFF_CONTEXT_LINES.to_string(),
        )
        .replace("__DIFF_BYTE_BUDGET__", &SHOW_CHANGES_DIFF_BYTES.to_string())
    } else {
        String::new()
    };

    let script = r#"{ LC_ALL=C; export LC_ALL;
         { git status --porcelain=v1 -b; printf 'status_exit=%s\n' "$?"; } | {
           n=0; total=0; trunc=0; trunc_count=0; trunc_bytes=0; trunc_path=0; exit_raw=; status_bytes=0;
           c_mod=0; c_add=0; c_del=0; c_ren=0; c_cop=0; c_unt=0; c_conf=0; c_stg=0; c_uns=0; have=0; pending=;
           while IFS= read -r line; do
             next=$line; line=$pending; pending=$next;
             if [ "$have" = 0 ]; then have=1; continue; fi;
             case "$line" in
               '## '*) printf '%s\n' "$line"; status_bytes=$((status_bytes+${#line}+1)) ;;
               *)
                 total=$((total+1)); xc=${line%"${line#?}"}; yc=${line#?}; yc=${yc%"${yc#?}"};
                 case "$xc$yc" in
                   '??') c_unt=$((c_unt+1)); label=untracked ;;
                   UU|AA|DD|AU|UA|DU|UD) c_conf=$((c_conf+1)); label=conflicted ;;
                   *R*) c_ren=$((c_ren+1)); label=renamed ;;
                   *C*) c_cop=$((c_cop+1)); label=copied ;;
                   *D*) c_del=$((c_del+1)); label=deleted ;;
                   *A*) c_add=$((c_add+1)); label=added ;;
                   *) c_mod=$((c_mod+1)); label=modified ;;
                 esac;
                 if [ "$label" != untracked ] && [ "$label" != conflicted ]; then
                   if [ "$xc" != ' ' ] && [ "$xc" != '?' ]; then c_stg=$((c_stg+1)); fi;
                   if [ "$yc" != ' ' ] && [ "$yc" != '?' ]; then c_uns=$((c_uns+1)); fi;
                 fi;
                 rec_len=$((${#line}+1));
                 if [ "$n" -ge __STATUS_LIMIT__ ]; then trunc=1; trunc_count=1;
                 elif [ "$((status_bytes + rec_len))" -gt __STATUS_BYTE_BUDGET__ ]; then trunc=1; trunc_bytes=1;
                 elif [ "$rec_len" -gt __STATUS_MAX_RECORD_BYTES__ ]; then trunc=1; trunc_path=1;
                 else printf '%s\n' "$line"; n=$((n+1)); status_bytes=$((status_bytes+rec_len)); fi ;;
             esac;
           done;
           case "$pending" in status_exit=*) exit_raw=${pending#status_exit=} ;; *) exit_raw= ;; esac;
           status_wire_bytes=$status_bytes; if [ "$status_bytes" -gt 0 ]; then status_bytes=$((status_bytes-1)); fi;
           if [ -z "$exit_raw" ]; then exit_raw=0; fi;
           if [ "$exit_raw" -eq 0 ] 2>/dev/null; then repo_probe=inside_worktree; repo_probe_exit=0;
           else
             repo_probe_stdout=$(LC_ALL=C git rev-parse --is-inside-work-tree 2>&1); repo_probe_exit=$?;
             if [ "$repo_probe_exit" -eq 0 ] && [ "$repo_probe_stdout" = true ]; then repo_probe=inside_worktree;
             elif printf '%s\n' "$repo_probe_stdout" | grep -Fq 'not a git repository'; then repo_probe=outside_worktree;
             else repo_probe=unavailable; fi;
           fi;
           sm=$(printf 'status_exit=%s\nrepository_probe=%s\nrepository_probe_exit=%s\nfiles_total=%s\nfiles_returned=%s\nfiles_truncated=%s\nfiles_limit=%s\nstatus_bytes=%s\nstatus_trunc_count=%s\nstatus_trunc_bytes=%s\nstatus_trunc_path=%s\nmodified=%s\nadded=%s\ndeleted=%s\nrenamed=%s\ncopied=%s\nuntracked=%s\nconflicted=%s\nstaged=%s\nunstaged=%s' \
             "$exit_raw" "$repo_probe" "$repo_probe_exit" "$total" "$n" "$trunc" __STATUS_LIMIT__ "$status_bytes" "$trunc_count" "$trunc_bytes" "$trunc_path" \
             "$c_mod" "$c_add" "$c_del" "$c_ren" "$c_cop" "$c_unt" "$c_conf" "$c_stg" "$c_uns");
           printf '%s\n' "$sm"; printf 'WCSF1:S:%010d:%010d\n' "$status_wire_bytes" "$(( ${#sm}+1 ))";
           head_exit_raw=; he=0; head_frame_bytes=0; head_commit=; head_short=; head_subject=;
           git log -1 --format=%s >/dev/null 2>&1; head_exit_raw=$?;
           if [ "$head_exit_raw" -eq 0 ] 2>/dev/null; then
             head_commit=$(git rev-parse --verify HEAD 2>/dev/null); head_short=$(git rev-parse --short "$head_commit" 2>/dev/null);
             if [ -n "$head_commit" ] && [ -n "$head_short" ]; then
               head_prefix=$(printf 'commit=%s\nshort=%s\nsummary=' "$head_commit" "$head_short"); head_prefix_bytes=${#head_prefix};
               head_subject_limit=$((__HEAD_BYTE_BUDGET__-head_prefix_bytes));
               if [ "$head_subject_limit" -lt 0 ]; then he=1;
               else
                 head_subject=$(git log -1 --format=%s "$head_commit" 2>/dev/null | dd bs=1 count=$((head_subject_limit+1)) 2>/dev/null);
                 git log -1 --format=%s "$head_commit" >/dev/null 2>&1; head_exit_raw=$?; head_subject_bytes=${#head_subject};
                 if [ "$head_subject_bytes" -gt "$head_subject_limit" ]; then he=1;
                 elif [ "$head_exit_raw" -eq 0 ] 2>/dev/null; then printf '%s%s' "$head_prefix" "$head_subject"; head_frame_bytes=$((head_prefix_bytes+head_subject_bytes)); fi;
               fi;
             fi;
           fi;
           hm=$(printf 'head_exit=%s\nhead_truncated=%s\nhead_bytes=%s' "$head_exit_raw" "$he" "$head_frame_bytes");
           printf '%s\n' "$hm"; printf 'WCSF1:H:%010d:%010d\n' "$head_frame_bytes" "$(( ${#hm}+1 ))";
           { git diff --stat 2>/dev/null; printf '__WEBCODEX_STAT_EXIT__=%s\n' "$?"; } | {
             sb=0; se=0; stat_exit_raw=; have=0; pending=;
             while IFS= read -r sline; do
               next=$sline; sline=$pending; pending=$next;
               if [ "$have" = 0 ]; then have=1; continue; fi;
               ll=$((${#sline}+1));
               if [ "$((sb + ll))" -gt __DIFF_STAT_BUDGET__ ]; then se=1; else printf '%s\n' "$sline"; sb=$((sb+ll)); fi;
             done;
             case "$pending" in __WEBCODEX_STAT_EXIT__=*) stat_exit_raw=${pending#__WEBCODEX_STAT_EXIT__=} ;; *) stat_exit_raw= ;; esac;
             stat_wire_bytes=$sb; if [ "$sb" -gt 0 ]; then sb=$((sb-1)); fi;
             tm=$(printf 'diff_stat_exit=%s\ndiff_stat_truncated=%s\ndiff_stat_bytes=%s' "$stat_exit_raw" "$se" "$sb");
             printf '%s\n' "$tm"; printf 'WCSF1:T:%010d:%010d\n' "$stat_wire_bytes" "$(( ${#tm}+1 ))";
           };
           nb_base=HEAD;
           if ! git rev-parse --verify HEAD >/dev/null 2>&1; then nb_base=$(printf '' | git hash-object -t tree --stdin 2>/dev/null); fi;
           { git -c core.quotePath=false diff --no-ext-diff --no-textconv --numstat --no-renames "$nb_base" -- 2>/dev/null; printf '__WEBCODEX_NUMSTAT_EXIT__=%s\n' "$?"; } | {
             nb=0; ne=0; numstat_exit_raw=; have=0; pending=;
             while IFS= read -r nline; do
               next=$nline; nline=$pending; pending=$next;
               if [ "$have" = 0 ]; then have=1; continue; fi;
               ll=$((${#nline}+1));
               if [ "$((nb + ll))" -gt __NUMSTAT_BYTE_BUDGET__ ]; then ne=1; else printf '%s\n' "$nline"; nb=$((nb+ll)); fi;
             done;
             case "$pending" in __WEBCODEX_NUMSTAT_EXIT__=*) numstat_exit_raw=${pending#__WEBCODEX_NUMSTAT_EXIT__=} ;; *) numstat_exit_raw= ;; esac;
             numstat_wire_bytes=$nb; if [ "$nb" -gt 0 ]; then nb=$((nb-1)); fi;
             nm=$(printf 'numstat_exit=%s\nnumstat_truncated=%s\nnumstat_bytes=%s' "$numstat_exit_raw" "$ne" "$nb");
             printf '%s\n' "$nm"; printf 'WCSF1:N:%010d:%010d\n' "$numstat_wire_bytes" "$(( ${#nm}+1 ))";
           }__DIFF_PART__;
           final_exit=$?;
           if [ "$exit_raw" -ne 0 ] && [ "$exit_raw" -ge 0 ] 2>/dev/null; then exit "$exit_raw"; else exit "$final_exit"; fi;
         }; }"#;

    let command = script
        .replace("__STATUS_LIMIT__", &status_files_limit.to_string())
        .replace(
            "__STATUS_BYTE_BUDGET__",
            &SHOW_CHANGES_STATUS_BYTES.to_string(),
        )
        .replace("__STATUS_MAX_RECORD_BYTES__", &(4 * 1024).to_string())
        .replace(
            "__DIFF_STAT_BUDGET__",
            &SHOW_CHANGES_DIFF_STAT_BYTES.to_string(),
        )
        .replace(
            "__NUMSTAT_BYTE_BUDGET__",
            &SHOW_CHANGES_NUMSTAT_BYTES.to_string(),
        )
        .replace("__HEAD_BYTE_BUDGET__", &SHOW_CHANGES_HEAD_BYTES.to_string())
        .replace("__DIFF_PART__", &diff_part);
    command
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parsed, production-bounded `show_changes` stdout frames. The status result
/// frame carries the authoritative `files_*` and per-category counts reported
/// by the command's streaming loop, so totals stay exact even when the
/// returned file records were truncated by the production-side limit.
#[derive(Debug, Clone, Default)]
pub(crate) struct ShowChangesStdout {
    pub(crate) framing_valid: bool,
    pub(crate) status: String,
    pub(crate) status_result: String,
    pub(crate) head: String,
    pub(crate) stat: String,
    pub(crate) numstat: String,
    pub(crate) diff: String,
    /// Authoritative status metadata parsed from the current bounded frame.
    /// `None` means the frame was absent or invalid.
    pub(crate) files_total: Option<usize>,
    pub(crate) files_returned: Option<usize>,
    pub(crate) files_truncated: Option<bool>,
    pub(crate) files_limit: Option<usize>,
    pub(crate) counts_modified: Option<usize>,
    pub(crate) counts_added: Option<usize>,
    pub(crate) counts_deleted: Option<usize>,
    pub(crate) counts_renamed: Option<usize>,
    pub(crate) counts_copied: Option<usize>,
    pub(crate) counts_untracked: Option<usize>,
    pub(crate) counts_conflicted: Option<usize>,
    pub(crate) counts_staged: Option<usize>,
    pub(crate) counts_unstaged: Option<usize>,
    /// Per-segment byte-budget truncation flags parsed from the status result
    /// frame. `None` when the current frame is absent or invalid.
    pub(crate) status_trunc_count: Option<bool>,
    pub(crate) status_trunc_bytes: Option<bool>,
    pub(crate) status_trunc_path: Option<bool>,
    /// Exact bytes in the parsed status frame.
    pub(crate) status_bytes: Option<usize>,
    /// HEAD metadata frame: real `git log -1` exit code and whether HEAD data
    /// was emitted. `None` when the frame is absent.
    pub(crate) head_exit: Option<i32>,
    /// Whether the HEAD metadata segment was dropped for overflowing its byte
    /// budget (a pathological commit subject). `None` when the frame is absent.
    pub(crate) head_truncated: Option<bool>,
    /// Exact bytes in the parsed HEAD frame.
    pub(crate) head_bytes: Option<usize>,
    /// `git diff --stat` metadata frame. `None` when the frame is absent.
    pub(crate) diff_stat_exit: Option<i32>,
    pub(crate) diff_stat_truncated: Option<bool>,
    /// Exact bytes in the parsed diff-stat frame.
    pub(crate) diff_stat_bytes: Option<usize>,
    /// Bounded per-file numstat frame used to enrich status file records.
    pub(crate) numstat_exit: Option<i32>,
    pub(crate) numstat_truncated: Option<bool>,
    pub(crate) numstat_bytes: Option<usize>,
    /// Number of diff hunks the production-side loop returned, parsed from the
    /// trailing diff metadata frame. `None` when `include_diff` is false or the
    /// frame is absent.
    pub(crate) diff_hunks_returned: Option<usize>,
    /// Whether the production-side loop dropped any diff data.
    pub(crate) diff_hunks_truncated: Option<bool>,
    /// Independent production-side diff truncation flags.
    pub(crate) diff_trunc_hunk_count: Option<bool>,
    pub(crate) diff_trunc_hunk_lines: Option<bool>,
    pub(crate) diff_trunc_bytes: Option<bool>,
    /// Whether the diff byte budget fired while emitting the body of an
    /// already-returned hunk. This distinguishes omitted current-hunk lines
    /// from a page boundary that only omitted later file/hunk records.
    pub(crate) diff_trunc_bytes_in_hunk: Option<bool>,
    /// Real full `git diff` exit code parsed from the diff metadata frame.
    pub(crate) diff_exit: Option<i32>,
    /// Bytes emitted in the bounded diff segment, parsed from the diff metadata
    /// frame.
    pub(crate) diff_bytes: Option<usize>,
}

fn parse_show_changes_wire_block(
    stdout: &str,
    end: usize,
    kind: u8,
) -> Option<(&str, &str, usize)> {
    let bytes = stdout.as_bytes();
    let trailer_start = end.checked_sub(SHOW_CHANGES_BLOCK_TRAILER_BYTES)?;
    let trailer = bytes.get(trailer_start..end)?;
    if trailer.get(..6)? != SHOW_CHANGES_BLOCK_MAGIC
        || trailer.get(6).copied()? != kind
        || trailer.get(7).copied()? != b':'
        || trailer.get(18).copied()? != b':'
        || trailer.get(29).copied()? != b'\n'
    {
        return None;
    }
    let data_wire_bytes = parse_fixed_decimal(trailer.get(8..18)?)?;
    let meta_wire_bytes = parse_fixed_decimal(trailer.get(19..29)?)?;
    let meta_start = trailer_start.checked_sub(meta_wire_bytes)?;
    let data_start = meta_start.checked_sub(data_wire_bytes)?;
    let data = std::str::from_utf8(bytes.get(data_start..meta_start)?).ok()?;
    let meta = std::str::from_utf8(bytes.get(meta_start..trailer_start)?).ok()?;
    Some((data, meta, data_start))
}

#[cfg(test)]
pub(crate) fn framed_show_changes_test_block(kind: char, body: &str, metadata: &str) -> String {
    format!(
        "{body}{metadata}WCSF1:{kind}:{:010}:{:010}\n",
        body.len(),
        metadata.len()
    )
}

#[cfg(test)]
pub(crate) fn framed_clean_show_changes_test_stdout(subject: &str, include_diff: bool) -> String {
    let head = format!("commit=abc123\nshort=abc123\nsummary={subject}\n");
    let head_bytes = head.strip_suffix('\n').unwrap_or(&head).len();
    let mut stdout = format!(
        "{}{}{}{}",
        framed_show_changes_test_block(
            'S',
            "## main\n",
            "status_exit=0\nrepository_probe=inside_worktree\nrepository_probe_exit=0\nfiles_total=0\nfiles_returned=0\nfiles_truncated=0\nfiles_limit=200\nmodified=0\nadded=0\ndeleted=0\nrenamed=0\ncopied=0\nuntracked=0\nconflicted=0\nstaged=0\nunstaged=0\nstatus_trunc_count=0\nstatus_trunc_bytes=0\nstatus_trunc_path=0\nstatus_bytes=7\n"
        ),
        framed_show_changes_test_block(
            'H',
            &head,
            &format!("head_exit=0\nhead_truncated=0\nhead_bytes={head_bytes}\n")
        ),
        framed_show_changes_test_block(
            'T',
            "",
            "diff_stat_exit=0\ndiff_stat_truncated=0\ndiff_stat_bytes=0\n"
        ),
        framed_show_changes_test_block(
            'N',
            "",
            "numstat_exit=0\nnumstat_truncated=0\nnumstat_bytes=0\n"
        )
    );
    if include_diff {
        stdout.push_str(&framed_show_changes_test_block(
            'D',
            "",
            "diff_exit=0\ndiff_hunks_returned=0\ndiff_hunks_truncated=0\ndiff_trunc_hunk_count=0\ndiff_trunc_hunk_lines=0\ndiff_trunc_bytes=0\ndiff_trunc_bytes_in_hunk=0\ndiff_bytes=0\n",
        ));
    }
    stdout
}

fn parse_framed_show_changes_stdout(stdout: &str, include_diff: bool) -> Option<ShowChangesStdout> {
    let mut cursor = stdout.len();
    let (diff, diff_meta) = if include_diff {
        let (body, meta, start) = parse_show_changes_wire_block(stdout, cursor, b'D')?;
        cursor = start;
        (strip_wire_lf(body)?, strip_wire_lf(meta)?)
    } else {
        (String::new(), String::new())
    };
    let (numstat, numstat_meta, start) = parse_show_changes_wire_block(stdout, cursor, b'N')?;
    cursor = start;
    let (stat, stat_meta, start) = parse_show_changes_wire_block(stdout, cursor, b'T')?;
    cursor = start;
    let (head, head_meta, start) = parse_show_changes_wire_block(stdout, cursor, b'H')?;
    cursor = start;
    let (status, status_result, start) = parse_show_changes_wire_block(stdout, cursor, b'S')?;
    if start != 0 {
        return None;
    }
    let status = strip_wire_lf(status)?;
    let status_result = strip_wire_lf(status_result)?;
    let stat = strip_wire_lf(stat)?;
    let stat_meta = strip_wire_lf(stat_meta)?;
    let numstat = strip_wire_lf(numstat)?;
    let numstat_meta = strip_wire_lf(numstat_meta)?;
    let head_meta = strip_wire_lf(head_meta)?;

    Some(ShowChangesStdout {
        framing_valid: true,
        files_total: parse_optional_usize(&status_result, "files_total"),
        files_returned: parse_optional_usize(&status_result, "files_returned"),
        files_truncated: parse_optional_bool(&status_result, "files_truncated"),
        files_limit: parse_optional_usize(&status_result, "files_limit"),
        counts_modified: parse_optional_usize(&status_result, "modified"),
        counts_added: parse_optional_usize(&status_result, "added"),
        counts_deleted: parse_optional_usize(&status_result, "deleted"),
        counts_renamed: parse_optional_usize(&status_result, "renamed"),
        counts_copied: parse_optional_usize(&status_result, "copied"),
        counts_untracked: parse_optional_usize(&status_result, "untracked"),
        counts_conflicted: parse_optional_usize(&status_result, "conflicted"),
        counts_staged: parse_optional_usize(&status_result, "staged"),
        counts_unstaged: parse_optional_usize(&status_result, "unstaged"),
        status_trunc_count: parse_optional_bool(&status_result, "status_trunc_count"),
        status_trunc_bytes: parse_optional_bool(&status_result, "status_trunc_bytes"),
        status_trunc_path: parse_optional_bool(&status_result, "status_trunc_path"),
        status_bytes: parse_optional_usize(&status_result, "status_bytes"),
        head_exit: parse_status_result_field(&head_meta, "head_exit")
            .and_then(|value| value.parse().ok()),
        head_truncated: parse_optional_bool(&head_meta, "head_truncated"),
        head_bytes: parse_optional_usize(&head_meta, "head_bytes"),
        diff_stat_exit: parse_status_result_field(&stat_meta, "diff_stat_exit")
            .and_then(|value| value.parse().ok()),
        diff_stat_truncated: parse_optional_bool(&stat_meta, "diff_stat_truncated"),
        diff_stat_bytes: parse_optional_usize(&stat_meta, "diff_stat_bytes"),
        numstat_exit: parse_status_result_field(&numstat_meta, "numstat_exit")
            .and_then(|value| value.parse().ok()),
        numstat_truncated: parse_optional_bool(&numstat_meta, "numstat_truncated"),
        numstat_bytes: parse_optional_usize(&numstat_meta, "numstat_bytes"),
        diff_hunks_returned: parse_optional_usize(&diff_meta, "diff_hunks_returned"),
        diff_hunks_truncated: parse_optional_bool(&diff_meta, "diff_hunks_truncated"),
        diff_trunc_hunk_count: parse_optional_bool(&diff_meta, "diff_trunc_hunk_count"),
        diff_trunc_hunk_lines: parse_optional_bool(&diff_meta, "diff_trunc_hunk_lines"),
        diff_trunc_bytes: parse_optional_bool(&diff_meta, "diff_trunc_bytes"),
        diff_trunc_bytes_in_hunk: parse_optional_bool(&diff_meta, "diff_trunc_bytes_in_hunk"),
        diff_exit: parse_status_result_field(&diff_meta, "diff_exit")
            .and_then(|value| value.parse().ok()),
        diff_bytes: parse_optional_usize(&diff_meta, "diff_bytes"),
        status,
        status_result,
        head: head.to_string(),
        stat,
        numstat,
        diff,
    })
}

pub(crate) fn split_show_changes_stdout(stdout: &str, include_diff: bool) -> ShowChangesStdout {
    parse_framed_show_changes_stdout(stdout, include_diff).unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedStatusHeader {
    branch: Option<String>,
    upstream_status: &'static str,
    upstream_reason_code: Option<&'static str>,
    upstream: Option<String>,
    ahead: Option<u64>,
    behind: Option<u64>,
}

fn parsed_branch_name(raw: &str) -> Option<String> {
    let branch = raw.trim().trim_matches('"');
    if branch.is_empty() {
        None
    } else {
        Some(branch.to_string())
    }
}

pub(crate) fn parse_status_header(line: &str) -> Option<ParsedStatusHeader> {
    let rest = line.strip_prefix("## ")?;

    for prefix in ["No commits yet on ", "Initial commit on "] {
        if let Some(branch) = rest.strip_prefix(prefix) {
            return Some(ParsedStatusHeader {
                branch: parsed_branch_name(branch),
                upstream_status: "absent",
                upstream_reason_code: None,
                upstream: None,
                ahead: None,
                behind: None,
            });
        }
    }

    if rest == "HEAD" || rest == "HEAD (no branch)" || rest.starts_with("HEAD (detached ") {
        return Some(ParsedStatusHeader {
            branch: None,
            upstream_status: "absent",
            upstream_reason_code: None,
            upstream: None,
            ahead: None,
            behind: None,
        });
    }

    let Some((local, tracking)) = rest.split_once("...") else {
        return Some(ParsedStatusHeader {
            branch: parsed_branch_name(rest.split(" [").next().unwrap_or(rest)),
            upstream_status: "absent",
            upstream_reason_code: None,
            upstream: None,
            ahead: None,
            behind: None,
        });
    };

    let (upstream, counts) = tracking
        .split_once(" [")
        .map_or((tracking, None), |(upstream, counts)| {
            (upstream, Some(counts))
        });
    let upstream = parsed_branch_name(upstream);
    let mut ahead = None;
    let mut behind = None;
    let mut gone = false;
    if let Some(counts) = counts {
        for item in counts.trim_end_matches(']').split(',') {
            let item = item.trim();
            if item == "gone" {
                gone = true;
            } else if let Some(value) = item.strip_prefix("ahead ") {
                ahead = value.parse().ok();
            } else if let Some(value) = item.strip_prefix("behind ") {
                behind = value.parse().ok();
            }
        }
    }

    if gone {
        return Some(ParsedStatusHeader {
            branch: parsed_branch_name(local),
            upstream_status: "gone",
            upstream_reason_code: Some("upstream_gone"),
            upstream,
            ahead: None,
            behind: None,
        });
    }

    if upstream.is_some() {
        ahead.get_or_insert(0);
        behind.get_or_insert(0);
    }
    Some(ParsedStatusHeader {
        branch: parsed_branch_name(local),
        upstream_status: "available",
        upstream_reason_code: None,
        upstream,
        ahead,
        behind,
    })
}

fn parse_show_changes_head_frame(head: &str) -> Option<(&str, &str, &str)> {
    let mut lines = head.split('\n');
    let commit = lines.next()?.strip_prefix("commit=")?;
    let short = lines.next()?.strip_prefix("short=")?;
    let summary = lines.next()?.strip_prefix("summary=")?;
    if commit.is_empty() || short.is_empty() || lines.next().is_some() {
        return None;
    }
    Some((commit, short, summary))
}

fn parse_show_changes_head(head: &str) -> serde_json::Value {
    if let Some((commit, short, summary)) = parse_show_changes_head_frame(head) {
        return json!({
            "commit": commit,
            "short": short,
            "summary": summary,
        });
    }
    json!({
        "commit": null,
        "short": null,
        "summary": null,
    })
}

fn frame_bytes_match(value: &str, reported: Option<usize>, budget: usize) -> bool {
    let actual = value.len();
    actual <= budget && reported == Some(actual)
}

fn show_changes_transport_safe(
    frames: &ShowChangesStdout,
    include_diff: bool,
    max_hunks: usize,
) -> bool {
    if !frames.framing_valid {
        return false;
    }
    let status_exit = parse_status_result_field(&frames.status_result, "status_exit")
        .and_then(|value| value.parse::<i32>().ok());
    let repository_probe = parse_status_result_field(&frames.status_result, "repository_probe");
    let repository_probe_exit =
        parse_status_result_field(&frames.status_result, "repository_probe_exit")
            .and_then(|value| value.parse::<i32>().ok());
    let (
        Some(_),
        Some("inside_worktree" | "outside_worktree" | "unavailable"),
        Some(_),
        Some(files_total),
        Some(files_returned),
        Some(files_truncated),
        Some(files_limit),
        Some(status_trunc_count),
        Some(status_trunc_bytes),
        Some(status_trunc_path),
    ) = (
        status_exit,
        repository_probe,
        repository_probe_exit,
        frames.files_total,
        frames.files_returned,
        frames.files_truncated,
        frames.files_limit,
        frames.status_trunc_count,
        frames.status_trunc_bytes,
        frames.status_trunc_path,
    )
    else {
        return false;
    };

    let category_total = [
        frames.counts_modified,
        frames.counts_added,
        frames.counts_deleted,
        frames.counts_renamed,
        frames.counts_copied,
        frames.counts_untracked,
        frames.counts_conflicted,
    ]
    .into_iter()
    .try_fold(0usize, |sum, value| sum.checked_add(value?));
    let status_records = frames
        .status
        .lines()
        .filter(|line| !line.starts_with("## "))
        .count();
    let status_truncated_by_reason = status_trunc_count || status_trunc_bytes || status_trunc_path;
    let status_valid = frame_bytes_match(
        &frames.status,
        frames.status_bytes,
        SHOW_CHANGES_STATUS_BYTES,
    ) && frames.counts_staged.is_some()
        && frames.counts_unstaged.is_some()
        && files_limit == SHOW_CHANGES_MAX_STATUS_FILES
        && files_returned == status_records
        && files_total >= files_returned
        && files_truncated == (files_total > files_returned)
        && files_truncated == status_truncated_by_reason
        && category_total == Some(files_total);

    let (Some(head_exit), Some(head_truncated)) = (frames.head_exit, frames.head_truncated) else {
        return false;
    };
    let head_shape_valid = if head_truncated {
        frames.head.is_empty() && frames.head_bytes == Some(0)
    } else if head_exit == 0 {
        parse_show_changes_head_frame(&frames.head).is_some()
    } else {
        frames.head.is_empty() && frames.head_bytes == Some(0)
    };
    let head_valid = frame_bytes_match(&frames.head, frames.head_bytes, SHOW_CHANGES_HEAD_BYTES)
        && head_shape_valid;

    let stat_valid = frames.diff_stat_exit.is_some()
        && frames.diff_stat_truncated.is_some()
        && frame_bytes_match(
            &frames.stat,
            frames.diff_stat_bytes,
            SHOW_CHANGES_DIFF_STAT_BYTES,
        );

    let numstat_valid = frames.numstat_exit.is_some()
        && frames.numstat_truncated.is_some()
        && frame_bytes_match(
            &frames.numstat,
            frames.numstat_bytes,
            SHOW_CHANGES_NUMSTAT_BYTES,
        );

    let diff_valid = if include_diff {
        let (
            Some(_),
            Some(hunks_returned),
            Some(hunks_truncated),
            Some(trunc_count),
            Some(trunc_lines),
            Some(trunc_bytes),
            Some(trunc_bytes_in_hunk),
        ) = (
            frames.diff_exit,
            frames.diff_hunks_returned,
            frames.diff_hunks_truncated,
            frames.diff_trunc_hunk_count,
            frames.diff_trunc_hunk_lines,
            frames.diff_trunc_bytes,
            frames.diff_trunc_bytes_in_hunk,
        )
        else {
            return false;
        };
        let actual_hunks = frames
            .diff
            .lines()
            .filter(|line| line.starts_with("@@ "))
            .count();
        frame_bytes_match(&frames.diff, frames.diff_bytes, SHOW_CHANGES_DIFF_BYTES)
            && hunks_returned == actual_hunks
            && hunks_returned <= max_hunks
            && hunks_truncated == (trunc_count || trunc_lines || trunc_bytes)
            && (!trunc_bytes_in_hunk || trunc_bytes)
    } else {
        true
    };

    status_valid && head_valid && stat_valid && numstat_valid && diff_valid
}

fn porcelain_path(path_part: &str) -> (String, Option<String>) {
    let path_part = path_part.trim().trim_matches('"');
    if let Some((old, new)) = path_part.split_once(" -> ") {
        (
            new.trim().trim_matches('"').to_string(),
            Some(old.trim().trim_matches('"').to_string()),
        )
    } else {
        (path_part.to_string(), None)
    }
}

fn is_unmerged_status(x: char, y: char) -> bool {
    // Porcelain v1 unmerged/conflict pairs (UU, AA, DD, AU, UA, DU, UD, ...).
    x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D')
}

fn status_label(x: char, y: char) -> &'static str {
    if x == '?' && y == '?' {
        return "untracked";
    }
    if is_unmerged_status(x, y) {
        return "conflicted";
    }
    if x == 'R' || y == 'R' {
        "renamed"
    } else if x == 'C' || y == 'C' {
        "copied"
    } else if x == 'D' || y == 'D' {
        "deleted"
    } else if x == 'A' || y == 'A' {
        "added"
    } else {
        "modified"
    }
}

fn looks_like_smoke_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("smoke")
        || lower.contains("tmp")
        || lower.contains("test")
        || lower.contains("anchor")
}

#[cfg(test)]
pub(crate) fn parse_show_changes_output(
    project: &str,
    status_stdout: &str,
    head_stdout: &str,
    diff_stat: &str,
    diff_stdout: Option<&str>,
    max_hunks: usize,
    max_hunk_lines: usize,
    exit_code: Option<i32>,
    stderr: &str,
) -> serde_json::Value {
    let has_reliable_header = status_stdout
        .lines()
        .any(|line| parse_status_header(line).is_some());
    let observation = if exit_code == Some(0) && has_reliable_header {
        ShowChangesStatusObservation::observed(exit_code)
    } else if exit_code == Some(0) {
        ShowChangesStatusObservation {
            kind: ShowChangesStatusObservationKind::OutputUnavailable,
            reason_code: Some("git_status_header_unavailable"),
            exit_code,
            repository_probe: "inside_worktree",
            repository_probe_exit_code: Some(0),
            git_available: true,
        }
    } else {
        ShowChangesStatusObservation {
            kind: ShowChangesStatusObservationKind::CommandFailed,
            reason_code: Some("git_status_command_failed"),
            exit_code,
            repository_probe: if has_reliable_header {
                "inside_worktree"
            } else {
                "unavailable"
            },
            repository_probe_exit_code: None,
            git_available: has_reliable_header,
        }
    };
    let frames = ShowChangesStdout {
        status: status_stdout.to_string(),
        head: head_stdout.to_string(),
        stat: diff_stat.to_string(),
        diff: diff_stdout.unwrap_or_default().to_string(),
        ..ShowChangesStdout::default()
    };
    let diff_for_parse = if diff_stdout.is_some() {
        Some(frames.diff.as_str())
    } else {
        None
    };
    parse_show_changes_output_with_observation(
        project,
        &frames.status,
        &frames.head,
        &frames.stat,
        diff_for_parse,
        max_hunks,
        max_hunk_lines,
        exit_code,
        stderr,
        observation,
        &frames,
    )
}

/// Map a full `git diff` exit code to the structured `diff_status` object
/// reported by `show_changes`. `observed` means the diff exit code was
/// captured (0 = clean diff, non-zero = command failure); `command_failed`
/// means a non-zero exit was observed; `output_unavailable` means the exit
/// code could not be captured by the production-side loop.
fn diff_status_json(diff_exit: Option<i32>) -> serde_json::Value {
    match diff_exit {
        Some(0) => json!({"status": "observed", "exit_code": 0}),
        Some(code) => json!({"status": "command_failed", "exit_code": code}),
        None => json!({"status": "output_unavailable", "exit_code": null}),
    }
}

/// Map the independently captured `git diff --stat` exit code to a strict
/// observation object. Unlike `transport_safe`, this status describes whether
/// the inspection itself succeeded and therefore participates in ToolResult
/// success for confirmed Git worktrees.
fn diff_stat_status_json(diff_stat_exit: Option<i32>) -> serde_json::Value {
    match diff_stat_exit {
        Some(0) => json!({
            "status": "observed", "exit_code": 0, "reason_code": null,
        }),
        Some(code) => json!({
            "status": "command_failed", "exit_code": code,
            "reason_code": "git_diff_stat_command_failed",
        }),
        None => json!({
            "status": "output_unavailable", "exit_code": null,
            "reason_code": "git_diff_stat_result_unavailable",
        }),
    }
}

fn show_changes_numstat_by_path(numstat: &str) -> BTreeMap<String, (u64, u64)> {
    let mut stats = BTreeMap::new();
    for line in numstat.lines() {
        let mut fields = line.strip_suffix('\r').unwrap_or(line).splitn(3, '\t');
        let (Some(additions), Some(deletions), Some(raw_path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let (Ok(additions), Ok(deletions)) = (additions.parse::<u64>(), deletions.parse::<u64>())
        else {
            continue;
        };
        let Some(path) = decode_git_quoted_path(raw_path) else {
            continue;
        };
        stats
            .entry(path)
            .and_modify(|entry: &mut (u64, u64)| {
                entry.0 = entry.0.saturating_add(additions);
                entry.1 = entry.1.saturating_add(deletions);
            })
            .or_insert((additions, deletions));
    }
    stats
}

fn enrich_show_changes_files_with_numstat(files: &mut [Value], frames: &ShowChangesStdout) {
    if frames.numstat_exit != Some(0) {
        return;
    }
    let stats = show_changes_numstat_by_path(&frames.numstat);
    for file in files {
        let Some(file) = file.as_object_mut() else {
            continue;
        };
        // The producer intentionally uses `--no-renames` to keep the bounded
        // numstat framing path-stable and simple. In that mode Git represents a
        // rename/copy as full old-path deletion plus full new-path addition, so
        // summing both paths would manufacture misleading line churn. Leave
        // those stats unknown until a rename-aware producer is available.
        if matches!(
            file.get("status").and_then(Value::as_str),
            Some("renamed" | "copied")
        ) {
            continue;
        }
        let path = file.get("path").and_then(Value::as_str);
        let old_path = file.get("old_path").and_then(Value::as_str);
        let mut additions = 0u64;
        let mut deletions = 0u64;
        let mut observed = false;
        for candidate in [old_path, path].into_iter().flatten() {
            if old_path == path && Some(candidate) == path && observed {
                continue;
            }
            if let Some((file_additions, file_deletions)) = stats.get(candidate) {
                additions = additions.saturating_add(*file_additions);
                deletions = deletions.saturating_add(*file_deletions);
                observed = true;
            }
        }
        if observed {
            file.insert("additions".to_string(), Value::from(additions));
            file.insert("deletions".to_string(), Value::from(deletions));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_show_changes_output_with_observation(
    project: &str,
    status_stdout: &str,
    head_stdout: &str,
    diff_stat: &str,
    diff_stdout: Option<&str>,
    max_hunks: usize,
    max_hunk_lines: usize,
    exit_code: Option<i32>,
    stderr: &str,
    observation: ShowChangesStatusObservation,
    frames: &ShowChangesStdout,
) -> serde_json::Value {
    let mut branch = None;
    let mut upstream_status = "unobserved";
    let mut upstream_reason_code = observation.reason_code.or(Some("git_status_unobserved"));
    let mut upstream = None;
    let mut ahead = None;
    let mut behind = None;
    let mut files = Vec::new();
    let mut modified = 0usize;
    let mut added = 0usize;
    let mut deleted = 0usize;
    let mut renamed = 0usize;
    let mut copied = 0usize;
    let mut untracked = 0usize;
    let mut conflicted = 0usize;
    let mut staged_count = 0usize;
    let mut unstaged_count = 0usize;
    let status_observed = observation.status_observed();
    let observation_reason_code = observation.reason_code;

    if status_observed {
        for line in status_stdout.lines() {
            if let Some(parsed) = parse_status_header(line) {
                branch = parsed.branch;
                upstream_status = parsed.upstream_status;
                upstream_reason_code = parsed.upstream_reason_code;
                upstream = parsed.upstream;
                ahead = parsed.ahead;
                behind = parsed.behind;
                continue;
            }
            if line.len() < 3 {
                continue;
            }
            let mut chars = line.chars();
            let x = chars.next().unwrap_or(' ');
            let y = chars.next().unwrap_or(' ');
            if x == '!' && y == '!' {
                continue;
            }
            let path_part = line.get(3..).unwrap_or_default();
            let (path, old_path) = porcelain_path(path_part);
            if path.is_empty() {
                continue;
            }
            let status = status_label(x, y);
            let is_untracked = status == "untracked";
            let is_conflicted = status == "conflicted";
            let staged = !is_untracked && !is_conflicted && x != ' ' && x != '?';
            let unstaged = !is_untracked && !is_conflicted && y != ' ' && y != '?';
            match status {
                "modified" => modified += 1,
                "added" => added += 1,
                "deleted" => deleted += 1,
                "renamed" => renamed += 1,
                "copied" => copied += 1,
                "untracked" => untracked += 1,
                "conflicted" => conflicted += 1,
                _ => {}
            }
            if staged {
                staged_count += 1;
            }
            if unstaged {
                unstaged_count += 1;
            }
            let mut file = serde_json::Map::new();
            file.insert("path".to_string(), json!(path));
            file.insert("status".to_string(), json!(status));
            file.insert("staged".to_string(), json!(staged));
            file.insert("unstaged".to_string(), json!(unstaged));
            file.insert(
                "kind".to_string(),
                json!(if is_untracked {
                    "untracked"
                } else if is_conflicted {
                    "conflicted"
                } else {
                    "tracked"
                }),
            );
            if let Some(old_path) = old_path {
                file.insert("old_path".to_string(), json!(old_path));
            }
            files.push(json!(file));
        }
    }

    // When the production-side status loop truncated the returned file
    // records, the per-category counts parsed above only cover the returned
    // subset. The command's streaming loop counted and classified *every*
    // entry, so prefer those authoritative counts whenever they are present.
    // `files_truncated` is true when the production-side limit dropped records
    // by either the count cap *or* the independent status byte budget; the
    // exact totals stay authoritative either way.
    let status_trunc_count = frames.status_trunc_count.unwrap_or(false);
    let status_trunc_bytes = frames.status_trunc_bytes.unwrap_or(false);
    let status_trunc_path = frames.status_trunc_path.unwrap_or(false);
    let files_truncated =
        frames.files_truncated.unwrap_or(false) || status_trunc_count || status_trunc_bytes;
    let files_total = frames.files_total;
    let files_returned = frames.files_returned.unwrap_or(files.len());
    let files_limit = frames.files_limit.unwrap_or(SHOW_CHANGES_MAX_STATUS_FILES);
    if files_truncated {
        if let Some(value) = frames.counts_modified {
            modified = value;
        }
        if let Some(value) = frames.counts_added {
            added = value;
        }
        if let Some(value) = frames.counts_deleted {
            deleted = value;
        }
        if let Some(value) = frames.counts_renamed {
            renamed = value;
        }
        if let Some(value) = frames.counts_copied {
            copied = value;
        }
        if let Some(value) = frames.counts_untracked {
            untracked = value;
        }
        if let Some(value) = frames.counts_conflicted {
            conflicted = value;
        }
        if let Some(value) = frames.counts_staged {
            staged_count = value;
        }
        if let Some(value) = frames.counts_unstaged {
            unstaged_count = value;
        }
    }

    enrich_show_changes_files_with_numstat(&mut files, frames);

    let clean = status_observed.then_some(files_total.map_or(files.is_empty(), |total| total == 0));
    let mut warnings = Vec::new();
    if !status_observed {
        let reason_code = observation_reason_code.unwrap_or("git_status_result_unavailable");
        let message = match observation.kind {
            ShowChangesStatusObservationKind::CommandFailed => {
                "git status command failed; worktree state is unavailable"
            }
            ShowChangesStatusObservationKind::OutputUnavailable => {
                "git status did not produce a reliable porcelain branch header"
            }
            ShowChangesStatusObservationKind::NonGit => "project is not a git repository",
            ShowChangesStatusObservationKind::Observed => "git status was observed",
        };
        warnings.push(json!({
            "kind": reason_code,
            "reason_code": reason_code,
            "message": message,
        }));
    }
    if upstream_status == "gone" {
        warnings.push(json!({
            "kind": "upstream_gone",
            "reason_code": "upstream_gone",
            "message": "configured upstream tracking branch is gone",
        }));
    }
    if conflicted > 0 {
        warnings.push(json!({
            "kind": "workspace_conflicts",
            "conflicted": conflicted,
            "message": "workspace has unresolved merge/rebase conflicts; inspect carefully and preserve conflict markers until intentionally resolved",
        }));
    }
    for file in &files {
        if file["kind"] != "untracked" {
            continue;
        }
        let path = file["path"].as_str().unwrap_or_default();
        if looks_like_smoke_path(path) {
            warnings.push(json!({
                "kind": "untracked_smoke_file",
                "path": path,
                "message": "untracked smoke/tmp/test/anchor file should be reviewed before commit",
            }));
        } else {
            warnings.push(json!({
                "kind": "untracked_file",
                "path": path,
                "message": "untracked file should be reviewed before commit",
            }));
        }
    }

    let suggested_next_actions = if observation.non_git() {
        vec![
            "git-backed status/diff is not applicable; continue with non-git review evidence"
                .to_string(),
        ]
    } else if status_observed {
        suggested_next_actions_for(
            clean.unwrap_or(false),
            untracked > 0,
            has_smoke_warning(&warnings),
            None,
        )
    } else {
        vec!["inspect git status failure before relying on worktree cleanliness".to_string()]
    };

    // Parse the already production-bounded diff hunks. The parser-local
    // `truncated` flag is consumed internally; show_changes exposes only the
    // authoritative source completeness annotated after producer metadata is
    // evaluated below.
    let (mut diff_hunks, parser_hunk_count, parser_truncated) = match diff_stdout {
        Some(diff) => parse_git_diff_hunks(diff, max_hunks, max_hunk_lines),
        None => (Vec::new(), 0, false),
    };

    // Collect stable truncation reason codes from the per-segment metadata
    // frames the production-side command emits. Each reason corresponds to a
    // concrete production-side bound that fired, never a transport tail marker.
    let mut truncation_reasons: Vec<&'static str> = Vec::new();
    if status_trunc_count {
        truncation_reasons.push("status_file_count_limit");
    }
    if status_trunc_bytes {
        truncation_reasons.push("status_byte_budget");
    }
    if status_trunc_path {
        truncation_reasons.push("status_path_overlong");
    }
    if frames.head_truncated == Some(true) {
        truncation_reasons.push("head_metadata_byte_budget");
    }
    if frames.diff_stat_truncated == Some(true) {
        truncation_reasons.push("diff_stat_byte_budget");
    }
    if frames.numstat_truncated == Some(true) {
        truncation_reasons.push("numstat_byte_budget");
    }
    if frames.diff_trunc_hunk_count == Some(true) {
        truncation_reasons.push("diff_hunk_count_limit");
    }
    if frames.diff_trunc_hunk_lines == Some(true) || parser_truncated {
        truncation_reasons.push("diff_hunk_line_limit");
    }
    if frames.diff_trunc_bytes == Some(true) {
        truncation_reasons.push("diff_byte_budget");
    }
    if frames.diff_trunc_bytes_in_hunk == Some(true) {
        truncation_reasons.push("diff_hunk_byte_budget");
    }
    let output_truncated = !truncation_reasons.is_empty();
    // Every segment must fit its independent production budget and match its
    // exact byte metadata. Missing, malformed, or internally inconsistent
    // metadata cannot prove transport safety.
    let transport_safe = show_changes_transport_safe(frames, diff_stdout.is_some(), max_hunks);
    annotate_show_changes_hunk_source_completeness(
        &mut diff_hunks,
        transport_safe,
        frames.diff_trunc_hunk_lines == Some(true) || frames.diff_trunc_bytes_in_hunk == Some(true),
    );

    let mut output = json!({
        "project": project,
        "git_available": observation.git_available,
        "non_git_project": observation.non_git(),
        "git_error": observation_reason_code.map(|reason| match reason {
            "git_status_config_error" => "git status configuration is invalid",
            "git_status_permission_denied" => "git status permission denied",
            "git_status_command_failed" => "git status command failed",
            "git_status_header_unavailable" => "git status output unavailable",
            "git_status_result_unavailable" => "git status execution result unavailable",
            "not_a_git_repository" => "not a git repository",
            _ => "git status unavailable",
        }),
        "branch": branch,
        "upstream_status": upstream_status,
        "upstream_reason_code": upstream_reason_code,
        "upstream": upstream,
        "ahead": ahead,
        "behind": behind,
        "head": parse_show_changes_head(head_stdout),
        "status_observation": observation.as_json(),
        "clean": clean,
        "counts": {
            "modified": modified,
            "added": added,
            "deleted": deleted,
            "renamed": renamed,
            "copied": copied,
            "untracked": untracked,
            "conflicted": status_observed.then_some(conflicted),
            "staged": staged_count,
            "unstaged": unstaged_count,
        },
        "files": files,
        "files_total": files_total,
        "files_returned": files_returned,
        "files_truncated": files_truncated,
        "files_limit": files_limit,
        "transport_safe": transport_safe,
        "output_budget_bytes": SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "output_truncated": output_truncated,
        "truncation_reasons": truncation_reasons,
        "diff_stat": diff_stat,
        "diff_exit": frames.diff_exit,
        "diff_status": diff_status_json(frames.diff_exit),
        "diff_stat_exit": frames.diff_stat_exit,
        "diff_stat_status": diff_stat_status_json(frames.diff_stat_exit),
        "head_exit": frames.head_exit,
        "warnings": warnings,
        "suggested_next_actions": suggested_next_actions,
        "session": null,
        "exit_code": exit_code,
        "stderr": stderr,
    });

    if diff_stdout.is_some() {
        // The production-side diff loop already bounded the raw diff to at
        // most `max_hunks` hunks and `max_hunk_lines` lines per hunk before
        // transport. Its reported counts are authoritative for aggregate
        // dropped hunks/lines; per-hunk source completeness is carried by the
        // single authoritative `source_completeness` annotation above.
        let hunk_count = frames.diff_hunks_returned.unwrap_or(parser_hunk_count);
        let hunks_truncated =
            frames.diff_hunks_truncated.unwrap_or(parser_truncated) || parser_truncated;
        output["hunks"] = json!(diff_hunks);
        output["hunk_count"] = json!(hunk_count);
        output["hunks_truncated"] = json!(hunks_truncated);
    }

    set_show_changes_verdict(&mut output);
    output
}

fn annotate_show_changes_hunk_source_completeness(
    files: &mut [Value],
    producer_metadata_trustworthy: bool,
    producer_current_hunk_omitted: bool,
) {
    for file in files {
        let Some(hunks) = file.get_mut("hunks").and_then(Value::as_array_mut) else {
            continue;
        };
        for hunk in hunks {
            let Some(hunk) = hunk.as_object_mut() else {
                continue;
            };
            let parser_truncated = hunk
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let source_completeness = if producer_metadata_trustworthy
                && !producer_current_hunk_omitted
                && !parser_truncated
            {
                "complete"
            } else {
                "unknown"
            };
            // show_changes has one authoritative hunk-completeness vocabulary.
            // The parser-local flag remains internal to parsing/git_diff_hunks.
            hunk.remove("truncated");
            hunk.insert(
                "source_completeness".to_string(),
                json!(source_completeness),
            );
        }
    }
}

fn skipped_untracked_preview(path: &str, reason: &str, byte_count: Option<u64>) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("path".to_string(), json!(path));
    object.insert("kind".to_string(), json!("skipped"));
    object.insert("reason".to_string(), json!(reason));
    if let Some(byte_count) = byte_count {
        object.insert("byte_count".to_string(), json!(byte_count));
    }
    Value::Object(object)
}

fn untracked_preview_path_is_invalid(path: &str) -> bool {
    let trimmed = path.trim();
    trimmed.is_empty()
        || trimmed == "."
        || validate_project_relative_path(trimmed).is_err()
        || trimmed.split('/').any(|part| part.is_empty())
}

fn untracked_preview_path_is_sensitive(path: &str) -> bool {
    webcodex_core::sensitive_paths::is_secret_path(path)
        || path
            .replace('\\', "/")
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .map(str::to_ascii_lowercase)
            .any(|part| {
                matches!(
                    part.as_str(),
                    "target" | "node_modules" | "id_rsa" | "id_ed25519"
                )
            })
}

fn untracked_preview_from_bytes(
    path: &str,
    data: &[u8],
    declared_byte_count: Option<u64>,
) -> Value {
    let byte_count = declared_byte_count.unwrap_or(data.len() as u64);
    if data.len() as u64 > SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_BYTES {
        return skipped_untracked_preview(
            path,
            "too_large",
            Some(byte_count.max(data.len() as u64)),
        );
    }
    if data
        .iter()
        .any(|byte| *byte == 0 || (*byte < 32 && !matches!(*byte, b'\t' | b'\n' | b'\r')))
    {
        return skipped_untracked_preview(path, "binary_or_non_utf8", Some(data.len() as u64));
    }
    let text = match std::str::from_utf8(data) {
        Ok(text) => text,
        Err(_) => {
            return skipped_untracked_preview(path, "binary_or_non_utf8", Some(data.len() as u64))
        }
    };
    let all_lines: Vec<&str> = text.lines().collect();
    let shown_lines: Vec<Value> = all_lines
        .iter()
        .take(SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_LINES)
        .enumerate()
        .map(|(index, line)| {
            json!({
                "line": index + 1,
                "text": line,
            })
        })
        .collect();
    json!({
        "path": path,
        "kind": "text",
        "line_count": all_lines.len(),
        "byte_count": byte_count,
        "truncated": all_lines.len() > SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_LINES,
        "lines": shown_lines,
    })
}

#[cfg(test)]
pub(crate) fn collect_show_changes_untracked_previews_for_root(
    root: &Path,
    untracked_paths: &[String],
) -> (Vec<Value>, bool) {
    let truncated = untracked_paths.len() > SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_FILES;
    let canonical_root = match root.canonicalize() {
        Ok(root) => root,
        Err(_) => return (Vec::new(), truncated),
    };
    let mut previews = Vec::new();
    for path in untracked_paths
        .iter()
        .take(SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_FILES)
    {
        if untracked_preview_path_is_invalid(path) || untracked_preview_path_is_sensitive(path) {
            previews.push(skipped_untracked_preview(
                path,
                "sensitive_or_excluded_path",
                None,
            ));
            continue;
        }
        let full_path = root.join(path);
        let metadata = match std::fs::symlink_metadata(&full_path) {
            Ok(metadata) => metadata,
            Err(_) => {
                previews.push(skipped_untracked_preview(path, "not_found", None));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            previews.push(skipped_untracked_preview(
                path,
                "sensitive_or_excluded_path",
                None,
            ));
            continue;
        }
        if !metadata.is_file() {
            previews.push(skipped_untracked_preview(path, "not_regular_file", None));
            continue;
        }
        let canonical = match full_path.canonicalize() {
            Ok(canonical) => canonical,
            Err(_) => {
                previews.push(skipped_untracked_preview(
                    path,
                    "sensitive_or_excluded_path",
                    None,
                ));
                continue;
            }
        };
        if !canonical.starts_with(&canonical_root) {
            previews.push(skipped_untracked_preview(
                path,
                "sensitive_or_excluded_path",
                None,
            ));
            continue;
        }
        let byte_count = metadata.len();
        if byte_count > SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_BYTES {
            previews.push(skipped_untracked_preview(
                path,
                "too_large",
                Some(byte_count),
            ));
            continue;
        }
        match std::fs::read(&full_path) {
            Ok(data) => previews.push(untracked_preview_from_bytes(path, &data, Some(byte_count))),
            Err(_) => previews.push(skipped_untracked_preview(path, "read_error", None)),
        }
    }
    (previews, truncated)
}

pub(crate) fn show_changes_untracked_paths(output: &Value) -> Vec<String> {
    output["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|file| file["kind"] == "untracked")
        .filter_map(|file| file["path"].as_str().map(str::to_string))
        .collect()
}

fn show_changes_untracked_preview_probe_command(path: &str) -> String {
    format!(
        "p={path}; \
         if [ -L \"$p\" ]; then printf 'SKIP\\tsensitive_or_excluded_path\\n'; \
         elif [ ! -e \"$p\" ]; then printf 'SKIP\\tnot_found\\n'; \
         elif [ ! -f \"$p\" ]; then printf 'SKIP\\tnot_regular_file\\n'; \
         else bytes=$(wc -c < \"$p\" 2>/dev/null | tr -d '[:space:]'); \
           case \"$bytes\" in \
             ''|*[!0-9]*) printf 'SKIP\\tread_error\\n' ;; \
             *) if [ \"$bytes\" -gt {max_bytes} ]; then printf 'SKIP\\ttoo_large\\t%s\\n' \"$bytes\"; \
                else printf 'DATA\\t%s\\n' \"$bytes\"; base64 < \"$p\" 2>/dev/null; fi ;; \
           esac; \
         fi",
        path = shell_escape_simple(path),
        max_bytes = SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_BYTES,
    )
}

fn parse_show_changes_agent_preview_probe(
    path: &str,
    stdout: &str,
    exit_code: Option<i32>,
) -> Value {
    let mut lines = stdout.lines();
    let Some(header) = lines.next() else {
        return skipped_untracked_preview(path, "read_error", None);
    };
    let parts: Vec<&str> = header.split('\t').collect();
    match parts.as_slice() {
        ["SKIP", reason] => skipped_untracked_preview(path, reason, None),
        ["SKIP", reason, byte_count] => {
            skipped_untracked_preview(path, reason, byte_count.parse::<u64>().ok())
        }
        ["DATA", byte_count] => {
            if exit_code != Some(0) {
                return skipped_untracked_preview(
                    path,
                    "read_error",
                    byte_count.parse::<u64>().ok(),
                );
            }
            let encoded = lines.collect::<Vec<_>>().join("");
            let data = match general_purpose::STANDARD.decode(encoded.as_bytes()) {
                Ok(data) => data,
                Err(_) => return skipped_untracked_preview(path, "read_error", None),
            };
            untracked_preview_from_bytes(path, &data, byte_count.parse::<u64>().ok())
        }
        _ => skipped_untracked_preview(path, "read_error", None),
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct SessionActionSignals {
    failed: bool,
    write_like: bool,
    shell_like: bool,
}

fn has_smoke_warning(warnings: &[Value]) -> bool {
    warnings
        .iter()
        .any(|warning| warning["kind"] == "untracked_smoke_file")
}

fn suggested_next_actions_for(
    clean: bool,
    has_untracked: bool,
    has_smoke_warning: bool,
    session: Option<SessionActionSignals>,
) -> Vec<String> {
    let mut actions = Vec::new();
    let session = session.unwrap_or_default();
    if clean && !session.failed {
        push_unique_action(&mut actions, "no changes detected");
    }
    if !clean {
        push_unique_action(&mut actions, "review diff");
        push_unique_action(&mut actions, "run focused tests");
        if has_untracked {
            push_unique_action(&mut actions, "review untracked files before commit");
        }
        if has_smoke_warning {
            push_unique_action(
                &mut actions,
                "clean untracked smoke/tmp files or intentionally commit them",
            );
        }
        push_unique_action(&mut actions, "commit or revert changes after review");
    }
    if session.failed {
        push_unique_action(&mut actions, "review failed tool calls in session_summary");
    }
    if session.write_like {
        push_unique_action(&mut actions, "review changed paths from this session");
    }
    if session.shell_like {
        push_unique_action(&mut actions, "check command/test results before commit");
    }
    actions
}

fn push_unique_action(actions: &mut Vec<String>, action: &str) {
    if !actions.iter().any(|existing| existing == action) {
        actions.push(action.to_string());
    }
}

pub(crate) fn apply_show_changes_session(
    output: &mut Value,
    session_id: Option<&str>,
    summary: Option<SessionSummary>,
    recent_events_limit: Option<usize>,
) {
    let Some(session_id) = session_id else {
        output["session"] = Value::Null;
        set_show_changes_verdict(output);
        return;
    };
    let session_signals = match summary {
        Some(summary) => {
            let changed_paths = session_changed_paths(&summary.events);
            let recent_events = recent_events_limit.filter(|limit| *limit > 0).map(|limit| {
                let start = summary.events.len().saturating_sub(limit);
                summary.events[start..]
                    .iter()
                    .map(show_changes_session_event)
                    .collect::<Vec<_>>()
            });
            let signals = SessionActionSignals {
                failed: summary.counts.failed > 0,
                write_like: summary.counts.write_like > 0,
                shell_like: summary.counts.shell_like > 0,
            };
            output["session"] = json!({
                "found": true,
                "session_id": summary.session_id,
                "project": summary.project,
                "title": summary.title,
                "created_at": summary.created_at,
                "updated_at": summary.updated_at,
                "counts": summary.counts,
                "changed_paths": changed_paths,
                "signals": {
                    "failed": signals.failed,
                    "write_like": signals.write_like,
                    "shell_like": signals.shell_like,
                },
            });
            if let Some(recent_events) = recent_events {
                output["session"]["recent_events"] = json!(recent_events);
            }
            Some(signals)
        }
        None => {
            output["session"] = json!({
                "found": false,
                "session_id": session_id,
                "message": "session not found",
            });
            if let Some(warnings) = output["warnings"].as_array_mut() {
                warnings.push(json!({
                    "kind": "session_not_found",
                    "session_id": session_id,
                    "message": "session not found",
                }));
            }
            None
        }
    };
    refresh_show_changes_suggestions(output, session_signals);
}

fn refresh_show_changes_suggestions(output: &mut Value, session: Option<SessionActionSignals>) {
    // Any unobserved status keeps an unavailable message as the primary
    // suggestion. Clean/dirty review actions only apply to a reliable
    // porcelain branch header.
    let status_observation = output
        .pointer("/status_observation/status")
        .and_then(Value::as_str)
        .unwrap_or("output_unavailable");
    if status_observation != "observed" {
        let session = session.unwrap_or_default();
        let mut actions = vec![if status_observation == "non_git" {
            "git-backed status/diff unavailable; project is not a git repository".to_string()
        } else {
            "git status unavailable; inspect the status failure before relying on worktree cleanliness".to_string()
        }];
        if session.failed {
            push_unique_action(&mut actions, "review failed tool calls in session_summary");
        }
        if session.write_like {
            push_unique_action(&mut actions, "review changed paths from this session");
        }
        if session.shell_like {
            push_unique_action(&mut actions, "check command/test results before commit");
        }
        output["suggested_next_actions"] = json!(actions);
        set_show_changes_verdict(output);
        return;
    }
    let clean = output["clean"].as_bool().unwrap_or(false);
    let has_untracked = output["counts"]["untracked"].as_u64().unwrap_or(0) > 0;
    let has_smoke_warning = output["warnings"]
        .as_array()
        .is_some_and(|warnings| has_smoke_warning(warnings));
    output["suggested_next_actions"] = json!(suggested_next_actions_for(
        clean,
        has_untracked,
        has_smoke_warning,
        session,
    ));
    set_show_changes_verdict(output);
}

fn set_show_changes_verdict(output: &mut Value) {
    let mut blocking_reasons: Vec<&'static str> = Vec::new();
    let mut warning_reasons: Vec<&'static str> = Vec::new();
    let mut actions = string_array(output.get("suggested_next_actions"));

    let git_available = output
        .get("git_available")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let non_git_project = output
        .get("non_git_project")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let status_observation = output
        .pointer("/status_observation/status")
        .and_then(Value::as_str)
        .unwrap_or("output_unavailable");
    let status_reason_code = output
        .pointer("/status_observation/reason_code")
        .and_then(Value::as_str);
    match status_observation {
        "command_failed" => {
            push_unique_reason(
                &mut warning_reasons,
                match status_reason_code {
                    Some("git_status_config_error") => "git_status_config_error",
                    Some("git_status_permission_denied") => "git_status_permission_denied",
                    _ => "git_status_command_failed",
                },
            );
        }
        "output_unavailable" => {
            push_unique_reason(&mut warning_reasons, "git_status_output_unavailable");
        }
        _ => {}
    }
    if non_git_project {
        push_unique_reason(&mut warning_reasons, "non_git_project");
        push_unique_action(
            &mut actions,
            "git-backed status/diff is not applicable; continue with non-git review evidence",
        );
    } else if !git_available {
        push_unique_reason(&mut warning_reasons, "git_unavailable");
        push_unique_action(
            &mut actions,
            "git-backed status/diff unavailable; inspect Git availability before relying on worktree review",
        );
    }

    if output
        .get("clean")
        .and_then(Value::as_bool)
        .is_some_and(|clean| !clean)
    {
        push_unique_reason(&mut warning_reasons, "workspace_dirty");
        push_unique_action(&mut actions, "review workspace changes with show_changes");
    }
    if output
        .pointer("/counts/conflicted")
        .and_then(Value::as_u64)
        .is_some_and(|count| count > 0)
    {
        push_unique_reason(&mut blocking_reasons, "workspace_conflicts");
        push_unique_action(&mut actions, "resolve workspace conflicts before closeout");
    }

    if status_observation == "command_failed"
        || (git_available
            && output
                .get("exit_code")
                .and_then(Value::as_i64)
                .is_some_and(|exit_code| exit_code != 0))
    {
        push_unique_reason(&mut blocking_reasons, "git_inspection_failed");
        push_unique_action(
            &mut actions,
            "rerun show_changes or inspect git status directly",
        );
    }

    if output
        .get("warnings")
        .and_then(Value::as_array)
        .is_some_and(|warnings| !warnings.is_empty())
    {
        push_unique_reason(&mut warning_reasons, "review_warnings_present");
    }

    let hunks_truncated = output
        .get("hunks_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if hunks_truncated {
        actions.retain(|action| action != "review workspace changes with show_changes");
        let diff_truncation_reasons = output
            .get("truncation_reasons")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|reason| match reason.as_str() {
                Some("diff_hunk_count_limit") => Some("diff_hunk_count_limit"),
                Some("diff_hunk_line_limit") => Some("diff_hunk_line_limit"),
                Some("diff_byte_budget") => Some("diff_byte_budget"),
                Some("diff_hunk_byte_budget") => Some("diff_hunk_byte_budget"),
                _ => None,
            })
            .collect::<Vec<_>>();
        let page_truncated = diff_truncation_reasons
            .iter()
            .any(|reason| matches!(*reason, "diff_hunk_count_limit" | "diff_byte_budget"));
        let hunk_line_truncated = diff_truncation_reasons
            .iter()
            .any(|reason| *reason == "diff_hunk_line_limit");
        // show_changes currently reports line truncation at the aggregate diff
        // level, not as authoritative per-hunk provenance. Keep whole-worktree
        // scope rather than guessing which returned path owns the omitted lines.
        let suggested_paths: Vec<String> = Vec::new();
        let suggested_max_hunk_lines = if hunk_line_truncated {
            MAX_MAX_HUNK_LINES
        } else {
            DEFAULT_MAX_HUNK_LINES
        };
        let project = output
            .get("project")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let canonical_recovery_call = SuggestedToolCall::new(
            "git_diff_hunks",
            json!({
                "project": project,
                "cached": false,
                "paths": suggested_paths,
                "max_hunks": DEFAULT_MAX_HUNKS,
                "max_hunk_lines": suggested_max_hunk_lines,
                "max_page_bytes": DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES,
            }),
        )
        .to_value();
        output["diff_review_handoff"] = json!({
            "next_call": canonical_recovery_call,
        });
        push_unique_reason(&mut warning_reasons, "truncated_by_limit");
        push_unique_action(
            &mut actions,
            "continue the diff review with git_diff_hunks; use paths to narrow scope when useful",
        );
        if page_truncated {
            push_unique_action(
                &mut actions,
                "follow git_diff_hunks recovery.later_hunks.next_call while has_more=true",
            );
        }
        if hunk_line_truncated {
            push_unique_action(
                &mut actions,
                "follow git_diff_hunks recovery.current_hunk.next_call; after the fresh handoff observation it may use bounded refinement or an exact hunk-fragment continuation",
            );
        }
    } else if let Some(object) = output.as_object_mut() {
        object.remove("diff_review_handoff");
    }

    let untracked_previews_truncated = output
        .get("untracked_previews_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if untracked_previews_truncated {
        actions.retain(|action| action != "review workspace changes with show_changes");
        push_unique_reason(&mut warning_reasons, "truncated_by_limit");
        push_unique_action(
            &mut actions,
            "inspect the relevant untracked files separately",
        );
    }

    if actions.is_empty() {
        actions.push("no action needed".to_string());
    }
    if hunks_truncated || untracked_previews_truncated {
        output["suggested_next_actions"] = json!(actions.clone());
    }
    let status = if blocking_reasons.is_empty() {
        if warning_reasons.is_empty() {
            "pass"
        } else {
            "warn"
        }
    } else {
        "fail"
    };

    output["verdict"] = json!({
        "status": status,
        "blocking": !blocking_reasons.is_empty(),
        "blocking_reasons": blocking_reasons,
        "warning_reasons": warning_reasons,
        "suggested_next_actions": actions,
    });
}

pub(super) fn sparsify_complete_show_changes_output(output: &mut serde_json::Map<String, Value>) {
    let Some(files_len) = output.get("files").and_then(Value::as_array).map(Vec::len) else {
        return;
    };
    let status_observed = output.get("status_observation").is_some_and(|status| {
        status.get("status").and_then(Value::as_str) == Some("observed")
            && status.get("repository_probe").and_then(Value::as_str) == Some("inside_worktree")
    });
    let diff_stat_observed = output.get("diff_stat_status").is_some_and(|status| {
        status.get("status").and_then(Value::as_str) == Some("observed")
            && status.get("exit_code").and_then(Value::as_i64) == Some(0)
            && status.get("reason_code").is_some_and(Value::is_null)
    });
    let has_hunks = output.get("hunks").and_then(Value::as_array).is_some();
    let diff_observed = if has_hunks {
        output.get("diff_exit").and_then(Value::as_i64) == Some(0)
            && output.get("diff_status").is_some_and(|status| {
                status.get("status").and_then(Value::as_str) == Some("observed")
                    && status.get("exit_code").and_then(Value::as_i64) == Some(0)
            })
    } else {
        output.get("diff_exit").is_some_and(Value::is_null)
    };
    let hunk_metadata_complete = match output.get("hunks").and_then(Value::as_array) {
        Some(hunks) => {
            git_diff_file_hunk_count(hunks).is_some_and(|count| {
                output.get("hunk_count").and_then(Value::as_u64) == Some(count as u64)
            }) && output.get("hunks_truncated").and_then(Value::as_bool) == Some(false)
                && !git_diff_files_have_omitted_lines(hunks)
        }
        None => output.get("hunk_count").is_none() && output.get("hunks_truncated").is_none(),
    };
    let ordinary_complete = output.get("git_available").and_then(Value::as_bool) == Some(true)
        && output.get("non_git_project").and_then(Value::as_bool) == Some(false)
        && output.get("git_error").is_some_and(Value::is_null)
        && status_observed
        && output.get("files_total").and_then(Value::as_u64) == Some(files_len as u64)
        && output.get("files_returned").and_then(Value::as_u64) == Some(files_len as u64)
        && output.get("files_truncated").and_then(Value::as_bool) == Some(false)
        && output.get("transport_safe").and_then(Value::as_bool) == Some(true)
        && output.get("output_truncated").and_then(Value::as_bool) == Some(false)
        && output
            .get("truncation_reasons")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        && output.get("diff_review_handoff").is_none()
        && output
            .get("untracked_previews_truncated")
            .is_none_or(|value| value.as_bool() == Some(false))
        && output
            .get("warnings")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        && output.get("exit_code").and_then(Value::as_i64) == Some(0)
        && output.get("stderr").and_then(Value::as_str) == Some("")
        && output.get("head_exit").and_then(Value::as_i64) == Some(0)
        && diff_stat_observed
        && diff_observed
        && hunk_metadata_complete;
    if !ordinary_complete {
        return;
    }

    if let Some(hunks) = output.get_mut("hunks").and_then(Value::as_array_mut) {
        sparsify_complete_diff_files(hunks);
    }
    for key in [
        "git_available",
        "non_git_project",
        "git_error",
        "status_observation",
        "files_total",
        "files_returned",
        "files_truncated",
        "files_limit",
        "transport_safe",
        "output_budget_bytes",
        "output_truncated",
        "truncation_reasons",
        "diff_exit",
        "diff_status",
        "diff_stat_exit",
        "diff_stat_status",
        "head_exit",
        "hunk_count",
        "hunks_truncated",
        "untracked_previews_truncated",
        "warnings",
        "exit_code",
        "stderr",
    ] {
        output.remove(key);
    }
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn push_unique_reason(reasons: &mut Vec<&'static str>, reason: &'static str) {
    if !reasons.iter().any(|existing| existing == &reason) {
        reasons.push(reason);
    }
}

fn session_changed_paths(events: &[SessionEvent]) -> Vec<String> {
    let mut paths = Vec::new();
    for event in events {
        for path in &event.changed_paths {
            let path = path.trim();
            if !path.is_empty() && !paths.iter().any(|existing| existing == path) {
                paths.push(path.to_string());
            }
        }
    }
    paths
}

fn show_changes_session_event(event: &SessionEvent) -> Value {
    json!({
        "event_id": event.event_id,
        "kind": event.kind,
        "timestamp": event.timestamp,
        "transport": event.transport,
        "tool_name": event.tool_name,
        "project": event.project,
        "resolved_project": event.resolved_project,
        "risk_class": event.risk_class,
        "read_like": event.read_like,
        "write_like": event.write_like,
        "shell_like": event.shell_like,
        "git_like": event.git_like,
        "change_summary_like": event.change_summary_like,
        "started_at": event.started_at,
        "finished_at": event.finished_at,
        "status": event.status,
        "exit_code": event.exit_code,
        "failure_kind": event.failure_kind,
        "duration_ms": event.duration_ms,
        "error_kind": event.error_kind,
        "error_message_summary": event.error_message_summary,
        "changed_paths": event.changed_paths,
        "job_id": event.job_id,
    })
}

impl ToolRuntime {
    async fn collect_show_changes_untracked_previews(
        &self,
        project: &str,
        untracked_paths: &[String],
    ) -> (Vec<Value>, bool) {
        let truncated = untracked_paths.len() > SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_FILES;
        if self.resolve_project(project).await.is_err() {
            return (Vec::new(), truncated);
        }
        let mut previews = Vec::new();
        for path in untracked_paths
            .iter()
            .take(SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_FILES)
        {
            if untracked_preview_path_is_invalid(path) || untracked_preview_path_is_sensitive(path)
            {
                previews.push(skipped_untracked_preview(
                    path,
                    "sensitive_or_excluded_path",
                    None,
                ));
                continue;
            }
            let command = show_changes_untracked_preview_probe_command(path);
            let preview = match self
                .run_project_internal_posix_script_capture(project, command, 10, None)
                .await
            {
                Ok(output) => {
                    parse_show_changes_agent_preview_probe(path, &output.stdout, output.exit_code)
                }
                Err(_) => skipped_untracked_preview(path, "read_error", None),
            };
            previews.push(preview);
        }
        (previews, truncated)
    }

    pub(crate) async fn git_status(&self, project: String) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let client_id = proj.client_id.clone();
        let (req_id, rx) = match self
            .runner_registry
            .enqueue_run(
                ShellRunRequest {
                    login: false,
                    client_id,
                    cwd: Some(proj.path.clone()),
                    command: "git status --porcelain".to_string(),
                    stdin: None,
                    timeout_secs: 30,
                    wait_timeout_secs: 32,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(e),
        };
        match tokio::time::timeout(Duration::from_secs(34), rx).await {
            Ok(Ok(resp)) => ToolResult::ok(json!({
                "stdout": resp.stdout,
                "stderr": resp.stderr,
                "exit_code": resp.exit_code,
            })),
            Ok(Err(_)) => {
                self.runner_registry.cancel_request(&req_id).await;
                ToolResult::err("request dropped")
            }
            Err(_) => {
                self.runner_registry.cancel_request(&req_id).await;
                ToolResult::err("timed out")
            }
        }
    }

    pub(crate) async fn show_changes(
        &self,
        project: String,
        session_id: Option<String>,
        include_diff: Option<bool>,
        max_hunks: Option<usize>,
        max_hunk_lines: Option<usize>,
        session_event_limit: Option<usize>,
    ) -> ToolResult {
        self.show_changes_observation(
            project,
            session_id,
            include_diff,
            max_hunks,
            max_hunk_lines,
            session_event_limit,
            false,
        )
        .await
    }

    /// Presentation must not execute repository-configured filters or hooks.
    /// Reuse the ordinary bounded producer/parser without recording a Session.
    pub(in crate::tool_runtime) async fn show_changes_for_presentation(
        &self,
        project: String,
    ) -> ToolResult {
        self.show_changes_observation(project, None, Some(false), None, None, None, true)
            .await
    }

    async fn show_changes_observation(
        &self,
        project: String,
        session_id: Option<String>,
        include_diff: Option<bool>,
        max_hunks: Option<usize>,
        max_hunk_lines: Option<usize>,
        session_event_limit: Option<usize>,
        read_only_presentation: bool,
    ) -> ToolResult {
        let include_diff = include_diff.unwrap_or(false);
        let max_hunks = max_hunks
            .filter(|n| *n > 0)
            .unwrap_or(SHOW_CHANGES_DEFAULT_MAX_HUNKS)
            .min(SHOW_CHANGES_MAX_HUNKS);
        let max_hunk_lines = max_hunk_lines
            .filter(|n| *n > 0)
            .unwrap_or(SHOW_CHANGES_DEFAULT_MAX_HUNK_LINES)
            .min(SHOW_CHANGES_MAX_HUNK_LINES);
        let recent_events_limit = session_event_limit
            .unwrap_or(0)
            .min(SHOW_CHANGES_MAX_SESSION_EVENT_LIMIT);
        let session_summary_limit = recent_events_limit
            .max(SHOW_CHANGES_SESSION_SIGNAL_EVENT_LIMIT)
            .min(SHOW_CHANGES_MAX_SESSION_EVENT_LIMIT);
        let mut command = show_changes_command(include_diff, max_hunks, max_hunk_lines);
        if read_only_presentation {
            command = format!(
                r#"set -eu
umask 077
GIT_OPTIONAL_LOCKS=0; export GIT_OPTIONAL_LOCKS
{safe_config}
git() {{
  if [ "$1" = diff ]; then
    shift
    command git -c include.path="$changes_git_overlay" -c core.fsmonitor=false diff --no-ext-diff --no-textconv "$@"
  else
    command git -c include.path="$changes_git_overlay" -c core.fsmonitor=false "$@"
  fi
}}
set +e
{command}"#,
                safe_config = super::super::changes::CHANGES_GIT_SAFE_CONFIG_SETUP,
            );
        }
        let output = match self
            .run_project_internal_posix_script_capture(&project, command, 30, None)
            .await
        {
            Ok(output) => output,
            Err(e) => return ToolResult::err(e),
        };
        let frames = split_show_changes_stdout(&output.stdout, include_diff);
        let status_observation = parse_show_changes_status_observation(
            &frames.status,
            &frames.status_result,
            &output.stderr,
        );
        let effective_exit_code = if status_observation.exit_code != Some(0) {
            status_observation.exit_code
        } else {
            output.exit_code
        };
        // Graceful degradation is reserved for a dedicated repository probe
        // that explicitly reports an outside-worktree directory. Status
        // configuration, permission, or execution failures remain distinct.
        if status_observation.non_git() {
            let mut payload = non_git_show_changes_payload_with_observation(
                &project,
                include_diff,
                status_observation,
            );
            let session_summary = session_id
                .as_deref()
                .and_then(|id| self.sessions.summary(id, Some(session_summary_limit)));
            apply_show_changes_session(
                &mut payload,
                session_id.as_deref(),
                session_summary,
                (recent_events_limit > 0).then_some(recent_events_limit),
            );
            return ToolResult::ok(payload);
        }
        let status_observed = status_observation.status_observed();
        let mut payload = parse_show_changes_output_with_observation(
            &project,
            &frames.status,
            &frames.head,
            &frames.stat,
            include_diff.then_some(frames.diff.as_str()),
            max_hunks,
            max_hunk_lines,
            effective_exit_code,
            &output.stderr,
            status_observation,
            &frames,
        );
        if include_diff {
            let untracked_paths = show_changes_untracked_paths(&payload);
            let (previews, truncated) = self
                .collect_show_changes_untracked_previews(&project, &untracked_paths)
                .await;
            payload["untracked_previews"] = json!(previews);
            payload["untracked_previews_truncated"] = json!(truncated);
        }
        let session_summary = session_id
            .as_deref()
            .and_then(|id| self.sessions.summary(id, Some(session_summary_limit)));
        apply_show_changes_session(
            &mut payload,
            session_id.as_deref(),
            session_summary,
            (recent_events_limit > 0).then_some(recent_events_limit),
        );
        // Success requires the command/result envelope, status, diff-stat, and
        // (when requested) full diff inspections all to be proven successful.
        // `transport_safe` is a legacy field name: it describes bounded
        // command/result-envelope integrity, not polling/WebSocket/QUIC wire
        // capacity, and must never mask an observed or unavailable inspection failure.
        let diff_stat_ok = frames.diff_stat_exit == Some(0);
        let diff_ok = if include_diff {
            // `diff_exit == None` means the exit code could not be captured
            // (e.g. legacy/result-retention-truncated stdout); treat as not proven-ok.
            frames.diff_exit == Some(0)
        } else {
            true
        };
        if status_observed && output.exit_code == Some(0) && diff_stat_ok && diff_ok {
            ToolResult::ok(payload)
        } else {
            let error = if !status_observed {
                "show_changes git status unavailable".to_string()
            } else if !diff_stat_ok {
                match frames.diff_stat_exit {
                    Some(code) => format!(
                        "show_changes git diff-stat inspection failed with exit code {code}"
                    ),
                    None => "show_changes git diff-stat inspection unavailable".to_string(),
                }
            } else if include_diff && !diff_ok {
                match frames.diff_exit {
                    Some(code) => {
                        format!("show_changes git diff inspection failed with exit code {code}")
                    }
                    None => "show_changes git diff inspection unavailable".to_string(),
                }
            } else {
                "show_changes git inspection failed".to_string()
            };
            ToolResult {
                success: false,
                output: payload,
                error: Some(error),
            }
        }
    }
}
