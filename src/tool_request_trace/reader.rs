//! On-demand bounded trace index reads. The capture switch controls new writes,
//! not an administrator's ability to inspect already retained evidence.
use super::*;

const PAGE_BYTES: usize = 64 * 1024;

fn read_events(trace_ref: &str) -> Result<Vec<Value>, TraceReadError> {
    validate_trace_ref(trace_ref)?;
    flush_trace_writer_for_read()?;
    let directory = trace_root().join(trace_ref);
    require_private_directory(&directory, "trace_not_found")?;
    require_private_regular_file(&directory.join(TRACE_OWNER_MARKER), "trace_corrupt")?;
    let path = directory.join("events.jsonl");
    // Serialize against this writer's append. Do not let a concurrent append
    // turn a bounded snapshot into a partial JSON line or an unbounded read.
    let _guard = trace_io_state().lock().map_err(|_| {
        TraceReadError::new("trace_store_unavailable", "trace index lock unavailable")
    })?;
    let metadata = require_private_regular_file(&path, "trace_corrupt")?;
    if metadata.len() > MAX_TRACE_EVENTS_FILE_BYTES {
        return Err(TraceReadError::new(
            "trace_too_large",
            "trace index exceeds read budget",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .map_err(|_| TraceReadError::new("trace_corrupt", "trace index unreadable"))?
        .take(MAX_TRACE_EVENTS_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| TraceReadError::new("trace_corrupt", "trace index unreadable"))?;
    if bytes.len() as u64 > MAX_TRACE_EVENTS_FILE_BYTES {
        return Err(TraceReadError::new(
            "trace_too_large",
            "trace index grew beyond read budget",
        ));
    }
    drop(_guard); // Parse the bounded snapshot without stalling the writer.
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| TraceReadError::new("trace_corrupt", "trace index is not UTF-8"))?;
    let mut events = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let event: Value = serde_json::from_str(line).map_err(|_| {
            TraceReadError::new(
                "trace_corrupt",
                "trace index has incomplete or invalid JSON",
            )
        })?;
        if event.get("server_trace_id").and_then(Value::as_str) != Some(trace_ref) {
            return Err(TraceReadError::new(
                "trace_corrupt",
                "trace event identity mismatch",
            ));
        }
        if events.len() == MAX_TRACE_PAYLOAD_ENTRIES {
            return Err(TraceReadError::new(
                "trace_too_large",
                "trace event count exceeds read budget",
            ));
        }
        events.push(event);
    }
    Ok(events)
}

pub(crate) fn read_trace(
    trace_ref: &str,
    offset: Option<usize>,
    limit: Option<usize>,
    payload_index: Option<usize>,
) -> Result<Value, TraceReadError> {
    validate_trace_ref(trace_ref)?;
    if payload_index.is_some() {
        return read_full_trace(trace_ref, offset, limit, payload_index);
    }
    let offset = offset.unwrap_or(0);
    let limit = limit
        .unwrap_or(DEFAULT_TRACE_INDEX_LIMIT)
        .clamp(1, MAX_TRACE_INDEX_LIMIT);
    if offset > MAX_TRACE_PAYLOAD_ENTRIES {
        return Err(TraceReadError::new(
            "invalid_trace_request",
            "trace offset exceeds read budget",
        ));
    }
    let events = match read_events(trace_ref) {
        Ok(events) => events,
        Err(error) if error.kind == "trace_not_found" => {
            return Ok(json!({
                "trace_ref":trace_ref, "status":"unavailable", "reason":"trace_not_retained",
                "capture_mode":capture_mode(), "capture_health":capture_health(), "events":[], "returned_count":0,
                "coverage":"unknown", "possible_reasons":["not_captured","capture_dropped","expired_or_evicted"],
            }))
        }
        Err(error) => return Err(error),
    };
    let known_dropped_events: u64 = events
        .iter()
        .filter(|event| event["event"] == "tool_trace_capture_gap")
        .filter_map(|event| event.get("known_dropped_events").and_then(Value::as_u64))
        .fold(0, u64::saturating_add);
    let mode = events
        .iter()
        .filter_map(|event| event.get("trace_mode").and_then(Value::as_str))
        .find(|mode| matches!(*mode, "metadata" | "full"))
        .unwrap_or("unknown");
    let mut payload_index = 0;
    let mut page = Vec::new();
    let mut bytes = 0;
    let mut next = offset;
    for (index, event) in events.iter().enumerate() {
        let payload =
            event.get("event").and_then(Value::as_str) == Some("tool_trace_payload_captured");
        let ordinal = payload_index;
        if payload {
            payload_index += 1;
        }
        if index < offset || page.len() >= limit {
            continue;
        }
        let mut projected = event.clone(); // Source file and each page are independently bounded.
        if let Some(object) = projected.as_object_mut() {
            object.remove("payload_path");
            if payload {
                object.insert("payload_index".into(), json!(ordinal));
            }
        }
        let size =
            crate::json_measurement::serialized_json_len(&projected).unwrap_or(PAGE_BYTES + 1);
        if bytes + size > PAGE_BYTES {
            if page.is_empty() {
                return Err(TraceReadError::new(
                    "trace_event_too_large",
                    "single trace event exceeds diagnostic page budget",
                ));
            }
            break;
        }
        bytes += size;
        page.push(projected);
        next = index + 1;
    }
    let next_offset = (next < events.len()).then_some(next);
    Ok(
        json!({"trace_ref":trace_ref,"status":"available","trace_mode":mode,"capture_mode":capture_mode(),
        "entry_count":events.len(),"payload_count":events.iter().filter(|e| e["event"]=="tool_trace_payload_captured").count(),
        "offset":offset,"returned_count":page.len(),"next_offset":next_offset,"events":page,
        "coverage":if known_dropped_events > 0 { "incomplete" } else { "observed_events_only" }, "known_dropped_events":known_dropped_events,
        "handler_finished_observed":events.iter().any(|event| event["event"].as_str().is_some_and(|name| name.ends_with("_tool_handler_finished"))),
        "max_page_bytes":PAGE_BYTES, "capture_health":capture_health(),
        "response_handoff_observed":events.iter().any(|event| event["event"].as_str().is_some_and(|name| name.ends_with("_tool_handler_returned"))),
        "delivery_boundary":"handler_returned_is_not_client_receipt_or_model_reading"}),
    )
}

pub(crate) fn capture_mode() -> &'static str {
    match crate::config::tool_request_trace_mode() {
        ToolRequestTraceMode::Off => "off",
        ToolRequestTraceMode::Metadata => "metadata",
        ToolRequestTraceMode::Full => "full",
    }
}

#[cfg(test)]
mod tests;
