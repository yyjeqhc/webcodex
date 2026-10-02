//! Bounded HTTP exchange and error classification; no polling or Job lifecycle ownership.
use crate::webcodex_runner::{config::RunnerConfig, contains_any};
use reqwest::blocking::Client;
use std::error::Error as StdError;
use std::io::Read;

/// Polling HTTP responses can carry the current largest 15 MiB request
/// payloads plus their JSON envelope, but must never be loaded without a
/// finite bound.
pub(crate) const RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunnerHttpErrorKind {
    ServerUnavailable,
    Auth,
    NotFound,
    /// A local URL/TLS configuration failure that retrying cannot repair.
    Config,
    /// 4xx (other than auth/endpoint kinds): the server understood the
    /// exchange and rejected this exact request. Resending the identical
    /// payload cannot succeed.
    ClientRejected,
    Status,
    RequestTimeout,
    Request,
    /// The response was incomplete or was recognizably produced by a
    /// temporary proxy/upstream failure.
    DecodeTransient,
    /// The response was complete enough to prove that it does not implement
    /// the expected server protocol.
    ProtocolDecode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunnerHttpError {
    pub(crate) kind: RunnerHttpErrorKind,
    pub(crate) path: String,
    pub(crate) summary: String,
    /// Bounded structured server error, when the response contract supplied
    /// one. Recovery classifiers use this instead of parsing display strings.
    pub(crate) server_error: Option<String>,
}

impl RunnerHttpError {
    pub(crate) fn status(path: &str, status: reqwest::StatusCode, body: &str) -> Self {
        let kind = match status.as_u16() {
            401 | 403 => RunnerHttpErrorKind::Auth,
            404 => RunnerHttpErrorKind::NotFound,
            // Explicitly retryable request-level statuses.
            408 | 429 => RunnerHttpErrorKind::Status,
            code if (500..600).contains(&code) => RunnerHttpErrorKind::ServerUnavailable,
            code if (400..500).contains(&code) => RunnerHttpErrorKind::ClientRejected,
            _ if looks_like_proxy_html_error(body) => RunnerHttpErrorKind::ServerUnavailable,
            _ => RunnerHttpErrorKind::Status,
        };
        let server_error = structured_body_error(body);
        let mut summary = http_status_summary(status);
        if kind == RunnerHttpErrorKind::ClientRejected {
            if let Some(detail) = server_error.as_deref() {
                summary = format!("{}: {}", summary, detail);
            }
        }
        Self {
            kind,
            path: bounded_endpoint_path(path),
            summary,
            server_error,
        }
    }

    pub(crate) fn request(path: &str, error: reqwest::Error) -> Self {
        let chain = error_chain_text(&error);
        let kind = if error.is_builder() || looks_like_fatal_tls_request(&chain) {
            RunnerHttpErrorKind::Config
        } else if looks_like_server_down_request(&error, &chain) {
            RunnerHttpErrorKind::ServerUnavailable
        } else if error.is_timeout() {
            RunnerHttpErrorKind::RequestTimeout
        } else {
            RunnerHttpErrorKind::Request
        };
        Self {
            kind,
            path: bounded_endpoint_path(path),
            summary: request_error_summary(error, &chain),
            server_error: None,
        }
    }

    pub(crate) fn decode_transient(path: &str, summary: String) -> Self {
        Self {
            kind: RunnerHttpErrorKind::DecodeTransient,
            path: bounded_endpoint_path(path),
            summary,
            server_error: None,
        }
    }

    pub(crate) fn protocol_decode(path: &str, summary: String) -> Self {
        Self {
            kind: RunnerHttpErrorKind::ProtocolDecode,
            path: bounded_endpoint_path(path),
            summary,
            server_error: None,
        }
    }
}

impl std::fmt::Display for RunnerHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            RunnerHttpErrorKind::ServerUnavailable => {
                write!(f, "server unavailable for {}: {}", self.path, self.summary)
            }
            RunnerHttpErrorKind::Auth => write!(
                f,
                "authentication failed for {}: {}; check agent token/config",
                self.path, self.summary
            ),
            RunnerHttpErrorKind::NotFound => write!(
                f,
                "endpoint missing or incompatible server for {}: {}",
                self.path, self.summary
            ),
            RunnerHttpErrorKind::Config => {
                write!(
                    f,
                    "HTTP/TLS configuration failed for {}: {}",
                    self.path, self.summary
                )
            }
            RunnerHttpErrorKind::ClientRejected => {
                write!(f, "server rejected {} request: {}", self.path, self.summary)
            }
            RunnerHttpErrorKind::Status
            | RunnerHttpErrorKind::RequestTimeout
            | RunnerHttpErrorKind::Request => {
                write!(f, "{} request failed: {}", self.path, self.summary)
            }
            RunnerHttpErrorKind::DecodeTransient => {
                write!(
                    f,
                    "transient response corruption for {}: {}",
                    self.path, self.summary
                )
            }
            RunnerHttpErrorKind::ProtocolDecode => write!(
                f,
                "response from {} incompatible with server protocol: {}",
                self.path, self.summary
            ),
        }
    }
}

