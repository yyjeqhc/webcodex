use super::*;

fn continuity_requirements() -> [(&'static str, Vec<RunnerFeature>); 4] {
    [
        (
            "browser_discover",
            vec![
                RunnerFeature::BrowserObserve,
                RunnerFeature::BrowserExtensionBridge,
            ],
        ),
        (
            "browser_attach",
            vec![
                RunnerFeature::BrowserControl,
                RunnerFeature::BrowserExtensionBridge,
            ],
        ),
        (
            "browser_launch_managed",
            vec![
                RunnerFeature::BrowserLaunch,
                RunnerFeature::BrowserManagedProfile,
            ],
        ),
        (
            "browser_surface",
            vec![
                RunnerFeature::BrowserObserve,
                RunnerFeature::BrowserSurfaceHandoff,
                RunnerFeature::ComputerObserve,
            ],
        ),
    ]
}

#[tokio::test]
async fn browser_continuity_operations_reach_the_exact_runner_wire() {
    for (kind, features) in continuity_requirements() {
        let registry = RunnerRegistry::default();
        let alice = auth_context(Some("alice"), false);
        let mut registration = runner_registration("continuity", "browser-inst", vec![]);
        registration.owner = Some("alice".into());
        registration.capabilities = v2_baseline_capabilities();
        for feature in features {
            registration.capabilities.set(feature, true);
        }
        registry.register(registration).await.unwrap();
        let (_, _receiver) = registry
            .enqueue_browser(
                "continuity".into(),
                kind,
                "{}".into(),
                "alice".into(),
                Some(&alice),
                5,
            )
            .await
            .unwrap_or_else(|error| panic!("{kind}: {error}"));
        let poll = || RunnerPollRequest {
            client_id: "continuity".into(),
            runner_instance_id: "browser-inst".into(),
        };
        let request = registry
            .poll(poll())
            .await
            .unwrap()
            .expect("exact Browser operation");
        assert_eq!(request.kind, kind);
        assert_eq!(request.stdin.as_deref(), Some("{}"));
        assert!(request.command.is_empty());
        assert!(request.process.is_none());
        assert!(request.script.is_none());
        assert!(registry.poll(poll()).await.unwrap().is_none());
    }
}

#[tokio::test]
async fn browser_continuity_rechecks_every_capability_without_dispatch() {
    for (kind, features) in continuity_requirements() {
        for missing in features.iter().copied() {
            let registry = RunnerRegistry::default();
            let alice = auth_context(Some("alice"), false);
            let mut registration = runner_registration("continuity-old", "browser-inst", vec![]);
            registration.owner = Some("alice".into());
            registration.capabilities = v2_baseline_capabilities();
            for feature in &features {
                registration.capabilities.set(*feature, true);
            }
            registration.capabilities.set(missing, false);
            registry.register(registration).await.unwrap();
            let error = registry
                .enqueue_browser(
                    "continuity-old".into(),
                    kind,
                    "{}".into(),
                    "alice".into(),
                    Some(&alice),
                    5,
                )
                .await
                .unwrap_err();
            assert!(error.contains("capability_unavailable"), "{kind}: {error}");
            assert!(error.contains(missing.as_wire_name()), "{kind}: {error}");
            assert!(registry
                .poll(RunnerPollRequest {
                    client_id: "continuity-old".into(),
                    runner_instance_id: "browser-inst".into(),
                })
                .await
                .unwrap()
                .is_none());
        }
    }
}

#[tokio::test]
async fn browser_batch_is_one_bounded_runner_invocation() {
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_browser_runner(&registry, "browser-batch", true, true, true, true, true).await;
    let operations = vec![
        serde_json::json!({"action": "input_text", "element_id": "element_fixture", "text": "x".repeat(4096)});
        32
    ];
    let payload = serde_json::json!({"browser_id": "browser_fixture", "page_id": "page_fixture", "operations": operations}).to_string();
    assert!(payload.len() > 32 * 1024);
    let (_, _receiver) = registry
        .enqueue_browser(
            "browser-batch".into(),
            "browser_batch",
            payload.clone(),
            "alice".into(),
            Some(&alice),
            120,
        )
        .await
        .unwrap();
    let poll = || RunnerPollRequest {
        client_id: "browser-batch".into(),
        runner_instance_id: "browser-inst".into(),
    };
    let request = registry.poll(poll()).await.unwrap().unwrap();
    assert_eq!(request.kind, "browser_batch");
    assert_eq!(request.stdin.as_deref(), Some(payload.as_str()));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
    assert!(registry.poll(poll()).await.unwrap().is_none());
    assert!(registry
        .enqueue_browser(
            "browser-batch".into(),
            "browser_batch",
            "x".repeat(256 * 1024 + 1),
            "alice".into(),
            Some(&alice),
            120
        )
        .await
        .unwrap_err()
        .contains("too large"));
}

#[tokio::test]
async fn browser_batch_requires_additive_capability_without_dispatch() {
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_browser_runner(
        &registry,
        "browser-old-batch",
        true,
        true,
        true,
        false,
        true,
    )
    .await;
    let error = registry
        .enqueue_browser(
            "browser-old-batch".into(),
            "browser_batch",
            "{}".into(),
            "alice".into(),
            Some(&alice),
            120,
        )
        .await
        .unwrap_err();
    assert!(error.contains("browser_batch"), "{error}");
    assert!(error.contains("capability_unavailable"), "{error}");
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "browser-old-batch".into(),
            runner_instance_id: "browser-inst".into(),
        })
        .await
        .unwrap()
        .is_none());
}

