use crate::{wire::Command, Error, TunnelClient};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::sync::atomic::Ordering;
use tokio::time::{timeout_at, Instant};

pub(crate) fn request_headers(command: &Command) -> Result<HeaderMap, Error> {
    let mut out = HeaderMap::new();
    let mut hop_by_hop = Vec::new();
    let mut bytes = 0usize;
    if command.headers.len() > 32 {
        return Err(Error::Protocol);
    }
    for (key, values) in &command.headers {
        let name = HeaderName::from_bytes(key.as_bytes()).map_err(|_| Error::Protocol)?;
        // A command cannot replace the bound local credential. Ingress proxy
        // and tracing metadata, however, is normal on real control-plane polls:
        // validate/bound it and discard it rather than rejecting the command.
        if matches!(
            name.as_str(),
            "authorization" | "proxy-authorization" | "cookie" | "cookie2"
        ) || values.len() > 32
        {
            return Err(Error::Protocol);
        }
        bytes = bytes.saturating_add(key.len());
        if bytes > 8192 {
            return Err(Error::Protocol);
        }
        for value in values {
            bytes = bytes.saturating_add(value.len());
            if bytes > 8192 {
                return Err(Error::Protocol);
            }
            HeaderValue::from_str(value).map_err(|_| Error::Protocol)?;
            if name == "connection" {
                for token in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    hop_by_hop.push(
                        HeaderName::from_bytes(token.as_bytes()).map_err(|_| Error::Protocol)?,
                    );
                }
            }
        }
        if matches!(
            name.as_str(),
            "mcp-session-id"
                | "mcp-protocol-version"
                | "last-event-id"
                | "mcp-method"
                | "mcp-name"
                | "x-openai-session"
                | "x-openai-subject"
        ) {
            if out.contains_key(&name) || values.len() != 1 {
                return Err(Error::Protocol);
            }
            out.insert(
                name,
                HeaderValue::from_str(&values[0]).map_err(|_| Error::Protocol)?,
            );
        }
    }
    // No incoming hop-by-hop nomination can turn metadata into a forwarded
    // connection-specific field. Host, framing and proxy identity stay local.
    for name in hop_by_hop {
        out.remove(name);
    }
    Ok(out)
}
pub(crate) fn validate(command: &Command) -> Result<bool, Error> {
    if command.request_id.is_empty()
        || command.request_id.len() > 256
        || command.shard_token.is_empty()
        || command.shard_token.len() > 8192
        || HeaderValue::from_str(&command.shard_token).is_err()
        || command.channel != "main"
    {
        return Err(Error::Protocol);
    }
    match command.command_type.as_str() {
        "jsonrpc" => {
            let raw = command.jsonrpc.as_ref().ok_or(Error::Protocol)?;
            let v: serde_json::Value =
                serde_json::from_str(raw.get()).map_err(|_| Error::Protocol)?;
            if !v.is_object()
                || v["jsonrpc"] != "2.0"
                || !v["method"].is_string()
                || v.get("result").is_some()
                || v.get("error").is_some()
                || v.get("id")
                    .is_some_and(|id| !(id.is_string() || id.is_i64() || id.is_u64()))
            {
                return Err(Error::Protocol);
            }
        }
        "session_termination" => {
            if command.jsonrpc.is_some()
                || !request_headers(command)?.contains_key("mcp-session-id")
            {
                return Err(Error::Protocol);
            }
        }
        _ => return Ok(false),
    }
    request_headers(command)?;
    Ok(true)
}
impl TunnelClient {
    pub(crate) async fn execute(&self, command: Command, received: Instant) -> Result<(), Error> {
        let deadline = match self.policy.deadline(command.response_timeout, received) {
            Ok(d) => d,
            Err(_) => {
                self.health.0.rejected.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
        };
        if deadline.is_some_and(|d| d <= Instant::now()) {
            return Ok(());
        }
        // The only dispatch entry; all retry loops are strictly downstream of it.
        let operation = self.forward(&command);
        let result = match deadline {
            Some(d) => timeout_at(d, operation)
                .await
                .unwrap_or(Err(Error::Deadline)),
            None => operation.await,
        };
        match result {
            Err(Error::Authentication | Error::Redirect) => result,
            // Receipt remains retained even when execution/delivery could not finish.
            _ => Ok(()),
        }
    }
}
