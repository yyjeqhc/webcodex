use super::*;
fn window(pid: u32, native_id: u32, title: &str) -> PlatformWindow {
    PlatformWindow {
        pid,
        native_id,
        x: 0,
        y: 0,
        identity_hash: [native_id as u8; 32],
        application: "Chrome".into(),
        title: title.into(),
        width: 800,
        height: 600,
        focused: Some(false),
        active: Some(false),
    }
}
#[test]
fn handoff_uses_exact_process_not_a_matching_title_or_renderer() {
    let found = exact_process_window(
        vec![window(11, 1, "same title"), window(22, 2, "same title")],
        22,
    )
    .unwrap();
    assert_eq!(found.native_id, 2);
    assert!(exact_process_window(vec![window(11, 1, "same title")], 22).is_err());
}
#[test]
fn multiple_windows_and_truncated_inventory_are_not_guessed() {
    assert!(
        exact_process_window(vec![window(22, 1, "active"), window(22, 2, "other")], 22).is_err()
    );
    let mut first = window(22, 1, "active");
    first.x = 10;
    first.y = 20;
    let mut second = window(22, 2, "other");
    second.x = 900;
    second.y = 20;
    let found =
        exact_process_window_with_bounds(vec![first.clone(), second], 22, Some((10, 20, 800, 600)))
            .unwrap();
    assert_eq!(found.native_id, 1);
    assert!(exact_process_window_with_bounds(
        vec![first.clone(), first],
        22,
        Some((10, 20, 800, 600)),
    )
    .is_err());
    assert!(exact_process_window(
        (0..=MAX_WINDOWS)
            .map(|id| window(22, id as u32, "tab"))
            .collect(),
        22
    )
    .is_err());
    assert!(exact_process_window(Vec::new(), 22).is_err());
    assert!(exact_process_window(vec![window(0, 1, "tab")], 0).is_err());
}

fn runtime() -> ComputerRuntime {
    let runtime = ComputerRuntime::new(ComputerConfig {
        max_encoded_image_bytes: 1024,
    });
    runtime.surfaces.lock().unwrap().insert(
        "surface_test".into(),
        SurfaceRecord {
            native_id: 1,
            pid: 2,
            identity_hash: [3; 32],
            application: "Browser".into(),
            title: "User login".into(),
            width: 100,
            height: 100,
        },
    );
    runtime
}

#[test]
fn browser_handoff_blocks_every_native_effect_without_native_dispatch() {
    let runtime = runtime();
    runtime
        .begin_browser_user_handoff("surface_test", "handoff_one")
        .unwrap();
    let results = [
        runtime.write_clipboard("text"),
        runtime.launch_application("application_missing"),
        runtime.activate_window("surface_test"),
        runtime.control("surface_test", "element_missing", ComputerAction::Press),
        runtime.scroll_to_element("surface_test", "element_missing"),
        runtime.scroll(
            "surface_test",
            "element_missing",
            ComputerScrollDirection::Down,
            ComputerScrollAmount::Small,
        ),
        runtime.key_input("surface_test", "enter", &[]),
        runtime.input_text("surface_test", "element_missing", "text"),
        runtime.pointer_effect(PointerAction::Move, "display_missing", 1, 0, 0),
        runtime.pointer_effect(PointerAction::Click, "display_missing", 1, 0, 0),
    ];
    for result in results {
        assert!(result
            .unwrap_err()
            .starts_with("browser_user_handoff_active:"));
    }
    // Read paths still reach their normal validation/platform boundary.
    assert!(!runtime
        .element_state("surface_test", "element_missing", false, false)
        .unwrap_err()
        .starts_with("browser_user_handoff_active:"));
}

#[test]
fn browser_handoff_exact_release_preserves_other_reservations_and_invalidates_observations() {
    let runtime = runtime();
    runtime
        .begin_browser_user_handoff("surface_test", "handoff_one")
        .unwrap();
    runtime
        .begin_browser_user_handoff("surface_test", "handoff_one")
        .unwrap();
    assert_eq!(runtime.browser_user_handoffs.lock().unwrap().len(), 1);
    runtime
        .begin_browser_user_handoff("surface_test", "handoff_two")
        .unwrap();
    runtime
        .elements
        .lock()
        .unwrap()
        .surface_generations
        .insert("surface_test".into(), 1);
    assert!(runtime
        .finish_browser_user_handoff("handoff_wrong")
        .is_err());
    assert_eq!(
        runtime.elements.lock().unwrap().surface_generations.len(),
        1
    );
    runtime
        .cancel_browser_user_handoff("handoff_wrong")
        .unwrap();
    assert_eq!(
        runtime.elements.lock().unwrap().surface_generations.len(),
        1
    );
    runtime.finish_browser_user_handoff("handoff_one").unwrap();
    assert!(runtime
        .elements
        .lock()
        .unwrap()
        .surface_generations
        .is_empty());
    assert!(runtime.native_effect_guard().is_err());
    runtime.finish_browser_user_handoff("handoff_one").unwrap();
    runtime.cancel_browser_user_handoff("handoff_one").unwrap();
    runtime.finish_browser_user_handoff("handoff_two").unwrap();
    assert!(runtime.native_effect_guard().is_err());
    runtime.cancel_browser_user_handoff("handoff_two").unwrap();
    assert!(runtime.finish_browser_user_handoff("handoff_two").is_err());
    assert!(runtime.native_effect_guard().is_ok());
}

#[test]
fn browser_handoff_rejects_unknown_surface_fence_conflict_and_capacity() {
    let runtime = runtime();
    assert!(runtime
        .begin_browser_user_handoff("surface_missing", "handoff_one")
        .is_err());
    assert!(runtime
        .begin_browser_user_handoff("surface_test", "bad fence")
        .is_err());
    assert!(runtime.browser_user_handoffs.lock().unwrap().is_empty());
    runtime
        .begin_browser_user_handoff("surface_test", "handoff_one")
        .unwrap();
    runtime
        .surfaces
        .lock()
        .unwrap()
        .get_mut("surface_test")
        .unwrap()
        .identity_hash = [4; 32];
    assert!(runtime
        .begin_browser_user_handoff("surface_test", "handoff_one")
        .unwrap_err()
        .starts_with("browser_user_handoff_conflict:"));
    for index in 1..MAX_WINDOWS {
        runtime
            .begin_browser_user_handoff("surface_test", &format!("handoff_{index}"))
            .unwrap();
    }
    assert!(runtime
        .begin_browser_user_handoff("surface_test", "handoff_full")
        .unwrap_err()
        .starts_with("browser_user_handoff_limit:"));
}

#[test]
fn browser_handoff_admission_and_native_dispatch_use_the_same_exclusive_guard() {
    let runtime = std::sync::Arc::new(runtime());
    let active_effect = runtime.native_effect_guard().unwrap();
    assert!(runtime.browser_user_handoffs.try_lock().is_err());
    let worker_runtime = runtime.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        worker_runtime.begin_browser_user_handoff("surface_test", "handoff_one")
    });
    started_rx.recv().unwrap();
    drop(active_effect);
    worker.join().unwrap().unwrap();
    assert!(runtime.native_effect_guard().is_err());
}
