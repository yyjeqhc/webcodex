use super::*;
fn root() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}
const ORIGIN: &str = "https://api.openai.com";

#[test]
fn live_owner_and_concurrent_recovery_cannot_archive() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let owner = RunFence::acquire(&root, ORIGIN, "first").unwrap();
    let identity = observe(&root, ORIGIN, "first").unwrap().unwrap();
    assert_eq!(
        archive(&root, ORIGIN, "first", &identity.run_id, |_| true).unwrap_err(),
        "tunnel_owner_active"
    );
    assert!(owner.path().exists());
    owner.finish().unwrap();
    assert!(RunFence::acquire(&root, ORIGIN, "first").is_ok());
}
#[test]
fn archival_is_exact_retains_bytes_and_does_not_replay_on_repeat() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let owner = RunFence::acquire(&root, ORIGIN, "first").unwrap();
    let identity = observe(&root, ORIGIN, "first").unwrap().unwrap();
    let path = owner.path().to_path_buf();
    let bytes = fs::read(&path).unwrap();
    drop(owner);
    assert_eq!(
        archive(&root, ORIGIN, "first", &identity.run_id, |_| false).unwrap_err(),
        "tunnel_owner_active"
    );
    assert!(archive(
        &root,
        ORIGIN,
        "wrong-profile-identity",
        &identity.run_id,
        |_| true
    )
    .is_err());
    assert!(archive(
        &root,
        ORIGIN,
        "first",
        &uuid::Uuid::new_v4().to_string(),
        |_| true
    )
    .is_err());
    let archived = archive(&root, ORIGIN, "first", &identity.run_id, |pid| {
        pid == identity.owner_pid
    })
    .unwrap();
    assert_eq!(fs::read(&archived).unwrap(), bytes);
    assert!(!path.exists());
    assert_eq!(
        archive(&root, ORIGIN, "first", &identity.run_id, |_| true).unwrap_err(),
        "tunnel_fence_missing"
    );
    let replacement = RunFence::acquire(&root, ORIGIN, "first").unwrap();
    drop(replacement);
    assert_eq!(
        archive(&root, ORIGIN, "first", &identity.run_id, |_| true).unwrap_err(),
        "tunnel_fence_changed"
    );
    assert_eq!(fs::read(&archived).unwrap(), bytes);
}
#[test]
fn legacy_and_invalid_markers_never_get_silently_upgraded_or_removed() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let path = paths(&root, ORIGIN, "first").0;
    for (bytes, expected) in [
        (
            b"native-tunnel-run-v1\n".as_slice(),
            "tunnel_legacy_owner_unverifiable",
        ),
        (b"not valid".as_slice(), "tunnel_fence_invalid"),
    ] {
        let mut file = private_options()
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        file.write_all(bytes).unwrap();
        assert_eq!(observe(&root, ORIGIN, "first").unwrap_err(), expected);
        assert_eq!(
            archive(
                &root,
                ORIGIN,
                "first",
                &uuid::Uuid::new_v4().to_string(),
                |_| true
            )
            .unwrap_err(),
            expected
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
#[test]
fn failed_archive_preserves_active_fence_and_existing_archive() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let owner = RunFence::acquire(&root, ORIGIN, "first").unwrap();
    let identity = observe(&root, ORIGIN, "first").unwrap().unwrap();
    let path = owner.path().to_path_buf();
    drop(owner);
    let parent = root.join(format!(
        "{}.recovered",
        path.file_name().unwrap().to_string_lossy()
    ));
    private_directory(&parent).unwrap();
    let destination = parent.join(&identity.run_id);
    fs::write(&destination, b"existing history").unwrap();
    assert_eq!(
        archive(&root, ORIGIN, "first", &identity.run_id, |_| true).unwrap_err(),
        "tunnel_archive_exists"
    );
    assert!(path.exists());
    assert_eq!(fs::read(destination).unwrap(), b"existing history");
}
#[test]
fn observing_missing_identity_creates_nothing() {
    let temp = root();
    let path = temp.path().join("missing");
    assert!(observe(&path, ORIGIN, "first").unwrap().is_none());
    assert!(!path.exists());
}

#[test]
fn protected_crash_archives_exact_run_and_keeps_receipts_across_clean_stops() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let binding = [7; 32];
    let (owner, receipts) = restartable(&root, ORIGIN, "first", binding, |_| true).unwrap();
    let previous = observe(&root, ORIGIN, "first").unwrap().unwrap();
    assert_eq!(previous.version, 3);
    let bytes = fs::read(owner.path()).unwrap();
    assert!(receipts.claim("effect-one").unwrap());
    assert!(restartable(&root, ORIGIN, "first", binding, |_| true).is_err());
    drop(receipts);
    drop(owner); // Abrupt shutdown leaves the fence and synced receipt.
    assert!(matches!(
        restartable(&root, ORIGIN, "first", binding, |_| false),
        Err("tunnel_owner_active")
    ));
    let (next, receipts) = restartable(&root, ORIGIN, "first", binding, |_| true).unwrap();
    assert!(!receipts.claim("effect-one").unwrap());
    assert!(receipts.claim("effect-two").unwrap());
    let archived = root
        .join(format!(
            "{}.recovered",
            next.path().file_name().unwrap().to_string_lossy()
        ))
        .join(previous.run_id);
    assert_eq!(fs::read(archived).unwrap(), bytes);
    next.finish().unwrap();
    drop(receipts);
    drop(next);
    assert_eq!(
        RunFence::acquire(&root, ORIGIN, "first").unwrap_err(),
        "tunnel_receipts_required"
    );
    let (last, receipts) = restartable(&root, ORIGIN, "first", binding, |_| true).unwrap();
    assert!(!receipts.claim("effect-one").unwrap());
    assert!(!receipts.claim("effect-two").unwrap());
    last.finish().unwrap();
}

