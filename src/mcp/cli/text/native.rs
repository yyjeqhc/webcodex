//! Explicit model-facing projections for the direct execution surface. Runtime
//! facts remain canonical; new telemetry fields do not automatically cost tokens.
use super::{compact::one_line, ordered_json};
use serde_json::{json, Value};
use std::fmt::Write;

fn selected(text: &mut String, value: &Value, keys: &[&str]) {
    for key in keys {
        if let Some(value) = value
            .get(*key)
            .filter(|v| !v.is_null() && **v != json!([]) && **v != json!({}) && **v != "")
        {
            let _ = writeln!(
                text,
                "{key}: {}",
                if value.is_string() {
                    one_line(value)
                } else {
                    ordered_json(value)
                }
            );
        }
    }
}

fn has_next(value: &Value) -> bool {
    ["continuation", "suggested_call", "recovery"]
        .iter()
        .any(|key| {
            let call = &value[*key];
            (call["tool"].is_string() && call["arguments"].is_object()) || has_nested_next(call)
        })
}

fn has_nested_next(value: &Value) -> bool {
    value.get("suggested_call").is_some() && has_next(value)
}

fn recovery(text: &mut String, value: &Value) {
    for key in ["continuation", "suggested_call", "recovery"] {
        if let Some(call) = value.get(key) {
            if let (Some(tool), Some(args)) = (call["tool"].as_str(), call["arguments"].as_object())
            {
                let _ = write!(text, "next: {tool}");
                let mut args: Vec<_> = args.iter().collect();
                args.sort_unstable_by_key(|(key, _)| *key);
                for (key, value) in args {
                    // CLI's canonical observation default is terminal. Keep all
                    // nondefaults and exact selectors, tokens, bounds and fences.
                    if tool == "observe_jobs" && key == "wake_on" && value == "terminal" {
                        continue;
                    }
                    let _ = write!(text, " {key}={}", ordered_json(value));
                }
                text.push('\n');
            } else if call.get("suggested_call").is_some() {
                recovery(text, call);
            } else {
                selected(text, value, &[key]);
            }
        }
    }
    selected(
        text,
        value,
        &["recovery_state", "recovery_reason", "retry_guidance"],
    );
}

fn exceptional(text: &mut String, value: &Value) {
    selected(
        text,
        value,
        &[
            "error",
            "error_kind",
            "failure_kind",
            "reason_code",
            "recovery_reason_code",
            "recovery_kind",
            "detail",
            "expected_read_revision",
            "candidate_ranges",
            "candidates",
            "conflicting_edit_ranges",
            "change_index",
            "edit_index",
            "failed_kind",
            "path",
            "warnings",
            "conflicts",
            "uncertain_paths",
            "rollback_errors",
            "partial_apply",
            "state_changed",
            "execution_state",
        ],
    );
    for key in [
        "candidates_truncated",
        "output_truncated",
        "truncated",
        "earlier_stdout_unavailable",
        "earlier_stderr_unavailable",
        "stdout_delta_reset",
        "stderr_delta_reset",
    ] {
        if value[key] == true {
            let _ = writeln!(text, "{key}: true");
        }
    }
    selected(
        text,
        value,
        &[
            "next_index",
            "next_start_line",
            "next_offset",
            "omitted_queries",
            "budget_limited",
            "omitted_count",
            "timeout",
            "retained_log_limit",
        ],
    );
    recovery(text, value);
}

