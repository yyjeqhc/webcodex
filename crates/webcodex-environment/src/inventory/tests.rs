use super::*;
use crate::{EnvironmentMode, EnvironmentStore, LocalAccount, RuntimeBinaries, SetupRequest};
use std::path::PathBuf;

fn fixture(local_server: bool, runner: bool) -> (tempfile::TempDir, EnvironmentRecord) {
    let temp = crate::test_tempdir().unwrap();
    let root = temp.path().join("environment");
    let store = EnvironmentStore::open(root.clone()).unwrap();
    let record = EnvironmentRecord {
        schema_version: 1,
        environment_id: "environment-original".into(),
        username: Some("alice".into()),
        runner_client_id: runner.then(|| "runner-original".into()),
        projects: vec![],
        configured: true,
        request: SetupRequest {
            runner_display_name: None,
            service_scope: crate::service::ServiceScope::User,
            mode: if local_server {
                EnvironmentMode::Create {
                    listen: "127.0.0.1:8080".into(),
                }
            } else {
                EnvironmentMode::Join
            },
            server_url: "http://127.0.0.1:8080".into(),
            project: None,
            runner: Some(runner),
            account: LocalAccount {
                name: "alice".into(),
                identity: "1000".into(),
                home: temp.path().to_path_buf(),
            },
            binaries: RuntimeBinaries {
                cli: root.join("webcodex"),
                server: root.join("webcodex-server"),
                runner: root.join("webcodex-runner"),
            },
        },
    };
    store.save_environment(&record).unwrap();
    (temp, record)
}
fn entry<'a>(inventory: &'a PathInventory, id: &str) -> &'a PathEntry {
    inventory.entries.iter().find(|e| e.id == id).unwrap()
}
fn private_file(path: &Path, bytes: &[u8]) {
    crate::storage::atomic_private_write(path, bytes).unwrap();
}

#[test]
fn missing_inventory_creates_nothing_and_stale_identity_is_explicit() {
    let temp = crate::test_tempdir().unwrap();
    let root = temp.path().join("never-created");
    let inventory = inspect_environment_paths(&root, None);
    assert_eq!(inventory.roots[0].status, PathStatus::Missing);
    assert!(inventory.environment_id.is_none());
    assert!(!root.exists());
    assert!(inspect_environment_paths(&root, Some("old"))
        .issues
        .iter()
        .any(|i| i.code == "environment_changed"));
}

#[test]
fn modes_and_saved_journal_are_observed_without_setup_or_locks() {
    for (server, runner) in [(true, true), (true, false), (false, true), (false, false)] {
        let (temp, record) = fixture(server, runner);
        let root = temp.path().join("environment");
        let inventory = inspect_environment_paths(&root, Some(&record.environment_id));
        assert_eq!(inventory.local_server, Some(server));
        assert_eq!(inventory.local_runner, Some(runner));
        assert_eq!(
            inventory.environment_id.as_deref(),
            Some("environment-original")
        );
        if !server {
            assert_eq!(entry(&inventory, "server.data").status, PathStatus::Remote);
        }
        if !runner {
            assert_eq!(
                entry(&inventory, "runner.configuration").status,
                PathStatus::NotConfigured
            );
        }
        assert!(!root.join("setup.lock").exists());
        assert!(!root.join("service-events.log").exists());
        let manifest = build_backup_manifest(&inventory);
        assert_eq!(manifest.kind, "manifest_only");
        assert!(manifest.cannot_restore && manifest.does_not_contain_files);
        assert_eq!(
            manifest
                .required_restore_materials
                .iter()
                .any(|r| r.id == "runner_identity_configuration_and_registration"),
            runner
        );
    }
}

