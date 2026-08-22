use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use webcodex_workspace::file_read_normalize::MODEL_RESULT_ENVELOPE_RESERVE_BYTES;
use webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES;

use super::git_committed::{
    checked_git_pipeline_to_file, committed_git_discovery_prefix,
    committed_git_isolated_view_setup, normalize_exact_commit_id,
};
use super::helpers::{decode_git_quoted_path, shell_escape_simple, validate_project_relative_path};
use super::tool_result::ToolResult;
use super::ToolRuntime;

pub(crate) const GIT_REVIEW_MAX_FILES: usize = 80;
pub(crate) const GIT_REVIEW_MAX_PATH_BYTES: usize = 512;
pub(crate) const GIT_REVIEW_MAX_SUBSYSTEMS: usize = 12;
pub(crate) const GIT_REVIEW_MAX_SIGNALS: usize = 12;
pub(crate) const GIT_REVIEW_MAX_PATHS_PER_SIGNAL: usize = 8;
pub(crate) const GIT_REVIEW_MAX_SYMBOLS_PER_FILE: usize = 6;
pub(crate) const GIT_REVIEW_MAX_TOTAL_SYMBOLS: usize = 80;
pub(crate) const GIT_REVIEW_MAX_SYMBOL_BYTES: usize = 120;
pub(crate) const GIT_REVIEW_MAX_DIFF_BYTES: usize = 64 * 1024;
const GIT_REVIEW_METADATA_BYTES: usize = 64 * 1024;
const GIT_REVIEW_MAX_WARNINGS: usize = 16;
const GIT_REVIEW_ERROR_SENTINEL: &str = "@@WEBCODEX_GIT_REVIEW_COMMAND_FAILED@@";

#[derive(Debug, Clone, PartialEq, Eq)]
struct NameStatusRecord {
    status: String,
    path: Option<String>,
    previous_path: Option<String>,
    path_omitted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NumstatRecord {
    additions: Option<u64>,
    deletions: Option<u64>,
    binary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawModeRecord {
    gitlink: bool,
}

#[derive(Debug, Clone)]
struct ReviewFile {
    status: String,
    path: Option<String>,
    previous_path: Option<String>,
    path_omitted: bool,
    additions: Option<u64>,
    deletions: Option<u64>,
    binary: Option<bool>,
    gitlink: Option<bool>,
    classes: Vec<String>,
    symbols: Vec<String>,
    symbol_inspection: &'static str,
}

#[derive(Debug, Clone)]
struct ReviewSignal {
    name: &'static str,
    category: &'static str,
    reason: &'static str,
    paths: Vec<String>,
    paths_truncated: bool,
}

fn git_review_failure(
    project: &str,
    requested_base: &str,
    requested_head: &str,
    reason_code: &'static str,
) -> ToolResult {
    let requested_base = normalize_exact_commit_id(requested_base).ok();
    let requested_head = normalize_exact_commit_id(requested_head).ok();
    ToolResult::err_with_output(
        format!("git_review_summary failed: {reason_code}"),
        json!({
            "project": project,
            "scope": {
                "requested_base": requested_base,
                "requested_head": requested_head,
                "merge_base": null,
                "base_is_ancestor": null,
                "commit_count": null,
            },
            "reason_code": reason_code,
            "deterministic": true,
            "llm_summary": false,
            "truncated": false,
            "warnings": [],
        }),
    )
}

fn bounded_review_diff_command(
    merge_base: &str,
    head: &str,
    mode: &str,
    max_lines: usize,
) -> String {
    let mode = match mode {
        "name_status" => "--name-status",
        "numstat" => "--numstat",
        "raw" => "--raw",
        _ => unreachable!("closed git review diff mode"),
    };
    let failure = format!("printf '{}\\n'; exit 0", GIT_REVIEW_ERROR_SENTINEL);
    let isolated_view = committed_git_isolated_view_setup(head, &failure);
    let producer = format!(
        "git --no-pager -c core.quotePath=false -c attr.tree={head_q} diff --no-ext-diff --no-textconv --find-renames {mode} {base_q} {head_q}",
        head_q = shell_escape_simple(head),
        mode = mode,
        base_q = shell_escape_simple(merge_base),
    );
    let consumer = format!(
        concat!(
            "awk -v max_lines={max_lines} -v max_bytes={max_bytes} '",
            "BEGIN{{n=0;b=0;emit=1}} ",
            "{{s=length($0)+1; if (emit && n<max_lines && b+s<=max_bytes) ",
            "{{print; n++; b+=s}} else emit=0}}'"
        ),
        max_lines = max_lines,
        max_bytes = GIT_REVIEW_METADATA_BYTES,
    );
    let pipeline = checked_git_pipeline_to_file(&producer, &consumer, "metadata", &failure);
    format!(
        concat!(
            "{prefix}",
            "if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then printf '{error}\\n'; exit 0; fi; ",
            "{isolated_view}",
            "{pipeline}",
            "cat \"$view/metadata.out\" 2>/dev/null || {{ {failure}; }}"
        ),
        prefix = committed_git_discovery_prefix(),
        isolated_view = isolated_view,
        pipeline = pipeline,
        error = GIT_REVIEW_ERROR_SENTINEL,
        failure = failure,
    )
}

fn bounded_output_path(path: String) -> (Option<String>, bool) {
    if path.len() > GIT_REVIEW_MAX_PATH_BYTES {
        (None, true)
    } else {
        (Some(path), false)
    }
}

fn normalize_status(status: &str) -> String {
    match status.as_bytes().first().copied() {
        Some(b'A') => "added",
        Some(b'D') => "deleted",
        Some(b'M') => "modified",
        Some(b'R') => "renamed",
        Some(b'C') => "copied",
        Some(b'T') => "type_changed",
        Some(b'U') => "unmerged",
        _ => "unknown",
    }
    .to_string()
}

fn parse_name_status(stdout: &str) -> (Vec<NameStatusRecord>, bool) {
    let mut records = Vec::new();
    let mut partial = false;
    for line in stdout.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        if line == GIT_REVIEW_ERROR_SENTINEL {
            return (Vec::new(), true);
        }
        let mut fields = line.split('\t');
        let Some(status) = fields.next() else {
            partial = true;
            continue;
        };
        let rename_like = status.starts_with('R') || status.starts_with('C');
        let (previous_raw, path_raw) = if rename_like {
            (fields.next(), fields.next())
        } else {
            (None, fields.next())
        };
        let Some(path_raw) = path_raw else {
            partial = true;
            continue;
        };
        let Some(path) = decode_git_quoted_path(path_raw) else {
            records.push(NameStatusRecord {
                status: normalize_status(status),
                path: None,
                previous_path: None,
                path_omitted: true,
            });
            partial = true;
            continue;
        };
        let (path, path_omitted) = bounded_output_path(path);
        let previous_path = previous_raw
            .and_then(decode_git_quoted_path)
            .and_then(|path| bounded_output_path(path).0);
        if rename_like && previous_raw.is_some() && previous_path.is_none() {
            partial = true;
        }
        partial |= path_omitted;
        records.push(NameStatusRecord {
            status: normalize_status(status),
            path,
            previous_path,
            path_omitted,
        });
    }
    (records, partial)
}

fn parse_numstat(stdout: &str) -> (Vec<NumstatRecord>, bool) {
    let mut records = Vec::new();
    let mut partial = false;
    for line in stdout.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        if line == GIT_REVIEW_ERROR_SENTINEL {
            return (Vec::new(), true);
        }
        let mut fields = line.splitn(3, '\t');
        let Some(additions) = fields.next() else {
            partial = true;
            continue;
        };
        let Some(deletions) = fields.next() else {
            partial = true;
            continue;
        };
        let binary = additions == "-" || deletions == "-";
        let additions = if binary { None } else { additions.parse().ok() };
        let deletions = if binary { None } else { deletions.parse().ok() };
        if !binary && (additions.is_none() || deletions.is_none()) {
            partial = true;
        }
        records.push(NumstatRecord {
            additions,
            deletions,
            binary,
        });
    }
    (records, partial)
}

