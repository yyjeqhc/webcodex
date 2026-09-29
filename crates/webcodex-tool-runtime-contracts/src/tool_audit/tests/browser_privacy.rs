use super::*;
use serde_json::json;

#[test]
fn browser_effect_request_audit_drops_sensitive_text_and_url() {
    let text_secret = "PASSWORD_SECRET_123";
    let text = session_log_arguments_for_tool_request(
        "browser_act",
        &json!({
            "action":"input_text",
            "client_id":"msi",
            "browser_id":"browser_abcdefghijklmnop",
            "page_id":"page_abcdefghijklmnop",
            "element_id":"element_abcdefghijklmnop",
            "text": text_secret
        }),
    );
    assert_eq!(text["text_present"], true);
    assert_eq!(text["text_bytes"], text_secret.len());
    let serialized = serde_json::to_string(&text).unwrap();
    assert!(!serialized.contains(text_secret));
    assert!(text.get("text").is_none());

    let option_secret = "PRIVATE_OPTION_SECRET";
    let option = session_log_arguments_for_tool_request(
        "browser_act",
        &json!({
            "action":"select_option",
            "client_id":"msi",
            "browser_id":"browser_abcdefghijklmnop",
            "page_id":"page_abcdefghijklmnop",
            "element_id":"element_abcdefghijklmnop",
            "option":option_secret
        }),
    );
    assert_eq!(option["option_present"], true);
    assert_eq!(option["option_bytes"], option_secret.len());
    assert!(!serde_json::to_string(&option)
        .unwrap()
        .contains(option_secret));
    assert!(option.get("option").is_none());

    let value_secret = "PRIVATE_VALUE_SECRET";
    let value = session_log_arguments_for_tool_request(
        "browser_act",
        &json!({
            "action":"set_value",
            "client_id":"msi",
            "browser_id":"browser_abcdefghijklmnop",
            "page_id":"page_abcdefghijklmnop",
            "element_id":"element_abcdefghijklmnop",
            "value":value_secret
        }),
    );
    assert_eq!(value["value_present"], true);
    assert_eq!(value["value_bytes"], value_secret.len());
    assert!(!serde_json::to_string(&value)
        .unwrap()
        .contains(value_secret));
    assert!(value.get("value").is_none());

    let private_path = "private/resume-SECRET.pdf";
    let upload = session_log_arguments_for_tool_request(
        "browser_act",
        &json!({
            "action":"upload_file",
            "client_id":"msi",
            "browser_id":"browser_abcdefghijklmnop",
            "page_id":"page_abcdefghijklmnop",
            "element_id":"element_abcdefghijklmnop",
            "project":"agent:msi:resume",
            "path":private_path
        }),
    );
    assert_eq!(upload["path_present"], true);
    assert_eq!(upload["path_bytes"], private_path.len());
    assert_eq!(upload["project"], "agent:msi:resume");
    assert!(!serde_json::to_string(&upload)
        .unwrap()
        .contains(private_path));
    assert!(upload.get("path").is_none());

    let private_url = "https://example.test/path?token=URL_QUERY_SECRET";
    let navigate = session_log_arguments_for_tool_request(
        "browser_act",
        &json!({
            "action":"navigate",
            "client_id":"msi",
            "browser_id":"browser_abcdefghijklmnop",
            "page_id":"page_abcdefghijklmnop",
            "url": private_url
        }),
    );
    assert_eq!(navigate["url_present"], true);
    assert!(!serde_json::to_string(&navigate)
        .unwrap()
        .contains(private_url));
    assert!(navigate.get("url").is_none());
}

#[test]
fn browser_observation_audit_keeps_only_bounded_metadata() {
    let output = json!({
        "execution_state":"completed",
        "state_changed":false,
        "browser_id":"browser_abcdefghijklmnop",
        "page_id":"page_abcdefghijklmnop",
        "node_count":1,
        "retained_count":200,
        "truncated":true,
        "console_retained":200,
        "console_count":2,
        "console_truncated":true,
        "network_retained":300,
        "network_count":1,
        "network_truncated":false,
        "pages":[{"title":"PAGE_BODY_SECRET","url":"https://example.test/?secret=QUERY_SECRET"}],
        "nodes":[{"role":"textbox","name":"AX_BODY_SECRET","value":"FORM_VALUE_SECRET","element_id":"element_abcdefghijklmnop"}],
        "entries":[{"level":"error","text":"CONSOLE_BODY_SECRET"}],
        "console":[{"level":"error","text":"DIAGNOSTIC_CONSOLE_SECRET"}],
        "network":[{"method":"GET","url":"https://example.test/?secret=NETWORK_SECRET"}],
        "content_base64":"BASE64_IMAGE_SECRET",
        "raw_dom":"RAW_DOM_SECRET",
        "raw_ax":"RAW_AX_SECRET",
        "debugger_url":"DEBUGGER_SECRET"
    });
    let projected = session_log_result_for_tool("browser_observe", &output);
    assert_eq!(projected["node_count"], 1);
    assert_eq!(projected["page_count"], 1);
    assert_eq!(projected["projected_node_count"], 1);
    assert_eq!(projected["retained_count"], 200);
    assert_eq!(projected["console_retained"], 200);
    assert_eq!(projected["console_count"], 2);
    assert_eq!(projected["console_truncated"], true);
    assert_eq!(projected["network_retained"], 300);
    assert_eq!(projected["network_count"], 1);
    assert_eq!(projected["network_truncated"], false);
    let serialized = serde_json::to_string(&projected).unwrap();
    for private in [
        "PAGE_BODY_SECRET",
        "QUERY_SECRET",
        "AX_BODY_SECRET",
        "FORM_VALUE_SECRET",
        "CONSOLE_BODY_SECRET",
        "DIAGNOSTIC_CONSOLE_SECRET",
        "NETWORK_SECRET",
        "BASE64_IMAGE_SECRET",
        "RAW_DOM_SECRET",
        "RAW_AX_SECRET",
        "DEBUGGER_SECRET",
    ] {
        assert!(!serialized.contains(private), "audit leaked {private}");
    }
    assert!(projected.get("pages").is_none());
    assert!(projected.get("nodes").is_none());
    assert!(projected.get("content_base64").is_none());
    assert!(projected.get("entries").is_none());
    assert!(projected.get("console").is_none());
    assert!(projected.get("network").is_none());
}

#[test]
fn browser_control_result_audit_drops_page_text_and_url() {
    let output = json!({
        "execution_state":"completed",
        "state_changed":true,
        "browser_id":"browser_abcdefghijklmnop",
        "page_id":"page_abcdefghijklmnop",
        "title":"PRIVATE_TITLE",
        "url":"https://example.test/?secret=PRIVATE_QUERY",
        "value":"PRIVATE_FORM_VALUE"
    });
    let projected = session_log_result_for_tool("browser_act", &output);
    let serialized = serde_json::to_string(&projected).unwrap();
    for private in ["PRIVATE_TITLE", "PRIVATE_QUERY", "PRIVATE_FORM_VALUE"] {
        assert!(!serialized.contains(private));
    }
}
