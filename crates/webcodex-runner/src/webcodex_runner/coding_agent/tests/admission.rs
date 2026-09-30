use super::*;

#[test]
#[cfg(unix)]
fn concurrent_duplicate_start_admission_creates_exactly_one_prompt() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "wait_cancel");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_concurrentdup01";
    let request = start_request(&manager, &root, run, BTreeMap::new());
    *manager.admission_test_barrier.lock().unwrap() = Some(Arc::new(std::sync::Barrier::new(2)));

    let first_manager = Arc::clone(&manager);
    let first_projects = projects.clone();
    let first_request = request.clone();
    let first = thread::spawn(move || first_manager.handle(first_request, &first_projects));
    let second_manager = Arc::clone(&manager);
    let second_projects = projects.clone();
    let second = thread::spawn(move || second_manager.handle(request, &second_projects));

    let first = first.join().unwrap();
    let second = second.join().unwrap();
    *manager.admission_test_barrier.lock().unwrap() = None;
    assert!(first.error.is_none(), "{:?}", first.error);
    assert!(second.error.is_none(), "{:?}", second.error);
    assert_eq!(successful_start_run_id(&first).as_deref(), Some(run));
    assert_eq!(successful_start_run_id(&second).as_deref(), Some(run));
    assert_eq!(
        manager
            .initial_claim_writes
            .load(std::sync::atomic::Ordering::SeqCst),
        1,
        "duplicate concurrent admission must publish only one initial durable claim"
    );
    assert_eq!(manager.runs.lock().unwrap().len(), 1);
    wait_for_received_method_count(&temp, "session/prompt", 1);
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        1,
        "duplicate concurrent admission dispatched more than one ACP prompt"
    );

    manager.handle(
        CodingAgentRequest::Cancel(CodingAgentCancelRequest {
            run_id: run.to_string(),
        }),
        &projects,
    );
    wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
}

#[test]
#[cfg(unix)]
fn concurrent_capacity_admission_never_exceeds_configured_limit() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "wait_cancel");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    *manager.admission_test_barrier.lock().unwrap() = Some(Arc::new(std::sync::Barrier::new(2)));
    let first_request = start_request(
        &manager,
        &root,
        "wc_agent_run_concurrentcap01",
        BTreeMap::new(),
    );
    let second_request = start_request(
        &manager,
        &root,
        "wc_agent_run_concurrentcap02",
        BTreeMap::new(),
    );

    let first_manager = Arc::clone(&manager);
    let first_projects = projects.clone();
    let first = thread::spawn(move || first_manager.handle(first_request, &first_projects));
    let second_manager = Arc::clone(&manager);
    let second_projects = projects.clone();
    let second = thread::spawn(move || second_manager.handle(second_request, &second_projects));

    let responses = [first.join().unwrap(), second.join().unwrap()];
    *manager.admission_test_barrier.lock().unwrap() = None;
    let successes = responses
        .iter()
        .filter_map(successful_start_run_id)
        .collect::<Vec<_>>();
    let capacity_failures = responses
        .iter()
        .filter(|response| {
            response.error.as_ref().map(|error| error.code.as_str())
                == Some("coding_agent_capacity_full")
        })
        .count();
    assert_eq!(successes.len(), 1, "exactly one Run may acquire the slot");
    assert_eq!(
        capacity_failures, 1,
        "the competing Run must fail capacity admission"
    );
    assert_eq!(
        manager
            .initial_claim_writes
            .load(std::sync::atomic::Ordering::SeqCst),
        1,
        "capacity admission must publish only the winning durable claim"
    );
    assert_eq!(
        manager
            .runs
            .lock()
            .unwrap()
            .values()
            .filter(|entry| !entry.snapshot().state.terminal())
            .count(),
        1,
        "active Run count exceeded max_concurrent_runs=1"
    );
    wait_for_received_method_count(&temp, "session/prompt", 1);
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        1
    );

    let winner = &successes[0];
    manager.handle(
        CodingAgentRequest::Cancel(CodingAgentCancelRequest {
            run_id: winner.clone(),
        }),
        &projects,
    );
    wait_for_snapshot(&manager, winner, |snapshot| snapshot.state.terminal());
}

