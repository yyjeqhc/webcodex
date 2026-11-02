use super::*;

#[test]
fn confirmed_pause_rejects_a_native_guard_waiting_for_input_lock() {
    let runtime = Arc::new(ComputerRuntime::new(ComputerConfig {
        max_encoded_image_bytes: 1024,
    }));
    let epoch = runtime.local_control().snapshot().unwrap().epoch;
    let input_lock = runtime.browser_user_handoffs.lock().unwrap();
    let worker_runtime = runtime.clone();
    let worker = std::thread::spawn(move || worker_runtime.native_effect_guard().is_ok());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !runtime.local_control().snapshot().unwrap().controlling {
        assert!(Instant::now() < deadline, "worker never reached the gate");
        std::thread::yield_now();
    }
    runtime.local_control().set_paused(&epoch, true).unwrap();
    drop(input_lock);
    assert!(
        !worker.join().unwrap(),
        "native effect guard admitted after pause was confirmed"
    );
}

#[test]
fn control_changed_during_native_preflight_never_dispatches_an_effect() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for pause in [false, true] {
        let runtime = Arc::new(ComputerRuntime::new(ComputerConfig {
            max_encoded_image_bytes: 1024,
        }));
        let effects = Arc::new(AtomicUsize::new(0));
        let (prepared_tx, prepared_rx) = std::sync::mpsc::channel();
        let (continue_tx, continue_rx) = std::sync::mpsc::channel();
        let worker_runtime = runtime.clone();
        let worker_effects = effects.clone();
        let worker = std::thread::spawn(move || {
            let guard = worker_runtime.native_effect_guard().unwrap();
            prepared_tx.send(()).unwrap();
            continue_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            guard
                .activity
                .commit(|| worker_effects.fetch_add(1, Ordering::SeqCst))
        });
        prepared_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        if pause {
            let epoch = runtime.local_control().snapshot().unwrap().epoch;
            runtime.local_control().set_paused(&epoch, true).unwrap();
        } else {
            runtime.local_control().invalidate().unwrap();
        }
        continue_tx.send(()).unwrap();
        let error = worker.join().unwrap().unwrap_err();
        assert!(error.starts_with(if pause {
            "computer_control_paused:"
        } else {
            "computer_control_changed:"
        }));
        assert_eq!(effects.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn rejected_pointer_commit_preserves_the_unspent_snapshot_generation() {
    let runtime = ComputerRuntime::new(ComputerConfig {
        max_encoded_image_bytes: 1024,
    });
    let guard = runtime.native_effect_guard().unwrap();
    let display = DisplayRecord {
        native_identity: vec![9],
        width: 1920,
        height: 1080,
        primary: false,
    };
    let mut snapshots = DisplaySnapshotRegistry::default();
    let display_id = "display_iavN7wEjRWeJq83v";
    let generation = snapshots.bind(display_id, &display).unwrap();
    let epoch = runtime.local_control().snapshot().unwrap().epoch;
    runtime.local_control().set_paused(&epoch, true).unwrap();
    let result = guard.activity.commit(|| {
        dispatch_after_spending_pointer_generation(
            &mut snapshots,
            display_id,
            generation,
            &display,
            |_| panic!("paused pointer must not dispatch"),
        )
    });
    assert!(result.unwrap_err().starts_with("computer_control_paused:"));
    snapshots
        .validate_pointer(display_id, generation, &display)
        .unwrap();
}

#[test]
fn every_native_effect_is_blocked_even_without_a_runner_request() {
    let runtime = crate::ComputerRuntime::new(crate::ComputerConfig {
        max_encoded_image_bytes: 1024,
    });
    let control = runtime.local_control();
    let epoch = control.snapshot().unwrap().epoch;
    control.set_paused(&epoch, true).unwrap();
    for result in [
        runtime.write_clipboard("fixture"),
        runtime.activate_window("invalid"),
        runtime.launch_application("invalid"),
        runtime.control("invalid", "invalid", crate::ComputerAction::Press),
        runtime.scroll_to_element("invalid", "invalid"),
        runtime.scroll(
            "invalid",
            "invalid",
            crate::ComputerScrollDirection::Down,
            crate::ComputerScrollAmount::Small,
        ),
        runtime.key_input("invalid", "a", &[]),
        runtime.input_text("invalid", "invalid", "fixture"),
        runtime.pointer_effect(crate::PointerAction::Move, "invalid", 1, 0, 0),
    ] {
        assert!(result.unwrap_err().starts_with("computer_control_paused:"));
    }
}
