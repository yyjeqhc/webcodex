//! Render already-bounded canonical observations without escaping source text.
//! Only known presentation redundancy is removed. Unknown fields, failures,
//! revision fences, incomplete coverage and exact recovery calls survive.
mod compact;

use serde_json::{json, Value};
use std::fmt::Write;

pub(in crate::mcp) fn project_response(tool: Option<&str>, body: &mut Value) {
    let Some(tool) = tool else { return };
    // Third-party providers own their content blocks and failure semantics.
    if matches!(tool, "mcp_tool" | "plugin_tool" | "ssh_resource") {
        return;
    }
    let Some(result) = body.get_mut("result").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(structured) = result.get("structuredContent") else {
        return;
    };
    let Some(success) = structured.get("success").and_then(Value::as_bool) else {
        return;
    };
    let mut output = structured.get("output").cloned().unwrap_or(Value::Null);
    let mut text = String::new();
    if !success {
        let _ = writeln!(
            text,
            "ERROR: {}",
            execution_error_text(
                tool,
                structured["error"].as_str().unwrap_or("Tool failed"),
                &output
            )
        );
    }
    if let Some(fields) = output.as_object_mut() {
        fields.remove("peer_awareness");
        // Native LSP failures already name the exact code and message in the
        // error header. Keep the code and all state/recovery fields.
        if !success
            && matches!(
                tool,
                "get_lsp_status"
                    | "list_document_symbols"
                    | "read_document_diagnostics"
                    | "read_symbol_hover"
                    | "list_workspace_symbols"
                    | "find_definition"
                    | "find_references"
                    | "read_call_hierarchy"
            )
            && fields
                .get("code")
                .and_then(Value::as_str)
                .zip(fields.get("message").and_then(Value::as_str))
                .is_some_and(|(code, message)| {
                    structured["error"].as_str() == Some(format!("{code}: {message}").as_str())
                })
        {
            fields.remove("message");
        }
    }
    match tool {
        "read_tool_manifest" if success => render_manifest(&mut text, output),
        "find_definition" | "find_references" if success => compact::locations(&mut text, output),
        "list_project_files" | "list_project_tracked_files" if success => {
            compact::files(&mut text, output)
        }
        "list_jobs" if success => compact::inventory(&mut text, output, "jobs"),
        "list_projects" if success => compact::inventory(&mut text, output, "projects"),
        "list_document_symbols" if success => compact::symbols(&mut text, output),
        "list_workspace_symbols" if success => compact::workspace_symbols(&mut text, output),
        "run_job" if success => compact::job_started(&mut text, output),
        "observe_jobs" | "wait_for_job_readiness" => compact::jobs(&mut text, output),
        "run_shell" | "run_process" | "run_script" | "run_detached_process" => {
            compact::execution(&mut text, output)
        }
        "read_workspace_changes" if success => compact::changes(&mut text, output),
        "edit_project_files" if success => compact::edits(&mut text, output),
        "read_files" => render_reads(&mut text, output),
        "search_project_texts" => render_search_batch(&mut text, output),
        "search_file_context" => {
            if let Some(fields) = output.as_object_mut() {
                if let Some(search) = fields.remove("search") {
                    render_search_batch(&mut text, search);
                }
                if let Some(reads) = fields.remove("reads") {
                    render_reads(&mut text, reads);
                }
                // An empty read error is redundant only when the producer
                // confirms success. Partial/failed read diagnostics stay visible.
                if fields.get("read_success") == Some(&Value::Bool(true)) {
                    fields.remove("read_success");
                    if fields.get("read_error") == Some(&Value::Null) {
                        fields.remove("read_error");
                    }
                }
            }
            render_fields(&mut text, &output);
        }
        "work_on_project" if success => {
            // Static Host workflow belongs in initialize.instructions. Retain
            // Session identity, workspace, blockers, permission and all receipts.
            if let Some(fields) = output.as_object_mut() {
                fields.remove("workflow");
                if let Some(context) = fields.remove("context_projection") {
                    let instructions = context["materials"].as_array().and_then(|materials| {
                        materials
                            .iter()
                            .find(|material| material["key"] == "project.instructions")
                    });
                    if let Some(instructions) = instructions {
                        // The explicit observation supersedes the body-free startup
                        // summary, including incomplete-scan/unavailable evidence.
                        fields.insert("instructions".into(), instructions.clone());
                    } else {
                        fields.insert("context_projection".into(), context);
                    }
                }
            }
            render_fields(&mut text, &output);
        }
        _ => render_fields(&mut text, &output),
    }
    if text.is_empty() {
        text.push_str("OK\n");
    }
    // A compact, lossless fallback bounds formatting overhead without inventing
    // a new truncation cursor or hiding recovery/effect facts.
    let canonical_bytes = crate::json_measurement::serialized_json_len(structured).unwrap_or(0);
    if text.len() > canonical_bytes.saturating_add(1024) {
        text = ordered_json(structured);
    }
    result.remove("structuredContent");
    result.insert("isError".into(), json!(!success));
    let content = result.entry("content").or_insert_with(|| json!([]));
    if let Some(blocks) = content.as_array_mut() {
        // Native image/resource blocks keep their host-native transport.
        blocks.retain(|block| block["type"] != "text");
        blocks.insert(0, json!({"type":"text", "text":text}));
    }
}

