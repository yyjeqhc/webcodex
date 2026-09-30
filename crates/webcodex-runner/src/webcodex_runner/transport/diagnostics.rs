//! Transport error classification and credential-safe logging.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunnerTransportError {
    Transient(String),
    ProxyConfiguration(String),
    Fatal(String),
}

impl RunnerTransportError {
    pub(super) fn transient(message: impl Into<String>) -> Self {
        Self::Transient(message.into())
    }

    pub(super) fn proxy_configuration(message: impl Into<String>) -> Self {
        Self::ProxyConfiguration(message.into())
    }

    pub(super) fn fatal(message: impl Into<String>) -> Self {
        Self::Fatal(message.into())
    }

    pub(super) fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }

    pub(super) fn is_proxy_configuration(&self) -> bool {
        matches!(self, Self::ProxyConfiguration(_))
    }

    pub(super) fn into_message(self) -> String {
        match self {
            Self::Transient(message) | Self::ProxyConfiguration(message) | Self::Fatal(message) => {
                message
            }
        }
    }
}

impl fmt::Display for RunnerTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient(message) | Self::ProxyConfiguration(message) | Self::Fatal(message) => {
                f.write_str(message)
            }
        }
    }
}

impl From<String> for RunnerTransportError {
    fn from(message: String) -> Self {
        classify_session_error(message)
    }
}

pub(super) fn is_fatal_auth_or_register_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "register rejected",
            "register_failed",
            "register_forbidden",
            "unauthorized",
            "forbidden",
            "invalid token",
            "bad token",
            "auth failed",
            "authentication",
            "expected registered ack",
            "register ack was not text",
            "register ack is not a valid envelope",
        ],
    )
}

pub(super) fn is_fatal_config_or_tls_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "invalid websocket url",
            "server_url must be",
            "transport=quic requires",
            "[quic]",
            "certificate",
            "webpki",
            "notvalidforname",
            "unknownissuer",
            "invalid server name",
            "invalid dns",
            "no application protocol",
            "alpn mismatch",
        ],
    )
}

pub(super) fn classify_session_error(message: impl Into<String>) -> RunnerTransportError {
    let message = message.into();
    if is_fatal_auth_or_register_error(&message) || is_fatal_config_or_tls_error(&message) {
        RunnerTransportError::fatal(message)
    } else {
        RunnerTransportError::transient(message)
    }
}

pub(super) fn redact_url_queries(message: &str) -> String {
    let mut remaining = message;
    let mut redacted = String::with_capacity(message.len());
    loop {
        let http = remaining.find("http://");
        let https = remaining.find("https://");
        let url_start = match (http, https) {
            (Some(http), Some(https)) => http.min(https),
            (Some(http), None) => http,
            (None, Some(https)) => https,
            (None, None) => {
                redacted.push_str(remaining);
                break;
            }
        };
        redacted.push_str(&remaining[..url_start]);
        let url = &remaining[url_start..];
        let url_end = url.find(char::is_whitespace).unwrap_or(url.len());
        let segment = &url[..url_end];
        if let Some(query_start) = segment.find('?') {
            redacted.push_str(&segment[..query_start]);
            redacted.push_str("?[redacted]");
        } else {
            redacted.push_str(segment);
        }
        remaining = &url[url_end..];
    }
    redacted
}

pub(super) fn concise_log_error(message: &str, token: &str) -> String {
    let mut sanitized = redact_url_queries(message).replace(['\r', '\n'], " ");
    let token = token.trim();
    if !token.is_empty() {
        sanitized = sanitized.replace(token, "[redacted]");
    }
    const MAX_CHARS: usize = 180;
    if sanitized.chars().count() > MAX_CHARS {
        let mut out = sanitized.chars().take(MAX_CHARS).collect::<String>();
        out.push_str("...");
        out
    } else {
        sanitized
    }
}

pub(super) fn server_log_label(server_url: &str) -> String {
    match url::Url::parse(server_url) {
        Ok(parsed) => {
            let Some(host) = parsed.host_str() else {
                return parsed.scheme().to_string();
            };
            let host = if host.contains(':') && !host.starts_with('[') {
                format!("[{}]", host)
            } else {
                host.to_string()
            };
            match parsed.port() {
                Some(port) => format!("{}://{}:{}", parsed.scheme(), host, port),
                None => format!("{}://{}", parsed.scheme(), host),
            }
        }
        Err(_) => server_url
            .split('?')
            .next()
            .unwrap_or(server_url)
            .trim_end_matches('/')
            .to_string(),
    }
}

pub(super) fn enabled_projects_count(projects: &[RunnerProjectSummary]) -> usize {
    projects.iter().filter(|project| !project.disabled).count()
}

pub(super) fn registered_log_line(
    cfg: &RunnerConfig,
    actual_transport: &str,
    projects_count: usize,
) -> String {
    format!(
        "webcodex-runner registered client_id={} server={} preferred_transport={} actual_transport={} projects={}",
        cfg.client_id,
        server_log_label(&cfg.server_url),
        effective_transport(cfg),
        actual_transport,
        projects_count
    )
}

pub(super) fn auto_quic_not_configured_log_line() -> &'static str {
    "webcodex-runner transport auto: quic not configured; skipping"
}

pub(super) fn auto_trying_log_line(transport: &str) -> String {
    format!("webcodex-runner transport auto: {} trying", transport)
}
