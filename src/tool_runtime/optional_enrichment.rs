//! Optional response decoration is budgeted; explicit replies/ACK and canonical
//! business/audit writes are not. The deadline is shared, never reset per sidecar.
use super::ToolResult;
use serde_json::Value;
use std::time::{Duration, Instant};

pub(crate) const BUDGET: Duration = Duration::from_millis(40);
pub(crate) fn deadline() -> Instant {
    Instant::now() + BUDGET
}

/// Size-check without committing delivery state or cloning the primary payload.
pub(crate) fn fits(result: &mut ToolResult, key: &str, value: Value) -> bool {
    let Some(output) = result.output.as_object_mut() else {
        return false;
    };
    let previous = output.insert(key.to_string(), value);
    let fits = crate::json_measurement::serialized_json_len(result).is_ok_and(|bytes| {
        bytes <= webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES
    });
    let output = result.output.as_object_mut().expect("object unchanged");
    output.remove(key);
    if let Some(previous) = previous {
        output.insert(key.to_string(), previous);
    }
    fits
}

pub(crate) fn omitted(deadline: Instant) -> &'static str {
    if Instant::now() >= deadline {
        "budget_exhausted"
    } else {
        "skipped_due_to_contention"
    }
}
