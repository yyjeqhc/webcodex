use super::*;
use crate::unified_update::{RuntimePlatform, RUNTIME_BINARIES};

#[test]
fn runtime_candidate_requires_explicit_native_three_component_bytes() {
    let directory = crate::test_tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let platform = RuntimePlatform::current().unwrap();
    let mut manifest = crate::unified_update::tests::source(platform);
    manifest["schema_version"] = 2.into();
    manifest["package_flavor"] = "runtime".into();
    manifest.as_object_mut().unwrap().remove("desktop_payload");
    manifest["artifacts"]
        .as_object_mut()
        .unwrap()
        .remove("webcodex-desktop");
    let mut sums = String::new();
    for binary in RUNTIME_BINARIES {
        let item = &mut manifest["artifacts"][binary];
        let path = item["path"].as_str().unwrap();
        let destination = root.join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        let bytes = format!("fixture bytes for {binary}");
        std::fs::write(destination, bytes.as_bytes()).unwrap();
        let hash = hex(bytes.as_bytes());
        sums.push_str(&format!("{hash}  {path}\n"));
        item["sha256"] = hash.into();
    }
    let write = |value: &Value| {
        let bytes = serde_json::to_vec(value).unwrap();
        std::fs::write(root.join("source-manifest.json"), &bytes).unwrap();
        std::fs::write(
            root.join("SHA256SUMS"),
            format!("{sums}{}  source-manifest.json\n", hex(&bytes)),
        )
        .unwrap();
    };
    write(&manifest);
    let candidate = verify_upgrade_candidate(&root).unwrap();
    assert_eq!(candidate.package_flavor, PackageFlavor::Runtime);
    assert_eq!(candidate.artifacts.len(), 3);
    assert!(candidate.desktop.is_none());
    assert!(!candidate.provenance_verified);
    // Full keeps its strict four-component admission, including Desktop.
    let mut full = manifest.clone();
    full["schema_version"] = 1.into();
    full.as_object_mut().unwrap().remove("package_flavor");
    write(&full);
    assert_eq!(
        verify_upgrade_candidate(&root).unwrap_err().code,
        "candidate_components"
    );
    let mut mixed = manifest.clone();
    mixed["desktop_payload"] = Value::Null;
    write(&mixed);
    assert_eq!(
        verify_upgrade_candidate(&root).unwrap_err().code,
        "candidate_components"
    );
    write(&manifest);
    std::fs::write(
        root.join(
            manifest["artifacts"]["webcodex-runner"]["path"]
                .as_str()
                .unwrap(),
        ),
        b"tampered",
    )
    .unwrap();
    assert_eq!(
        verify_upgrade_candidate(&root).unwrap_err().code,
        "candidate_checksum"
    );
}
