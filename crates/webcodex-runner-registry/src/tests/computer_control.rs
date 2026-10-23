use super::*;

#[tokio::test]
async fn computer_letter_shortcuts_require_both_capabilities_before_enqueue() {
    for (base, letters) in [(false, false), (true, false), (false, true), (true, true)] {
        let registry = RunnerRegistry::default();
        let mut registration = runner_registration("letters", "instance", Vec::new());
        registration.owner = Some("alice".into());
        registration.capabilities.computer_key_input = base;
        registration.capabilities.computer_letter_shortcuts = letters;
        registry
            .register(current_runner_registration(registration))
            .await
            .unwrap();
        let access = auth_context(Some("alice"), false);
        let payload = r#"{"surface_id":"surface_test","key":"p","modifiers":["control"]}"#;
        let result = registry
            .enqueue_computer(
                "letters".into(),
                "computer_key_input",
                payload.into(),
                "alice".into(),
                Some(&access),
                5,
            )
            .await;
        assert_eq!(result.is_ok(), base && letters);
        let request = registry
            .poll(RunnerPollRequest {
                client_id: "letters".into(),
                runner_instance_id: "instance".into(),
            })
            .await
            .unwrap();
        if base && letters {
            let request = request.expect("accepted letter chord");
            assert_eq!(request.kind, "computer_key_input");
            assert_eq!(request.stdin.as_deref(), Some(payload));
        } else {
            assert!(request.is_none());
            assert!(result.unwrap_err().contains(if base {
                "computer_letter_shortcuts"
            } else {
                "computer_key_input"
            }));
        }
    }
}

#[tokio::test]
async fn computer_letter_shortcuts_recheck_access_and_capabilities_at_dequeue() {
    for change in ["owner", "key_input", "letters", "instance"] {
        let registry = RunnerRegistry::default();
        let mut registration = runner_registration("letters", "instance", Vec::new());
        registration.owner = Some("alice".into());
        registration.capabilities.computer_key_input = true;
        registration.capabilities.computer_letter_shortcuts = true;
        registry
            .register(current_runner_registration(registration.clone()))
            .await
            .unwrap();
        let access = auth_context(Some("alice"), false);
        let (_, rx) = registry
            .enqueue_computer(
                "letters".into(),
                "computer_key_input",
                r#"{"surface_id":"surface_test","key":"j","modifiers":["control"]}"#.into(),
                "alice".into(),
                Some(&access),
                5,
            )
            .await
            .unwrap();
        match change {
            "owner" => registration.owner = Some("bob".into()),
            "key_input" => registration.capabilities.computer_key_input = false,
            "instance" => registration.runner_instance_id = "replacement".into(),
            _ => registration.capabilities.computer_letter_shortcuts = false,
        }
        registry
            .register(current_runner_registration(registration))
            .await
            .unwrap();
        assert!(registry
            .poll(RunnerPollRequest {
                client_id: "letters".into(),
                runner_instance_id: if change == "instance" {
                    "replacement"
                } else {
                    "instance"
                }
                .into()
            })
            .await
            .unwrap()
            .is_none());
        let response = rx.await.unwrap();
        assert_eq!(response.request_dispatched, Some(false));
        assert!(response.error.unwrap().starts_with(match change {
            "owner" => "client_access_denied:",
            "instance" => "runner instance was replaced before the request completed; the request was not handed to the replacement instance",
            _ => "capability_unavailable:",
        }));
    }
}