pub(crate) fn http_status_summary(status: reqwest::StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("HTTP {} {}", status.as_u16(), reason),
        None => format!("HTTP {}", status.as_u16()),
    }
}

pub(crate) fn looks_like_proxy_html_error(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("<html")
        && contains_any(
            &lower,
            &[
                "bad gateway",
                "service unavailable",
                "gateway timeout",
                "nginx",
                "upstream",
            ],
        )
}

pub(crate) fn looks_like_server_down_request(error: &reqwest::Error, chain: &str) -> bool {
    if error.is_connect() {
        return true;
    }
    let lower = chain.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "connection refused",
            "connection reset",
            "connection aborted",
            "connection closed",
            "early eof",
            "unexpected eof",
            "incomplete message",
            "broken pipe",
        ],
    )
}

pub(crate) fn looks_like_fatal_tls_request(chain: &str) -> bool {
    let lower = chain.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "certificate verify failed",
            "invalid peer certificate",
            "unknownissuer",
            "notvalidforname",
            "certificateunknown",
            "invalid certificate",
            "no application protocol",
            "alpn mismatch",
        ],
    )
}

pub(crate) fn looks_like_auth_failure_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "unauthorized",
            "forbidden",
            "invalid token",
            "bad token",
            "auth failed",
            "authentication",
        ],
    )
}

pub(crate) fn is_active_instance_lease_conflict(client_id: &str, error: Option<&str>) -> bool {
    let expected = format!(
        "agent client {} is already online with a different instance",
        client_id
    );
    error == Some(expected.as_str())
}

pub(crate) fn is_unknown_polling_session(client_id: &str, error: Option<&str>) -> bool {
    let expected = format!("unknown shell client: {}", client_id);
    error == Some(expected.as_str())
}

pub(crate) fn error_chain_text(error: &reqwest::Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = StdError::source(error);
    while let Some(err) = source {
        parts.push(err.to_string());
        source = err.source();
    }
    parts.join(": ")
}

pub(crate) fn request_error_summary(error: reqwest::Error, chain: &str) -> String {
    let lower = chain.to_ascii_lowercase();
    if lower.contains("connection refused") {
        "connection refused".to_string()
    } else if lower.contains("connection reset") {
        "connection reset".to_string()
    } else if lower.contains("connection aborted") {
        "connection aborted".to_string()
    } else if lower.contains("broken pipe") {
        "broken pipe".to_string()
    } else if contains_any(
        &lower,
        &[
            "connection closed",
            "early eof",
            "unexpected eof",
            "incomplete message",
        ],
    ) {
        "connection closed before response completed".to_string()
    } else if error.is_connect() {
        "connection failed".to_string()
    } else if error.is_timeout() {
        "request timed out".to_string()
    } else {
        bounded_single_line(&error.without_url().to_string())
    }
}

