//! Shared JSON-wire accounting for instruction and startup projections.
//! These helpers neither observe data nor choose a domain's truncation priority.
use serde_json::Value;

/// Bound by serialized payload cost, including escaping, without splitting UTF-8.
pub(super) fn bounded_json_string(value: &str, max_payload_bytes: usize) -> (String, bool) {
    let mut output = String::new();
    let mut used = 0usize;
    for character in value.chars() {
        let encoded = serde_json::to_string(&character.to_string())
            .expect("single character JSON serialization");
        let cost = encoded.len().saturating_sub(2);
        if used.saturating_add(cost) > max_payload_bytes {
            return (output, true);
        }
        output.push(character);
        used = used.saturating_add(cost);
    }
    (output, false)
}

pub(super) fn json_string_payload_len(value: &str) -> usize {
    serde_json::to_string(value)
        .map(|encoded| encoded.len().saturating_sub(2))
        .unwrap_or(0)
}

pub(super) fn serialized_len(value: &Value) -> usize {
    crate::json_measurement::serialized_json_len(value).unwrap_or(usize::MAX)
}
