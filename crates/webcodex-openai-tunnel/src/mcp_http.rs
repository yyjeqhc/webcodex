use crate::{
    command::request_headers,
    control_plane::bounded_body,
    response::response_headers,
    wire::{Command, Headers},
    Error, TunnelClient,
};
use serde_json::{
    json,
    value::{to_raw_value, RawValue},
    Value,
};
use std::sync::atomic::Ordering;

fn terminal(value: &Value, id: &Value) -> bool {
    value.is_object()
        && value["jsonrpc"] == "2.0"
        && value.get("id") == Some(id)
        && (value.get("result").is_some() ^ value.get("error").is_some())
        && value.get("method").is_none()
        && value
            .get("error")
            .is_none_or(|e| e.is_object() && e["code"].is_i64() && e["message"].is_string())
}
fn notification(value: &Value) -> bool {
    value.is_object()
        && value["jsonrpc"] == "2.0"
        && value["method"].is_string()
        && value.get("id").is_none()
        && value.get("result").is_none()
        && value.get("error").is_none()
}

impl TunnelClient {
    pub(crate) async fn forward(&self, command: &Command) -> Result<(), Error> {
        let headers = request_headers(command)?;
        let termination = command.command_type == "session_termination";
        let mut request = if termination {
            self.mcp.delete(self.target.url.clone())
        } else {
            self.mcp.post(self.target.url.clone())
        };
        request = request
            .headers(headers)
            .header("accept", "application/json, text/event-stream");
        if let Some(credential) = &self.target.credential {
            request = request.header("authorization", credential.0.clone());
        }
        if !termination {
            request = request
                .header("content-type", "application/json")
                .body(serde_json::to_vec(&command.jsonrpc).map_err(|_| Error::Protocol)?);
        }
        // Conservative fence is set before the HTTP stack is polled, never cleared
        // by dropping a future, timing out, or failing to deliver a response.
        self.health.0.unsettled.fetch_add(1, Ordering::SeqCst);
        let response = match request.send().await {
            Ok(r) => r,
            Err(_) => return self.failure(command, 502, "client_internal", false).await,
        };
        let code = response.status().as_u16();
        if response.status().is_redirection() {
            return Err(Error::Redirect);
        }
        if !(200..=599).contains(&code) {
            return self.failure(command, 502, "protocol", true).await;
        }
        let headers = response_headers(response.headers());
        let request_value: Option<Value> = command
            .jsonrpc
            .as_ref()
            .map(|v| serde_json::from_str(v.get()).map_err(|_| Error::Protocol))
            .transpose()?;
        let id = request_value.as_ref().and_then(|v| v.get("id"));
        if termination || id.is_none() {
            // Rejection is still a completed HTTP exchange. In particular, an
            // MCP server may reject DELETE with 405 or an expired session with
            // 404; the control plane must receive that terminal status too.
            self.deliver(
                command,
                code,
                if termination {
                    "session_termination_response"
                } else {
                    "notify_ack"
                },
                None,
                headers,
            )
            .await?;
        } else {
            let id = id.ok_or(Error::Protocol)?;
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if content_type == "text/event-stream" {
                match self.sse(command, response, code, headers, id).await {
                    Ok(()) => {}
                    Err(Error::Protocol | Error::Capacity | Error::Transport) => {
                        // The MCP stream ended without a deliverable result. Do
                        // not replay the dispatch or wait for the caller to time
                        // out. Keep the uncertainty fence after reporting it.
                        return self.failure(command, 502, "protocol", true).await;
                    }
                    // An attempted terminal delivery may already have arrived.
                    // Never replace it with a second, different terminal result.
                    Err(error) => return Err(error),
                }
            } else {
                let body = match bounded_body(response, self.limits.body_bytes).await {
                    Ok(body) => body,
                    Err(_) => return self.failure(command, 502, "protocol", true).await,
                };
                match serde_json::from_slice::<Value>(&body) {
                    Ok(value) if terminal(&value, id) => {
                        self.deliver(
                            command,
                            code,
                            "jsonrpc_response",
                            Some(
                                serde_json::from_slice::<Box<RawValue>>(&body)
                                    .map_err(|_| Error::Protocol)?,
                            ),
                            headers,
                        )
                        .await?
                    }
                    _ => {
                        return self
                            .failure(
                                command,
                                if code >= 400 { code } else { 502 },
                                if code >= 400 {
                                    "target_http"
                                } else {
                                    "protocol"
                                },
                                true,
                            )
                            .await
                    }
                }
            }
        }
        self.health.0.unsettled.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
    async fn failure(
        &self,
        command: &Command,
        code: u16,
        source: &str,
        received: bool,
    ) -> Result<(), Error> {
        let request_value: Option<Value> = command
            .jsonrpc
            .as_ref()
            .map(|v| serde_json::from_str(v.get()).map_err(|_| Error::Protocol))
            .transpose()?;
        // This does not imply that an effect did not occur. Keep the uncertainty fence.
        let Some(id) = request_value.as_ref().and_then(|v| v.get("id")) else {
            return self
                .deliver(
                    command,
                    code,
                    if command.command_type == "session_termination" {
                        "session_termination_response"
                    } else {
                        "notify_ack"
                    },
                    None,
                    Headers::new(),
                )
                .await;
        };
        let mut provenance =
            json!({"version":1,"source":source,"upstream_response_received":received});
        if source == "target_http" {
            provenance["upstream_status"] = json!(code);
        }
        let value = json!({"jsonrpc":"2.0","id":id,"error":{"code":-32603,"message":"Tunnel target exchange failed",
            "data":{"tunnel_failure":provenance}}});
        self.deliver(
            command,
            code,
            "jsonrpc_response",
            Some(to_raw_value(&value).map_err(|_| Error::Protocol)?),
            Headers::new(),
        )
        .await
    }
    async fn sse(
        &self,
        command: &Command,
        mut response: reqwest::Response,
        code: u16,
        headers: Headers,
        id: &Value,
    ) -> Result<(), Error> {
        let mut decoder = Sse::new(self.limits.event_bytes);
        let mut notifications = true;
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            // Feed bytewise: one large HTTP chunk must not create an unbounded event queue.
            for byte in chunk {
                if let Some(data) = decoder.push(byte)? {
                    // MCP permits an empty-data SSE priming event carrying an
                    // event ID. It is framing metadata, not a JSON-RPC message.
                    if data.is_empty() {
                        continue;
                    }
                    let value: Value =
                        serde_json::from_slice(&data).map_err(|_| Error::Protocol)?;
                    let raw = serde_json::from_slice::<Box<RawValue>>(&data)
                        .map_err(|_| Error::Protocol)?;
                    if terminal(&value, id) {
                        return self
                            .deliver(command, code, "jsonrpc_response", Some(raw), headers)
                            .await;
                    }
                    if !notification(&value) {
                        return Err(Error::Protocol);
                    }
                    if notifications {
                        match self
                            .deliver(command, code, "jsonrpc_notify", Some(raw), headers.clone())
                            .await
                        {
                            Ok(()) => {}
                            Err(e @ (Error::Authentication | Error::Redirect)) => return Err(e),
                            Err(_) => notifications = false,
                        }
                    }
                }
            }
        }
        Err(Error::Protocol)
    }
}

