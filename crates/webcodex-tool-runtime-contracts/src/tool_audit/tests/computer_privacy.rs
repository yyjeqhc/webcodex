use super::*;
use serde_json::json;

#[test]
fn request_audit_fails_closed_and_never_mutates_business_arguments() {
    // Structured validation lifecycle controls are audit metadata, not
    // validation identity inputs. Keep the explicit synchronous grace in
    // the bounded request projection while preserving the business args.
    let cargo_test = json!({
        "project": "agent:test:demo",
        "filter": "focused",
        "timeout_secs": 600,
        "sync_wait_secs": 1
    });
    let cargo_test_before = cargo_test.clone();
    let cargo_test_summary = session_log_arguments_for_tool_request("cargo_test", &cargo_test);
    assert_eq!(cargo_test_summary["sync_wait_secs"], 1);
    assert_eq!(cargo_test_summary["timeout_secs"], 600);
    let validation_target_id = cargo_test_summary["validation_target_id"]
        .as_str()
        .expect("cargo_test validation target");
    let mut later_grace = cargo_test.clone();
    later_grace["sync_wait_secs"] = json!(60);
    let later_summary = session_log_arguments_for_tool_request("cargo_test", &later_grace);
    assert_eq!(later_summary["sync_wait_secs"], 60);
    assert_eq!(later_summary["validation_target_id"], validation_target_id);
    assert_eq!(cargo_test, cargo_test_before);

    let project_validate = json!({
        "project": "agent:test:demo",
        "action": "check",
        "scope": {"packages": ["private-package-a", "private-package-b"]},
        "timeout_secs": 90
    });
    let project_summary =
        session_log_arguments_for_tool_request("project_validate", &project_validate);
    assert_eq!(project_summary["packages_present"], true);
    assert_eq!(project_summary["package_count"], 2);
    let serialized = serde_json::to_string(&project_summary).unwrap();
    assert!(!serialized.contains("private-package-a"));
    assert!(!serialized.contains("private-package-b"));

    let project_build = json!({
        "project": "agent:test:demo",
        "adapter": "rust",
        "scope": {"packages": ["private-build-a", "private-build-b"]},
        "timeout_secs": 1800
    });
    let build_summary = session_log_arguments_for_tool_request("project_build", &project_build);
    assert_eq!(build_summary["adapter"], "rust");
    assert_eq!(build_summary["packages_present"], true);
    assert_eq!(build_summary["package_count"], 2);
    assert_eq!(build_summary["timeout_secs"], 1800);
    let serialized = serde_json::to_string(&build_summary).unwrap();
    assert!(!serialized.contains("private-build-a"));
    assert!(!serialized.contains("private-build-b"));

    let unknown = json!({"secret": "UNKNOWN_TOOL_SECRET"});
    let unknown_before = unknown.clone();
    assert_eq!(
        session_log_arguments_for_tool_request("future_unknown_tool", &unknown),
        json!({})
    );
    assert_eq!(unknown, unknown_before);

    let malformed = json!({"secret": "MALFORMED_READ_SECRET"});
    let malformed_before = malformed.clone();
    assert_eq!(
        session_log_arguments_for_tool_request("read_files", &malformed),
        json!({})
    );
    assert_eq!(malformed, malformed_before);

    let retired_alias = json!({"client_id": "PRIVATE_RETIRED_ALIAS_CLIENT"});
    assert_eq!(
        session_log_arguments_for_tool_request("list_agents", &retired_alias),
        json!({}),
        "retired aliases must not become a second audit identity"
    );
}

#[test]
fn request_audit_omits_plugin_ssh_native_path_and_observation_secrets() {
    const PLUGIN_SECRET: &str = "PLUGIN_ARGUMENT_SECRET";
    const SSH_TARGET: &str = "root@private-native-target";
    const SSH_CWD: &str = "/private/native/default/cwd";
    const PROJECT_PATH: &str = "/private/native/project/root";
    const JOB_TOKEN: &str = "wjob-private-observation-token";

    let plugin_binding = "wc_pbind_qqqqqqqqqqqqqqqqqqqqqg".to_string();
    let plugin = json!({
        "action": "call",
        "binding": plugin_binding,
        "arguments": {
            "secret": PLUGIN_SECRET,
            "nested": {"token": "PRIVATE_PLUGIN_TOKEN"}
        }
    });
    let plugin_before = plugin.clone();
    let plugin_summary = session_log_arguments_for_tool_request("plugin_tool", &plugin);
    assert_eq!(plugin, plugin_before);
    assert_eq!(plugin_summary["action"], "call");
    assert_eq!(plugin_summary["binding_present"], true);
    assert_eq!(plugin_summary["arguments_present"], true);
    assert_eq!(plugin_summary["argument_key_count"], 2);
    let plugin_serialized = serde_json::to_string(&plugin_summary).unwrap();
    assert!(!plugin_serialized.contains(PLUGIN_SECRET));
    assert!(!plugin_serialized.contains("PRIVATE_PLUGIN_TOKEN"));
    assert!(!plugin_serialized.contains("wc_pbind_"));

    let ssh = json!({
        "action": "register",
        "runner": "private-runner",
        "binding": "wc_sbind_u7u7u7u7u7u7u7u7u7u7uw".to_string(),
        "name": "private-ssh-name",
        "target": SSH_TARGET,
        "default_cwd": SSH_CWD
    });
    let ssh_summary = session_log_arguments_for_tool_request("manage_ssh_resource", &ssh);
    assert_eq!(ssh_summary["action"], "register");
    for key in [
        "runner_present",
        "binding_present",
        "name_present",
        "target_present",
        "default_cwd_present",
    ] {
        assert_eq!(ssh_summary[key], true, "missing SSH presence bit {key}");
    }
    let ssh_serialized = serde_json::to_string(&ssh_summary).unwrap();
    for secret in [
        SSH_TARGET,
        SSH_CWD,
        "private-runner",
        "private-ssh-name",
        "wc_sbind_",
    ] {
        assert!(!ssh_serialized.contains(secret));
    }

    let register = json!({
        "client_id": "special",
        "id": "demo",
        "name": "Demo",
        "path": PROJECT_PATH,
        "description": "PRIVATE PROJECT DESCRIPTION",
        "allow_patch": true,
        "overwrite": false
    });
    let register_summary = session_log_arguments_for_tool_request("register_project", &register);
    assert_eq!(register_summary["path_present"], true);
    assert_eq!(register_summary["description_present"], true);
    let register_serialized = serde_json::to_string(&register_summary).unwrap();
    assert!(!register_serialized.contains(PROJECT_PATH));
    assert!(!register_serialized.contains("PRIVATE PROJECT DESCRIPTION"));

    let observe_jobs = json!({
        "items": [{
            "job_id": "job-safe",
            "after_observation_token": JOB_TOKEN
        }],
        "tail_lines": 20,
        "wait_secs": 1
    });
    let job_summary = session_log_arguments_for_tool_request("observe_jobs", &observe_jobs);
    assert_eq!(job_summary["item_count"], 1);
    assert_eq!(job_summary["token_count"], 1);
    assert_eq!(job_summary["job_ids"], json!(["job-safe"]));
    assert!(!serde_json::to_string(&job_summary)
        .unwrap()
        .contains(JOB_TOKEN));
}

#[test]
fn result_audit_fails_closed_for_unknown_tools_without_mutating_business_output() {
    let output = json!({
        "secret": "UNKNOWN_RESULT_SECRET",
        "nested": {"token": "PRIVATE_RESULT_TOKEN"}
    });
    let before = output.clone();
    let projected = session_log_result_for_tool("future_unknown_tool", &output);
    assert_eq!(projected, json!({}));
    assert_eq!(output, before);
    assert_eq!(
        session_log_result_for_tool("list_agents", &output),
        json!({}),
        "retired aliases must not inherit a canonical result audit policy"
    );
    let serialized = serde_json::to_string(&projected).unwrap();
    assert!(!serialized.contains("UNKNOWN_RESULT_SECRET"));
    assert!(!serialized.contains("PRIVATE_RESULT_TOKEN"));
}

#[test]
fn agent_continuation_app_audit_omits_host_binding_and_resume_secrets() {
    let args = json!({
        "agent_id": "wc_agent_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "endpoint_id": "wc_endpoint_u7u7u7u7u7u7u7u7",
        "expected_controller_generation": 7,
        "binding_id": "wc_host_binding_PRIVATE_BINDING",
        "wake_id": "wc_wake_zMzMzMzMzMzMzMzM",
        "attempt_id": "wc_wake_attempt_3d3d3d3d3d3d3d3d"
    });
    let arguments =
        session_log_arguments_for_tool_request("prepare_agent_continuation_wake", &args);
    let arguments_text = serde_json::to_string(&arguments).unwrap();
    assert_eq!(arguments["agent_id"], args["agent_id"]);
    assert_eq!(arguments["wake_id"], args["wake_id"]);
    assert!(!arguments_text.contains("PRIVATE_BINDING"));
    assert!(!arguments_text.contains("binding_id"));

    let result = json!({
        "agent_id": args["agent_id"],
        "endpoint_id": args["endpoint_id"],
        "wake_id": args["wake_id"],
        "attempt_id": args["attempt_id"],
        "wake_revision": 9,
        "dispatch_observation": "dispatch_prepared",
        "state_changed": true,
        "app_protocol": {
            "binding_id": "wc_host_binding_PRIVATE_BINDING",
            "automatic_message": "consume_token=wc_wake_consume_PRIVATE_TOKEN\nPRIVATE MESSAGE BODY"
        }
    });
    let projected = session_log_result_for_tool("prepare_agent_continuation_wake", &result);
    let projected_text = serde_json::to_string(&projected).unwrap();
    assert_eq!(projected["wake_id"], args["wake_id"]);
    assert_eq!(projected["dispatch_observation"], "dispatch_prepared");
    for forbidden in [
        "PRIVATE_BINDING",
        "PRIVATE_TOKEN",
        "PRIVATE MESSAGE BODY",
        "automatic_message",
        "app_protocol",
    ] {
        assert!(
            !projected_text.contains(forbidden),
            "audit leaked {forbidden}"
        );
    }
}

