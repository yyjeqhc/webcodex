use crate::Error;
use reqwest::{ClientBuilder, Proxy, Url};
use std::fmt;

/// Control-plane transport policy only. The fixed MCP hop always bypasses proxies.
/// Embedded hosts should select Direct or Explicit rather than inheriting process state.
#[derive(Clone, Default)]
pub enum ControlPlaneProxy {
    /// Preserve the standalone client's standard HTTP(S)_PROXY / NO_PROXY behavior.
    #[default]
    System,
    Direct,
    /// An HTTP(S) proxy URL, optionally with credentials. Validated at client construction.
    Explicit(String),
}

impl fmt::Debug for ControlPlaneProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::System => "ControlPlaneProxy::System",
            Self::Direct => "ControlPlaneProxy::Direct",
            Self::Explicit(_) => "ControlPlaneProxy::Explicit([REDACTED])",
        })
    }
}

impl ControlPlaneProxy {
    pub(crate) fn apply(self, builder: ClientBuilder) -> Result<ClientBuilder, Error> {
        match self {
            Self::System => Ok(builder),
            Self::Direct => Ok(builder.no_proxy()),
            Self::Explicit(value) => {
                if value.len() > 8192 {
                    return Err(Error::Configuration);
                }
                let url = Url::parse(&value).map_err(|_| Error::Configuration)?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.host_str().is_none()
                    || !matches!(url.path(), "" | "/")
                    || url.query().is_some()
                    || url.fragment().is_some()
                {
                    return Err(Error::Configuration);
                }
                let proxy = Proxy::all(url).map_err(|_| Error::Configuration)?;
                // Explicit policy never inherits NO_PROXY or another profile's environment.
                Ok(builder.no_proxy().proxy(proxy))
            }
        }
    }
}
