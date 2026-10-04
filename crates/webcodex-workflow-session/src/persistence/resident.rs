//! Lossless materialization of our own canonical, process-local Cold bytes.
//! Disk input must first pass the stream loader's sanitize-on-restore boundary.
use super::{ColdSessionRecord, PersistedSessionRecord, SessionRecord};

pub(super) fn materialize(
    persisted: PersistedSessionRecord,
    cold: &ColdSessionRecord,
) -> SessionRecord {
    // Exhaustive destructuring makes new durable fields a compilation failure
    // here, rather than silently dropping them on a residency transition.
    let PersistedSessionRecord {
        session_id,
        project,
        owner_authority_fingerprint,
        title,
        mode,
        guards,
        execution_context,
        lifecycle,
        created_at,
        updated_at,
        events,
        messages,
        message_delivery_replays,
        events_observed,
        legacy_context_revision: _,
        git_baseline_tree,
        repository_edit_observed,
        materialized_validation_job_ids,
        message_observation_revision,
        message_observation_floor,
        message_observation_revisions,
        assignment_history_floors,
        assignment_history_tracking_complete,
        completion_assignment_fence_fingerprints,
        completion_assignment_fence_tracking_complete,
    } = persisted;
    let mut completion_assignment_fence_fingerprints: std::collections::BTreeMap<_, _> =
        completion_assignment_fence_fingerprints
            .into_iter()
            .map(|(id, fence)| (id, Some(fence)))
            .collect();
    for id in &cold.unfenced_completion_ids {
        completion_assignment_fence_fingerprints
            .entry(id.clone())
            .or_insert(None);
    }
    SessionRecord {
        session_id,
        project,
        owner_authority_fingerprint,
        title,
        mode,
        guards,
        execution_context,
        lifecycle,
        created_at,
        updated_at,
        events: events.into(),
        messages: messages.into(),
        message_delivery_replays,
        events_observed,
        git_baseline_tree,
        repository_edit_observed,
        materialized_validation_job_ids: materialized_validation_job_ids.into(),
        message_observation_revision,
        message_observation_floor,
        message_observation_revisions,
        assignment_history_floors,
        assignment_history_tracking_complete,
        completion_assignment_fence_fingerprints,
        completion_assignment_fence_tracking_complete,
        project_instructions: cold.resident_instructions.as_deref().cloned(),
    }
}
