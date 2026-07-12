//! Shared redaction, bounding, and time helpers for session modules.
use serde_json::{json, Value};

use super::model::{
    MAX_INPUT_ARRAY_ITEMS, MAX_INPUT_OBJECT_KEYS, MAX_INPUT_STRING_CHARS, MAX_SUMMARY_STRING_CHARS,
    MAX_VALIDATION_EXCERPT_CHARS,
};

pub(super) fn redact_and_bound_value(value: &Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut redacted = serde_json::Map::new();
            for (key, value) in obj.iter().take(MAX_INPUT_OBJECT_KEYS) {
                if is_sensitive_key(key) {
                    redacted.insert(key.clone(), Value::String("[redacted]".to_string()));
                } else {
                    redacted.insert(key.clone(), redact_and_bound_value(value));
                }
            }
            if obj.len() > MAX_INPUT_OBJECT_KEYS {
                redacted.insert("_truncated".to_string(), json!(true));
            }
            Value::Object(redacted)
        }
        Value::Array(values) => {
            let mut redacted: Vec<Value> = values
                .iter()
                .take(MAX_INPUT_ARRAY_ITEMS)
                .map(redact_and_bound_value)
                .collect();
            if values.len() > MAX_INPUT_ARRAY_ITEMS {
                redacted.push(json!({"_truncated": true}));
            }
            Value::Array(redacted)
        }
        Value::String(s) if looks_like_secret_string(s) => Value::String("[redacted]".to_string()),
        Value::String(s) => Value::String(bound_chars(s, MAX_INPUT_STRING_CHARS)),
        _ => value.clone(),
    }
}

pub(super) fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key == "authorization"
        || key == "auth"
        || key == "client_secret"
        || key == "pat"
        || key == "bearer"
}

pub(super) fn looks_like_secret_string(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("bearer ")
        || value.contains("wc_pat_")
        || value.contains("wc_oat_")
        || value.contains("wc_ort_")
        || value.contains("wc_agent_")
        || value.contains("wc_acct_")
        || value.contains("wc_pair_")
        || value.contains("wc_csec_")
        || value.contains("client_secret")
}

pub(super) fn bound_summary_string(value: &str) -> String {
    bound_chars(value, MAX_SUMMARY_STRING_CHARS)
}

pub(super) fn bound_event_error_summary(value: &str, shell_like: bool) -> String {
    if !shell_like {
        return bound_summary_string(value);
    }
    let summary = value
        .lines()
        .take_while(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("stdout_tail:")
                && !trimmed.starts_with("stderr_tail:")
                && !trimmed.starts_with("stdout:")
                && !trimmed.starts_with("stderr:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let summary = summary.trim();
    if summary.is_empty() {
        "shell command failed; stdout/stderr omitted from session event".to_string()
    } else {
        bound_summary_string(summary)
    }
}

pub(super) struct ValidationExcerpt {
    pub(super) text: String,
    pub(super) filtered: bool,
}

pub(super) fn validation_excerpt(value: &str) -> ValidationExcerpt {
    let mut filtered = false;
    let mut lines = Vec::new();
    for line in value.lines() {
        let sanitized = sanitize_validation_line(line.trim_end_matches('\r'));
        if sanitized != line {
            filtered = true;
        }
        if validation_line_is_suspicious(&sanitized) {
            filtered = true;
            continue;
        }
        lines.push(sanitized);
    }
    let mut text = lines.join("\n");
    if value.ends_with('\n') && !text.is_empty() {
        text.push('\n');
    }
    let bounded = bound_validation_excerpt(&text);
    if bounded != text {
        filtered = true;
    }
    ValidationExcerpt {
        text: bounded,
        filtered,
    }
}

pub(super) fn sanitize_validation_line(line: &str) -> String {
    line.chars()
        .filter(|ch| !ch.is_control() || *ch == '\t')
        .collect()
}

pub(super) fn validation_line_is_suspicious(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let compact: String = lower
        .chars()
        .filter(|ch| !matches!(*ch, '_' | '-') && !ch.is_whitespace())
        .collect();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("authorization")
        || lower.contains("bearer")
        || compact.contains("apikey")
        || compact.contains("accesskey")
        || compact.contains("privatekey")
}

pub(super) fn bound_validation_excerpt(value: &str) -> String {
    let count = value.chars().count();
    if count <= MAX_VALIDATION_EXCERPT_CHARS {
        return value.to_string();
    }
    if MAX_VALIDATION_EXCERPT_CHARS <= 3 {
        return ".".repeat(MAX_VALIDATION_EXCERPT_CHARS);
    }
    let keep = MAX_VALIDATION_EXCERPT_CHARS - 3;
    let suffix: String = value
        .chars()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{suffix}")
}

pub(super) fn bound_chars(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

pub(super) fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}