/// Supports LF, CRLF and CR line endings, multiline data, comments, and UTF-8 BOM.
struct Sse {
    line: Vec<u8>,
    data: Vec<u8>,
    max: usize,
    cr: bool,
    first: bool,
}
impl Sse {
    fn new(max: usize) -> Self {
        Self {
            line: vec![],
            data: vec![],
            max,
            cr: false,
            first: true,
        }
    }
    fn push(&mut self, b: u8) -> Result<Option<Vec<u8>>, Error> {
        if b == b'\n' && self.cr {
            self.cr = false;
            return Ok(None);
        }
        self.cr = b == b'\r';
        if b != b'\n' && b != b'\r' {
            if self.line.len() + self.data.len() >= self.max {
                return Err(Error::Capacity);
            }
            self.line.push(b);
            return Ok(None);
        }
        let mut line = std::mem::take(&mut self.line);
        if self.first {
            self.first = false;
            if line.starts_with(&[0xef, 0xbb, 0xbf]) {
                line.drain(..3);
            }
        }
        if line.is_empty() {
            if self.data.is_empty() {
                return Ok(None);
            }
            self.data.pop();
            return Ok(Some(std::mem::take(&mut self.data)));
        }
        let colon = line.iter().position(|b| *b == b':').unwrap_or(line.len());
        if &line[..colon] == b"data" {
            let mut value = line.get(colon + 1..).unwrap_or_default();
            if value.first() == Some(&b' ') {
                value = &value[1..];
            }
            if self.data.len() + value.len() + 1 > self.max {
                return Err(Error::Capacity);
            }
            self.data.extend_from_slice(value);
            self.data.push(b'\n');
        }
        Ok(None)
    }
}
