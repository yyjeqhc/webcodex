use crate::{
    control_plane::{backoff, retry_after},
    wire::{Command, Headers, Response},
    Error, TunnelClient,
};
use reqwest::header::HeaderMap;
use serde_json::value::RawValue;
use tokio::time::sleep;

pub(crate) fn response_headers(headers: &HeaderMap) -> Headers {
    let nominated: Vec<_> = headers
        .get_all("connection")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|v| v.trim().to_ascii_lowercase())
        .collect();
    let mut out = Headers::new();
    for name in [
        "content-type",
        "mcp-session-id",
        "mcp-protocol-version",
        "last-event-id",
        "access-control-expose-headers",
        "www-authenticate",
    ] {
        if nominated.iter().any(|n| n == name) {
            continue;
        }
        let values: Vec<_> = headers
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect();
        if !values.is_empty() {
            out.insert(name.into(), values);
        }
    }
    out
}
impl TunnelClient {
    pub(crate) async fn deliver(
        &self,
        command: &Command,
        code: u16,
        kind: &'static str,
        value: Option<Box<RawValue>>,
        headers: Headers,
    ) -> Result<(), Error> {
        let body = serde_json::to_vec(&Response {
            request_id: &command.request_id,
            channel: &command.channel,
            resp_json: value,
            resp_headers: headers,
            resp_code: code,
            resp_type: kind,
        })
        .map_err(|_| Error::Protocol)?;
        if body.len() > self.limits.body_bytes {
            return Err(Error::Capacity);
        }
        let notification = kind == "jsonrpc_notify";
        for attempt in 0..self.limits.response_attempts {
            let result = self
                .control
                .post(self.endpoint("response"))
                .header("X-Tunnel-Shard-Token", &command.shard_token)
                .header("content-type", "application/json")
                .body(body.clone())
                .timeout(self.limits.poll_timeout)
                .send()
                .await;
            let delay = match result {
                Ok(response) => {
                    let status = response.status().as_u16();
                    match status {
                        200 | 404 => return Ok(()),
                        401 | 403 => return Err(Error::Authentication),
                        300..=399 => return Err(Error::Redirect),
                        429 => backoff(attempt).max(retry_after(response.headers())),
                        408 | 502 | 503 | 504 if !notification => {
                            backoff(attempt).max(retry_after(response.headers()))
                        }
                        _ => return Err(Error::Delivery),
                    }
                }
                // Reqwest exposes no reliable write fence. Conservatively treat every
                // notification transport failure as ambiguous, never retry it.
                Err(_) if notification => return Err(Error::Delivery),
                Err(_) => backoff(attempt),
            };
            if attempt + 1 == self.limits.response_attempts {
                break;
            }
            sleep(delay).await;
        }
        Err(Error::Delivery)
    }
}
