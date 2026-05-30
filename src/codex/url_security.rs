use std::net::{IpAddr, ToSocketAddrs};

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || matches!(v6.segments()[0], 0xfc00..=0xfdff | 0xfe80..=0xfebf)
        }
    }
}

pub(super) fn is_allowed_chatgpt_estuary_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("chatgpt.com")
        && url.path() == "/backend-api/estuary/content"
        && url
            .query_pairs()
            .any(|(k, v)| k == "id" && v.starts_with("file_"))
        && url.query_pairs().any(|(k, v)| k == "sig" && !v.is_empty())
}

pub(super) fn validate_source_url(source_url: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(source_url).map_err(|e| format!("Invalid source_url: {}", e))?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("source_url must use http or https".to_string()),
    }
    if url.username() != "" || url.password().is_some() {
        return Err("source_url must not contain credentials".to_string());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "source_url must include a host".to_string())?;
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "localhost" || host_lower.ends_with(".localhost") {
        return Err("source_url host is not allowed".to_string());
    }
    if is_allowed_chatgpt_estuary_url(&url) {
        return Ok(url);
    }
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("Failed to resolve source_url host: {}", e))?;
    let mut saw_addr = false;
    for addr in addrs {
        saw_addr = true;
        if is_blocked_ip(addr.ip()) {
            return Err("source_url resolves to a blocked private/local address".to_string());
        }
    }
    if !saw_addr {
        return Err("source_url host resolved to no addresses".to_string());
    }
    Ok(url)
}
