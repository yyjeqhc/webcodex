use super::*;

// Constructed from v0.4.3 apps/desktop/src-tauri/src/models.rs, not serialized
// from current Rust structs. Every path/ID is synthetic; no credential bytes.
const V043_STATE: &str = include_str!("test_fixtures/desktop_state_v0_4_3.json");

#[test]
fn v043_desktop_state_and_retired_layouts() {
    // adopt_saved_environment uses the user's EnvironmentStore. Keep that exact
    // entry point under test, but isolate its platform directories in a child
    // rather than changing the test process's global environment or real state.
    let Some(root) = std::env::var_os("WEBCODEX_TEST_V043_UPGRADE_ROOT") else {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "state::legacy_upgrade_tests::v043_desktop_state_and_retired_layouts",
                "--nocapture",
            ])
            .env("WEBCODEX_TEST_V043_UPGRADE_ROOT", &root)
            .env("XDG_CONFIG_HOME", &root)
            .env("APPDATA", &root)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let state = root.join("desktop-state.json");
    std::fs::write(&state, V043_STATE).unwrap();
    let mut config = load_config(&state, &ActivityLog::default()).unwrap();
    let before = serde_json::to_value(&config).unwrap();
    environment::adopt_saved_environment(&mut config);
    assert_eq!(serde_json::to_value(&config).unwrap(), before);
    assert!(config.persistent_environment.is_none());
    assert_eq!(config.saved_projects.len(), 2);
    let runtime = config.runtime.as_ref().unwrap();
    assert_eq!(
        runtime.runner_config.as_deref(),
        Some(Path::new("C:/fixture/profile/runner.toml"))
    );
    assert_eq!(
        runtime.user_token_file.as_deref(),
        Some(Path::new("C:/fixture/profile/webcodex-user-token"))
    );
    assert_eq!(
        runtime.server_env_file.as_deref(),
        Some(Path::new("C:/fixture/runtime/local/webcodex.env"))
    );
    assert!(runtime.runner_client_id.is_none());
    assert_eq!(
        stored_runner_client_id(&config).as_deref(),
        Some("fixture:runner")
    );
    // The optional field also existed in v0.4.3. Its saved identity wins; an
    // absent field may only recover from the exact saved Project suffix.
    let mut explicit: Value = serde_json::from_str(V043_STATE).unwrap();
    explicit["runtime"]["runner_client_id"] = serde_json::json!("fixture-explicit");
    std::fs::write(&state, serde_json::to_vec(&explicit).unwrap()).unwrap();
    let mut saved = load_config(&state, &ActivityLog::default()).unwrap();
    environment::adopt_saved_environment(&mut saved);
    assert_eq!(
        stored_runner_client_id(&saved).as_deref(),
        Some("fixture-explicit")
    );
    let mut mismatched = config.clone();
    mismatched.runtime.as_mut().unwrap().project_id = Some("different-project".into());
    assert!(stored_runner_client_id(&mismatched).is_none());

    let executor = tokio::runtime::Runtime::new().unwrap();
    executor.block_on(check_retired_layouts(&root, config));
}

async fn check_retired_layouts(root: &Path, saved: StoredDesktopConfig) {
    let profile = root.join("profile");
    std::fs::create_dir(&profile).unwrap();
    let runner = profile.join("runner.toml");
    let agent = profile.join("agent.toml");
    let registry = profile.join("projects.d");
    std::fs::write(&agent, "client_id = 'fixture:runner'\n").unwrap();
    let mut config = saved;
    config.runtime.as_mut().unwrap().runner_config = Some(runner.clone());
    let mut core = DesktopCore::new(root.join("desktop"), root.join("resources")).unwrap();
    let cancellation = CancellationContext::never();
    for scenario in 0..5 {
        match scenario {
            0 => {} // runner.toml missing, only agent.toml exists
            1 => {
                // projects.d would no longer be discovered
                std::fs::create_dir(&registry).unwrap();
                std::fs::write(&runner, "client_id = 'fixture:runner'\n").unwrap();
            }
            2 => std::fs::write(&runner, "projects_dir = 'projects.d'\n").unwrap(),
            3 => std::fs::write(
                &runner,
                format!(
                    "project_registry_dir = '{}'\n",
                    profile.join("project-registry").display()
                ),
            )
            .unwrap(),
            4 => {
                // Runner's actual default registry is under its config base.
                std::fs::remove_dir(&registry).unwrap();
                std::fs::create_dir_all(root.join("webcodex/projects.d")).unwrap();
                std::fs::write(&runner, "client_id = 'fixture:runner'\n").unwrap();
            }
            _ => unreachable!(),
        }
        let runner_before = std::fs::read(&runner).ok();
        core.config = config.clone();
        let input: crate::models::EnvironmentInput =
            serde_json::from_value(serde_json::json!({"mode":"create"})).unwrap();
        let error = core
            .configure_environment(input, &cancellation)
            .await
            .unwrap_err();
        assert_eq!(
            error.code, "legacy_runner_layout_unsupported",
            "scenario {scenario}"
        );
        assert!(error.next_action.contains("backup"));
        assert!(error.next_action.contains("client_id"));
        assert_eq!(
            core.resume_saved_runtime(&cancellation)
                .await
                .unwrap_err()
                .code,
            "legacy_runner_layout_unsupported"
        );
        assert!(core
            .process_snapshot(ProcessKey::LocalRunner)
            .await
            .is_none());
        assert!(core
            .process_snapshot(ProcessKey::LocalServer)
            .await
            .is_none());
        assert!(
            webcodex_environment::migration_journal(&environment::store().unwrap())
                .unwrap()
                .is_none()
        );
        assert_eq!(std::fs::read(&runner).ok(), runner_before);
        assert_eq!(
            std::fs::read_to_string(&agent).unwrap(),
            "client_id = 'fixture:runner'\n"
        );
    }
    // Explicit bindings do not rely on discovery, even with the historical
    // filenames. Preserve them and let the existing import identity checks run.
    std::fs::create_dir(&registry).unwrap();
    std::fs::write(
        &agent,
        format!("project_registry_dir = '{}'\n", registry.display()),
    )
    .unwrap();
    config.runtime.as_mut().unwrap().runner_config = Some(agent);
    environment::validate_legacy_runner_layout(&config).unwrap();
    std::fs::write(
        &runner,
        format!("project_registry_dir = '{}'\n", registry.display()),
    )
    .unwrap();
    config.runtime.as_mut().unwrap().runner_config = Some(runner);
    environment::validate_legacy_runner_layout(&config).unwrap();
}
