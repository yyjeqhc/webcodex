use super::*;

#[test]
fn source_is_explicit_and_release_resolution_prefers_custom() {
    let custom_directory = std::env::temp_dir().join("webcodex-missing-runtime");
    assert!(custom_directory.is_absolute());
    let source: RuntimeSource = serde_json::from_value(serde_json::json!({
        "kind": "custom",
        "directory": custom_directory.clone(),
    }))
    .unwrap();
    assert!(matches!(source, RuntimeSource::Custom { .. }));
    let (path, resolution) = source_directory(&source, None).unwrap();
    assert_eq!(path, custom_directory);
    assert_eq!(resolution, ResolvedBinarySource::Custom);
    assert_eq!(RuntimeSource::default(), RuntimeSource::Bundled);
}

#[cfg(unix)]
fn write_fixture_binary(directory: &Path, name: &str, info: &MachineBuildInfo) {
    use std::os::unix::fs::PermissionsExt;
    let bytes = serde_json::to_string(info).unwrap();
    let script = format!("#!/bin/sh\n[ \"$1\" = --build-info-json ] || exit 17\ncat <<'WEBCODEX_BUILD_INFO'\n{bytes}\nWEBCODEX_BUILD_INFO\n");
    let path = directory.join(name);
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(unix)]
fn fixture(changes: impl Fn(usize, &mut MachineBuildInfo)) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "webcodex-runtime-contract-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (index, name) in ["webcodex", "webcodex-server", "webcodex-runner"]
        .into_iter()
        .enumerate()
    {
        let mut info = webcodex_build_info::machine_build_info(name);
        changes(index, &mut info);
        write_fixture_binary(&dir, name, &info);
    }
    dir
}

