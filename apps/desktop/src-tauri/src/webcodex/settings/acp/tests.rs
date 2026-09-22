use super::*;
use crate::coding_agents::tests::{request, Scratch};

fn runtime(root: &Path, slot: &str, extra: &str) -> StoredRuntime {
    let path = root.join(format!("runner-{slot}.toml"));
    std::fs::write(&path, format!(
        "# retain identity and policy comments\nserver_url = \"http://127.0.0.1:1\"\nclient_id = \"{slot}\"\ntoken = \"test-token-placeholder\"\nproject_registry_dir = \"registry\"\n[policy]\nallow_shell = true # operator policy\n{extra}"
    )).unwrap();
    StoredRuntime {
        server_url: "http://127.0.0.1:1".into(),
        server_env_file: None,
        runner_config: Some(path),
        user_token_file: None,
        runner_client_id: Some(slot.into()),
        project_id: None,
        runtime_project_id: None,
    }
}

fn operator() -> &'static str {
    "\n[acp]\nmax_concurrent_runs = 2 # keep global setting\npermission_timeout_secs = 9\n[[acp.agents]]\nid = \"operator\"\nname = \"Operator Agent\" # keep provider comment\nexecutable = \"/operator/agent\"\nargs = [\"--operator\"]\n"
}

#[test]
fn acp_reconcile_preserves_operator_configuration_and_applies_only_after_save() {
    let dir = Scratch::new();
    let runtime = runtime(&dir.0, "A", operator());
    let path = runtime.runner_config.as_ref().unwrap();
    let original = std::fs::read_to_string(path).unwrap();
    let mut store = CodingAgentStore::load(&dir.0);
    store.stage_update(request("pi", 0)).unwrap();
    reconcile_acp(&runtime, &store, true).unwrap();
    store.commit().unwrap();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        original,
        "Save must not modify live TOML"
    );
    reconcile_acp(&runtime, &store, false).unwrap();
    let current = std::fs::read_to_string(path).unwrap();
    for preserved in [
        "# retain identity",
        "token = \"test-token-placeholder\"",
        "client_id = \"A\"",
        "server_url = \"http://127.0.0.1:1\"",
        "# operator policy",
        "# keep global setting",
        "# keep provider comment",
        "args = [\"--operator\"]",
    ] {
        assert!(current.contains(preserved), "missing {preserved}");
    }
    assert!(current.contains("SUB2API_API_KEY"));
    assert!(current.contains("desktop_owner"));
    let before = current;
    reconcile_acp(&runtime, &CodingAgentStore::load(&dir.0), false).unwrap();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        before,
        "Runner restart reconciliation is idempotent"
    );
}

#[test]
fn acp_reconcile_survives_repair_slot_regeneration_and_removal_tombstones() {
    let dir = Scratch::new();
    let a = runtime(&dir.0, "A", operator());
    let b = runtime(&dir.0, "B", operator());
    let mut store = CodingAgentStore::load(&dir.0);
    store.stage_update(request("pi", 0)).unwrap();
    store.commit().unwrap();
    for slot in [&a, &b] {
        reconcile_acp(slot, &store, false).unwrap();
    }
    let mut restarted = CodingAgentStore::load(&dir.0);
    let mut edit = request("pi", 1);
    edit.previous_id = Some("pi".into());
    edit.profile.name = "Pi Local".into();
    restarted.stage_update(edit).unwrap();
    restarted.commit().unwrap();
    reconcile_acp(&b, &restarted, false).unwrap();
    assert!(std::fs::read_to_string(b.runner_config.as_ref().unwrap())
        .unwrap()
        .contains("Pi Local"));
    restarted.stage_remove("pi", 2).unwrap();
    restarted.commit().unwrap();
    // The older slot still contains the original provider, not the edit.
    for slot in [&b, &a] {
        reconcile_acp(slot, &CodingAgentStore::load(&dir.0), false).unwrap();
        let doc = std::fs::read_to_string(slot.runner_config.as_ref().unwrap()).unwrap();
        assert!(!doc.contains("id = \"pi\""));
        assert!(doc.contains("id = \"operator\""));
    }
    let regenerated = runtime(&dir.0, "C", operator());
    reconcile_acp(&regenerated, &CodingAgentStore::load(&dir.0), false).unwrap();
    assert!(!std::fs::read_to_string(regenerated.runner_config.unwrap())
        .unwrap()
        .contains("id = \"pi\""));
}

#[test]
fn acp_reconcile_collision_and_operator_replacement_fail_closed() {
    let dir = Scratch::new();
    let runtime = runtime(&dir.0, "A", operator());
    let path = runtime.runner_config.as_ref().unwrap();
    let original = std::fs::read(path).unwrap();
    let mut candidate = CodingAgentStore::load(&dir.0);
    candidate.stage_update(request("operator", 0)).unwrap();
    assert_eq!(
        reconcile_acp(&runtime, &candidate, true).unwrap_err().code,
        "coding_agent_ownership_conflict"
    );
    assert_eq!(std::fs::read(path).unwrap(), original);
    assert!(!dir.0.join("coding-agents.json").exists());
    let mut store = CodingAgentStore::load(&dir.0);
    store.stage_update(request("pi", 0)).unwrap();
    store.commit().unwrap();
    reconcile_acp(&runtime, &store, false).unwrap();
    let modified = std::fs::read_to_string(path)
        .unwrap()
        .replace(store.owner_id(), "another-owner");
    std::fs::write(path, &modified).unwrap();
    assert!(reconcile_acp(&runtime, &store, false).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), modified);
    store.stage_remove("pi", 1).unwrap();
    assert!(
        reconcile_acp(&runtime, &store, true).is_err(),
        "a tombstone cannot delete a replacement operator provider"
    );
}

#[test]
fn acp_reconcile_supports_inline_syntax_and_checks_exact_runner_identity() {
    let dir = Scratch::new();
    let runtime = runtime(&dir.0, "A", "");
    let path = runtime.runner_config.as_ref().unwrap();
    let original = std::fs::read_to_string(path).unwrap();
    // acp must be at the root, before [policy].
    std::fs::write(path, original.replace("[policy]", "acp = { max_concurrent_runs = 2, agents = [{ id = \"operator\", name = \"Operator Agent\", executable = \"/operator/agent\" }] }\n[policy]")).unwrap();
    let mut store = CodingAgentStore::load(&dir.0);
    store.stage_update(request("pi", 0)).unwrap();
    store.commit().unwrap();
    reconcile_acp(&runtime, &store, false).unwrap();
    let current = std::fs::read_to_string(path).unwrap();
    assert!(current.contains("operator"));
    assert!(current.contains("desktop_owner"));
    let mut other = runtime.clone();
    other.runner_client_id = Some("replaced".into());
    assert!(reconcile_acp(&other, &store, false).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), current);
}
