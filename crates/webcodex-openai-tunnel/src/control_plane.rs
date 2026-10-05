use crate::{
    wire::{Command, Envelope},
    Error, TunnelClient,
};
use reqwest::{header::HeaderMap, Response};
use std::time::{Duration, SystemTime};
use tokio::time::Instant;

pub(crate) async fn bounded_body(mut response: Response, max: usize) -> Result<Vec<u8>, Error> {
    if response.content_length().is_some_and(|n| n > max as u64) {
        return Err(Error::Capacity);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
        if chunk.len() > max.saturating_sub(body.len()) {
            return Err(Error::Capacity);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
pub(crate) fn retry_after(headers: &HeaderMap) -> Duration {
    let parsed = headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.parse::<u64>().ok().map(Duration::from_secs).or_else(|| {
                httpdate::parse_http_date(v)
                    .ok()?
                    .duration_since(SystemTime::now())
                    .ok()
            })
        });
    parsed.unwrap_or_default().min(Duration::from_secs(60))
}
pub(crate) fn backoff(attempt: u32) -> Duration {
    let base = 200u64.saturating_mul(1 << attempt.min(6)).min(10_000);
    let jitter = (uuid::Uuid::new_v4().as_u128() % 101) as u64;
    Duration::from_millis(base + jitter)
}
pub(crate) struct PollFailure {
    pub error: Error,
    pub delay: Duration,
}
impl From<Error> for PollFailure {
    fn from(error: Error) -> Self {
        Self {
            error,
            delay: Duration::ZERO,
        }
    }
}
impl TunnelClient {
    pub(crate) fn endpoint(&self, operation: &str) -> String {
        format!(
            "{}v1/tunnels/{}/{operation}",
            self.cp.origin, self.cp.tunnel
        )
    }
    pub(crate) async fn poll(&self, limit: usize) -> Result<(Instant, Vec<Command>), PollFailure> {
        let response = self
            .control
            .get(self.endpoint("poll"))
            .query(&[
                ("limit", limit.min(25).to_string()),
                ("timeout_ms", "15000".into()),
            ])
            .timeout(self.limits.poll_timeout)
            .send()
            .await
            .map_err(|_| Error::Transport)?;
        // Capture headers receipt before decoding, including all time spent reading the body.
        let received = Instant::now();
        let status = response.status().as_u16();
        match status {
            204 => Ok((received, vec![])),
            200 => {
                let body = bounded_body(response, self.limits.body_bytes).await?;
                let envelope: Envelope =
                    serde_json::from_slice(&body).map_err(|_| Error::Protocol)?;
                if envelope.commands.len() > self.limits.ingress_commands {
                    return Err(Error::Capacity.into());
                }
                Ok((received, envelope.commands))
            }
            401 | 403 => Err(Error::Authentication.into()),
            300..=399 => Err(Error::Redirect.into()),
            408 | 429 | 500..=599 => Err(PollFailure {
                error: Error::Transport,
                delay: retry_after(response.headers()),
            }),
            _ => Err(Error::Protocol.into()),
        }
    }
}
