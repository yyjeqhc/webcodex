use super::*;

#[test]
fn absent_status_is_bounded_private_and_creates_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("desktop");
    let environment = temp.path().join("environment");
    let manager = UpdateManager::with_environment_root(data.clone(), environment.clone());
    let view = manager.status_view(&[]).unwrap();
    assert_eq!(view.schema_version, 1);
    assert_eq!(view.installed.len(), 4);
    assert_eq!(view.candidate_components.len(), 4);
    assert!(!view.download.can_install);
    assert!(view.upgrade.is_none());
    assert!(!data.exists());
    assert!(!environment.exists());
    let bytes = serde_json::to_vec(&view).unwrap();
    assert!(bytes.len() <= MAX_UPDATE_VIEW_BYTES);
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains(&temp.path().display().to_string()));
    for field in [
        "argv",
        "receipt",
        "installer_path",
        "candidate_dir",
        "journal",
    ] {
        assert!(!text.contains(field), "{field}");
    }
}

#[test]
fn component_projection_rejects_custom_identifiers_without_exposing_probe_canaries() {
    let build: MachineBuildInfo = serde_json::from_value(serde_json::json!({
        "schema_version":1,"binary":"webcodex-desktop","version":"1.2.3",
        "git_commit":"a".repeat(40),"git_dirty":true,"built_at":"1700000000",
        "target":"x86_64-unknown-linux-gnu","architecture":"x86_64",
        "desktop_runtime_contract":{"min_generation":1,"max_generation":1}
    }))
    .unwrap();
    let canary = "PRIVATE.CANARY";
    for (field, value) in [
        ("version", canary.to_string()),
        ("version", format!("1.2.3+{canary}")),
        ("version", format!("1.2.3-{canary}")),
        ("target", canary.to_string()),
        ("architecture", canary.to_string()),
    ] {
        let mut custom = build.clone();
        match field {
            "version" => custom.version = value,
            "target" => custom.target = value,
            "architecture" => custom.architecture = value,
            _ => unreachable!(),
        }
        assert!(custom.validate("webcodex-desktop").is_ok());
        let projected = components(&[custom]);
        assert!(projected.iter().all(|component| component.build.is_none()));
        assert!(!serde_json::to_string(&projected).unwrap().contains(canary));
    }
    let projected = components(&[build]);
    assert_eq!(projected[0].build.as_ref().unwrap().version, "1.2.3");
    assert_eq!(projected[0].build.as_ref().unwrap().git_dirty, Some(true));
}

#[tokio::test]
async fn active_download_projection_keeps_live_progress_without_saved_state_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("desktop");
    let manager =
        UpdateManager::with_environment_root(data.clone(), temp.path().join("environment"));
    manager.change(|record| {
        record.phase = DownloadPhase::Downloading;
        record.downloaded_bytes = 42;
        record.total_bytes = None;
    });
    let _guard = manager.attempt.lock().await;
    let view = manager.status_view(&[]).unwrap();
    assert_eq!(view.download.phase, DownloadPhase::Downloading);
    assert_eq!(view.download.downloaded_bytes, 42);
    assert_eq!(view.download.total_bytes, None);
    assert!(!view.download.can_install);
    assert!(!data.exists());
}

#[test]
fn verified_runtime_target_projects_three_components_without_inference() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("desktop");
    let manager =
        UpdateManager::with_environment_root(data.clone(), temp.path().join("environment"));
    let platform = RuntimePlatform::current().unwrap();
    let target = InstallerTarget::runtime(platform, PackageFormat::Deb);
    if cfg!(target_os = "linux") {
        let view = manager.status_view_for_target(&[], Some(target)).unwrap();
        assert_eq!(view.installed_target, Some(target));
        assert_eq!(view.installed.len(), 3);
        assert_eq!(view.candidate_components.len(), 3);
        assert!(view
            .installed
            .iter()
            .all(|c| c.binary != "webcodex-desktop"));
        assert!(view.installed.iter().all(|c| c.build.is_none()));
    } else {
        assert!(manager.status_view_for_target(&[], Some(target)).is_err());
    }
    let unknown = manager.status_view(&[]).unwrap();
    assert!(unknown.installed_target.is_none());
    assert_eq!(unknown.installed.len(), 4);
    assert!(!data.exists());
}