#[test]
fn automatic_recovery_preserves_legacy_missing_corrupt_and_retargeted_history() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let legacy = RunFence::acquire(&root, ORIGIN, "legacy").unwrap();
    let legacy_path = legacy.path().to_owned();
    drop(legacy);
    assert!(matches!(
        restartable(&root, ORIGIN, "legacy", [1; 32], |_| true),
        Err("tunnel_restart_uncertain")
    ));
    assert!(legacy_path.exists());

    let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    let path = owner.path().to_owned();
    let bytes = fs::read(&path).unwrap();
    assert!(receipts.claim("effect").unwrap());
    drop(receipts);
    drop(owner);
    assert!(matches!(
        restartable(&root, ORIGIN, "first", [2; 32], |_| true),
        Err("tunnel_restart_uncertain")
    ));
    let journal = path.with_extension("receipts");
    let saved = fs::read(&journal).unwrap();
    fs::remove_file(&journal).unwrap();
    assert!(matches!(
        restartable(&root, ORIGIN, "first", [1; 32], |_| true),
        Err("tunnel_receipts_missing")
    ));
    let mut corrupt = saved;
    corrupt[40] ^= 1;
    let mut file = private_options().create_new(true).open(&journal).unwrap();
    file.write_all(&corrupt).unwrap();
    drop(file);
    assert!(matches!(
        restartable(&root, ORIGIN, "first", [1; 32], |_| true),
        Err("tunnel_receipts_invalid")
    ));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read(&journal).unwrap(), corrupt);
}

#[test]
fn interrupted_receipt_append_preserves_previously_synced_admissions() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    let path = owner.path().with_extension("receipts");
    assert!(receipts.claim("synced-effect").unwrap());
    drop(receipts);
    drop(owner);
    let mut file = private_options().open(&path).unwrap();
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::End(0)).unwrap();
    file.write_all(&[0; 17]).unwrap();
    drop(file);
    let (next, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    assert!(!receipts.claim("synced-effect").unwrap());
    assert!(receipts.claim("new-effect").unwrap());
    assert_eq!(fs::metadata(path).unwrap().len(), 40 + 2 * 64);
    next.finish().unwrap();
}

#[test]
fn oversized_receipt_history_is_rejected_without_reading_or_replacing_it() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    let path = owner.path().to_owned();
    let bytes = fs::read(&path).unwrap();
    drop(receipts);
    drop(owner);
    let file = private_options()
        .open(path.with_extension("receipts"))
        .unwrap();
    file.set_len(64_000_041).unwrap();
    drop(file);
    assert!(matches!(
        restartable(&root, ORIGIN, "first", [1; 32], |_| true),
        Err("tunnel_receipts_invalid")
    ));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

