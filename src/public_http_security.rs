//! WebPi public-host hardening for Custom GPT Actions.
//!
//! The standalone WebPi deployment is intentionally loopback-bound and reached
//! through Cloudflare Tunnel. When explicitly enabled, requests arriving via
//! the configured public host (or carrying Cloudflare's edge-only
//! CF-Connecting-IP header) are restricted to the OpenAPI document and GPT
//! Actions. Loopback callers retain the full local API/MCP/admin surface.
//!
//! Invalid-auth rate limiting is deliberately separate from normal Action
//! throughput: only failed public authentication attempts are counted. A valid
//! PAT never enters the limiter, so coding bursts and Runner concurrency are
//! unaffected.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use salvo::http::{HeaderValue, Method, StatusCode};
use salvo::prelude::*;

const PUBLIC_ACTIONS_ONLY_ENV: &str = "WEBPI_PUBLIC_ACTIONS_ONLY";
const INVALID_AUTH_RATE_LIMIT_ENABLED_ENV: &str = "WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_ENABLED";
const INVALID_AUTH_RATE_LIMIT_MAX_ENV: &str = "WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_MAX";
const INVALID_AUTH_RATE_LIMIT_WINDOW_SECS_ENV: &str =
    "WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_WINDOW_SECS";
const INVALID_AUTH_RATE_LIMIT_PENALTY_SECS_ENV: &str =
    "WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_PENALTY_SECS";
const CF_CONNECTING_IP: &str = "cf-connecting-ip";
const INVALID_AUTH_CLIENT_CAP: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InvalidAuthRateLimitPolicy {
    pub(crate) max_failures: u32,
    pub(crate) window_secs: u64,
    pub(crate) penalty_secs: u64,
}

impl Default for InvalidAuthRateLimitPolicy {
    fn default() -> Self {
        Self {
            // High enough that a normal private GPT can never hit it
            // accidentally, while still bounding brute-force/error floods.
            max_failures: 120,
            window_secs: 60,
            penalty_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct InvalidAuthClientState {
    window_started_at: u64,
    failures: u32,
    blocked_until: u64,
}

#[derive(Debug, Default)]
pub(crate) struct InvalidAuthLimiter {
    clients: HashMap<String, InvalidAuthClientState>,
}

impl InvalidAuthLimiter {
    pub(crate) fn record_failure_at(
        &mut self,
        client: &str,
        now: u64,
        policy: InvalidAuthRateLimitPolicy,
    ) -> Option<u64> {
        let key =
            if self.clients.contains_key(client) || self.clients.len() < INVALID_AUTH_CLIENT_CAP {
                client
            } else {
                "__overflow__"
            }
            .to_string();

        let state = self.clients.entry(key).or_insert(InvalidAuthClientState {
            window_started_at: now,
            failures: 0,
            blocked_until: 0,
        });

        if state.blocked_until > now {
            return Some(state.blocked_until.saturating_sub(now).max(1));
        }
        if state.blocked_until != 0
            || now.saturating_sub(state.window_started_at) >= policy.window_secs
        {
            *state = InvalidAuthClientState {
                window_started_at: now,
                failures: 0,
                blocked_until: 0,
            };
        }

        state.failures = state.failures.saturating_add(1);
        if state.failures > policy.max_failures {
            state.blocked_until = now.saturating_add(policy.penalty_secs);
            return Some(policy.penalty_secs.max(1));
        }
        None
    }
}

static INVALID_AUTH_LIMITER: OnceLock<Mutex<InvalidAuthLimiter>> = OnceLock::new();

pub(crate) fn public_actions_only_enabled() -> bool {
    crate::config::env_flag(PUBLIC_ACTIONS_ONLY_ENV).unwrap_or(false)
}

fn public_invalid_auth_rate_limit_enabled() -> bool {
    crate::config::env_flag(INVALID_AUTH_RATE_LIMIT_ENABLED_ENV).unwrap_or(false)
}

fn bounded_env_u64(name: &str, default: u64, min: u64, max: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|value| value.clamp(min, max))
        .unwrap_or(default)
}

fn invalid_auth_rate_limit_policy() -> InvalidAuthRateLimitPolicy {
    let default = InvalidAuthRateLimitPolicy::default();
    InvalidAuthRateLimitPolicy {
        max_failures: bounded_env_u64(
            INVALID_AUTH_RATE_LIMIT_MAX_ENV,
            u64::from(default.max_failures),
            20,
            10_000,
        ) as u32,
        window_secs: bounded_env_u64(
            INVALID_AUTH_RATE_LIMIT_WINDOW_SECS_ENV,
            default.window_secs,
            10,
            3600,
        ),
        penalty_secs: bounded_env_u64(
            INVALID_AUTH_RATE_LIMIT_PENALTY_SECS_ENV,
            default.penalty_secs,
            10,
            3600,
        ),
    }
}

fn normalize_domain_or_ip(value: &str) -> Option<String> {
    if let Ok(address) = value.parse::<IpAddr>() {
        return Some(address.to_string());
    }
    match url::Host::parse(value).ok()? {
        url::Host::Domain(domain) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            (!domain.is_empty()).then_some(domain)
        }
        url::Host::Ipv4(address) => Some(address.to_string()),
        url::Host::Ipv6(address) => Some(address.to_string()),
    }
}

fn request_host_matches_public_url(req: &Request) -> bool {
    let Some(host) = req
        .headers()
        .get("host")
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Ok(authority) = host.parse::<salvo::http::uri::Authority>() else {
        return false;
    };
    let Some(request_host) = normalize_domain_or_ip(authority.host()) else {
        return false;
    };

    let Ok(public_url) = std::env::var("WEBPI_PUBLIC_URL") else {
        return false;
    };
    let Ok(public_url) = url::Url::parse(public_url.trim()) else {
        return false;
    };
    let Some(public_host) = public_url.host_str().and_then(normalize_domain_or_ip) else {
        return false;
    };
    if request_host != public_host {
        return false;
    }

    match public_url.port() {
        Some(expected) => authority.port_u16() == Some(expected),
        None => authority.port_u16().is_none_or(|port| {
            public_url
                .port_or_known_default()
                .is_some_and(|expected| expected == port)
        }),
    }
}

fn valid_cloudflare_connecting_ip(req: &Request) -> Option<String> {
    req.headers()
        .get(CF_CONNECTING_IP)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<IpAddr>().ok())
        .map(|address| address.to_string())
}