fn execution(text: &mut String, value: &Value) {
    let details = value.get("details").unwrap_or(value);
    let status = details["status"]
        .as_str()
        .or_else(|| details["execution_state"].as_str())
        .or_else(|| details["command_execution_state"].as_str());
    if let Some(status) = status {
        let _ = write!(text, "{status}");
    }
    if let Some(code) = details["exit_code"].as_i64() {
        let _ = write!(
            text,
            "{}exit_code: {code}",
            if status.is_some() { "; " } else { "" }
        );
    }
    if status.is_some() || details["exit_code"].is_number() {
        text.push('\n');
    }
    for (key, label) in [("stdout_tail", "stdout"), ("stderr_tail", "stderr")] {
        if let Some(log) = details[key].as_str().filter(|s| !s.is_empty()) {
            let _ = write!(text, "{label}:\n{log}");
            if !log.ends_with('\n') {
                text.push('\n');
            }
        }
    }
    for key in ["stdout_truncated", "stderr_truncated"] {
        if details[key] == true {
            let _ = writeln!(text, "{key}: true");
        }
    }
    if !has_next(details) && !has_next(value) {
        selected(text, details, &["job_id", "observation_token"]);
    }
    if details["log_delta_status"] == "reset"
        && details["stdout_delta_reset"] != true
        && details["stderr_delta_reset"] != true
    {
        selected(text, details, &["log_delta_status"]);
    }
    if details["state_changed"].is_null()
        && details.get("state_changed").is_some()
        && (details["failure_kind"] == "outcome_unknown" || status == Some("outcome_unknown"))
    {
        text.push_str("state_changed: unknown\n");
    }
    if details["cwd"].as_str().is_some_and(|cwd| cwd != ".") {
        selected(text, details, &["cwd"]);
    }
    if details["command_execution_state"]
        .as_str()
        .is_some_and(|state| Some(state) != status)
    {
        selected(text, details, &["command_execution_state"]);
    }
    // A timeout is not proof that effects ended. Never offer a fresh execution
    // or a different execution tool as a way to recover missing output.
    if matches!(status, Some("timeout" | "timed_out" | "outcome_unknown"))
        || details["failure_kind"] == "timeout"
        || details["command_execution_state"] == "timed_out"
    {
        text.push_str(if details["job_id"].is_string() {
            "Effects may be incomplete or unknown; inspect the retained Job and target state before retrying.\n"
        } else {
            "Effects may be incomplete or unknown; inspect target state before retrying.\n"
        });
        if details["failure_kind"] == "timeout" || details["command_execution_state"] == "timed_out"
        {
            if let Some(seconds) = details["effective_timeout_secs"].as_u64() {
                let _ = writeln!(text, "timeout: {seconds}s");
            }
        }
    }
    let mut evidence = details.clone();
    if let Some(fields) = evidence.as_object_mut() {
        fields.remove("execution_state");
        fields.remove("retry_guidance");
        if fields
            .get("failure_kind")
            .is_some_and(|v| v == "command_exit_nonzero")
        {
            fields.remove("failure_kind");
        }
        if matches!(status, Some("completed" | "failed")) {
            fields.remove("state_changed");
        }
    }
    exceptional(text, &evidence);
    if details != value {
        recovery(text, value);
    }
}

pub(super) fn reads(text: &mut String, value: &Value) {
    if let Some(items) = value["items"].as_array() {
        for item in items {
            let read = &item["output"];
            let path = one_line(&item["path"]);
            if super::ordinary_read_failure(item) {
                let _ = writeln!(text, "{path}: {}", one_line(&item["error"]));
                continue;
            }
            if let Some(source) = read["text"].as_str() {
                let _ = write!(text, "==> {path}");
                if let Some(start) = read["start_line"].as_u64() {
                    let _ = write!(
                        text,
                        ":{start}-{}",
                        read["end_line"].as_u64().unwrap_or(start)
                    );
                }
                if let Some(total) = read["total_lines"].as_u64() {
                    let _ = write!(text, " of {total}");
                }
                if let Some(rev) = read["read_revision"].as_u64() {
                    let _ = write!(text, " (read_revision {rev})");
                }
                text.push('\n');
                text.push_str(source);
                if !source.ends_with('\n') {
                    text.push('\n');
                }
            } else {
                let _ = writeln!(text, "==> {path}");
                selected(text, item, &["error"]);
            }
            if read["has_more"] == true {
                text.push_str("more lines available\n");
            }
            let mut evidence = read.clone();
            if let Some(fields) = evidence.as_object_mut() {
                fields.remove("path");
            }
            exceptional(text, &evidence);
        }
    }
    exceptional(text, value);
}

