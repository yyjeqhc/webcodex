//! Borrowed projection parity and an opt-in cost comparison against the parent.
use super::*;
use serde_json::json;
mod reference;
use reference::reference_summary;

fn fixture(count: usize) -> SessionRecord {
    let store = SessionStore::new(100, 2000);
    let id = store
        .start_session(Some("project".into()), Some("projection fixture".into()))
        .session_id;
    let mut record = store
        .inner
        .lock()
        .unwrap()
        .sessions
        .get(&id)
        .unwrap()
        .hot()
        .unwrap()
        .clone();
    for i in 0..count {
        let mut event = session_closed_system_event(&id, 10 + i as i64);
        event.event_id = format!("evt_{i:032x}");
        event.kind = "tool_call_finished".into();
        event.tool_name = "edit_project_files".into();
        event.status = Some("succeeded".into());
        event.write_like = true;
        event.input_summary = Some(json!({"diagnostic": "bounded input evidence ".repeat(32)}));
        event.changed_paths = vec![format!("src/file_{}.rs", i / 2)];
        event.effect_evidence = Some(ToolEffectEventEvidence {
            state_changed: Some(true),
            ..Default::default()
        });
        event.logical_invocation_id = Some(format!(
            "{}{:032x}",
            crate::model::LOGICAL_INVOCATION_ID_PREFIX,
            i / 2
        ));
        event.logical_invocation_role = Some(
            if i % 2 == 0 {
                crate::model::LOGICAL_INVOCATION_ROLE_RECORDER
            } else {
                crate::model::LOGICAL_INVOCATION_ROLE_BUSINESS
            }
            .into(),
        );
        event.call_id = Some(format!("call_{i:032x}"));
        match (i / 2) % 6 {
            1 => {
                event.logical_invocation_role =
                    Some(crate::model::LOGICAL_INVOCATION_ROLE_RECORDER.into())
            }
            2 => {
                event.logical_invocation_id = None;
                event.logical_invocation_role = None;
            }
            3 if i % 2 == 1 => event.status = Some("failed".into()),
            4 => event.call_id = Some("reused-call-id".into()),
            5 if i % 2 == 0 => event.kind = "tool_call_started".into(),
            _ => {}
        }
        record.events.push_back(Arc::new(event));
    }
    record.events_observed = count as u64;
    record
}

#[test]
fn borrowed_summary_matches_parent_for_every_tail_and_wrapped_deque() {
    for count in [0, 1, 8, 81, 2000] {
        let mut record = fixture(count);
        // A wrapped deque and an evicted prefix must not change source sequence semantics.
        for _ in 0..count / 3 {
            let event = record.events.pop_front().unwrap();
            record.events.push_back(event);
        }
        record.events_observed += 13;
        for limit in [
            None,
            Some(0),
            Some(1),
            Some(50),
            Some(200),
            Some(usize::MAX),
        ] {
            assert_eq!(
                serde_json::to_value(summarize_record(&record, limit, None)).unwrap(),
                serde_json::to_value(reference_summary(&record, limit, None)).unwrap()
            );
        }
    }
}

#[test]
fn borrowed_selection_points_at_original_arcs_and_keeps_ambiguous_pairs() {
    let record = fixture(4);
    let selected = crate::events::canonical_tool_call_finished_event_refs(
        record.events.iter().map(Arc::as_ref),
    );
    assert_eq!(selected.len(), 3);
    for (actual, index) in selected.iter().zip([1, 2, 3]) {
        assert!(std::ptr::eq(*actual, record.events[index].as_ref()));
    }
    let after: Vec<_> = record.events.iter().map(Arc::strong_count).collect();
    assert_eq!(
        after,
        vec![1; 4],
        "projection must not retain extra Arc owners either"
    );
}

#[test]
fn borrowed_hot_cold_summary_and_path_evidence_are_identical_and_non_promoting() {
    let store = SessionStore::new(1, 2000);
    let record = fixture(81);
    let id = record.session_id.clone();
    {
        store.inner.lock().unwrap().insert_session(record);
    }
    let hot_summary = store.summary(&id, Some(8)).unwrap();
    let hot_paths = store.retained_changed_path_evidence(&id).unwrap();
    assert!(hot_paths.1);
    store.start_session(None, None);
    assert!(store.cold_payload_bytes_for_test(&id).is_some());
    let before = store.status();
    assert_eq!(
        serde_json::to_value(store.summary(&id, Some(8)).unwrap()).unwrap(),
        serde_json::to_value(hot_summary).unwrap()
    );
    assert_eq!(
        store.retained_changed_path_evidence(&id).unwrap(),
        hot_paths
    );
    assert_eq!(store.status().hot_sessions, before.hot_sessions);
    assert_eq!(store.status().cold_sessions, before.cold_sessions);
    assert_eq!(store.status().capacity_evictions, 0);
}

#[test]
fn borrowed_path_evidence_keeps_eviction_and_saturated_path_uncertainty() {
    let store = SessionStore::new(1, 2000);
    let mut record = fixture(2);
    let id = record.session_id.clone();
    Arc::make_mut(&mut record.events[1]).changed_paths = (0..MAX_INPUT_ARRAY_ITEMS)
        .map(|i| format!("p{i}.rs"))
        .collect();
    record.events_observed += 20;
    store.inner.lock().unwrap().insert_session(record);
    let (paths, complete) = store.retained_changed_path_evidence(&id).unwrap();
    assert_eq!(paths.len(), MAX_INPUT_ARRAY_ITEMS);
    assert!(!complete);
}

#[test]
#[ignore = "manual pure projection cost comparison; synthetic retained evidence"]
fn borrowed_projection_cost_comparison() {
    let record = fixture(2000);
    for limit in [0, 50, 200] {
        let reference_start = Instant::now();
        for _ in 0..200 {
            std::hint::black_box(reference_summary(&record, Some(limit), None));
        }
        let reference_ms = reference_start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        for _ in 0..200 {
            std::hint::black_box(summarize_record(&record, Some(limit), None));
        }
        println!(
            "BORROWED_PROJECTION {}",
            json!({"events":2000,"iterations":200,"limit":limit,
            "parent_ms":reference_ms,"borrowed_ms":start.elapsed().as_secs_f64()*1000.0,
            "eliminated_event_deep_clones_per_call":2000,"scope":"synthetic in-process projection, not end-to-end latency"})
        );
    }
}
