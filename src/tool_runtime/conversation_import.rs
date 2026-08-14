//! Bounded Control-side import of ChatGPT conversation attachments.
//!
//! ChatGPT supplies temporary OpenAI-hosted file references. WebCodex validates
//! and consumes those references immediately on the Control side, then routes
//! the downloaded bytes through the existing SaveProjectArtifact mutation path.

use super::sessions::SessionTransport;
use super::tool_call::OpenAiHostFileRef;
use super::{ToolCall, ToolResult, ToolRuntime};
use crate::artifact_policy::ooxml_extension_for_mime;
use crate::auth::AuthContext;
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

pub(crate) const MAX_IMPORT_FILES: usize = 10;
pub(crate) const MAX_IMPORT_FILE_BYTES: usize = 10 * 1024 * 1024;
const IMPORT_OCTET_STREAM_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".webp", ".pdf", ".zip", ".docx", ".pptx", ".xlsx", ".txt", ".csv",
    ".json",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConversationImportDownloadPolicy {
    GptActionOpenAiHost,
    TrustedMcpHostFile,
}

const IMPORT_DNS_RESOLUTION_TIMEOUT: Duration = Duration::from_secs(5);
const IMPORT_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
struct TrustedDownloadTarget {
    url: reqwest::Url,
    resolver_host: Option<String>,
    pinned_addrs: Vec<SocketAddr>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OpenAiFileIdRef {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) id: Option<String>,
    #[serde(default)]
    pub(crate) mime_type: Option<String>,
    pub(crate) download_link: String,
}

pub(crate) struct ImportConversationFilesInput {
    pub(crate) openai_file_id_refs: Vec<OpenAiFileIdRef>,
    pub(crate) project: String,
    pub(crate) output_dir: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) overwrite: Option<bool>,
    pub(crate) session_id: Option<String>,
}

impl From<OpenAiHostFileRef> for OpenAiFileIdRef {
    fn from(value: OpenAiHostFileRef) -> Self {
        Self {
            name: value.file_name,
            // The MCP host file_id is part of the host transport shape only.
            // WebCodex never dereferences or returns it.
            id: None,
            mime_type: value.mime_type,
            download_link: value.download_url,
        }
    }
}

fn sanitize_import_name(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    for ch in name.rsplit('/').next().unwrap_or(name).chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('.').trim_matches('_');
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn default_import_leaf(file_ref: &OpenAiFileIdRef, index: usize, mime: &str) -> String {
    let fallback = format!("artifact-{}", index + 1);
    match file_ref.name.as_deref().or(file_ref.id.as_deref()) {
        Some(source_name) => sanitize_import_name(source_name, &fallback),
        None => match ooxml_extension_for_mime(mime) {
            Some(extension) => format!("{fallback}{extension}"),
            None => fallback,
        },
    }
}

fn join_import_path(output_dir: Option<&str>, leaf: &str) -> Result<String, String> {
    let dir = output_dir
        .unwrap_or("artifacts/imports")
        .trim()
        .trim_matches('/');
    let candidate = if dir.is_empty() {
        leaf.to_string()
    } else {
        format!("{dir}/{leaf}")
    };
    crate::tool_runtime::files::validate_artifact_file_path(&candidate)?;
    Ok(candidate)
}

fn mime_allowed_for_import(mime: &str, path: &str) -> bool {
    let lower_path = path.to_ascii_lowercase();
    if let Some(required_extension) = ooxml_extension_for_mime(mime) {
        return lower_path.ends_with(required_extension);
    }
    matches!(
        mime,
        "image/png"
            | "image/jpeg"
            | "image/webp"
            | "application/pdf"
            | "application/zip"
            | "text/plain"
            | "text/csv"
            | "application/json"
    ) || (mime == "application/octet-stream"
        && IMPORT_OCTET_STREAM_EXTENSIONS
            .iter()
            .any(|suffix| lower_path.ends_with(suffix)))
}

fn validate_openai_download_url(download_link: &str) -> Result<reqwest::Url, String> {
    let url =
        reqwest::Url::parse(download_link).map_err(|e| format!("invalid download_link: {e}"))?;
    if url.scheme() != "https" {
        return Err("download_link must use https".to_string());
    }
    let Some(host) = url.host_str().map(|h| h.to_ascii_lowercase()) else {
        return Err("download_link must include a host".to_string());
    };
    if host != "files.oaiusercontent.com" && !host.ends_with(".oaiusercontent.com") {
        return Err("download_link host is not an OpenAI file host".to_string());
    }
    Ok(url)
}

fn ipv4_is_public(ip: Ipv4Addr) -> bool {
    let [a, b, c, _d] = ip.octets();
    if a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224
    {
        return false;
    }
    true
}

fn ipv6_embedded_ipv4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let segments = ip.segments();
    if segments[..5] == [0, 0, 0, 0, 0] && (segments[5] == 0xffff || segments[5] == 0) {
        return Some(Ipv4Addr::new(
            (segments[6] >> 8) as u8,
            segments[6] as u8,
            (segments[7] >> 8) as u8,
            segments[7] as u8,
        ));
    }
    None
}

