use super::*;
use serde_json::json;

#[test]
fn rust_build_exact_generated_output_reader_keeps_secret_and_generic_artifact_guards() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("target/dogfood")).unwrap();
    std::fs::write(root.join("target/dogfood/app"), b"controlled output").unwrap();
    let stop = AtomicBool::new(false);
    let inspect = |path: &str| {
        crate::webcodex_runner::artifacts::observe_cargo_build_artifact(
            &root,
            path,
            Instant::now() + Duration::from_secs(10),
            &stop,
        )
    };
    let observed = inspect("target/dogfood/app").unwrap();
    assert_eq!(observed.bytes, 17);
    assert_eq!(observed.path, "target/dogfood/app");
    assert!(
        crate::webcodex_runner::artifacts::observe_build_artifact(&root, "target/dogfood/app")
            .is_err(),
        "non-Cargo build readers retain bulk restrictions"
    );
    assert!(
        crate::webcodex_runner::artifacts::validate_artifact_runner_path("target/dogfood/app")
            .is_err(),
        "generic artifact policy unchanged"
    );
    for path in [
        "target/.env",
        "target/secrets/key",
        "target/node_modules/output",
        "target/credential.pem",
        "../outside",
    ] {
        assert!(inspect(path).is_err(), "{path}");
    }
    #[cfg(unix)]
    {
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("target/dogfood/link")).unwrap();
        assert!(inspect("target/dogfood/link").is_err());
    }
    stop.store(true, Ordering::SeqCst);
    assert_eq!(inspect("target/dogfood/app").unwrap_err(), "build_stopped");
}

fn request() -> ProjectBuildRequest {
    ProjectBuildRequest {
        project_id: "demo".into(),
        cwd: None,
        adapter: webcodex_core::project_build::ProjectBuildAdapter::Rust,
        scope: None,
        dependency_policy: None,
        dependency_resolution_policy: None,
        profile: None,
        targets: None,
        artifact_policy: Some(webcodex_core::project_build::ProjectBuildArtifactPolicy::Observe),
    }
}

#[test]
fn rust_build_artifact_observation_requires_complete_selected_safe_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("demo"), "artifact").unwrap();
    let selected = SelectedPackages {
        ids: BTreeSet::from(["local".into()]),
    };
    let artifact = json!({"reason":"compiler-artifact","package_id":"local","target":{"name":"demo","kind":["bin"],"src_path":root.join("main.rs")},"filenames":[root.join("demo")],"fresh":true});
    let finished = json!({"reason":"build-finished","success":true});
    let payload = format!("{artifact}\n{finished}\n");
    assert_eq!(
        artifact_paths(&root, &request(), &selected, &payload).unwrap(),
        ["demo"]
    );
    for text in [
        artifact.to_string(),
        format!("{artifact}\n{{bad\n{finished}"),
        format!("{payload}{artifact}\n"),
        finished.to_string(),
    ] {
        assert!(artifact_paths(&root, &request(), &selected, &text).is_err());
    }
    let mut foreign = artifact.clone();
    foreign["package_id"] = json!("dependency");
    assert!(artifact_paths(
        &root,
        &request(),
        &selected,
        &format!("{foreign}\n{finished}")
    )
    .is_err());
    let outside = tempfile::NamedTempFile::new().unwrap();
    let mut escaped = artifact.clone();
    escaped["filenames"] = json!([outside.path()]);
    assert_eq!(
        artifact_paths(
            &root,
            &request(),
            &selected,
            &format!("{escaped}\n{finished}")
        )
        .unwrap_err(),
        "build_artifact_path_invalid"
    );
    let mut selected_request = request();
    selected_request.targets = Some(ProjectBuildTargets {
        library: false,
        binaries: vec!["another".into()],
    });
    assert_eq!(
        artifact_paths(&root, &selected_request, &selected, &payload).unwrap_err(),
        "build_artifacts_missing"
    );
}

#[test]
fn rust_build_artifact_metadata_selects_only_project_packages_and_rejects_truncation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    std::fs::write(root.join("Cargo.toml"), "").unwrap();
    let metadata = json!({"packages":[{"id":"local","name":"demo","manifest_path":root.join("Cargo.toml")}],"workspace_members":["local"],"workspace_default_members":["local"]});
    assert_eq!(
        selected_packages(&root, &root, &request(), &metadata.to_string())
            .unwrap()
            .ids,
        BTreeSet::from(["local".into()])
    );
    let outside = tempfile::NamedTempFile::new().unwrap();
    let mut invalid = metadata.clone();
    invalid["packages"][0]["manifest_path"] = json!(outside.path());
    assert!(selected_packages(&root, &root, &request(), &invalid.to_string()).is_err());
    let mut result = ShellCommandResult::completed(CommandResult {
        exit_code: Some(0),
        stdout: Some(metadata.to_string()),
        stderr: None,
        duration_ms: None,
        error: None,
    });
    result.stdout_truncated = true;
    assert_eq!(
        parsed_output(&result).unwrap_err(),
        "build_artifact_output_truncated"
    );
    result.stdout_truncated = false;
    result.result.stdout = Some("\u{fffd}".into());
    assert_eq!(
        parsed_output(&result).unwrap_err(),
        "build_artifact_output_invalid_utf8"
    );
}

#[test]
fn rust_build_artifact_selection_uses_admitted_typed_workspace_exclusions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    std::fs::write(root.join("Cargo.toml"), "").unwrap();
    std::fs::create_dir(root.join("member")).unwrap();
    std::fs::write(root.join("member/Cargo.toml"), "").unwrap();
    let metadata = json!({"packages":[{"id":"local","name":"demo","manifest_path":root.join("Cargo.toml")},{"id":"member-id","name":"member","manifest_path":root.join("member/Cargo.toml")}],"workspace_members":["local","member-id"],"workspace_default_members":["local","member-id"]});
    let mut request = request();
    request.scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: vec!["member".into()],
    });
    assert_eq!(
        selected_packages(&root, &root, &request, &metadata.to_string())
            .unwrap()
            .ids,
        BTreeSet::from(["local".into()])
    );
    request
        .scope
        .as_mut()
        .unwrap()
        .exclude_packages
        .push("demo".into());
    assert!(selected_packages(&root, &root, &request, &metadata.to_string()).is_err());
}
