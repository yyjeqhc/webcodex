use super::*;
use webcodex_environment::{
    EnvironmentMode, EnvironmentRecord, EnvironmentStore, LocalAccount, RuntimeBinaries,
    SetupRequest,
};

struct Fixture {
    temp: tempfile::TempDir,
    data: PathBuf,
    root: PathBuf,
    path: PathBuf,
    config: StoredDesktopConfig,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::Builder::new()
            .tempdir_in(std::env::temp_dir().canonicalize().unwrap())
            .unwrap();
        let data = temp.path().join("desktop");
        std::fs::create_dir(&data).unwrap();
        let root = temp.path().join("environment");
        let path = data.join("desktop-state.json");
        Self {
            temp,
            data,
            root,
            path,
            config: StoredDesktopConfig::default(),
        }
    }
    fn observe(&self) -> PathInventory {
        self.observe_builds(&[])
    }
    fn observe_builds(
        &self,
        builds: &[webcodex_core::desktop_runtime_contract::MachineBuildInfo],
    ) -> PathInventory {
        collect_inventory(
            &InventoryContext {
                config: &self.config,
                data_dir: &self.data,
                config_path: &self.path,
                configuration_issue: false,
                cached_builds: builds,
            },
            &self.root,
            "tauri",
        )
    }
    fn save_environment(&mut self, local_server: bool, local_runner: bool) {
        let store = EnvironmentStore::open(self.root.clone()).unwrap();
        let record = EnvironmentRecord {
            schema_version: 1,
            environment_id: "fixture-environment".into(),
            request: SetupRequest {
                runner_display_name: None,
                service_scope: webcodex_environment::service::ServiceScope::User,
                mode: if local_server {
                    EnvironmentMode::Create {
                        listen: "127.0.0.1:1".into(),
                    }
                } else {
                    EnvironmentMode::Join
                },
                server_url: "http://127.0.0.1:1".into(),
                project: None,
                runner: Some(local_runner),
                account: LocalAccount {
                    name: "fixture".into(),
                    identity: "1234".into(),
                    home: self.temp.path().to_path_buf(),
                },
                binaries: RuntimeBinaries {
                    cli: self.temp.path().join("cli"),
                    server: self.temp.path().join("server"),
                    runner: self.temp.path().join("runner"),
                },
            },
            username: Some("fixture".into()),
            runner_client_id: local_runner.then(|| "fixture-runner".into()),
            projects: vec![],
            configured: true,
        };
        store.save_environment(&record).unwrap();
        self.config.persistent_environment = Some(record.environment_id);
    }
    fn exported(&self, name: &str) -> PathBuf {
        self.temp.path().join(name)
    }
}

#[test]
fn observation_does_not_create_state_or_export_private_desktop_values() {
    let mut fixture = Fixture::new();
    fixture.config.extra.insert(
        "arbitrary".into(),
        serde_json::json!({"value":"private-canary-in-desktop-extra"}),
    );
    std::fs::write(&fixture.path, b"private-canary-in-config-body").unwrap();
    let before = std::fs::read(&fixture.path).unwrap();
    let inventory = fixture.observe();
    assert!(!fixture.root.exists());
    assert!(!fixture.data.join("setup.lock").exists());
    assert_eq!(std::fs::read(&fixture.path).unwrap(), before);
    assert_eq!(inventory.roots[0].status, PathStatus::Missing);
    assert_eq!(inventory.roots[1].status, PathStatus::Present);
    let manifest = build_backup_manifest(&inventory);
    let output = String::from_utf8(export::bounded_json(&manifest).unwrap()).unwrap();
    assert!(!output.contains("private-canary"));
    assert!(manifest.does_not_contain_files && manifest.cannot_restore);
    let log = inventory
        .entries
        .iter()
        .find(|entry| entry.id == "desktop.activity")
        .unwrap();
    assert_eq!(log.kind, PathKind::InMemory);
    assert!(log.directory_to_open.is_none());
    assert!(!fixture.data.join("logs").exists());
}

