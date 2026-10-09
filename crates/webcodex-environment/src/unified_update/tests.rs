use super::*;
use serde_json::{json, Value};

pub(super) fn manifest() -> Value {
    let mut artifacts = serde_json::Map::new();
    let mut installers = serde_json::Map::new();
    for platform in RuntimePlatform::ALL {
        artifacts.insert(platform.as_str().into(), json!({"url":release_asset_url("1.2.3", &format!("webcodex-v1.2.3-{}.tar.gz", platform.as_str())).unwrap(),"sha256":"a".repeat(64)}));
    }
    for target in InstallerTarget::ALL {
        let filename = target.installer_filename("1.2.3");
        installers.insert(target.as_str(), json!({
            "platform":target.platform,
            "format":target.format,
            "filename":filename,
            "url":release_asset_url("1.2.3",&filename).unwrap(),
            "sha256":"b".repeat(64),
            "source_manifest_url":release_asset_url("1.2.3",&target.platform.source_filename("1.2.3")).unwrap(),
            "source_manifest_sha256":"c".repeat(64)
        }));
    }
    json!({"version":"1.2.3","binaries":RUNTIME_BINARIES,"artifacts":artifacts,"installers":installers})
}
fn parse(value: &Value) -> UpdateResult<UnifiedInstallerManifest> {
    UnifiedInstallerManifest::parse(&serde_json::to_vec(value).unwrap(), "1.2.3")
}

