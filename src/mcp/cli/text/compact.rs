//! Tool-specific text layouts. Consume only fields represented in the text or
//! proven redundant for this shape; unknown and exceptional evidence falls back.
use super::{ordered_json, render_fields};
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

pub(super) fn one_line(value: &Value) -> String {
    match value.as_str() {
        Some(s) if !s.chars().any(char::is_control) => s.to_owned(),
        _ => ordered_json(value),
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

/// The declaration span remains in the canonical typed Runner result. The
/// model's next navigation action needs the identifier selection, with nesting.
pub(super) fn symbols(text: &mut String, mut output: Value) {
    fn valid(symbol: &Value) -> bool {
        symbol["name"].is_string()
            && symbol["kind"].is_string()
            && range(&symbol["selection_range"]).is_some()
            && symbol
                .get("children")
                .is_none_or(|v| v.as_array().is_some_and(|a| a.iter().all(valid)))
    }
    fn emit(text: &mut String, mut symbol: Value, depth: usize) -> usize {
        let start = &symbol["selection_range"]["start"];
        let _ = writeln!(
            text,
            "{}{}:{} {} {}",
            "  ".repeat(depth.min(32)),
            start["line"],
            start["column"],
            one_line(&symbol["kind"]),
            one_line(&symbol["name"])
        );
        let children = take(&mut symbol, "children");
        remove(
            &mut symbol,
            &["name", "kind", "kind_code", "range", "selection_range"],
        );
        // Signature/detail is meaningful when the server supplies it.
        render_fields(text, &symbol);
        1 + children
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .map(|child| emit(text, child, depth + 1))
            .sum::<usize>()
    }
    let Some(symbols) = output["symbols"].as_array().filter(|a| a.iter().all(valid)) else {
        render_fields(text, &output);
        return;
    };
    if symbols.is_empty() {
        text.push_str("No symbols.\n");
    }
    if let Some(path) = output.get("path") {
        let _ = writeln!(text, "{}", one_line(path));
    }
    let count: usize = take(&mut output, "symbols")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(|symbol| emit(text, symbol, 0))
        .sum();
    remove(&mut output, &["project", "path", "language"]);
    for key in ["returned_count", "total_count"] {
        omit_equal(&mut output, key, json!(count));
    }
    omit_equal(&mut output, "truncated", json!(false));
    for key in ["external_results_omitted", "invalid_results_omitted"] {
        omit_equal(&mut output, key, json!(0));
    }
    render_fields(text, &output);
}

/// Workspace symbols have a file and an optional location, unlike document
/// symbols' mandatory identifier selection. Never invent a missing position.
pub(super) fn workspace_symbols(text: &mut String, mut output: Value) {
    let Some(symbols) = output["symbols"].as_array().filter(|symbols| {
        symbols.iter().all(|symbol| {
            symbol["path"].is_string()
                && symbol["name"].is_string()
                && symbol["kind"].is_string()
                && symbol
                    .get("range")
                    .is_none_or(|value| value.is_null() || range(value).is_some())
        })
    }) else {
        render_fields(text, &output);
        return;
    };
    let count = symbols.len();
    if count == 0 {
        text.push_str("No symbols.\n");
    }
    for mut symbol in take(&mut output, "symbols")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .cloned()
    {
        let location = match range(&symbol["range"]) {
            Some(position) => format!("{}:{position}", one_line(&symbol["path"])),
            None => format!("{} (position unavailable)", one_line(&symbol["path"])),
        };
        let _ = writeln!(
            text,
            "{location} {} {}",
            one_line(&symbol["kind"]),
            one_line(&symbol["name"])
        );
        // kind_code can name an unknown LSP extension or disagree with kind.
        // Preserve the producer value instead of maintaining another kind map.
        remove(&mut symbol, &["path", "range", "kind", "name"]);
        omit_equal(&mut symbol, "container_name", Value::Null);
        render_fields(text, &symbol);
    }
    remove(&mut output, &["project", "query"]);
    for key in ["returned_count", "total_results"] {
        omit_equal(&mut output, key, json!(count));
    }
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
            if entry["kind"] == "dir" && !entry["path"].as_str().unwrap().ends_with('/') {
                "/"
            } else {
                ""
            }
        );
        if let Some(count) = entry.get("file_count").and_then(Value::as_u64) {
            let _ = write!(text, " ({count} files)");
            remove(&mut entry, &["file_count"]);
        }
        text.push('\n');
        remove(&mut entry, &["path", "kind"]);
        render_fields(text, &entry);
    }
    remove(&mut output, &["project", "path", "source"]);
    for key in ["returned", "total_entries", "total_files"] {
        omit_equal(&mut output, key, json!(count));
    }
    for key in ["truncated", "list_truncated"] {
        omit_equal(&mut output, key, json!(false));
    }
    for key in ["depth", "limit", "next_offset"] {
        omit_equal(&mut output, key, Value::Null);
    }
    omit_equal(&mut output, "offset", json!(0));
    omit_equal(&mut output, "depth_auto", json!(false));
    render_fields(text, &output);
}

