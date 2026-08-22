use super::helpers::shell_escape_simple;
use super::ToolRuntime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommittedGitScope {
    pub(crate) requested_base: String,
    pub(crate) requested_head: String,
    pub(crate) merge_base: String,
    pub(crate) base_is_ancestor: bool,
    pub(crate) commit_count: u64,
    pub(crate) files_changed: u64,
    pub(crate) insertions: u64,
    pub(crate) deletions: u64,
    pub(crate) binary_files: u64,
}

pub(crate) fn normalize_exact_commit_id(value: &str) -> Result<String, &'static str> {
    if value.len() != 40 || !value.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err("invalid_commit_id");
    }
    Ok(value.to_ascii_lowercase())
}

pub(crate) fn committed_git_discovery_prefix() -> &'static str {
    concat!(
        "export LC_ALL=C GIT_PAGER=cat GIT_NO_REPLACE_OBJECTS=1 GIT_NO_LAZY_FETCH=1 ",
        "GIT_OPTIONAL_LOCKS=0 GIT_ATTR_NOSYSTEM=1 GIT_CONFIG_COUNT=0; ",
        "unset GIT_EXTERNAL_DIFF GIT_DIFF_OPTS GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE ",
        "GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_COMMON_DIR ",
        "GIT_CEILING_DIRECTORIES GIT_ATTR_SOURCE GIT_CONFIG GIT_CONFIG_PARAMETERS ",
        "GIT_CONFIG_GLOBAL GIT_CONFIG_SYSTEM GIT_CONFIG_NOSYSTEM GIT_SHALLOW_FILE ",
        "GIT_NAMESPACE; "
    )
}

pub(crate) fn committed_git_isolated_view_setup(head: &str, failure: &str) -> String {
    format!(
        concat!(
            "object_dir=$(git rev-parse --path-format=absolute --git-path objects 2>/dev/null) || {{ {failure}; }}; ",
            "view=$(mktemp -d /tmp/webcodex-git-review.XXXXXX 2>/dev/null) || {{ {failure}; }}; ",
            "cleanup_git_review_view() {{ rm -rf -- \"$view\"; }}; ",
            "trap cleanup_git_review_view EXIT; trap 'exit 130' HUP INT TERM; ",
            "mkdir -p \"$view/refs\" \"$view/objects/info\" || {{ {failure}; }}; ",
            "printf 'ref: refs/heads/unused\\n' >\"$view/HEAD\" || {{ {failure}; }}; ",
            "printf '[core]\\n\\trepositoryformatversion = 0\\n\\tbare = true\\n\\tattributesFile = /dev/null\\n' >\"$view/config\" || {{ {failure}; }}; ",
            "export GIT_DIR=\"$view\" GIT_OBJECT_DIRECTORY=\"$object_dir\" GIT_ATTR_SOURCE={head_q} ",
            "GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1; "
        ),
        failure = failure,
        head_q = shell_escape_simple(head),
    )
}

pub(crate) fn checked_git_pipeline_to_file(
    producer: &str,
    consumer: &str,
    stem: &str,
    failure: &str,
) -> String {
    debug_assert!(stem
        .as_bytes()
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_'));
    format!(
        concat!(
            "rm -f \"$view/{stem}.status\" \"$view/{stem}.out\"; ",
            "{{ {producer}; producer_exit=$?; printf '%s\\n' \"$producer_exit\" >\"$view/{stem}.status\"; }} | ",
            "{consumer} >\"$view/{stem}.out\"; ",
            "consumer_exit=$?; ",
            "producer_exit=$(cat \"$view/{stem}.status\" 2>/dev/null || true); ",
            "if [ \"$consumer_exit\" -ne 0 ] || [ \"$producer_exit\" != 0 ]; then {failure}; fi; "
        ),
        producer = producer,
        consumer = consumer,
        stem = stem,
        failure = failure,
    )
}

pub(crate) fn committed_git_scope_command(base: &str, head: &str) -> String {
    let isolated_view =
        committed_git_isolated_view_setup(head, "printf 'status=git_view_unavailable\\n'; exit 0");
    let head_q = shell_escape_simple(head);
    let stats_producer = format!(
        "git --no-pager -c core.quotePath=false -c attr.tree={head_q} diff --no-ext-diff --no-textconv --find-renames --numstat \"$merge_base\" {head_q}"
    );
    let stats_pipeline = checked_git_pipeline_to_file(
        &stats_producer,
        "awk 'BEGIN{f=0;a=0;d=0;b=0} {f++; if ($1==\"-\" || $2==\"-\") b++; else {a+=$1; d+=$2}} END{printf \"files_changed=%d\\ninsertions=%d\\ndeletions=%d\\nbinary_files=%d\\n\",f,a,d,b}'",
        "scope_stats",
        "printf 'status=diff_failed\\n'; exit 0",
    );
    format!(
        concat!(
            "{prefix}",
            "if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then printf 'status=not_git\\n'; exit 0; fi; ",
            "{isolated_view}",
            "base_type=$(git cat-file -t {base_q} 2>/dev/null || true); ",
            "if [ \"$base_type\" != commit ]; then printf 'status=base_not_commit\\n'; exit 0; fi; ",
            "head_type=$(git cat-file -t {head_q} 2>/dev/null || true); ",
            "if [ \"$head_type\" != commit ]; then printf 'status=head_not_commit\\n'; exit 0; fi; ",
            "merge_bases=$(git merge-base --all {base_q} {head_q} 2>/dev/null || true); ",
            "if [ -z \"$merge_bases\" ]; then printf 'status=no_merge_base\\n'; exit 0; fi; ",
            "merge_base_count=$(printf '%s\\n' \"$merge_bases\" | awk 'NF{{n++}} END{{print n+0}}'); ",
            "if [ \"$merge_base_count\" -ne 1 ]; then printf 'status=ambiguous_merge_base\\n'; exit 0; fi; ",
            "merge_base=$merge_bases; ",
            "if git merge-base --is-ancestor {base_q} {head_q} >/dev/null 2>&1; then base_is_ancestor=true; ",
            "else ancestor_exit=$?; if [ \"$ancestor_exit\" -eq 1 ]; then base_is_ancestor=false; else printf 'status=ancestor_failed\\n'; exit 0; fi; fi; ",
            "commit_count=$(git rev-list --count \"$merge_base..{head}\" 2>/dev/null) || {{ printf 'status=rev_list_failed\\n'; exit 0; }}; ",
            "{stats_pipeline}",
            "stats=$(cat \"$view/scope_stats.out\" 2>/dev/null) || {{ printf 'status=stats_failed\\n'; exit 0; }}; ",
            "printf 'status=ok\\nrequested_base=%s\\nrequested_head=%s\\nmerge_base=%s\\nbase_is_ancestor=%s\\ncommit_count=%s\\n%s' ",
            "{base_q} {head_q} \"$merge_base\" \"$base_is_ancestor\" \"$commit_count\" \"$stats\""
        ),
        prefix = committed_git_discovery_prefix(),
        isolated_view = isolated_view,
        stats_pipeline = stats_pipeline,
        base_q = shell_escape_simple(base),
        head_q = head_q,
        head = head,
    )
}

fn parse_scope_value<'a>(stdout: &'a str, key: &str) -> Option<&'a str> {
    stdout.lines().find_map(|line| {
        let (field, value) = line.split_once('=')?;
        (field == key).then_some(value)
    })
}