pub(crate) fn bounded_single_line(text: &str) -> String {
    const MAX_CHARS: usize = 160;
    let mut out = String::new();
    let mut last_space = false;
    for ch in text.chars() {
        let ch = if ch.is_whitespace() || ch.is_control() {
            ' '
        } else {
            ch
        };
        if ch == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(ch);
        if out.chars().count() >= MAX_CHARS {
            out.push_str("...");
            break;
        }
    }
    out.trim().to_string()
}

pub(crate) fn bounded_endpoint_path(path: &str) -> String {
    let without_query = path.split_once('?').map_or(path, |(path, _)| path);
    bounded_single_line(without_query)
}

/// Extract the structured `error` field from a JSON error response body, if
/// present. Non-JSON bodies (proxy HTML, truncated payloads) yield `None` so
/// raw response bytes never leak into diagnostics.
pub(crate) fn structured_body_error(body: &str) -> Option<String> {
    const MAX_PARSE_BYTES: usize = 64 * 1024;
    if body.len() > MAX_PARSE_BYTES {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = value.get("error")?.as_str()?;
    let error = bounded_single_line(error);
    if error.is_empty() {
        None
    } else {
        Some(error)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BoundedResponseBody {
    pub(crate) bytes: Vec<u8>,
    pub(crate) exceeded_limit: bool,
}

pub(crate) fn read_bounded_response_body<R: Read>(
    reader: &mut R,
    content_length: Option<u64>,
    max_bytes: usize,
) -> std::io::Result<BoundedResponseBody> {
    let read_limit = (max_bytes as u64).saturating_add(1);
    let initial_capacity = content_length
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default()
        .min(max_bytes.saturating_add(1));
    let mut bytes = Vec::with_capacity(initial_capacity);
    reader.take(read_limit).read_to_end(&mut bytes)?;
    let exceeded_limit = bytes.len() > max_bytes;
    if exceeded_limit {
        bytes.truncate(max_bytes);
    }
    Ok(BoundedResponseBody {
        bytes,
        exceeded_limit,
    })
}

pub(crate) fn bounded_response_content_type(
    value: Option<&reqwest::header::HeaderValue>,
    token: &str,
) -> String {
    match value {
        Some(value) => value
            .to_str()
            .ok()
            .and_then(|value| {
                let media_type = value.split(';').next()?.trim();
                let lower = media_type.to_ascii_lowercase();
                let token = token.trim();
                if media_type.is_empty()
                    || lower.contains("authorization")
                    || lower.contains("bearer")
                    || (!token.is_empty() && media_type.contains(token))
                    || !media_type.chars().all(|ch| {
                        ch.is_ascii_alphanumeric()
                            || matches!(
                                ch,
                                '/' | '!' | '#' | '$' | '&' | '^' | '_' | '.' | '+' | '-'
                            )
                    })
                {
                    None
                } else {
                    Some(bounded_single_line(media_type))
                }
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "<redacted-or-invalid>".to_string()),
        None => "<missing>".to_string(),
    }
}

pub(crate) fn response_decode_summary(
    status: reqwest::StatusCode,
    content_type: &str,
    detail: impl AsRef<str>,
) -> String {
    format!(
        "status={} content_type={} {}",
        http_status_summary(status),
        content_type,
        detail.as_ref()
    )
}

pub(crate) fn looks_like_transient_proxy_response(content_type: &str, body: &[u8]) -> bool {
    const MAX_INSPECT_BYTES: usize = 8 * 1024;
    let inspected = &body[..body.len().min(MAX_INSPECT_BYTES)];
    let text = String::from_utf8_lossy(inspected);
    let lower = text.to_ascii_lowercase();
    let has_temporary_gateway_marker = contains_any(
        &lower,
        &[
            "bad gateway",
            "service unavailable",
            "gateway timeout",
            "upstream connect error",
            "upstream connection error",
            "upstream unavailable",
            "proxy error",
            "temporarily unavailable",
        ],
    );
    let looks_html = content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("text/html"))
        || lower.contains("<html")
        || lower.contains("<!doctype html");
    if looks_html && has_temporary_gateway_marker {
        return true;
    }
    let plain = lower.trim();
    body.len() <= MAX_INSPECT_BYTES
        && (matches!(
            plain,
            "bad gateway"
                | "service unavailable"
                | "gateway timeout"
                | "upstream unavailable"
                | "temporarily unavailable"
        ) || plain.starts_with("upstream connect error")
            || plain.starts_with("upstream connection error"))
}

pub(crate) fn serde_json_category_name(error: &serde_json::Error) -> &'static str {
    match error.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    }
}

pub(crate) fn decode_json_response<R>(
    path: &str,
    status: reqwest::StatusCode,
    content_type: &str,
    body: BoundedResponseBody,
) -> Result<R, RunnerHttpError>
where
    R: serde::de::DeserializeOwned,
{
    if body.bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(RunnerHttpError::decode_transient(
            path,
            response_decode_summary(status, content_type, "empty response body"),
        ));
    }
    if looks_like_transient_proxy_response(content_type, &body.bytes) {
        return Err(RunnerHttpError::decode_transient(
            path,
            response_decode_summary(
                status,
                content_type,
                "recognized temporary proxy/upstream response",
            ),
        ));
    }
    if body.exceeded_limit {
        return Err(RunnerHttpError::protocol_decode(
            path,
            response_decode_summary(
                status,
                content_type,
                format!(
                    "response body exceeds limit_bytes={}",
                    RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES
                ),
            ),
        ));
    }
    serde_json::from_slice(&body.bytes).map_err(|error| {
        let detail = format!(
            "serde_category={} line={} column={}",
            serde_json_category_name(&error),
            error.line(),
            error.column()
        );
        let summary = response_decode_summary(status, content_type, detail);
        if error.is_eof() {
            RunnerHttpError::decode_transient(path, summary)
        } else {
            RunnerHttpError::protocol_decode(path, summary)
        }
    })
}