#[test]
fn known_mixed_configs_project_custom_locations_and_never_secret_values() {
    let (temp, _) = fixture(true, true);
    let root = temp.path().join("environment");
    let custom_data = temp.path().join("custom-data");
    let custom_trace = temp.path().join("custom-trace");
    let custom_registry = temp.path().join("custom-registry");
    private_file(&root.join("server/webcodex.env"), format!("WEBCODEX_DATA='{}'\nWEBCODEX_TOOL_REQUEST_TRACE_DIR={}\nWEBCODEX_TOKEN=wc_boot_PRIVATE_CANARY\n", custom_data.display(), custom_trace.display()).as_bytes());
    private_file(&root.join("runner.toml"), format!("server_url='http://127.0.0.1:8080'\nclient_id='runner-original'\nowner='alice'\nproject_registry_dir='{}'\ntoken='wc_agent_PRIVATE_CANARY'\n", custom_registry.display()).as_bytes());
    private_file(&root.join("webcodex-user-token"), b"wc_pat_PRIVATE_CANARY");
    private_file(
        &root.join("enrollment-recovery.json"),
        b"PRIVATE_CANARY_RECOVERY",
    );
    let inventory = inspect_environment_paths(&root, None);
    assert_eq!(
        entry(&inventory, "server.data").configured_path.as_ref(),
        Some(&custom_data)
    );
    assert_eq!(
        entry(&inventory, "server.trace").configured_path.as_ref(),
        Some(&custom_trace)
    );
    assert_eq!(
        entry(&inventory, "runner.registry")
            .configured_path
            .as_ref(),
        Some(&custom_registry)
    );
    let manifest = serde_json::to_string(&build_backup_manifest(&inventory)).unwrap();
    assert!(!manifest.contains("PRIVATE_CANARY"));
    assert!(manifest.len() < 1024 * 1024);
}

#[test]
fn duplicates_relative_paths_and_malformed_config_are_not_guessed() {
    let temp = crate::test_tempdir().unwrap();
    let root = temp.path().join("server");
    private_file(
        &root.join("webcodex.env"),
        b"export WEBCODEX_DATA='relative data'\n",
    );
    let observed = inspect_server_locations(&root.join("webcodex.env"), Some(&root));
    assert_eq!(
        observed.data.configured_path,
        Some(PathBuf::from("relative data"))
    );
    assert_eq!(
        observed.trace.configured_path,
        Some(PathBuf::from("relative data/tool-request-traces"))
    );
    assert_eq!(observed.data.status, PathStatus::Missing);
    private_file(
        &root.join("webcodex.env"),
        b"WEBCODEX_DATA=../outside-data\n",
    );
    let parent_relative = inspect_server_locations(&root.join("webcodex.env"), Some(&root));
    assert_eq!(parent_relative.data.status, PathStatus::Unconfirmed);
    assert_eq!(
        parent_relative.data.configured_path,
        Some(PathBuf::from("../outside-data"))
    );
    assert!(parent_relative.data.directory_to_open.is_none());
    private_file(
        &root.join("webcodex.env"),
        b"WEBCODEX_DATA=one\nWEBCODEX_DATA=two\n",
    );
    assert_eq!(
        inspect_server_locations(&root.join("webcodex.env"), Some(&root))
            .data
            .status,
        PathStatus::Unconfirmed
    );
    private_file(&root.join("webcodex.env"), b"private canary malformed\n");
    let result = inspect_server_locations(&root.join("webcodex.env"), Some(&root));
    assert_eq!(result.data.status, PathStatus::Invalid);
    assert!(result.data.configured_path.is_none());
}

#[test]
fn invalid_and_oversized_records_fail_closed_without_fallback() {
    let (temp, record) = fixture(false, false);
    let root = temp.path().join("environment");
    private_file(
        &root.join("setup.json"),
        &serde_json::to_vec(&crate::SetupJournal {
            schema_version: 1,
            operation_id: "setup-original".into(),
            environment: record,
            steps: Default::default(),
            last_diagnostic: None,
        })
        .unwrap(),
    );
    private_file(&root.join("environment.json"), b"broken private canary");
    assert!(inspect_environment_paths(&root, None)
        .environment_id
        .is_none());
    private_file(&root.join("environment.json"), &vec![b' '; 1024 * 1024 + 1]);
    assert_eq!(
        entry(
            &inspect_environment_paths(&root, None),
            "environment.record"
        )
        .status,
        PathStatus::Unreadable
    );
    std::fs::remove_file(root.join("environment.json")).unwrap();
    assert_eq!(
        inspect_environment_paths(&root, None)
            .environment_id
            .as_deref(),
        Some("environment-original")
    );
}

#[cfg(unix)]
#[test]
fn linked_roots_and_private_file_links_are_rejected_without_repair() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (temp, _) = fixture(true, false);
    let root = temp.path().join("environment");
    let alias = temp.path().join("linked");
    symlink(&root, &alias).unwrap();
    assert_eq!(
        inspect_environment_paths(&alias, None).roots[0].status,
        PathStatus::UnsafePath
    );
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        inspect_environment_paths(&root, None).roots[0].status,
        PathStatus::Unreadable
    );
    assert_eq!(
        std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o755
    );
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    symlink(root.join("environment.json"), root.join("setup.json")).unwrap();
    assert_eq!(
        entry(&inspect_environment_paths(&root, None), "environment.setup").status,
        PathStatus::UnsafePath
    );
}