fn render_manifest(text: &mut String, mut output: Value) {
    let Some(fields) = output.as_object_mut() else {
        render_fields(text, &output);
        return;
    };
    let schema = fields.remove("input_schema");
    if let Some(name) = fields.remove("name") {
        let _ = writeln!(text, "{}", name.as_str().unwrap_or("?"));
    }
    if let Some(tools) = fields.remove("tools") {
        if let Some(tools) = tools.as_array() {
            for tool in tools {
                let (Some(name), Some(route)) =
                    (tool["name"].as_str(), tool["route"]["mode"].as_str())
                else {
                    render_fields(text, tool);
                    continue;
                };
                let via = tool["route"]["via"]
                    .as_str()
                    .map(|via| format!(" via {via}"))
                    .unwrap_or_default();
                let description = tool["description"].as_str().unwrap_or("");
                let _ = writeln!(text, "{name} [{route}{via}] {description}");
                let mut remaining = tool.as_object().unwrap().clone();
                for key in ["name", "description"] {
                    remaining.remove(key);
                }
                if let Some(route) = remaining.get_mut("route").and_then(Value::as_object_mut) {
                    route.remove("mode");
                    route.remove("via");
                    if route.is_empty() {
                        remaining.remove("route");
                    }
                }
                // Keep selection-relevant authority/effect and any future fields.
                if !remaining.is_empty() {
                    let _ = writeln!(text, "  {}", ordered_json(&Value::Object(remaining)));
                }
            }
            if tools.is_empty() {
                let _ = writeln!(text, "No matching tools.");
            }
        } else {
            fields.insert("tools".into(), tools);
        }
    }
    if let Some(route) = fields.remove("route") {
        let _ = writeln!(text, "route: {}", ordered_json(&route));
    }
    if let Some(flows) = fields
        .get_mut("recommended_flows")
        .and_then(Value::as_array_mut)
    {
        for flow in std::mem::take(flows) {
            // Canonical flow prose and partial/omitted membership stay together.
            let _ = writeln!(text, "flow: {}", ordered_json(&flow));
        }
        fields.remove("recommended_flows");
    }
    render_fields(text, &output);
    if let Some(schema) = schema {
        // Exact lookup keeps the full necessary schema as one parser-ready JSON line.
        let _ = writeln!(text, "input_schema: {}", ordered_json(&schema));
    }
}

fn ordinary_read_failure(item: &Value) -> bool {
    let read = &item["output"];
    item["success"] == false
        && item["path"].is_string()
        && read["path"] == item["path"]
        && read["error_kind"] == "read_file_failed"
        && read["state_changed"] == false
        && read["reason_code"]
            .as_str()
            .is_some_and(|reason| item["error"] == format!("read_file failed: {reason}"))
        && read.as_object().is_some_and(|fields| fields.len() == 4)
        && item.as_object().is_some_and(|fields| {
            fields.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "index" | "path" | "success" | "output" | "error"
                )
            })
        })
}