fn ipv6_is_public(ip: Ipv6Addr) -> bool {
    if ip.is_unspecified() || ip.is_loopback() {
        return false;
    }
    if let Some(ipv4) = ipv6_embedded_ipv4(ip) {
        return ipv4_is_public(ipv4);
    }
    let segments = ip.segments();
    let first = segments[0];
    if first & 0xff00 == 0xff00 // multicast ff00::/8
        || first & 0xfe00 == 0xfc00 // unique-local fc00::/7
        || first & 0xffc0 == 0xfe80 // link-local fe80::/10
        || first & 0xffc0 == 0xfec0 // deprecated site-local fec0::/10
        || (first == 0x0100 && segments[1..4] == [0, 0, 0]) // discard-only 100::/64
        || (first == 0x0064 && segments[1] == 0xff9b) // NAT64 well-known/local-use prefixes
        || first == 0x2002 // 6to4 embeds an IPv4 destination
        || (first == 0x2001 && segments[1] <= 0x01ff) // IETF protocol assignments
        || (first == 0x2001 && segments[1] == 0x0db8) // documentation
        || (first == 0x3fff && segments[1] & 0xf000 == 0)
    // documentation 3fff::/20
    {
        return false;
    }
    true
}

fn ip_is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ipv4_is_public(ip),
        IpAddr::V6(ip) => ipv6_is_public(ip),
    }
}

#[cfg(test)]
static IMPORT_TEST_NETWORK_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> =
    std::sync::OnceLock::new();

#[cfg(test)]
pub(crate) async fn lock_import_test_network() -> tokio::sync::MutexGuard<'static, ()> {
    IMPORT_TEST_NETWORK_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

#[cfg(test)]
static IMPORT_TEST_RESOLVED_IPS: std::sync::OnceLock<std::sync::Mutex<Option<Vec<IpAddr>>>> =
    std::sync::OnceLock::new();
#[cfg(test)]
static IMPORT_TEST_DNS_RESOLUTION_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn set_import_test_resolved_ips(ips: Option<Vec<IpAddr>>) {
    let slot = IMPORT_TEST_RESOLVED_IPS.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("import test resolved IP mutex poisoned") = ips;
}

