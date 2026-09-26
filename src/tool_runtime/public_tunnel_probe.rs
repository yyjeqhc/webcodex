use super::{ToolResult, ToolRuntime};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROBE_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const PROBE_REQUEST_TIMEOUT: Duration = Duration::from_secs(4);
const PROBE_TOTAL_TIMEOUT: Duration = Duration::from_secs(8);
const PROBE_MIN_REFRESH_SECS: i64 = 5;
const PROBE_STALE_AFTER_SECS: i64 = 300;
const MAX_OPENAPI_BODY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublicTunnelProbeSnapshot {
    pub(crate) observed_at: i64,
    pub(crate) latency_ms: u64,
    pub(crate) verified: bool,
    pub(crate) reason_code: &'static str,
    pub(crate) openapi_status: Option<u16>,
    pub(crate) protected_status: Option<u16>,
    pub(crate) tls_verified: bool,
}

impl PublicTunnelProbeSnapshot {
    fn status(&self) -> &'static str {
        if self.reason_code == "not_configured" {
            "not_configured"
        } else if self.verified {
            "verified"
        } else {
            "degraded"
        }
    }

    fn as_json(&self, now: i64, cached: bool) -> Value {
        let age_secs = now.saturating_sub(self.observed_at).max(0);
        json!({
            "status": self.status(),
            "verified": self.verified,
            "reason_code": self.reason_code,
            "observed_at": self.observed_at,
            "age_secs": age_secs,
            "stale_after_secs": PROBE_STALE_AFTER_SECS,
            "stale": age_secs > PROBE_STALE_AFTER_SECS,
            "latency_ms": self.latency_ms,
            "openapi_status": self.openapi_status,
            "protected_status": self.protected_status,
            "tls_verified": self.tls_verified,
            "cached": cached,
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct PublicTunnelProbeState {
    snapshot: Mutex<Option<PublicTunnelProbeSnapshot>>,
    gate: tokio::sync::Mutex<()>,
}

impl PublicTunnelProbeState {
    pub(crate) fn snapshot(&self) -> Option<PublicTunnelProbeSnapshot> {
        self.snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) fn health_projection(&self, configured: bool) -> Value {
        if !configured {
            return json!({"status": "not_configured"});
        }
        let Some(snapshot) = self.snapshot() else {
            return json!({"status": "configured_unverified"});
        };
        let now = now_ts();
        let age_secs = now.saturating_sub(snapshot.observed_at).max(0);
        let status = if age_secs > PROBE_STALE_AFTER_SECS {
            "stale"
        } else if snapshot.verified {
            "verified"
        } else {
            "degraded"
        };
        json!({
            "status": status,
            "verified": snapshot.verified,
            "reason_code": snapshot.reason_code,
            "observed_at": snapshot.observed_at,
            "age_secs": age_secs,
            "stale_after_secs": PROBE_STALE_AFTER_SECS,
            "latency_ms": snapshot.latency_ms,
            "openapi_status": snapshot.openapi_status,
            "protected_status": snapshot.protected_status,
            "tls_verified": snapshot.tls_verified,
        })
    }

    pub(crate) async fn probe(&self, configured_public_url: Option<&str>) -> Value {
        let _guard = self.gate.lock().await;
        let now = now_ts();
        if let Some(snapshot) = self.snapshot() {
            if now.saturating_sub(snapshot.observed_at) < PROBE_MIN_REFRESH_SECS {
                return snapshot.as_json(now, true);
            }
        }

        let snapshot = match configured_public_url {
            Some(origin) => probe_origin(origin).await,
            None => PublicTunnelProbeSnapshot {
                observed_at: now,
                latency_ms: 0,
                verified: false,
                reason_code: "not_configured",
                openapi_status: None,
                protected_status: None,
                tls_verified: false,
            },
        };
        let previous = {
            let mut slot = self
                .snapshot
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let previous = slot.clone();
            *slot = Some(snapshot.clone());
            previous
        };
        record_probe_transition(previous.as_ref(), &snapshot);
        snapshot.as_json(now_ts(), false)
    }
}

impl ToolRuntime {
    pub(crate) async fn public_tunnel_probe(&self) -> ToolResult {
        let mut output = self
            .runtime_info
            .public_tunnel_probe
            .probe(self.runtime_info.configured_public_url.as_deref())
            .await;
        if let Some(object) = output.as_object_mut() {
            object.insert("state_changed".to_string(), Value::Bool(false));
        }
        ToolResult::ok(output)
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

fn record_probe_transition(
    previous: Option<&PublicTunnelProbeSnapshot>,
    current: &PublicTunnelProbeSnapshot,
) {
    if current.reason_code == "not_configured" {
        return;
    }
    if !current.verified && previous.is_none_or(|value| value.verified) {
        webcodex_core::runtime_diagnostics::record(
            webcodex_core::runtime_diagnostics::DiagnosticSeverity::Warn,
            "public_tunnel",
            "probe_failed",
            None,
        );
    } else if current.verified && previous.is_some_and(|value| !value.verified) {
        webcodex_core::runtime_diagnostics::record(
            webcodex_core::runtime_diagnostics::DiagnosticSeverity::Info,
            "public_tunnel",
            "probe_recovered",
            None,
        );
    }
}

async fn probe_origin(origin: &str) -> PublicTunnelProbeSnapshot {
    let started = Instant::now();
    let parsed = match reqwest::Url::parse(origin) {
        Ok(url)
            if matches!(url.scheme(), "http" | "https")
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none() =>
        {
            url
        }
        _ => return failed_snapshot(started, "invalid_public_url", None, None, false),
    };
    let tls_expected = parsed.scheme() == "https";
    match tokio::time::timeout(
        PROBE_TOTAL_TIMEOUT,
        probe_origin_inner(origin, tls_expected),
    )
    .await
    {
        Ok(snapshot) => snapshot,
        Err(_) => failed_snapshot(started, "probe_deadline_exceeded", None, None, false),
    }
}

async fn probe_origin_inner(origin: &str, tls_expected: bool) -> PublicTunnelProbeSnapshot {
    let started = Instant::now();
    let client = match reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(PROBE_CONNECT_TIMEOUT)
        .timeout(PROBE_REQUEST_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(_) => return failed_snapshot(started, "client_init_failed", None, None, false),
    };
    let base = origin.trim_end_matches('/');
    let openapi = match client.get(format!("{base}/openapi.json")).send().await {
        Ok(response) => response,
        Err(_) => return failed_snapshot(started, "openapi_request_failed", None, None, false),
    };
    let openapi_status = openapi.status().as_u16();
    let tls_verified = tls_expected;
    if !openapi.status().is_success() {
        return failed_snapshot(
            started,
            "openapi_http_status",
            Some(openapi_status),
            None,
            tls_verified,
        );
    }
    let body = match bounded_body(openapi).await {
        Ok(body) => body,
        Err(reason) => {
            return failed_snapshot(started, reason, Some(openapi_status), None, tls_verified)
        }
    };
    let openapi_json: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => {
            return failed_snapshot(
                started,
                "openapi_invalid_json",
                Some(openapi_status),
                None,
                tls_verified,
            )
        }
    };
    if openapi_json.pointer("/info/title").and_then(Value::as_str) != Some("WebPi GPT Actions") {
        return failed_snapshot(
            started,
            "openapi_identity_mismatch",
            Some(openapi_status),
            None,
            tls_verified,
        );
    }

    let protected = match client
        .post(format!("{base}/api/actions/runtime_status"))
        .json(&json!({"compact": true}))
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return failed_snapshot(
                started,
                "protected_request_failed",
                Some(openapi_status),
                None,
                tls_verified,
            )
        }
    };
    let protected_status = protected.status().as_u16();
    if !matches!(protected_status, 401 | 403) {
        return failed_snapshot(
            started,
            "protected_not_rejected",
            Some(openapi_status),
            Some(protected_status),
            tls_verified,
        );
    }

    PublicTunnelProbeSnapshot {
        observed_at: now_ts(),
        latency_ms: elapsed_ms(started),
        verified: true,
        reason_code: "verified",
        openapi_status: Some(openapi_status),
        protected_status: Some(protected_status),
        tls_verified,
    }
}

async fn bounded_body(mut response: reqwest::Response) -> Result<Vec<u8>, &'static str> {
    if response
        .content_length()
        .is_some_and(|bytes| bytes > MAX_OPENAPI_BODY_BYTES as u64)
    {
        return Err("openapi_body_too_large");
    }
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if body.len().saturating_add(chunk.len()) <= MAX_OPENAPI_BODY_BYTES => {
                body.extend_from_slice(&chunk)
            }
            Ok(Some(_)) => return Err("openapi_body_too_large"),
            Ok(None) => return Ok(body),
            Err(_) => return Err("openapi_body_read_failed"),
        }
    }
}

