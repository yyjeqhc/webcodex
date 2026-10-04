// Frozen parent implementation from main@46c8fe0d: parity/cost oracle only.
use super::*;

pub(super) fn reference_summary(
    record: &SessionRecord,
    limit: Option<usize>,
    cold: Option<&ColdSessionRecord>,
) -> SessionSummary {
    let limit = limit
        .unwrap_or(DEFAULT_SUMMARY_LIMIT)
        .clamp(0, MAX_SUMMARY_LIMIT);
    let retained_events = record
        .events
        .iter()
        .map(|event| event.as_ref().clone())
        .collect::<Vec<_>>();
    let finished_events = crate::events::canonical_tool_call_finished_events(&retained_events);
    let counts = SessionCounts {
        tool_calls: finished_events.len(),
        succeeded: finished_events
            .iter()
            .filter(|event| event.status.as_deref() == Some("succeeded"))
            .count(),
        failed: finished_events
            .iter()
            .filter(|event| event.status.as_deref() == Some("failed"))
            .count(),
        read_like: finished_events
            .iter()
            .filter(|event| event.read_like)
            .count(),
        write_like: finished_events
            .iter()
            .filter(|event| event.write_like)
            .count(),
        shell_like: finished_events
            .iter()
            .filter(|event| event.shell_like)
            .count(),
        git_like: finished_events
            .iter()
            .filter(|event| event.git_like)
            .count(),
        change_summary_like: finished_events
            .iter()
            .filter(|event| event.change_summary_like)
            .count(),
    };
    let retained_total = record.events.len();
    let observed_total = record.events_observed.max(retained_total as u64) as usize;
    let skip = retained_total.saturating_sub(limit);
    let events: Vec<SessionEvent> = record
        .events
        .iter()
        .skip(skip)
        .map(|event| event.as_ref().clone())
        .collect();
    let events_returned = events.len();
    let project_instructions = match cold {
        Some(cold) => cold.project_instructions.clone(),
        None => record
            .project_instructions
            .as_ref()
            .map(|snapshot| snapshot.to_summary()),
    };
    SessionSummary {
        session_id: record.session_id.clone(),
        project: record.project.clone(),
        title: record.title.clone(),
        mode: record.mode,
        guards: record.guards,
        execution_context: record.execution_context.clone(),
        lifecycle: record.lifecycle,
        git_baseline_tree: record.git_baseline_tree.clone(),
        repository_edit_observed: record.repository_edit_observed,
        created_at: record.created_at,
        updated_at: record.updated_at,
        counts,
        events,
        events_total: observed_total,
        events_retained: retained_total,
        events_evicted: observed_total.saturating_sub(retained_total),
        retention_truncated: observed_total > retained_total,
        ledger_first_retained_sequence: observed_total.saturating_sub(retained_total),
        events_returned,
        events_truncated: observed_total > events_returned,
        first_retained_sequence: observed_total.saturating_sub(events_returned),
        project_instructions,
        messages: build_messages_summary(record),
    }
}
