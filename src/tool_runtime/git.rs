use super::tool_result::ToolResult;

mod diff_hunks;
mod log;
mod mutations;
mod shared;
mod show_changes;

#[cfg(test)]
pub(crate) use self::diff_hunks::{
    git_diff_hunks_command, parse_git_diff_hunks, GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES,
    MAX_MAX_HUNK_LINES,
};
#[cfg(test)]
pub(crate) use self::log::{
    git_log_command, git_log_command_at_head, git_log_next_skip, normalize_git_log_limit,
    normalize_git_log_skip, parse_git_log_commits,
};
#[cfg(test)]
pub(crate) use self::mutations::{parse_git_commit_marker, GIT_COMMIT_RESULT_PREFIX};
#[cfg(test)]
pub(crate) use self::show_changes::{
    apply_show_changes_session, collect_show_changes_untracked_previews_for_root,
    framed_clean_show_changes_test_stdout, framed_show_changes_test_block,
    non_git_show_changes_payload, parse_show_changes_output,
    parse_show_changes_output_with_observation, parse_show_changes_status_observation,
    parse_status_header, show_changes_command, show_changes_untracked_paths,
    split_show_changes_stdout, ShowChangesStdout, SHOW_CHANGES_DIFF_STAT_BYTES,
    SHOW_CHANGES_HEAD_BYTES, SHOW_CHANGES_MAX_STATUS_FILES, SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
    SHOW_CHANGES_SENTINEL,
};

pub(crate) fn sparsify_complete_git_review_success(tool_name: &str, result: &mut ToolResult) {
    if !result.success {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    match tool_name {
        "git_diff_hunks" => self::diff_hunks::sparsify_complete_git_diff_hunks_output(output),
        "show_changes" => self::show_changes::sparsify_complete_show_changes_output(output),
        _ => {}
    }
}
