use crate::tool_runtime::sessions::{SessionEvent, SessionSummary};
use serde_json::Value;

pub(in crate::tool_runtime::tests) fn output_has_file(output: &Value, path: &str) -> bool {
    output["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"].as_str() == Some(path))
}

pub(in crate::tool_runtime::tests) fn preview_for_path<'a>(
    output: &'a Value,
    path: &str,
) -> &'a Value {
    output["untracked_previews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|preview| preview["path"].as_str() == Some(path))
        .unwrap_or_else(|| {
            panic!(
                "missing preview for {path}: {}",
                output["untracked_previews"]
            )
        })
}

pub(in crate::tool_runtime::tests) fn finished_event<'a>(
    summary: &'a SessionSummary,
    tool_name: &str,
) -> &'a SessionEvent {
    summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == tool_name)
        .unwrap_or_else(|| {
            panic!(
                "missing finished event for {tool_name}: {:?}",
                summary.events
            )
        })
}

/// Assert a unified-diff agent command is one of the two fixed, known-safe
/// invocations and never carries diff content, a `cd` prefix, a heredoc,
/// or an `echo`/`cat` splice of the diff body.
pub(in crate::tool_runtime::tests) fn assert_safe_patch_command(command: &str, marker: &str) {
    let allowed = ["git apply --check -", "git apply -"];
    assert!(
        allowed.contains(&command),
        "unexpected patch command (must be a fixed git apply invocation): {}",
        command
    );
    assert!(
        !command.contains(marker),
        "patch content leaked into command: {}",
        command
    );
    assert!(
        !command.contains("cd "),
        "command must not use a cd prefix (cwd is supplied via the shell request): {}",
        command
    );
    assert!(
        !command.contains("<<"),
        "command must not use a heredoc: {}",
        command
    );
    assert!(
        !command.contains("echo "),
        "command must not use echo: {}",
        command
    );
    assert!(
        !command.contains("cat "),
        "command must not splice the patch via cat: {}",
        command
    );
}

pub(in crate::tool_runtime::tests) fn observe_job_continuation_job_id(output: &Value) -> &str {
    output["continuation"]["arguments"]["items"][0]["job_id"]
        .as_str()
        .filter(|job_id| !job_id.is_empty())
        .expect("Job continuation must carry exact durable identity")
}

pub(in crate::tool_runtime::tests) fn assert_sparse_pending_job_handoff(output: &Value) -> &str {
    assert_eq!(output["execution_state"], "pending");
    assert_observe_job_continuation(output);
    let keys = output
        .as_object()
        .expect("pending handoff output object")
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        keys,
        ["continuation", "execution_state"].into_iter().collect(),
        "normal model-facing pending handoff must stay sparse"
    );
    observe_job_continuation_job_id(output)
}

pub(in crate::tool_runtime::tests) fn assert_observe_job_continuation(output: &Value) {
    use crate::tool_runtime::{ObserveJobsWakeOn, ToolCall};
    let hint = &output["continuation"];
    assert_eq!(hint["tool"], "observe_jobs");
    let continuation_job_id = observe_job_continuation_job_id(output);
    if let Some(job_id) = output.get("job_id").and_then(Value::as_str) {
        assert_eq!(continuation_job_id, job_id);
    }
    if let Some(token) = output.get("observation_token") {
        assert_eq!(
            hint["arguments"]["items"][0]["after_observation_token"],
            *token
        );
    } else {
        assert!(hint["arguments"]["items"][0]["after_observation_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty()));
        assert!(output.get("continuation_semantics").is_none());
    }
    let call = ToolCall::from_tool_name(hint["tool"].as_str().unwrap(), hint["arguments"].clone())
        .expect("Job continuation must be parser-ready");
    assert!(matches!(
        call,
        ToolCall::ObserveJobs {
            wait_secs: Some(webcodex_core::runtime_contract::DEFAULT_JOB_CONTINUATION_WAIT_SECS),
            wake_on: ObserveJobsWakeOn::Terminal,
            ..
        }
    ));
    assert!(
        webcodex_core::runtime_contract::DEFAULT_JOB_CONTINUATION_WAIT_SECS
            <= webcodex_core::runtime_contract::MAX_JOB_OBSERVATION_WAIT_SECS
    );
}