#[test]
fn job_terminal_continuation_app_audit_omits_binding_and_private_message() {
    let wait_id = "wc_job_wait_q6urq6urq6urq6ur";
    let binding_id = format!(
        "wc_host_binding_{}",
        webcodex_core::compact::encode([0xc1; 16])
    );
    let attempt_id = format!(
        "wc_job_delivery_{}",
        webcodex_core::compact::encode([0xc2; 12])
    );
    let prepare_input = json!({
        "wait_id": wait_id,
        "binding_id": binding_id
    });
    let prepare_args =
        session_log_arguments_for_tool_request("prepare_job_terminal_continuation", &prepare_input);
    let prepare_args_text = serde_json::to_string(&prepare_args).unwrap();
    assert_eq!(prepare_args["wait_id"], wait_id);
    assert!(!prepare_args_text.contains(&binding_id));
    assert!(!prepare_args_text.contains("binding_id"));
    assert!(!prepare_args_text.contains("attempt_id"));

    let finish_input = json!({
        "wait_id": wait_id,
        "binding_id": binding_id,
        "attempt_id": attempt_id,
        "outcome": "dispatch_accepted"
    });
    let finish_args =
        session_log_arguments_for_tool_request("finish_job_terminal_continuation", &finish_input);
    let finish_args_text = serde_json::to_string(&finish_args).unwrap();
    assert_eq!(finish_args["wait_id"], wait_id);
    assert_eq!(finish_args["attempt_id"], attempt_id);
    assert_eq!(finish_args["outcome"], "dispatch_accepted");
    assert!(!finish_args_text.contains(&binding_id));
    assert!(!finish_args_text.contains("binding_id"));

    let result = json!({
        "wait_id": wait_id,
        "job_id": "wc_job_private",
        "delivery_state": "prepared",
        "attempt_id": attempt_id,
        "dispatch_observation": "dispatch_prepared",
        "state_changed": true,
        "app_protocol": {
            "automatic_message": "PRIVATE MESSAGE BODY /private/path TOKEN=secret",
            "binding_id": binding_id
        }
    });
    let projected = session_log_result_for_tool("prepare_job_terminal_continuation", &result);
    let projected_text = serde_json::to_string(&projected).unwrap();
    assert_eq!(projected["wait_id"], wait_id);
    assert_eq!(projected["attempt_id"], attempt_id);
    assert_eq!(projected["dispatch_observation"], "dispatch_prepared");
    for forbidden in [
        "PRIVATE MESSAGE BODY",
        "/private/path",
        "TOKEN=secret",
        "automatic_message",
        "app_protocol",
        "binding_id",
    ] {
        assert!(
            !projected_text.contains(forbidden),
            "Job terminal App audit leaked {forbidden}"
        );
    }
}

#[test]
fn computer_application_list_ledger_omits_names_ids_and_native_identity() {
    let output = json!({
        "applications": [{
            "application_id": "application_iavN7wEjRWeJq83v",
            "display_name": "Private App",
            "native_identity": "never-allowed"
        }],
        "count": 1,
        "truncated": false
    });
    let summary = session_log_result_for_tool("observe_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary, json!({"count": 1, "truncated": false}));
    assert!(!serialized.contains("Private App"));
    assert!(!serialized.contains("application_"));
    assert!(!serialized.contains("native_identity"));
}

#[test]
fn trace_reader_audit_result_never_persists_raw_payload() {
    let summary = session_log_result_for_tool(
        "read_tool_trace",
        &json!({
            "trace_ref": "01234567-89ab-cdef-0123-456789abcdef",
            "trace_mode": "full",
            "payload_index": 2,
            "phase": "final_response",
            "payload_bytes": 123,
            "payload_sha256": "a".repeat(64),
            "payload_available": true,
            "payload": {
                "private_token": "PRIVATE_RAW_TRACE_BODY",
                "stdout": "PRIVATE_OUTPUT"
            }
        }),
    );
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary["payload_index"], 2);
    assert_eq!(summary["phase"], "final_response");
    assert_eq!(summary["payload_bytes"], 123);
    assert!(summary.get("payload").is_none());
    assert!(!serialized.contains("PRIVATE_RAW_TRACE_BODY"));
    assert!(!serialized.contains("PRIVATE_OUTPUT"));
    assert!(!serialized.contains("private_token"));
}

#[test]
fn skill_runtime_audit_results_are_metadata_only() {
    let list = session_log_result_for_tool(
        "list_skills",
        &json!({
            "project": "agent:test:demo",
            "catalog_revision": "wc_skillcat_deadbeef",
            "total_count": 1,
            "returned_count": 1,
            "truncated": false,
            "invalid_count": 0,
            "discovery_truncated": false,
            "skills": [{"name": "PRIVATE DESCRIPTION", "description": "PRIVATE CATALOG BODY"}]
        }),
    );
    let list_serialized = serde_json::to_string(&list).unwrap();
    assert!(!list_serialized.contains("PRIVATE DESCRIPTION"));
    assert!(!list_serialized.contains("PRIVATE CATALOG BODY"));
    assert!(list.get("skills").is_none());

    let read = session_log_result_for_tool(
        "read_skill_file",
        &json!({
            "project": "agent:test:demo",
            "skill_id": "wc_skill_ASNFZ4mrze8BI0VniavN7w",
            "definition_revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "path": "SKILL.md",
            "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "text": "PRIVATE_SKILL_BODY",
            "start_line": 1,
            "end_line": 2,
            "returned_lines": 2,
            "has_more": false,
            "next_start_line": null
        }),
    );
    let read_serialized = serde_json::to_string(&read).unwrap();
    assert!(!read_serialized.contains("PRIVATE_SKILL_BODY"));
    assert!(read.get("text").is_none());
    assert_eq!(read["path"], "SKILL.md");
    assert_eq!(read["returned_lines"], 2);
}

#[test]
fn skill_management_audit_omits_paths_keys_and_package_bodies() {
    let args = session_log_arguments_for_tool_request(
        "install_skill",
        &json!({
            "project": "agent:test:demo",
            "skill_key": "demo",
            "artifact_path": "artifacts/PRIVATE_PACKAGE_NAME.zip",
            "expected_artifact_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "idempotency_key": "PRIVATE_IDEMPOTENCY_KEY",
            "activate": true,
            "expected_state_revision": "wc_skillstate_u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7s"
        }),
    );
    let args_serialized = serde_json::to_string(&args).unwrap();
    assert!(!args_serialized.contains("PRIVATE_PACKAGE_NAME"));
    assert!(!args_serialized.contains("PRIVATE_IDEMPOTENCY_KEY"));
    assert_eq!(args["artifact_path_present"], true);
    assert_eq!(args["idempotency_key_present"], true);

    let typed_args = ToolCall::SkillInstall {
        project: "agent:test:demo".to_string(),
        skill_key: "demo".to_string(),
        artifact_path: "artifacts/PRIVATE_PACKAGE_NAME.zip".to_string(),
        expected_artifact_sha256:
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        idempotency_key: "PRIVATE_IDEMPOTENCY_KEY".to_string(),
        activate: Some(true),
        expected_state_revision: Some(
            "wc_skillstate_u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7s".to_string(),
        ),
        session_id: None,
    }
    .session_log_arguments();
    let typed_serialized = serde_json::to_string(&typed_args).unwrap();
    assert!(!typed_serialized.contains("PRIVATE_PACKAGE_NAME"));
    assert!(!typed_serialized.contains("PRIVATE_IDEMPOTENCY_KEY"));
    assert_eq!(typed_args["skill_key"], "demo");
    assert_eq!(typed_args["artifact_path_present"], true);
    assert_eq!(typed_args["idempotency_key_present"], true);

    let versions = session_log_result_for_tool(
        "list_skill_versions",
        &json!({
            "project": "agent:test:demo",
            "skill_id": "wc_skill_ASNFZ4mrze8BI0VniavN7w",
            "skill_key": "demo",
            "state_revision": "wc_skillstate_zMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMw",
            "active_package_revision": "wc_skillpkg_3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d0",
            "total_count": 1,
            "offset": 0,
            "next_offset": null,
            "versions": [{
                "description": "PRIVATE_REVISION_DESCRIPTION",
                "native_store_path": "/PRIVATE/NATIVE/STORE/PATH"
            }]
        }),
    );
    let versions_serialized = serde_json::to_string(&versions).unwrap();
    assert!(!versions_serialized.contains("PRIVATE_REVISION_DESCRIPTION"));
    assert!(!versions_serialized.contains("PRIVATE/NATIVE/STORE"));
    assert!(versions.get("versions").is_none());

    let install = session_log_result_for_tool(
        "install_skill",
        &json!({
            "project": "agent:test:demo",
            "skill_id": "wc_skill_ASNFZ4mrze8BI0VniavN7w",
            "skill_key": "demo",
            "package_revision": "wc_skillpkg_3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d0",
            "definition_revision": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
            "artifact_sha256": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "file_count": 2,
            "total_bytes": 123,
            "installed": true,
            "activated": false,
            "replayed": false,
            "state_revision": "wc_skillstate_zMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMw",
            "active_package_revision": null,
            "raw_skill_body": "PRIVATE_SKILL_BODY",
            "archive_bytes": "PRIVATE_ZIP_BYTES",
            "native_store_path": "/PRIVATE/NATIVE/STORE/PATH",
            "staging_path": "/PRIVATE/STAGING/PATH"
        }),
    );
    let install_serialized = serde_json::to_string(&install).unwrap();
    for private in [
        "PRIVATE_SKILL_BODY",
        "PRIVATE_ZIP_BYTES",
        "PRIVATE/NATIVE/STORE",
        "PRIVATE/STAGING/PATH",
    ] {
        assert!(!install_serialized.contains(private), "leaked {private}");
    }
}