pub(crate) fn post_json<T, R>(
    client: &Client,
    cfg: &RunnerConfig,
    path: &str,
    body: &T,
) -> Result<R, RunnerHttpError>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    post_json_with_auth(client, &cfg.server_url, &cfg.token, path, body)
}

pub(crate) fn post_json_with_auth<T, R>(
    client: &Client,
    server_url: &str,
    token: &str,
    path: &str,
    body: &T,
) -> Result<R, RunnerHttpError>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    let mut req = client.post(url);
    if !token.trim().is_empty() {
        req = req.bearer_auth(token.trim());
    }
    let resp = req
        .json(body)
        .send()
        .map_err(|e| RunnerHttpError::request(path, e))?;
    let status = resp.status();
    let content_type =
        bounded_response_content_type(resp.headers().get(reqwest::header::CONTENT_TYPE), token);
    let content_length = resp.content_length();
    if content_length.is_some_and(|length| length > RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES as u64) {
        if !status.is_success() {
            return Err(RunnerHttpError::status(path, status, ""));
        }
        return Err(RunnerHttpError::protocol_decode(
            path,
            response_decode_summary(
                status,
                &content_type,
                format!(
                    "declared response body exceeds limit_bytes={}",
                    RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES
                ),
            ),
        ));
    }
    let mut resp = resp;
    let body = match read_bounded_response_body(
        &mut resp,
        content_length,
        RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES,
    ) {
        Ok(body) => body,
        Err(error) if status.is_success() => {
            return Err(RunnerHttpError::decode_transient(
                path,
                response_decode_summary(
                    status,
                    &content_type,
                    format!("response body read interrupted io_kind={:?}", error.kind()),
                ),
            ));
        }
        Err(_) => return Err(RunnerHttpError::status(path, status, "")),
    };
    if !status.is_success() {
        let text = String::from_utf8_lossy(&body.bytes);
        return Err(RunnerHttpError::status(path, status, &text));
    }
    decode_json_response(path, status, &content_type, body)
}
