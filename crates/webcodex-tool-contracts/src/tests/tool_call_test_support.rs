use crate::{registered_tool_specs, ToolSpec};
use serde_json::{json, Value};

const SAMPLE_PROJECT: &str = "agent:oe:private-drop";
const UNIT_TOOL_FIXTURES: &[&str] = &[
    "list_tools",
    "list_projects",
    "list_runners",
    "runtime_status",
];

pub(super) fn sample_tool_args(name: &str) -> Value {
    let spec = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .unwrap_or_else(|| panic!("missing tool spec for {name}"));
    sample_tool_args_for_spec(&spec)
}

fn sample_tool_args_for_spec(spec: &ToolSpec) -> Value {
    let required = spec.input_schema["required"]
        .as_array()
        .unwrap_or_else(|| panic!("{} schema should list required fields", spec.name));
    if required.is_empty() && UNIT_TOOL_FIXTURES.contains(&spec.name.as_str()) {
        return Value::Null;
    }

    let mut args: serde_json::Map<String, Value> = required
        .iter()
        .map(|field| {
            let field = field
                .as_str()
                .unwrap_or_else(|| panic!("{} required field should be a string", spec.name));
            (field.to_string(), sample_field_value(field))
        })
        .collect();
    match spec.name.as_str() {
        "work_on_project" => {
            args.insert("project".to_string(), json!(SAMPLE_PROJECT));
        }
        "update_goal" => {
            args.insert("expected_revision".to_string(), json!(1));
        }
        "service_deploy" | "service_rollback" => {
            args.insert("expected_revision".to_string(), json!(1));
        }
        "observe_jobs" => {
            args.insert("items".to_string(), json!([{"job_id": "job_123"}]));
        }
        "search_and_read" => {
            args.insert("query".to_string(), json!({"pattern": "fn main"}));
        }
        "browser_observe" => {
            args.insert("action".to_string(), json!("targets"));
        }
        "browser_act" => {
            args.insert("action".to_string(), json!("launch"));
            args.insert("client_id".to_string(), json!("oe"));
        }
        "computer_observe" => {
            args.insert("action".to_string(), json!("targets"));
        }
        "computer_control" => {
            args.insert("action".to_string(), json!("focus"));
            args.insert("client_id".to_string(), json!("oe"));
            args.insert("surface_id".to_string(), json!("surface_test"));
            args.insert("element_id".to_string(), json!("element_test"));
        }
        "plugin_tool" => {
            args.insert("action".to_string(), json!("list"));
        }
        "project_artifact" => {
            args.insert("action".to_string(), json!("metadata"));
        }
        "ssh_resource" => {
            args.insert("action".to_string(), json!("list"));
            args.insert("runner".to_string(), json!("runner-a"));
        }
        _ => {}
    }
    Value::Object(args)
}

