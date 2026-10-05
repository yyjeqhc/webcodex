use serde_json::json;
use std::time::Duration;
use tokio::time::Instant;
use webcodex_openai_tunnel::{
    deadline::ResponseTimeout, policy::DeadlinePolicy, wire::Command, Credential,
};

#[test]
fn wire_timeout_compatibility_is_separate_from_execution_policy() {
    for value in [
        json!("-1s"),
        json!("1.5s"),
        json!(42),
        json!({}),
        json!("9223372036854775808ns"),
        json!("1e3s"),
        json!(" 1s"),
        json!("1m30s"),
    ] {
        let c:Command=serde_json::from_value(json!({"request_id":"r","shard_token":"s","command_type":"jsonrpc","response_timeout":value,"future":true})).unwrap();
        assert_eq!(c.response_timeout, ResponseTimeout::Invalid);
        assert!(DeadlinePolicy::ProtocolCompatible
            .deadline(c.response_timeout, Instant::now())
            .unwrap()
            .is_none());
        assert!(DeadlinePolicy::default()
            .deadline(c.response_timeout, Instant::now())
            .is_err());
    }
    for (value, expected) in [
        (json!(null), ResponseTimeout::Absent),
        (json!("0s"), ResponseTimeout::Valid(Duration::ZERO)),
        (
            json!("30s"),
            ResponseTimeout::Valid(Duration::from_secs(30)),
        ),
    ] {
        let parsed: ResponseTimeout = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, expected);
    }
    assert!(
        !format!("{:?}", Credential::bearer("private-value").unwrap()).contains("private-value")
    );
}
