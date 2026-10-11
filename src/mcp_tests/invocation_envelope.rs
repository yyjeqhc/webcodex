use super::*;
use crate::tool_runtime::context_projection::{
    MAX_CONTEXT_REQUEST_ITEMS, MAX_CONTEXT_REQUEST_KEY_CHARS,
};
use crate::tool_runtime::sessions::{
    MAX_MESSAGE_RESOLUTION_CHARS, MAX_TOOL_CALL_ACK_MESSAGE_IDS, MAX_TOOL_CALL_ACK_REF_CHARS,
};
use crate::tool_runtime::window_collaboration::MAX_WINDOW_REPLY_CHARS;

const MESSAGE_ID: &str = "wc_msg_abcd-efgh_ijklmn";

fn parse(envelope: Value) -> Result<crate::mcp::tools::McpInvocationEnvelope, String> {
    let business = json!({"project": "proj", "items": [{"path": "src/lib.rs"}]});
    let mut arguments = business.clone();
    arguments["_wc"] = envelope;
    let result = parse_mcp_invocation_envelope("read_files", &mut arguments, true);
    assert_eq!(
        arguments, business,
        "invocation metadata must be stripped once"
    );
    crate::tool_runtime::ToolCall::from_tool_name("read_files", arguments)
        .expect("business parsing must not receive invocation metadata");
    result
}

#[test]
fn invocation_envelope_normalizes_metadata_without_changing_business_arguments() {
    let invocation = parse(json!({
        "record": "  session-selector  ",
        "ack": [format!("  {MESSAGE_ID}  "), MESSAGE_ID, "wc_msg_0123456789abcdef"],
        "ack_ref": "  wc_ack1_example  ",
        "resolve": {"message_id": MESSAGE_ID, "resolution": "  handled  "},
        "reply": {"reply_to": MESSAGE_ID, "message": "  done  "},
        "context": [" project.instructions ", "future.material", "project.instructions"]
    }))
    .unwrap();
    assert_eq!(
        invocation.recording_session_selector.as_deref(),
        Some("session-selector")
    );
    let metadata = invocation.metadata;
    assert_eq!(
        metadata.ack_session_message_ids,
        [MESSAGE_ID, "wc_msg_0123456789abcdef"]
    );
    assert_eq!(metadata.ack_ref.as_deref(), Some("wc_ack1_example"));
    let resolution = metadata.session_message_resolution.unwrap();
    assert_eq!(resolution.message_id, MESSAGE_ID);
    assert_eq!(resolution.resolution, "handled");
    let reply = metadata.window_reply.unwrap();
    assert_eq!(reply.reply_to_message_id, MESSAGE_ID);
    assert_eq!(reply.message, "done");
    assert_eq!(
        metadata.context_request,
        ["project.instructions", "future.material"]
    );
}

#[test]
fn invocation_envelope_missing_fields_default_but_explicit_null_is_invalid() {
    let absent = parse(json!({})).unwrap();
    assert!(absent.recording_session_selector.is_none());
    assert!(absent.metadata.ack_session_message_ids.is_empty());
    assert!(absent.metadata.ack_ref.is_none());
    assert!(absent.metadata.session_message_resolution.is_none());
    assert!(absent.metadata.window_reply.is_none());
    assert!(absent.metadata.context_request.is_empty());
    assert!(absent.metadata.control.is_none());
    assert!(!absent.metadata.compact_execution);

    for field in [
        "record", "ack", "ack_ref", "resolve", "reply", "context", "control",
    ] {
        assert!(parse(json!({field: null})).is_err(), "{field}");
    }
}

#[test]
fn invocation_envelope_rejects_malformed_fields_with_canonical_paths() {
    for (envelope, expected) in [
        (
            json!({"record": "  "}),
            "field '_wc.record' must be a non-empty string",
        ),
        (
            json!({"record": 12}),
            "field '_wc.record' must be a non-empty string",
        ),
        (
            json!({"ack": "invalid"}),
            "field '_wc.ack' must be an array of wc_msg_* ids",
        ),
        (
            json!({"ack": [1]}),
            "field '_wc.ack' must contain only wc_msg_* strings",
        ),
        (
            json!({"ack": ["not-a-message-id"]}),
            "field '_wc.ack' must contain only valid wc_msg_* ids",
        ),
        (
            json!({"ack_ref": ["wc_ack1_example"]}),
            "field '_wc.ack_ref' must be a non-empty bounded string",
        ),
        (
            json!({"ack_ref": "  "}),
            "field '_wc.ack_ref' must be a non-empty bounded string",
        ),
        (
            json!({"resolve": []}),
            "field '_wc.resolve' must be an object with message_id and resolution",
        ),
        (
            json!({"resolve": {"message_id": MESSAGE_ID, "resolution": "done", "extra": true}}),
            "field '_wc.resolve' accepts exactly message_id and resolution",
        ),
        (
            json!({"resolve": {"message_id": 1, "resolution": "done"}}),
            "_wc.resolve.message_id must be a wc_msg_* string",
        ),
        (
            json!({"resolve": {"message_id": "bad-id", "resolution": "done"}}),
            "_wc.resolve.message_id must be a valid wc_msg_* id",
        ),
        (
            json!({"resolve": {"message_id": MESSAGE_ID, "resolution": false}}),
            "_wc.resolve.resolution must be a string",
        ),
        (
            json!({"resolve": {"message_id": MESSAGE_ID, "resolution": "  "}}),
            "_wc.resolve.resolution must not be empty",
        ),
        (
            json!({"reply": []}),
            "field '_wc.reply' must be an object with reply_to and message",
        ),
        (
            json!({"reply": {"reply_to": MESSAGE_ID}}),
            "field '_wc.reply' accepts exactly reply_to and message",
        ),
        (
            json!({"reply": {"reply_to": 1, "message": "done"}}),
            "_wc.reply.reply_to must be a wc_msg_* string",
        ),
        (
            json!({"reply": {"reply_to": "bad-id", "message": "done"}}),
            "_wc.reply.reply_to must be a valid wc_msg_* id",
        ),
        (
            json!({"reply": {"reply_to": MESSAGE_ID, "message": false}}),
            "_wc.reply.message must be a string",
        ),
        (
            json!({"reply": {"reply_to": MESSAGE_ID, "message": "  "}}),
            "_wc.reply.message must not be empty",
        ),
        (
            json!({"context": "project.instructions"}),
            "field '_wc.context' must be an array of bounded context material keys",
        ),
        (
            json!({"context": [false]}),
            "field '_wc.context' must contain only strings",
        ),
    ] {
        assert_eq!(parse(envelope).unwrap_err(), expected);
    }
    for key in ["", "bad key", "bad\nkey", "bad\0key"] {
        assert_eq!(parse(json!({"context": [key]})).unwrap_err(), format!(
            "field '_wc.context' keys must be non-empty, at most {MAX_CONTEXT_REQUEST_KEY_CHARS} characters, and contain no whitespace or control characters"
        ));
    }
}

