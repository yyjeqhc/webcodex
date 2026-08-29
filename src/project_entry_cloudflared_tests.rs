use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}

fn leaked(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn test_asset(bytes: &[u8], gzip_archive: bool) -> CloudflaredAsset {
    let binary_sha256 = if gzip_archive {
        unreachable!("archive tests construct their asset explicitly")
    } else {
        sha256_bytes(bytes)
    };
    CloudflaredAsset {
        target: "test-target",
        file_name: "cloudflared-test",
        archive_sha256: leaked(binary_sha256.clone()),
        binary_sha256: leaked(binary_sha256),
        gzip_archive,
    }
}

async fn serve_once(body: Vec<u8>) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let observed = requests.clone();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        observed.fetch_add(1, Ordering::SeqCst);
        let mut request = vec![0_u8; 4096];
        let _ = socket.read(&mut request).await.unwrap();
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(header.as_bytes()).await.unwrap();
        socket.write_all(&body).await.unwrap();
        socket.shutdown().await.unwrap();
    });
    (format!("http://{address}/cloudflared"), requests, task)
}

#[test]
fn release_assets_are_pinned_per_supported_platform() {
    let linux_amd64 = cloudflared_asset_for("linux", "x86_64").unwrap();
    assert_eq!(linux_amd64.file_name, "cloudflared-linux-amd64");
    assert_eq!(
        linux_amd64.archive_sha256,
        "9d71c677db00134c1bd4144b7783486b654ad281b1ea62b4972098d19f770f17"
    );
    assert_eq!(linux_amd64.archive_sha256, linux_amd64.binary_sha256);
    assert!(!linux_amd64.gzip_archive);

    let linux_arm64 = cloudflared_asset_for("linux", "aarch64").unwrap();
    assert_eq!(linux_arm64.file_name, "cloudflared-linux-arm64");
    assert_eq!(
        linux_arm64.binary_sha256,
        "65259e652a7bea08bf5df603233ab22b8bf3116af8df9f9206209af6a1b955c0"
    );

    let darwin_amd64 = cloudflared_asset_for("macos", "x86_64").unwrap();
    assert_eq!(darwin_amd64.file_name, "cloudflared-darwin-amd64.tgz");
    assert_eq!(
        darwin_amd64.archive_sha256,
        "70d1c8684fa6d14b5843787ec8d1ea8e18b23650e424f4ea43d849a506487c3b"
    );
    assert_eq!(
        darwin_amd64.binary_sha256,
        "e88fe5874d42a94f49a7ea59cabc3722d2962d0449232b0f3b1a426a712e275c"
    );
    assert!(darwin_amd64.gzip_archive);

    let darwin_arm64 = cloudflared_asset_for("macos", "aarch64").unwrap();
    assert_eq!(darwin_arm64.file_name, "cloudflared-darwin-arm64.tgz");
    assert_eq!(
        darwin_arm64.archive_sha256,
        "90c5a4f914d705fd70c135dba6d80b1791d254b08d6d4136301941f88330dd09"
    );
    assert_eq!(
        darwin_arm64.binary_sha256,
        "f35c50089cd25f77a4cb5a2152036bc26db15aa31fbe11f7995d2e42a4ed6257"
    );

    let windows_amd64 = cloudflared_asset_for("windows", "x86_64").unwrap();
    assert_eq!(windows_amd64.file_name, "cloudflared-windows-amd64.exe");
    assert_eq!(windows_amd64.target, "windows-amd64");
    assert_eq!(
        windows_amd64.binary_sha256,
        "8635da433b6df8194746e88ed9d2589566c20e38bfc2a80e431a348b7c765841"
    );
    assert_eq!(windows_amd64.archive_sha256, windows_amd64.binary_sha256);
    assert!(!windows_amd64.gzip_archive);

    let windows_arm64 = cloudflared_asset_for("windows", "aarch64").unwrap_err();
    assert_eq!(windows_arm64.code, "tunnel_unavailable");
    assert!(windows_arm64.message.contains("no Windows ARM64 artifact"));
}