fn restartable(
    root: &Path,
    origin: &str,
    tunnel: &str,
    binding: [u8; 32],
    owner_absent: impl FnOnce(u32) -> bool,
) -> Result<(RunFence, Receipts), &'static str> {
    RunFence::acquire_restartable(root, origin, tunnel, binding, None, |identity| {
        owner_absent(identity.owner_pid)
    })
    .map(|(guard, receipts, cutoff)| {
        assert_eq!(cutoff, None);
        (guard, receipts)
    })
}

#[test]
fn protected_run_retains_the_exact_process_instance_for_reboot_pid_reuse() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let (owner, receipts, cutoff) =
        RunFence::acquire_restartable(&root, ORIGIN, "instance", [1; 32], Some(123), |_| true)
            .unwrap();
    assert_eq!(cutoff, None);
    drop(receipts);
    drop(owner);
    let (next, _, _) =
        RunFence::acquire_restartable(&root, ORIGIN, "instance", [1; 32], Some(456), |old| {
            assert_eq!(old.owner_started, Some(123));
            true
        })
        .unwrap();
    assert_eq!(
        observe(&root, ORIGIN, "instance")
            .unwrap()
            .unwrap()
            .owner_started,
        Some(456)
    );
    next.finish().unwrap();
}

#[test]
fn lost_receipts_after_clean_stop_never_reset_protected_history() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    let path = owner.path().to_owned();
    assert!(receipts.claim("already-dispatched").unwrap());
    owner.finish().unwrap();
    drop(receipts);
    drop(owner);
    fs::remove_file(path.with_extension("receipts")).unwrap();
    let lock_path = path.with_extension("lock");
    let history = fs::read(&lock_path).unwrap();
    assert!(matches!(
        restartable(&root, ORIGIN, "first", [1; 32], |_| true),
        Err("tunnel_receipts_missing")
    ));
    assert!(!path.exists());
    assert!(!path.with_extension("receipts").exists());
    assert_eq!(fs::read(lock_path).unwrap(), history);
    assert_eq!(
        RunFence::acquire(&root, ORIGIN, "first").unwrap_err(),
        "tunnel_receipts_required"
    );
}

#[test]
fn automatic_archive_count_exhaustion_preserves_all_history() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let (mut owner, mut receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    for _ in 0..128 {
        drop(receipts);
        drop(owner);
        (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    }
    let path = owner.path().to_owned();
    let active = fs::read(&path).unwrap();
    drop(receipts);
    drop(owner);
    // Budget rejection must happen before journal validation can trim a tail.
    use std::io::{Seek, SeekFrom};
    let journal = path.with_extension("receipts");
    let mut file = private_options().open(&journal).unwrap();
    file.seek(SeekFrom::End(0)).unwrap();
    file.write_all(&[0; 17]).unwrap();
    drop(file);
    let receipt_bytes = fs::read(&journal).unwrap();
    let history = fs::read(path.with_extension("lock")).unwrap();
    assert!(matches!(
        restartable(&root, ORIGIN, "first", [1; 32], |_| true),
        Err("tunnel_archive_capacity")
    ));
    assert_eq!(fs::read(&path).unwrap(), active);
    assert_eq!(fs::read(journal).unwrap(), receipt_bytes);
    assert_eq!(fs::read(path.with_extension("lock")).unwrap(), history);
    assert_eq!(
        fs::read_dir(root.join(format!(
            "{}.recovered",
            path.file_name().unwrap().to_string_lossy()
        )))
        .unwrap()
        .count(),
        128
    );
}

#[test]
fn missing_or_corrupt_protection_after_clean_stop_preserves_receipts() {
    for corrupt in [false, true] {
        let temp = root();
        let root = temp.path().canonicalize().unwrap();
        let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
        let path = owner.path().to_owned();
        assert!(receipts.claim("already-dispatched").unwrap());
        owner.finish().unwrap();
        drop(receipts);
        drop(owner);
        let journal = path.with_extension("receipts");
        let receipt_bytes = fs::read(&journal).unwrap();
        let lock_path = path.with_extension("lock");
        let mut history = fs::read(&lock_path).unwrap();
        if corrupt {
            history[0] ^= 1;
        } else {
            history.clear();
        }
        let mut file = private_options().truncate(true).open(&lock_path).unwrap();
        file.write_all(&history).unwrap();
        drop(file);
        let expected = if corrupt {
            "tunnel_protected_history_invalid"
        } else {
            "tunnel_protected_history_missing"
        };
        assert!(
            matches!(restartable(&root, ORIGIN, "first", [1; 32], |_| true), Err(code) if code == expected)
        );
        assert!(!path.exists());
        assert_eq!(fs::read(journal).unwrap(), receipt_bytes);
        assert_eq!(fs::read(lock_path).unwrap(), history);
        assert!(RunFence::acquire(&root, ORIGIN, "first").is_err());
    }
}

#[test]
fn clean_protection_cannot_be_rebound_to_a_new_target() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    let path = owner.path().to_owned();
    assert!(receipts.claim("already-dispatched").unwrap());
    owner.finish().unwrap();
    drop(receipts);
    drop(owner);
    let journal = fs::read(path.with_extension("receipts")).unwrap();
    let history = fs::read(path.with_extension("lock")).unwrap();
    assert!(matches!(
        restartable(&root, ORIGIN, "first", [2; 32], |_| true),
        Err("tunnel_protected_history_binding_changed")
    ));
    assert!(!path.exists());
    assert_eq!(fs::read(path.with_extension("receipts")).unwrap(), journal);
    assert_eq!(fs::read(path.with_extension("lock")).unwrap(), history);
}

