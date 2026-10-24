//! Tool-specific text layouts. Consume only fields represented in the text or
//! proven redundant for this shape; unknown and exceptional evidence falls back.
use super::{render_fields, scalar};
use serde_json::{json, Value};
use std::fmt::Write;

fn remove(value: &mut Value, keys: &[&str]) {
    if let Some(fields) = value.as_object_mut() {
        for key in keys {
            fields.remove(*key);
        }
    }
}

fn omit_equal(value: &mut Value, key: &str, expected: Value) {
    if value.get(key) == Some(&expected) {
        remove(value, &[key]);
    }
}

fn take(value: &mut Value, key: &str) -> Option<Value> {
    value.as_object_mut()?.remove(key)
}

fn one_line(value: &Value) -> String {
    match value.as_str() {
        Some(s) if !s.chars().any(char::is_control) => s.to_owned(),
        _ => value.to_string(),
    }
}

fn range(value: &Value) -> Option<String> {
    if value.as_object()?.len() != 2
        || value["start"].as_object()?.len() != 2
        || value["end"].as_object()?.len() != 2
    {
        return None;
    }
    let sl = value["start"]["line"].as_u64()?;
    let sc = value["start"]["column"].as_u64()?;
    let el = value["end"]["line"].as_u64()?;
    let ec = value["end"]["column"].as_u64()?;
    Some(if (sl, sc) == (el, ec) {
        format!("{sl}:{sc}")
    } else {
        format!("{sl}:{sc}-{el}:{ec}")
    })
}

pub(super) fn locations(text: &mut String, mut output: Value) {
    let Some(items) = output["locations"].as_array() else {
        render_fields(text, &output);
        return;
    };
    if !items
        .iter()
        .all(|item| item["path"].is_string() && range(&item["range"]).is_some())
    {
        render_fields(text, &output);
        return;
    }
    let count = items.len();
    if count == 0 {
        text.push_str("No locations.\n");
    }
    for mut item in take(&mut output, "locations")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .cloned()
    {
        let _ = writeln!(
            text,
            "{}:{}",
            one_line(&item["path"]),
            range(&item["range"]).unwrap()
        );
        remove(&mut item, &["path", "range"]);
        if let Some(target) = item.get("target_range").and_then(range) {
            let _ = writeln!(text, "  target_range: {target}");
            remove(&mut item, &["target_range"]);
        }
        render_fields(text, &item);
    }
    remove(&mut output, &["project", "path", "query_position"]);
    omit_equal(&mut output, "returned_count", json!(count));
    omit_equal(&mut output, "total_results", json!(count));
    omit_equal(&mut output, "truncated", json!(false));
    for key in ["external_results_omitted", "invalid_results_omitted"] {
        omit_equal(&mut output, key, json!(0));
    }
    render_fields(text, &output);
}

pub(super) fn files(text: &mut String, mut output: Value) {
    let Some(entries) = output["entries"].as_array() else {
        render_fields(text, &output);
        return;
    };
    if !entries
        .iter()
        .all(|e| e["path"].is_string() && matches!(e["kind"].as_str(), Some("file" | "dir")))
    {
        render_fields(text, &output);
        return;
    }
    let count = entries.len();
    if count == 0 {
        text.push_str("No files.\n");
    }
    for mut entry in take(&mut output, "entries")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .cloned()
    {
        let _ = write!(
            text,
            "{}{}",
            one_line(&entry["path"]),
            if entry["kind"] == "dir" { "/" } else { "" }
        );
        if let Some(count) = entry.get("file_count").and_then(Value::as_u64) {
            let _ = write!(text, " ({count} files)");
            remove(&mut entry, &["file_count"]);
        }
        text.push('\n');
        remove(&mut entry, &["path", "kind"]);
        render_fields(text, &entry);
    }
    remove(&mut output, &["project", "path"]);
    for key in ["returned", "total_entries"] {
        omit_equal(&mut output, key, json!(count));
    }
    for key in ["truncated", "list_truncated"] {
        omit_equal(&mut output, key, json!(false));
    }
    omit_equal(&mut output, "next_offset", Value::Null);
    omit_equal(&mut output, "offset", json!(0));
    render_fields(text, &output);
}