#[test]
fn explicit_override_is_authoritative_then_path_is_used() {
    let temp = tempfile::tempdir().unwrap();
    let override_binary = temp.path().join("override-cloudflared");
    let path_dir = temp.path().join("path-bin");
    fs::create_dir(&path_dir).unwrap();
    let path_binary = path_dir.join(executable_name("cloudflared"));
    fs::write(&override_binary, b"override").unwrap();
    fs::write(&path_binary, b"path").unwrap();
    let path = std::env::join_paths([path_dir]).unwrap();

    assert_eq!(
        existing_cloudflared_from(Some(&override_binary), Some(&path)).unwrap(),
        Some(override_binary.clone())
    );
    assert_eq!(
        existing_cloudflared_from(None, Some(&path)).unwrap(),
        Some(path_binary)
    );

    let missing = temp.path().join("missing");
    let error = existing_cloudflared_from(Some(&missing), Some(&path)).unwrap_err();
    assert_eq!(error.code, "tunnel_unavailable");
    assert!(error.message.contains("WEBCODEX_CLOUDFLARED_BIN"));
}

#[test]
fn managed_root_prefers_xdg_then_home_and_requires_private_user_base() {
    assert_eq!(
        managed_cloudflared_root_from(
            Some(OsStr::new("/state")),
            Some(OsStr::new("/home/user")),
            Some(OsStr::new("/local")),
        )
        .unwrap(),
        PathBuf::from("/state/webcodex/tools/cloudflared")
    );
    assert_eq!(
        managed_cloudflared_root_from(
            None,
            Some(OsStr::new("/home/user")),
            Some(OsStr::new("/local")),
        )
        .unwrap(),
        PathBuf::from("/home/user/.local/state/webcodex/tools/cloudflared")
    );
    assert_eq!(
        managed_cloudflared_root_from(None, None, Some(OsStr::new("/local"))).unwrap(),
        PathBuf::from("/local/WebCodex/tools/cloudflared")
    );
    let error = managed_cloudflared_root_from(None, None, None).unwrap_err();
    assert_eq!(error.code, "tunnel_unavailable");
    assert!(error.message.contains("private user directory"));
    let relative_xdg = managed_cloudflared_root_from(
        Some(OsStr::new("relative-state")),
        Some(OsStr::new("/home/user")),
        None,
    )
    .unwrap_err();
    assert!(relative_xdg.message.contains("XDG_STATE_HOME"));
    let relative_home =
        managed_cloudflared_root_from(None, Some(OsStr::new("relative-home")), None).unwrap_err();
    assert!(relative_home.message.contains("HOME"));
    let relative_local =
        managed_cloudflared_root_from(None, None, Some(OsStr::new("relative-local"))).unwrap_err();
    assert!(relative_local.message.contains("LOCALAPPDATA"));
}

#[test]
fn npm_network_environment_is_bounded_and_preserves_precedence() {
    let values = std::collections::HashMap::from([
        (
            "npm_config_https_proxy",
            "http://npm-user:npm-secret@npm-proxy.test:8443",
        ),
        ("npm_config_proxy", "http://npm-proxy.test:8080"),
        ("npm_config_noproxy", "localhost,.internal.test"),
        (
            "npm_config_ca",
            "-----BEGIN CERTIFICATE-----\\ninline-ca\\n-----END CERTIFICATE-----",
        ),
        ("npm_config_strict_ssl", "false"),
        ("HTTPS_PROXY", "http://system-proxy.test:9443"),
        ("NO_PROXY", "system.test"),
    ]);
    let npm = npm_network_settings_from_env_with(|name| {
        values.get(name).map(|value| (*value).to_string())
    });
    let network = resolve_download_network_config_with(
        "https://github.com/cloudflare/cloudflared",
        &npm,
        |name| values.get(name).map(|value| (*value).to_string()),
    )
    .unwrap();
    assert_eq!(
        network.proxy.as_deref(),
        Some("http://npm-user:npm-secret@npm-proxy.test:8443")
    );
    assert_eq!(
        network.no_proxy.as_deref(),
        Some("localhost,.internal.test")
    );
    assert_eq!(network.reject_unauthorized, Some(false));
    assert_eq!(
        network.ca_pem.as_deref(),
        Some(b"-----BEGIN CERTIFICATE-----\ninline-ca\n-----END CERTIFICATE-----".as_slice())
    );
}