pub(crate) fn is_public_request(req: &Request) -> bool {
    public_actions_only_enabled()
        && (valid_cloudflare_connecting_ip(req).is_some() || request_host_matches_public_url(req))
}

fn public_surface_allowed(method: &Method, path: &str) -> bool {
    if path == "/openapi.json" {
        return matches!(*method, Method::GET | Method::HEAD);
    }
    if path == "/artifact-download" {
        return *method == Method::GET;
    }
    if *method != Method::POST {
        return false;
    }
    let Some(tool_name) = path.strip_prefix("/api/actions/") else {
        return false;
    };
    !tool_name.is_empty() && !tool_name.contains('/')
}

fn public_client_key(req: &Request) -> String {
    valid_cloudflare_connecting_ip(req).unwrap_or_else(|| "public-unknown".to_string())
}

fn unix_time_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Called only from an authentication failure path.
///
/// Returns true when the response was converted to a bounded HTTP 429. Valid
/// credentials never call this function and are therefore never throttled.
pub(crate) fn render_invalid_auth_rate_limit_if_needed(
    req: &Request,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) -> bool {
    if !is_public_request(req) || !public_invalid_auth_rate_limit_enabled() {
        return false;
    }

    let policy = invalid_auth_rate_limit_policy();
    let client = public_client_key(req);
    let limiter = INVALID_AUTH_LIMITER.get_or_init(|| Mutex::new(InvalidAuthLimiter::default()));
    let retry_after = limiter
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .record_failure_at(&client, unix_time_secs(), policy);

    let Some(retry_after) = retry_after else {
        return false;
    };

    res.status_code(StatusCode::TOO_MANY_REQUESTS);
    if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
        res.headers_mut().insert("retry-after", value);
    }
    res.headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    res.render(Json(serde_json::json!({"error": "Too Many Requests"})));
    ctrl.skip_rest();
    true
}

pub(crate) struct PublicHttpSecurity;

#[async_trait]
impl Handler for PublicHttpSecurity {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        if !is_public_request(req) || public_surface_allowed(req.method(), req.uri().path()) {
            ctrl.call_next(req, depot, res).await;
            return;
        }

        // Deliberately hide the existence and authentication behavior of
        // loopback-only management/MCP surfaces from the public hostname.
        res.status_code(StatusCode::NOT_FOUND);
        res.headers_mut()
            .insert("cache-control", HeaderValue::from_static("no-store"));
        res.render(Json(serde_json::json!({"error": "Not Found"})));
        ctrl.skip_rest();
    }
}
