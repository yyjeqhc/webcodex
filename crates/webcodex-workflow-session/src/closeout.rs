use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::{canonical_tool_call_finished_events, SessionEvent};

/// Deterministic closeout projection over canonical finished Session events.
///
/// Tool rows are ordered by tool name, changed paths are deduplicated and
/// sorted, and only the first 200 changed paths are exposed. This helper owns
/// no runtime authority and performs no I/O.
pub fn closeout_work_projection(events: &[SessionEvent]) -> (Value, Value) {
    let mut tools = BTreeMap::<String, (u64, u64, u64, Option<i64>)>::new();
    let mut changed_paths = BTreeSet::<String>::new();
    for event in canonical_tool_call_finished_events(events) {
        let counts = tools.entry(event.tool_name.clone()).or_default();
        counts.0 = counts.0.saturating_add(1);
        match event.status.as_deref() {
            Some("succeeded") => counts.1 = counts.1.saturating_add(1),
            Some("failed") => counts.2 = counts.2.saturating_add(1),
            _ => {}
        }
        counts.3 = event.finished_at.or(Some(event.timestamp));
        changed_paths.extend(event.changed_paths.iter().cloned());
    }
    let work = tools
        .into_iter()
        .map(|(tool_name, (count, succeeded, failed, completed_at))| {
            json!({
                "tool_name": tool_name,
                "count": count,
                "succeeded": succeeded,
                "failed": failed,
                "last_completed_at": completed_at,
            })
        })
        .collect::<Vec<_>>();
    (
        json!(work),
        json!(changed_paths.into_iter().take(200).collect::<Vec<_>>()),
    )
}
