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

/// Assert a patch-related agent command is one of the fixed, known-safe
/// invocations and never carries patch content, a `cd` prefix, a heredoc,
/// or an `echo`/`cat` splice of the patch body.
pub(in crate::tool_runtime::tests) fn assert_safe_patch_command(command: &str, marker: &str) {
    let allowed = [
        "git apply --check -",
        "git apply --check - && echo OK",
        "git apply --stat -",
        "git apply -",
    ];
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
    // The only permitted `echo` is the fixed `echo OK` success marker; it
    // never carries patch content. `cat` must never appear (no splicing).
    if command.contains("echo ") {
        assert_eq!(command, "git apply --check - && echo OK");
    }
    assert!(
        !command.contains("cat "),
        "command must not splice the patch via cat: {}",
        command
    );
}
