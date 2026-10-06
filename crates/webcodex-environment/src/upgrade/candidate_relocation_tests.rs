use super::*;

/// Disposable bytes exercise the actual manifest/hash/containment verifier;
/// they are never executed and need no publisher or package-manager access.
fn write_candidate(root: &Path) -> UpgradeCandidate {
    ensure_private_directory(root).unwrap();
    let platform = crate::unified_update::RuntimePlatform::current().unwrap();
    let mut artifacts = serde_json::Map::new();
    let mut sums = String::new();
    let mut hashes = BTreeMap::new();
    let desktop_relative = if cfg!(target_os = "macos") {
        "artifacts/Desktop.app/Contents/MacOS/webcodex-desktop"
    } else if cfg!(windows) {
        "artifacts/WebCodex.exe"
    } else {
        "artifacts/webcodex-desktop"
    };
    for name in COMPONENTS {
        let relative = if name == "webcodex-desktop" {
            desktop_relative.to_string()
        } else {
            format!("artifacts/bin/{name}{}", std::env::consts::EXE_SUFFIX)
        };
        let path = root.join(&relative);
        // This fixture validates candidate identity/relocation, not storage ACL
        // hardening. The candidate root above is already private; creating nested
        // disposable artifact directories directly avoids coupling these tests to
        // platform-specific ACL mutation on every intermediate directory.
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("disposable {name} bytes")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            for ancestor in path
                .parent()
                .unwrap()
                .ancestors()
                .take_while(|p| *p != root)
            {
                std::fs::set_permissions(ancestor, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let hash = digest(&path).unwrap();
        hashes.insert(name, hash.clone());
        sums.push_str(&format!("{hash}  {relative}\n"));
        let build = json!({
            "schema_version": 1, "binary": name, "version": "1.2.3",
            "git_commit": "a".repeat(40), "git_dirty": false, "built_at": "123",
            "target": platform.target(), "architecture": std::env::consts::ARCH,
            "desktop_runtime_contract": DESKTOP_RUNTIME_CONTRACT,
            "environment_data_format": DATA_FORMAT,
            "agent_protocol_generation": webcodex_core::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2.get()
        });
        artifacts.insert(
            name.into(),
            json!({
                "path": relative, "sha256": hash, "build_info": build,
                "build_info_sha256": hex(&serde_json::to_vec(&build).unwrap()),
                "probe": "native-build-job"
            }),
        );
    }
    let desktop_path = if cfg!(target_os = "macos") {
        "artifacts/Desktop.app"
    } else {
        desktop_relative
    };
    #[cfg(unix)]
    let desktop_hash = desktop_installed_hash(&root.join(desktop_path)).unwrap();
    #[cfg(not(unix))]
    let desktop_hash = hashes["webcodex-desktop"].clone();
    let mut desktop = json!({
        "path": desktop_path, "sha256": desktop_hash,
        "executable": if cfg!(target_os = "macos") {
            "Contents/MacOS/webcodex-desktop"
        } else if cfg!(windows) { "WebCodex.exe" } else { "webcodex-desktop" }
    });
    if cfg!(windows) {
        desktop["managed_files"] = json!([
            {"path":"WebCodex.exe", "sha256":hashes["webcodex-desktop"]},
            {"path":"webcodex-runtime/webcodex.exe", "sha256":hashes["webcodex"]},
            {"path":"webcodex-runtime/webcodex-server.exe", "sha256":hashes["webcodex-server"]},
            {"path":"webcodex-runtime/webcodex-runner.exe", "sha256":hashes["webcodex-runner"]}
        ]);
    }
    let manifest = json!({
        "schema_version": 1, "version": "1.2.3", "source_sha": "a".repeat(40),
        "source_workflow_run_id": 123,
        "source_workflow_ref": "yyjeqhc/webcodex/.github/workflows/release-build.yml@refs/tags/v1.2.3",
        "platform": platform.as_str(), "target": platform.target(),
        "architecture": std::env::consts::ARCH,
        "desktop_runtime_contract": DESKTOP_RUNTIME_CONTRACT,
        "artifacts": artifacts, "desktop_payload": desktop
    });
    let bytes = serde_json::to_vec(&manifest).unwrap();
    sums.push_str(&format!("{}  source-manifest.json\n", hex(&bytes)));
    std::fs::write(root.join("source-manifest.json"), bytes).unwrap();
    std::fs::write(root.join("SHA256SUMS"), sums).unwrap();
    verify_upgrade_candidate(root).unwrap()
}

#[test]
fn candidate_relocation_accepts_verified_private_copy_but_old_comparison_rejects() {
    let directory = crate::test_tempdir().unwrap();
    let prepared = write_candidate(&directory.path().join("owner-private"));
    let relocated = write_candidate(&directory.path().join("hook-private"));
    assert_ne!(
        serde_json::to_value(&prepared).unwrap(),
        serde_json::to_value(&relocated).unwrap()
    );
    verify_relocated_candidate_identity(&prepared, &relocated).unwrap();
    assert_eq!(prepared.manifest_sha256, relocated.manifest_sha256);
    assert_ne!(prepared.root, relocated.root);
    // Root normalization works for bundle directories and Windows mappings,
    // while the installation-relative managed-file identities remain exact.
    let mut bundle_a = prepared.clone();
    let mut bundle_b = relocated.clone();
    for candidate in [&mut bundle_a, &mut bundle_b] {
        let path = candidate.root.join("artifacts/Desktop.app");
        let executable = path.join("Contents/MacOS/webcodex-desktop");
        candidate
            .artifacts
            .get_mut("webcodex-desktop")
            .unwrap()
            .path = executable.clone();
        candidate.desktop.as_mut().unwrap().path = path;
        candidate.desktop.as_mut().unwrap().executable = executable;
        candidate
            .desktop
            .as_mut()
            .unwrap()
            .managed_files
            .insert("WebCodex.exe".into(), "a".repeat(64));
    }
    verify_relocated_candidate_identity(&bundle_a, &bundle_b).unwrap();
    bundle_b
        .desktop
        .as_mut()
        .unwrap()
        .managed_files
        .insert("WebCodex.exe".into(), "b".repeat(64));
    assert!(verify_relocated_candidate_identity(&bundle_a, &bundle_b).is_err());
}

#[test]
fn candidate_relocation_preserves_every_metadata_and_payload_identity() {
    let directory = crate::test_tempdir().unwrap();
    let prepared = write_candidate(&directory.path().join("owner"));
    let relocated = write_candidate(&directory.path().join("hook"));
    for (pointer, value) in [
        ("/version", json!("1.2.4")),
        ("/source_sha", json!("c".repeat(40))),
        ("/platform", json!("changed-platform")),
        ("/source_workflow_run_id", json!(124)),
        ("/source_workflow_ref", json!("changed-workflow")),
        ("/manifest_sha256", json!("c".repeat(64))),
        ("/provenance_verified", json!(true)),
        ("/artifacts/webcodex/sha256", json!("c".repeat(64))),
        ("/artifacts/webcodex/build_info/built_at", json!("124")),
        ("/desktop/sha256", json!("c".repeat(64))),
    ] {
        let mut value_with_change = serde_json::to_value(&relocated).unwrap();
        *value_with_change.pointer_mut(pointer).unwrap() = value;
        let changed = serde_json::from_value(value_with_change).unwrap();
        assert!(
            verify_relocated_candidate_identity(&prepared, &changed).is_err(),
            "{pointer}"
        );
    }
    let mut changed = relocated.clone();
    changed.artifacts.get_mut("webcodex").unwrap().path =
        changed.root.join("different-layout/webcodex");
    assert!(verify_relocated_candidate_identity(&prepared, &changed).is_err());
    changed = relocated.clone();
    changed.artifacts.remove("webcodex-runner");
    assert!(verify_relocated_candidate_identity(&prepared, &changed).is_err());
    changed = relocated.clone();
    changed.desktop = None;
    assert!(verify_relocated_candidate_identity(&prepared, &changed).is_err());
    // The actual verifier still authenticates copied bytes before comparison.
    std::fs::write(&relocated.artifacts["webcodex"].path, b"changed bytes").unwrap();
    assert_eq!(
        verify_upgrade_candidate(&relocated.root).unwrap_err().code,
        "candidate_checksum"
    );
}

#[test]
fn candidate_relocation_rejects_uncontained_absolute_and_parent_paths() {
    let directory = crate::test_tempdir().unwrap();
    let prepared = write_candidate(&directory.path().join("owner"));
    let relocated = write_candidate(&directory.path().join("hook"));
    for path in [
        PathBuf::from("artifacts/bin/webcodex"),
        prepared.artifacts["webcodex"].path.clone(),
        relocated
            .root
            .with_file_name("hook-extra")
            .join("artifacts/bin/webcodex"),
        relocated.root.join("../owner/artifacts/bin/webcodex"),
        relocated.root.clone(),
    ] {
        let mut changed = relocated.clone();
        changed.artifacts.get_mut("webcodex").unwrap().path = path;
        assert_eq!(
            verify_relocated_candidate_identity(&prepared, &changed)
                .unwrap_err()
                .code,
            "candidate_path"
        );
    }
    let mut changed = relocated.clone();
    changed.desktop.as_mut().unwrap().executable =
        prepared.artifacts["webcodex-desktop"].path.clone();
    assert_eq!(
        verify_relocated_candidate_identity(&prepared, &changed)
            .unwrap_err()
            .code,
        "candidate_path"
    );
    changed = relocated.clone();
    changed.root = PathBuf::from("relative-root");
    assert_eq!(
        verify_relocated_candidate_identity(&prepared, &changed)
            .unwrap_err()
            .code,
        "candidate_path"
    );
    changed = relocated.clone();
    changed.root = directory.path().join("hook/../hook");
    assert_eq!(
        verify_relocated_candidate_identity(&prepared, &changed)
            .unwrap_err()
            .code,
        "candidate_path"
    );
    #[cfg(unix)]
    {
        std::fs::remove_file(&relocated.artifacts["webcodex"].path).unwrap();
        std::os::unix::fs::symlink(
            &prepared.artifacts["webcodex"].path,
            &relocated.artifacts["webcodex"].path,
        )
        .unwrap();
        assert_eq!(
            verify_upgrade_candidate(&relocated.root).unwrap_err().code,
            "candidate_path"
        );
    }
}
