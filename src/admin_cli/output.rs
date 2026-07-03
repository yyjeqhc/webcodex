use serde_json::Value;

pub(super) fn format_error(status: u16, content_type: &str, body: &str, token: &str) -> String {
    if content_type
        .split(';')
        .next()
        .is_some_and(|ct| ct.trim().eq_ignore_ascii_case("application/json"))
    {
        if let Ok(value) = serde_json::from_str::<Value>(body) {
            if let Some(error) = value.get("error").and_then(Value::as_str) {
                return sanitize(
                    token,
                    &format!("request failed: HTTP {}: {}", status, error),
                );
            }
            return sanitize(
                token,
                &format!("request failed: HTTP {}: {}", status, value),
            );
        }
    }
    format!(
        "request failed: HTTP {} (content-type: {})",
        status, content_type
    )
}

pub(super) fn sanitize(token: &str, message: &str) -> String {
    if token.is_empty() {
        message.to_string()
    } else {
        message.replace(token, "[redacted]")
    }
}
