use super::*;

#[tokio::test]
async fn enqueue_internal_posix_script_is_typed_and_capability_fenced() {
    let registry = ShellClientRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "internal-posix-on",
        "inst",
        ShellClientCapabilities {
            shell: true,
            internal_posix_script: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let script = "while [ 0 -lt 1 ]; do break; done\n";
    let (request_id, _rx) = registry
        .enqueue_internal_posix_script(
            "internal-posix-on".to_string(),
            Some("/tmp/proj".to_string()),
            script.to_string(),
            30,
            32,
            "tester".to_string(),
            None,
        )
        .await
        .expect("capable client should accept internal POSIX work");

    let polled = registry
        .poll(ShellAgentPollRequest {
            client_id: "internal-posix-on".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "run_internal_posix_script");
    assert!(polled.command.is_empty());
    assert!(polled.stdin.is_none());
    let payload = polled.script.expect("typed internal script payload");
    assert_eq!(
        payload.language,
        crate::shell_protocol::ShellScriptLanguage::Sh
    );
    assert_eq!(payload.script, script);
    assert!(payload.args.is_empty());
}

#[tokio::test]
async fn enqueue_internal_posix_script_missing_capability_fails_closed() {
    let registry = ShellClientRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "internal-posix-off",
        "inst",
        ShellClientCapabilities {
            shell: true,
            structured_script_payload: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let error = registry
        .enqueue_internal_posix_script(
            "internal-posix-off".to_string(),
            Some("/tmp/proj".to_string()),
            "printf ok\n".to_string(),
            30,
            32,
            "tester".to_string(),
            None,
        )
        .await
        .unwrap_err();
    assert!(error.starts_with("capability_unavailable:"), "{error}");
    assert!(error.contains("internal_posix_script"), "{error}");
    assert_structured_delete_client_idle(&registry, "internal-posix-off").await;
}

#[tokio::test]
async fn enqueue_internal_posix_script_preserves_generated_command_wire_bound() {
    let registry = ShellClientRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "internal-posix-bound",
        "inst",
        ShellClientCapabilities {
            shell: true,
            internal_posix_script: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let error = registry
        .enqueue_internal_posix_script(
            "internal-posix-bound".to_string(),
            Some("/tmp/proj".to_string()),
            "x".repeat(crate::shell_protocol::RAW_SHELL_WIRE_MAX_BYTES + 1),
            30,
            32,
            "tester".to_string(),
            None,
        )
        .await
        .unwrap_err();
    assert!(error.contains("Runner wire envelope"), "{error}");
    assert_structured_delete_client_idle(&registry, "internal-posix-bound").await;
}

#[tokio::test]
async fn same_instance_internal_posix_capability_downgrade_is_rejected() {
    let registry = ShellClientRegistry::default();
    register_instance_with_capabilities(
        &registry,
        "internal-posix-monotonic",
        "inst",
        ShellClientCapabilities {
            shell: true,
            internal_posix_script: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let error = register_instance_with_capabilities(
        &registry,
        "internal-posix-monotonic",
        "inst",
        ShellClientCapabilities {
            shell: true,
            internal_posix_script: false,
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(
        error.contains("cannot downgrade internal_posix_script"),
        "{error}"
    );
    let view = registry
        .get_client_view("internal-posix-monotonic")
        .await
        .unwrap();
    assert!(view.capabilities.internal_posix_script);
}
