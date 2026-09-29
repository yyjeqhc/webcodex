use super::*;

#[tokio::test]
async fn managed_instructions_use_effective_root_without_a_project_or_runner() {
    let temporary = tempfile::tempdir().unwrap();
    let effective = temporary.path().join("physical-desktop-root");
    let app = AppState::new_resolved(
        crate::desktop_data_dir::DesktopDataDir {
            effective: effective.clone(),
            source: crate::desktop_data_dir::DesktopDataDirSource::Environment,
            physical_resolution_changed: true,
        },
        temporary.path().join("resources"),
    )
    .unwrap();
    let before = app.managed_instructions_read().await.unwrap();
    assert!(!before.exists);
    assert_eq!(before.path, effective.join("instructions/AGENTS.md"));
    let saved = app
        .managed_instructions_save(SaveRequest {
            expected_revision: before.revision,
            content: "my local rules".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(saved.path).unwrap(),
        "my local rules"
    );
    assert!(app.get_state().project.is_none());
    let core = app.core.lock().await;
    assert!(core.as_ref().unwrap().config.runtime.is_none());
}
