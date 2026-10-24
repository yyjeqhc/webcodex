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
    }
    match tool {
        "find_definition" | "find_references" if success => compact::locations(&mut text, output),
        "list_project_files" | "list_project_tracked_files" if success => {
            compact::files(&mut text, output)
        }
        "observe_jobs" | "wait_for_job_readiness" => compact::jobs(&mut text, output),
        "run_shell" | "run_process" => compact::execution(&mut text, output),
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
        text = serde_json::to_string(structured).expect("JSON value");
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

fn render_reads(text: &mut String, mut output: Value) {
    let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) else {
        render_fields(text, &output);
        return;
    };
    let count = items.len();
    let all_succeeded = !items.is_empty()
        && items.iter().all(|item| {
            item.get("success")
                .is_none_or(|v| v.as_bool() == Some(true))
                && item["error"].is_null()
                && item["output"]["text"].is_string()
        });
    for (index, mut item) in std::mem::take(items).into_iter().enumerate() {
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
                read["read_revision"]
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
            render_fields(text, &item);
        }
    }
    output.as_object_mut().unwrap().remove("items");
    if all_succeeded
        && output
            .get("output_truncated")
            .is_none_or(|v| v.as_bool() == Some(false))
    {
        for key in ["requested_count", "returned_count", "succeeded_count"] {
            if output[key].as_u64() == Some(count as u64) {
                output.as_object_mut().unwrap().remove(key);
            }
        }
        for (key, value) in [
            ("failed_count", json!(0)),
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
    if !matches!(tool, "run_shell" | "run_process") {
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
            let _ = writeln!(text, "==> query {}", item["index"]);
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

fn render_search(text: &mut String, mut output: Value) {
    if let Some(matches) = output.get_mut("matches").and_then(Value::as_array_mut) {
        for mut matched in std::mem::take(matches) {
            let _ = writeln!(
                text,
                "{}:{}:{}",
                matched["path"].as_str().unwrap_or("?"),
                matched["line"],
                matched["preview"].as_str().unwrap_or("")
            );
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
                let _ = writeln!(text, "{indent}{value}");
                return;
            }
            for (key, value) in fields {
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
        .unwrap_or_else(|| value.to_string())
}
