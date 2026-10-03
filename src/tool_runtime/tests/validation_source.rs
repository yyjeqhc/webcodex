use super::*;
use webcodex_core::validation_source::{ObservedMutationFence, ValidationFreshness};

#[test]
fn source_fence_multiple_writers_remain_active_until_every_known_completion() {
    let registry = ValidationSourceRegistry::default();
    let first = registry.begin("p").unwrap();
    let second = registry.begin("p").unwrap();
    first.finish(&noop());
    let during = registry.capture("p").unwrap();
    assert!(!during.quiescent);
    assert_eq!(
        registry.observe("p", Some(&during)).observed_mutation_fence,
        ObservedMutationFence::Unknown
    );
    second.finish(&noop());
    assert!(registry.capture("p").unwrap().quiescent);
    assert_eq!(
        registry.observe("p", Some(&during)).observed_mutation_fence,
        ObservedMutationFence::Crossed
    );
}

#[test]
fn source_fence_handoff_and_generation_exhaustion_cannot_manufacture_quiescence() {
    let registry = ValidationSourceRegistry::default();
    registry
        .begin("job")
        .unwrap()
        .finish(&crate::tool_runtime::ToolResult::ok(
            serde_json::json!({"job_id":"existing-job","state_changed":false}),
        ));
    assert!(!registry.capture("job").unwrap().quiescent);
    let state = registry.project("exhausted").unwrap();
    state.lock().unwrap().generation = MAX_SOURCE_GENERATION;
    let before = registry.capture("exhausted").unwrap();
    registry.begin("exhausted").unwrap().finish(&noop());
    let after = registry.capture("exhausted").unwrap();
    assert_eq!(after.generation, MAX_SOURCE_GENERATION);
    assert!(!after.quiescent);
    assert_eq!(
        registry
            .observe("exhausted", Some(&before))
            .observed_mutation_fence,
        ObservedMutationFence::Unknown
    );
}

#[test]
fn presentation_completion_does_not_manufacture_validation_quiescence() {
    let registry = ValidationSourceRegistry::default();
    registry
        .begin("p")
        .unwrap()
        .finish(&crate::tool_runtime::ToolResult::ok(
            serde_json::json!({"job_id":"wc_job_0123456789abcdef","execution_state":"pending"}),
        ));
    let pending = registry.capture_presentation("p").unwrap();
    assert_eq!(pending.pending_jobs.len(), 1);
    assert!(!registry.capture("p").unwrap().quiescent);
    assert!(registry.resolve_presentation_jobs("p", &pending));
    assert!(registry
        .capture_presentation("p")
        .unwrap()
        .pending_jobs
        .is_empty());
    assert!(!registry.capture("p").unwrap().quiescent);
    let old = registry.capture_presentation("p").unwrap();
    let active = registry.begin("p").unwrap();
    assert!(registry.capture_presentation("p").is_none());
    assert!(!registry.resolve_presentation_jobs("p", &old));
    drop(active);
    assert!(registry.capture_presentation("p").is_none());
}

fn noop() -> crate::tool_runtime::ToolResult {
    crate::tool_runtime::ToolResult::ok(serde_json::json!({"state_changed": false}))
}

#[test]
fn source_fence_tracks_inflight_completed_and_noop_attempts_not_content_equality() {
    let registry = ValidationSourceRegistry::default();
    let before = registry.capture("p").unwrap();
    let writer = registry.begin("p").unwrap();
    let during = registry.capture("p").unwrap();
    assert!(!during.quiescent);
    assert_eq!(
        registry.observe("p", Some(&before)).freshness,
        ValidationFreshness::Stale
    );
    writer.finish(&noop());
    assert_eq!(
        registry.observe("p", Some(&during)).freshness,
        ValidationFreshness::Stale
    );
    let after = registry.capture("p").unwrap();
    assert!(after.quiescent);
    assert_eq!(
        registry.observe("p", Some(&after)).observed_mutation_fence,
        ObservedMutationFence::Uncrossed
    );
    assert_eq!(
        registry.observe("p", Some(&after)).freshness,
        ValidationFreshness::Unproven
    );
}

#[test]
fn source_fence_cancellation_and_unknown_writers_never_reopen_clean_epoch() {
    let registry = ValidationSourceRegistry::default();
    drop(registry.begin("p"));
    let after = registry.capture("p").unwrap();
    assert!(!after.quiescent);
    registry.begin("p").unwrap().finish(&noop());
    assert!(!registry.capture("p").unwrap().quiescent);
    assert_eq!(
        registry
            .observe("p", Some(&registry.capture("p").unwrap()))
            .observed_mutation_fence,
        ObservedMutationFence::Unknown
    );
}

#[test]
fn source_fence_project_and_restart_epochs_are_not_interchangeable() {
    let registry = ValidationSourceRegistry::default();
    let start = registry.capture("one").unwrap();
    registry.begin("two").unwrap().finish(&noop());
    assert_eq!(
        registry
            .observe("one", Some(&start))
            .observed_mutation_fence,
        ObservedMutationFence::Uncrossed
    );
    assert_eq!(
        registry
            .observe("two", Some(&start))
            .observed_mutation_fence,
        ObservedMutationFence::Unknown
    );
    assert_eq!(
        ValidationSourceRegistry::default()
            .observe("one", Some(&start))
            .observed_mutation_fence,
        ObservedMutationFence::Unknown
    );
}

#[test]
fn source_fence_capacity_does_not_evict_active_writers() {
    let registry = ValidationSourceRegistry::default();
    let writer = registry.begin("p").unwrap();
    for i in 1..MAX_TRACKED_PROJECTS {
        assert!(registry.capture(&i.to_string()).is_some());
    }
    assert!(registry.capture("overflow").is_none());
    assert!(!registry.capture("p").unwrap().quiescent);
    writer.finish(&noop());
    assert!(registry.capture("p").unwrap().quiescent);
}