#[test]
#[cfg(unix)]
fn capacity_stale_provider_and_replay_are_fenced_before_duplicate_prompt() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "wait_cancel");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let first_run = "wc_agent_run_capacity0001";
    assert!(manager
        .handle(
            start_request(&manager, &root, first_run, BTreeMap::new()),
            &projects
        )
        .error
        .is_none());
    wait_for_snapshot(&manager, first_run, |snapshot| {
        snapshot.state == CodingAgentRunState::Running
    });
    let second = manager.handle(
        start_request(
            &manager,
            &root,
            "wc_agent_run_capacity0002",
            BTreeMap::new(),
        ),
        &projects,
    );
    assert_eq!(
        second.error.as_ref().map(|error| error.code.as_str()),
        Some("coding_agent_capacity_full")
    );
    let mut stale = start_request(&manager, &root, "wc_agent_run_stale000001", BTreeMap::new());
    if let CodingAgentRequest::Start(request) = &mut stale {
        request.provider_instance_id = "replaced-provider".to_string();
    }
    let stale = manager.handle(stale, &projects);
    assert_eq!(
        stale.error.as_ref().map(|error| error.code.as_str()),
        Some("stale_coding_agent_provider")
    );
    manager.handle(
        CodingAgentRequest::Cancel(CodingAgentCancelRequest {
            run_id: first_run.to_string(),
        }),
        &projects,
    );
    wait_for_snapshot(&manager, first_run, |snapshot| snapshot.state.terminal());

    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "end");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_replay000001";
    let request = start_request(&manager, &root, run, BTreeMap::new());
    assert!(manager.handle(request.clone(), &projects).error.is_none());
    wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert!(manager.handle(request, &projects).error.is_none());
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        1
    );
    let mut conflict = start_request(&manager, &root, run, BTreeMap::new());
    if let CodingAgentRequest::Start(request) = &mut conflict {
        request.intent_fingerprint = "different-fingerprint".to_string();
    }
    assert_eq!(
        manager
            .handle(conflict, &projects)
            .error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("idempotency_conflict")
    );
}

#[test]
fn project_binding_requires_current_writable_registration() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    let other = temp.path().join("other");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&other).unwrap();
    let projects = temp.path().join("projects");
    fs::create_dir_all(&projects).unwrap();
    let config_path = projects.join("p.toml");
    let write_registration = |allow_patch: bool, disabled: bool, path: &Path| {
        fs::write(
            &config_path,
            format!(
                "id = \"demo\"\npath = {:?}\nallow_patch = {allow_patch}\ndisabled = {disabled}\n",
                path.to_string_lossy()
            ),
        )
        .unwrap();
    };

    write_registration(false, false, &root);
    assert!(!project_binding_matches(
        &projects,
        "test",
        "agent:test:demo",
        root.to_string_lossy().as_ref()
    ));

    write_registration(true, true, &root);
    assert!(!project_binding_matches(
        &projects,
        "test",
        "agent:test:demo",
        root.to_string_lossy().as_ref()
    ));

    write_registration(true, false, &root);
    assert!(project_binding_matches(
        &projects,
        "test",
        "agent:test:demo",
        root.to_string_lossy().as_ref()
    ));
    assert!(!project_binding_matches(
        &projects,
        "test",
        "agent:test:wrong",
        root.to_string_lossy().as_ref()
    ));
    assert!(!project_binding_matches(
        &projects,
        "test",
        "agent:test:demo",
        other.to_string_lossy().as_ref()
    ));
}

#[cfg(unix)]
#[test]
fn project_binding_accepts_canonical_alias_and_rejects_symlink_retarget() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    let alias = temp.path().join("repo-alias");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    symlink(&first, &alias).unwrap();
    let projects = temp.path().join("projects");
    fs::create_dir_all(&projects).unwrap();
    fs::write(
        projects.join("p.toml"),
        format!(
            "id = \"demo\"\npath = {:?}\nallow_patch = true\n",
            alias.to_string_lossy()
        ),
    )
    .unwrap();
    let canonical_first = canonicalize_existing(&first).unwrap();
    assert!(project_binding_matches(
        &projects,
        "test",
        "agent:test:demo",
        canonical_first.to_string_lossy().as_ref()
    ));

    fs::remove_file(&alias).unwrap();
    symlink(&second, &alias).unwrap();
    assert!(!project_binding_matches(
        &projects,
        "test",
        "agent:test:demo",
        canonical_first.to_string_lossy().as_ref()
    ));
}

