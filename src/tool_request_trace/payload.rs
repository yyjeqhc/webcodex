//! Payload responsibility of the existing trace subsystem.
use super::*;

#[derive(Debug, Clone)]
pub(super) struct IndexedTracePayload {
    pub(super) phase: String,
    pub(super) payload_bytes: usize,
    pub(super) compressed_bytes: usize,
    pub(super) payload_sha256: String,
    pub(super) file_name: String,
}

pub(super) fn invalid_trace_ref() -> TraceReadError {
    TraceReadError::new(
        "invalid_trace_ref",
        "trace_ref must be the canonical UUID returned by an eligible failed tool call",
    )
}

pub(super) fn validate_trace_ref(trace_ref: &str) -> Result<(), TraceReadError> {
    let parsed = Uuid::parse_str(trace_ref).map_err(|_| invalid_trace_ref())?;
    if parsed.to_string() != trace_ref {
        return Err(invalid_trace_ref());
    }
    Ok(())
}

pub(super) fn parse_trace_payload_index(
    trace_ref: &str,
    trace_dir: &Path,
) -> Result<Vec<IndexedTracePayload>, TraceReadError> {
    require_private_regular_file(&trace_dir.join(TRACE_OWNER_MARKER), "trace_corrupt")?;
    let events_path = trace_dir.join("events.jsonl");
    let read_guard = trace_io_state().lock().map_err(|_| {
        TraceReadError::new("trace_store_unavailable", "trace index lock unavailable")
    })?;
    let metadata = require_private_regular_file(&events_path, "trace_corrupt")?;
    if metadata.len() > MAX_TRACE_EVENTS_FILE_BYTES {
        return Err(TraceReadError::new(
            "trace_too_large",
            "trace event index exceeds the diagnostic read ceiling",
        ));
    }
    let mut text = String::new();
    File::open(events_path)
        .map_err(|_| TraceReadError::new("trace_corrupt", "trace event index is unreadable"))?
        .take(MAX_TRACE_EVENTS_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|_| TraceReadError::new("trace_corrupt", "trace event index is unreadable"))?;
    drop(read_guard);
    if text.len() as u64 > MAX_TRACE_EVENTS_FILE_BYTES {
        return Err(TraceReadError::new(
            "trace_too_large",
            "trace index grew beyond read budget",
        ));
    }
    let mut payloads = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let event: Value = serde_json::from_str(line).map_err(|_| {
            TraceReadError::new("trace_corrupt", "trace event index contains invalid JSON")
        })?;
        if event.get("event").and_then(Value::as_str) != Some("tool_trace_payload_captured") {
            continue;
        }
        if event.get("server_trace_id").and_then(Value::as_str) != Some(trace_ref)
            || event.get("encoding").and_then(Value::as_str) != Some("json+zstd")
        {
            return Err(TraceReadError::new(
                "trace_corrupt",
                "trace payload index correlation is invalid",
            ));
        }
        let phase = event
            .get("phase")
            .and_then(Value::as_str)
            .filter(|phase| !phase.is_empty() && phase.len() <= 80 && safe_phase(phase) == *phase)
            .ok_or_else(|| TraceReadError::new("trace_corrupt", "trace payload phase is invalid"))?
            .to_string();
        let payload_bytes = event
            .get("payload_bytes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| TraceReadError::new("trace_corrupt", "trace payload size is invalid"))?;
        let compressed_bytes = event
            .get("compressed_bytes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                TraceReadError::new("trace_corrupt", "trace compressed size is invalid")
            })?;
        let payload_sha256 = event
            .get("payload_sha256")
            .and_then(Value::as_str)
            .filter(|digest| {
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            .ok_or_else(|| TraceReadError::new("trace_corrupt", "trace payload digest is invalid"))?
            .to_string();
        let payload_path = event
            .get("payload_path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                TraceReadError::new("trace_corrupt", "trace payload location is invalid")
            })?;
        let Some(file_name) = payload_path.strip_prefix("payloads/") else {
            return Err(TraceReadError::new(
                "trace_corrupt",
                "trace payload location is outside the owned payload directory",
            ));
        };
        if file_name.is_empty()
            || file_name.contains('/')
            || file_name.contains('\\')
            || file_name.contains("..")
            || !file_name.ends_with(".json.zst")
        {
            return Err(TraceReadError::new(
                "trace_corrupt",
                "trace payload filename failed the safety check",
            ));
        }
        payloads.push(IndexedTracePayload {
            phase,
            payload_bytes,
            compressed_bytes,
            payload_sha256,
            file_name: file_name.to_string(),
        });
        if payloads.len() > MAX_TRACE_PAYLOAD_ENTRIES {
            return Err(TraceReadError::new(
                "trace_too_large",
                "trace payload index exceeds the diagnostic entry ceiling",
            ));
        }
    }
    Ok(payloads)
}

pub(super) fn trace_payload_metadata(index: usize, payload: &IndexedTracePayload) -> Value {
    json!({
        "payload_index": index,
        "phase": payload.phase,
        "payload_bytes": payload.payload_bytes,
        "compressed_bytes": payload.compressed_bytes,
        "payload_sha256": payload.payload_sha256,
        "payload_available": payload.payload_bytes <= MAX_MODEL_TRACE_PAYLOAD_BYTES,
    })
}

