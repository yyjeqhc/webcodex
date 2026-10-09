use super::*;

const RUNNER: &str = "server_url = 'http://127.0.0.1:8080'\nclient_id = 'runner-original'\nowner = 'alice'\ndisplay_name = 'Work laptop'\nproject_registry_dir = 'registry'\ntoken = 'secret-canary-token'\nargv = ['secret-canary-argument']\n[extension]\narbitrary = 'secret-canary-extension'\n";

#[test]
fn settings_export_has_actual_allowlisted_values_without_raw_files_or_extensions() {
    let (temp, mut record) = fixture(true, true);
    let root = temp.path().join("environment");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    record.projects.push(crate::ProjectRecord {
        id: "project-original".into(),
        path: project.clone(),
    });
    EnvironmentStore::existing(root.clone())
        .unwrap()
        .save_environment(&record)
        .unwrap();
    private_file(&root.join("runner.toml"), RUNNER.as_bytes());
    EnvironmentStore::open(root.join("server")).unwrap();
    private_file(&root.join("server/webcodex.env"), b"WEBCODEX_DATA=data\nWEBCODEX_TOKEN=secret-canary-server\nARBITRARY_EXTENSION=secret-canary-extension\n");
    std::fs::create_dir(root.join("server/data")).unwrap();
    std::fs::write(
        root.join("server/data/webcodex.db"),
        b"secret-canary-database",
    )
    .unwrap();
    std::fs::write(root.join("server/lifecycle.log"), b"secret-canary-log").unwrap();
    for path in [root.join("unknown.json"), root.join("webcodex-user-token")] {
        private_file(&path, b"secret-canary-file");
    }
    std::fs::write(project.join("source.rs"), b"secret-canary-file").unwrap();
    let before = std::fs::read(root.join("environment.json")).unwrap();
    let inventory = inspect_environment_paths(&root, None);
    let document = build_settings_export(&inventory);
    let json = serde_json::to_value(&document).unwrap();
    assert_eq!(json["kind"], "settings_export");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["environment"]["local_server"]["value"], true);
    assert_eq!(json["environment"]["service_scope"]["value"], "user");
    assert_eq!(
        json["environment"]["projects"]["value"][0]["id"],
        "project-original"
    );
    assert_eq!(
        json["environment"]["projects"]["value"][0]["path"]["value"],
        project.to_str().unwrap()
    );
    assert_eq!(json["device_display_name"]["value"], "Work laptop");
    assert_eq!(json["desktop_preferences"]["state"], "not_applicable");
    let output = serde_json::to_string(&document).unwrap();
    for excluded in [
        "secret-canary",
        "127.0.0.1",
        "runner-original",
        "source.rs",
        "runner.toml",
        "extension",
    ] {
        assert!(!output.contains(excluded), "unexpected field: {excluded}");
    }
    assert!(output.len() < 64 * 1024);
    assert_eq!(
        std::fs::read(root.join("environment.json")).unwrap(),
        before
    );
    assert!(!root.join("setup.lock").exists());
    let manifest = serde_json::to_value(build_backup_manifest(&inventory)).unwrap();
    assert_eq!(manifest["kind"], "manifest_only");
    assert!(manifest["inventory"].get("settings").is_none());
    assert!(!serde_json::to_string(&manifest)
        .unwrap()
        .contains("Work laptop"));
}

#[test]
fn settings_export_preserves_roles_and_unknowns_without_inventing_defaults() {
    for (server, runner) in [(true, true), (true, false), (false, true), (false, false)] {
        let (temp, _) = fixture(server, runner);
        let inventory = inspect_environment_paths(&temp.path().join("environment"), None);
        let document = build_settings_export(&inventory);
        assert_eq!(
            document.environment.local_server,
            Setting::Known { value: server }
        );
        assert_eq!(
            document.environment.local_runner,
            Setting::Known { value: runner }
        );
        assert_eq!(
            document.device_display_name,
            if runner {
                Setting::Unknown
            } else {
                Setting::NotApplicable
            }
        );
        if runner {
            let root = temp.path().join("environment");
            private_file(
                &root.join("runner.toml"),
                RUNNER
                    .replace("display_name = 'Work laptop'\n", "")
                    .as_bytes(),
            );
            assert_eq!(
                build_settings_export(&inspect_environment_paths(&root, None)).device_display_name,
                Setting::Known { value: None }
            );
        }
    }
    let temp = crate::test_tempdir().unwrap();
    let root = temp.path().join("missing");
    let document = build_settings_export(&inspect_environment_paths(&root, None));
    assert_eq!(document.environment.service_scope, Setting::Unknown);
    assert_eq!(document.environment.projects, Setting::Unknown);
    assert!(!root.exists());
}