#[cfg(target_os = "macos")]
#[test]
fn project_binding_accepts_var_private_var_alias() {
    assert_eq!(
        canonicalize_existing(Path::new("/var")).unwrap(),
        canonicalize_existing(Path::new("/private/var")).unwrap()
    );
    let temp = TempDir::new().unwrap();
    let projects = temp.path().join("projects");
    fs::create_dir_all(&projects).unwrap();
    fs::write(
        projects.join("p.toml"),
        "id = \"demo\"\npath = \"/private/var\"\nallow_patch = true\n",
    )
    .unwrap();
    assert!(project_binding_matches(
        &projects,
        "test",
        "agent:test:demo",
        "/var"
    ));
}

#[cfg(windows)]
#[test]
fn project_binding_accepts_windows_case_and_verbatim_disk_identity() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("RepoCase");
    fs::create_dir_all(&root).unwrap();
    let canonical = canonicalize_existing(&root).unwrap();
    let canonical_text = canonical.to_string_lossy().to_string();
    let plain = canonical_text
        .strip_prefix(r"\\?\")
        .unwrap_or(&canonical_text)
        .to_string();
    let case_variant = plain.to_ascii_uppercase();
    let verbatim = format!(r"\\?\{plain}");
    let projects = temp.path().join("projects");
    fs::create_dir_all(&projects).unwrap();
    fs::write(
        projects.join("p.toml"),
        format!("id = \"demo\"\npath = {:?}\nallow_patch = true\n", plain),
    )
    .unwrap();
    assert!(project_binding_matches(
        &projects,
        "test",
        "agent:test:demo",
        &case_variant
    ));
    assert!(project_binding_matches(
        &projects,
        "test",
        "agent:test:demo",
        &verbatim
    ));
}

#[test]
#[cfg(unix)]
fn child_environment_is_cleared_and_missing_mapping_never_spawns() {
    crate::tests::IsolatedEnv::new()
        .set("WEBCODEX_TEST_ACP_VISIBLE", "visible-value")
        .set("WEBCODEX_TEST_ACP_HIDDEN", "must-not-reach-child")
        .run("inherited", || {
            let temp = crate::tests::executable_tempdir();
            let (exe, args) = fake_agent(&temp, "end");
            let mut cfg = fake_config(exe, args);
            cfg.agents[0].env_from_env = BTreeMap::from([(
                "ACP_VISIBLE".to_string(),
                "WEBCODEX_TEST_ACP_VISIBLE".to_string(),
            )]);
            let projects = project_fixture(&temp);
            let root = temp.path().join("repo");
            let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
            let run = "wc_agent_run_envclear0001";
            assert!(manager
                .handle(
                    start_request(&manager, &root, run, BTreeMap::new()),
                    &projects
                )
                .error
                .is_none());
            wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
            let startup = wire_log(&temp)
                .into_iter()
                .find(|entry| entry.get("env_keys").is_some())
                .unwrap();
            assert_eq!(startup["env_keys"], json!(["ACP_VISIBLE"]));

            let temp = crate::tests::executable_tempdir();
            let (exe, args) = fake_agent(&temp, "end");
            let mut cfg = fake_config(exe, args);
            cfg.agents[0].env_from_env = BTreeMap::from([(
                "ACP_VISIBLE".to_string(),
                "WEBCODEX_TEST_ACP_MISSING".to_string(),
            )]);
            let projects = project_fixture(&temp);
            let root = temp.path().join("repo");
            let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
            let response = manager.handle(
                start_request(
                    &manager,
                    &root,
                    "wc_agent_run_missingenv01",
                    BTreeMap::new(),
                ),
                &projects,
            );
            assert_eq!(
                response.error.as_ref().map(|error| error.code.as_str()),
                Some("coding_agent_environment_unavailable")
            );
            assert!(
                wire_log(&temp).is_empty(),
                "provider child must not start when an env source is missing"
            );
        });
}
