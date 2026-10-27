//! Borrowed projection parity and an opt-in cost comparison against the parent.
use super::*;
use serde_json::json;
mod console_no_validation_tests;
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

fn cache_no_validation_activity(_: &SessionEvent) -> bool {
    false
}

fn cache_not_run_summary(_: &[SessionEvent], _: usize) -> serde_json::Value {
    json!({"available":false, "latest_status":"not_run"})
}

fn cache_passed_summary(_: &[SessionEvent], _: usize) -> serde_json::Value {
    json!({"available":true, "latest_status":"passed"})
}

fn cache_hooks(passed: bool) -> crate::console::ConsoleValidationHooks {
    crate::console::ConsoleValidationHooks {
        event_observes_validation_activity: cache_no_validation_activity,
        validation_summary_from_events: if passed {
            cache_passed_summary
        } else {
            cache_not_run_summary
        },
    }
}

#[test]
fn closed_console_list_cache_preserves_project_hooks_and_immutable_source_identity() {
    let store = SessionStore::new(1, 2_000);
    let record = fixture(200);
    let id = record.session_id.clone();
    store.inner.lock().unwrap().insert_session(record);
    store.close_session(&id).unwrap();

    {
        let inner = store.inner.lock().unwrap();
        let StoredSession::Cold(cold) = inner.sessions.get(&id).unwrap() else {
            panic!("Closed Session should be Cold");
        };
        assert!(cold.console_list_cache.lock().unwrap().is_none());
    }
    let no_validation = cache_hooks(false);
    let original = store
        .with_record_for_query(&id, |record, _| {
            crate::console::build_list_item(record, "project", no_validation)
        })
        .unwrap();
    let first = store.console_list_for_project("project", None, no_validation);
    assert_eq!(first.returned, 1);
    assert_eq!(
        serde_json::to_value(&first.sessions[0]).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    for _ in 0..3 {
        let cached = store.console_list_for_project("project", None, no_validation);
        assert_eq!(
            serde_json::to_value(&cached).unwrap(),
            serde_json::to_value(&first).unwrap()
        );
    }
    assert_eq!(
        store
            .console_list_for_project("different-project", None, no_validation)
            .returned,
        0
    );

    // Hook identity prevents presenting a cached result from a different
    // validation projection even when the cold bytes have not changed.
    let passed = store.console_list_for_project("project", None, cache_hooks(true));
    assert_eq!(passed.sessions[0].overview.validation.state, "passed");
    let again = store.console_list_for_project("project", None, no_validation);
    assert_eq!(again.sessions[0].overview.validation.state, "not_run");

    // A raw replacement cannot re-expose the earlier cached row. In production
    // Cold raw bytes are immutable, but this guards internal repair/test paths.
    {
        let mut inner = store.inner.lock().unwrap();
        let StoredSession::Cold(cold) = inner.sessions.get_mut(&id).unwrap() else {
            panic!("Closed Session should remain Cold");
        };
        cold.raw = std::sync::Arc::from(
            serde_json::value::to_raw_value(&json!({"broken":"raw cold payload"})).unwrap(),
        );
    }
    assert_eq!(
        store
            .console_list_for_project("project", None, no_validation)
            .returned,
        0
    );

    // The cache does not outlive its retained Session identity.
    let weak = {
        let inner = store.inner.lock().unwrap();
        let StoredSession::Cold(cold) = inner.sessions.get(&id).unwrap() else {
            panic!("Closed Session should remain Cold");
        };
        std::sync::Arc::downgrade(&cold.console_list_cache)
    };
    drop(store);
    assert!(weak.upgrade().is_none());
}

#[test]
fn active_cold_console_list_does_not_retain_any_cache() {
    let store = SessionStore::new(1, 2_000);
    let record = fixture(100);
    let id = record.session_id.clone();
    store.inner.lock().unwrap().insert_session(record);
    store.start_session(Some("other".into()), None);
    let list = store.console_list_for_project("project", None, cache_hooks(false));
    assert_eq!(list.returned, 1);
    let inner = store.inner.lock().unwrap();
    let StoredSession::Cold(cold) = inner.sessions.get(&id).unwrap() else {
        panic!("Active history should be compacted to Cold");
    };
    assert!(cold.console_list_cache.lock().unwrap().is_none());
}

#[test]
#[ignore = "manual 2,000-event Closed Session cache benchmark and retained row size"]
fn closed_console_list_cache_repeated_read_cost_comparison() {
    let store = SessionStore::new(1, 2_000);
    let record = fixture(2_000);
    let id = record.session_id.clone();
    store.inner.lock().unwrap().insert_session(record);
    store.close_session(&id).unwrap();
    let hooks = cache_hooks(false);
    let baseline = store
        .with_record_for_query(&id, |record, _| {
            crate::console::build_list_item(record, "project", hooks)
        })
        .unwrap();
    let warm = store.console_list_for_project("project", None, hooks);
    assert_eq!(
        serde_json::to_value(&baseline).unwrap(),
        serde_json::to_value(&warm.sessions[0]).unwrap()
    );
    let payload_bytes = serde_json::to_vec(&warm.sessions[0]).unwrap().len();
    let rounds = 16_u128;
    let mut legacy_ns = 0_u128;
    let mut cached_ns = 0_u128;
    for iteration in 0..rounds {
        if iteration % 2 == 0 {
            let t = Instant::now();
            std::hint::black_box(
                store
                    .with_record_for_query(&id, |record, _| {
                        crate::console::build_list_item(record, "project", hooks)
                    })
                    .unwrap(),
            );
            legacy_ns += t.elapsed().as_nanos();
        }
        let t = Instant::now();
        std::hint::black_box(store.console_list_for_project("project", None, hooks));
        cached_ns += t.elapsed().as_nanos();
        if iteration % 2 != 0 {
            let t = Instant::now();
            std::hint::black_box(
                store
                    .with_record_for_query(&id, |record, _| {
                        crate::console::build_list_item(record, "project", hooks)
                    })
                    .unwrap(),
            );
            legacy_ns += t.elapsed().as_nanos();
        }
    }
    println!(
        "closed_console_list events=2000 legacy_avg_ms={:.3} cached_avg_ms={:.3} speedup={:.1}x retained_row_json_bytes={}",
        legacy_ns as f64 / rounds as f64 / 1e6,
        cached_ns as f64 / rounds as f64 / 1e6,
        legacy_ns as f64 / cached_ns as f64,
        payload_bytes,
    );
}

#[test]
#[ignore = "manual repeated multi-Session Console inventory benchmark with 60 Closed identities"]
fn closed_console_multi_session_batch_cost_comparison() {
    let store = SessionStore::new_in_memory_with_limits(2, 100, 2_000);
    let ids = (0..60)
        .map(|_| {
            let record = fixture(200);
            let id = record.session_id.clone();
            store.inner.lock().unwrap().insert_session(record);
            store.close_session(&id).unwrap();
            id
        })
        .collect::<Vec<_>>();
    let hooks = cache_hooks(false);
    let projects = ["project"];
    let initial = store.console_lists_for_projects(&projects, Some(100), hooks);
    assert_eq!(initial["project"].returned, ids.len());
    assert!(initial["project"]
        .sessions
        .iter()
        .all(|row| row.lifecycle == "closed"));
    let encoded = serde_json::to_vec(&initial).unwrap();
    let mut uncached_ns = 0_u128;
    let mut cached_ns = 0_u128;
    let iterations = 8_u128;
    for round in 0..iterations {
        // The actual batch inventory implementation is identical in both
        // cases; only the transient memoized Closed projections are cleared.
        if round % 2 == 0 {
            {
                let inner = store.inner.lock().unwrap();
                for id in &ids {
                    let StoredSession::Cold(cold) = inner.sessions.get(id).unwrap() else {
                        panic!("Closed identity must remain cold");
                    };
                    *cold.console_list_cache.lock().unwrap() = None;
                }
            }
            let start = Instant::now();
            let baseline = store.console_lists_for_projects(&projects, Some(100), hooks);
            uncached_ns += start.elapsed().as_nanos();
            assert_eq!(serde_json::to_vec(&baseline).unwrap(), encoded);
        }
        let start = Instant::now();
        let warmed = store.console_lists_for_projects(&projects, Some(100), hooks);
        cached_ns += start.elapsed().as_nanos();
        assert_eq!(serde_json::to_vec(&warmed).unwrap(), encoded);
        if round % 2 != 0 {
            {
                let inner = store.inner.lock().unwrap();
                for id in &ids {
                    let StoredSession::Cold(cold) = inner.sessions.get(id).unwrap() else {
                        panic!("Closed identity must remain cold");
                    };
                    *cold.console_list_cache.lock().unwrap() = None;
                }
            }
            let start = Instant::now();
            let baseline = store.console_lists_for_projects(&projects, Some(100), hooks);
            uncached_ns += start.elapsed().as_nanos();
            assert_eq!(serde_json::to_vec(&baseline).unwrap(), encoded);
        }
    }
    println!(
        "console_batch sessions={} events_per_session=200 uncached_avg_ms={:.3} cached_avg_ms={:.3} speedup={:.2}x response_bytes={}",
        ids.len(),
        uncached_ns as f64 / iterations as f64 / 1e6,
        cached_ns as f64 / iterations as f64 / 1e6,
        uncached_ns as f64 / cached_ns as f64,
        encoded.len(),
    );
}
