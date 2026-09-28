//! Explicit opt-in macOS dogfood. This builds and expands a disposable package;
//! it never invokes Installer, administrator authorization, launchctl, or an
//! installed Environment. The fixture is intentionally not executable provenance.
use super::*;
use crate::operation::CancellationSignal;
use crate::updates::download::{stream_installer, verify_file};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use webcodex_environment::unified_update::{sha256, verify_source_manifest, RuntimePlatform};

fn source(platform: RuntimePlatform) -> Vec<u8> {
    let mut components = serde_json::Map::new();
    for name in [
        "webcodex",
        "webcodex-server",
        "webcodex-runner",
        "webcodex-desktop",
    ] {
        let info = json!({"schema_version":1,"binary":name,"version":"99.0.0","git_commit":"a".repeat(40),"git_dirty":false,
            "built_at":"1234567890","target":platform.target(),"architecture":platform.architecture(),
            "desktop_runtime_contract":{"min_generation":1,"max_generation":1},"environment_data_format":1,"agent_protocol_generation":2});
        let path = if name == "webcodex-desktop" {
            "artifacts/WebCodex Desktop.app/Contents/MacOS/webcodex-desktop".to_owned()
        } else {
            format!("artifacts/bin/{name}")
        };
        components.insert(name.into(), json!({"path":path,"sha256":"b".repeat(64),"build_info_sha256":sha256(&serde_json::to_vec(&info).unwrap()),
            "build_info":info,"probe":"native-build-job"}));
    }
    serde_json::to_vec(&json!({"schema_version":1,"version":"99.0.0","source_sha":"a".repeat(40),"platform":platform,
        "target":platform.target(),"architecture":platform.architecture(),"source_workflow_run_id":123,
        "source_workflow_ref":"yyjeqhc/webcodex/.github/workflows/release-build.yml@refs/tags/v99.0.0",
        "desktop_runtime_contract":{"min_generation":1,"max_generation":1},"artifacts":components,
        "desktop_payload":{"path":"artifacts/WebCodex Desktop.app","sha256":"c".repeat(64),"executable":"Contents/MacOS/webcodex-desktop"}})).unwrap()
}

#[tokio::test]
#[ignore = "Explicit isolated macOS package-tool dogfood; no privileged install"]
async fn desktop_real_process_macos_verified_update_package_fixture() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary
        .path()
        .canonicalize()
        .unwrap()
        .join("fixture with spaces");
    std::fs::create_dir(&root).unwrap();
    let payload = root.join("payload");
    std::fs::create_dir(&payload).unwrap();
    std::fs::write(
        payload.join("fixture-only.txt"),
        b"This package must never be installed",
    )
    .unwrap();
    let scripts = root.join("scripts");
    std::fs::create_dir_all(scripts.join("upgrade-candidate")).unwrap();
    let platform = RuntimePlatform::current().unwrap();
    assert!(matches!(
        platform,
        RuntimePlatform::DarwinArm64 | RuntimePlatform::DarwinX64
    ));
    let source_bytes = source(platform);
    std::fs::write(
        scripts.join("upgrade-candidate/source-manifest.json"),
        &source_bytes,
    )
    .unwrap();
    std::fs::write(
        scripts.join("upgrade-candidate/SHA256SUMS"),
        format!("{}  source-manifest.json\n", sha256(&source_bytes)),
    )
    .unwrap();
    // Extraction must not run package hooks. No authority to install this
    // synthetic package is ever requested or constructed by this test.
    std::fs::write(scripts.join("preinstall"), b"#!/bin/sh\nexit 91\n").unwrap();
    std::fs::set_permissions(
        scripts.join("preinstall"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let package = root.join("fixture.pkg");
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        tokio::process::Command::new("/usr/bin/pkgbuild")
            .arg("--root")
            .arg(&payload)
            .args([
                "--identifier",
                "dev.webcodex.updater.fixture",
                "--version",
                "99.0.0",
                "--install-location",
                "/",
            ])
            .arg("--scripts")
            .arg(&scripts)
            .arg(&package)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&package).unwrap();
    let expected_hash = sha256(&bytes);
    let size = bytes.len();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let _ = socket.read(&mut request).await;
        socket
            .write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        socket.write_all(&bytes).await.unwrap();
        socket.shutdown().await.unwrap();
    });
    // Only the injected low-level byte stream is loopback. Publisher/redirect
    // admission is independently tested and accepts no loopback production URL.
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{address}/fixture.pkg"))
        .send()
        .await
        .unwrap();
    let cache = PrivateUpdateCache::open(root.join("private-updater")).unwrap();
    let target = InstallerTarget::new(platform, PackageFormat::Pkg);
    let filename = target.installer_filename("99.0.0");
    let downloaded = stream_installer(
        response,
        &cache,
        &filename,
        &expected_hash,
        1024 * 1024,
        &CancellationSignal::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(downloaded, size as u64);
    assert_eq!(
        verify_file(
            &cache,
            &filename,
            &expected_hash,
            1024 * 1024,
            &CancellationSignal::new()
        )
        .await
        .unwrap(),
        downloaded
    );
    assert!(!cache.file("installer.part").unwrap().exists());
    let candidate = extract_candidate(&cache, &cache.file(&filename).unwrap(), target)
        .await
        .unwrap();
    let extracted = std::fs::read(candidate.join("source-manifest.json")).unwrap();
    assert_eq!(
        std::fs::read_to_string(candidate.join("SHA256SUMS")).unwrap(),
        format!("{}  source-manifest.json\n", sha256(&extracted))
    );
    assert_eq!(sha256(&extracted), sha256(&source_bytes));
    assert_eq!(
        verify_source_manifest(&extracted, "99.0.0", platform)
            .unwrap()
            .platform,
        platform
    );
    // A parsed source document alone cannot grant prepare/install authority.
    // The existing Core verifier refuses absent executable/sidecar payloads.
    assert!(webcodex_environment::verify_upgrade_candidate(&candidate).is_err());
    eprintln!("NATIVE_UPDATER_FIXTURE platform={} bytes={} private_download=true sha256=true source_manifest=true native_pkg_expansion=true core_rejected_incomplete_candidate=true privileged_handoff=false", platform.as_str(), downloaded);
}