fn render_reads(text: &mut String, mut output: Value) {
    let has_followup = output.get("suggested_call").is_some();
    let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) else {
        render_fields(text, &output);
        return;
    };
    let count = items.len();
    let succeeded = items
        .iter()
        .filter(|item| {
            item.get("success").is_none_or(|v| v == true) && item["output"]["text"].is_string()
        })
        .count();
    let all_presented = !items.is_empty()
        && items.iter().all(|item| {
            (item
                .get("success")
                .is_none_or(|v| v.as_bool() == Some(true))
                && item["error"].is_null()
                && item["output"]["text"].is_string())
                || ordinary_read_failure(item)
        });
    for (index, mut item) in std::mem::take(items).into_iter().enumerate() {
        if ordinary_read_failure(&item) {
            let _ = writeln!(
                text,
                "{}: {}",
                compact::one_line(&item["path"]),
                compact::one_line(&item["error"])
            );
            continue;
        }
        let path = item["path"].as_str().unwrap_or("?").to_string();
        let read = &mut item["output"];
        if let Some(source) = read.get("text").and_then(Value::as_str).map(str::to_string) {
            // Canonical complete-file reads omit their range; never print "null".
            let range = match (read["start_line"].as_u64(), read["end_line"].as_u64()) {
                (Some(start), Some(end)) => format!(":{start}-{end}"),
                (Some(start), None) => format!(":{start}"),
                (None, _) => String::new(),
            };
            let _ = writeln!(
                text,
                "==> {path}{range} (read_revision {})",
                ordered_json(&read["read_revision"])
            );
            text.push_str(&source);
            if !source.ends_with('\n') {
                text.push('\n');
            }
            if let Some(fields) = read.as_object_mut() {
                for key in [
                    "text",
                    "path",
                    "start_line",
                    "end_line",
                    "read_revision",
                    "format",
                ] {
                    fields.remove(key);
                }
                if fields.get("truncated").is_none_or(|v| v == false) && !has_followup {
                    for key in ["limit", "returned_lines", "has_more", "next_start_line"] {
                        fields.remove(key);
                    }
                }
                if fields.get("truncated") == Some(&json!(false)) {
                    fields.remove("truncated");
                }
            }
            render_fields(text, read);
            if let Some(fields) = item.as_object_mut() {
                for key in ["path", "output"] {
                    fields.remove(key);
                }
                if fields.get("success") == Some(&Value::Bool(true)) {
                    fields.remove("success");
                }
                if fields.get("error") == Some(&Value::Null) {
                    fields.remove("error");
                }
            }
            if item["index"].as_u64() == Some(index as u64) {
                item.as_object_mut().unwrap().remove("index");
            }
            render_fields(text, &item);
        } else {
            let _ = writeln!(text, "==> {path}");
            if let Some(fields) = item.as_object_mut() {
                fields.remove("path");
                if fields.get("index") == Some(&json!(index)) {
                    fields.remove("index");
                }
            }
            if item["success"] == false {
                let error = item["error"].clone();
                if let Some(fields) = item.get_mut("output").and_then(Value::as_object_mut) {
                    if fields.get("error") == Some(&error) {
                        fields.remove("error");
                    }
                    if fields.get("path") == Some(&json!(path)) {
                        fields.remove("path");
                    }
                    if fields.is_empty() {
                        item.as_object_mut().unwrap().remove("output");
                    }
                }
                item.as_object_mut().unwrap().remove("success");
            }
            render_fields(text, &item);
        }
    }
    output.as_object_mut().unwrap().remove("items");
    if all_presented
        && output
            .get("output_truncated")
            .is_none_or(|v| v.as_bool() == Some(false))
    {
        for key in ["requested_count", "returned_count"] {
            if output[key].as_u64() == Some(count as u64) {
                output.as_object_mut().unwrap().remove(key);
            }
        }
        for (key, value) in [
            ("succeeded_count", json!(succeeded)),
            ("failed_count", json!(count - succeeded)),
            ("output_truncated", json!(false)),
            ("next_index", Value::Null),
        ] {
            if output.get(key) == Some(&value) {
                output.as_object_mut().unwrap().remove(key);
            }
        }
        output.as_object_mut().unwrap().remove("project");
    }
    render_fields(text, &output);
}

/// Replace only the MCP adapter's exact native execution log pointer. The
/// following renderer prints those tails directly; provider errors never enter.
fn execution_error_text(tool: &str, error: &str, output: &Value) -> String {
    if !matches!(tool, "run_shell" | "run_process" | "run_script") {
        return error.to_owned();
    }
    let logs = output.get("details").unwrap_or(output);
    let replacement = if logs["stdout_tail"] == "" && logs["stderr_tail"] == "" {
        "Logs: stdout and stderr were empty."
    } else {
        "Logs: available stdout_tail and stderr_tail are included below."
    };
    error
        .replace(
            "Logs: see output.stdout_tail and output.stderr_tail.",
            replacement,
        )
        .replace(
            "Logs: see output.details.stdout_tail and output.details.stderr_tail.",
            replacement,
        )
}

fn render_search_batch(text: &mut String, mut output: Value) {
    if let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) {
        for mut item in std::mem::take(items) {
            let _ = writeln!(text, "==> query {}", ordered_json(&item["index"]));
            if let Some(fields) = item.as_object_mut() {
                if let Some(search) = fields.remove("output") {
                    render_search(text, search);
                }
                fields.remove("index");
                if fields.get("success") == Some(&Value::Bool(true)) {
                    fields.remove("success");
                }
                if fields.get("error") == Some(&Value::Null) {
                    fields.remove("error");
                }
            }
            render_fields(text, &item);
        }
        output.as_object_mut().unwrap().remove("items");
        render_fields(text, &output);
    } else {
        render_search(text, output);
    }
}

