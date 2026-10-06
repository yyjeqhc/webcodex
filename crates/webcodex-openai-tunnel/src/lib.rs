#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
mod command;
mod control_plane;
pub mod deadline;
mod error;
mod health;
mod mcp_http;
pub mod policy;
mod poller;
mod proxy;
mod response;
pub mod wire;

pub use error::Error;
pub use health::{Health, HealthSnapshot};
use policy::{DeadlinePolicy, Limits};
pub use proxy::ControlPlaneProxy;
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    Client, Url,
};
use std::{fmt, future::Future, time::Duration};

/// Secret header material is never included in Debug or error output.
pub struct Credential(HeaderValue);
impl Credential {
    pub fn bearer(token: &str) -> Result<Self, Error> {
        if token.trim().is_empty() {
            return Err(Error::Configuration);
        }
        Self::authorization(&format!("Bearer {token}"))
    }
    pub fn authorization(value: &str) -> Result<Self, Error> {
        if value.is_empty() || value.len() > 8192 {
            return Err(Error::Configuration);
        }
        let mut h = HeaderValue::from_str(value).map_err(|_| Error::Configuration)?;
        h.set_sensitive(true);
        Ok(Self(h))
    }
}
impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credential([REDACTED])")
    }
}

pub struct ControlPlaneIdentity {
    origin: Url,
    tunnel: String,
    credential: Credential,
}
impl ControlPlaneIdentity {
    pub fn new(origin: &str, tunnel: &str, credential: Credential) -> Result<Self, Error> {
        let origin = checked_url(origin)?;
        if origin.path() != "/"
            || origin.query().is_some()
            || tunnel.is_empty()
            || tunnel.len() > 256
            || !tunnel
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(Error::Configuration);
        }
        Ok(Self {
            origin,
            tunnel: tunnel.into(),
            credential,
        })
    }
}
pub struct FixedMcpTarget {
    url: Url,
    credential: Option<Credential>,
}
impl FixedMcpTarget {
    pub fn new(url: &str, credential: Credential) -> Result<Self, Error> {
        Ok(Self {
            url: checked_url(url)?,
            credential: Some(credential),
        })
    }
    /// Explicitly bind a target requiring no local Authorization header. This
    /// never forwards the control-plane credential or admits command overrides.
    pub fn unauthenticated(url: &str) -> Result<Self, Error> {
        Ok(Self {
            url: checked_url(url)?,
            credential: None,
        })
    }
}
fn checked_url(value: &str) -> Result<Url, Error> {
    let url = Url::parse(value).map_err(|_| Error::Configuration)?;
    let loopback = url
        .host_str()
        .is_some_and(|h| matches!(h, "127.0.0.1" | "[::1]"));
    if !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err(Error::Configuration);
    }
    Ok(url)
}

/// One immutable binding and one run. Reconstructing a client requires the host to
/// resolve restart ambiguity; this library does not provide a durable receipt ledger.
pub struct TunnelClient {
    cp: ControlPlaneIdentity,
    target: FixedMcpTarget,
    control: Client,
    mcp: Client,
    policy: DeadlinePolicy,
    limits: Limits,
    health: Health,
}
impl TunnelClient {
    pub fn new(
        cp: ControlPlaneIdentity,
        target: FixedMcpTarget,
        policy: DeadlinePolicy,
        limits: Limits,
    ) -> Result<Self, Error> {
        Self::new_with_proxy(cp, target, policy, limits, ControlPlaneProxy::System)
    }

    /// Construct an immutable profile without changing process-global proxy configuration.
    pub fn new_with_proxy(
        cp: ControlPlaneIdentity,
        target: FixedMcpTarget,
        policy: DeadlinePolicy,
        limits: Limits,
        proxy: ControlPlaneProxy,
    ) -> Result<Self, Error> {
        limits.validate()?;
        if matches!(policy, DeadlinePolicy::RequireFinite { max_duration } if max_duration.is_zero() || max_duration > Duration::from_secs(86400))
        {
            return Err(Error::Configuration);
        }
        let builder = || {
            Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .connect_timeout(Duration::from_secs(10))
        };
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(AUTHORIZATION, cp.credential.0.clone());
        headers.insert(
            "X-Tunnel-Client-Name",
            HeaderValue::from_static("rust-secure-mcp-tunnel"),
        );
        headers.insert(
            "X-Tunnel-Client-Version",
            HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
        );
        headers.insert(
            "X-Tunnel-Client-Wire-Protocol-Version",
            HeaderValue::from_static("2026-08-25"),
        );
        headers.insert(
            "X-Tunnel-MCP-Server-Info",
            HeaderValue::from_static("{\"version\":1,\"channels\":[{\"name\":\"main\"}]}"),
        );
        headers.insert(
            "X-Tunnel-Client-Instance-Id",
            HeaderValue::from_str(&uuid::Uuid::new_v4().to_string())
                .map_err(|_| Error::Configuration)?,
        );
        let control = proxy
            .apply(builder())?
            .default_headers(headers)
            .build()
            .map_err(|_| Error::Configuration)?;
        // Local credentials never traverse an ambient proxy. Remote targets are explicit too.
        let mcp = builder()
            .no_proxy()
            .build()
            .map_err(|_| Error::Configuration)?;
        Ok(Self {
            cp,
            target,
            control,
            mcp,
            policy,
            limits,
            health: Health::default(),
        })
    }
    pub fn health(&self) -> Health {
        self.health.clone()
    }
    /// Stop ingress, then allow admitted work ten seconds to settle, including response
    /// delivery. Dropping this future is abrupt owner loss, not a clean shutdown.
    pub async fn run(self, stop: impl Future<Output = ()>) -> Result<(), Error> {
        self.run_with_drain(stop, Duration::from_secs(10)).await
    }

    /// The drain bound is independent of wire command deadlines. Original command
    /// deadlines still apply; a deadline, lost delivery, or drain timeout never permits
    /// local replay. The host must retain its restart fence on an uncertain result.
    pub async fn run_with_drain(
        self,
        stop: impl Future<Output = ()>,
        drain_timeout: Duration,
    ) -> Result<(), Error> {
        if drain_timeout.is_zero() || drain_timeout > Duration::from_secs(120) {
            return Err(Error::Configuration);
        }
        self.poller(stop, drain_timeout).await
    }
}