fn edits(text: &mut String, value: &Value) {
    if value["dry_run"] == true {
        text.push_str("Dry run; no files written.\n");
    }
    if let Some(files) = value["files"].as_array() {
        for file in files {
            let _ = write!(
                text,
                "{} {}",
                one_line(&file["kind"]),
                one_line(&file["path"])
            );
            if let Some(path) = file.get("to_path").filter(|v| v.is_string()) {
                let _ = write!(text, " -> {}", one_line(path));
            }
            if let Some(edits) = file["edits"].as_array().filter(|a| !a.is_empty()) {
                let _ = write!(text, " ({} edits", edits.len());
                for edit in edits {
                    let _ = write!(
                        text,
                        ", lines {}-{}",
                        edit["old_start_line"], edit["old_end_line"]
                    );
                    if let Some(n) = edit["match_count"].as_u64() {
                        let _ = write!(text, ", match_count: {n}");
                    }
                    if edit["match_ranges_truncated"] == true {
                        text.push_str(", match_ranges_truncated: true");
                    }
                    if let Some(warning) = edit["warning"].as_str() {
                        let _ = write!(text, ", {warning}");
                    }
                }
                text.push(')');
            }
            let changed = if value["dry_run"] == true {
                "would_change"
            } else {
                "changed"
            };
            if file[changed] == false {
                text.push_str("; unchanged");
            } else if value["dry_run"] != true {
                text.push_str("; changed");
            }
            if let Some(rev) = file["read_revision"].as_u64() {
                let _ = write!(text, "; read_revision {rev}");
            }
            text.push('\n');
            selected(text, file, &["warnings"]);
        }
    }
    let mut evidence = value.clone();
    if value["files"].is_array() && value["error_kind"].is_null() {
        if let Some(fields) = evidence.as_object_mut() {
            fields.remove("execution_state");
            fields.remove("state_changed");
        }
    }
    exceptional(text, &evidence);
}

fn workspace(text: &mut String, success: bool, value: &Value) {
    if let Some(branch) = value["branch"].as_str() {
        let _ = writeln!(text, "## {branch}");
    }
    if value["clean"] == true {
        text.push_str("Workspace clean.\n");
    }
    if let Some(files) = value["files"].as_array() {
        for file in files {
            let status = match file["status"].as_str() {
                Some("untracked") => "??",
                Some("modified") => "M",
                Some("added") => "A",
                Some("deleted") => "D",
                Some("renamed") => "R",
                Some("copied") => "C",
                Some("conflicted") => "conflict",
                Some(other) => other,
                None => "?",
            };
            let _ = write!(text, "{status} ");
            if let Some(old) = file["old_path"].as_str() {
                let _ = write!(text, "{old} -> ");
            }
            let _ = write!(text, "{}", one_line(&file["path"]));
            if file["staged"] == true {
                text.push_str(if file["unstaged"] == true {
                    " (staged + unstaged)"
                } else {
                    " (staged)"
                });
            }
            text.push('\n');
        }
    }
    if let Some(stat) = value["diff_stat"].as_str().filter(|s| !s.is_empty()) {
        text.push_str(stat);
        if !stat.ends_with('\n') {
            text.push('\n');
        }
    }
    selected(text, value, &["hunks", "untracked_previews", "upstream"]);
    for key in ["ahead", "behind"] {
        if value[key].as_u64().is_some_and(|n| n > 0) {
            selected(text, value, &[key]);
        }
    }
    for key in ["status_observation", "diff_status", "diff_stat_status"] {
        let status = value[key]["status"].as_str();
        let failed = if key == "status_observation" {
            value[key].is_object() && status != Some("observed")
        } else {
            let requested = if key == "diff_status" {
                value["hunks"].is_array()
            } else {
                value["non_git_project"] != true
            };
            status == Some("command_failed")
                || (requested && !success && status == Some("output_unavailable"))
        };
        if failed {
            selected(text, value, &[key]);
        }
    }
    selected(text, value, &["git_error", "truncation_reasons"]);
    for key in ["files_truncated", "hunks_truncated"] {
        if value[key] == true {
            let _ = writeln!(text, "{key}: true");
        }
    }
    let mut evidence = value.clone();
    if let Some(warnings) = evidence["warnings"].as_array_mut() {
        warnings.retain(|warning| {
            warning["kind"] != "untracked_file"
                || !value["files"].as_array().is_some_and(|files| {
                    files.iter().any(|file| {
                        file["status"] == "untracked" && file["path"] == warning["path"]
                    })
                })
        });
    }
    exceptional(text, &evidence);
}