fn failed_snapshot(
    started: Instant,
    reason_code: &'static str,
    openapi_status: Option<u16>,
    protected_status: Option<u16>,
    tls_verified: bool,
) -> PublicTunnelProbeSnapshot {
    PublicTunnelProbeSnapshot {
        observed_at: now_ts(),
        latency_ms: elapsed_ms(started),
        verified: false,
        reason_code,
        openapi_status,
        protected_status,
        tls_verified,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn test_origin(protected_status: u16) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0_u8; 8192];
                let read = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..read]);
                let (status, body) = if request.starts_with("GET /openapi.json ") {
                    (
                        200_u16,
                        r#"{"info":{"title":"WebPi GPT Actions"}}"#.to_string(),
                    )
                } else {
                    (protected_status, r#"{"error":"unauthorized"}"#.to_string())
                };
                let reason = match status {
                    200 => "OK",
                    401 => "Unauthorized",
                    403 => "Forbidden",
                    _ => "Other",
                };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn probe_verifies_openapi_identity_and_unauthenticated_boundary() {
        let origin = test_origin(401).await;
        let snapshot = probe_origin(&origin).await;
        assert!(snapshot.verified);
        assert_eq!(snapshot.reason_code, "verified");
        assert_eq!(snapshot.openapi_status, Some(200));
        assert_eq!(snapshot.protected_status, Some(401));
        assert!(!snapshot.tls_verified);
    }

    #[tokio::test]
    async fn probe_rejects_public_origin_that_accepts_unauthenticated_action() {
        let origin = test_origin(200).await;
        let snapshot = probe_origin(&origin).await;
        assert!(!snapshot.verified);
        assert_eq!(snapshot.reason_code, "protected_not_rejected");
        assert_eq!(snapshot.protected_status, Some(200));
    }

    #[tokio::test]
    async fn probe_state_reuses_fresh_cached_evidence() {
        let state = PublicTunnelProbeState::default();
        let origin = test_origin(401).await;
        let first = state.probe(Some(&origin)).await;
        let second = state.probe(Some(&origin)).await;
        assert_eq!(first["verified"], true);
        assert_eq!(first["cached"], false);
        assert_eq!(second["verified"], true);
        assert_eq!(second["cached"], true);
        assert_eq!(first["observed_at"], second["observed_at"]);
    }
}