fn sample_field_value(field: &str) -> Value {
    match field {
        "project" => json!(SAMPLE_PROJECT),
        "command" => json!("true"),
        "executable" => json!("git"),
        "language" => json!("sh"),
        "script" => json!("true"),
        "patch" => json!("diff --git a/a b/a\n"),
        "paths" => json!(["old.txt"]),
        "items" => json!([{"path": "src/lib.rs"}]),
        "queries" => json!([{"pattern": "fn main"}]),
        "path" => json!("src/lib.rs"),
        "old" | "old_text" => json!("a"),
        "new" | "new_text" => json!("b"),
        "pattern" => json!("fn main"),
        "text" => json!("// hi\n"),
        "content" => json!("fn main() {}\n"),
        "instruction" => json!("implement the requested change"),
        "objective" => json!("Preserve durable high-level intent without execution authority."),
        "title" => json!("Durable agent work"),
        "include_project_instructions" | "include_workflow_guidance" => json!(false),
        "content_base64" => json!("AA=="),
        "openaiFileIdRefs" => json!([{
            "download_url": "https://files.oaiusercontent.com/test",
            "file_id": "file_test"
        }]),
        "start_line" | "end_line" | "line" | "column" | "offset" => json!(1),
        "upload_id" => json!("wc_upload_test_1"),
        "edits" => json!([{"kind": "replace_exact", "old_text": "a", "new_text": "b"}]),
        "changes" => json!([{
            "kind": "edit",
            "path": "src/lib.rs",
            "expected_read_revision": 3817291045227_u64,
            "edits": [{"kind": "replace_exact", "old_text": "a", "new_text": "b"}]
        }]),
        "prompt" => json!("summarize"),
        "query" => json!("ToolRuntime"),
        "diff" => json!("diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n"),
        "job_id" => json!("job_123"),
        "idempotency_key" => json!("sample-detached-key"),
        "operation" => json!("restart"),
        "receipt_id" => json!("wc_deploy_1234567890abcdef1234567890abcdef"),
        "target_manifest" => json!({
            "version": "0.4.1",
            "git_commit": "a".repeat(40),
            "artifacts": []
        }),
        "handle" => json!("reviewer"),
        "display_name" => json!("Reviewer"),
        "agent_id" => json!("wc_dagent_qqqqqqqqqqqqqqqq".to_string()),
        "assignee_agent_id" => json!("wc_dagent_qqqqqqqqqqqqqqqq".to_string()),
        "task_id" => json!("wc_agent_task_ERERERERERERERER".to_string()),
        "wait_id" => json!("wc_agent_wait_ZmZmZmZmZmZmZmZm".to_string()),
        "events" => json!([{
            "kind": "agent_task_terminal",
            "task_id": "wc_agent_task_ERERERERERERERER".to_string()
        }]),
        "goal_id" => json!("wc_goal_AAAAAAAAAAAAAAAA".to_string()),
        "attempt_id" => json!("wc_agent_task_attempt_IiIiIiIiIiIiIiIi".to_string()),
        "attempt_fence" => json!("wc_agent_task_fence_MzMzMzMzMzMzMzMzMzMzMw".to_string()),
        "attempt_controller_generation" => json!(1),
        "outcome" => json!("succeeded"),
        "agent_ids" => json!(["wc_dagent_qqqqqqqqqqqqqqqq".to_string()]),
        "endpoint_id" => json!("wc_endpoint_u7u7u7u7u7u7u7u7".to_string()),
        "conversation_id" => json!("wc_conv_zMzMzMzMzMzMzMzM".to_string()),
        "delivery_ids" => json!(["wc_delivery_3d3d3d3d3d3d3d3d".to_string()]),
        "expected_profile_revision" => json!(1),
        "expected_controller_generation" => json!(1),
        "wake_id" => json!("wc_wake_7u7u7u7u7u7u7u7u".to_string()),
        "consume_token" => json!("wc_wake_consume______________________w".to_string()),
        "host" => json!("ChatGPT"),
        "body" => json!("hello"),
        "provider_id" => json!("codex"),
        "run_id" => json!("wc_agent_run_sample_1234"),
        "shell_id" => json!("wc_shell_123"),
        "session_id" => json!(format!("wc_sess_{}", "1".repeat(32))),
        "source" => json!("text('ok');"),
        "checkpoint_id" => json!("wc_ckpt_1234"),
        "confirm" => json!(true),
        "draining" => json!(true),
        "client_id" => json!("oe"),
        "application_id" => json!("application_qqqqqqqqqqqqqqqq".to_string()),
        "display_id" => json!("display_qqqqqqqqqqqqqqqq".to_string()),
        "snapshot_generation" => json!(1),
        "x" | "y" => json!(0),
        "surface_id" => json!("surface_test"),
        "element_id" => json!("element_test"),
        "action" => json!("focus"),
        "key" => json!("tab"),
        "id" => json!("private-drop"),
        "base_commit" => json!("a".repeat(40)),
        "head_commit" => json!("b".repeat(40)),
        "expected_head" => json!("a".repeat(40)),
        "expected_generation" => json!(1),
        "expected_revision" => json!(format!("sha256:{}", "a".repeat(64))),
        "name" => json!("Private Drop"),
        "kind" => json!("note"),
        "message" => json!("hello"),
        "answer" => json!("done"),
        "completion_key" => json!("sample-completion-key"),
        "expected_assignment_fence" => json!(format!("wsa2_{}", "A".repeat(22))),
        "message_id" => json!("wc_msg_0001"),
        "peer_id" => json!(format!("wc_peer_{}", "a".repeat(32))),
        "execution_context" => json!({}),
        "skill_id" => json!("wc_skill_EREREREREREREREREREREQ"),
        "expected_definition_revision" => json!("a".repeat(64)),
        other => panic!("missing sample value for required field {other}"),
    }
}

pub(super) fn sample_tool_args_with_session(name: &str) -> Value {
    let mut args = sample_tool_args(name);
    let obj = args
        .as_object_mut()
        .unwrap_or_else(|| panic!("{name} does not accept object arguments"));
    obj.insert(
        "session_id".to_string(),
        Value::String("wc_sess_accessor".to_string()),
    );
    args
}
