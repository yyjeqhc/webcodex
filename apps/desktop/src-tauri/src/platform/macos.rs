use super::{normalize_proxy_server, SystemProxyCandidate};
use system_configuration::core_foundation::{
    base::CFType, dictionary::CFDictionary, number::CFNumber, string::CFString,
};
use system_configuration::dynamic_store::SCDynamicStoreBuilder;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ProxyEndpoint {
    enabled: bool,
    host: Option<String>,
    port: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SystemProxySettings {
    http: ProxyEndpoint,
    https: ProxyEndpoint,
    pac_enabled: bool,
    socks_enabled: bool,
}

pub fn system_http_proxy_candidate() -> Option<SystemProxyCandidate> {
    let store = SCDynamicStoreBuilder::new("dev.webcodex.desktop.proxy-discovery").build()?;
    let proxies = store.get_proxies()?;
    project_http_proxy(&system_proxy_settings(&proxies))
}

fn system_proxy_settings(proxies: &CFDictionary<CFString, CFType>) -> SystemProxySettings {
    SystemProxySettings {
        http: ProxyEndpoint {
            enabled: dictionary_flag(proxies, "HTTPEnable"),
            host: dictionary_string(proxies, "HTTPProxy"),
            port: dictionary_port(proxies, "HTTPPort"),
        },
        https: ProxyEndpoint {
            enabled: dictionary_flag(proxies, "HTTPSEnable"),
            host: dictionary_string(proxies, "HTTPSProxy"),
            port: dictionary_port(proxies, "HTTPSPort"),
        },
        pac_enabled: dictionary_flag(proxies, "ProxyAutoConfigEnable"),
        socks_enabled: dictionary_flag(proxies, "SOCKSEnable"),
    }
}

fn dictionary_string(proxies: &CFDictionary<CFString, CFType>, key: &str) -> Option<String> {
    proxies
        .find(CFString::new(key))
        .and_then(|value| value.downcast::<CFString>())
        .map(|value| value.to_string())
}

fn dictionary_number(proxies: &CFDictionary<CFString, CFType>, key: &str) -> Option<i64> {
    proxies
        .find(CFString::new(key))
        .and_then(|value| value.downcast::<CFNumber>())
        .and_then(|value| value.to_i64())
}

fn dictionary_flag(proxies: &CFDictionary<CFString, CFType>, key: &str) -> bool {
    dictionary_number(proxies, key).is_some_and(|value| value != 0)
}

fn dictionary_port(proxies: &CFDictionary<CFString, CFType>, key: &str) -> Option<u16> {
    let value = dictionary_number(proxies, key)?;
    u16::try_from(value).ok().filter(|port| *port != 0)
}

fn project_http_proxy(settings: &SystemProxySettings) -> Option<SystemProxyCandidate> {
    // macOS HTTPS ("Secure Web Proxy") is still an HTTP CONNECT proxy endpoint.
    // Prefer it because Tunnel control-plane traffic is HTTPS, then fall back to
    // the explicit HTTP proxy. PAC and SOCKS are diagnostic facts only in v1.
    [&settings.https, &settings.http]
        .into_iter()
        .filter(|endpoint| endpoint.enabled)
        .find_map(|endpoint| endpoint_url(endpoint).map(|url| SystemProxyCandidate { url }))
}

fn endpoint_url(endpoint: &ProxyEndpoint) -> Option<String> {
    let host = endpoint.host.as_deref()?.trim();
    let port = endpoint.port?;
    if host.is_empty()
        || host.len() > 255
        || host
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        || host.contains('@')
        || host.contains('/')
        || host.contains('?')
        || host.contains('#')
        || host.contains("://")
    {
        return None;
    }

    let host = match url::Host::parse(host).ok()? {
        url::Host::Ipv6(address) => format!("[{address}]"),
        parsed => parsed.to_string(),
    };
    normalize_proxy_server(&format!("{host}:{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(enabled: bool, host: &str, port: u16) -> ProxyEndpoint {
        ProxyEndpoint {
            enabled,
            host: Some(host.to_string()),
            port: Some(port),
        }
    }

    #[test]
    fn http_enabled_only_is_projected() {
        let settings = SystemProxySettings {
            http: endpoint(true, "127.0.0.1", 7890),
            ..Default::default()
        };
        assert_eq!(
            project_http_proxy(&settings).map(|candidate| candidate.url),
            Some("http://127.0.0.1:7890".to_string())
        );
    }

    #[test]
    fn https_enabled_only_is_projected() {
        let settings = SystemProxySettings {
            https: endpoint(true, "proxy.example.test", 8443),
            ..Default::default()
        };
        assert_eq!(
            project_http_proxy(&settings).map(|candidate| candidate.url),
            Some("http://proxy.example.test:8443".to_string())
        );
    }

    #[test]
    fn https_precedes_http_when_both_are_enabled() {
        let settings = SystemProxySettings {
            http: endpoint(true, "http.example.test", 8080),
            https: endpoint(true, "https.example.test", 8443),
            ..Default::default()
        };
        assert_eq!(
            project_http_proxy(&settings).map(|candidate| candidate.url),
            Some("http://https.example.test:8443".to_string())
        );
    }

    #[test]
    fn disabled_stale_endpoints_do_not_activate() {
        let settings = SystemProxySettings {
            http: endpoint(false, "127.0.0.1", 7890),
            https: endpoint(false, "127.0.0.1", 7890),
            ..Default::default()
        };
        assert_eq!(project_http_proxy(&settings), None);
    }

    #[test]
    fn pac_or_socks_alone_are_not_translated_to_http() {
        let settings = SystemProxySettings {
            pac_enabled: true,
            socks_enabled: true,
            ..Default::default()
        };
        assert_eq!(project_http_proxy(&settings), None);
    }

    #[test]
    fn malformed_hosts_ports_and_credentials_fail_closed() {
        for host in [
            "",
            "user:secret@127.0.0.1",
            "http://127.0.0.1",
            "proxy.example.test/path",
            "proxy.example.test?x=y",
            "proxy example.test",
        ] {
            let settings = SystemProxySettings {
                https: endpoint(true, host, 7890),
                ..Default::default()
            };
            assert_eq!(project_http_proxy(&settings), None, "host={host:?}");
        }

        let settings = SystemProxySettings {
            https: ProxyEndpoint {
                enabled: true,
                host: Some("127.0.0.1".to_string()),
                port: None,
            },
            ..Default::default()
        };
        assert_eq!(project_http_proxy(&settings), None);
    }

    #[test]
    fn empty_configuration_is_direct_and_loopback_is_supported() {
        assert_eq!(project_http_proxy(&SystemProxySettings::default()), None);
        let settings = SystemProxySettings {
            https: endpoint(true, "127.0.0.1", 7890),
            ..Default::default()
        };
        assert_eq!(
            project_http_proxy(&settings).map(|candidate| candidate.url),
            Some("http://127.0.0.1:7890".to_string())
        );
    }

    #[test]
    fn malformed_preferred_https_can_fall_back_to_valid_http() {
        let settings = SystemProxySettings {
            https: endpoint(true, "user:secret@127.0.0.1", 8443),
            http: endpoint(true, "127.0.0.1", 7890),
            ..Default::default()
        };
        assert_eq!(
            project_http_proxy(&settings).map(|candidate| candidate.url),
            Some("http://127.0.0.1:7890".to_string())
        );
    }
}