#[test]
fn saved_roles_and_authoritative_root_remain_separate_from_desktop_data() {
    for (server, runner) in [(true, true), (true, false), (false, true), (false, false)] {
        let mut fixture = Fixture::new();
        fixture.save_environment(server, runner);
        let before = std::fs::read(fixture.root.join("environment.json")).unwrap();
        let inventory = fixture.observe();
        assert_eq!(
            inventory.environment_id.as_deref(),
            Some("fixture-environment")
        );
        assert_eq!(
            (inventory.local_server, inventory.local_runner),
            (Some(server), Some(runner))
        );
        assert_eq!(
            inventory.roots[0].canonical_path.as_deref(),
            Some(fixture.root.as_path())
        );
        assert_eq!(
            inventory.roots[1].canonical_path.as_deref(),
            Some(fixture.data.as_path())
        );
        assert_ne!(
            inventory.roots[0].canonical_path,
            inventory.roots[1].canonical_path
        );
        assert_eq!(
            std::fs::read(fixture.root.join("environment.json")).unwrap(),
            before
        );
        assert!(!fixture.root.join("setup.lock").exists());
    }
}

#[test]
fn legacy_and_quick_share_dont_read_an_unrelated_environment() {
    for (experience, server, runner) in [
        (
            Experience::Full,
            ServerTopology::Remote {
                url: "http://host.invalid".into(),
            },
            RunnerTopology::Local,
        ),
        (
            Experience::Full,
            ServerTopology::Remote {
                url: "http://host.invalid".into(),
            },
            RunnerTopology::None,
        ),
        (
            Experience::QuickShare,
            ServerTopology::Local,
            RunnerTopology::Local,
        ),
    ] {
        let mut fixture = Fixture::new();
        std::fs::create_dir(&fixture.root).unwrap();
        std::fs::write(
            fixture.root.join("environment.json"),
            "private-canary-in-unrelated-environment",
        )
        .unwrap();
        fixture.config.topology = Some(crate::models::RuntimeTopology {
            experience,
            server,
            runner,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        let inventory = fixture.observe();
        assert_eq!(inventory.roots[0].status, PathStatus::NotApplicable);
        assert!(inventory.environment_id.is_none());
        assert!(inventory.issues.is_empty());
        assert!(
            !String::from_utf8(export::bounded_json(&inventory).unwrap())
                .unwrap()
                .contains("private-canary")
        );
        for entry in inventory
            .entries
            .iter()
            .filter(|entry| entry.status == PathStatus::Remote)
        {
            assert!(entry.canonical_path.is_none() && entry.directory_to_open.is_none());
        }
    }
}

#[test]
fn navigation_requires_current_revision_and_known_confirmed_local_entry() {
    let fixture = Fixture::new();
    let mut inventory = fixture.observe();
    let request = |id: &str, revision: &str| OpenInventoryRequest {
        entry_id: id.into(),
        expected_revision: revision.into(),
    };
    assert_eq!(
        confirmed_location(&inventory, &request("desktop.root", &inventory.revision)).unwrap(),
        fixture.data
    );
    assert_eq!(
        confirmed_location(
            &inventory,
            &request("/tmp/arbitrary-path", &inventory.revision)
        )
        .unwrap_err()
        .code,
        "inventory_location_unavailable"
    );
    assert_eq!(
        confirmed_location(&inventory, &request("desktop.root", "old-revision"))
            .unwrap_err()
            .code,
        "inventory_changed"
    );
    inventory.entries.push(reference(
        "remote.project",
        "project",
        "project_reference",
        PathKind::RemoteReference,
        PathStatus::Remote,
        SafetyCategory::Data,
    ));
    inventory.entries.push(reference(
        "unknown.directory",
        "server",
        "server_data",
        PathKind::Directory,
        PathStatus::Unconfirmed,
        SafetyCategory::Data,
    ));
    inventory.entries.push(reference(
        "system.log",
        "server",
        "service_journal",
        PathKind::SystemLog,
        PathStatus::Present,
        SafetyCategory::Log,
    ));
    for id in ["remote.project", "unknown.directory", "system.log"] {
        assert_eq!(
            confirmed_location(&inventory, &request(id, &inventory.revision))
                .unwrap_err()
                .code,
            "inventory_location_unavailable"
        );
    }
}

#[test]
fn context_and_environment_changes_fence_old_actions() {
    let mut fixture = Fixture::new();
    fixture.save_environment(true, false);
    let before = fixture.observe();
    fixture.config.runtime_selection_revision += 1;
    let changed = fixture.observe();
    assert_eq!(
        check_revision(&changed, &before.revision).unwrap_err().code,
        "inventory_changed"
    );
    fixture.config.persistent_environment = Some("different-environment".into());
    let changed = fixture.observe();
    assert!(changed
        .issues
        .iter()
        .any(|issue| issue.code == "environment_changed"));
    assert_eq!(
        check_revision(&changed, &changed.revision)
            .unwrap_err()
            .code,
        "inventory_changed"
    );
}

#[test]
fn settings_export_uses_only_owned_preferences_and_existing_native_export_guards() {
    use crate::desktop_locale::DesktopLocale;
    let mut fixture = Fixture::new();
    fixture.save_environment(false, true);
    fixture.config.extra.insert(
        "secret_extension".into(),
        serde_json::json!({"token":"secret-canary"}),
    );
    fixture.config.update_cache.automatic_download = false;
    fixture.config.update_cache.latest = Some(crate::updates::ReleaseNotice {
        version: "secret-canary".into(),
        runtime_version: "secret-canary".into(),
        release_url: "https://user:secret-canary@private.invalid".into(),
        compatibility: webcodex_environment::unified_update::UpdateCompatibility::Unknown,
    });
    std::fs::write(&fixture.path, b"secret-canary-raw-file").unwrap();
    let before = fixture.observe();
    let request = ExportInventoryRequest {
        kind: InventoryDocument::SettingsExport,
        expected_revision: before.revision.clone(),
        path: fixture.exported("settings.json"),
    };
    let preferences = || desktop_preferences(&fixture.config, false, DesktopLocale::EnUs);
    export_document(&before, &request, preferences).unwrap();
    let bytes = std::fs::read(&request.path).unwrap();
    let settings: webcodex_environment::inventory::SettingsExport =
        serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        settings.desktop_preferences,
        Setting::Known {
            value: DesktopPreferences {
                language: SettingsLanguage::EnUs,
                automatic_update_download: false,
            }
        }
    );
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains("secret-canary"));
    assert!(export_document(&before, &request, preferences).is_err());
    assert!(export::write_document(&fixture.path, &bytes, &before).is_err());
    assert_eq!(
        std::fs::read(&fixture.path).unwrap(),
        b"secret-canary-raw-file"
    );
    assert!(!fixture.root.join("setup.lock").exists());
}

