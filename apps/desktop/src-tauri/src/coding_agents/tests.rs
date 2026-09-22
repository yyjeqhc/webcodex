use super::*;

pub(crate) struct Scratch(pub PathBuf);
impl Scratch {
    pub(crate) fn new() -> Self {
        let root = std::env::temp_dir().join(format!("webcodex-acp-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn request(id: &str, revision: u64) -> CodingAgentUpdate {
    CodingAgentUpdate {
        target: crate::webcodex::settings::SettingsTarget {
            config_path: "unused-runner.toml".into(),
            client_id: "mini".into(),
            server_url: "http://127.0.0.1:1".into(),
        },
        expected_revision: revision,
        previous_id: None,
        profile: CodingAgentProfile {
            provider_id: id.into(),
            name: "Pi Agent".into(),
            executable: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            args: vec!["--acp".into()],
            enabled: true,
            env_from_env: BTreeMap::from([("OPENAI_API_KEY".into(), "SUB2API_API_KEY".into())]),
            allowed_config_options: vec!["mode".into()],
        },
        global_settings: None,
    }
}

#[test]
fn coding_agents_add_edit_remove_and_restart_keep_desired_state_and_tombstones() {
    let dir = Scratch::new();
    let mut store = CodingAgentStore::load(&dir.0);
    store.stage_update(request("pi", 0)).unwrap();
    assert!(
        !dir.0.join("coding-agents.json").exists(),
        "staging must not persist"
    );
    store.commit().unwrap();
    let owner = store.owner_id().to_owned();
    let mut restarted = CodingAgentStore::load(&dir.0);
    assert_eq!(restarted.owner_id(), owner);
    assert!(restarted.snapshot(None).restart_required);
    assert!(!restarted.snapshot(Some(1)).restart_required);
    let mut edit = request("pi", 1);
    edit.previous_id = Some("pi".into());
    edit.profile.name = "Pi Local".into();
    edit.profile.enabled = false;
    restarted.stage_update(edit).unwrap();
    restarted.commit().unwrap();
    let mut restarted = CodingAgentStore::load(&dir.0);
    assert_eq!(restarted.profiles().unwrap()[0].name, "Pi Local");
    assert!(!restarted.profiles().unwrap()[0].enabled);
    restarted.stage_remove("pi", 2).unwrap();
    restarted.commit().unwrap();
    let removed = CodingAgentStore::load(&dir.0);
    assert!(removed.profiles().unwrap().is_empty());
    assert!(removed.managed_ids().contains("pi"));
    assert_eq!(removed.revision(), 3);
}

#[test]
fn coding_agents_rename_retains_old_id_tombstone() {
    let dir = Scratch::new();
    let mut store = CodingAgentStore::load(&dir.0);
    store.stage_update(request("old", 0)).unwrap();
    store.commit().unwrap();
    let mut update = request("new", 1);
    update.previous_id = Some("old".into());
    store.stage_update(update).unwrap();
    store.commit().unwrap();
    assert_eq!(
        store.managed_ids(),
        &BTreeSet::from(["old".into(), "new".into()])
    );
    assert_eq!(store.profiles().unwrap()[0].provider_id, "new");
}

#[test]
fn coding_agents_stale_revision_and_external_writer_fail_closed() {
    let dir = Scratch::new();
    let mut first = CodingAgentStore::load(&dir.0);
    let mut second = CodingAgentStore::load(&dir.0);
    first.stage_update(request("pi", 0)).unwrap();
    second.stage_update(request("other", 0)).unwrap();
    first.commit().unwrap();
    assert!(second.commit().is_err());
    assert!(first.stage_remove("pi", 0).is_err());
    assert_eq!(
        CodingAgentStore::load(&dir.0).profiles().unwrap()[0].provider_id,
        "pi"
    );
}

#[test]
fn coding_agents_invalid_manifest_never_becomes_empty_writable_state() {
    let dir = Scratch::new();
    std::fs::write(dir.0.join("coding-agents.json"), b"invalid-private-data").unwrap();
    let mut store = CodingAgentStore::load(&dir.0);
    assert!(store.snapshot(None).config_error);
    assert!(store.profiles().is_err());
    assert!(store.stage_update(request("pi", 0)).is_err());
    assert!(!serde_json::to_string(&store.snapshot(None))
        .unwrap()
        .contains("invalid-private-data"));
}

#[test]
fn coding_agents_public_state_contains_names_not_environment_values_or_owner_identity() {
    let dir = Scratch::new();
    let mut store = CodingAgentStore::load(&dir.0);
    store.stage_update(request("pi", 0)).unwrap();
    store.commit().unwrap();
    let value = serde_json::to_value(store.snapshot(None)).unwrap();
    assert_eq!(
        value["profiles"][0]["env_from_env"]["OPENAI_API_KEY"],
        "SUB2API_API_KEY"
    );
    assert!(!value.to_string().contains(store.owner_id()));
    assert!(value["profiles"][0].get("env").is_none());
    let mut profile = serde_json::to_value(&store.profiles().unwrap()[0]).unwrap();
    profile["env"] = serde_json::json!({"OPENAI_API_KEY":"not-an-accepted-value"});
    assert!(serde_json::from_value::<CodingAgentProfile>(profile).is_err());
    for source in [
        "raw-value-with-hyphens",
        "a=b",
        "WEBCODEX_SHARED_KEY",
        "Authorization",
        "https://api.example",
    ] {
        let mut update = request("test", 1);
        update
            .profile
            .env_from_env
            .insert("OPENAI_API_KEY".into(), source.into());
        assert!(store.clone().stage_update(update).is_err());
    }
}

#[test]
fn coding_agents_capacity_and_advanced_settings_are_bounded() {
    let dir = Scratch::new();
    let mut store = CodingAgentStore::load(&dir.0);
    for (runs, timeout) in [(0, 5), (9, 5), (1, 0), (1, 61)] {
        let mut update = request("pi", 0);
        update.global_settings = Some(AcpGlobalSettings {
            max_concurrent_runs: runs,
            permission_timeout_secs: timeout,
        });
        assert!(store.clone().stage_update(update).is_err());
    }
    for index in 0..CODING_AGENT_MAX_PROVIDERS {
        store
            .stage_update(request(&format!("p{index}"), index as u64))
            .unwrap();
        store.commit().unwrap();
    }
    assert!(store
        .stage_update(request("overflow", CODING_AGENT_MAX_PROVIDERS as u64))
        .is_err());
}