fn parse_raw_modes(stdout: &str) -> (Vec<RawModeRecord>, bool) {
    let mut records = Vec::new();
    let mut partial = false;
    for line in stdout.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        if line == GIT_REVIEW_ERROR_SENTINEL {
            return (Vec::new(), true);
        }
        let Some((meta, _path)) = line.split_once('\t') else {
            partial = true;
            continue;
        };
        let mut fields = meta.split_whitespace();
        let Some(old_mode) = fields.next().and_then(|field| field.strip_prefix(':')) else {
            partial = true;
            continue;
        };
        let Some(new_mode) = fields.next() else {
            partial = true;
            continue;
        };
        records.push(RawModeRecord {
            gitlink: old_mode == "160000" || new_mode == "160000",
        });
    }
    (records, partial)
}

fn path_tokens(path: &str) -> BTreeSet<String> {
    path.to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn path_extension(path: &str) -> &str {
    path.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("")
}

fn is_docs_path(_path: &str, lower: &str) -> bool {
    lower.starts_with("docs/")
        || lower.starts_with("doc/")
        || matches!(
            path_extension(lower),
            "md" | "mdx" | "rst" | "adoc" | "asciidoc"
        )
        || lower.rsplit('/').next().is_some_and(|name| {
            matches!(name, "readme" | "security" | "changelog") || name.starts_with("readme.")
        })
}

fn is_test_path(lower: &str) -> bool {
    let segments: Vec<&str> = lower.split('/').collect();
    segments
        .iter()
        .any(|segment| matches!(*segment, "test" | "tests"))
        || lower.ends_with("_test.rs")
        || lower.ends_with("_tests.rs")
        || lower.contains(".test.")
        || lower.contains(".spec.")
}

fn is_ci_path(lower: &str, tokens: &BTreeSet<String>) -> bool {
    lower.starts_with(".github/")
        || lower.starts_with(".gitlab/")
        || tokens.contains("ci")
        || tokens.contains("workflows")
}

fn contains_any(tokens: &BTreeSet<String>, values: &[&str]) -> bool {
    values.iter().any(|value| tokens.contains(*value))
}

fn classify_path(path: &str) -> Vec<String> {
    let lower = path.to_ascii_lowercase();
    let tokens = path_tokens(path);
    let docs = is_docs_path(path, &lower);
    let test = is_test_path(&lower);
    let ci = is_ci_path(&lower, &tokens);
    let config = contains_any(&tokens, &["config", "configs", "configuration"])
        || matches!(path_extension(&lower), "toml" | "yaml" | "yml")
        || lower.ends_with("compose.json")
        || lower.ends_with("package.json");
    let auth_security = contains_any(
        &tokens,
        &[
            "auth",
            "oauth",
            "security",
            "scope",
            "scopes",
            "permission",
            "permissions",
            "credential",
            "credentials",
            "secret",
            "secrets",
            "token",
            "acl",
        ],
    );
    let protocol_wire = contains_any(
        &tokens,
        &[
            "protocol",
            "wire",
            "schema",
            "schemas",
            "openapi",
            "mcp",
            "rpc",
            "transport",
        ],
    );
    let tool_contract = lower.starts_with("src/tool_runtime/registry/")
        || (tokens.contains("tool")
            && contains_any(
                &tokens,
                &[
                    "call",
                    "catalog",
                    "definition",
                    "definitions",
                    "registry",
                    "schema",
                    "schemas",
                    "spec",
                    "specs",
                ],
            ));
    let persistence_migration = contains_any(
        &tokens,
        &[
            "persistence",
            "migration",
            "migrations",
            "database",
            "db",
            "sql",
            "storage",
        ],
    );
    let cli_api = contains_any(
        &tokens,
        &[
            "cli",
            "http",
            "api",
            "openapi",
            "route",
            "routes",
            "endpoint",
            "endpoints",
        ],
    );
    let execution_runtime = contains_any(
        &tokens,
        &[
            "runner",
            "runtime",
            "job",
            "jobs",
            "execution",
            "lifecycle",
            "process",
            "shell",
            "dispatch",
            "supervisor",
        ],
    );
    let source_ext = matches!(
        path_extension(&lower),
        "rs" | "go"
            | "py"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "java"
            | "kt"
            | "swift"
            | "sh"
            | "ps1"
    );
    let production = !docs
        && !test
        && !ci
        && (source_ext
            || config
            || auth_security
            || protocol_wire
            || tool_contract
            || persistence_migration
            || cli_api
            || execution_runtime);

    let mut classes = Vec::new();
    for (class, enabled) in [
        ("production", production),
        ("test", test),
        ("docs", docs),
        ("ci", ci),
        ("config", config),
        ("auth_security", auth_security),
        ("protocol_wire", protocol_wire),
        ("tool_contract", tool_contract),
        ("persistence_migration", persistence_migration),
        ("cli_api", cli_api),
        ("execution_runtime", execution_runtime),
    ] {
        if enabled {
            classes.push(class.to_string());
        }
    }
    if classes.is_empty() {
        classes.push("unknown".to_string());
    }
    classes
}

fn classify_review_paths(
    status: &str,
    path: Option<&str>,
    previous_path: Option<&str>,
) -> Vec<String> {
    let mut classes = path
        .map(classify_path)
        .unwrap_or_else(|| vec!["unknown".to_string()]);
    if status == "renamed" {
        if let Some(previous_path) = previous_path {
            for class in classify_path(previous_path) {
                if !classes.iter().any(|existing| existing == &class) {
                    classes.push(class);
                }
            }
        }
    }
    if classes.iter().any(|class| class != "unknown") {
        classes.retain(|class| class != "unknown");
    }
    if classes.is_empty() {
        classes.push("unknown".to_string());
    }
    classes
}

fn has_class(file: &ReviewFile, class: &str) -> bool {
    file.classes.iter().any(|value| value == class)
}

fn runtime_config_surface(path: &str, classes: &[String]) -> bool {
    if !classes.iter().any(|class| class == "config") {
        return false;
    }
    let tokens = path_tokens(path);
    contains_any(
        &tokens,
        &[
            "runner", "runtime", "agent", "server", "webcodex", "deploy", "config",
        ],
    )
}

fn symbol_probe_path_eligible(path: &str) -> bool {
    validate_project_relative_path(path).is_ok()
        && !crate::sensitive_paths::is_secret_path(path)
        && !crate::sensitive_paths::is_bulk_skipped_path(path)
}

fn symbol_probe_eligible(file: &ReviewFile) -> bool {
    let Some(path) = file.path.as_deref() else {
        return false;
    };
    if file.binary != Some(false)
        || file.gitlink != Some(false)
        || !symbol_probe_path_eligible(path)
    {
        return false;
    }
    if file.status == "renamed" {
        let Some(previous_path) = file.previous_path.as_deref() else {
            return false;
        };
        if !symbol_probe_path_eligible(previous_path) {
            return false;
        }
    }
    true
}

fn git_review_symbol_command(merge_base: &str, head: &str, paths: &[String]) -> String {
    let joined = paths
        .iter()
        .map(|path| shell_escape_simple(path))
        .collect::<Vec<_>>()
        .join(" ");
    let failure = format!("printf '{}\\n'; exit 0", GIT_REVIEW_ERROR_SENTINEL);
    let isolated_view = committed_git_isolated_view_setup(head, &failure);
    let producer = format!(
        "git --no-pager -c core.quotePath=false -c attr.tree={head_q} diff --no-ext-diff --no-textconv --find-renames --unified=0 --src-prefix=a/ --dst-prefix=b/ {base_q} {head_q} -- {paths}",
        base_q = shell_escape_simple(merge_base),
        head_q = shell_escape_simple(head),
        paths = joined,
    );
    let consumer = format!(
        "{{ head -c {max_bytes}; head_exit=$?; cat >/dev/null; drain_exit=$?; [ \"$head_exit\" -eq 0 ] && [ \"$drain_exit\" -eq 0 ]; }}",
        max_bytes = GIT_REVIEW_MAX_DIFF_BYTES,
    );
    let pipeline = checked_git_pipeline_to_file(&producer, &consumer, "symbols", &failure);
    format!(
        concat!(
            "{prefix}",
            "if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then printf '{error}\\n'; exit 0; fi; ",
            "{isolated_view}",
            "{pipeline}",
            "cat \"$view/symbols.out\" 2>/dev/null || {{ {failure}; }}"
        ),
        prefix = committed_git_discovery_prefix(),
        isolated_view = isolated_view,
        pipeline = pipeline,
        error = GIT_REVIEW_ERROR_SENTINEL,
        failure = failure,
    )
}

fn parse_diff_header_path(line: &str) -> Option<String> {
    let raw = line.strip_prefix("+++ ")?;
    if raw == "/dev/null" {
        return None;
    }
    let decoded = decode_git_quoted_path(raw)?;
    decoded.strip_prefix("b/").map(str::to_string)
}

fn symbol_hint_from_hunk(line: &str) -> Option<String> {
    let rest = line.strip_prefix("@@")?;
    let closing = rest.find("@@")?;
    let context = rest[closing + 2..].trim();
    if context.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "fn ",
        "struct ",
        "enum ",
        "trait ",
        "impl ",
        "type ",
        "mod ",
        "class ",
        "def ",
        "func ",
        "function ",
    ];
    let lower = context.to_ascii_lowercase();
    let mut start = None;
    for keyword in KEYWORDS {
        if let Some(index) = lower.find(keyword) {
            let boundary_ok = index == 0
                || !lower.as_bytes()[index - 1].is_ascii_alphanumeric()
                    && lower.as_bytes()[index - 1] != b'_';
            if boundary_ok && start.is_none_or(|current| index < current) {
                start = Some(index);
            }
        }
    }
    let start = start?;
    let mut hint = context[start..]
        .split(['{', ';'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if hint.is_empty() {
        return None;
    }
    if hint.len() > GIT_REVIEW_MAX_SYMBOL_BYTES {
        let mut end = GIT_REVIEW_MAX_SYMBOL_BYTES;
        while !hint.is_char_boundary(end) {
            end -= 1;
        }
        hint.truncate(end);
    }
    Some(hint)
}

fn collect_symbol_hints(diff: &str, files: &mut [ReviewFile], total_symbols: &mut usize) -> bool {
    if diff.starts_with(GIT_REVIEW_ERROR_SENTINEL) {
        return true;
    }
    let mut path_to_index = BTreeMap::new();
    for (index, file) in files.iter().enumerate() {
        if let Some(path) = file.path.as_ref() {
            path_to_index.insert(path.clone(), index);
        }
    }
    let mut current = None;
    let mut partial = diff.as_bytes().len() >= GIT_REVIEW_MAX_DIFF_BYTES;
    for line in diff.lines() {
        if line.starts_with("+++ ") {
            current =
                parse_diff_header_path(line).and_then(|path| path_to_index.get(&path).copied());
            continue;
        }
        let Some(index) = current else {
            continue;
        };
        if !line.starts_with("@@") {
            continue;
        }
        let Some(hint) = symbol_hint_from_hunk(line) else {
            continue;
        };
        if files[index]
            .symbols
            .iter()
            .any(|existing| existing == &hint)
        {
            continue;
        }
        if files[index].symbols.len() >= GIT_REVIEW_MAX_SYMBOLS_PER_FILE
            || *total_symbols >= GIT_REVIEW_MAX_TOTAL_SYMBOLS
        {
            partial = true;
            continue;
        }
        files[index].symbols.push(hint);
        *total_symbols += 1;
    }
    partial
}

fn push_warning(warnings: &mut Vec<String>, warning: &str) {
    if warnings.len() < GIT_REVIEW_MAX_WARNINGS && !warnings.iter().any(|item| item == warning) {
        warnings.push(warning.to_string());
    }
}

fn coverage_value(observed: bool, partial: bool) -> Value {
    if observed {
        json!(true)
    } else if partial {
        Value::Null
    } else {
        json!(false)
    }
}

fn build_review_signals(files: &[ReviewFile], coverage_partial: bool) -> (Vec<ReviewSignal>, bool) {
    let mut signals = Vec::new();
    let definitions = [
        (
            "auth_or_scope_surface_touched",
            "auth_security",
            "production auth/security/scope path classification",
        ),
        (
            "protocol_or_wire_schema_surface_touched",
            "protocol_wire",
            "production protocol/wire/schema path classification",
        ),
        (
            "tool_schema_or_definition_surface_touched",
            "tool_contract",
            "production runtime tool schema/definition path classification",
        ),
        (
            "persistence_or_migration_surface_touched",
            "persistence_migration",
            "production persistence/migration path classification",
        ),
        (
            "execution_lifecycle_surface_touched",
            "execution_runtime",
            "production execution/runtime lifecycle path classification",
        ),
        (
            "public_cli_or_http_surface_touched",
            "cli_api",
            "production CLI/HTTP/API path classification",
        ),
    ];
    for (name, class, reason) in definitions {
        let mut paths = files
            .iter()
            .filter(|file| has_class(file, "production") && has_class(file, class))
            .filter_map(|file| file.path.clone())
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        if !paths.is_empty() {
            let paths_truncated = paths.len() > GIT_REVIEW_MAX_PATHS_PER_SIGNAL;
            paths.truncate(GIT_REVIEW_MAX_PATHS_PER_SIGNAL);
            signals.push(ReviewSignal {
                name,
                category: "contract_surface",
                reason,
                paths,
                paths_truncated,
            });
        }
    }
    let mut config_paths = files
        .iter()
        .filter(|file| has_class(file, "production"))
        .filter_map(|file| {
            let path = file.path.as_ref()?;
            runtime_config_surface(path, &file.classes).then_some(path.clone())
        })
        .collect::<Vec<_>>();
    config_paths.sort();
    config_paths.dedup();
    if !config_paths.is_empty() {
        let paths_truncated = config_paths.len() > GIT_REVIEW_MAX_PATHS_PER_SIGNAL;
        config_paths.truncate(GIT_REVIEW_MAX_PATHS_PER_SIGNAL);
        signals.push(ReviewSignal {
            name: "runner_or_runtime_config_surface_touched",
            category: "contract_surface",
            reason: "production runtime/runner configuration path classification",
            paths: config_paths,
            paths_truncated,
        });
    }

    let production_changed = files.iter().any(|file| has_class(file, "production"));
    let tests_changed = files.iter().any(|file| has_class(file, "test"));
    let docs_changed = files.iter().any(|file| has_class(file, "docs"));
    if production_changed && !tests_changed && !coverage_partial {
        signals.push(ReviewSignal {
            name: "production_without_test_changes",
            category: "review_coverage",
            reason:
                "production files changed while no test files changed in the exact review range",
            paths: Vec::new(),
            paths_truncated: false,
        });
    }
    let contract_surface = signals
        .iter()
        .any(|signal| signal.category == "contract_surface");
    if contract_surface && !docs_changed && !coverage_partial {
        signals.push(ReviewSignal {
            name: "contract_surface_without_doc_changes",
            category: "review_coverage",
            reason: "a deterministic contract surface was touched while no docs files changed in the exact review range",
            paths: Vec::new(),
            paths_truncated: false,
        });
    }
    let partial = coverage_partial || signals.len() > GIT_REVIEW_MAX_SIGNALS;
    signals.truncate(GIT_REVIEW_MAX_SIGNALS);
    (signals, partial)
}

fn review_files_json(files: &[ReviewFile]) -> Vec<Value> {
    files
        .iter()
        .map(|file| {
            json!({
                "path": file.path,
                "previous_path": file.previous_path,
                "path_omitted": file.path_omitted,
                "status": file.status,
                "additions": file.additions,
                "deletions": file.deletions,
                "binary": file.binary,
                "gitlink": file.gitlink,
                "classes": file.classes,
                "symbols": file.symbols,
                "symbol_inspection": file.symbol_inspection,
            })
        })
        .collect()
}

impl ToolRuntime {
    pub(crate) async fn git_review_summary(
        &self,
        project: String,
        base_commit: String,
        head_commit: String,
    ) -> ToolResult {
        let base = match normalize_exact_commit_id(&base_commit) {
            Ok(value) => value,
            Err(reason) => return git_review_failure(&project, &base_commit, &head_commit, reason),
        };
        let head = match normalize_exact_commit_id(&head_commit) {
            Ok(value) => value,
            Err(reason) => return git_review_failure(&project, &base, &head_commit, reason),
        };
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let scope = match self
            .resolve_committed_git_scope(&resolved.resolved_id, &base, &head)
            .await
        {
            Ok(scope) => scope,
            Err(reason) => return git_review_failure(&project, &base, &head, reason),
        };

        let max_lines = GIT_REVIEW_MAX_FILES;
        let mut observations = Vec::new();
        for mode in ["name_status", "numstat", "raw"] {
            let output = match self
                .run_project_internal_posix_script_capture(
                    &resolved.resolved_id,
                    bounded_review_diff_command(
                        &scope.merge_base,
                        &scope.requested_head,
                        mode,
                        max_lines,
                    ),
                    30,
                    None,
                )
                .await
            {
                Ok(output) if output.exit_code == Some(0) && output.error.is_none() => output,
                _ => {
                    return git_review_failure(
                        &project,
                        &base,
                        &head,
                        "git_diff_metadata_unavailable",
                    )
                }
            };
            if output.stdout.starts_with(GIT_REVIEW_ERROR_SENTINEL) {
                return git_review_failure(&project, &base, &head, "git_diff_failed");
            }
            observations.push(output.stdout);
        }
        let (names, name_partial) = parse_name_status(&observations[0]);
        let (numstats, numstat_partial) = parse_numstat(&observations[1]);
        let (raw_modes, raw_partial) = parse_raw_modes(&observations[2]);

        let files_total = usize::try_from(scope.files_changed).unwrap_or(usize::MAX);
        let mut warnings = Vec::new();
        let files_truncated = names.len() < files_total || files_total > GIT_REVIEW_MAX_FILES;
        let metadata_alignment_complete = !name_partial
            && !numstat_partial
            && !raw_partial
            && names.len() == numstats.len()
            && names.len() == raw_modes.len();
        let file_stats_partial = !metadata_alignment_complete;
        let file_modes_partial = !metadata_alignment_complete;
        let classification_partial = name_partial;
        if files_truncated {
            push_warning(&mut warnings, "changed_file_list_truncated");
        }
        if file_stats_partial {
            push_warning(&mut warnings, "per_file_numstat_partial");
        }
        if file_modes_partial {
            push_warning(&mut warnings, "per_file_mode_metadata_partial");
        }
        if classification_partial {
            push_warning(&mut warnings, "path_classification_partial");
        }

        let mut files = names
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let numstat = metadata_alignment_complete
                    .then(|| numstats.get(index).copied())
                    .flatten();
                let mode = metadata_alignment_complete
                    .then(|| raw_modes.get(index).copied())
                    .flatten();
                let classes = classify_review_paths(
                    &name.status,
                    name.path.as_deref(),
                    name.previous_path.as_deref(),
                );
                ReviewFile {
                    status: name.status,
                    path: name.path,
                    previous_path: name.previous_path,
                    path_omitted: name.path_omitted,
                    additions: numstat.and_then(|value| value.additions),
                    deletions: numstat.and_then(|value| value.deletions),
                    binary: numstat.map(|value| value.binary),
                    gitlink: mode.map(|value| value.gitlink),
                    classes,
                    symbols: Vec::new(),
                    symbol_inspection: "not_attempted",
                }
            })
            .collect::<Vec<_>>();

        let mut symbol_paths = Vec::new();
        let mut sensitive_skipped = 0usize;
        for file in &mut files {
            if symbol_probe_eligible(file) {
                if let Some(path) = file.path.clone() {
                    symbol_paths.push(path);
                    file.symbol_inspection = "inspected";
                }
            } else if file.binary == Some(true) {
                file.symbol_inspection = "skipped_binary";
            } else if file.gitlink == Some(true) {
                file.symbol_inspection = "skipped_gitlink";
            } else if file.status == "renamed" && file.previous_path.is_none() {
                file.symbol_inspection = "skipped_path_unavailable";
            } else if file.binary.is_none() || file.gitlink.is_none() {
                file.symbol_inspection = "skipped_metadata_partial";
            } else if file.path.is_none() {
                file.symbol_inspection = "skipped_path_unavailable";
            } else {
                file.symbol_inspection = "skipped_sensitive_or_excluded";
                sensitive_skipped += 1;
            }
        }
        symbol_paths.sort();
        symbol_paths.dedup();
        let mut total_symbols = 0usize;
        let mut symbols_partial = files_truncated || classification_partial;
        let mut diff_bytes_inspected = 0usize;
        if !symbol_paths.is_empty() {
            let output = self
                .run_project_internal_posix_script_capture(
                    &resolved.resolved_id,
                    git_review_symbol_command(
                        &scope.merge_base,
                        &scope.requested_head,
                        &symbol_paths,
                    ),
                    30,
                    None,
                )
                .await;
            match output {
                Ok(output) if output.exit_code == Some(0) && output.error.is_none() => {
                    diff_bytes_inspected = output
                        .stdout
                        .as_bytes()
                        .len()
                        .min(GIT_REVIEW_MAX_DIFF_BYTES);
                    symbols_partial |=
                        collect_symbol_hints(&output.stdout, &mut files, &mut total_symbols);
                }
                _ => {
                    symbols_partial = true;
                    for file in &mut files {
                        if file.symbol_inspection == "inspected" {
                            file.symbol_inspection = "unavailable";
                        }
                    }
                    push_warning(&mut warnings, "symbol_context_unavailable");
                }
            }
        }
        if sensitive_skipped > 0 {
            push_warning(
                &mut warnings,
                "sensitive_or_excluded_symbol_context_skipped",
            );
        }
        if symbols_partial {
            push_warning(&mut warnings, "symbol_hints_partial");
        }

        let coverage_partial = files_truncated || classification_partial;
        let production_changed = files.iter().any(|file| has_class(file, "production"));
        let tests_changed = files.iter().any(|file| has_class(file, "test"));
        let docs_changed = files.iter().any(|file| has_class(file, "docs"));

        let mut class_counts = BTreeMap::<String, usize>::new();
        for file in &files {
            for class in &file.classes {
                *class_counts.entry(class.clone()).or_default() += 1;
            }
        }

        let subsystem_names = [
            "auth_security",
            "protocol_wire",
            "tool_contract",
            "config",
            "persistence_migration",
            "cli_api",
            "execution_runtime",
        ];
        let mut subsystems = Vec::new();
        for subsystem in subsystem_names {
            let mut paths = files
                .iter()
                .filter(|file| has_class(file, subsystem))
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>();
            paths.sort();
            paths.dedup();
            if paths.is_empty() {
                continue;
            }
            let path_count = paths.len();
            let paths_truncated = path_count > GIT_REVIEW_MAX_PATHS_PER_SIGNAL;
            paths.truncate(GIT_REVIEW_MAX_PATHS_PER_SIGNAL);
            subsystems.push(json!({
                "name": subsystem,
                "path_count_observed": path_count,
                "paths": paths,
                "paths_truncated": paths_truncated,
            }));
        }
        let subsystems_partial = coverage_partial || subsystems.len() > GIT_REVIEW_MAX_SUBSYSTEMS;
        subsystems.truncate(GIT_REVIEW_MAX_SUBSYSTEMS);

        let (signals, signals_partial) = build_review_signals(&files, coverage_partial);
        let signal_json = signals
            .iter()
            .map(|signal| {
                json!({
                    "name": signal.name,
                    "category": signal.category,
                    "reason": signal.reason,
                    "paths": signal.paths,
                    "paths_truncated": signal.paths_truncated,
                })
            })
            .collect::<Vec<_>>();

        let truncated = files_truncated
            || classification_partial
            || file_stats_partial
            || file_modes_partial
            || symbols_partial
            || subsystems_partial
            || signals_partial;
        let payload = json!({
            "project": project,
            "scope": {
                "requested_base": scope.requested_base,
                "requested_head": scope.requested_head,
                "merge_base": scope.merge_base,
                "base_is_ancestor": scope.base_is_ancestor,
                "commit_count": scope.commit_count,
                "diff_range": format!("{}..{}", scope.merge_base, scope.requested_head),
            },
            "stats": {
                "files_changed": scope.files_changed,
                "insertions": scope.insertions,
                "deletions": scope.deletions,
                "binary_files": scope.binary_files,
            },
            "file_classes": {
                "counts_observed": class_counts,
                "partial": coverage_partial,
            },
            "subsystems": subsystems,
            "signals": signal_json,
            "files": review_files_json(&files),
            "coverage": {
                "production_changed": coverage_value(production_changed, coverage_partial),
                "tests_changed": coverage_value(tests_changed, coverage_partial),
                "docs_changed": coverage_value(docs_changed, coverage_partial),
                "partial": coverage_partial,
            },
            "bounds": {
                "max_files": GIT_REVIEW_MAX_FILES,
                "max_path_bytes": GIT_REVIEW_MAX_PATH_BYTES,
                "max_subsystems": GIT_REVIEW_MAX_SUBSYSTEMS,
                "max_signals": GIT_REVIEW_MAX_SIGNALS,
                "max_paths_per_signal": GIT_REVIEW_MAX_PATHS_PER_SIGNAL,
                "max_symbols_per_file": GIT_REVIEW_MAX_SYMBOLS_PER_FILE,
                "max_total_symbols": GIT_REVIEW_MAX_TOTAL_SYMBOLS,
                "max_symbol_bytes": GIT_REVIEW_MAX_SYMBOL_BYTES,
                "max_diff_bytes_inspected": GIT_REVIEW_MAX_DIFF_BYTES,
            },
            "truncation": {
                "files_total": scope.files_changed,
                "files_returned": files.len(),
                "files_truncated": files_truncated,
                "classification_partial": classification_partial,
                "file_stats_partial": file_stats_partial,
                "file_modes_partial": file_modes_partial,
                "symbols_returned": total_symbols,
                "symbols_partial": symbols_partial,
                "diff_bytes_inspected": diff_bytes_inspected,
                "subsystems_partial": subsystems_partial,
                "signals_partial": signals_partial,
            },
            "deterministic": true,
            "llm_summary": false,
            "truncated": truncated,
            "warnings": warnings,
            "reason_code": null,
        });
        let result = ToolResult::ok(payload);
        if serde_json::to_vec(&result)
            .map(|bytes| {
                bytes.len()
                    <= MAX_SERIALIZED_OUTPUT_BYTES
                        .saturating_sub(MODEL_RESULT_ENVELOPE_RESERVE_BYTES)
            })
            .unwrap_or(false)
        {
            result
        } else {
            git_review_failure(&project, &base, &head, "output_budget_exceeded")
        }
    }
}