#[test]
fn preference_changes_keep_location_actions_valid_and_export_current_values() {
    use crate::desktop_locale::DesktopLocale;
    let mut fixture = Fixture::new();
    let before = fixture.observe();
    fixture.config.update_cache.automatic_download =
        !fixture.config.update_cache.automatic_download;
    let changed = fixture.observe();
    assert_eq!(changed.revision, before.revision);
    assert_eq!(
        confirmed_location(
            &changed,
            &OpenInventoryRequest {
                entry_id: "desktop.root".into(),
                expected_revision: before.revision.clone(),
            }
        )
        .unwrap(),
        fixture.data
    );
    for (kind, name) in [
        (InventoryDocument::Inventory, "inventory.json"),
        (InventoryDocument::BackupManifest, "manifest.json"),
    ] {
        let request = ExportInventoryRequest {
            kind,
            expected_revision: before.revision.clone(),
            path: fixture.exported(name),
        };
        export_document(&changed, &request, || {
            panic!("location documents must not collect Desktop preferences")
        })
        .unwrap();
    }
    let request = ExportInventoryRequest {
        kind: InventoryDocument::SettingsExport,
        expected_revision: before.revision.clone(),
        path: fixture.exported("current-settings.json"),
    };
    export_document(&changed, &request, || {
        desktop_preferences(&fixture.config, false, DesktopLocale::ZhCn)
    })
    .unwrap();
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&request.path).unwrap()).unwrap();
    assert_eq!(settings["inventory_revision"], before.revision);
    assert_eq!(
        settings["desktop_preferences"]["value"]["language"],
        "zh-CN"
    );
    assert_eq!(
        settings["desktop_preferences"]["value"]["automatic_update_download"],
        fixture.config.update_cache.automatic_download
    );
    fixture.config.runtime_selection_revision += 1;
    let stale = ExportInventoryRequest {
        path: fixture.exported("stale-settings.json"),
        ..request
    };
    assert_eq!(
        export_document(&fixture.observe(), &stale, || {
            panic!("stale context must fail before preference collection")
        })
        .unwrap_err()
        .code,
        "inventory_changed"
    );
    assert!(!stale.path.exists());
}