#[test]
fn interrupted_protection_initialization_never_reseeds_a_journal() {
    for complete in [false, true] {
        let temp = root();
        let root = temp.path().canonicalize().unwrap();
        let (path, mut lock) = lock(&root, ORIGIN, "first").unwrap();
        if complete {
            initialize_protected_history(&mut lock, [1; 32], &root).unwrap();
        } else {
            lock.write_all(b"WCTHST01partial").unwrap();
            lock.sync_all().unwrap();
        }
        drop(lock);
        let history = fs::read(path.with_extension("lock")).unwrap();
        let expected = if complete {
            "tunnel_receipts_missing"
        } else {
            "tunnel_protected_history_invalid"
        };
        assert!(
            matches!(restartable(&root, ORIGIN, "first", [1; 32], |_| true), Err(code) if code == expected)
        );
        assert!(!path.exists());
        assert!(!path.with_extension("receipts").exists());
        assert_eq!(fs::read(path.with_extension("lock")).unwrap(), history);
        assert!(RunFence::acquire(&root, ORIGIN, "first").is_err());
    }
}

#[test]
fn automatic_archive_byte_exhaustion_preserves_active_receipts_and_archives() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    assert!(receipts.claim("already-dispatched").unwrap());
    let path = owner.path().to_owned();
    let active = fs::read(&path).unwrap();
    let identity = observe(&root, ORIGIN, "first").unwrap().unwrap();
    drop(receipts);
    drop(owner);
    let journal = fs::read(path.with_extension("receipts")).unwrap();
    let history = fs::read(path.with_extension("lock")).unwrap();
    let directory = root.join(format!(
        "{}.recovered",
        path.file_name().unwrap().to_string_lossy()
    ));
    private_directory(&directory).unwrap();
    let mut archives = Vec::new();
    // Legal 4-KiB marker records fill the byte budget before the count budget.
    for _ in 0..16 {
        let mut saved = identity.clone();
        saved.run_id = uuid::Uuid::new_v4().to_string();
        let archive = directory.join(&saved.run_id);
        let mut bytes = serde_json::to_vec(&saved).unwrap();
        bytes.resize(4096, b' ');
        let mut file = private_options().create_new(true).open(&archive).unwrap();
        file.write_all(&bytes).unwrap();
        drop(file);
        archives.push((archive, bytes));
    }
    assert!(matches!(
        restartable(&root, ORIGIN, "first", [1; 32], |_| true),
        Err("tunnel_archive_capacity")
    ));
    assert_eq!(fs::read(path.with_extension("receipts")).unwrap(), journal);
    assert_eq!(fs::read(path.with_extension("lock")).unwrap(), history);
    assert_eq!(fs::read(&path).unwrap(), active);
    assert!(!directory.join(&identity.run_id).exists());
    for (archive, bytes) in archives {
        assert_eq!(fs::read(archive).unwrap(), bytes);
    }
}