#[cfg(test)]
use super::git_committed::{
    committed_git_scope_command as git_review_scope_command,
    parse_committed_git_scope as parse_git_review_scope,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_review_summary_exact_commit_validation_is_narrow() {
        let lower = "a".repeat(40);
        assert_eq!(normalize_exact_commit_id(&lower).unwrap(), lower);
        let upper = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        assert_eq!(
            normalize_exact_commit_id(upper).unwrap(),
            upper.to_ascii_lowercase()
        );
        for invalid in [
            "HEAD",
            "HEAD~2",
            "--help",
            "refs/heads/main",
            "abc123",
            "gggggggggggggggggggggggggggggggggggggggg",
        ] {
            assert_eq!(normalize_exact_commit_id(invalid), Err("invalid_commit_id"));
        }
    }

    #[test]
    fn git_review_summary_invalid_commit_failure_does_not_echo_unbounded_input() {
        let malformed = "source-like-invalid-value".repeat(1024);
        let result =
            git_review_failure("project", &malformed, &"A".repeat(40), "invalid_commit_id");
        assert!(!result.success);
        assert!(result.output["scope"]["requested_base"].is_null());
        assert_eq!(result.output["scope"]["requested_head"], "a".repeat(40));
        let serialized = serde_json::to_string(&result.output).unwrap();
        assert!(!serialized.contains("source-like-invalid-value"));
    }

    #[test]
    fn git_review_summary_scope_parser_fails_closed_on_multiple_merge_bases() {
        assert_eq!(
            parse_git_review_scope("status=ambiguous_merge_base\n"),
            Err("ambiguous_merge_base")
        );
        let command = git_review_scope_command(&"a".repeat(40), &"b".repeat(40));
        assert!(command.contains("git merge-base --all"));
        assert!(command.contains("status=ambiguous_merge_base"));
    }

    #[test]
    fn git_review_summary_path_classifier_uses_token_boundaries() {
        let tokenizer = classify_path("src/tokenizer.rs");
        assert!(tokenizer.iter().any(|class| class == "production"));
        assert!(!tokenizer.iter().any(|class| class == "auth_security"));
        let token = classify_path("src/auth/token.rs");
        assert!(token.iter().any(|class| class == "auth_security"));
        let protocol = classify_path("src/protocol.rs");
        assert!(protocol.iter().any(|class| class == "protocol_wire"));
        let test = classify_path("tests/auth.rs");
        assert!(test.iter().any(|class| class == "test"));
        assert!(!test.iter().any(|class| class == "production"));
        let docs = classify_path("docs/AUTH.md");
        assert!(docs.iter().any(|class| class == "docs"));
    }

    #[test]
    fn git_review_summary_checked_pipeline_fences_producer_and_consumer_status() {
        let command = checked_git_pipeline_to_file(
            "git diff --example",
            "awk '{print}'",
            "probe",
            "printf 'failed\\n'; exit 0",
        );
        assert!(command.contains("producer_exit=$?"));
        assert!(command.contains("consumer_exit=$?"));
        assert!(command.contains("$view/probe.status"));
        assert!(command.contains("$view/probe.out"));
        assert!(command.contains("[ \"$producer_exit\" != 0 ]"));
    }

    #[test]
    fn git_review_summary_git_path_decoder_preserves_utf8_spaces_and_tabs() {
        assert_eq!(
            decode_git_quoted_path("space name.rs").unwrap(),
            "space name.rs"
        );
        assert_eq!(decode_git_quoted_path("路径.rs").unwrap(), "路径.rs");
        assert_eq!(
            decode_git_quoted_path("secret key.pem\t").unwrap(),
            "secret key.pem"
        );
        assert_eq!(
            decode_git_quoted_path("\"secret\\tkey.pem\"\t").unwrap(),
            "secret\tkey.pem"
        );
        assert_eq!(
            decode_git_quoted_path("\"tab\\tname.rs\"").unwrap(),
            "tab\tname.rs"
        );
    }

    #[test]
    fn git_review_summary_metadata_parsers_cover_rename_delete_add_binary_and_gitlink() {
        let (names, names_partial) = parse_name_status(
            "R100\told name.rs\tnew name.rs\nD\tdeleted.rs\nA\t路径.rs\nM\t\"tab\\tname.rs\"\n",
        );
        assert!(!names_partial);
        assert_eq!(names.len(), 4);
        assert_eq!(names[0].status, "renamed");
        assert_eq!(names[0].previous_path.as_deref(), Some("old name.rs"));
        assert_eq!(names[0].path.as_deref(), Some("new name.rs"));
        assert_eq!(names[1].status, "deleted");
        assert_eq!(names[2].path.as_deref(), Some("路径.rs"));
        assert_eq!(names[3].path.as_deref(), Some("tab\tname.rs"));

        let (numstats, numstat_partial) =
            parse_numstat("0\t0\told name.rs => new name.rs\n3\t1\tdeleted.rs\n-\t-\tbinary.bin\n");
        assert!(!numstat_partial);
        assert_eq!(numstats[0].additions, Some(0));
        assert_eq!(numstats[1].deletions, Some(1));
        assert!(numstats[2].binary);
        assert_eq!(numstats[2].additions, None);

        let (modes, mode_partial) = parse_raw_modes(
            ":100644 100644 1111111 2222222 M\tsrc/lib.rs\n:160000 160000 3333333 4444444 M\tdeps/sub\n",
        );
        assert!(!mode_partial);
        assert!(!modes[0].gitlink);
        assert!(modes[1].gitlink);
    }

    fn review_file_for_test(path: &str, classes: &[&str]) -> ReviewFile {
        ReviewFile {
            status: "modified".to_string(),
            path: Some(path.to_string()),
            previous_path: None,
            path_omitted: false,
            additions: Some(1),
            deletions: Some(1),
            binary: Some(false),
            gitlink: Some(false),
            classes: classes.iter().map(|class| (*class).to_string()).collect(),
            symbols: Vec::new(),
            symbol_inspection: "not_attempted",
        }
    }

    #[test]
    fn git_review_summary_coverage_hints_require_complete_observation() {
        let production =
            review_file_for_test("src/runtime/job.rs", &["production", "execution_runtime"]);
        let (signals, partial) = build_review_signals(std::slice::from_ref(&production), false);
        assert!(!partial);
        let names = signals
            .iter()
            .map(|signal| signal.name)
            .collect::<BTreeSet<_>>();
        assert!(names.contains("execution_lifecycle_surface_touched"));
        assert!(names.contains("production_without_test_changes"));
        assert!(names.contains("contract_surface_without_doc_changes"));

        let (signals, partial) = build_review_signals(std::slice::from_ref(&production), true);
        assert!(partial);
        let names = signals
            .iter()
            .map(|signal| signal.name)
            .collect::<BTreeSet<_>>();
        assert!(names.contains("execution_lifecycle_surface_touched"));
        assert!(!names.contains("production_without_test_changes"));
        assert!(!names.contains("contract_surface_without_doc_changes"));

        let tests_only = review_file_for_test("tests/auth.rs", &["test", "auth_security"]);
        let (signals, partial) = build_review_signals(&[tests_only], false);
        assert!(!partial);
        assert!(
            signals.is_empty(),
            "tests-only auth path must not claim a production contract surface"
        );
    }

    #[test]
    fn git_review_summary_sensitive_paths_are_metadata_only_for_symbols() {
        let secret = review_file_for_test(".env", &["unknown"]);
        assert!(!symbol_probe_eligible(&secret));
        let ordinary =
            review_file_for_test("src/runtime/job.rs", &["production", "execution_runtime"]);
        assert!(symbol_probe_eligible(&ordinary));
    }

    #[test]
    fn git_review_summary_symbol_hints_are_bounded_and_nonsemantic() {
        assert_eq!(
            symbol_hint_from_hunk("@@ -10 +10 @@ pub(crate) async fn review_target() {").as_deref(),
            Some("fn review_target()")
        );
        assert_eq!(
            symbol_hint_from_hunk("@@ -1 +1 @@ impl ReviewMap {").as_deref(),
            Some("impl ReviewMap")
        );
        assert!(symbol_hint_from_hunk("@@ -1 +1 @@ let token = value;").is_none());
    }
}