#[test]
fn revision_changes_with_projected_configuration_but_not_time() {
    let (temp, _) = fixture(true, false);
    let root = temp.path().join("environment");
    private_file(
        &root.join("server/webcodex.env"),
        b"WEBCODEX_DATA=./first\nWEBCODEX_TOKEN=wc_boot_unchanged\n",
    );
    let mut before = inspect_environment_paths(&root, None);
    let revision = before.revision.clone();
    before.observed_at_ms += 1;
    recompute_revision(&mut before);
    assert_eq!(before.revision, revision);
    private_file(
        &root.join("server/webcodex.env"),
        b"WEBCODEX_DATA=./second\nWEBCODEX_TOKEN=wc_boot_unchanged\n",
    );
    assert_ne!(inspect_environment_paths(&root, None).revision, revision);
    let stale = inspect_environment_paths(&root, Some("another-environment"));
    assert!(stale.issues.iter().any(|i| i.code == "environment_changed"));
    assert!(stale.local_server.is_none());
}

#[test]
fn secret_like_path_values_are_suppressed_and_profile_inventory_is_bounded() {
    let (temp, _) = fixture(true, false);
    let root = temp.path().join("environment");
    private_file(
        &root.join("server/webcodex.env"),
        b"WEBCODEX_DATA=/tmp/wc_pat_PRIVATE_CANARY\n",
    );
    let profiles: Vec<_> = (0..100)
        .map(|i| crate::TunnelRecord {
            profile_id: format!("profile{i}"),
            name: format!("Profile {i}"),
            host_mode: crate::TunnelHostMode::Standalone,
            autostart: true,
            revision: 1,
            runtime_revision: 1,
            installed: true,
            started: false,
        })
        .collect();
    private_file(
        &root.join("tunnel.json"),
        &serde_json::to_vec(&profiles).unwrap(),
    );
    let inventory = inspect_environment_paths(&root, None);
    assert!(inventory
        .issues
        .iter()
        .any(|i| i.code == "tunnel_profiles_truncated"));
    assert!(inventory.entries.len() <= MAX_INVENTORY_ENTRIES);
    assert!(!serde_json::to_string(&inventory)
        .unwrap()
        .contains("PRIVATE_CANARY"));
}

#[test]
fn foreign_runner_binding_never_exposes_a_registry_open_target() {
    let (temp, _) = fixture(false, true);
    let root = temp.path().join("environment");
    let registry = temp.path().join("foreign-registry");
    std::fs::create_dir(&registry).unwrap();
    for (client, server) in [
        ("foreign-runner", "http://127.0.0.1:8080"),
        ("runner-original", "http://127.0.0.1:9999"),
    ] {
        private_file(&root.join("runner.toml"), format!("client_id='{client}'\nowner='alice'\nserver_url='{server}'\nproject_registry_dir='{}'\ntoken='wc_agent_CANARY'\n", registry.display()).as_bytes());
        let inventory = inspect_environment_paths(&root, None);
        let observed = entry(&inventory, "runner.registry");
        assert_eq!(observed.status, PathStatus::Unconfirmed);
        assert!(observed.canonical_path.is_none() && observed.directory_to_open.is_none());
        let mut legacy = empty_environment_inventory();
        append_runner_configuration_paths(
            &mut legacy,
            &root.join("runner.toml"),
            Some("runner-original"),
            Some("http://127.0.0.1:8080"),
        );
        assert!(entry(&legacy, "runner.registry")
            .directory_to_open
            .is_none());
    }
}

#[test]
fn relative_runner_registry_requires_the_known_saved_service_directory() {
    let (temp, _) = fixture(false, true);
    let root = temp.path().join("environment");
    std::fs::create_dir(root.join("relative-registry")).unwrap();
    private_file(&root.join("runner.toml"), b"client_id='runner-original'\nowner='alice'\nserver_url='http://127.0.0.1:8080'\nproject_registry_dir='relative-registry'\ntoken='wc_agent_CANARY'\n");
    let inventory = inspect_environment_paths(&root, None);
    assert_eq!(
        entry(&inventory, "runner.registry").status,
        PathStatus::Present
    );
    assert_eq!(
        entry(&inventory, "runner.registry").canonical_path.as_ref(),
        Some(&root.join("relative-registry"))
    );
    let legacy =
        inspect_runner_locations(&root.join("runner.toml"), &root.join("project-registry"));
    assert_eq!(legacy.registry.status, PathStatus::Unconfirmed);
    assert!(legacy.registry.canonical_path.is_none());
}