#[test]
fn communication_audit_omits_profile_and_message_bodies() {
    const PRIVATE_HANDLE: &str = "PRIVATE_HANDLE";
    const PRIVATE_DISPLAY: &str = "PRIVATE DISPLAY";
    const PRIVATE_DESCRIPTION: &str = "PRIVATE AGENT DESCRIPTION";
    const PRIVATE_LABEL: &str = "PRIVATE_LABEL";
    const PRIVATE_BODY: &str = "PRIVATE CONVERSATION BODY";
    const PRIVATE_KEY: &str = "PRIVATE_IDEMPOTENCY_KEY";

    let create = ToolCall::CreateAgentIdentity {
        handle: PRIVATE_HANDLE.to_string(),
        display_name: PRIVATE_DISPLAY.to_string(),
        description: Some(PRIVATE_DESCRIPTION.to_string()),
        specialty_labels: vec![PRIVATE_LABEL.to_string()],
        idempotency_key: PRIVATE_KEY.to_string(),
    }
    .session_log_arguments();
    let create_text = create.to_string();
    for private in [
        PRIVATE_HANDLE,
        PRIVATE_DISPLAY,
        PRIVATE_DESCRIPTION,
        PRIVATE_LABEL,
        PRIVATE_KEY,
    ] {
        assert!(
            !create_text.contains(private),
            "create audit leaked {private}"
        );
    }
    assert_eq!(create["description_bytes"], PRIVATE_DESCRIPTION.len());
    assert_eq!(create["specialty_label_count"], 1);
    assert_eq!(create["idempotency_key_present"], true);

    let post = ToolCall::PostConversationMessage {
        conversation_id: "wc_conv_iavN7wEjRWeJq83v".to_string(),
        body: PRIVATE_BODY.to_string(),
        author_agent_id: None,
        endpoint_id: None,
        expected_controller_generation: None,
        recipient_agent_ids: Some(vec!["wc_dagent_iavN7wEjRWeJq83v".to_string()]),
        reply_to: None,
        idempotency_key: Some(PRIVATE_KEY.to_string()),
        wake_reply_id: None,
        reply_operation_index: None,
    }
    .session_log_arguments();
    let post_text = post.to_string();
    assert!(!post_text.contains(PRIVATE_BODY));
    assert!(!post_text.contains(PRIVATE_KEY));
    assert_eq!(post["body_bytes"], PRIVATE_BODY.len());
    assert_eq!(post["recipient_count"], 1);

    let result = session_log_result_for_tool(
        "post_conversation_message",
        &json!({
            "message": {
                "message_id": "wc_cmsg_iavN7wEjRWeJq83v",
                "conversation_id": "wc_conv_iavN7wEjRWeJq83v",
                "seq": 4,
                "body": PRIVATE_BODY,
                "deliveries": [{"delivery_id": "wc_delivery_iavN7wEjRWeJq83v"}]
            },
            "replayed": false,
            "state_changed": true
        }),
    );
    assert!(!result.to_string().contains(PRIVATE_BODY));
    assert_eq!(result["seq"], 4);
    assert_eq!(result["delivery_count"], 1);

    let bootstrap = session_log_result_for_tool(
        "bootstrap_agent_conversation",
        &json!({
            "acting_agent": {
                "agent_id": "wc_dagent_iavN7wEjRWeJq83v",
                "description": PRIVATE_DESCRIPTION,
                "specialty_labels": [PRIVATE_LABEL]
            },
            "endpoint": {
                "endpoint_id": "wc_endpoint_iavN7wEjRWeJq83v",
                "controller_generation": 4,
                "client_attachment_id": "PRIVATE_HOST_ATTACHMENT"
            },
            "selected_conversation": {
                "conversation_id": "wc_conv_iavN7wEjRWeJq83v"
            },
            "inbox": {"queued_delivery_count": 2},
            "wake": {
                "wake_id": "wc_wake_iavN7wEjRWeJq83v",
                "state": "pending",
                "consume_token": "PRIVATE_CONSUME_TOKEN",
                "message_body": PRIVATE_BODY
            },
            "host_binding": {
                "adapter_kind": "host_adapter",
                "runtime_wake_capable": true,
                "production_auto_resume_available": false,
                "callback_secret": "PRIVATE_CALLBACK_SECRET"
            },
            "wake_activation": {
                "wake_id": "wc_wake_iavN7wEjRWeJq83v",
                "attempt_id": "wc_wake_attempt_iavN7wEjRWeJq83v",
                "consume_token": "PRIVATE_ACTIVATION_CONSUME_TOKEN",
                "adapter_kind": "explicit_activation"
            }
        }),
    );
    let bootstrap_text = bootstrap.to_string();
    for private in [
        PRIVATE_DESCRIPTION,
        PRIVATE_LABEL,
        PRIVATE_BODY,
        "PRIVATE_HOST_ATTACHMENT",
        "PRIVATE_CONSUME_TOKEN",
        "PRIVATE_CALLBACK_SECRET",
        "PRIVATE_ACTIVATION_CONSUME_TOKEN",
    ] {
        assert!(
            !bootstrap_text.contains(private),
            "bootstrap audit leaked {private}"
        );
    }
    assert_eq!(bootstrap["controller_generation"], 4);
    assert_eq!(bootstrap["queued_delivery_count"], 2);

    let activation_request = ToolCall::BootstrapAgentConversation {
        agent_id: "wc_dagent_iavN7wEjRWeJq83v".to_string(),
        endpoint_id: "wc_endpoint_iavN7wEjRWeJq83v".to_string(),
        expected_controller_generation: 4,
        conversation_id: None,
        wake_id: Some("wc_wake_iavN7wEjRWeJq83v".to_string()),
        activation_idempotency_key: Some(PRIVATE_KEY.to_string()),
    }
    .session_log_arguments();
    assert!(!activation_request.to_string().contains(PRIVATE_KEY));
}