#[test]
fn standard_proxy_environment_remains_the_fallback() {
    let npm = NpmNetworkSettings::default();
    let values = std::collections::HashMap::from([
        ("HTTPS_PROXY", "http://system-proxy.test:9443"),
        ("NO_PROXY", "localhost,127.0.0.1"),
    ]);
    let network = resolve_download_network_config_with(
        "https://github.com/cloudflare/cloudflared",
        &npm,
        |name| values.get(name).map(|value| (*value).to_string()),
    )
    .unwrap();
    assert_eq!(
        network.proxy.as_deref(),
        Some("http://system-proxy.test:9443")
    );
    assert_eq!(network.no_proxy.as_deref(), Some("localhost,127.0.0.1"));
    assert_eq!(network.ca_pem, None);
    assert_eq!(network.reject_unauthorized, None);
}

#[test]
fn npm_cafile_is_bounded_and_errors_do_not_expose_paths_or_ca_content() {
    let temp = tempfile::tempdir().unwrap();
    let ca_file = temp.path().join("corporate-secret-ca.pem");
    fs::write(&ca_file, b"not-a-real-certificate-but-bounded").unwrap();
    let npm = NpmNetworkSettings {
        cafile: Some(ca_file.to_string_lossy().into_owned()),
        ..NpmNetworkSettings::default()
    };
    let network =
        resolve_download_network_config_with("https://github.com", &npm, |_| None).unwrap();
    assert_eq!(
        network.ca_pem.as_deref(),
        Some(b"not-a-real-certificate-but-bounded".as_slice())
    );

    let oversized = temp.path().join("too-large-secret-ca.pem");
    let file = File::create(&oversized).unwrap();
    file.set_len(CLOUDFLARED_MAX_CA_FILE_BYTES as u64 + 1)
        .unwrap();
    let error = resolve_download_network_config_with(
        "https://github.com",
        &NpmNetworkSettings {
            cafile: Some(oversized.to_string_lossy().into_owned()),
            ..NpmNetworkSettings::default()
        },
        |_| None,
    )
    .unwrap_err();
    assert!(error.message.contains("size limit"));
    assert!(!error.message.contains("too-large-secret-ca"));

    let invalid_proxy = DownloadNetworkConfig {
        proxy: Some("http://user:proxy-secret@[invalid".to_string()),
        ..DownloadNetworkConfig::default()
    };
    let error = build_managed_download_client(&invalid_proxy).unwrap_err();
    assert!(error.message.contains("proxy URL"));
    assert!(!error.message.contains("proxy-secret"));

    let invalid_ca = DownloadNetworkConfig {
        ca_pem: Some(
            b"-----BEGIN CERTIFICATE-----\nprivate-ca-secret\n-----END CERTIFICATE-----\n".to_vec(),
        ),
        ..DownloadNetworkConfig::default()
    };
    let error = build_managed_download_client(&invalid_ca).unwrap_err();
    assert!(error.message.contains("CA bundle"));
    assert!(!error.message.contains("private-ca-secret"));
}

#[cfg(unix)]
#[tokio::test]
async fn npm_config_query_uses_fixed_argv_and_normalizes_null() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let npm = temp.path().join("fake-npm");
    fs::write(
        &npm,
        b"#!/bin/sh\n[ \"$1\" = config ] && [ \"$2\" = get ] || exit 9\ncase \"$3\" in\n  proxy) printf '%s\\n' 'http://query-proxy.test:8080' ;;\n  ca) printf '%s\\n' null ;;\n  *) exit 8 ;;\nesac\n",
    )
    .unwrap();
    fs::set_permissions(&npm, fs::Permissions::from_mode(0o700)).unwrap();

    assert_eq!(
        query_npm_config_value_with(npm.as_os_str(), "proxy").await,
        Some("http://query-proxy.test:8080".to_string())
    );
    assert_eq!(
        query_npm_config_value_with(npm.as_os_str(), "ca").await,
        None
    );
}