#[test]
fn record_project_references_are_bounded_and_remote_paths_are_display_only() {
    for runner in [true, false] {
        let (temp, mut record) = fixture(false, runner);
        let root = temp.path().join("environment");
        record.projects = (0..100)
            .map(|i| crate::ProjectRecord {
                id: format!("project-{i}"),
                path: temp.path().join(format!("project-{i}")),
            })
            .collect();
        private_file(
            &root.join("environment.json"),
            &serde_json::to_vec(&record).unwrap(),
        );
        let inventory = inspect_environment_paths(&root, None);
        assert_eq!(
            inventory
                .entries
                .iter()
                .filter(|e| e.component == "project")
                .count(),
            16
        );
        assert!(inventory
            .issues
            .iter()
            .any(|i| i.code == "project_references_truncated"));
        if !runner {
            assert_eq!(
                entry(&inventory, "project.project-0.reference").status,
                PathStatus::Remote
            );
            assert!(entry(&inventory, "project.project-0.reference")
                .directory_to_open
                .is_none());
        }
    }
}

#[test]
fn unavailable_configs_do_not_claim_uninspected_dependent_paths_are_missing() {
    let temp = crate::test_tempdir().unwrap();
    let config = temp.path().join("missing.env");
    assert_eq!(
        inspect_server_locations(&config, Some(temp.path()))
            .data
            .status,
        PathStatus::Unconfirmed
    );
    assert_eq!(
        inspect_runner_locations(&config, &temp.path().join("registry"))
            .registry
            .status,
        PathStatus::Unconfirmed
    );
}

#[test]
fn adapter_additions_share_manifest_bounds_and_safe_build_strips_private_metadata() {
    let mut inventory = empty_environment_inventory();
    let info = webcodex_core::desktop_runtime_contract::MachineBuildInfo {
        schema_version: 1,
        binary: "unknown-PRIVATE_CANARY".into(),
        version: "1.2.3-private+PRIVATE_CANARY".into(),
        git_commit: Some("PRIVATE_CANARY".into()),
        git_dirty: Some(true),
        built_at: Some("PRIVATE_CANARY".into()),
        target: "PRIVATE_CANARY".into(),
        architecture: "PRIVATE_CANARY".into(),
        desktop_runtime_contract: webcodex_core::desktop_runtime_contract::DESKTOP_RUNTIME_CONTRACT,
        agent_protocol_generation: None,
        environment_data_format: Some(1),
    };
    for index in 0..100 {
        inventory.entries.push(reference_entry(
            &format!("test.{index}"),
            "desktop",
            "desktop_activity",
            "derived",
            PathKind::InMemory,
            SafetyCategory::Log,
            PathStatus::NotApplicable,
        ));
        inventory.builds.push(BuildObservation {
            source: BuildSource::PreviouslyVerified,
            build: safe_build(&info),
        });
    }
    recompute_revision(&mut inventory);
    assert_eq!(inventory.entries.len(), MAX_INVENTORY_ENTRIES);
    assert_eq!(inventory.builds.len(), 8);
    assert!(inventory
        .issues
        .iter()
        .any(|i| i.code == "inventory_truncated"));
    let manifest = serde_json::to_string(&build_backup_manifest(&inventory)).unwrap();
    assert!(!manifest.contains("PRIVATE_CANARY"));
    assert!(manifest.len() < 1024 * 1024);
}

#[cfg(target_os = "linux")]
#[test]
fn service_log_references_use_saved_exact_units_and_scope_without_probing() {
    let (temp, _) = fixture(true, true);
    let inventory = inspect_environment_paths(&temp.path().join("environment"), None);
    let server = entry(&inventory, "server.lifecycle_log")
        .log_source
        .as_ref()
        .unwrap();
    assert_eq!(server.kind, LogSourceKind::SystemdJournal);
    assert_eq!(server.unit_name.as_deref(), Some("webcodex.service"));
    assert_eq!(
        server.service_scope,
        Some(crate::service::ServiceScope::User)
    );
    let runner = entry(&inventory, "runner.lifecycle_log")
        .log_source
        .as_ref()
        .unwrap();
    assert_eq!(
        runner.unit_name.as_deref(),
        Some("webcodex-runner-1000.service")
    );
    assert!(!temp.path().join("environment/setup.lock").exists());
}