#[test]
fn all_native_locales_project_constrained_export_values() {
    use crate::desktop_locale::DesktopLocale;
    let fixture = Fixture::new();
    let inventory = fixture.observe();
    for (locale, language) in [
        (DesktopLocale::EnUs, "en-US"),
        (DesktopLocale::ZhCn, "zh-CN"),
        (DesktopLocale::ZhTw, "zh-TW"),
        (DesktopLocale::DeDe, "de-DE"),
        (DesktopLocale::FrFr, "fr-FR"),
        (DesktopLocale::JaJp, "ja-JP"),
        (DesktopLocale::KoKr, "ko-KR"),
    ] {
        let request = ExportInventoryRequest {
            kind: InventoryDocument::SettingsExport,
            expected_revision: inventory.revision.clone(),
            path: fixture.exported(&format!("settings-{language}.json")),
        };
        export_document(&inventory, &request, || {
            desktop_preferences(&fixture.config, false, locale)
        })
        .unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&request.path).unwrap()).unwrap();
        assert_eq!(json["desktop_preferences"]["value"]["language"], language);
    }
    assert_eq!(
        desktop_preferences(&fixture.config, true, DesktopLocale::EnUs),
        Setting::Unknown
    );
}

#[test]
fn build_metadata_uses_allowlisted_fields_and_never_probes_programs() {
    let fixture = Fixture::new();
    let mut build = crate::commands::get_desktop_build_info();
    build.binary = "webcodex-runner".into();
    build.version = "1.2.3-private-canary".into();
    build.git_commit = Some("private-canary".into());
    build.built_at = Some("private-canary".into());
    build.architecture = "private-canary".into();
    let inventory = fixture.observe_builds(&[build]);
    let output = String::from_utf8(export::bounded_json(&inventory).unwrap()).unwrap();
    assert!(!output.contains("private-canary"));
    assert_eq!(inventory.builds[1].build.version.as_deref(), Some("1.2.3"));
    assert_eq!(inventory.builds[1].source, BuildSource::PreviouslyVerified);
}

#[test]
fn export_is_explicit_bounded_create_new_and_never_overwrites_configuration() {
    let fixture = Fixture::new();
    let inventory = fixture.observe();
    let path = fixture.exported("inventory.json");
    let bytes = export::bounded_json(&inventory).unwrap();
    export::write_document(&path, &bytes, &inventory).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        export::write_document(&path, b"replacement", &inventory)
            .unwrap_err()
            .code,
        "inventory_export_file_exists_or_unavailable"
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    for managed in [
        fixture.path.clone(),
        fixture.root.join("environment.json"),
        fixture.data.join("new-configuration.json"),
    ] {
        assert_eq!(
            export::write_document(&managed, b"{}", &inventory)
                .unwrap_err()
                .code,
            "inventory_export_managed_path"
        );
    }
    assert!(!fixture.root.exists());
    assert!(!fixture.data.join("new-configuration.json").exists());
    assert_eq!(
        export::write_document(
            &fixture.exported("oversized.json"),
            &vec![b'a'; 1024 * 1024 + 1],
            &inventory
        )
        .unwrap_err()
        .code,
        "inventory_too_large"
    );
    assert!(!fixture.exported("oversized.json").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
            0
        );
    }
}