fn parse_u64_scope(stdout: &str, key: &str) -> Option<u64> {
    parse_scope_value(stdout, key)?.parse().ok()
}

pub(crate) fn parse_committed_git_scope(stdout: &str) -> Result<CommittedGitScope, &'static str> {
    match parse_scope_value(stdout, "status") {
        Some("ok") => {}
        Some("not_git") => return Err("not_a_git_repository"),
        Some("base_not_commit") => return Err("base_commit_missing_or_not_commit"),
        Some("head_not_commit") => return Err("head_commit_missing_or_not_commit"),
        Some("no_merge_base") => return Err("no_merge_base"),
        Some("ambiguous_merge_base") => return Err("ambiguous_merge_base"),
        Some("git_view_unavailable") => return Err("git_isolated_view_unavailable"),
        Some("ancestor_failed") => return Err("merge_base_ancestor_check_failed"),
        Some("rev_list_failed") => return Err("commit_count_unavailable"),
        Some("diff_failed") | Some("stats_failed") => return Err("git_diff_failed"),
        _ => return Err("scope_observation_unavailable"),
    }
    let requested_base = normalize_exact_commit_id(
        parse_scope_value(stdout, "requested_base").ok_or("scope_observation_unavailable")?,
    )
    .map_err(|_| "scope_observation_unavailable")?;
    let requested_head = normalize_exact_commit_id(
        parse_scope_value(stdout, "requested_head").ok_or("scope_observation_unavailable")?,
    )
    .map_err(|_| "scope_observation_unavailable")?;
    let merge_base = normalize_exact_commit_id(
        parse_scope_value(stdout, "merge_base").ok_or("scope_observation_unavailable")?,
    )
    .map_err(|_| "scope_observation_unavailable")?;
    let base_is_ancestor = match parse_scope_value(stdout, "base_is_ancestor") {
        Some("true") => true,
        Some("false") => false,
        _ => return Err("scope_observation_unavailable"),
    };
    Ok(CommittedGitScope {
        requested_base,
        requested_head,
        merge_base,
        base_is_ancestor,
        commit_count: parse_u64_scope(stdout, "commit_count")
            .ok_or("scope_observation_unavailable")?,
        files_changed: parse_u64_scope(stdout, "files_changed")
            .ok_or("scope_observation_unavailable")?,
        insertions: parse_u64_scope(stdout, "insertions").ok_or("scope_observation_unavailable")?,
        deletions: parse_u64_scope(stdout, "deletions").ok_or("scope_observation_unavailable")?,
        binary_files: parse_u64_scope(stdout, "binary_files")
            .ok_or("scope_observation_unavailable")?,
    })
}

impl ToolRuntime {
    pub(crate) async fn resolve_committed_git_scope(
        &self,
        resolved_project: &str,
        base_commit: &str,
        head_commit: &str,
    ) -> Result<CommittedGitScope, &'static str> {
        let base = normalize_exact_commit_id(base_commit)?;
        let head = normalize_exact_commit_id(head_commit)?;
        let output = self
            .run_project_internal_posix_script_capture(
                resolved_project,
                committed_git_scope_command(&base, &head),
                30,
                None,
            )
            .await
            .map_err(|_| "scope_observation_unavailable")?;
        if output.exit_code != Some(0) || output.error.is_some() {
            return Err("scope_observation_unavailable");
        }
        let scope = parse_committed_git_scope(&output.stdout)?;
        if scope.requested_base != base || scope.requested_head != head {
            return Err("scope_observation_mismatch");
        }
        Ok(scope)
    }
}