#[tokio::test]
async fn computer_letter_shortcuts_reject_stale_instances_and_invalid_chords() {
    let registry = RunnerRegistry::default();
    let mut registration = runner_registration("letters", "new-instance", Vec::new());
    registration.owner = Some("alice".into());
    registration.capabilities.computer_key_input = true;
    registration.capabilities.computer_letter_shortcuts = true;
    registry
        .register(current_runner_registration(registration))
        .await
        .unwrap();
    let access = auth_context(Some("alice"), false);
    let error = registry
        .enqueue_computer_for_instance(
            "letters".into(),
            "computer_key_input",
            r#"{"surface_id":"surface_test","key":"p","modifiers":["control"]}"#.into(),
            "alice".into(),
            Some(&access),
            5,
            "old-instance",
        )
        .await
        .unwrap_err();
    assert!(error.starts_with("runner_replaced:"));
    for modifiers in [
        serde_json::json!([]),
        serde_json::json!(["option"]),
        serde_json::json!(["control", "shift"]),
        serde_json::Value::Null,
    ] {
        let payload =
            serde_json::json!({"surface_id":"surface_test", "key":"p", "modifiers":modifiers})
                .to_string();
        assert!(registry
            .enqueue_computer(
                "letters".into(),
                "computer_key_input",
                payload,
                "alice".into(),
                Some(&access),
                5
            )
            .await
            .is_err());
    }
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "letters".into(),
            runner_instance_id: "new-instance".into()
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn computer_control_enqueue_requires_independent_capability() {
    // CU-AX2 control remains independently fenced from CU-AX3 text input.
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_runner(
        &registry,
        "computer-control",
        "alice",
        true,
        true,
        false,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","element_id":"element_test","action":"focus"}"#;
    let error = registry
        .enqueue_computer(
            "computer-control".to_string(),
            "computer_control",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_control"));

    register_computer_test_runner(
        &registry,
        "computer-control",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-control".to_string(),
            "computer_control",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "computer-control".to_string(),
            runner_instance_id: "computer-inst".to_string(),
        })
        .await
        .unwrap()
        .expect("queued computer control request");
    assert_eq!(request.kind, "computer_control");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_scroll_to_element_requires_independent_capability() {
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_runner(
        &registry,
        "computer-scroll-control-only",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","element_id":"element_test"}"#;
    let error = registry
        .enqueue_computer(
            "computer-scroll-control-only".to_string(),
            "computer_scroll_to_element",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_scroll_to_element"));

    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            owner: Some("alice".to_string()),
            capabilities: RunnerCapabilities {
                computer_control: true,
                computer_scroll_to_element: true,
                ..Default::default()
            },
            ..runner_registration(
                "computer-scroll-capable",
                "computer-scroll-inst",
                Vec::new(),
            )
        }))
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-scroll-capable".to_string(),
            "computer_scroll_to_element",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "computer-scroll-capable".to_string(),
            runner_instance_id: "computer-scroll-inst".to_string(),
        })
        .await
        .unwrap()
        .expect("queued computer scroll request");
    assert_eq!(request.kind, "computer_scroll_to_element");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_key_input_requires_independent_capability() {
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_runner(
        &registry,
        "computer-key-control-only",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test","key":"tab","modifiers":["shift"]}"#;
    let error = registry
        .enqueue_computer(
            "computer-key-control-only".to_string(),
            "computer_key_input",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_key_input"));

    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            owner: Some("alice".to_string()),
            capabilities: RunnerCapabilities {
                computer_control: true,
                computer_key_input: true,
                ..Default::default()
            },
            ..runner_registration("computer-key-capable", "computer-key-inst", Vec::new())
        }))
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-key-capable".to_string(),
            "computer_key_input",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "computer-key-capable".to_string(),
            runner_instance_id: "computer-key-inst".to_string(),
        })
        .await
        .unwrap()
        .expect("queued computer key-input request");
    assert_eq!(request.kind, "computer_key_input");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}

#[tokio::test]
async fn computer_pointer_enqueue_requires_independent_capability_and_typed_envelope() {
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);
    let payload =
        r#"{"display_id":"display_iavN7wEjRWeJq83v","snapshot_generation":7,"x":123,"y":456}"#;

    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            owner: Some("alice".to_string()),
            capabilities: RunnerCapabilities {
                computer_control: true,
                computer_display_observe: true,
                ..Default::default()
            },
            ..runner_registration("computer-pointer-old", "pointer-old-inst", Vec::new())
        }))
        .await
        .unwrap();
    for kind in ["computer_pointer_move", "computer_pointer_click"] {
        let error = registry
            .enqueue_computer(
                "computer-pointer-old".to_string(),
                kind,
                payload.to_string(),
                "alice".to_string(),
                Some(&alice),
                5,
            )
            .await
            .unwrap_err();
        assert!(
            error.contains("does not support computer_pointer_control"),
            "{error}"
        );
    }

    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            owner: Some("alice".to_string()),
            capabilities: RunnerCapabilities {
                computer_pointer_control: true,
                ..Default::default()
            },
            ..runner_registration("computer-pointer-capable", "pointer-inst", Vec::new())
        }))
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-pointer-capable".to_string(),
            "computer_pointer_click",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "computer-pointer-capable".to_string(),
            runner_instance_id: "pointer-inst".to_string(),
        })
        .await
        .unwrap()
        .expect("queued computer pointer request");
    assert_eq!(request.kind, "computer_pointer_click");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.cwd.is_none());
    assert!(request.path.is_none());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}
