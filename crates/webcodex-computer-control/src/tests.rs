use super::*;

#[test]
fn control_commit_rechecks_pause_and_epoch_after_preflight() {
    for pause in [false, true] {
        let control = LocalControl::new();
        let epoch = control.snapshot().unwrap().epoch;
        let pending = control.begin(true, Some(&epoch)).unwrap();
        if pause {
            control.set_paused(&epoch, true).unwrap();
        } else {
            control.invalidate().unwrap();
        }
        let mut effects = 0;
        let error = pending.commit(|| effects += 1).unwrap_err();
        assert!(error.starts_with(if pause {
            "computer_control_paused:"
        } else {
            "computer_control_changed:"
        }));
        assert_eq!(effects, 0);
    }
    let observation = LocalControl::new().begin(false, None).unwrap();
    assert!(observation
        .commit(|| panic!("observation cannot commit input"))
        .is_err());
}

#[test]
fn pause_after_commit_preserves_sequence_cleanup_and_uncertain_result() {
    let control = LocalControl::new();
    let epoch = control.snapshot().unwrap().epoch;
    let input = control.begin(true, Some(&epoch)).unwrap();
    let mut steps = 0;
    let result = input
        .commit(|| {
            steps += 1;
            let paused = control.set_paused(&epoch, true).unwrap();
            assert!(paused.paused && paused.controlling);
            assert!(control.set_paused(&paused.epoch, false).is_err());
            // Once the first effect crossed the commit boundary, finish necessary
            // key releases/native ownership cleanup and retain the original outcome.
            steps += 1;
            Err::<(), _>("outcome_unknown: native sequence could not be confirmed")
        })
        .unwrap();
    assert_eq!(
        result.unwrap_err(),
        "outcome_unknown: native sequence could not be confirmed"
    );
    assert_eq!(steps, 2);
    drop(input);
    let paused = control.snapshot().unwrap();
    assert!(!paused.controlling);
    control.set_paused(&paused.epoch, false).unwrap();
}

#[test]
fn pause_is_independent_from_observation_and_waits_for_controls_before_resume() {
    let control = LocalControl::new();
    let epoch = control.snapshot().unwrap().epoch;
    let observation = control.begin(false, None).unwrap();
    let input = control.begin(true, Some(&epoch)).unwrap();
    let stopped = control.set_paused(&epoch, true).unwrap();
    assert!(stopped.paused && stopped.observing && stopped.controlling);
    assert!(control.begin(true, Some(&stopped.epoch)).is_err());
    assert!(control.begin(false, None).is_ok());
    assert!(control.set_paused(&stopped.epoch, false).is_err());
    drop(input);
    let resumed = control.set_paused(&stopped.epoch, false).unwrap();
    assert!(!resumed.paused && resumed.observing && !resumed.controlling);
    assert!(control.begin(true, Some(&epoch)).is_err());
    assert!(control.begin(true, Some(&stopped.epoch)).is_err());
    assert!(control.begin(true, Some(&resumed.epoch)).is_ok());
    drop(observation);
    assert!(!control.snapshot().unwrap().observing);
}

#[test]
fn local_pause_survives_restart_but_request_and_local_ui_epochs_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let first = LocalControl::persistent(dir.path()).unwrap();
    let enabled = first.snapshot().unwrap();
    let paused = first.set_paused(&enabled.epoch, true).unwrap();
    let replacement = LocalControl::persistent(dir.path()).unwrap();
    let current = replacement.snapshot().unwrap();
    assert!(current.paused);
    assert_ne!(paused.epoch, current.epoch);
    assert!(replacement.set_paused(&paused.epoch, false).is_err());
    replacement.set_paused(&current.epoch, false).unwrap();
    assert!(
        !LocalControl::persistent(dir.path())
            .unwrap()
            .snapshot()
            .unwrap()
            .paused
    );
}

#[test]
fn corrupted_saved_state_does_not_enable_control() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("computer-control-state"), "unexpected").unwrap();
    assert!(LocalControl::persistent(dir.path()).is_err());
}

#[test]
fn session_invalidation_preserves_pause_and_rejects_old_control() {
    let control = LocalControl::new();
    let initial = control.snapshot().unwrap();
    control.invalidate().unwrap();
    assert!(control.begin(true, Some(&initial.epoch)).is_err());
    let current = control.snapshot().unwrap();
    let paused = control.set_paused(&current.epoch, true).unwrap();
    control.invalidate().unwrap();
    assert!(control.snapshot().unwrap().paused);
    assert!(control.set_paused(&paused.epoch, false).is_err());
}

#[test]
fn failed_pause_persistence_cannot_be_acknowledged_by_an_idempotent_retry() {
    let dir = tempfile::tempdir().unwrap();
    let control = LocalControl::persistent(dir.path()).unwrap();
    // A directory at the final file path deterministically prevents replacement.
    std::fs::create_dir(dir.path().join("computer-control-state")).unwrap();
    let initial = control.snapshot().unwrap();
    assert!(control.set_paused(&initial.epoch, true).is_err());
    let current = control.snapshot().unwrap();
    assert!(current.paused);
    assert!(control.set_paused(&current.epoch, true).is_err());
    assert!(control.begin(true, Some(&current.epoch)).is_err());
}