#[test]
fn agent_task_attempt_ref_audit_keeps_canonical_ids_and_omits_fence() {
    const PRIVATE_FENCE: &str = "wc_agent_task_fence_PRIVATE_FENCE_MUST_NOT_PERSIST";
    let by_ref = session_log_arguments_for_tool_request(
        "start_agent_task_endpoint_continuation",
        &json!({
            "attempt_ref": "~ta4",
            "attempt_fence": PRIVATE_FENCE,
        }),
    );
    assert_eq!(by_ref["attempt_ref"], "~ta4");
    assert_eq!(by_ref["attempt_fence_present"], true);
    assert!(by_ref.get("task_id").is_none());
    assert!(!by_ref.to_string().contains(PRIVATE_FENCE));

    let typed = ToolCall::StartAgentTaskEndpointContinuation {
        attempt_ref: Some("~ta4".to_string()),
        task_id: None,
        attempt_id: None,
        assignee_agent_id: None,
        attempt_fence: Some(PRIVATE_FENCE.to_string()),
        attempt_controller_generation: None,
    }
    .session_log_arguments();
    assert_eq!(typed["attempt_ref"], "~ta4");
    assert_eq!(typed["attempt_fence_present"], true);
    assert!(typed.get("task_id").is_none());
    assert!(!typed.to_string().contains(PRIVATE_FENCE));

    let by_tuple = ToolCall::StartAgentTaskEndpointContinuation {
        attempt_ref: None,
        task_id: Some("wc_agent_task_iavN7wEjRWeJq83v".to_string()),
        attempt_id: Some("wc_agent_task_attempt_iavN7wEjRWeJq83v".to_string()),
        assignee_agent_id: Some("wc_dagent_iavN7wEjRWeJq83v".to_string()),
        attempt_fence: Some(PRIVATE_FENCE.to_string()),
        attempt_controller_generation: Some(2),
    }
    .session_log_arguments();
    assert_eq!(by_tuple["task_id"], "wc_agent_task_iavN7wEjRWeJq83v");
    assert_eq!(
        by_tuple["attempt_id"],
        "wc_agent_task_attempt_iavN7wEjRWeJq83v"
    );
    assert_eq!(by_tuple["attempt_controller_generation"], 2);
    assert_eq!(by_tuple["attempt_fence_present"], true);
    assert!(by_tuple.get("attempt_ref").is_none());
    assert!(!by_tuple.to_string().contains(PRIVATE_FENCE));

    let result = session_log_result_for_tool(
        "start_agent_task_endpoint_continuation",
        &json!({
            "execution": {
                "task_id": "wc_agent_task_iavN7wEjRWeJq83v",
                "attempt_id": "wc_agent_task_attempt_iavN7wEjRWeJq83v",
                "wake_id": "wc_wake_iavN7wEjRWeJq83v",
                "wake_state": "pending",
                "endpoint_id": null,
                "endpoint_controller_generation": null
            },
            "attempt_fence": PRIVATE_FENCE,
            "replayed": false,
            "state_changed": true
        }),
    );
    assert_eq!(result["task_id"], "wc_agent_task_iavN7wEjRWeJq83v");
    assert_eq!(
        result["attempt_id"],
        "wc_agent_task_attempt_iavN7wEjRWeJq83v"
    );
    assert_eq!(result["wake_id"], "wc_wake_iavN7wEjRWeJq83v");
    assert!(!result.to_string().contains(PRIVATE_FENCE));
}

#[test]
fn agent_task_active_turn_heartbeat_audit_omits_raw_proof_and_attempt_fence() {
    const PRIVATE_FENCE: &str = "wc_agent_task_fence_PRIVATE_FENCE_MUST_NOT_PERSIST";
    const PRIVATE_WAKE: &str = "wc_wake_PRIVATE_WAKE_MUST_NOT_PERSIST";
    const PRIVATE_TOKEN: &str = "wc_wake_consume_PRIVATE_TOKEN_MUST_NOT_PERSIST";
    let request = session_log_arguments_for_tool_request(
        "heartbeat_agent_task_attempt",
        &json!({
            "task_id": "wc_agent_task_iavN7wEjRWeJq83v",
            "attempt_id": "wc_agent_task_attempt_iavN7wEjRWeJq83v",
            "assignee_agent_id": "wc_dagent_iavN7wEjRWeJq83v",
            "attempt_fence": PRIVATE_FENCE,
            "attempt_controller_generation": 9,
            "active_turn_wake_id": PRIVATE_WAKE,
            "active_turn_consume_token": PRIVATE_TOKEN,
        }),
    );
    assert_eq!(request["attempt_fence_present"], true);
    assert_eq!(request["active_turn_proof_present"], true);
    assert_eq!(request["attempt_controller_generation"], 9);
    let request_text = request.to_string();
    for private in [PRIVATE_FENCE, PRIVATE_WAKE, PRIVATE_TOKEN] {
        assert!(
            !request_text.contains(private),
            "heartbeat audit leaked {private}"
        );
    }

    let typed = ToolCall::HeartbeatAgentTaskAttempt {
        attempt_ref: None,
        task_id: Some("wc_agent_task_iavN7wEjRWeJq83v".to_string()),
        attempt_id: Some("wc_agent_task_attempt_iavN7wEjRWeJq83v".to_string()),
        assignee_agent_id: Some("wc_dagent_iavN7wEjRWeJq83v".to_string()),
        attempt_fence: Some(PRIVATE_FENCE.to_string()),
        attempt_controller_generation: Some(9),
        active_turn_wake_id: Some(PRIVATE_WAKE.to_string()),
        active_turn_consume_token: Some(PRIVATE_TOKEN.to_string()),
    }
    .session_log_arguments();
    assert_eq!(typed["active_turn_proof_present"], true);
    let typed_text = typed.to_string();
    for private in [PRIVATE_FENCE, PRIVATE_WAKE, PRIVATE_TOKEN] {
        assert!(
            !typed_text.contains(private),
            "typed heartbeat audit leaked {private}"
        );
    }
}

#[test]
fn agent_wake_consume_audit_omits_raw_consume_token_and_payload_fields() {
    const PRIVATE_TOKEN: &str = "wc_wake_consume_PRIVATE_TOKEN_MUST_NOT_PERSIST";
    const PRIVATE_BODY: &str = "PRIVATE_WAKE_PAYLOAD_BODY";
    const PRIVATE_DESCRIPTION: &str = "PRIVATE_AGENT_DESCRIPTION";
    const PRIVATE_DIGEST: &str = "PRIVATE_PRINCIPAL_DIGEST";
    const PRIVATE_KEY: &str = "PRIVATE_IDEMPOTENCY_KEY";

    let request = session_log_arguments_for_tool_request(
        "consume_agent_wake",
        &json!({
            "agent_id": "wc_dagent_iavN7wEjRWeJq83v",
            "endpoint_id": "wc_endpoint_iavN7wEjRWeJq83v",
            "expected_controller_generation": 7,
            "wake_id": "wc_wake_iavN7wEjRWeJq83v",
            "consume_token": PRIVATE_TOKEN,
            "body": PRIVATE_BODY,
            "description": PRIVATE_DESCRIPTION,
            "principal_digest": PRIVATE_DIGEST,
            "idempotency_key": PRIVATE_KEY
        }),
    );
    assert_eq!(request, json!({}));
    let typed_request = ToolCall::ConsumeAgentWake {
        agent_id: "wc_dagent_iavN7wEjRWeJq83v".to_string(),
        endpoint_id: "wc_endpoint_iavN7wEjRWeJq83v".to_string(),
        expected_controller_generation: 7,
        wake_id: "wc_wake_iavN7wEjRWeJq83v".to_string(),
        consume_token: PRIVATE_TOKEN.to_string(),
    }
    .session_log_arguments();
    assert_eq!(typed_request["consume_token_present"], true);
    assert_eq!(typed_request["expected_controller_generation"], 7);
    assert!(!typed_request.to_string().contains(PRIVATE_TOKEN));
    let request_text = request.to_string();
    for private in [
        PRIVATE_TOKEN,
        PRIVATE_BODY,
        PRIVATE_DESCRIPTION,
        PRIVATE_DIGEST,
        PRIVATE_KEY,
    ] {
        assert!(
            !request_text.contains(private),
            "wake consume audit leaked {private}"
        );
    }

    let result = session_log_result_for_tool(
        "consume_agent_wake",
        &json!({
            "wake_id": "wc_wake_iavN7wEjRWeJq83v",
            "target_agent_id": "wc_dagent_iavN7wEjRWeJq83v",
            "state": "consumed",
            "already_consumed": false,
            "consumed_at_unix_ms": 123,
            "state_changed": true,
            "consume_token": PRIVATE_TOKEN,
            "body": PRIVATE_BODY,
            "description": PRIVATE_DESCRIPTION,
            "principal_digest": PRIVATE_DIGEST,
            "idempotency_key": PRIVATE_KEY
        }),
    );
    let result_text = result.to_string();
    for private in [
        PRIVATE_TOKEN,
        PRIVATE_BODY,
        PRIVATE_DESCRIPTION,
        PRIVATE_DIGEST,
        PRIVATE_KEY,
    ] {
        assert!(
            !result_text.contains(private),
            "wake consume result audit leaked {private}"
        );
    }
    assert_eq!(result["state"], "consumed");
    assert_eq!(result["state_changed"], true);
}

