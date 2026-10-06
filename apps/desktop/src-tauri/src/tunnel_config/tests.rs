use super::*;
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let temp = std::env::temp_dir();
        #[cfg(unix)]
        let temp = fs::canonicalize(temp).unwrap();
        let root = temp.join(format!("webcodex-tunnel-profiles-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("secrets")).unwrap();
        Self(root)
    }
    fn path(&self) -> PathBuf {
        self.0.join("secrets/tunnel-config.json")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn create(name: &str, tunnel_id: &str, key: &str) -> TunnelProfileRequest {
    TunnelProfileRequest {
        id: None,
        name: name.into(),
        tunnel_id: tunnel_id.into(),
        api_key: Some(key.into()),
        autostart: true,
        host_mode: webcodex_environment::TunnelHostMode::Standalone,
        expected_revision: None,
    }
}
fn edit(
    id: TunnelProfileId,
    name: &str,
    tunnel_id: &str,
    key: Option<&str>,
) -> TunnelProfileRequest {
    TunnelProfileRequest {
        id: Some(id.to_string()),
        name: name.into(),
        tunnel_id: tunnel_id.into(),
        api_key: key.map(str::to_owned),
        autostart: true,
        host_mode: webcodex_environment::TunnelHostMode::Standalone,
        expected_revision: None,
    }
}
fn command_key(store: &TunnelConfig, id: TunnelProfileId) -> String {
    let mut command = Command::new("unused");
    command.env("CONTROL_PLANE_API_KEY", "unrelated-environment-fixture");
    store.apply_profile_to_command(id, &mut command).unwrap();
    command
        .get_envs()
        .find(|(k, _)| *k == "CONTROL_PLANE_API_KEY")
        .unwrap()
        .1
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn singleton_migrates_atomically_to_default_and_preserves_autostart_intent() {
    for autostart in [false, true] {
        let fixture = Fixture::new();
        let legacy = br#"{"tunnel_id":"tunnel_existing","api_key":"legacy-fixture-secret"}"#;
        fs::write(fixture.path(), legacy).unwrap();
        let config = TunnelConfig::load(&fixture.path(), autostart);
        assert!(!config.invalid);
        let profiles = config.profiles();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, TunnelProfileId::DEFAULT.to_string());
        assert_eq!(profiles[0].name, "ChatGPT");
        assert_eq!(profiles[0].autostart, autostart);
        assert_eq!(profiles[0].enabled, autostart);
        assert_eq!(
            command_key(&config, TunnelProfileId::DEFAULT),
            "legacy-fixture-secret"
        );
        let disk: StoredProfiles =
            serde_json::from_slice(&fs::read(fixture.path()).unwrap()).unwrap();
        assert_eq!(disk.schema_version, SCHEMA_VERSION);
        let after = fs::read(fixture.path()).unwrap();
        let reloaded = TunnelConfig::load(&fixture.path(), !autostart);
        assert_eq!(reloaded.profiles(), profiles, "migration runs only once");
        assert_eq!(fs::read(fixture.path()).unwrap(), after);
        assert_eq!(
            fs::read_dir(fixture.0.join("secrets")).unwrap().count(),
            1,
            "no retired-secret backup"
        );
    }
}

#[test]
fn interrupted_migration_keeps_the_original_valid_file() {
    let fixture = Fixture::new();
    let legacy = br#"{"tunnel_id":"tunnel_original","api_key":"original-fixture-secret"}"#;
    fs::write(fixture.path(), legacy).unwrap();
    let failed = TunnelConfig::load_with_writer(&fixture.path(), true, |path, bytes, _| {
        crate::state::write_atomic_file_with_hook(path, bytes, |_| {
            Err(io::ErrorKind::Interrupted.into())
        })
        .map_err(|_| invalid())
    });
    assert!(failed.invalid);
    assert!(failed.profiles().is_empty());
    assert_eq!(fs::read(fixture.path()).unwrap(), legacy);
    assert_eq!(fs::read_dir(fixture.0.join("secrets")).unwrap().count(), 1);
    let recovered = TunnelConfig::load(&fixture.path(), true);
    assert_eq!(
        command_key(&recovered, TunnelProfileId::DEFAULT),
        "original-fixture-secret"
    );
}

#[test]
fn explicit_null_migrates_to_only_the_legacy_environment_profile() {
    let fixture = Fixture::new();
    fs::write(fixture.path(), b"null").unwrap();
    let config = TunnelConfig::load(&fixture.path(), false);
    assert!(!config.invalid);
    assert_eq!(config.stored.profiles.len(), 1);
    assert_eq!(config.stored.profiles[0].id, TunnelProfileId::DEFAULT);
    assert!(config.stored.profiles[0].credentials.is_none());
    assert!(!config.stored.profiles[0].autostart);
}

#[test]
fn multiple_profiles_persist_without_projecting_secrets_and_update_only_the_selected_pair() {
    let fixture = Fixture::new();
    let mut config = TunnelConfig::default();
    let a = config
        .update_profile(
            &fixture.path(),
            create("Personal", "tunnel_a", "secret-a-fixture"),
        )
        .unwrap();
    let b = config
        .update_profile(
            &fixture.path(),
            create("Work", "tunnel_b", "secret-b-fixture"),
        )
        .unwrap();
    let c = config
        .update_profile(
            &fixture.path(),
            create("Account 3", "tunnel_c", "secret-c-fixture"),
        )
        .unwrap();
    assert_ne!(a, b);
    assert_ne!(b, c);
    let mut reloaded = TunnelConfig::load(&fixture.path(), false);
    assert_eq!(reloaded.profiles().len(), 3);
    let serialized = serde_json::to_string(&reloaded.profiles()).unwrap();
    for secret in ["secret-a-fixture", "secret-b-fixture", "secret-c-fixture"] {
        assert!(!serialized.contains(secret));
    }
    reloaded
        .update_profile(
            &fixture.path(),
            edit(a, "ChatGPT Personal", "tunnel_a_new", None),
        )
        .unwrap();
    assert_eq!(command_key(&reloaded, a), "secret-a-fixture");
    assert_eq!(command_key(&reloaded, b), "secret-b-fixture");
    reloaded
        .update_profile(
            &fixture.path(),
            edit(
                a,
                "ChatGPT Personal",
                "tunnel_a_new",
                Some("replacement-fixture"),
            ),
        )
        .unwrap();
    assert_eq!(command_key(&reloaded, a), "replacement-fixture");
    assert_eq!(command_key(&reloaded, b), "secret-b-fixture");
    assert!(!fs::read_to_string(fixture.path())
        .unwrap()
        .contains("secret-a-fixture"));
    reloaded.remove(&fixture.path(), a).unwrap();
    assert_eq!(reloaded.profiles().len(), 2);
    assert_eq!(command_key(&reloaded, b), "secret-b-fixture");
    assert!(!fs::read_to_string(fixture.path())
        .unwrap()
        .contains("replacement-fixture"));
}

#[test]
fn deleting_default_cannot_resurrect_environment_fallback_after_restart() {
    let fixture = Fixture::new();
    let mut config = TunnelConfig::default();
    config
        .update(
            &fixture.path(),
            TunnelConfigRequest::Save {
                tunnel_id: "tunnel_default".into(),
                api_key: Some("fixture-secret".into()),
            },
        )
        .unwrap();
    config
        .remove(&fixture.path(), TunnelProfileId::DEFAULT)
        .unwrap();
    let reloaded = TunnelConfig::load(&fixture.path(), true);
    assert!(reloaded.stored.profiles.is_empty());
    assert!(!reloaded.snapshot().is_configured());
    assert!(reloaded
        .apply_to_command(&mut Command::new("unused"))
        .is_err());
}

#[test]
fn explicit_first_run_connect_remembers_default_autostart() {
    let fixture = Fixture::new();
    fs::write(
        fixture.path(),
        br#"{"tunnel_id":"tunnel_existing","api_key":"legacy-fixture-secret"}"#,
    )
    .unwrap();
    let mut config = TunnelConfig::load(&fixture.path(), false);
    assert!(!config.profiles()[0].autostart);
    config.enable_default_onboarding(&fixture.path()).unwrap();
    let restored = TunnelConfig::load(&fixture.path(), false);
    assert!(restored.profiles()[0].enabled);
    assert!(restored.profiles()[0].autostart);
}

#[test]
fn stopped_and_non_autostart_profiles_keep_their_desired_state() {
    let fixture = Fixture::new();
    let mut config = TunnelConfig::default();
    let a = config
        .update_profile(&fixture.path(), create("Personal", "tunnel_a", "fixture-a"))
        .unwrap();
    let mut request = create("Work", "tunnel_b", "fixture-b");
    request.autostart = false;
    let b = config.update_profile(&fixture.path(), request).unwrap();
    config.set_enabled(&fixture.path(), a, false).unwrap();
    let reloaded = TunnelConfig::load(&fixture.path(), true);
    assert!(
        !reloaded
            .profiles()
            .iter()
            .find(|p| p.id == a.to_string())
            .unwrap()
            .enabled
    );
    assert!(
        !reloaded
            .profiles()
            .iter()
            .find(|p| p.id == b.to_string())
            .unwrap()
            .autostart
    );
}

#[test]
fn duplicate_ids_and_tunnel_ids_are_rejected_without_retiring_a_valid_pair() {
    let fixture = Fixture::new();
    let mut config = TunnelConfig::default();
    let a = config
        .update_profile(&fixture.path(), create("Personal", "tunnel_a", "fixture-a"))
        .unwrap();
    let original = fs::read(fixture.path()).unwrap();
    assert!(config
        .update_profile(&fixture.path(), create("Work", "tunnel_a", "fixture-b"))
        .is_err());
    assert_eq!(fs::read(fixture.path()).unwrap(), original);
    let mut request = edit(a, "Changed", "tunnel_a", None);
    request.expected_revision = Some(999);
    assert_eq!(
        config
            .update_profile(&fixture.path(), request)
            .unwrap_err()
            .code,
        "tunnel_profile_changed"
    );
    let mut bad = config.stored.clone();
    bad.profiles.push(bad.profiles[0].clone());
    fs::write(fixture.path(), serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(TunnelConfig::load(&fixture.path(), true).invalid);
}

#[test]
fn malformed_or_unreadable_saved_config_fails_closed_without_secret_errors() {
    let fixture = Fixture::new();
    for data in [
        b"not-json-with-fixture-secret".to_vec(),
        vec![b' '; MAX_CONFIG_BYTES as usize + 1],
    ] {
        fs::write(fixture.path(), &data).unwrap();
        let mut config = TunnelConfig::load(&fixture.path(), true);
        assert!(config.invalid);
        assert_eq!(config.snapshot().source, TunnelConfigSource::Invalid);
        let error = config
            .update_profile(
                &fixture.path(),
                create("New", "tunnel_new", "new-fixture-secret"),
            )
            .unwrap_err();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("fixture-secret"));
        assert_eq!(fs::read(fixture.path()).unwrap(), data);
    }
}

#[test]
fn invalid_input_and_failed_writes_leave_both_memory_and_disk_unchanged() {
    let fixture = Fixture::new();
    let mut config = TunnelConfig::default();
    let a = config
        .update_profile(
            &fixture.path(),
            create("Personal", "tunnel_a", "old-fixture"),
        )
        .unwrap();
    let before = fs::read(fixture.path()).unwrap();
    for request in [
        edit(a, "", "tunnel_a", None),
        edit(a, "Name", "invalid/id", None),
        edit(a, "Name", "tunnel_a", Some("invalid\nsecret")),
    ] {
        assert!(config.update_profile(&fixture.path(), request).is_err());
    }
    let blocked = fixture.0.join("directory");
    fs::create_dir(&blocked).unwrap();
    assert!(config
        .update_profile(
            &blocked,
            edit(a, "Changed", "tunnel_changed", Some("new-fixture"))
        )
        .is_err());
    assert_eq!(command_key(&config, a), "old-fixture");
    assert_eq!(fs::read(fixture.path()).unwrap(), before);
}

#[test]
fn stale_desktop_writer_cannot_overwrite_newer_profiles() {
    let fixture = Fixture::new();
    let mut first = TunnelConfig::default();
    let a = first
        .update_profile(&fixture.path(), create("A", "tunnel_a", "fixture-a"))
        .unwrap();
    let mut stale = TunnelConfig::load(&fixture.path(), true);
    first
        .update_profile(&fixture.path(), create("B", "tunnel_b", "fixture-b"))
        .unwrap();
    assert!(stale
        .update_profile(
            &fixture.path(),
            edit(a, "Stale", "tunnel_a", Some("stale-fixture"))
        )
        .is_err());
    assert_eq!(
        TunnelConfig::load(&fixture.path(), true).profiles().len(),
        2
    );
    assert_eq!(command_key(&first, a), "fixture-a");
}

#[cfg(unix)]
#[test]
fn private_mode_and_symlink_rejection_survive_migration() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fixture = Fixture::new();
    fs::write(
        fixture.path(),
        br#"{"tunnel_id":"tunnel_a","api_key":"fixture-secret"}"#,
    )
    .unwrap();
    assert!(!TunnelConfig::load(&fixture.path(), true).invalid);
    assert_eq!(
        fs::metadata(fixture.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let link = fixture.0.join("link.json");
    symlink(fixture.path(), &link).unwrap();
    assert!(TunnelConfig::load(&link, true).invalid);
}

#[tokio::test]
async fn desktop_restart_and_activity_never_project_stored_keys() {
    let fixture = Fixture::new();
    let app = crate::state::AppState::new(fixture.0.clone(), fixture.0.join("resources")).unwrap();
    app.update_tunnel_config(TunnelConfigRequest::Save {
        tunnel_id: "tunnel_persisted".into(),
        api_key: Some("private-fixture-key".into()),
    })
    .await
    .unwrap();
    assert!(app.get_state().openai_tunnel_configured);
    assert!(!serde_json::to_string(&app.get_state())
        .unwrap()
        .contains("private-fixture-key"));
    assert!(!serde_json::to_string(&app.activity())
        .unwrap()
        .contains("private-fixture-key"));
    let restarted =
        crate::state::AppState::new(fixture.0.clone(), fixture.0.join("resources")).unwrap();
    assert_eq!(
        restarted
            .get_state()
            .openai_tunnel_config
            .saved_tunnel_id
            .as_deref(),
        Some("tunnel_persisted")
    );
}

#[cfg(unix)]
fn write_environment_profile_fixture(
    root: &Path,
    profiles: &[(&str, &str, &str)],
) -> webcodex_environment::EnvironmentStore {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

    fn private_write(path: &Path, bytes: &[u8]) {
        let parent = path.parent().unwrap();
        let mut directories = fs::DirBuilder::new();
        directories.recursive(true).mode(0o700);
        directories.create(parent).unwrap();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    let store = webcodex_environment::EnvironmentStore::open(root.to_path_buf()).unwrap();
    let records = profiles
        .iter()
        .map(|(profile_id, _, _)| webcodex_environment::TunnelRecord {
            profile_id: (*profile_id).to_owned(),
            name: format!("Profile {profile_id}"),
            host_mode: webcodex_environment::TunnelHostMode::Standalone,
            autostart: true,
            revision: 1,
            runtime_revision: 1,
            installed: false,
            started: false,
        })
        .collect::<Vec<_>>();
    private_write(
        &root.join("tunnel.json"),
        &serde_json::to_vec_pretty(&records).unwrap(),
    );
    for (profile_id, tunnel_id, api_key) in profiles {
        private_write(
            &root
                .join("server/tunnels")
                .join(profile_id)
                .join("webcodex.env"),
            format!(
                "WEBCODEX_ADDR=127.0.0.1:62645\nWEBCODEX_TOKEN=local-fixture\nCONTROL_PLANE_TUNNEL_ID={tunnel_id}\nCONTROL_PLANE_API_KEY={api_key}\nWEBCODEX_TUNNEL_PROFILE_ID={profile_id}\n"
            )
            .as_bytes(),
        );
    }
    store
}

#[cfg(unix)]
#[test]
fn persistent_environment_accepts_cli_profiles_when_no_desktop_catalog_exists() {
    let fixture = Fixture::new();
    let local = TunnelConfig::load(&fixture.path(), false);
    assert!(!local.persisted_file_present());
    let store = write_environment_profile_fixture(
        &fixture.0.join("environment-no-local"),
        &[("work", "tunnel_work", "work-private-key")],
    );

    local.ensure_persistent_catalog_compatible(&store).unwrap();
    let profiles = webcodex_environment::tunnel_profile_snapshots(&store).unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].profile_id, "work");
    assert_eq!(profiles[0].tunnel_id, "tunnel_work");
    let public = serde_json::to_string(&profiles).unwrap();
    assert!(!public.contains("work-private-key"));
    assert!(!fixture.path().exists());
}

#[cfg(unix)]
#[test]
fn persistent_environment_reconciles_exact_legacy_claims_and_fails_closed_on_conflict() {
    let fixture = Fixture::new();
    let mut local = TunnelConfig::load(&fixture.path(), false);
    local
        .update_profile(
            &fixture.path(),
            TunnelProfileRequest {
                id: Some(TunnelProfileId::DEFAULT.to_string()),
                name: "ChatGPT".into(),
                tunnel_id: "tunnel_default".into(),
                api_key: Some("default-private-key".into()),
                autostart: true,
                host_mode: webcodex_environment::TunnelHostMode::Standalone,
                expected_revision: None,
            },
        )
        .unwrap();
    assert!(local.persisted_file_present());
    let store = write_environment_profile_fixture(
        &fixture.0.join("environment-exact"),
        &[
            ("default", "tunnel_default", "default-private-key"),
            ("work", "tunnel_work", "work-private-key"),
        ],
    );
    let local_before = fs::read(fixture.path()).unwrap();
    let catalog_before = fs::read(store.root().join("tunnel.json")).unwrap();
    local.ensure_persistent_catalog_compatible(&store).unwrap();
    assert_eq!(fs::read(fixture.path()).unwrap(), local_before);
    assert_eq!(
        fs::read(store.root().join("tunnel.json")).unwrap(),
        catalog_before
    );

    let binding = store.root().join("server/tunnels/default/webcodex.env");
    let rotated = fs::read_to_string(&binding)
        .unwrap()
        .replace("default-private-key", "rotated-private-key");
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = fs::OpenOptions::new()
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&binding)
        .unwrap();
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .unwrap();
    file.write_all(rotated.as_bytes()).unwrap();
    file.sync_all().unwrap();
    let rotated_before = fs::read(&binding).unwrap();

    local.ensure_persistent_catalog_compatible(&store).unwrap();
    assert_eq!(fs::read(fixture.path()).unwrap(), local_before);
    assert_eq!(fs::read(&binding).unwrap(), rotated_before);

    let conflicting = fs::read_to_string(&binding)
        .unwrap()
        .replace("tunnel_default", "tunnel_different");
    let mut file = fs::OpenOptions::new()
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&binding)
        .unwrap();
    file.write_all(conflicting.as_bytes()).unwrap();
    file.sync_all().unwrap();
    let binding_before = fs::read(&binding).unwrap();

    let error = local
        .ensure_persistent_catalog_compatible(&store)
        .unwrap_err();
    assert_eq!(error.code, "tunnel_catalog_conflict");
    assert_eq!(fs::read(fixture.path()).unwrap(), local_before);
    assert_eq!(
        fs::read(store.root().join("tunnel.json")).unwrap(),
        catalog_before
    );
    assert_eq!(fs::read(binding).unwrap(), binding_before);
    let encoded = serde_json::to_string(&error).unwrap();
    assert!(!encoded.contains("default-private-key"));
    assert!(!encoded.contains("rotated-private-key"));
}

#[cfg(unix)]
#[test]
fn persistent_environment_rejects_a_missing_legacy_profile_without_deleting_either_catalog() {
    let fixture = Fixture::new();
    let mut local = TunnelConfig::default();
    local
        .update_profile(
            &fixture.path(),
            create("Legacy", "tunnel_legacy", "legacy-private-key"),
        )
        .unwrap();
    let local_before = fs::read(fixture.path()).unwrap();
    let store = write_environment_profile_fixture(
        &fixture.0.join("environment-missing"),
        &[("work", "tunnel_work", "work-private-key")],
    );
    let catalog_before = fs::read(store.root().join("tunnel.json")).unwrap();

    assert_eq!(
        local
            .ensure_persistent_catalog_compatible(&store)
            .unwrap_err()
            .code,
        "tunnel_catalog_conflict"
    );
    assert_eq!(fs::read(fixture.path()).unwrap(), local_before);
    assert_eq!(
        fs::read(store.root().join("tunnel.json")).unwrap(),
        catalog_before
    );
}