#[test]
fn invocation_envelope_bounds_count_input_items_before_deduplication() {
    assert_eq!(
        parse(json!({"ack": vec![MESSAGE_ID; MAX_TOOL_CALL_ACK_MESSAGE_IDS]}))
            .unwrap()
            .metadata
            .ack_session_message_ids,
        [MESSAGE_ID]
    );
    assert_eq!(
        parse(json!({"ack": vec![MESSAGE_ID; MAX_TOOL_CALL_ACK_MESSAGE_IDS + 1]})).unwrap_err(),
        format!("field '_wc.ack' accepts at most {MAX_TOOL_CALL_ACK_MESSAGE_IDS} message ids")
    );
    assert_eq!(
        parse(json!({"context": vec!["future.material"; MAX_CONTEXT_REQUEST_ITEMS]}))
            .unwrap()
            .metadata
            .context_request,
        ["future.material"]
    );
    assert_eq!(
        parse(json!({"context": vec!["future.material"; MAX_CONTEXT_REQUEST_ITEMS + 1]}))
            .unwrap_err(),
        format!(
            "field '_wc.context' accepts at most {MAX_CONTEXT_REQUEST_ITEMS} context material keys"
        )
    );
}

#[test]
fn invocation_envelope_preserves_byte_and_character_bounds() {
    // ACK refs retain their byte bound; human text and context keys count Unicode chars.
    let ack_ref =
        "é".repeat(MAX_TOOL_CALL_ACK_REF_CHARS / 2) + &"x".repeat(MAX_TOOL_CALL_ACK_REF_CHARS % 2);
    assert_eq!(ack_ref.len(), MAX_TOOL_CALL_ACK_REF_CHARS);
    assert!(parse(json!({"ack_ref": ack_ref})).is_ok());
    assert_eq!(
        parse(json!({"ack_ref": format!("{ack_ref}x")})).unwrap_err(),
        "field '_wc.ack_ref' must be a non-empty bounded string"
    );
    for (field, text_field, id_field, limit, expected) in [
        (
            "resolve",
            "resolution",
            "message_id",
            MAX_MESSAGE_RESOLUTION_CHARS,
            "_wc.resolve.resolution",
        ),
        (
            "reply",
            "message",
            "reply_to",
            MAX_WINDOW_REPLY_CHARS,
            "_wc.reply.message",
        ),
    ] {
        assert!(
            parse(json!({field: {id_field: MESSAGE_ID, text_field: "中".repeat(limit)}})).is_ok()
        );
        assert_eq!(
            parse(json!({field: {id_field: MESSAGE_ID, text_field: "中".repeat(limit + 1)}}))
                .unwrap_err(),
            format!("{expected} exceeds {limit} chars")
        );
    }
    assert!(parse(json!({"context": ["中".repeat(MAX_CONTEXT_REQUEST_KEY_CHARS)]})).is_ok());
    assert!(parse(json!({"context": ["中".repeat(MAX_CONTEXT_REQUEST_KEY_CHARS + 1)]})).is_err());
}

#[test]
fn invocation_envelope_error_precedence_and_control_privacy_stay_stable() {
    let mut envelope = json!({
        "unsupported": true,
        "control": {"private-fixture-material": "must-not-appear-in-errors"},
        "compact_execution": null,
        "record": null, "ack": null, "ack_ref": null,
        "resolve": null, "reply": null, "context": null
    });
    for (field, expected) in [
        ("unsupported", "unsupported _wc field 'unsupported' for tool 'call_runtime_tool'"),
        ("control", "_wc.control requires closed before/communication/after_success objects with bounded canonical operations"),
        ("compact_execution", "_wc.compact_execution must be a boolean"),
        ("record", "field '_wc.record' must be a non-empty string"),
        ("ack", "field '_wc.ack' must be an array of wc_msg_* ids"),
        ("ack_ref", "field '_wc.ack_ref' must be a non-empty bounded string"),
        ("resolve", "field '_wc.resolve' must be an object with message_id and resolution"),
        ("reply", "field '_wc.reply' must be an object with reply_to and message"),
        ("context", "field '_wc.context' must be an array of bounded context material keys"),
    ] {
        let mut arguments = json!({"_wc": envelope});
        assert_eq!(parse_mcp_invocation_envelope("call_runtime_tool", &mut arguments, true).unwrap_err(), expected);
        envelope.as_object_mut().unwrap().remove(field);
    }
}