pub(super) fn execution(text: &mut String, mut output: Value) {
    // Present lifecycle literally, including outcome_unknown and pending. This
    // renderer never decides whether an execution may be retried.
    // Drop lifecycle echoes only when the producer's facts agree. Uncertain,
    // pending, malformed and contradictory receipts retain their evidence.
    if output["execution_state"] == "completed"
        && output["command_started"] == true
        && output["command_completed"] == true
        && output["terminal"] == true
        && output["promoted_to_job"] == false
        && output["job_id"].is_null()
        && matches!(
            output["failure_kind"].as_str(),
            Some("command_exit_nonzero") | None
        )
        && output["exit_code"].is_i64()
    {
        remove(
            &mut output,
            &[
                "command_started",
                "command_completed",
                "terminal",
                "promoted_to_job",
            ],
        );
        for key in ["job_id", "job_status", "observation_token"] {
            omit_equal(&mut output, key, Value::Null);
        }
        omit_equal(&mut output, "tool_failure", json!(false));
        omit_equal(&mut output, "stdout_lines", json!(0));
    }
    if let Some(lifecycle) = output["status"]
        .as_str()
        .and_then(|s| webcodex_core::runner_job_lifecycle::RunnerJobLifecycle::from_wire(s).ok())
    {
        omit_equal(&mut output, "terminal", json!(lifecycle.is_terminal()));
    }
    if let (Some(changed), Some(meaningful), Some(heartbeat)) = (
        output["changed"].as_bool(),
        output["meaningful_changed"].as_bool(),
        output["heartbeat_changed"].as_bool(),
    ) {
        if changed == (meaningful || heartbeat) {
            let _ = writeln!(
                text,
                "updates: {}",
                match (meaningful, heartbeat) {
                    (true, true) => "meaningful + heartbeat",
                    (true, false) => "meaningful",
                    (false, true) => "heartbeat",
                    (false, false) => "none",
                }
            );
            remove(
                &mut output,
                &["changed", "meaningful_changed", "heartbeat_changed"],
            );
        }
    }
    let mut header = Vec::new();
    for key in [
        "job_id",
        "status",
        "execution_state",
        "command_execution_state",
        "outcome",
    ] {
        if let Some(value) = output.get(key).filter(|value| value.is_string()).cloned() {
            header.push(format!("{key}: {}", one_line(&value)));
            remove(&mut output, &[key]);
        }
    }
    if let Some(code) = output.get("exit_code").filter(|v| v.is_i64()).cloned() {
        header.push(format!("exit_code: {code}"));
        remove(&mut output, &["exit_code"]);
    }
    if !header.is_empty() {
        let _ = writeln!(text, "{}", header.join("; "));
    }
    for key in ["stdout_tail", "stderr_tail"] {
        if let Some(log) = output.get(key).and_then(Value::as_str).map(str::to_owned) {
            if !log.is_empty() {
                let _ = writeln!(text, "{key}:\n{log}");
            }
            remove(&mut output, &[key]);
        }
    }
    // These flags describe log loss; only a known false is redundant.
    for key in [
        "stdout_truncated",
        "stderr_truncated",
        "stdout_delta_reset",
        "stderr_delta_reset",
        "earlier_stdout_unavailable",
        "earlier_stderr_unavailable",
    ] {
        omit_equal(&mut output, key, json!(false));
    }
    render_fields(text, &output);
}

pub(super) fn jobs(text: &mut String, mut output: Value) {
    if let Some(items) = output.get("items").and_then(Value::as_array) {
        let count = items.len();
        if count == 0 {
            text.push_str("No Jobs.\n");
        }
        for mut item in take(&mut output, "items")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .cloned()
        {
            // Canonical partial batches wrap each observation; sparse batches
            // already expose its fields. Never flatten away a failed item.
            if let Some(mut observation) = take(&mut item, "output") {
                if observation["job_id"] == item["job_id"] && item["job_id"].is_string() {
                    remove(&mut item, &["job_id"]);
                }
                execution(text, std::mem::take(&mut observation));
                omit_equal(&mut item, "success", json!(true));
                omit_equal(&mut item, "error", Value::Null);
                omit_equal(&mut item, "error_kind", Value::Null);
                render_fields(text, &item);
            } else {
                execution(text, item);
            }
        }
    }
    if let Some(ready) = output.get("ready").and_then(Value::as_array) {
        if ready.is_empty() {
            text.push_str("No ready Jobs.\n");
        }
        for item in take(&mut output, "ready")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .cloned()
        {
            execution(text, item);
        }
    }
    if output
        .get("pending_job_ids")
        .and_then(Value::as_array)
        .is_some_and(|ids| ids.iter().all(Value::is_string))
    {
        let ids = take(&mut output, "pending_job_ids").unwrap();
        let ids = ids.as_array().unwrap();
        let _ = writeln!(
            text,
            "pending: {}",
            if ids.is_empty() {
                "none".to_owned()
            } else {
                ids.iter().map(one_line).collect::<Vec<_>>().join(", ")
            }
        );
    }
    render_fields(text, &output);
}