pub(super) fn render(tool: &str, success: bool, error: &str, value: &Value) -> Option<String> {
    if !matches!(
        tool,
        "run_shell"
            | "run_process"
            | "run_script"
            | "run_detached_process"
            | "observe_jobs"
            | "stop_job"
            | "read_files"
            | "edit_project_files"
            | "read_workspace_changes"
            | "search_project_texts"
    ) {
        return None;
    }
    let mut text = String::new();
    if !success {
        let details = value.get("details").unwrap_or(value);
        if matches!(tool, "run_shell" | "run_process" | "run_script")
            && details["exit_code"].is_number()
        {
            text.push_str("ERROR: command failed\n");
        } else if tool == "edit_project_files"
            && value["error_kind"] == "path_policy_rejected"
            && value["execution_state"] == "not_started"
            && value["state_changed"] == false
        {
            text.push_str("ERROR: Path resolves outside the Project or allowed roots. No files were modified. Choose a path inside those roots.\n");
        } else {
            let _ = writeln!(text, "ERROR: {error}");
        }
    }
    match tool {
        "run_shell" | "run_process" | "run_script" | "run_detached_process" => {
            execution(&mut text, value)
        }
        "observe_jobs" => {
            if let Some(items) = value["items"].as_array() {
                for item in items {
                    if let Some(output) = item.get("output").filter(|v| v.is_object()) {
                        execution(&mut text, output);
                    } else {
                        execution(&mut text, item);
                    }
                }
            }
            if value["wait"]["outcome"] == "timeout" {
                text.push_str("Wait deadline reached; Job may still be running.\n");
            }
            let mut evidence = value.clone();
            super::compact::deduplicate_job_recovery(&mut evidence);
            exceptional(&mut text, &evidence);
        }
        "read_files" => reads(&mut text, value),
        "edit_project_files" => edits(&mut text, value),
        "stop_job" => {
            selected(&mut text, value, &["job_id", "stop_effect", "final_status"]);
            // A missing Workflow Session is normal for execution-only callers.
            let mut evidence = value.clone();
            if let Some(warnings) = evidence["warnings"].as_array_mut() {
                warnings.retain(|w| w["warning_kind"] != "job_session_unknown");
            }
            exceptional(&mut text, &evidence);
        }
        "read_workspace_changes" => workspace(&mut text, success, value),
        "search_project_texts" => {
            // The existing layouts own matches/context/count modes. Only the
            // declared content and incomplete-coverage contract reaches them.
            fn project(v: &Value) -> Value {
                let keys = [
                    "index",
                    "success",
                    "error",
                    "error_kind",
                    "reason_code",
                    "failure_stage",
                    "detail_code",
                    "detail",
                    "matches",
                    "files",
                    "result_mode",
                    "truncated",
                    "output_truncated",
                    "limit",
                    "next_index",
                    "omitted_queries",
                    "warnings",
                    "continuation",
                    "retry_guidance",
                    "total_matches",
                    "match_count",
                    "matched_file_count",
                    "omitted_match_count",
                    "coverage",
                    "incomplete_reasons",
                    "suggested_call",
                    "zero_match_hint",
                    "candidates",
                    "candidates_truncated",
                    "truncation_reason",
                ];
                let Some(map) = v.as_object() else {
                    return v.clone();
                };
                let mut selected = serde_json::Map::new();
                for (key, value) in map {
                    if keys.contains(&key.as_str()) && !value.is_null() && value != &json!(false) {
                        selected.insert(key.clone(), value.clone());
                    }
                }
                if let Some(items) = map.get("items").and_then(Value::as_array) {
                    selected.insert(
                        "items".into(),
                        Value::Array(items.iter().map(project).collect()),
                    );
                }
                if let Some(output) = map.get("output") {
                    selected.insert("output".into(), project(output));
                }
                Value::Object(selected)
            }
            super::render_search_batch(&mut text, project(value));
        }
        _ => unreachable!(),
    }
    if text.is_empty() {
        text.push_str("OK\n");
    }
    Some(text)
}