#[tokio::test]
async fn computer_clipboard_enqueue_requires_independent_capabilities_and_typed_envelopes() {
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);

    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            owner: Some("alice".to_string()),
            capabilities: RunnerCapabilities {
                computer_observe: true,
                computer_control: true,
                computer_text_input: true,
                ..Default::default()
            },
            ..runner_registration("computer-clipboard-old", "clipboard-old-inst", Vec::new())
        }))
        .await
        .unwrap();
    for (kind, payload, capability) in [
        ("computer_read_clipboard", "{}", "computer_clipboard_read"),
        (
            "computer_write_clipboard",
            r#"{"text":"hello"}"#,
            "computer_clipboard_write",
        ),
    ] {
        let error = registry
            .enqueue_computer(
                "computer-clipboard-old".to_string(),
                kind,
                payload.to_string(),
                "alice".to_string(),
                Some(&alice),
                5,
            )
            .await
            .unwrap_err();
        assert!(
            error.contains(&format!("does not support {capability}")),
            "{error}"
        );
    }

    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            owner: Some("alice".to_string()),
            capabilities: RunnerCapabilities {
                computer_clipboard_read: true,
                ..Default::default()
            },
            ..runner_registration("computer-clipboard-read", "clipboard-read-inst", Vec::new())
        }))
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-clipboard-read".to_string(),
            "computer_read_clipboard",
            "{}".to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "computer-clipboard-read".to_string(),
            runner_instance_id: "clipboard-read-inst".to_string(),
        })
        .await
        .unwrap()
        .expect("queued clipboard read request");
    assert_eq!(request.kind, "computer_read_clipboard");
    assert_eq!(request.stdin.as_deref(), Some("{}"));
    assert!(request.command.is_empty());
    assert!(request.cwd.is_none());
    assert!(request.path.is_none());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
    let error = registry
        .enqueue_computer(
            "computer-clipboard-read".to_string(),
            "computer_write_clipboard",
            r#"{"text":"hello"}"#.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_clipboard_write"));

    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            owner: Some("alice".to_string()),
            capabilities: RunnerCapabilities {
                computer_clipboard_write: true,
                ..Default::default()
            },
            ..runner_registration(
                "computer-clipboard-write",
                "clipboard-write-inst",
                Vec::new(),
            )
        }))
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-clipboard-write".to_string(),
            "computer_write_clipboard",
            r#"{"text":"hello"}"#.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "computer-clipboard-write".to_string(),
            runner_instance_id: "clipboard-write-inst".to_string(),
        })
        .await
        .unwrap()
        .expect("queued clipboard write request");
    assert_eq!(request.kind, "computer_write_clipboard");
    assert_eq!(request.stdin.as_deref(), Some(r#"{"text":"hello"}"#));
    assert!(request.command.is_empty());
    assert!(request.cwd.is_none());
    assert!(request.path.is_none());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
    let error = registry
        .enqueue_computer(
            "computer-clipboard-write".to_string(),
            "computer_read_clipboard",
            "{}".to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_clipboard_read"));
}

#[tokio::test]
async fn computer_window_activation_requires_its_own_additive_capability() {
    let registry = RunnerRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_runner(
        &registry,
        "computer-activate",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let payload = r#"{"surface_id":"surface_test"}"#;
    let error = registry
        .enqueue_computer(
            "computer-activate".to_string(),
            "computer_activate_window",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_window_activate"));

    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            owner: Some("alice".to_string()),
            capabilities: RunnerCapabilities {
                shell: true,
                file_read: true,
                computer_observe: true,
                computer_accessibility_observe: true,
                computer_control: true,
                computer_window_activate: true,
                ..Default::default()
            },
            ..runner_registration("computer-activate", "computer-inst", Vec::new())
        }))
        .await
        .unwrap();
    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-activate".to_string(),
            "computer_activate_window",
            payload.to_string(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "computer-activate".to_string(),
            runner_instance_id: "computer-inst".to_string(),
        })
        .await
        .unwrap()
        .expect("queued computer window activation request");
    assert_eq!(request.kind, "computer_activate_window");
    assert_eq!(request.stdin.as_deref(), Some(payload));
    assert!(request.command.is_empty());
    assert!(request.process.is_none());
    assert!(request.script.is_none());
}