#[tokio::test]
async fn explicit_proxy_and_no_proxy_are_applied_to_managed_downloads() {
    let body = b"proxy-delivered".to_vec();
    let temp = tempfile::tempdir().unwrap();
    let proxied = temp.path().join("proxied.bin");
    let (proxy_url, requests, proxy) = serve_once(body.clone()).await;
    download_cloudflared_asset_with_network(
        "http://origin.invalid/cloudflared?token=must-not-leak",
        &proxied,
        &DownloadNetworkConfig {
            proxy: Some(proxy_url),
            ..DownloadNetworkConfig::default()
        },
    )
    .await
    .unwrap();
    proxy.await.unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    assert_eq!(fs::read(&proxied).unwrap(), body);

    let direct = temp.path().join("direct.bin");
    let (target_url, direct_requests, target) = serve_once(b"direct-delivered".to_vec()).await;
    download_cloudflared_asset_with_network(
        &target_url,
        &direct,
        &DownloadNetworkConfig {
            proxy: Some("http://127.0.0.1:9".to_string()),
            no_proxy: Some("127.0.0.1".to_string()),
            ..DownloadNetworkConfig::default()
        },
    )
    .await
    .unwrap();
    target.await.unwrap();
    assert_eq!(direct_requests.load(Ordering::SeqCst), 1);
    assert_eq!(fs::read_to_string(direct).unwrap(), "direct-delivered");
}

#[cfg(unix)]
#[tokio::test]
async fn managed_binary_is_downloaded_verified_and_reused_without_network() {
    let script = format!("#!/bin/sh\necho 'cloudflared version {CLOUDFLARED_VERSION} (test)'\n");
    let bytes = script.into_bytes();
    let asset = test_asset(&bytes, false);
    let state = tempfile::tempdir().unwrap();
    let (url, requests, server) = serve_once(bytes.clone()).await;

    let installed = ensure_managed_cloudflared_at(state.path(), asset, &url)
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    assert_eq!(fs::read(&installed).unwrap(), bytes);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&installed).unwrap().permissions().mode() & 0o777,
        0o700
    );

    let reused = ensure_managed_cloudflared_at(
        state.path(),
        asset,
        "http://127.0.0.1:9/should-not-be-requested",
    )
    .await
    .unwrap();
    assert_eq!(reused, installed);
}

#[cfg(unix)]
#[tokio::test]
async fn checksum_failure_never_installs_downloaded_binary() {
    let script = format!("#!/bin/sh\necho 'cloudflared version {CLOUDFLARED_VERSION} (test)'\n");
    let bytes = script.into_bytes();
    let asset = CloudflaredAsset {
        target: "checksum-failure",
        file_name: "cloudflared-test",
        archive_sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        binary_sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        gzip_archive: false,
    };
    let state = tempfile::tempdir().unwrap();
    let (url, _requests, server) = serve_once(bytes).await;

    let error = ensure_managed_cloudflared_at(state.path(), asset, &url)
        .await
        .unwrap_err();
    server.await.unwrap();
    assert_eq!(error.code, "tunnel_unavailable");
    assert!(error.message.contains("SHA-256 verification"));
    assert!(!managed_binary_path(state.path(), asset).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn verified_darwin_style_archive_extracts_only_before_binary_verification() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let script = format!("#!/bin/sh\necho 'cloudflared version {CLOUDFLARED_VERSION} (test)'\n");
    let binary = source.join("cloudflared");
    fs::write(&binary, script.as_bytes()).unwrap();
    let archive = temp.path().join("cloudflared.tgz");
    let status = std::process::Command::new("tar")
        .args(["-czf"])
        .arg(&archive)
        .arg("-C")
        .arg(&source)
        .arg("cloudflared")
        .status()
        .unwrap();
    assert!(status.success());
    let archive_bytes = fs::read(&archive).unwrap();
    let archive_hash = leaked(sha256_bytes(&archive_bytes));
    let binary_hash = leaked(sha256_bytes(script.as_bytes()));
    let asset = CloudflaredAsset {
        target: "archive-test",
        file_name: "cloudflared-darwin-test.tgz",
        archive_sha256: archive_hash,
        binary_sha256: binary_hash,
        gzip_archive: true,
    };
    let state = tempfile::tempdir().unwrap();
    let (url, _requests, server) = serve_once(archive_bytes).await;

    let installed = ensure_managed_cloudflared_at(state.path(), asset, &url)
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(fs::read_to_string(installed).unwrap(), script);
}