#[test]
fn runtime_platforms_and_installer_targets_are_distinct_deterministic_facts() {
    for (os, arch, expected) in [
        ("linux", "x86_64", RuntimePlatform::LinuxX64),
        ("linux", "aarch64", RuntimePlatform::LinuxArm64),
        ("macos", "x86_64", RuntimePlatform::DarwinX64),
        ("macos", "aarch64", RuntimePlatform::DarwinArm64),
        ("windows", "x86_64", RuntimePlatform::Win32X64),
        ("windows", "aarch64", RuntimePlatform::Win32Arm64),
    ] {
        assert_eq!(RuntimePlatform::from_native(os, arch), Some(expected));
    }
    for (os, arch) in [
        ("linux", "arm"),
        ("macos", "arm64"),
        ("windows", "x86"),
        ("freebsd", "x86_64"),
    ] {
        assert_eq!(RuntimePlatform::from_native(os, arch), None);
    }
    assert_eq!(
        RuntimePlatform::ALL
            .iter()
            .map(|v| v.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        6
    );
    assert_eq!(InstallerTarget::ALL.len(), 8);
    assert_eq!(
        InstallerTarget::ALL
            .iter()
            .filter(|t| t.platform == RuntimePlatform::LinuxX64)
            .count(),
        2
    );
    assert!(InstallerTarget::ALL.iter().all(|t| t.valid()));
    for platform in [RuntimePlatform::LinuxX64, RuntimePlatform::LinuxArm64] {
        let formats = InstallerTarget::ALL
            .iter()
            .filter(|target| target.platform == platform)
            .map(|target| target.format)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(formats, [PackageFormat::Deb, PackageFormat::Rpm].into());
    }
    for platform in [RuntimePlatform::DarwinX64, RuntimePlatform::DarwinArm64] {
        assert_eq!(
            InstallerTarget::ALL
                .iter()
                .filter(|target| target.platform == platform)
                .map(|target| target.format)
                .collect::<Vec<_>>(),
            vec![PackageFormat::Pkg]
        );
    }
    for platform in [RuntimePlatform::Win32X64, RuntimePlatform::Win32Arm64] {
        assert_eq!(
            InstallerTarget::ALL
                .iter()
                .filter(|target| target.platform == platform)
                .map(|target| target.format)
                .collect::<Vec<_>>(),
            vec![PackageFormat::Exe]
        );
    }
    for target in InstallerTarget::ALL {
        assert_eq!(InstallerTarget::parse(&target.as_str()), Some(target));
        assert!(target
            .installer_filename("1.2.3")
            .ends_with(target.format.extension()));
    }
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
        value["installers"]["darwin-arm64-pkg"][field] = "https://evil.invalid/update".into();
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
        .remove("win32-arm64-exe");
    assert!(parse(&missing).is_err());
    let mut extra = manifest();
    extra["unexpected"] = true.into();
    assert!(parse(&extra).is_err());
    let mut unknown = manifest();
    unknown["installers"]["freebsd-x64-pkg"] = unknown["installers"]["linux-x64-deb"].clone();
    assert!(parse(&unknown).is_err());
    let mut uppercase = manifest();
    uppercase["installers"]["linux-x64-deb"]["sha256"] = "A".repeat(64).into();
    assert!(parse(&uppercase).is_err());
    let mut escaped = manifest();
    escaped["installers"]["linux-x64-deb"]["url"] =
        "https://github.com/other/webcodex/releases/download/v1.2.3/setup.deb".into();
    assert!(parse(&escaped).is_err());
    let mut split_source = manifest();
    split_source["installers"]["linux-x64-rpm"]["source_manifest_sha256"] = "d".repeat(64).into();
    assert!(parse(&split_source).is_err());
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

pub(crate) fn source(platform: RuntimePlatform) -> Value {
    let desktop_path = if matches!(
        platform,
        RuntimePlatform::DarwinX64 | RuntimePlatform::DarwinArm64
    ) {
        "artifacts/WebCodex Desktop.app/Contents/MacOS/webcodex-desktop"
    } else if matches!(
        platform,
        RuntimePlatform::Win32X64 | RuntimePlatform::Win32Arm64
    ) {
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
        components.insert(name.into(),json!({"path":if name=="webcodex-desktop"{desktop_path.to_string()}else{format!("artifacts/bin/{name}{}",if matches!(platform, RuntimePlatform::Win32X64 | RuntimePlatform::Win32Arm64){".exe"}else{""})},"sha256":"b".repeat(64),"build_info_sha256":sha256(&serde_json::to_vec(&info).unwrap()),"build_info":info,"probe":"native-build-job"}));
    }
    let mut desktop = if matches!(
        platform,
        RuntimePlatform::DarwinX64 | RuntimePlatform::DarwinArm64
    ) {
        json!({"path":"artifacts/WebCodex Desktop.app","sha256":"c".repeat(64),"executable":"Contents/MacOS/webcodex-desktop"})
    } else {
        json!({"path":desktop_path,"sha256":"b".repeat(64),"executable":if matches!(platform, RuntimePlatform::Win32X64 | RuntimePlatform::Win32Arm64){"WebCodex.exe"}else{"webcodex-desktop"}})
    };
    if matches!(
        platform,
        RuntimePlatform::Win32X64 | RuntimePlatform::Win32Arm64
    ) {
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
    for platform in RuntimePlatform::ALL {
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

#[test]
fn guarded_windows_capability_is_raw_hash_bound_and_keeps_legacy_wire() {
    let platform = RuntimePlatform::Win32X64;
    let mut value = source(platform);
    let verify = |value: &Value| {
        verify_source_manifest(&serde_json::to_vec(value).unwrap(), "1.2.3", platform)
    };
    assert!(!verify(&value).unwrap().supports_guarded_windows_handoff());
    let field = WINDOWS_GUARDED_BOOTSTRAP_BUILD_INFO_FIELD;
    value["artifacts"]["webcodex"]["build_info"][field] = json!(1);
    // An unhashed claim cannot affect admission.
    assert!(verify(&value).is_err());
    let info = value["artifacts"]["webcodex"]["build_info"].clone();
    value["artifacts"]["webcodex"]["build_info_sha256"] =
        sha256(&serde_json::to_vec(&info).unwrap()).into();
    assert!(verify(&value).unwrap().supports_guarded_windows_handoff());
    // Old MachineBuildInfo readers already accept additive fields; the marker
    // is absent from their typed serialization and from installer entries.
    let old: webcodex_core::desktop_runtime_contract::MachineBuildInfo =
        serde_json::from_value(info).unwrap();
    old.validate("webcodex").unwrap();
    assert!(serde_json::to_value(old).unwrap().get(field).is_none());
    let installer = manifest();
    assert!(parse(&installer).is_ok());
    for entry in installer["installers"].as_object().unwrap().values() {
        assert_eq!(entry.as_object().unwrap().len(), 7);
        assert!(entry.get("guarded_handoff_version").is_none());
    }
    let mut changed = installer;
    changed["installers"]["win32-x64-exe"]["guarded_handoff_version"] = json!(1);
    assert!(parse(&changed).is_err());
    value["artifacts"]["webcodex"]["build_info"][field] = json!(2);
    value["artifacts"]["webcodex"]["build_info_sha256"] =
        sha256(&serde_json::to_vec(&value["artifacts"]["webcodex"]["build_info"]).unwrap()).into();
    assert!(!verify(&value).unwrap().supports_guarded_windows_handoff());
    for invalid in [json!(0), json!(true), json!("1"), json!(65536), json!(null)] {
        value["artifacts"]["webcodex"]["build_info"][field] = invalid;
        value["artifacts"]["webcodex"]["build_info_sha256"] =
            sha256(&serde_json::to_vec(&value["artifacts"]["webcodex"]["build_info"]).unwrap())
                .into();
        assert!(verify(&value).is_err());
    }
    let mut linux = source(RuntimePlatform::LinuxX64);
    linux["artifacts"]["webcodex"]["build_info"][field] = json!(1);
    linux["artifacts"]["webcodex"]["build_info_sha256"] =
        sha256(&serde_json::to_vec(&linux["artifacts"]["webcodex"]["build_info"]).unwrap()).into();
    assert!(verify_source_manifest(
        &serde_json::to_vec(&linux).unwrap(),
        "1.2.3",
        RuntimePlatform::LinuxX64
    )
    .is_err());
}

#[test]
fn runtime_flavor_is_explicit_linux_only_and_legacy_full_wire_is_unchanged() {
    let full = InstallerTarget::new(RuntimePlatform::LinuxX64, PackageFormat::Deb);
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        json!({"platform":"linux-x64","format":"deb"})
    );
    let runtime = InstallerTarget::runtime(RuntimePlatform::LinuxX64, PackageFormat::Deb);
    assert_eq!(runtime.as_str(), "linux-x64-runtime-deb");
    assert_eq!(
        runtime.installer_filename("1.2.3"),
        "webcodex-runtime-v1.2.3-linux-x64.deb"
    );
    assert_eq!(
        runtime.source_filename("1.2.3"),
        "webcodex-runtime-source-v1.2.3-linux-x64.json"
    );
    assert_eq!(InstallerTarget::parse(&runtime.as_str()), Some(runtime));
    assert!(!InstallerTarget::runtime(RuntimePlatform::DarwinArm64, PackageFormat::Pkg).valid());
    assert!(!InstallerTarget::runtime(RuntimePlatform::Win32X64, PackageFormat::Exe).valid());
    assert_eq!(
        serde_json::from_value::<InstallerTarget>(serde_json::to_value(full).unwrap())
            .unwrap()
            .flavor,
        PackageFlavor::Full
    );
}

#[test]
fn runtime_source_requires_exact_three_components_and_explicit_schema() {
    for platform in [RuntimePlatform::LinuxX64, RuntimePlatform::LinuxArm64] {
        let mut value = source(platform);
        value["schema_version"] = 2.into();
        value["package_flavor"] = "runtime".into();
        value.as_object_mut().unwrap().remove("desktop_payload");
        value["artifacts"]
            .as_object_mut()
            .unwrap()
            .remove("webcodex-desktop");
        let bytes = serde_json::to_vec(&value).unwrap();
        let checked =
            verify_source_manifest_for_flavor(&bytes, "1.2.3", platform, PackageFlavor::Runtime)
                .unwrap();
        assert_eq!(checked.flavor, PackageFlavor::Runtime);
        assert!(checked.component_build("webcodex-desktop").is_none());
        assert!(verify_source_manifest(&bytes, "1.2.3", platform).is_err());
        for change in [
            "schema_version",
            "package_flavor",
            "desktop_payload",
            "extra_component",
            "missing_component",
            "wrong_identity",
        ] {
            let mut bad = value.clone();
            match change {
                "schema_version" => bad[change] = 1.into(),
                "package_flavor" => bad[change] = "full".into(),
                "desktop_payload" => bad[change] = serde_json::Value::Null,
                "extra_component" => {
                    bad["artifacts"]["webcodex-desktop"] =
                        source(platform)["artifacts"]["webcodex-desktop"].clone();
                }
                "missing_component" => {
                    bad["artifacts"]
                        .as_object_mut()
                        .unwrap()
                        .remove("webcodex-server");
                }
                _ => bad["artifacts"]["webcodex"]["build_info"]["git_dirty"] = true.into(),
            }
            assert!(
                verify_source_manifest_for_flavor(
                    &serde_json::to_vec(&bad).unwrap(),
                    "1.2.3",
                    platform,
                    PackageFlavor::Runtime
                )
                .is_err(),
                "{change}"
            );
        }
        let mut explicit_full = source(platform);
        explicit_full["package_flavor"] = "full".into();
        assert!(verify_source_manifest(
            &serde_json::to_vec(&explicit_full).unwrap(),
            "1.2.3",
            platform
        )
        .is_err());
        let mut broken_full = source(platform);
        broken_full["artifacts"]
            .as_object_mut()
            .unwrap()
            .remove("webcodex-desktop");
        broken_full
            .as_object_mut()
            .unwrap()
            .remove("desktop_payload");
        assert!(verify_source_manifest(
            &serde_json::to_vec(&broken_full).unwrap(),
            "1.2.3",
            platform
        )
        .is_err());
    }
}

pub(super) fn catalog_v2() -> serde_json::Value {
    let mut catalog = manifest();
    catalog["schema_version"] = 2.into();
    for target in InstallerTarget::RUNTIME {
        let name = target.installer_filename("1.2.3");
        catalog["installers"][target.as_str()] = json!({"flavor":"runtime","platform":target.platform,"format":target.format,"filename":name,"url":release_asset_url("1.2.3",&name).unwrap(),"sha256":"d".repeat(64),"source_manifest_url":release_asset_url("1.2.3", &target.source_filename("1.2.3")).unwrap(),"source_manifest_sha256":"e".repeat(64)});
    }
    catalog
}
#[test]
fn v2_catalog_is_complete_strict_and_has_exact_legacy_full_projection() {
    let catalog = catalog_v2();
    let mut extra_full_flavor = catalog.clone();
    extra_full_flavor["installers"]["linux-x64-deb"]["flavor"] = "full".into();
    assert!(UnifiedInstallerManifest::parse_v2(
        &serde_json::to_vec(&extra_full_flavor).unwrap(),
        "1.2.3"
    )
    .is_err());
    let mut legacy_extra = manifest();
    legacy_extra["installers"]["linux-x64-deb"]["flavor"] = "full".into();
    assert!(parse(&legacy_extra).is_err());
    let bytes = serde_json::to_vec(&catalog).unwrap();
    let checked = UnifiedInstallerManifest::parse_v2(&bytes, "1.2.3").unwrap();
    assert_eq!(checked.catalog_version, 2);
    assert_eq!(checked.installers.len(), 12);
    assert!(UnifiedInstallerManifest::parse(&bytes, "1.2.3").is_err());
    let mut legacy = checked;
    legacy.installers.retain(|_, entry| entry.flavor.is_full());
    assert_eq!(serde_json::to_value(&legacy).unwrap(), manifest());
    let mut incomplete = catalog.clone();
    incomplete["installers"]
        .as_object_mut()
        .unwrap()
        .remove("linux-arm64-runtime-rpm");
    assert!(
        UnifiedInstallerManifest::parse_v2(&serde_json::to_vec(&incomplete).unwrap(), "1.2.3")
            .is_err()
    );
    let mut retargeted = catalog;
    retargeted["installers"]["linux-x64-runtime-rpm"]["source_manifest_sha256"] =
        "f".repeat(64).into();
    assert!(
        UnifiedInstallerManifest::parse_v2(&serde_json::to_vec(&retargeted).unwrap(), "1.2.3")
            .is_err()
    );
}