#[test]
fn memory_audit_is_metadata_only_for_search_read_set_and_delete() {
    let private_query = "PRIVATE_MEMORY_QUERY";
    let private_summary = "PRIVATE_MEMORY_SUMMARY";
    let private_body = "PRIVATE_MEMORY_BODY";
    let private_tag = "PRIVATE_MEMORY_TAG";
    let revision = format!("wc_memrev_{}", "a".repeat(64));
    let memory_id = "wc_mem_iavN7wEjRWeJq83v";

    let search_args = session_log_arguments_for_tool_request(
        "search_memory",
        &json!({
            "project": "agent:test:demo",
            "query": private_query,
            "tags": [private_tag],
            "limit": 10
        }),
    );
    let search_args_serialized = search_args.to_string();
    assert!(!search_args_serialized.contains(private_query));
    assert!(!search_args_serialized.contains(private_tag));
    assert_eq!(search_args["query_present"], true);
    assert_eq!(search_args["tag_count"], 1);

    let set_args = session_log_arguments_for_tool_request(
        "set_memory",
        &json!({
            "project": "agent:test:demo",
            "memory_key": "policy",
            "summary": private_summary,
            "body": private_body,
            "priority": "high",
            "bootstrap": true,
            "tags": [private_tag]
        }),
    );
    let set_args_serialized = set_args.to_string();
    for private in [private_summary, private_body, private_tag] {
        assert!(!set_args_serialized.contains(private));
    }
    assert_eq!(set_args["summary_present"], true);
    assert_eq!(set_args["body_present"], true);
    assert_eq!(set_args["tag_count"], 1);

    let typed_set = ToolCall::MemorySet {
        project: "agent:test:demo".to_string(),
        memory_key: "policy".to_string(),
        summary: private_summary.to_string(),
        body: Some(private_body.to_string()),
        priority: Some("high".to_string()),
        bootstrap: Some(true),
        tags: Some(vec![private_tag.to_string()]),
        expected_revision: None,
        session_id: None,
    }
    .session_log_arguments();
    let typed_set_serialized = typed_set.to_string();
    for private in [private_summary, private_body, private_tag] {
        assert!(!typed_set_serialized.contains(private));
    }

    let search_result = session_log_result_for_tool(
        "search_memory",
        &json!({
            "project": "agent:test:demo",
            "catalog_revision": format!("wc_memcat_{}", "b".repeat(64)),
            "total_count": 1,
            "returned_count": 1,
            "truncated": false,
            "memories": [{
                "memory_id": memory_id,
                "memory_key": "policy",
                "summary": private_summary,
                "tags": [private_tag],
                "revision": revision
            }]
        }),
    );
    let search_result_serialized = search_result.to_string();
    assert!(!search_result_serialized.contains(private_summary));
    assert!(!search_result_serialized.contains(private_tag));
    assert!(search_result.get("memories").is_none());

    let read_result = session_log_result_for_tool(
        "read_memory",
        &json!({
            "project": "agent:test:demo",
            "memory_id": memory_id,
            "memory_key": "policy",
            "summary": private_summary,
            "body": private_body,
            "priority": "high",
            "bootstrap": true,
            "tags": [private_tag],
            "revision": revision
        }),
    );
    let read_result_serialized = read_result.to_string();
    for private in [private_summary, private_body, private_tag] {
        assert!(!read_result_serialized.contains(private));
    }
    assert_eq!(read_result["returned_body_bytes"], private_body.len());

    let set_result = session_log_result_for_tool(
        "set_memory",
        &json!({
            "project": "agent:test:demo",
            "memory_id": memory_id,
            "memory_key": "policy",
            "revision": revision,
            "created": true,
            "state_changed": true,
            "summary": private_summary,
            "body": private_body,
            "tags": [private_tag]
        }),
    );
    let set_result_serialized = set_result.to_string();
    for private in [private_summary, private_body, private_tag] {
        assert!(!set_result_serialized.contains(private));
    }

    let delete_result = session_log_result_for_tool(
        "delete_memory",
        &json!({
            "project": "agent:test:demo",
            "memory_id": memory_id,
            "memory_key": "policy",
            "revision": revision,
            "deleted": true,
            "state_changed": true,
            "body": private_body
        }),
    );
    let private_principal_digest = format!("wc_memprincipal_{}", "d".repeat(64));
    let private_native_root = "/PRIVATE/NATIVE/MEMORY/ROOT";
    let scope_id = format!("wc_memscope_{}", "c".repeat(64));
    let catalog_revision = format!("wc_memcat_{}", "e".repeat(64));
    let purge_args = session_log_arguments_for_tool_request(
        "purge_memory_scope",
        &json!({
            "memory_scope_id": scope_id,
            "expected_catalog_revision": catalog_revision,
            "confirm": true,
            "body": private_body,
        }),
    );
    assert_eq!(purge_args, json!({}));
    assert!(!purge_args.to_string().contains(private_body));
    let typed_purge = ToolCall::MemoryScopePurge {
        memory_scope_id: scope_id.clone(),
        expected_catalog_revision: catalog_revision.clone(),
        confirm: true,
    }
    .session_log_arguments();
    assert!(typed_purge.get("confirm").is_none());
    assert_eq!(typed_purge["memory_scope_id"], scope_id);
    assert_eq!(typed_purge["expected_catalog_revision"], catalog_revision);

    let scope_list = session_log_result_for_tool(
        "list_memory_scopes",
        &json!({
            "total_count": 1,
            "returned_count": 1,
            "truncated": false,
            "scopes": [{
                "memory_scope_id": scope_id,
                "identity_state": "attributed",
                "current_status": "not_current",
                "catalog_revision": catalog_revision,
                "memory_count": 1,
                "summary": private_summary,
                "body": private_body,
                "tags": [private_tag],
                "native_root": private_native_root,
                "principal_digest": private_principal_digest
            }]
        }),
    );
    let scope_list_text = scope_list.to_string();
    assert_eq!(scope_list["total_count"], 1);
    assert_eq!(scope_list["returned_count"], 1);
    assert!(scope_list.get("scopes").is_none());
    for private in [
        private_summary,
        private_body,
        private_tag,
        private_native_root,
        private_principal_digest.as_str(),
    ] {
        assert!(
            !scope_list_text.contains(private),
            "scope-list audit leaked {private}"
        );
    }

    let purge = session_log_result_for_tool(
        "purge_memory_scope",
        &json!({
            "memory_scope_id": scope_id,
            "catalog_revision": catalog_revision,
            "purged_count": 1,
            "purged": true,
            "state_changed": true,
            "summary": private_summary,
            "body": private_body,
            "tags": [private_tag],
            "native_root": private_native_root,
            "principal_digest": private_principal_digest
        }),
    );
    let purge_text = purge.to_string();
    assert_eq!(purge["memory_scope_id"], scope_id);
    assert_eq!(purge["catalog_revision"], catalog_revision);
    assert_eq!(purge["purged_count"], 1);
    assert!(purge.get("purged").is_none());
    assert_eq!(purge["state_changed"], true);
    for private in [
        private_summary,
        private_body,
        private_tag,
        private_native_root,
        private_principal_digest.as_str(),
    ] {
        assert!(
            !purge_text.contains(private),
            "purge audit leaked {private}"
        );
    }
    assert!(!delete_result.to_string().contains(private_body));
}