#[test]
fn pause_persistence_failure_survives_status_reads_until_a_durable_retry() {
    let dir = tempfile::tempdir().unwrap();
    let control = LocalControl::persistent(dir.path()).unwrap();
    let path = dir.path().join("computer-control-state");
    std::fs::create_dir(&path).unwrap();
    let initial = control.snapshot().unwrap();
    assert!(control.set_paused(&initial.epoch, true).is_err());
    for _ in 0..3 {
        let status = serde_json::to_value(control.snapshot().unwrap()).unwrap();
        assert_eq!(status["paused"], true);
        assert_eq!(status["pause_persistence_failed"], true);
    }
    // Preserve the obstruction so the retry changes only the persistence boundary.
    std::fs::rename(&path, dir.path().join("blocked-state-path")).unwrap();
    let failed = control.snapshot().unwrap();
    let saved = control.set_paused(&failed.epoch, true).unwrap();
    assert_eq!(saved.epoch, failed.epoch);
    assert_eq!(
        serde_json::to_value(&saved).unwrap()["pause_persistence_failed"],
        false
    );
    assert!(
        LocalControl::persistent(dir.path())
            .unwrap()
            .snapshot()
            .unwrap()
            .paused
    );
}

#[cfg(windows)]
#[test]
fn locked_enabled_file_reports_unsaved_pause_and_retry_survives_restart() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("computer-control-state");
    std::fs::write(&path, b"enabled\n").unwrap();
    let control = LocalControl::persistent(dir.path()).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let initial = control.snapshot().unwrap();
    assert!(control.set_paused(&initial.epoch, true).is_err());
    let failed = control.snapshot().unwrap();
    assert!(failed.paused);
    assert_eq!(
        serde_json::to_value(&failed).unwrap()["pause_persistence_failed"],
        true
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"enabled\n");
    assert!(
        !LocalControl::persistent(dir.path())
            .unwrap()
            .snapshot()
            .unwrap()
            .paused
    );
    drop(lock);
    control.set_paused(&failed.epoch, true).unwrap();
    assert!(
        LocalControl::persistent(dir.path())
            .unwrap()
            .snapshot()
            .unwrap()
            .paused
    );
}

#[test]
fn local_protocol_is_closed_and_does_not_accept_native_operations() {
    for request in [
        r#"{"action":"resume","expected_epoch":"a","payload":"b"}"#,
        r#"{"action":"computer_key_input"}"#,
    ] {
        assert!(serde_json::from_str::<LocalRequest>(request).is_err());
    }
    let mut old_status = serde_json::to_value(LocalControl::new().snapshot().unwrap()).unwrap();
    old_status
        .as_object_mut()
        .unwrap()
        .remove("pause_persistence_failed");
    assert!(serde_json::from_value::<ControlSnapshot>(old_status).is_err());
}

#[cfg(windows)]
#[test]
fn windows_management_pipe_confirms_exact_server_and_keeps_input_paused() {
    let dir = tempfile::tempdir().unwrap();
    let control = LocalControl::persistent(dir.path()).unwrap();
    let exe = std::env::current_exe().unwrap();
    if let Err(error) = serve(dir.path(), control.clone()) {
        // Service/headless Windows lanes must retain the same fail-closed
        // interactive-session boundary as production.
        assert_eq!(
            error,
            "computer_control_unavailable: interactive session required"
        );
        assert_eq!(
            request(dir.path(), &exe, &LocalRequest::Status).unwrap_err(),
            error
        );
        assert!(!control.snapshot().unwrap().paused);
        return;
    }
    let initial = request(dir.path(), &exe, &LocalRequest::Status).unwrap();
    let paused = request(
        dir.path(),
        &exe,
        &LocalRequest::Pause {
            expected_epoch: initial.epoch,
        },
    )
    .unwrap();
    assert!(paused.paused);
    assert!(control.begin(true, Some(&paused.epoch)).is_err());
    assert!(control.begin(false, None).is_ok());
    assert!(request(
        dir.path(),
        &exe,
        &LocalRequest::Resume {
            expected_epoch: "stale".into()
        }
    )
    .is_err());
    let current = request(dir.path(), &exe, &LocalRequest::Status).unwrap();
    assert!(current.paused);
    assert!(request(
        dir.path(),
        &dir.path().join("wrong-runner.exe"),
        &LocalRequest::Status
    )
    .is_err());
    let resumed = request(
        dir.path(),
        &exe,
        &LocalRequest::Resume {
            expected_epoch: current.epoch,
        },
    )
    .unwrap();
    assert!(!resumed.paused);
    {
        use std::os::windows::fs::OpenOptionsExt;
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(dir.path().join("computer-control-state"))
            .unwrap();
        assert!(request(
            dir.path(),
            &exe,
            &LocalRequest::Pause {
                expected_epoch: resumed.epoch
            }
        )
        .is_err());
        let mut failed = request(dir.path(), &exe, &LocalRequest::Status).unwrap();
        for _ in 0..3 {
            failed = request(dir.path(), &exe, &LocalRequest::Status).unwrap();
            assert!(failed.paused && failed.pause_persistence_failed);
        }
        drop(locked);
        let saved = request(
            dir.path(),
            &exe,
            &LocalRequest::Pause {
                expected_epoch: failed.epoch,
            },
        )
        .unwrap();
        assert!(saved.paused && !saved.pause_persistence_failed);
        assert!(
            LocalControl::persistent(dir.path())
                .unwrap()
                .snapshot()
                .unwrap()
                .paused
        );
    }
    assert!(serve(dir.path(), LocalControl::new()).is_err());
}