pub(super) fn execution(text: &mut String, mut output: Value) {
    if output.as_object().is_some_and(|fields| {
        fields.keys().all(|key| {
            matches!(
                key.as_str(),
                "stdout_tail"
                    | "stderr_tail"
                    | "stdout_truncated"
                    | "stderr_truncated"
                    | "stdout_lines"
                    | "stderr_lines"
            )
        })
    }) && output["stdout_tail"].is_string()
        && output.get("stderr_tail").is_none_or(|v| v == "")
        && ["stdout", "stderr"].iter().all(|stream| {
            output.get(format!("{stream}_lines")).is_none_or(|count| {
                count.as_u64()
                    == Some(
                        output[format!("{stream}_tail")]
                            .as_str()
                            .unwrap_or("")
                            .lines()
                            .count() as u64,
                    )
            })
        })
        && ["stdout_truncated", "stderr_truncated"]
            .iter()
            .all(|key| output.get(key).is_none_or(|v| v == false))
    {
        let stdout = output["stdout_tail"].as_str().unwrap();
        text.push_str(stdout);
        if !stdout.is_empty() && !stdout.ends_with('\n') {
            text.push('\n');
        }
        return;
    }
    // Present lifecycle literally, including outcome_unknown and pending. This
    // renderer never decides whether an execution may be retried.
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
    render_fields(text, &output);
}

