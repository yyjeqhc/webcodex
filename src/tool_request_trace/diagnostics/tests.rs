use super::*;

#[test]
fn bounded_requests_keep_actual_selectors_without_body_or_credential_values() {
    let source = "large-private-script".repeat(30_000);
    let data = request(
        "call_runtime_tool",
        &json!({"tool":"run_script", "arguments": {
        "project":"agent:special:demo", "cwd":"src", "language":"python", "script":source,
        "args":["one","two"], "env":{"SECRET":"never-copy"}, "token":"never-copy"},
        "_wc":{"context":["project.instructions"],"record":"~s12"}}),
    )
    .unwrap();
    assert_eq!(data["value"]["entry_tool"], "call_runtime_tool");
    assert_eq!(data["value"]["arguments"]["cwd"], "src");
    assert_eq!(data["value"]["arguments"]["script"]["bytes"], source.len());
    assert_eq!(data["value"]["invocation"]["record"], "~s12");
    let bytes = serde_json::to_vec(&data).unwrap();
    assert!(bytes.len() <= MAX_DIAGNOSTIC_BYTES);
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("large-private-script"));
    assert!(!text.contains("never-copy"));
    assert!(request("read_tool_trace", &json!({"trace_ref":"secret"})).is_none());
    assert!(request("get_work_result_state", &json!({})).is_none());
}

#[test]
fn context_is_diagnostic_even_on_nonselected_tools_and_logs_are_not_copied() {
    let requested = request(
        "get_runtime_status",
        &json!({"_wc":{"context":["webcodex.workflow"]}}),
    )
    .unwrap();
    assert_eq!(
        requested["value"]["arguments"]["omitted"],
        "tool_not_selected"
    );
    let receipt = result("get_runtime_status", &json!({"success":true,"output":{"context_projection":{"materials":[{"key":"webcodex.workflow","status":"unavailable","reason_code":"context_projection_budget_exceeded"}]}}})).unwrap();
    assert_eq!(
        receipt["value"]["output"]["context"]["materials"][0]["reason_code"],
        "context_projection_budget_exceeded"
    );
    let logs = result("observe_jobs", &json!({"success":true,"output":{"items":[{"stdout_tail":"log-body-sentinel","stderr_tail":"log-body-sentinel"}]}})).unwrap();
    assert!(!logs.to_string().contains("log-body-sentinel"));
    assert!(request(
        "read_tool_trace",
        &json!({"_wc":{"context":["project.instructions"]}})
    )
    .is_none());
}

#[test]
fn malformed_context_is_captured_before_parsing_and_limits_are_explicit() {
    let data = request(
        "work_on_project",
        &json!({"client_id":"special","path":"/root/git/demo","_wc":{"context":"wrong-shape"}}),
    )
    .unwrap();
    assert_eq!(data["value"]["invocation"]["context"], "wrong-shape");
    let data = arguments(
        "run_process",
        &json!({"args":vec!["测".repeat(20_000);256]}),
    )
    .unwrap();
    assert_eq!(data["truncated"], true);
    assert!(serde_json::to_vec(&data).unwrap().len() <= MAX_DIAGNOSTIC_BYTES);
}

#[test]
fn final_context_receipt_distinguishes_loaded_body_truncation_and_budget_omission() {
    let data=result("work_on_project", &json!({"result":{"structuredContent":{"success":true,"output":{
        "semantic_navigation":{"status":"not_observed","available":null},
        "context_projection":{"truncated":true,"materials":[
            {"key":"project.instructions","status":"available","projection":{"status":"loaded","content_included":true,"truncated":true,"sources":[{"path":"AGENTS.md","source_scope":"project","fingerprint":"version-a","content":"body-not-to-persist","truncated":true,"read_more":{"path":"AGENTS.md","start_line":60}}]}},
            {"key":"webcodex.workflow","status":"unavailable","reason_code":"context_projection_budget_exceeded"}
        ]}
    }}}})).unwrap();
    let materials = &data["value"]["output"]["context"]["materials"];
    assert_eq!(materials[0]["content_included"], true);
    assert_eq!(materials[0]["sources"][0]["returned_bytes"], 19);
    assert_eq!(materials[0]["sources"][0]["fingerprint"], "version-a");
    assert_eq!(
        materials[1]["reason_code"],
        "context_projection_budget_exceeded"
    );
    assert_eq!(
        data["value"]["output"]["semantic_navigation"]["status"],
        "not_observed"
    );
    assert!(!data.to_string().contains("body-not-to-persist"));
}