#[cfg(test)]
pub(crate) fn reset_import_test_dns_resolution_count() {
    IMPORT_TEST_DNS_RESOLUTION_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
pub(crate) fn import_test_dns_resolution_count() -> usize {
    IMPORT_TEST_DNS_RESOLUTION_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

async fn resolve_download_domain(host: &str) -> Result<Vec<IpAddr>, String> {
    #[cfg(test)]
    IMPORT_TEST_DNS_RESOLUTION_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    #[cfg(test)]
    {
        if let Some(ips) = IMPORT_TEST_RESOLVED_IPS
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("import test resolved IP mutex poisoned")
            .clone()
        {
            return Ok(ips);
        }
    }
    let resolved = tokio::time::timeout(
        IMPORT_DNS_RESOLUTION_TIMEOUT,
        tokio::net::lookup_host((host, 443)),
    )
    .await
    .map_err(|_| "host-provided download URL DNS resolution timed out".to_string())?
    .map_err(|_| "host-provided download URL DNS resolution failed".to_string())?;
    Ok(resolved.map(|addr| addr.ip()).collect())
}

async fn validate_trusted_mcp_download_url(
    download_link: &str,
) -> Result<TrustedDownloadTarget, String> {
    let url = reqwest::Url::parse(download_link)
        .map_err(|_| "invalid host-provided download URL".to_string())?;
    if url.scheme() != "https" {
        return Err("host-provided download URL must use https".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("host-provided download URL must not contain userinfo".to_string());
    }
    if url.port().is_some_and(|port| port != 443) {
        return Err("host-provided download URL must use port 443".to_string());
    }
    let Some(host) = url.host() else {
        return Err("host-provided download URL must include a host".to_string());
    };

    let (resolver_host, ips) = match host {
        url::Host::Ipv4(ip) => (None, vec![IpAddr::V4(ip)]),
        url::Host::Ipv6(ip) => (None, vec![IpAddr::V6(ip)]),
        url::Host::Domain(domain) => {
            let domain = domain.trim();
            if domain.is_empty() {
                return Err("host-provided download URL has an empty host".to_string());
            }
            (
                Some(domain.to_string()),
                resolve_download_domain(domain).await?,
            )
        }
    };
    if ips.is_empty() {
        return Err("host-provided download URL resolved to no addresses".to_string());
    }
    if ips.iter().copied().any(|ip| !ip_is_public(ip)) {
        return Err("host-provided download URL resolves to a non-public address".to_string());
    }
    let mut pinned_addrs = Vec::with_capacity(ips.len());
    for ip in ips {
        let addr = SocketAddr::new(ip, 443);
        if !pinned_addrs.contains(&addr) {
            pinned_addrs.push(addr);
        }
    }
    Ok(TrustedDownloadTarget {
        url,
        resolver_host,
        pinned_addrs,
    })
}

fn build_download_client(
    trusted_target: Option<&TrustedDownloadTarget>,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .timeout(IMPORT_DOWNLOAD_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none());
    if let Some(target) = trusted_target {
        if let Some(host) = target.resolver_host.as_deref() {
            builder = builder.resolve_to_addrs(host, &target.pinned_addrs);
        }
    }
    builder
        .build()
        .map_err(|_| "failed to build bounded import HTTP client".to_string())
}

async fn prepare_download_request(
    download_link: &str,
    policy: ConversationImportDownloadPolicy,
) -> Result<(reqwest::Client, reqwest::Url), String> {
    match policy {
        ConversationImportDownloadPolicy::GptActionOpenAiHost => {
            let url = validate_openai_download_url(download_link)?;
            let client = build_download_client(None)?;
            Ok((client, request_url_for_download(url)))
        }
        ConversationImportDownloadPolicy::TrustedMcpHostFile => {
            let target = validate_trusted_mcp_download_url(download_link).await?;
            let client = build_download_client(Some(&target))?;
            Ok((client, request_url_for_download(target.url)))
        }
    }
}

#[cfg(test)]
static IMPORT_TEST_DOWNLOAD_BASE_URL: std::sync::OnceLock<std::sync::Mutex<Option<String>>> =
    std::sync::OnceLock::new();

#[cfg(test)]
pub(crate) fn set_import_test_download_base_url(base_url: Option<String>) {
    let slot = IMPORT_TEST_DOWNLOAD_BASE_URL.get_or_init(|| std::sync::Mutex::new(None));
    *slot
        .lock()
        .expect("import test download base mutex poisoned") = base_url;
}

fn request_url_for_download(validated_url: reqwest::Url) -> reqwest::Url {
    #[cfg(test)]
    {
        let base_url = IMPORT_TEST_DOWNLOAD_BASE_URL
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("import test download base mutex poisoned")
            .clone();
        if let Some(base_url) = base_url {
            let mut rewritten = reqwest::Url::parse(&base_url)
                .expect("test import download base URL must be valid");
            rewritten.set_path(validated_url.path());
            rewritten.set_query(validated_url.query());
            return rewritten;
        }
    }
    validated_url
}

async fn read_bounded_download(
    response: &mut reqwest::Response,
    source_name: &str,
) -> Result<Vec<u8>, String> {
    if let Some(len) = response.content_length() {
        if len > MAX_IMPORT_FILE_BYTES as u64 {
            return Err(format!(
                "download for '{source_name}' exceeds {MAX_IMPORT_FILE_BYTES} bytes"
            ));
        }
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| format!("failed to read download for '{source_name}'"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_IMPORT_FILE_BYTES {
            return Err(format!(
                "download for '{source_name}' exceeds {MAX_IMPORT_FILE_BYTES} bytes"
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

impl ToolRuntime {
    pub(crate) async fn import_conversation_files(
        &self,
        input: ImportConversationFilesInput,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
        download_policy: ConversationImportDownloadPolicy,
    ) -> ToolResult {
        if input.openai_file_id_refs.is_empty()
            || input.openai_file_id_refs.len() > MAX_IMPORT_FILES
        {
            return ToolResult::err(format!(
                "openaiFileIdRefs must contain 1..={MAX_IMPORT_FILES} files"
            ));
        }
        let mut imported = Vec::new();
        for (idx, file_ref) in input.openai_file_id_refs.iter().enumerate() {
            let source_name = file_ref
                .name
                .as_deref()
                .or(file_ref.id.as_deref())
                .unwrap_or("artifact");
            let mime = file_ref
                .mime_type
                .as_deref()
                .unwrap_or("application/octet-stream");
            let fallback = format!("artifact-{}", idx + 1);
            let leaf = input
                .targets
                .as_ref()
                .and_then(|targets| targets.get(idx))
                .map(|target| sanitize_import_name(target, &fallback))
                .unwrap_or_else(|| default_import_leaf(file_ref, idx, mime));
            let path = match join_import_path(input.output_dir.as_deref(), &leaf) {
                Ok(path) => path,
                Err(e) => return ToolResult::err(e),
            };
            if !mime_allowed_for_import(mime, &path) {
                return ToolResult::err(format!(
                    "unsupported MIME type for '{source_name}': {mime}"
                ));
            }
            let (client, request_url) =
                match prepare_download_request(&file_ref.download_link, download_policy).await {
                    Ok(prepared) => prepared,
                    Err(e) => return ToolResult::err(e),
                };
            let mut response = match client.get(request_url).send().await {
                Ok(response) => response,
                Err(_) => {
                    // reqwest error text can include the request URL. Keep the
                    // temporary host URL out of durable/model-visible errors.
                    return ToolResult::err(format!("failed to download '{source_name}'"));
                }
            };
            if !response.status().is_success() {
                return ToolResult::err(format!(
                    "download for '{source_name}' returned HTTP {}",
                    response.status()
                ));
            }
            let bytes = match read_bounded_download(&mut response, source_name).await {
                Ok(bytes) => bytes,
                Err(e) => return ToolResult::err(e),
            };
            let result = Box::pin(self.dispatch_with_auth_transport_options(
                ToolCall::SaveProjectArtifact {
                    project: input.project.clone(),
                    path: path.clone(),
                    content_base64: general_purpose::STANDARD.encode(&bytes),
                    session_id: input.session_id.clone(),
                    mime_type: Some(mime.to_string()),
                    overwrite: input.overwrite,
                },
                auth,
                transport.clone(),
                false,
                false,
            ))
            .await;
            if !result.success {
                return result;
            }
            let mut obj = Map::new();
            obj.insert(
                "source_name".to_string(),
                Value::String(source_name.to_string()),
            );
            obj.insert("project".to_string(), Value::String(input.project.clone()));
            obj.insert("path".to_string(), Value::String(path));
            obj.insert(
                "bytes_written".to_string(),
                result.output["bytes_written"].clone(),
            );
            obj.insert("mime_type".to_string(), Value::String(mime.to_string()));
            obj.insert("sha256".to_string(), result.output["sha256"].clone());
            imported.push(Value::Object(obj));
        }
        ToolResult::ok(json!({"imported": imported, "count": imported.len()}))
    }

    pub(crate) async fn dispatch_conversation_import_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> ToolResult {
        let ToolCall::ImportConversationFilesToProject {
            project,
            openai_file_id_refs,
            output_dir,
            targets,
            overwrite,
            session_id,
            trusted_mcp_host_file_import,
        } = call
        else {
            unreachable!("dispatch_conversation_import_tool called with non-import tool")
        };
        if !matches!(transport, SessionTransport::Mcp) {
            return ToolResult::err(
                "import_conversation_files_to_project requires the MCP host file-reference mechanism; use the dedicated /api/artifacts/import GPT Action outside MCP",
            );
        }
        if !trusted_mcp_host_file_import {
            return ToolResult::err(
                "import_conversation_files_to_project requires an explicitly trusted OAuth MCP client",
            );
        }
        self.import_conversation_files(
            ImportConversationFilesInput {
                openai_file_id_refs: openai_file_id_refs.into_iter().map(Into::into).collect(),
                project,
                output_dir,
                targets,
                overwrite,
                session_id,
            },
            auth,
            transport,
            ConversationImportDownloadPolicy::TrustedMcpHostFile,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_import_leaf_preserves_ooxml_when_host_omits_filename() {
        let file_ref = OpenAiFileIdRef {
            name: None,
            id: None,
            mime_type: Some(crate::artifact_policy::PPTX_MIME.to_string()),
            download_link: "https://files.oaiusercontent.com/file".to_string(),
        };
        assert_eq!(
            default_import_leaf(&file_ref, 0, crate::artifact_policy::PPTX_MIME),
            "artifact-1.pptx"
        );
    }

    #[test]
    fn trusted_mcp_ssrf_policy_rejects_non_public_ip_ranges() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.0.1",
        ] {
            assert!(!ip_is_public(ip.parse().unwrap()), "{ip} must be rejected");
        }
        for ip in ["::1", "::", "fc00::1", "fd12::1", "fe80::1", "ff02::1"] {
            assert!(!ip_is_public(ip.parse().unwrap()), "{ip} must be rejected");
        }
        assert!(ip_is_public("8.8.8.8".parse().unwrap()));
        assert!(ip_is_public("2606:4700:4700::1111".parse().unwrap()));
    }

    #[tokio::test]
    async fn trusted_mcp_url_policy_requires_safe_public_https_target() {
        for url in [
            "http://8.8.8.8/file",
            "https://user:secret@8.8.8.8/file",
            "https://8.8.8.8:8443/file",
            "https://127.0.0.1/file",
            "https://169.254.169.254/latest/meta-data",
            "https://[::1]/file",
            "https://[fc00::1]/file",
            "https://[fe80::1]/file",
        ] {
            let error = validate_trusted_mcp_download_url(url)
                .await
                .expect_err("unsafe target must fail closed");
            assert!(
                !error.contains(url),
                "error leaked full temporary URL: {error}"
            );
        }
        let target = validate_trusted_mcp_download_url("https://8.8.8.8/file")
            .await
            .expect("public HTTPS literal should be accepted");
        assert_eq!(target.pinned_addrs, vec!["8.8.8.8:443".parse().unwrap()]);
    }

    #[tokio::test]
    async fn trusted_mcp_domain_resolution_fails_closed_on_empty_or_non_public_results() {
        let _lock = lock_import_test_network().await;
        for ips in [
            Vec::<IpAddr>::new(),
            vec!["127.0.0.1".parse().unwrap()],
            vec!["169.254.1.1".parse().unwrap()],
            vec!["::1".parse().unwrap()],
            vec!["fe80::1".parse().unwrap()],
        ] {
            set_import_test_resolved_ips(Some(ips));
            reset_import_test_dns_resolution_count();
            let error = validate_trusted_mcp_download_url("https://download.example/file")
                .await
                .expect_err("unsafe DNS result must fail closed");
            assert!(
                error.contains("resolved to no addresses")
                    || error.contains("resolves to a non-public address"),
                "unexpected error: {error}"
            );
            assert_eq!(import_test_dns_resolution_count(), 1);
        }
        set_import_test_resolved_ips(None);
        reset_import_test_dns_resolution_count();
    }
}