pub(super) fn jobs(text: &mut String, mut output: Value) {
    // These are two labels for one exact recovery only when the entire call,
    // including follow_up_kind, agrees. Keep distinct targets/cursors/semantics.
    if let Some(call) = output
        .get("suggested_call")
        .filter(|call| call["tool"].is_string() && call["arguments"].is_object())
    {
        let repeated = output["items"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item.get("continuation") == Some(call)
                    || item["output"].get("continuation") == Some(call)
            })
        });
        if repeated {
            remove(&mut output, &["suggested_call"]);
        }
    }

    if output["items"]
        .as_array()
        .is_some_and(|items| items.len() == 1)
        && output["items"][0].get("output").is_none()
        && matches!(
            output["wait"]["outcome"].as_str(),
            Some("immediate" | "updated" | "terminal" | "timeout")
        )
        && output["wait"].as_object().is_some_and(|wait| {
            wait.keys()
                .all(|k| matches!(k.as_str(), "outcome" | "waited_ms" | "update_kind"))
        })
    {
        let item = &output["items"][0];
        if let (Some(changed), Some(meaningful), Some(heartbeat)) = (
            item["changed"].as_bool(),
            item["meaningful_changed"].as_bool(),
            item["heartbeat_changed"].as_bool(),
        ) {
            if changed == (meaningful || heartbeat) {
                let wait = &output["wait"];
                let outcome = wait["outcome"].as_str().unwrap();
                let waited = wait["waited_ms"].as_u64().unwrap_or(0);
                if outcome != "terminal" {
                    let label = if meaningful {
                        "meaningful"
                    } else if heartbeat {
                        "heartbeat"
                    } else {
                        "none"
                    };
                    let _ = write!(text, "updates: {label}");
                    if waited > 0 {
                        let _ = write!(text, " (waited {waited}ms)");
                    }
                    if outcome == "timeout" {
                        text.push_str("; wait deadline reached");
                    }
                    text.push('\n');
                }
                remove(
                    &mut output["items"][0],
                    &["changed", "meaningful_changed", "heartbeat_changed"],
                );
                remove(&mut output, &["wait"]);
            }
        }
    }
    if let Some(items) = output.get("items").and_then(Value::as_array) {
        let count = items.len();
        // The visible item list proves these totals only for a complete batch.
        // Keep classification/log counts, unequal totals and unknown evidence.
        if output["output_truncated"] == false
            && output["requested_count"].as_u64() == Some(count as u64)
            && output["returned_count"].as_u64() == Some(count as u64)
        {
            remove(&mut output, &["requested_count", "returned_count"]);
        }
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
            if item["success"] == false
                && item["job_id"].is_string()
                && item["error_kind"].is_string()
                && item.get("output") == Some(&Value::Null)
            {
                // No observation payload exists for this failed selector. Its
                // identity, error and any execution/recovery facts remain below.
                remove(&mut item, &["output"]);
                execution(text, item);
                continue;
            }
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

pub(super) fn inventory(text: &mut String, mut output: Value, collection: &str) {
    let Some(items) = output.get(collection).and_then(Value::as_array) else {
        render_fields(text, &output);
        return;
    };
    let count = items.len();
    if count == 0 {
        let _ = writeln!(text, "No {collection}.");
    }
    for mut item in take(&mut output, collection)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .cloned()
    {
        let keys: &[&str] = if collection == "jobs" {
            &["job_id", "status", "command_summary", "exit_code"]
        } else {
            &["project_ref", "id", "name", "path", "agent_status"]
        };
        let mut row = Vec::new();
        for key in keys {
            if let Some(value) = take(&mut item, key).filter(|v| !v.is_null()) {
                row.push(one_line(&value));
            }
        }
        let _ = writeln!(text, "{}", row.join(" | "));
        for key in [
            "project",
            "session_id",
            "started_at",
            "ended_at",
            "exit_code",
            "activity",
            "command_execution_state",
            "validation",
            "build_artifacts",
            "state_changed",
            "failure_kind",
            "recovery_state",
            "recovery_reason",
            "recovery_reason_code",
            "last_update_seq",
        ] {
            omit_equal(&mut item, key, Value::Null);
        }
        if collection == "jobs" {
            remove(
                &mut item,
                &["created_at", "started_at", "ended_at", "duration_ms"],
            );
            omit_equal(&mut item, "command_execution_state", json!("completed"));
        } else {
            remove(
                &mut item,
                &["agent_project_id", "client_id", "executor", "last_seen"],
            );
            for key in ["enabled", "connected"] {
                omit_equal(&mut item, key, json!(true));
            }
            omit_equal(&mut item, "disabled", json!(false));
            omit_equal(&mut item, "active_jobs", json!(0));
            omit_equal(&mut item, "description", json!(""));
            omit_equal(&mut item, "description", Value::Null);
            if item.get("resolved_shell_profile") == Some(&Value::Null)
                && item["shell_profile_status"] == "not_configured"
            {
                remove(
                    &mut item,
                    &["resolved_shell_profile", "shell_profile_status"],
                );
            }
        }
        render_fields(text, &item);
    }
    // Paginated Project discovery states both the page and total counts.
    if collection != "projects" || output.get("next_offset").is_none() {
        for key in ["count", "matched_count"] {
            omit_equal(&mut output, key, json!(count));
        }
    }
    omit_equal(&mut output, "truncated", json!(false));
    omit_equal(&mut output, "recommended_for_smoke", json!([]));
    render_fields(text, &output);
}
