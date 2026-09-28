pub(super) use crate::test_support::sample_tool_args;
use serde_json::Value;

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
