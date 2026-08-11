#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerHttpOptions {
    pub proxy: Option<String>,
    pub no_system_proxy: bool,
}

fn proxy_validation_error() -> String {
    "--proxy must be an http://host:port URL without credentials, path, query, or fragment"
        .to_string()
}

fn authority_has_explicit_port(authority: &str) -> bool {
    if authority.starts_with('[') {
        let Some(close) = authority.find(']') else {
            return false;
        };
        return authority[close + 1..]
            .strip_prefix(':')
            .is_some_and(|port| !port.is_empty());
    }
    authority
        .rsplit_once(':')
        .is_some_and(|(host, port)| !host.is_empty() && !host.contains(':') && !port.is_empty())
}

fn validate_http_proxy(proxy: &str) -> Result<(), String> {
    if proxy.is_empty() {
        return Err("--proxy cannot be empty".to_string());
    }
    if proxy.trim() != proxy {
        return Err(proxy_validation_error());
    }
    let authority = proxy
        .strip_prefix("http://")
        .ok_or_else(proxy_validation_error)?;
    if authority.is_empty()
        || authority.contains('@')
        || authority.contains('/')
        || authority.contains('?')
        || authority.contains('#')
        || !authority_has_explicit_port(authority)
    {
        return Err(proxy_validation_error());
    }
    let parsed = reqwest::Url::parse(proxy).map_err(|_| proxy_validation_error())?;
    if parsed.scheme() != "http"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(proxy_validation_error());
    }
    Ok(())
}

impl ServerHttpOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.proxy.is_some() && self.no_system_proxy {
            return Err("--proxy and --no-system-proxy are mutually exclusive".to_string());
        }
        if let Some(proxy) = self.proxy.as_deref() {
            validate_http_proxy(proxy)?;
        }
        Ok(())
    }
}

pub fn build_server_http_client(options: &ServerHttpOptions) -> Result<reqwest::Client, String> {
    options.validate()?;
    let mut builder = reqwest::Client::builder();
    if options.no_system_proxy {
        builder = builder.no_proxy();
    } else if let Some(proxy) = options.proxy.as_deref() {
        let explicit = reqwest::Proxy::all(proxy).map_err(|_| proxy_validation_error())?;
        builder = builder.no_proxy().proxy(explicit);
    }
    builder
        .build()
        .map_err(|_| "failed to build HTTP client".to_string())
}
