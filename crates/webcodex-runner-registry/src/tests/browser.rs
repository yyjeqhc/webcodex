use super::*;

async fn register_browser_runner(
    registry: &RunnerRegistry,
    client_id: &str,
    observe: bool,
    control: bool,
    element_action_admission: bool,
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
    register_browser_runner(&registry, "browser-old", false, false, false, false).await;

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
    register_browser_runner(&registry, "browser-legacy", true, true, false, true).await;

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
    register_browser_runner(&registry, "browser-new", true, true, true, true).await;

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