#[test]
fn computer_display_list_ledger_omits_ids_and_native_topology() {
    let output = json!({
        "displays": [{
            "display_id": "display_iavN7wEjRWeJq83v",
            "width": 1920,
            "height": 1080,
            "primary": true,
            "native_identity": "PRIVATE_NATIVE_ID",
            "device_path": "PRIVATE_DEVICE_PATH",
            "global_x": -1920
        }],
        "count": 1,
        "truncated": false
    });
    let summary = session_log_result_for_tool("observe_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary, json!({"count": 1, "truncated": false}));
    assert!(!serialized.contains("display_"));
    assert!(!serialized.contains("PRIVATE_NATIVE_ID"));
    assert!(!serialized.contains("PRIVATE_DEVICE_PATH"));
    assert!(!serialized.contains("global_x"));
}

#[test]
fn computer_display_snapshot_ledger_omits_image_and_native_topology() {
    let display_id = "display_iavN7wEjRWeJq83v";
    let request = json!({
        "action": "snapshot_display",
        "client_id": "msi",
        "display_id": display_id,
        "max_width": 1024,
        "max_height": 768,
        "global_x": -1920
    });
    let request_summary = session_log_arguments_for_tool_request("observe_computer", &request);
    assert_eq!(request_summary, json!({}));
    let typed_request = ToolCall::ComputerObserve(ComputerObserveToolCall::SnapshotDisplay {
        client_id: "msi".to_string(),
        display_id: display_id.to_string(),
        max_width: Some(1024),
        max_height: Some(768),
    })
    .session_log_arguments();
    assert_eq!(typed_request["display_id"], display_id);

    let output = json!({
        "display_id": display_id,
        "snapshot_generation": 9,
        "source_width": 1920,
        "source_height": 1080,
        "width": 1024,
        "height": 576,
        "mime_type": "image/jpeg",
        "file_bytes": 1234,
        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "captured_at_unix_ms": 1_700_000_000_000u64,
        "content_base64": "PRIVATE_IMAGE_BODY",
        "native_identity": "PRIVATE_NATIVE_ID",
        "device_path": "PRIVATE_DEVICE_PATH",
        "global_x": -1920,
        "scale_factor": 1.25
    });
    let summary = session_log_result_for_tool("observe_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary["display_id"], display_id);
    assert_eq!(summary["snapshot_generation"], 9);
    assert_eq!(summary["sha256"], output["sha256"]);
    assert!(!serialized.contains("PRIVATE_IMAGE_BODY"));
    assert!(!serialized.contains("PRIVATE_NATIVE_ID"));
    assert!(!serialized.contains("PRIVATE_DEVICE_PATH"));
    assert!(!serialized.contains("global_x"));
    assert!(!serialized.contains("scale_factor"));
}

#[test]
fn computer_clipboard_ledger_omits_body_hashes_and_native_state() {
    const PRIVATE_TEXT: &str = "PRIVATE_CLIPBOARD_TEXT";
    let read_request = json!({
        "action": "read_clipboard",
        "client_id": "msi",
        "text": PRIVATE_TEXT,
        "hwnd": "PRIVATE_HWND",
    });
    let read_request_summary =
        session_log_arguments_for_tool_request("observe_computer", &read_request);
    assert_eq!(read_request_summary, json!({}));
    let typed_read = ToolCall::ComputerObserve(ComputerObserveToolCall::ReadClipboard {
        client_id: "msi".to_string(),
    })
    .session_log_arguments();
    assert_eq!(
        typed_read,
        json!({"action":"read_clipboard", "client_id":"msi"})
    );

    let write_request = json!({
        "action": "write_clipboard",
        "client_id": "msi",
        "text": PRIVATE_TEXT,
        "sha256": "PRIVATE_CLIPBOARD_HASH",
        "native_handle": "PRIVATE_HGLOBAL",
    });
    let write_request_summary =
        session_log_arguments_for_tool_request("control_computer", &write_request);
    assert_eq!(write_request_summary, json!({}));
    let typed_write = ToolCall::ComputerControl(ComputerControlToolCall::WriteClipboard {
        client_id: "msi".to_string(),
        text: PRIVATE_TEXT.to_string(),
    })
    .session_log_arguments();
    assert_eq!(typed_write["client_id"], "msi");
    assert_eq!(typed_write["text_bytes"], PRIVATE_TEXT.len());
    assert!(!typed_write.to_string().contains(PRIVATE_TEXT));

    let read_output = json!({
        "available": true,
        "text": PRIVATE_TEXT,
        "text_bytes": PRIVATE_TEXT.len(),
        "success": true,
        "error_kind": null,
        "execution_state": null,
        "sha256": "PRIVATE_CLIPBOARD_HASH",
        "hwnd": "PRIVATE_HWND",
        "native_owner": "PRIVATE_OWNER",
    });
    let read_summary = session_log_result_for_tool("observe_computer", &read_output);
    assert_eq!(read_summary["available"], true);
    assert_eq!(read_summary["text_bytes"], PRIVATE_TEXT.len());
    let read_serialized = serde_json::to_string(&read_summary).unwrap();
    for secret in [
        PRIVATE_TEXT,
        "PRIVATE_CLIPBOARD_HASH",
        "PRIVATE_HWND",
        "PRIVATE_OWNER",
    ] {
        assert!(!read_serialized.contains(secret));
    }

    let write_output = json!({
        "text_bytes": PRIVATE_TEXT.len(),
        "success": true,
        "error_kind": null,
        "execution_state": "completed",
        "state_changed": true,
        "text": PRIVATE_TEXT,
        "sha256": "PRIVATE_CLIPBOARD_HASH",
        "hglobal": "PRIVATE_HGLOBAL",
        "clipboard_owner": "PRIVATE_OWNER",
    });
    let write_summary = session_log_result_for_tool("control_computer", &write_output);
    assert_eq!(write_summary["text_bytes"], PRIVATE_TEXT.len());
    assert_eq!(write_summary["success"], true);
    let write_serialized = serde_json::to_string(&write_summary).unwrap();
    for secret in [
        PRIVATE_TEXT,
        "PRIVATE_CLIPBOARD_HASH",
        "PRIVATE_HGLOBAL",
        "PRIVATE_OWNER",
    ] {
        assert!(!write_serialized.contains(secret));
    }
}

#[test]
fn computer_pointer_ledger_keeps_only_source_space_and_opaque_lifecycle_metadata() {
    let display_id = "display_iavN7wEjRWeJq83v";
    let request = json!({
        "action": "pointer_click",
        "client_id": "msi",
        "display_id": display_id,
        "snapshot_generation": 11,
        "x": 321,
        "y": 654,
        "global_x": -1599,
        "native_identity": "PRIVATE_NATIVE_ID"
    });
    let request_summary = session_log_arguments_for_tool_request("control_computer", &request);
    assert_eq!(request_summary, json!({}));
    let typed_request = ToolCall::ComputerControl(ComputerControlToolCall::PointerClick {
        client_id: "msi".to_string(),
        display_id: display_id.to_string(),
        snapshot_generation: 11,
        x: 321,
        y: 654,
    })
    .session_log_arguments();
    assert_eq!(typed_request["display_id"], display_id);
    assert_eq!(typed_request["snapshot_generation"], 11);
    assert_eq!(typed_request["x"], 321);
    assert_eq!(typed_request["y"], 654);

    let output = json!({
        "display_id": display_id,
        "snapshot_generation": 11,
        "x": 321,
        "y": 654,
        "success": true,
        "error_kind": null,
        "execution_state": "completed",
        "state_changed": true,
        "content_base64": "PRIVATE_IMAGE_BODY",
        "native_identity": "PRIVATE_NATIVE_ID",
        "device_path": "PRIVATE_DEVICE_PATH",
        "global_x": -1599,
        "virtual_left": -1920,
        "dpi_scale": 1.25,
        "bounds": [0.0, 0.0, 1920.0, 1080.0],
        "rotation": 0.0,
        "event_source": "CombinedSessionState",
        "cursor_native_x": 160.5,
        "held_buttons": 0
    });
    let summary = session_log_result_for_tool("control_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary["display_id"], display_id);
    assert_eq!(summary["snapshot_generation"], 11);
    assert_eq!(summary["x"], 321);
    assert_eq!(summary["y"], 654);
    for secret in [
        "PRIVATE_IMAGE_BODY",
        "PRIVATE_NATIVE_ID",
        "PRIVATE_DEVICE_PATH",
        "global_x",
        "virtual_left",
        "dpi_scale",
        "bounds",
        "rotation",
        "event_source",
        "cursor_native_x",
        "held_buttons",
    ] {
        assert!(!serialized.contains(secret), "{secret}");
    }
}

#[test]
fn computer_application_launch_ledger_keeps_only_opaque_lifecycle_metadata() {
    let application_id = "application_iavN7wEjRWeJq83v";
    let output = json!({
        "application_id": application_id,
        "success": true,
        "error_kind": null,
        "execution_state": null,
        "state_changed": null,
        "native_identity": "PRIVATE_NATIVE_ID",
        "path": "C:\\Private\\app.exe",
        "display_name": "Private App"
    });
    let summary = session_log_result_for_tool("control_computer", &output);
    assert_eq!(
        summary,
        json!({
            "application_id": application_id,
            "success": true,
            "error_kind": null,
            "execution_state": null,
            "state_changed": null
        })
    );
    let serialized = serde_json::to_string(&summary).unwrap();
    assert!(!serialized.contains("PRIVATE_NATIVE_ID"));
    assert!(!serialized.contains("Private App"));
    assert!(!serialized.contains("app.exe"));
}

#[test]
fn computer_list_ledger_result_omits_window_content() {
    let output = json!({
        "windows": [{
            "surface_id": "surface_secret",
            "application": "Private App",
            "title": "Confidential Window Title",
            "width": 1200,
            "height": 800,
            "focused": true,
            "active": true
        }],
        "count": 1,
        "truncated": false
    });
    let summary = session_log_result_for_tool("observe_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary, json!({"count": 1, "truncated": false}));
    assert!(!serialized.contains("Confidential"));
    assert!(!serialized.contains("Private App"));
    assert!(!serialized.contains("surface_secret"));
}

#[test]
fn computer_accessibility_tree_ledger_result_omits_semantic_content() {
    let output = json!({
        "platform": "macos",
        "surface_id": "surface_safe",
        "nodes": [{
            "element_id": "element_secret",
            "parent_element_id": null,
            "depth": 0,
            "role": "AXWindow",
            "subrole": null,
            "title": "Private Chat",
            "description": "Confidential",
            "value": "SUPER_SECRET_MESSAGE",
            "placeholder": null,
            "enabled": true,
            "focused": false,
            "child_count": 2
        }],
        "node_count": 1,
        "truncated": true,
        "max_depth": 6,
        "max_nodes": 128
    });
    let summary = session_log_result_for_tool("observe_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary["surface_id"], "surface_safe");
    assert_eq!(summary["node_count"], 1);
    assert!(!serialized.contains("SUPER_SECRET"));
    assert!(!serialized.contains("Private Chat"));
    assert!(!serialized.contains("element_secret"));
}

#[test]
fn computer_find_elements_audit_omits_label_and_semantic_result_content() {
    let secret = "PRIVATE SEARCH TERM";
    let private_role = "PRIVATE ROLE FILTER";
    let private_subrole = "PRIVATE SUBROLE FILTER";
    let request = json!({
        "action": "find_elements",
        "client_id": "mini",
        "surface_id": "surface_safe",
        "role": private_role,
        "subrole": private_subrole,
        "label": secret,
        "focused": false,
        "limit": 4,
    });
    let request_summary = session_log_arguments_for_tool_request("observe_computer", &request);
    let request_serialized = serde_json::to_string(&request_summary).unwrap();
    assert_eq!(request_summary["client_id"], "mini");
    assert_eq!(request_summary["surface_id"], "surface_safe");
    assert_eq!(request_summary["role_present"], true);
    assert_eq!(request_summary["subrole_present"], true);
    assert_eq!(request_summary["label_present"], true);
    assert!(!request_serialized.contains(secret));
    assert!(!request_serialized.contains(private_role));
    assert!(!request_serialized.contains(private_subrole));

    let parsed_summary = ToolCall::ComputerObserve(ComputerObserveToolCall::FindElements {
        client_id: "mini".to_string(),
        surface_id: "surface_safe".to_string(),
        role: Some(private_role.to_string()),
        subrole: Some(private_subrole.to_string()),
        label: Some(secret.to_string()),
        focused: Some(false),
        enabled: None,
        limit: Some(4),
    })
    .session_log_arguments();
    let parsed_serialized = serde_json::to_string(&parsed_summary).unwrap();
    assert_eq!(parsed_summary["role_present"], true);
    assert_eq!(parsed_summary["subrole_present"], true);
    assert_eq!(parsed_summary["label_present"], true);
    assert!(!parsed_serialized.contains(secret));
    assert!(!parsed_serialized.contains(private_role));
    assert!(!parsed_serialized.contains(private_subrole));

    let output = json!({
        "platform": "macos",
        "surface_id": "surface_safe",
        "elements": [{
            "element_id": "element_secret",
            "role": "AXTextField",
            "subrole": "AXSearchField",
            "title": "Private Search",
            "description": "Confidential",
            "placeholder": secret,
            "enabled": true,
            "focused": false
        }],
        "count": 1,
        "scanned_nodes": 18,
        "truncated": false
    });
    let result_summary = session_log_result_for_tool("observe_computer", &output);
    let result_serialized = serde_json::to_string(&result_summary).unwrap();
    assert_eq!(result_summary["surface_id"], "surface_safe");
    assert_eq!(result_summary["count"], 1);
    assert_eq!(result_summary["scanned_nodes"], 18);
    assert!(!result_serialized.contains(secret));
    assert!(!result_serialized.contains("element_secret"));
    assert!(!result_serialized.contains("Private Search"));
}

#[test]
fn computer_element_state_ledger_omits_content_derived_state() {
    let request = json!({
        "action": "element_state",
        "client_id": "mini",
        "surface_id": "surface_safe",
        "element_id": "element_safe",
    });
    let request_summary = session_log_arguments_for_tool_request("observe_computer", &request);
    assert_eq!(request_summary, request);

    let output = json!({
        "platform": "macos",
        "surface_id": "surface_safe",
        "element_id": "element_safe",
        "observation_generation": 9,
        "enabled": true,
        "focused": true,
        "protected": false,
        "value_empty": false,
        "can_press": true,
        "can_focus": true,
        "can_input_text": false
    });
    let summary = session_log_result_for_tool("observe_computer", &output);
    assert_eq!(summary["surface_id"], "surface_safe");
    assert_eq!(summary["element_id"], "element_safe");
    assert_eq!(summary["observation_generation"], 9);
    for field in [
        "enabled",
        "focused",
        "protected",
        "value_empty",
        "can_press",
        "can_focus",
        "can_input_text",
    ] {
        assert!(summary.get(field).is_none(), "audit leaked {field}");
    }
}

#[test]
fn computer_activate_window_ledger_is_exact_metadata_only() {
    let request = json!({
        "action": "activate_window",
        "client_id": "mini",
        "surface_id": "surface_safe",
    });
    let request_summary = session_log_arguments_for_tool_request("control_computer", &request);
    assert_eq!(request_summary, request);

    let output = json!({
        "platform": "macos",
        "surface_id": "surface_safe",
        "success": true,
        "application": "PRIVATE APP",
        "title": "PRIVATE WINDOW"
    });
    let summary = session_log_result_for_tool("control_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary["surface_id"], "surface_safe");
    assert_eq!(summary["success"], true);
    assert!(!serialized.contains("PRIVATE APP"));
    assert!(!serialized.contains("PRIVATE WINDOW"));
}

#[test]
fn computer_control_ledger_result_is_metadata_only() {
    // Control remains metadata-only independently of CU-AX3.
    let output = json!({
        "platform": "macos",
        "surface_id": "surface_safe",
        "element_id": "element_safe",
        "action": "press",
        "success": true,
        "title": "PRIVATE CONTROL TARGET",
        "value": "SUPER_SECRET_VALUE"
    });
    let summary = session_log_result_for_tool("control_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary["surface_id"], "surface_safe");
    assert_eq!(summary["element_id"], "element_safe");
    assert_eq!(summary["action"], "press");
    assert_eq!(summary["success"], true);
    assert!(!serialized.contains("PRIVATE CONTROL TARGET"));
    assert!(!serialized.contains("SUPER_SECRET_VALUE"));
}

#[test]
fn computer_scroll_to_element_ledger_is_metadata_only() {
    let request = json!({
        "action": "scroll_to_element",
        "client_id": "mini",
        "surface_id": "surface_safe",
        "element_id": "element_safe",
    });
    let request_summary = session_log_arguments_for_tool_request("control_computer", &request);
    assert_eq!(request_summary, request);

    let output = json!({
        "platform": "macos",
        "surface_id": "surface_safe",
        "element_id": "element_safe",
        "success": true,
        "title": "PRIVATE SCROLLED TARGET",
        "value": "SUPER_SECRET_VALUE"
    });
    let summary = session_log_result_for_tool("control_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary["surface_id"], "surface_safe");
    assert_eq!(summary["element_id"], "element_safe");
    assert_eq!(summary["success"], true);
    assert!(!serialized.contains("PRIVATE SCROLLED TARGET"));
    assert!(!serialized.contains("SUPER_SECRET_VALUE"));
}

#[test]
fn computer_key_input_ledger_is_closed_metadata_only() {
    let request = json!({
        "action": "key",
        "client_id": "mini",
        "surface_id": "surface_safe",
        "key": "tab",
        "modifiers": ["shift"],
        "native_key": "MUST_NOT_PERSIST",
        "keycode": 123
    });
    let request_summary = session_log_arguments_for_tool_request("control_computer", &request);
    assert_eq!(request_summary, json!({}));
    let typed_request = ToolCall::ComputerControl(ComputerControlToolCall::Key {
        client_id: "mini".to_string(),
        surface_id: "surface_safe".to_string(),
        key: "tab".to_string(),
        modifiers: Some(vec!["shift".to_string()]),
    })
    .session_log_arguments();
    assert_eq!(typed_request["key"], "tab");
    assert_eq!(typed_request["modifiers"], json!(["shift"]));

    let output = json!({
        "platform": "macos",
        "surface_id": "surface_safe",
        "key": "tab",
        "modifiers": ["shift"],
        "success": true,
        "title": "PRIVATE FOCUSED TARGET",
        "value": "SUPER_SECRET_VALUE"
    });
    let summary = session_log_result_for_tool("control_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary["surface_id"], "surface_safe");
    assert_eq!(summary["key"], "tab");
    assert_eq!(summary["modifiers"], json!(["shift"]));
    assert_eq!(summary["success"], true);
    assert!(!serialized.contains("PRIVATE FOCUSED TARGET"));
    assert!(!serialized.contains("SUPER_SECRET_VALUE"));
}

#[test]
fn computer_text_input_request_and_result_never_persist_text() {
    let secret = "不要记录我🙂";
    let request = json!({
        "action": "input_text",
        "client_id": "mini",
        "surface_id": "surface_safe",
        "element_id": "element_safe",
        "text": secret,
    });
    let request_summary = session_log_arguments_for_tool_request("control_computer", &request);
    let request_serialized = serde_json::to_string(&request_summary).unwrap();
    assert_eq!(request_summary["client_id"], "mini");
    assert_eq!(request_summary["surface_id"], "surface_safe");
    assert_eq!(request_summary["element_id"], "element_safe");
    assert_eq!(request_summary["text_bytes"], secret.len());
    assert!(!request_serialized.contains(secret));
    assert!(request_summary.get("text").is_none());

    let typed = ToolCall::ComputerControl(ComputerControlToolCall::InputText {
        client_id: "mini".to_string(),
        surface_id: "surface_safe".to_string(),
        element_id: "element_safe".to_string(),
        text: secret.to_string(),
    });
    let typed_summary = typed.session_log_arguments();
    let typed_serialized = serde_json::to_string(&typed_summary).unwrap();
    assert_eq!(typed_summary["text_bytes"], secret.len());
    assert!(!typed_serialized.contains(secret));
    assert!(typed_summary.get("text").is_none());

    let output = json!({
        "platform": "macos",
        "surface_id": "surface_safe",
        "element_id": "element_safe",
        "text_bytes": secret.len(),
        "success": true,
        "text": secret,
        "value": secret,
    });
    let result_summary = session_log_result_for_tool("control_computer", &output);
    let result_serialized = serde_json::to_string(&result_summary).unwrap();
    assert_eq!(result_summary["text_bytes"], secret.len());
    assert_eq!(result_summary["success"], true);
    assert!(!result_serialized.contains(secret));
    assert!(result_summary.get("text").is_none());
    assert!(result_summary.get("value").is_none());
}

#[test]
fn computer_snapshot_ledger_request_omits_region_coordinates() {
    let request = json!({
        "action": "snapshot_window",
        "client_id": "mini",
        "surface_id": "surface_safe",
        "region": {"x": 111, "y": 222, "width": 333, "height": 444},
        "max_width": 800,
        "max_height": 600
    });
    let summary = session_log_arguments_for_tool_request("observe_computer", &request);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary["region_present"], true);
    assert_eq!(summary["max_width"], 800);
    assert_eq!(summary["max_height"], 600);
    assert!(summary.get("region").is_none());
    assert!(!serialized.contains("111"));
    assert!(!serialized.contains("222"));
    assert!(!serialized.contains("333"));
    assert!(!serialized.contains("444"));
}

#[test]
fn computer_snapshot_ledger_result_omits_image_and_titles() {
    // Snapshot privacy remains unchanged.
    let output = json!({
        "surface": {
            "surface_id": "surface_safe",
            "application": "Private App",
            "title": "Confidential Window Title",
            "width": 1200,
            "height": 800,
            "focused": null,
            "active": null
        },
        "source_width": 1200,
        "source_height": 800,
        "region": {"x": 111, "y": 222, "width": 900, "height": 600},
        "width": 900,
        "height": 600,
        "mime_type": "image/jpeg",
        "file_bytes": 12345,
        "sha256": "PRIVATE_SCREENSHOT_DIGEST",
        "captured_at_unix_ms": 1700000000000u64,
        "content_base64": "SUPER_SECRET_SCREENSHOT_BYTES"
    });
    let summary = session_log_result_for_tool("observe_computer", &output);
    let serialized = serde_json::to_string(&summary).unwrap();
    assert_eq!(summary["surface_id"], "surface_safe");
    assert_eq!(summary["width"], 900);
    assert_eq!(summary["height"], 600);
    assert_eq!(summary["file_bytes"], 12345);
    assert_eq!(summary["region_present"], true);
    assert!(summary.get("sha256").is_none());
    assert!(summary.get("region").is_none());
    assert!(!serialized.contains("SUPER_SECRET"));
    assert!(!serialized.contains("Confidential"));
    assert!(!serialized.contains("Private App"));
}

#[test]
fn computer_save_snapshot_audit_omits_image_digest_region_coordinates_and_session() {
    let request = json!({
        "project": "agent:target:demo",
        "path": "artifacts/ui.jpg",
        "client_id": "source-mac",
        "surface_id": "surface_safe",
        "region": {"x": 111, "y": 222, "width": 333, "height": 444},
        "max_width": 800,
        "max_height": 600,
        "session_id": "wc_sess_private"
    });
    let request_summary =
        session_log_arguments_for_tool_request("save_computer_snapshot", &request);
    let request_serialized = serde_json::to_string(&request_summary).unwrap();
    assert_eq!(request_summary["project"], "agent:target:demo");
    assert_eq!(request_summary["path"], "artifacts/ui.jpg");
    assert_eq!(request_summary["region_present"], true);
    assert!(request_summary.get("region").is_none());
    assert!(request_summary.get("session_id").is_none());
    for secret in ["111", "222", "333", "444", "wc_sess_private"] {
        assert!(!request_serialized.contains(secret));
    }

    let parsed_summary = ToolCall::ComputerSaveSnapshot {
        project: "agent:target:demo".to_string(),
        path: "artifacts/ui.jpg".to_string(),
        client_id: "source-mac".to_string(),
        surface_id: "surface_safe".to_string(),
        region: Some(ComputerSnapshotRegion {
            x: 111,
            y: 222,
            width: 333,
            height: 444,
        }),
        max_width: Some(800),
        max_height: Some(600),
        session_id: Some("wc_sess_private".to_string()),
    }
    .session_log_arguments();
    let parsed_serialized = serde_json::to_string(&parsed_summary).unwrap();
    assert_eq!(parsed_summary["region_present"], true);
    assert!(parsed_summary.get("region").is_none());
    assert!(parsed_summary.get("session_id").is_none());
    for secret in ["111", "222", "333", "444", "wc_sess_private"] {
        assert!(!parsed_serialized.contains(secret));
    }

    let output = json!({
        "project": "agent:target:demo",
        "path": "artifacts/ui.jpg",
        "client_id": "source-mac",
        "surface_id": "surface_safe",
        "source_width": 1200,
        "source_height": 800,
        "region": {"x": 111, "y": 222, "width": 900, "height": 600},
        "width": 900,
        "height": 600,
        "mime_type": "image/jpeg",
        "file_bytes": 12345,
        "sha256": "PRIVATE_SCREENSHOT_DIGEST",
        "saved": true,
        "content_base64": "SUPER_SECRET_SCREENSHOT_BYTES",
        "surface": {"application": "Private App", "title": "Confidential"}
    });
    let result_summary = session_log_result_for_tool("save_computer_snapshot", &output);
    let result_serialized = serde_json::to_string(&result_summary).unwrap();
    assert_eq!(result_summary["saved"], true);
    assert_eq!(result_summary["file_bytes"], 12345);
    assert_eq!(result_summary["region_present"], true);
    assert!(result_summary.get("sha256").is_none());
    assert!(result_summary.get("region").is_none());
    assert!(!result_serialized.contains("PRIVATE_SCREENSHOT_DIGEST"));
    assert!(!result_serialized.contains("SUPER_SECRET"));
    assert!(!result_serialized.contains("Private App"));
    assert!(!result_serialized.contains("Confidential"));
}
#[test]
fn coding_agent_audit_is_body_free_for_requests_and_observations() {
    const PROMPT: &str = "PRIVATE_ACP_PROMPT_DO_NOT_PERSIST";
    const IDEMPOTENCY: &str = "PRIVATE_ACP_IDEMPOTENCY_KEY";
    const MESSAGE: &str = "PRIVATE_AGENT_MESSAGE_BODY";
    const REASONING: &str = "PRIVATE_REASONING_BODY";
    const TOOL_LABEL: &str = "PRIVATE_TOOL_LABEL";
    const TOKEN: &str = "PRIVATE_OBSERVATION_TOKEN";

    let request = json!({
        "project": "agent:special:demo",
        "provider_id": "codex",
        "idempotency_key": IDEMPOTENCY,
        "instruction": PROMPT,
        "config": {"mode": "agent"},
        "timeout_secs": 60,
        "recording_session_id": "wc_sess_safe"
    });
    let request_summary = session_log_arguments_for_tool_request("start_coding_agent", &request);
    assert_eq!(request_summary, json!({}));

    let typed_request = ToolCall::CodingAgentStart {
        project: "agent:special:demo".to_string(),
        provider_id: "codex".to_string(),
        idempotency_key: IDEMPOTENCY.to_string(),
        instruction: PROMPT.to_string(),
        config: Some(std::collections::BTreeMap::from([(
            "mode".to_string(),
            webcodex_core::coding_agent::CodingAgentConfigValue::String("agent".to_string()),
        )])),
        timeout_secs: Some(60),
        recording_session_id: Some("wc_sess_safe".to_string()),
        context_session_id: Some("wc_sess_context".to_string()),
    }
    .session_log_arguments();
    let request_serialized = serde_json::to_string(&typed_request).unwrap();
    assert_eq!(typed_request["instruction_bytes"], PROMPT.len());
    assert_eq!(typed_request["config_count"], 1);
    assert_eq!(typed_request["idempotency_key_present"], true);
    assert!(!request_serialized.contains(PROMPT));
    assert!(!request_serialized.contains(IDEMPOTENCY));
    assert!(!request_serialized.contains("agent\""));
    assert!(typed_request.get("recording_session_id").is_none());
    assert!(!request_serialized.contains("wc_sess_safe"));
    assert_eq!(typed_request["context_session_present"], true);
    assert!(!request_serialized.contains("wc_sess_context"));

    let observe_request = json!({
        "run_id": "wc_agent_run_safe",
        "after_observation_token": TOKEN,
        "wait_secs": 3
    });
    let observe_request_summary =
        session_log_arguments_for_tool_request("observe_coding_agent", &observe_request);
    let observe_request_serialized = serde_json::to_string(&observe_request_summary).unwrap();
    assert_eq!(observe_request_summary["token_present"], true);
    assert!(!observe_request_serialized.contains(TOKEN));

    let output = json!({
        "run_id": "wc_agent_run_safe",
        "project": "agent:special:demo",
        "provider_id": "codex",
        "state": "running",
        "execution_state": "started",
        "events": [
            {"sequence": 1, "kind": "agent_message", "text": MESSAGE, "label": null, "status": null, "usage": null},
            {"sequence": 2, "kind": "reasoning", "text": REASONING, "label": null, "status": null, "usage": null},
            {"sequence": 3, "kind": "tool_activity", "text": null, "label": TOOL_LABEL, "status": "running", "usage": null}
        ],
        "observation_token": TOKEN,
        "has_more": false,
        "history_lost": false,
        "first_retained_sequence": 1,
        "terminal": null,
        "recovery_kind": "reobserve"
    });
    let result_summary = session_log_result_for_tool("observe_coding_agent", &output);
    let result_serialized = serde_json::to_string(&result_summary).unwrap();
    assert_eq!(result_summary["event_count"], 3);
    assert_eq!(
        result_summary["event_body_bytes"],
        MESSAGE.len() + REASONING.len()
    );
    for private in [MESSAGE, REASONING, TOOL_LABEL, TOKEN] {
        assert!(!result_serialized.contains(private));
    }
    assert!(result_summary.get("events").is_none());
    assert!(result_summary.get("observation_token").is_none());
}