fn render_search_context(text: &mut String, path: &str, matched: &mut Value, key: &str) {
    let Some(lines) = matched[key].as_array().filter(|lines| {
        lines.iter().all(|line| {
            line.as_object().is_some_and(|fields| fields.len() == 2)
                && line["line"].as_u64().is_some_and(|number| number > 0)
                && line["text"].is_string()
        })
    }) else {
        // Unknown context shapes retain every field in the generic projection.
        return;
    };
    for line in lines {
        let _ = writeln!(
            text,
            "{path}-{}-{}",
            line["line"],
            line["text"].as_str().unwrap()
        );
    }
    matched.as_object_mut().unwrap().remove(key);
}

fn render_search(text: &mut String, mut output: Value) {
    if output["files"].as_array().is_some_and(|files| {
        files
            .iter()
            .all(|file| file.is_string() || file["path"].is_string())
    }) {
        let files = output.as_object_mut().unwrap().remove("files").unwrap();
        for file in files.as_array().unwrap() {
            if file.is_string() {
                let _ = writeln!(text, "{}", compact::one_line(file));
            } else {
                let mut fields = file.as_object().unwrap().clone();
                let path = fields.remove("path").unwrap();
                let _ = write!(text, "{}", compact::one_line(&path));
                fields.sort_keys();
                for (key, value) in fields {
                    let _ = write!(text, " {key}: {}", scalar(&value));
                }
                text.push('\n');
            }
        }
    }
    if let Some(matches) = output.get_mut("matches").and_then(Value::as_array_mut) {
        let grouped = matches.iter().any(|matched| {
            ["context_before", "context_after"].iter().any(|key| {
                matched[*key]
                    .as_array()
                    .is_some_and(|lines| !lines.is_empty())
            })
        });
        for (index, mut matched) in std::mem::take(matches).into_iter().enumerate() {
            // Keep context groups separate instead of deduplicating overlapping
            // lines: a later hit must retain its own matching-line marker.
            if grouped && index > 0 {
                text.push_str("--\n");
            }
            let valid = matched["path"].is_string()
                && matched["line"].as_u64().is_some_and(|line| line > 0)
                && matched.get("preview").is_none_or(Value::is_string);
            if !valid {
                render_fields(text, &matched);
                continue;
            }
            let path = compact::one_line(&matched["path"]);
            render_search_context(text, &path, &mut matched, "context_before");
            if let Some(preview) = matched.get("preview").and_then(Value::as_str) {
                let _ = writeln!(text, "{path}:{}:{preview}", matched["line"]);
            } else {
                // search_file_context removes previews already covered by its
                // versioned read ranges. The match location still matters.
                let _ = writeln!(text, "{path}:{}", matched["line"]);
            }
            render_search_context(text, &path, &mut matched, "context_after");
            if let Some(fields) = matched.as_object_mut() {
                for key in ["path", "line", "preview", "read_hint"] {
                    fields.remove(key);
                }
            }
            render_fields(text, &matched);
        }
        output.as_object_mut().unwrap().remove("matches");
    }
    render_fields(text, &output);
}

fn render_fields(text: &mut String, value: &Value) {
    render_value(text, value, 0);
}

fn render_value(text: &mut String, value: &Value, depth: usize) {
    let indent = "  ".repeat(depth.min(4));
    match value {
        Value::Object(fields) => {
            if fields.get("tool").is_some_and(Value::is_string)
                && fields.get("arguments").is_some_and(Value::is_object)
            {
                let _ = writeln!(text, "{indent}{}", ordered_json(value));
                return;
            }
            // Tool-specific layouts above own their key order. Unknown fields
            // use lexical order at the rendering boundary: preserve_order maps
            // retain insertion order, and remove() may swap remaining entries.
            let mut entries: Vec<_> = fields.iter().collect();
            entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
            for (key, value) in entries {
                // Do not recursively filter arbitrary keys: provider data,
                // source text and error/recovery payloads retain their meaning.
                if matches!(value, Value::Object(_) | Value::Array(_)) {
                    let _ = writeln!(text, "{indent}{key}:");
                    render_value(text, value, depth + 1);
                } else {
                    let _ = writeln!(text, "{indent}{key}: {}", scalar(value));
                }
            }
        }
        Value::Array(values) => {
            if values.is_empty() {
                let _ = writeln!(text, "{indent}[]");
            }
            for (index, value) in values.iter().enumerate() {
                let _ = writeln!(text, "{indent}[{index}]");
                render_value(text, value, depth + 1);
            }
        }
        _ => {
            let _ = writeln!(text, "{indent}{}", scalar(value));
        }
    }
}

fn scalar(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| ordered_json(value))
}

/// Only the CLI text projection sorts objects. Canonical JSON and provider
/// payloads keep their own serialization; arrays retain their semantic order.
fn ordered_json(value: &Value) -> String {
    let mut ordered = value.clone();
    ordered.sort_all_objects();
    ordered.to_string()
}
