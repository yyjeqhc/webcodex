//! CLI-only continuation projection for synchronous head/tail previews. Preserve
//! the canonical Job binding/epoch/revision; change only read cursors, never effects.
use serde_json::{json, Value};
use webcodex_core::job_observation::JobObservationToken;

pub(super) fn skip_displayed_prefix(tool: &str, output: &mut Value) {
    if !matches!(tool, "run_shell" | "run_process" | "run_script") {
        return;
    }
    let details = output.get("details").unwrap_or(output);
    let call = &details["continuation"];
    if call["tool"] != "observe_jobs"
        || call["arguments"]["items"]
            .as_array()
            .is_none_or(|a| a.len() != 1)
    {
        return;
    }
    let item = &call["arguments"]["items"][0];
    if item["page"] != true || item["job_id"] != details["job_id"] || !details["job_id"].is_string()
    {
        return;
    }
    let Some(mut token) = item["after_observation_token"]
        .as_str()
        .and_then(|s| JobObservationToken::parse(s).ok())
    else {
        return;
    };
    for (stream, cursor) in [
        ("stdout", &mut token.stdout_cursor),
        ("stderr", &mut token.stderr_cursor),
    ] {
        let (Some(start), Some(log), Some(lines)) = (
            *cursor,
            details[format!("{stream}_tail")].as_str(),
            details[format!("{stream}_lines")].as_u64(),
        ) else {
            continue;
        };
        let shown = if details[format!("{stream}_truncated")] == false
            && log.lines().count() as u64 == lines
        {
            lines
        } else {
            displayed_prefix_lines(log, details["output_hint"].is_string())
        };
        *cursor = Some(start.saturating_add(shown.min(lines)));
    }
    let details = if output.get("details").is_some() {
        &mut output["details"]
    } else {
        output
    };
    details["continuation"]["arguments"]["items"][0]["after_observation_token"] =
        json!(token.encode());
}

/// Recognize only the positions inserted by the canonical preview producers,
/// not arbitrary marker text in command output. A clipped partial line must be
/// read again; the newline inserted by the character preview is not source.
fn displayed_prefix_lines(log: &str, line_bounded: bool) -> u64 {
    use crate::tool_runtime::{COMMAND_OUTPUT_MAX_LINES, COMMAND_STDIO_TAIL_CHARS};
    let marker = "\n... output omitted ...\n";
    let head_chars = (COMMAND_STDIO_TAIL_CHARS - marker.chars().count()) / 2;
    let head: String = log.chars().take(head_chars).collect();
    let char_prefix = log
        .get(head.len()..)
        .filter(|s| s.starts_with(marker))
        .map(|_| head.bytes().filter(|b| *b == b'\n').count() as u64);
    let line_prefix = (line_bounded
        && log.split_inclusive('\n').nth(COMMAND_OUTPUT_MAX_LINES / 2)
            == Some("... output omitted ...\n"))
    .then_some((COMMAND_OUTPUT_MAX_LINES / 2) as u64);
    match (char_prefix, line_prefix) {
        (Some(a), Some(b)) => a.min(b),
        (Some(n), None) | (None, Some(n)) => n,
        _ => 0, // Unknown preview shape: recover conservatively from retained start.
    }
}