#[cfg(unix)]
#[test]
fn exports_reject_linked_parent_and_existing_link_destinations() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let inventory = fixture.observe();
    let alias = fixture.exported("alias");
    symlink(&fixture.data, &alias).unwrap();
    assert_eq!(
        export::write_document(&alias.join("export.json"), b"{}", &inventory)
            .unwrap_err()
            .code,
        "inventory_export_path_unconfirmed"
    );
    let target = fixture.exported("foreign.json");
    std::fs::write(&target, b"do not replace").unwrap();
    let link = fixture.exported("link.json");
    symlink(&target, &link).unwrap();
    assert_eq!(
        export::write_document(&link, b"{}", &inventory)
            .unwrap_err()
            .code,
        "inventory_export_file_exists_or_unavailable"
    );
    assert_eq!(std::fs::read(target).unwrap(), b"do not replace");
}

#[test]
fn ipc_requests_cannot_assert_a_navigation_path_or_include_secrets() {
    assert!(serde_json::from_value::<OpenInventoryRequest>(serde_json::json!({"entry_id":"desktop.root","expected_revision":"revision","path":"/tmp/arbitrary"})).is_err());
    assert!(serde_json::from_value::<ExportInventoryRequest>(serde_json::json!({"kind":"backup_manifest","path":"/tmp/export.json","expected_revision":"revision","include_secrets":true})).is_err());
    assert!(serde_json::from_value::<ExportInventoryRequest>(serde_json::json!({"kind":"secret_backup","path":"/tmp/export.json","expected_revision":"revision"})).is_err());
}

#[test]
fn incomplete_inventory_cannot_authorize_an_export_destination() {
    let fixture = Fixture::new();
    for code in [
        "inventory_truncated",
        "tunnel_profiles_truncated",
        "project_references_truncated",
    ] {
        let mut inventory = fixture.observe();
        inventory.issues.push(InventoryIssue {
            code: code.into(),
            entry_id: None,
        });
        let path = fixture.exported("must-not-be-created.json");
        assert_eq!(
            export::write_document(&path, b"{}", &inventory)
                .unwrap_err()
                .code,
            "inventory_incomplete"
        );
        assert!(!path.exists());
        let request = OpenInventoryRequest {
            entry_id: "desktop.root".into(),
            expected_revision: inventory.revision.clone(),
        };
        assert_eq!(
            confirmed_location(&inventory, &request).unwrap(),
            fixture.data
        );
    }
}

#[test]
fn legacy_server_relative_locations_have_no_invented_working_directory() {
    let mut fixture = Fixture::new();
    fixture.config.topology = Some(crate::models::RuntimeTopology {
        experience: Experience::QuickShare,
        server: ServerTopology::Local,
        runner: RunnerTopology::None,
        exposure: Exposure::None,
        enrollment: Enrollment::ManagedPairing,
    });
    // Provision with the existing private-file writer on every platform. A new
    // std::fs file inherits Windows temp ACLs, which the production reader must
    // reject. Rename an unlocked private fixture before writing its env payload.
    let private_store = EnvironmentStore::open(fixture.temp.path().join("legacy-private")).unwrap();
    drop(private_store.lock().unwrap());
    let env_file = private_store.root().join("server.env");
    std::fs::rename(private_store.root().join("setup.lock"), &env_file).unwrap();
    std::fs::write(&env_file,"WEBCODEX_DATA=relative-data\nWEBCODEX_TOOL_REQUEST_TRACE_DIR=relative-traces\nWEBCODEX_TOKEN=private-canary\n").unwrap();
    #[cfg(windows)]
    webcodex_environment::runtime_entry::validate_windows_env_acl(&env_file).unwrap();
    fixture.config.runtime = Some(crate::models::StoredRuntime {
        server_url: "http://127.0.0.1:1".into(),
        server_env_file: Some(env_file),
        runner_config: None,
        user_token_file: None,
        runner_client_id: None,
        project_id: None,
        runtime_project_id: None,
    });
    let inventory = fixture.observe();
    for id in ["server.data", "server.trace"] {
        let entry = inventory
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .unwrap();
        assert_eq!(entry.status, PathStatus::Unconfirmed);
        assert!(entry.canonical_path.is_none() && entry.directory_to_open.is_none());
        assert!(!entry.configured_path.as_ref().unwrap().is_absolute());
    }
    assert!(!inventory
        .entries
        .iter()
        .any(|entry| entry.id == "server.database"));
    assert!(
        !String::from_utf8(export::bounded_json(&inventory).unwrap())
            .unwrap()
            .contains("private-canary")
    );
}