pub(crate) fn read_full_trace(
    trace_ref: &str,
    offset: Option<usize>,
    limit: Option<usize>,
    payload_index: Option<usize>,
) -> Result<Value, TraceReadError> {
    validate_trace_ref(trace_ref)?;
    if payload_index.is_some() && (offset.is_some() || limit.is_some()) {
        return Err(TraceReadError::new(
            "invalid_trace_request",
            "offset and limit cannot be combined with payload_index",
        ));
    }
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(DEFAULT_TRACE_INDEX_LIMIT);
    if offset > 4095 || !(1..=MAX_TRACE_INDEX_LIMIT).contains(&limit) {
        return Err(TraceReadError::new(
            "invalid_trace_request",
            "trace listing bounds are outside the supported range",
        ));
    }
    if payload_index.is_some_and(|index| index > 4095) {
        return Err(TraceReadError::new(
            "invalid_trace_request",
            "payload_index is outside the supported range",
        ));
    }

    flush_trace_writer_for_read()?;
    let trace_dir = trace_root().join(trace_ref);
    require_private_directory(&trace_dir, "trace_not_found")?;
    let payloads = parse_trace_payload_index(trace_ref, &trace_dir)?;

    let Some(payload_index) = payload_index else {
        let returned = payloads
            .iter()
            .enumerate()
            .skip(offset)
            .take(limit)
            .map(|(index, payload)| trace_payload_metadata(index, payload))
            .collect::<Vec<_>>();
        let next_offset =
            (offset + returned.len() < payloads.len()).then_some(offset + returned.len());
        return Ok(json!({
            "trace_ref": trace_ref,
            "trace_mode": "full",
            "payload_count": payloads.len(),
            "returned_count": returned.len(),
            "offset": offset,
            "next_offset": next_offset,
            "payloads": returned,
            "max_payload_bytes": MAX_MODEL_TRACE_PAYLOAD_BYTES,
        }));
    };

    let Some(indexed) = payloads.get(payload_index) else {
        return Err(TraceReadError::new(
            "invalid_payload_index",
            "payload_index does not identify a retained payload in this trace",
        ));
    };
    if indexed.payload_bytes > MAX_MODEL_TRACE_PAYLOAD_BYTES {
        return Ok(json!({
            "trace_ref": trace_ref,
            "trace_mode": "full",
            "payload_index": payload_index,
            "phase": indexed.phase,
            "payload_bytes": indexed.payload_bytes,
            "payload_sha256": indexed.payload_sha256,
            "payload_available": false,
            "max_payload_bytes": MAX_MODEL_TRACE_PAYLOAD_BYTES,
            "reason": "payload_exceeds_model_read_limit",
        }));
    }
    if indexed.compressed_bytes > MAX_MODEL_TRACE_COMPRESSED_BYTES {
        return Err(TraceReadError::new(
            "trace_corrupt",
            "trace payload compressed size exceeds the diagnostic read ceiling",
        ));
    }

    let payload_dir = trace_dir.join("payloads");
    require_private_directory(&payload_dir, "trace_corrupt")?;
    let payload_path = payload_dir.join(&indexed.file_name);
    let metadata = require_private_regular_file(&payload_path, "trace_corrupt")?;
    if metadata.len() != indexed.compressed_bytes as u64 {
        return Err(TraceReadError::new(
            "trace_corrupt",
            "trace payload compressed size does not match its index",
        ));
    }
    let file = File::open(payload_path)
        .map_err(|_| TraceReadError::new("trace_corrupt", "trace payload is unreadable"))?;
    let decoder = zstd::stream::read::Decoder::new(file)
        .map_err(|_| TraceReadError::new("trace_corrupt", "trace payload decompression failed"))?;
    let mut raw = Vec::with_capacity(indexed.payload_bytes);
    decoder
        .take((MAX_MODEL_TRACE_PAYLOAD_BYTES + 1) as u64)
        .read_to_end(&mut raw)
        .map_err(|_| TraceReadError::new("trace_corrupt", "trace payload decompression failed"))?;
    if raw.len() != indexed.payload_bytes || sha256_hex(&raw) != indexed.payload_sha256 {
        return Err(TraceReadError::new(
            "trace_corrupt",
            "trace payload failed its size or digest check",
        ));
    }
    let payload: Value = serde_json::from_slice(&raw)
        .map_err(|_| TraceReadError::new("trace_corrupt", "trace payload is not valid JSON"))?;
    Ok(json!({
        "trace_ref": trace_ref,
        "trace_mode": "full",
        "payload_index": payload_index,
        "phase": indexed.phase,
        "payload_bytes": indexed.payload_bytes,
        "payload_sha256": indexed.payload_sha256,
        "payload_available": true,
        "max_payload_bytes": MAX_MODEL_TRACE_PAYLOAD_BYTES,
        "payload": payload,
    }))
}