pub(super) fn changes(text: &mut String, mut output: Value) {
    if let Some(clean) = output["clean"].as_bool() {
        let _ = writeln!(
            text,
            "Workspace {}.",
            if clean { "clean" } else { "has changes" }
        );
        remove(&mut output, &["clean", "project"]);
        if output["upstream_status"] == "observed" && output["upstream"].is_string() {
            let _ = writeln!(
                text,
                "upstream: {}; ahead {}; behind {}",
                one_line(&output["upstream"]),
                output["ahead"],
                output["behind"]
            );
            remove(
                &mut output,
                &["upstream_status", "upstream", "ahead", "behind"],
            );
            omit_equal(&mut output, "upstream_reason_code", Value::Null);
        }
        if clean {
            for key in [
                "files",
                "hunks",
                "untracked_previews",
                "suggested_next_actions",
            ] {
                omit_equal(&mut output, key, json!([]));
            }
            if output["counts"]
                .as_object()
                .is_some_and(|counts| counts.values().all(|v| v.as_u64() == Some(0)))
            {
                remove(&mut output, &["counts"]);
            }
            omit_equal(&mut output, "session", Value::Null);
            omit_equal(&mut output, "diff_stat", json!(""));
        }
    }
    render_fields(text, &output);
}

pub(super) fn edits(text: &mut String, mut output: Value) {
    let Some(files) = output["files"].as_array() else {
        render_fields(text, &output);
        return;
    };
    if !files
        .iter()
        .all(|f| f["path"].is_string() && f["kind"].is_string())
    {
        render_fields(text, &output);
        return;
    }
    let count = files.len();
    let dry_run = output["dry_run"] == true;
    if dry_run && (output["changed"] == true || output["state_changed"] == true) {
        render_fields(text, &output);
        return;
    }
    let aggregates: Vec<_> = ["changed", "would_change"]
        .into_iter()
        .filter_map(|key| {
            files
                .iter()
                .map(|f| f[key].as_bool())
                .collect::<Option<Vec<_>>>()
                .map(|values| (key, values.into_iter().any(|v| v)))
        })
        .collect();
    for (key, expected) in aggregates {
        omit_equal(&mut output, key, json!(expected));
    }
    if dry_run {
        text.push_str("Dry run; no files written.\n");
    }
    for (index, mut file) in take(&mut output, "files")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .enumerate()
    {
        omit_equal(&mut file, "index", json!(index));
        let _ = write!(
            text,
            "{} {}",
            scalar(&file["kind"]),
            one_line(&file["path"])
        );
        if let Some(target) = file.get("to_path").filter(|v| v.is_string()) {
            let _ = write!(text, " -> {}", one_line(target));
        }
        if let Some(revision) = file.get("read_revision").and_then(Value::as_u64) {
            let _ = write!(text, "; read_revision {revision}");
            remove(&mut file, &["read_revision"]);
        }
        if let Some(changed) = file
            .get(if dry_run { "would_change" } else { "changed" })
            .and_then(Value::as_bool)
        {
            let _ = write!(
                text,
                "; {}",
                if changed {
                    if dry_run {
                        "would change"
                    } else {
                        "changed"
                    }
                } else {
                    "unchanged"
                }
            );
            remove(
                &mut file,
                &[if dry_run { "would_change" } else { "changed" }],
            );
            if dry_run {
                omit_equal(&mut file, "changed", json!(false));
            } else {
                omit_equal(&mut file, "would_change", json!(changed));
            }
        }
        text.push('\n');
        remove(&mut file, &["kind", "path"]);
        if let Some(edits) = file.get("edits").and_then(Value::as_array) {
            if edits.iter().all(|e| {
                e["kind"].is_string()
                    && e["old_start_line"].is_u64()
                    && e["old_end_line"].is_u64()
                    && e["new_line_count"].is_u64()
            }) {
                for (index, mut edit) in take(&mut file, "edits")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned()
                    .enumerate()
                {
                    omit_equal(&mut edit, "index", json!(index));
                    let _ = writeln!(
                        text,
                        "  {} lines {}-{} -> {} lines",
                        scalar(&edit["kind"]),
                        edit["old_start_line"],
                        edit["old_end_line"],
                        edit["new_line_count"]
                    );
                    remove(
                        &mut edit,
                        &["kind", "old_start_line", "old_end_line", "new_line_count"],
                    );
                    render_fields(text, &edit);
                }
            }
        }
        if file["to_path"].is_string() || file["to_path"].is_null() {
            remove(&mut file, &["to_path"]);
        }
        omit_equal(&mut file, "edits", json!([]));
        render_fields(text, &file);
    }
    remove(&mut output, &["project"]);
    if dry_run {
        remove(&mut output, &["dry_run"]);
        omit_equal(&mut output, "changed", json!(false));
        omit_equal(&mut output, "state_changed", json!(false));
        omit_equal(&mut output, "applied_count", json!(0));
    }
    omit_equal(&mut output, "planned_count", json!(count));
    omit_equal(&mut output, "execution_state", json!("completed"));
    omit_equal(&mut output, "changed_paths", json!([]));
    if let Some(summary) = output.get_mut("change_summary") {
        omit_equal(summary, "requested_changes", json!(count));
        omit_equal(summary, "warnings", json!(0));
        if dry_run {
            omit_equal(summary, "changed_files", json!(0));
        }
    }
    render_fields(text, &output);
}