#[cfg(unix)]
async fn candidate(directory: &Path) -> (RuntimeCandidate, Option<ResolvedBinaries>) {
    probe(
        RuntimeSource::Custom {
            directory: directory.to_path_buf(),
        },
        None,
        7,
        &CancellationContext::never(),
        Deadline::after(std::time::Duration::from_secs(15)),
    )
    .await
    .unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn different_revisions_versions_and_dirty_builds_are_accepted_with_advisories() {
    let dir = fixture(|i, info| {
        info.version = format!("0.{}.0", i + 4);
        info.git_commit = Some(format!("{:040x}", i + 1));
        info.git_dirty = Some(i == 2);
    });
    let (view, resolved) = candidate(&dir).await;
    assert_eq!(view.compatibility, ProtocolCompatibility::Compatible);
    assert!(view
        .binaries
        .iter()
        .all(|binary| binary.present == Some(true)
            && binary.startup_check == BinaryStartupCheck::Passed));
    assert!(view
        .advisories
        .contains(&"different_source_revisions".to_string()));
    assert!(view
        .advisories
        .contains(&"different_package_versions".to_string()));
    assert!(view
        .advisories
        .contains(&"dirty_build_operator_responsibility".to_string()));
    verify_resolved_files(&resolved.unwrap()).await.unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn malformed_unknown_wrong_architecture_and_incompatible_are_not_trusted() {
    for (case, expected) in [
        (0, "build_info_schema_unsupported"),
        (1, "binary_architecture_mismatch"),
        (2, "runtime_contract_incompatible"),
        (3, "runtime_contract_malformed"),
    ] {
        let dir = fixture(|i, info| {
            if i == 1 {
                match case {
                    0 => info.schema_version = 9,
                    1 => info.architecture = "other".into(),
                    2 => {
                        info.desktop_runtime_contract =
                            webcodex_core::desktop_runtime_contract::DesktopRuntimeContract {
                                min_generation: 2,
                                max_generation: 2,
                            }
                    }
                    _ => info.desktop_runtime_contract.min_generation = 0,
                }
            }
        });
        let (view, resolved) = candidate(&dir).await;
        assert!(resolved.is_none());
        assert_eq!(view.error_code.as_deref(), Some(expected));
        assert_eq!(view.binaries[1].startup_check, BinaryStartupCheck::Passed);
        assert_eq!(
            view.compatibility,
            if matches!(case, 1 | 2) {
                ProtocolCompatibility::Incompatible
            } else {
                ProtocolCompatibility::Unknown
            }
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn missing_custom_and_missing_binary_never_fall_back() {
    let dir = fixture(|_, _| {});
    std::fs::remove_file(dir.join("webcodex-runner")).unwrap();
    let (view, resolved) = candidate(&dir).await;
    assert_eq!(view.error_code.as_deref(), Some("binary_missing"));
    assert_eq!(view.compatibility, ProtocolCompatibility::Unknown);
    assert_eq!(view.binaries[2].present, Some(false));
    assert_eq!(
        view.binaries[2].startup_check,
        BinaryStartupCheck::NotChecked
    );
    assert!(resolved.is_none());
    std::fs::remove_dir_all(&dir).unwrap();
    let (view, resolved) = candidate(&dir).await;
    assert_eq!(
        view.error_code.as_deref(),
        Some("runtime_directory_missing")
    );
    assert!(resolved.is_none());
}

#[tokio::test]
async fn a_present_file_is_not_startup_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(file_name("webcodex"));
    std::fs::write(&path, "not an executable image").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let (view, resolved) = probe(
        RuntimeSource::Custom {
            directory: dir.path().into(),
        },
        None,
        0,
        &CancellationContext::never(),
        Deadline::after(std::time::Duration::from_secs(5)),
    )
    .await
    .unwrap();
    assert!(resolved.is_none());
    assert_eq!(view.compatibility, ProtocolCompatibility::Unknown);
    assert_eq!(view.binaries[0].present, Some(true));
    assert_eq!(view.binaries[0].startup_check, BinaryStartupCheck::Failed);
    assert_eq!(
        view.binaries[0].error_code.as_deref(),
        Some("webcodex_command_start_failed")
    );
    assert!(view.binaries[0]
        .diagnostics
        .as_ref()
        .unwrap()
        .io_kind
        .is_some());
    assert_eq!(view.binaries[1].present, Some(false));
    let failure = ResolvedBinaries::resolve_source_until(
        &RuntimeSource::Custom {
            directory: dir.path().into(),
        },
        None,
        &CancellationContext::never(),
        Deadline::after(std::time::Duration::from_secs(5)),
    )
    .await
    .unwrap_err();
    assert_eq!(failure.code, "webcodex_command_start_failed");
    assert_eq!(failure.details.as_ref().unwrap()["phase"], "runtime_probe");
    assert_eq!(failure.details.as_ref().unwrap()["executable"], "webcodex");
}

#[cfg(unix)]
#[tokio::test]
async fn execution_permission_does_not_prove_successful_startup() {
    let dir = fixture(|_, _| {});
    std::fs::write(dir.join("webcodex"), "#!/bin/sh\nexit 17\n").unwrap();
    let (view, resolved) = candidate(&dir).await;
    assert!(resolved.is_none());
    assert_eq!(view.compatibility, ProtocolCompatibility::Unknown);
    let binary = &view.binaries[0];
    assert_eq!(binary.present, Some(true));
    assert_eq!(binary.startup_check, BinaryStartupCheck::Failed);
    assert_eq!(
        binary.error_code.as_deref(),
        Some("webcodex_command_failed")
    );
    assert_eq!(binary.diagnostics.as_ref().unwrap().exit_code, Some(17));
    assert!(binary.sha256.is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn windows_launch_status_is_preserved_without_output_or_paths() {
    let failure = DesktopError::new(
        "webcodex_command_failed",
        "private message",
        "private action",
    )
    .with_details(serde_json::json!({
        "exit_code": 0xc0000022_u32 as i32,
        "stderr": "private output",
        "executable": "private path",
        "reason_code": "private detail",
    }));
    let diagnostics = probe_diagnostics(&failure).unwrap();
    assert_eq!(diagnostics.exit_code.unwrap() as u32, 0xc0000022);
    let value = serde_json::to_value(diagnostics).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 2);
    assert!(!value.to_string().contains("private"));
    assert!(probe_diagnostics(&error("unused")).is_none());
}

#[test]
fn denied_file_inspection_does_not_claim_the_file_is_missing() {
    use std::io::ErrorKind;
    assert_eq!(
        presence_after_inspection_error(ErrorKind::NotFound),
        Some(false)
    );
    assert_eq!(
        presence_after_inspection_error(ErrorKind::PermissionDenied),
        None
    );
    assert_eq!(presence_after_inspection_error(ErrorKind::Other), None);
}

#[cfg(unix)]
#[tokio::test]
async fn a_missing_execute_bit_blocks_startup_without_protocol_evidence() {
    use std::os::unix::fs::PermissionsExt;
    let dir = fixture(|_, _| {});
    std::fs::set_permissions(dir.join("webcodex"), std::fs::Permissions::from_mode(0o600)).unwrap();
    let (view, resolved) = candidate(&dir).await;
    assert!(resolved.is_none());
    assert_eq!(view.compatibility, ProtocolCompatibility::Unknown);
    assert_eq!(view.binaries[0].present, Some(true));
    assert_eq!(view.binaries[0].startup_check, BinaryStartupCheck::Failed);
    assert_eq!(
        view.binaries[0].error_code.as_deref(),
        Some("binary_not_executable")
    );
    assert!(view.binaries[0].metadata.is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn changed_candidate_bytes_invalidate_the_previous_fence() {
    let dir = fixture(|_, _| {});
    let (_, resolved) = candidate(&dir).await;
    let resolved = resolved.unwrap();
    std::fs::write(dir.join("webcodex-runner"), "replaced").unwrap();
    assert_eq!(
        verify_resolved_files(&resolved).await.unwrap_err().code,
        "runtime_candidate_changed"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn state_round_trip_keeps_custom_and_unknown_entries() {
    let config: crate::models::StoredDesktopConfig = serde_json::from_str(r#"{"runtime_binary_source":{"kind":"custom","directory":"/operator/runtime"},"future_owned_setting":{"preserve":true}}"#).unwrap();
    let encoded = serde_json::to_value(&config).unwrap();
    assert_eq!(encoded["runtime_binary_source"]["kind"], "custom");
    assert_eq!(encoded["future_owned_setting"]["preserve"], true);
    assert_eq!(config.schema_version, 1);
}

#[cfg(unix)]
#[tokio::test]
async fn an_ordinary_restart_rejects_replaced_custom_files_until_explicit_reapproval() {
    let dir = fixture(|_, _| {});
    let source = RuntimeSource::Custom {
        directory: dir.clone(),
    };
    let (_, resolved) = candidate(&dir).await;
    let approved_fingerprint = resolved.unwrap().fingerprint;

    let mut replacement = webcodex_build_info::machine_build_info("webcodex-runner");
    replacement.git_commit = Some("abcdef012345abcdef012345abcdef012345abcd".into());
    write_fixture_binary(&dir, "webcodex-runner", &replacement);

    // Model a new Desktop process: source and approval are restored from the
    // persisted config, while no candidate bytes are cached in memory.
    let mut adapter = crate::webcodex::WebCodexAdapter::new(None);
    adapter.set_runtime_source(source);
    adapter.set_runtime_approval(Some(approved_fingerprint));
    let error = adapter
        .ensure_binaries(&CancellationContext::never())
        .await
        .unwrap_err();
    assert_eq!(error.code, "runtime_candidate_changed");
    std::fs::remove_dir_all(dir).unwrap();
}