async fn register_browser_runner(
    registry: &RunnerRegistry,
    client_id: &str,
    observe: bool,
    control: bool,
    element_action_admission: bool,
    batch: bool,
    launch: bool,
) {
    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            computer_session_availability: None,
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: client_id.to_string(),
            runner_instance_id: "browser-inst".to_string(),
            runner_protocol_generation: RUNNER_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: RunnerCapabilities {
                browser_observe: observe,
                browser_control: control,
                browser_element_action_admission: element_action_admission,
                browser_batch: batch,
                browser_launch: launch,
                ..v2_baseline_capabilities()
            },
            policy: None,
        }))
        .await
        .unwrap();
}

#[tokio::test]
async fn browser_capability_is_checked_before_dispatch_and_never_falls_back() {
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_browser_runner(&registry, "browser-old", false, false, false, false, false).await;

    for (kind, payload, capability) in [
        ("browser_list_browsers", r#"{}"#, "browser_observe"),
        ("browser_launch", r#"{}"#, "browser_launch"),
        (
            "browser_navigate",
            r#"{"browser_id":"browser_test","page_id":"page_test","url":"https://example.test/"}"#,
            "browser_control",
        ),
        (
            "browser_select_option",
            r#"{"browser_id":"browser_test","page_id":"page_test","element_id":"element_test","option":"Engineering"}"#,
            "browser_control",
        ),
        (
            "browser_set_value",
            r#"{"browser_id":"browser_test","page_id":"page_test","element_id":"element_test","value":"2027-06"}"#,
            "browser_control",
        ),
        (
            "browser_upload_file",
            r#"{"browser_id":"browser_test","page_id":"page_test","element_id":"element_test","project_root":"C:\\fixture","path":"resume.pdf"}"#,
            "browser_control",
        ),
    ] {
        let error = registry
            .enqueue_browser(
                "browser-old".to_string(),
                kind,
                payload.to_string(),
                "alice".to_string(),
                Some(&alice),
                5,
            )
            .await
            .unwrap_err();
        assert!(error.contains("capability_unavailable"), "{error}");
        assert!(error.contains(capability), "{error}");
        assert!(registry
            .poll(RunnerPollRequest {
                client_id: "browser-old".to_string(),
                runner_instance_id: "browser-inst".to_string(),
            })
            .await
            .unwrap()
            .is_none());
    }
}

#[tokio::test]
async fn browser_element_action_admission_is_additive_and_fails_closed_for_old_runners() {
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_browser_runner(&registry, "browser-legacy", true, true, false, false, true).await;

    for kind in [
        "browser_snapshot",
        "browser_click",
        "browser_input_text",
        "browser_select_option",
        "browser_set_value",
        "browser_upload_file",
    ] {
        let error = registry
            .enqueue_browser(
                "browser-legacy".to_string(),
                kind,
                "{}".to_string(),
                "alice".to_string(),
                Some(&alice),
                5,
            )
            .await
            .unwrap_err();
        assert!(error.contains("capability_unavailable"), "{kind}: {error}");
        assert!(
            error.contains("browser_element_action_admission"),
            "{kind}: {error}"
        );
        assert!(registry
            .poll(RunnerPollRequest {
                client_id: "browser-legacy".to_string(),
                runner_instance_id: "browser-inst".to_string(),
            })
            .await
            .unwrap()
            .is_none());
    }

    registry
        .enqueue_browser(
            "browser-legacy".to_string(),
            "browser_navigate",
            "{}".to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .expect("generic Browser control remains compatible with the older capability");
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "browser-legacy".to_string(),
            runner_instance_id: "browser-inst".to_string(),
        })
        .await
        .unwrap()
        .expect("navigation request");
    assert_eq!(request.kind, "browser_navigate");
}

#[tokio::test]
async fn browser_precise_operation_is_preserved_on_wire_without_shell_fields() {
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_browser_runner(&registry, "browser-new", true, true, true, false, true).await;

    let (_request_id, _receiver) = registry
        .enqueue_browser(
            "browser-new".to_string(),
            "browser_snapshot",
            r#"{"browser_id":"browser_test","page_id":"page_test"}"#.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "browser-new".to_string(),
            runner_instance_id: "browser-inst".to_string(),
        })
        .await
        .unwrap()
        .expect("browser request");
    assert_eq!(request.kind, "browser_snapshot");
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
    assert_eq!(
        request.stdin.as_deref(),
        Some(r#"{"browser_id":"browser_test","page_id":"page_test"}"#)
    );
}