#[test]
fn invalid_duplicate_secret_or_mismatched_runner_values_remain_unknown() {
    let (temp, _) = fixture(false, true);
    let root = temp.path().join("environment");
    for config in [
        RUNNER.replace(
            "display_name = 'Work laptop'",
            "display_name = 'Work laptop'\ndisplay_name = 'duplicate'",
        ),
        "malformed = [".into(),
        RUNNER.replace("Work laptop", "https://user:password@private.invalid"),
        RUNNER.replace("Work laptop", "wc_pat_secret-canary-token"),
        RUNNER.replace("runner-original", "different-runner"),
        RUNNER.replace("'Work laptop'", "42"),
        RUNNER.replace(
            "project_registry_dir = 'registry'",
            "project_registry_dir = 42",
        ),
    ] {
        private_file(&root.join("runner.toml"), config.as_bytes());
        let document = build_settings_export(&inspect_environment_paths(&root, None));
        assert_eq!(document.device_display_name, Setting::Unknown);
        assert!(!serde_json::to_string(&document)
            .unwrap()
            .contains("secret-canary"));
    }
}

#[test]
fn display_name_changes_preserve_inventory_and_manifest_revisions() {
    let (temp, _) = fixture(false, true);
    let root = temp.path().join("environment");
    private_file(&root.join("runner.toml"), RUNNER.as_bytes());
    let before = inspect_environment_paths(&root, None);
    private_file(
        &root.join("runner.toml"),
        RUNNER.replace("Work laptop", "Home laptop").as_bytes(),
    );
    let after = inspect_environment_paths(&root, None);
    assert_eq!(before.revision, after.revision);
    assert_eq!(
        build_settings_export(&after).device_display_name,
        Setting::Known {
            value: Some("Home laptop".into())
        }
    );
    let mut decoded: PathInventory =
        serde_json::from_value(serde_json::to_value(&after).unwrap()).unwrap();
    recompute_revision(&mut decoded);
    assert_eq!(decoded.revision, after.revision);
    let mut before = serde_json::to_value(build_backup_manifest(&before)).unwrap();
    let mut after = serde_json::to_value(build_backup_manifest(&after)).unwrap();
    for value in [&mut before, &mut after] {
        value["inventory"]["observed_at_ms"] = serde_json::Value::Null;
    }
    assert_eq!(before, after);
}

#[test]
fn settings_schema_rejects_unknown_fields_and_duplicate_projects_stay_unknown() {
    let (temp, mut record) = fixture(false, true);
    record.projects = vec![
        crate::ProjectRecord {
            id: "duplicate".into(),
            path: temp.path().join("project")
        };
        2
    ];
    let root = temp.path().join("environment");
    EnvironmentStore::existing(root.clone())
        .unwrap()
        .save_environment(&record)
        .unwrap();
    let document = build_settings_export(&inspect_environment_paths(&root, None));
    assert_eq!(document.environment.projects, Setting::Unknown);
    let mut json = serde_json::to_value(document).unwrap();
    json["raw_configuration"] = serde_json::json!("secret-canary");
    assert!(serde_json::from_value::<SettingsExport>(json).is_err());
    assert!(serde_json::from_value::<DesktopPreferences>(
        serde_json::json!({"language":"custom-secret", "automatic_update_download":true})
    )
    .is_err());
}

#[test]
fn settings_export_remains_bounded_with_all_saved_project_reference_slots() {
    let (temp, mut record) = fixture(false, true);
    let root = temp.path().join("environment");
    record.projects = (0..16)
        .map(|index| crate::ProjectRecord {
            id: format!("project-{index}"),
            path: temp
                .path()
                .join(format!("{index}-{}", "quote\"".repeat(600))),
        })
        .collect();
    EnvironmentStore::existing(root.clone())
        .unwrap()
        .save_environment(&record)
        .unwrap();
    private_file(&root.join("runner.toml"), RUNNER.as_bytes());
    let inventory = inspect_environment_paths(&root, None);
    let json = serde_json::to_vec_pretty(&build_settings_export(&inventory)).unwrap();
    assert!(json.len() < 1024 * 1024);
    let parsed: SettingsExport = serde_json::from_slice(&json).unwrap();
    assert!(matches!(parsed.environment.projects, Setting::Known { value } if value.len() == 16));
}