#[test]
fn serialized_inventory_and_manifest_obey_the_byte_bound_with_escaped_paths() {
    let mut inventory = empty_environment_inventory();
    let long_path = PathBuf::from(format!("/{}", "\\".repeat(4094)));
    for index in 0..MAX_INVENTORY_ENTRIES {
        let mut row = reference_entry(
            &format!("test.{index}"),
            "desktop",
            "known_location",
            "derived",
            PathKind::Directory,
            SafetyCategory::Metadata,
            PathStatus::Unconfirmed,
        );
        row.configured_path = Some(long_path.clone());
        row.canonical_path = Some(long_path.clone());
        row.directory_to_open = Some(long_path.clone());
        inventory.entries.push(row);
    }
    recompute_revision(&mut inventory);
    assert!(inventory.entries.len() < MAX_INVENTORY_ENTRIES);
    assert!(inventory
        .issues
        .iter()
        .any(|i| i.code == "inventory_truncated"));
    assert!(
        serde_json::to_vec_pretty(&build_backup_manifest(&inventory))
            .unwrap()
            .len()
            < 1024 * 1024
    );
}

#[cfg(unix)]
#[test]
fn invalid_unicode_paths_are_unknown_without_unserializable_path_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let path = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/invalid-\xff".to_vec()));
    let row = local_path_entry(
        "test.path",
        "desktop",
        "known_location",
        "derived",
        &path,
        PathKind::Directory,
        SafetyCategory::Metadata,
    );
    assert_eq!(row.status, PathStatus::Invalid);
    assert!(row.configured_path.is_none());
    assert!(serde_json::to_vec(&row).is_ok());
}

#[test]
fn absent_runner_registry_key_uses_the_startup_default_not_the_environment_root() {
    let (temp, _) = fixture(false, true);
    let root = temp.path().join("environment");
    private_file(&root.join("runner.toml"), b"client_id='runner-original'\nowner='alice'\nserver_url='http://127.0.0.1:8080'\ntoken='wc_agent_CANARY'\n");
    let inventory = inspect_environment_paths(&root, None);
    let expected = webcodex_runner_config::paths::select_project_registry_dir(
        &webcodex_runner_config::paths::default_client_config_base_dir().unwrap(),
    )
    .unwrap();
    assert_eq!(
        entry(&inventory, "runner.registry")
            .configured_path
            .as_ref(),
        Some(&expected)
    );
    assert_ne!(expected, root.join("project-registry"));
    assert_eq!(entry(&inventory, "runner.registry").source, "derived");
    let unavailable = inspect_runner_locations(&root.join("runner.toml"), Path::new(""));
    assert_eq!(unavailable.registry.status, PathStatus::Unconfirmed);
    assert!(unavailable.registry.configured_path.is_none());
}

#[test]
fn unknown_server_working_directory_preserves_relative_keys_without_inventing_locations() {
    let temp = crate::test_tempdir().unwrap();
    let config = temp.path().join("server/webcodex.env");
    private_file(&config, b"WEBCODEX_DATA=relative-data\n");
    let mut inventory = empty_environment_inventory();
    append_server_configuration_paths(&mut inventory, &config, None);
    let data = entry(&inventory, "server.data");
    assert_eq!(data.configured_path, Some(PathBuf::from("relative-data")));
    assert_eq!(data.status, PathStatus::Unconfirmed);
    assert!(data.canonical_path.is_none() && data.directory_to_open.is_none());
    assert!(!inventory
        .entries
        .iter()
        .any(|row| row.id == "server.database"));
    let mut managed = empty_environment_inventory();
    append_server_configuration_paths(&mut managed, &config, config.parent());
    assert_eq!(
        entry(&managed, "server.database").status,
        PathStatus::Missing
    );
    assert_eq!(
        entry(&managed, "server.database").configured_path.as_ref(),
        Some(&config.parent().unwrap().join("relative-data/webcodex.db"))
    );
    let absolute = temp.path().join("absolute-data");
    std::fs::create_dir(&absolute).unwrap();
    private_file(
        &config,
        format!("WEBCODEX_DATA='{}'\n", absolute.display()).as_bytes(),
    );
    let known = inspect_server_locations(&config, None);
    assert_eq!(known.data.status, PathStatus::Present);
    assert_eq!(known.data.canonical_path.as_ref(), Some(&absolute));
}