#[test]
fn invalid_archive_entries_do_not_mutate_protected_state() {
    for oversized in [false, true] {
        let temp = root();
        let root = temp.path().canonicalize().unwrap();
        let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
        let path = owner.path().to_owned();
        let active = fs::read(&path).unwrap();
        drop(receipts);
        drop(owner);
        let journal = fs::read(path.with_extension("receipts")).unwrap();
        let history = fs::read(path.with_extension("lock")).unwrap();
        let directory = root.join(format!(
            "{}.recovered",
            path.file_name().unwrap().to_string_lossy()
        ));
        private_directory(&directory).unwrap();
        let invalid = directory.join("invalid");
        if oversized {
            let file = private_options().create_new(true).open(invalid).unwrap();
            file.set_len(4097).unwrap();
        } else {
            fs::create_dir(invalid).unwrap();
        }
        assert!(matches!(
            restartable(&root, ORIGIN, "first", [1; 32], |_| true),
            Err("tunnel_archive_invalid")
        ));
        assert_eq!(fs::read(path.with_extension("receipts")).unwrap(), journal);
        assert_eq!(fs::read(path.with_extension("lock")).unwrap(), history);
        assert_eq!(fs::read(&path).unwrap(), active);
    }
}

#[test]
fn explicitly_reconciled_legacy_runs_can_initialize_protected_history() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let legacy = RunFence::acquire(&root, ORIGIN, "first").unwrap();
    let identity = observe(&root, ORIGIN, "first").unwrap().unwrap();
    let bytes = fs::read(legacy.path()).unwrap();
    drop(legacy);
    let archived = archive(&root, ORIGIN, "first", &identity.run_id, |_| true).unwrap();
    let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true)
        .expect("explicitly reconciled v2 history must not prevent the first protected run");
    assert_eq!(fs::read(archived).unwrap(), bytes);
    assert!(receipts.claim("new-protected-request").unwrap());
    owner.finish().unwrap();
}

#[test]
fn surviving_protected_archives_cannot_be_reopened_by_legacy_callers() {
    let temp = root();
    let root = temp.path().canonicalize().unwrap();
    let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    assert!(receipts.claim("old-effect").unwrap());
    drop(receipts);
    drop(owner);
    let (owner, receipts) = restartable(&root, ORIGIN, "first", [1; 32], |_| true).unwrap();
    let path = owner.path().to_owned();
    owner.finish().unwrap();
    drop(receipts);
    drop(owner);
    fs::remove_file(path.with_extension("receipts")).unwrap();
    private_options()
        .truncate(true)
        .open(path.with_extension("lock"))
        .unwrap();
    assert_eq!(
        RunFence::acquire(&root, ORIGIN, "first").unwrap_err(),
        "tunnel_protected_history_missing"
    );
    assert!(!path.exists());
    assert!(matches!(
        restartable(&root, ORIGIN, "first", [1; 32], |_| true),
        Err("tunnel_protected_history_missing")
    ));
}

#[test]
fn ambiguous_legacy_archives_never_initialize_protection() {
    for contents in [None, Some(b"invalid".as_slice())] {
        let temp = root();
        let root = temp.path().canonicalize().unwrap();
        let (path, lock) = lock(&root, ORIGIN, "first").unwrap();
        drop(lock);
        let directory = archive_directory(&path).unwrap();
        private_directory(&directory).unwrap();
        if let Some(bytes) = contents {
            let mut file = private_options()
                .create_new(true)
                .open(directory.join(uuid::Uuid::new_v4().to_string()))
                .unwrap();
            file.write_all(bytes).unwrap();
        }
        assert!(restartable(&root, ORIGIN, "first", [1; 32], |_| true).is_err());
        assert!(RunFence::acquire(&root, ORIGIN, "first").is_err());
        assert!(!path.exists());
        assert!(!path.with_extension("receipts").exists());
        assert!(fs::read(path.with_extension("lock")).unwrap().is_empty());
    }
}
