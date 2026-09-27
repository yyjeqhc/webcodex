use super::*;
use serde_json::{json, Value};

pub(super) fn manifest() -> Value {
    let mut artifacts = serde_json::Map::new();
    let mut installers = serde_json::Map::new();
    for platform in InstallerPlatform::ALL {
        artifacts.insert(platform.as_str().into(), json!({"url":release_asset_url("1.2.3", &format!("webcodex-v1.2.3-{}.tar.gz", platform.as_str())).unwrap(),"sha256":"a".repeat(64)}));
        let filename = platform.installer_filename("1.2.3");
        installers.insert(platform.as_str().into(), json!({"filename":filename,"url":release_asset_url("1.2.3",&filename).unwrap(),"sha256":"b".repeat(64),"source_manifest_url":release_asset_url("1.2.3",&platform.source_filename("1.2.3")).unwrap(),"source_manifest_sha256":"c".repeat(64)}));
    }
    json!({"version":"1.2.3","binaries":RUNTIME_BINARIES,"artifacts":artifacts,"installers":installers})
}
fn parse(value: &Value) -> UpdateResult<UnifiedInstallerManifest> {
    UnifiedInstallerManifest::parse(&serde_json::to_vec(value).unwrap(), "1.2.3")
}

#[test]
fn six_platforms_are_local_deterministic_facts() {
    for (os, arch, expected) in [
        ("linux", "x86_64", InstallerPlatform::LinuxX64),
        ("linux", "aarch64", InstallerPlatform::LinuxArm64),
        ("macos", "x86_64", InstallerPlatform::DarwinX64),
        ("macos", "aarch64", InstallerPlatform::DarwinArm64),
        ("windows", "x86_64", InstallerPlatform::Win32X64),
        ("windows", "aarch64", InstallerPlatform::Win32Arm64),
    ] {
        assert_eq!(InstallerPlatform::from_native(os, arch), Some(expected));
    }
    for (os, arch) in [
        ("linux", "arm"),
        ("macos", "arm64"),
        ("windows", "x86"),
        ("freebsd", "x86_64"),
    ] {
        assert_eq!(InstallerPlatform::from_native(os, arch), None);
    }
    assert_eq!(
        InstallerPlatform::ALL
            .iter()
            .map(|v| v.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        6
    );
}

#[test]
fn manifest_requires_exact_version_shape_platforms_and_source_binding() {
    assert!(parse(&manifest()).is_ok());
    for field in [
        "filename",
        "url",
        "sha256",
        "source_manifest_url",
        "source_manifest_sha256",
    ] {
        let mut value = manifest();
        value["installers"]["darwin-arm64"][field] = "https://evil.invalid/update".into();
        assert!(parse(&value).is_err(), "{field}");
    }
    for value in ["1.2.4", "1.2.3-beta", "../1.2.3", ""] {
        let mut doc = manifest();
        doc["version"] = value.into();
        assert!(parse(&doc).is_err());
    }
    let mut missing = manifest();
    missing["installers"]
        .as_object_mut()
        .unwrap()
        .remove("win32-arm64");
    assert!(parse(&missing).is_err());
    let mut extra = manifest();
    extra["unexpected"] = true.into();
    assert!(parse(&extra).is_err());
    let mut unknown = manifest();
    unknown["installers"]["freebsd-x64"] = unknown["installers"]["linux-x64"].clone();
    assert!(parse(&unknown).is_err());
    let mut uppercase = manifest();
    uppercase["installers"]["linux-x64"]["sha256"] = "A".repeat(64).into();
    assert!(parse(&uppercase).is_err());
    let mut escaped = manifest();
    escaped["installers"]["linux-x64"]["url"] =
        "https://github.com/other/webcodex/releases/download/v1.2.3/setup.deb".into();
    assert!(parse(&escaped).is_err());
}

#[test]
fn duplicate_json_keys_and_unbounded_manifest_fail_closed() {
    assert!(
        UnifiedInstallerManifest::parse(br#"{"version":"1.2.3","version":"1.2.3"}"#, "1.2.3")
            .is_err()
    );
    assert!(strict_json(br#"{"a":{"b":1,"b":2}}"#).is_err());
    assert!(
        UnifiedInstallerManifest::parse(&vec![b' '; MAX_MANIFEST_BYTES as usize + 1], "1.2.3")
            .is_err()
    );
}

#[test]
fn urls_cannot_smuggle_paths_credentials_ports_or_redirect_hosts() {
    assert_eq!(
        release_asset_url("1.2.3", "manifest.json").unwrap(),
        "https://github.com/yyjeqhc/webcodex/releases/download/v1.2.3/manifest.json"
    );
    for value in [
        "../manifest.json",
        "x/y",
        "x\\y",
        "x?token=secret",
        ".",
        "..",
    ] {
        assert!(release_asset_url("1.2.3", value).is_err());
    }
    for value in [
        "http://github.com/a",
        "https://github.com.evil.invalid/a",
        "https://evil.invalid/a",
        "https://u:p@github.com/a",
        "https://github.com:444/a",
        "https://raw.githubusercontent.com/a",
    ] {
        assert!(
            !trusted_download_url(&url::Url::parse(value).unwrap()),
            "{value}"
        );
    }
    for host in [
        "github.com",
        "api.github.com",
        "release-assets.githubusercontent.com",
        "objects.githubusercontent.com",
    ] {
        assert!(trusted_download_url(
            &url::Url::parse(&format!("https://{host}/a")).unwrap()
        ));
    }
}

fn source(platform: InstallerPlatform) -> Value {
    let desktop_path = if platform.extension() == "pkg" {
        "artifacts/WebCodex Desktop.app/Contents/MacOS/webcodex-desktop"
    } else if platform.extension() == "exe" {
        "artifacts/WebCodex.exe"
    } else {
        "artifacts/webcodex-desktop"
    };
    let mut components = serde_json::Map::new();
    for name in [
        "webcodex",
        "webcodex-server",
        "webcodex-runner",
        "webcodex-desktop",
    ] {
        let info = json!({"schema_version":1,"binary":name,"version":"1.2.3","git_commit":"a".repeat(40),"git_dirty":false,"built_at":"1234567890","target":platform.target(),"architecture":platform.architecture(),"desktop_runtime_contract":{"min_generation":1,"max_generation":1},"environment_data_format":1,"agent_protocol_generation":2});
        components.insert(name.into(),json!({"path":if name=="webcodex-desktop"{desktop_path.to_string()}else{format!("artifacts/bin/{name}{}",if platform.extension()=="exe"{".exe"}else{""})},"sha256":"b".repeat(64),"build_info_sha256":sha256(&serde_json::to_vec(&info).unwrap()),"build_info":info,"probe":"native-build-job"}));
    }
    let mut desktop = if platform.extension() == "pkg" {
        json!({"path":"artifacts/WebCodex Desktop.app","sha256":"c".repeat(64),"executable":"Contents/MacOS/webcodex-desktop"})
    } else {
        json!({"path":desktop_path,"sha256":"b".repeat(64),"executable":if platform.extension()=="exe"{"WebCodex.exe"}else{"webcodex-desktop"}})
    };
    if platform.extension() == "exe" {
        desktop["managed_files"] = json!([
            "WebCodex.exe",
            "webcodex-runtime/webcodex.exe",
            "webcodex-runtime/webcodex-server.exe",
            "webcodex-runtime/webcodex-runner.exe"
        ]
        .map(|path| json!({"path":path,"sha256":"b".repeat(64)})));
    }
    json!({"schema_version":1,"version":"1.2.3","source_sha":"a".repeat(40),"platform":platform,"target":platform.target(),"architecture":platform.architecture(),"source_workflow_run_id":123,"source_workflow_ref":"yyjeqhc/webcodex/.github/workflows/release-build.yml@refs/tags/v1.2.3","desktop_runtime_contract":{"min_generation":1,"max_generation":1},"artifacts":components,"desktop_payload":desktop})
}

#[test]
fn source_verification_binds_all_four_native_components_not_just_a_checksum() {
    for platform in InstallerPlatform::ALL {
        let value = source(platform);
        assert!(
            verify_source_manifest(&serde_json::to_vec(&value).unwrap(), "1.2.3", platform).is_ok(),
            "{}",
            platform.as_str()
        );
        for field in [
            "source_sha",
            "source_workflow_ref",
            "target",
            "architecture",
            "version",
        ] {
            let mut altered = value.clone();
            altered[field] = "wrong".into();
            assert!(verify_source_manifest(
                &serde_json::to_vec(&altered).unwrap(),
                "1.2.3",
                platform
            )
            .is_err());
        }
        let mut altered = value.clone();
        altered["artifacts"]["webcodex"]["build_info"]["git_dirty"] = true.into();
        assert!(
            verify_source_manifest(&serde_json::to_vec(&altered).unwrap(), "1.2.3", platform)
                .is_err()
        );
        let mut altered = value.clone();
        altered["artifacts"]
            .as_object_mut()
            .unwrap()
            .remove("webcodex-runner");
        assert!(
            verify_source_manifest(&serde_json::to_vec(&altered).unwrap(), "1.2.3", platform)
                .is_err()
        );
        let mut altered = value.clone();
        altered["artifacts"]["webcodex"]["path"] = "../../webcodex".into();
        assert!(
            verify_source_manifest(&serde_json::to_vec(&altered).unwrap(), "1.2.3", platform)
                .is_err()
        );
    }
}
