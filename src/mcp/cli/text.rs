//! Render already-bounded canonical observations without escaping source text.
//! Only known presentation redundancy is removed. Unknown fields, failures,
//! revision fences, incomplete coverage and exact recovery calls survive.
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
            structured["error"].as_str().unwrap_or("Tool failed")
        );
    }
    if let Some(fields) = output.as_object_mut() {
        fields.remove("peer_awareness");
    }
    match tool {
        "read_tool_manifest" if success => render_manifest(&mut text, output),
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
                    let _ = writeln!(text, "  {}", Value::Object(remaining));
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
        let _ = writeln!(text, "route: {route}");
    }
    if let Some(flows) = fields
        .get_mut("recommended_flows")
        .and_then(Value::as_array_mut)
    {
        for flow in std::mem::take(flows) {
            // Canonical flow prose and partial/omitted membership stay together.
            let _ = writeln!(text, "flow: {flow}");
        }
        fields.remove("recommended_flows");
    }
    render_fields(text, &output);
    if let Some(schema) = schema {
        // Exact lookup keeps the full necessary schema as one parser-ready JSON line.
        let _ = writeln!(text, "input_schema: {schema}");
    }
}

fn render_reads(text: &mut String, mut output: Value) {
    let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) else {
        render_fields(text, &output);
        return;
    };
    for mut item in std::mem::take(items) {
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
            render_fields(text, &item);
        } else {
            let _ = writeln!(text, "==> {path}");
            render_fields(text, &item);
        }
    }
    output.as_object_mut().unwrap().remove("items");
    render_read_continuation(text, &mut output);
    render_fields(text, &output);
}

fn render_read_continuation(text: &mut String, output: &mut Value) {
    let Some(call) = output.get("suggested_call") else {
        return;
    };
    if call["tool"] != "read_files" || !call["arguments"]["items"].is_array() {
        return;
    }
    let mut call = output
        .as_object_mut()
        .unwrap()
        .remove("suggested_call")
        .unwrap();
    let _ = writeln!(text, "Next read_files:");
    let arguments = &mut call["arguments"];
    let items = arguments.as_object_mut().unwrap().remove("items").unwrap();
    for mut item in items.as_array().unwrap().iter().cloned() {
        let path = item
            .as_object_mut()
            .and_then(|fields| fields.remove("path"));
        let _ = write!(
            text,
            "  {}",
            path.as_ref().map(scalar).unwrap_or_else(|| "?".into())
        );
        if let Some(fields) = item.as_object() {
            for (key, value) in fields {
                let _ = write!(text, "; {key}: {}", scalar(value));
            }
        } else {
            let _ = write!(text, "; {item}");
        }
        text.push('\n');
    }
    // Keep the exact bound Project, explicit Session, byte budget and follow-up
    // posture once, including zero-progress recovery. None may be inferred.
    render_fields(text, arguments);
    if let Some(fields) = call.as_object_mut() {
        fields.remove("tool");
        fields.remove("arguments");
    }
    render_fields(text, &call);
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
