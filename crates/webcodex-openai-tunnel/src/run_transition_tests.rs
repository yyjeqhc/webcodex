use super::*;

const ORIGIN: &str = "https://api.openai.com";
const TUNNEL: &str = "transition-test";

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    active: PathBuf,
    header: Vec<u8>,
    journal: Vec<u8>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let (owner, receipts, cutoff) =
            RunFence::acquire_restartable(&root, ORIGIN, TUNNEL, [1; 32], None, |_| true).unwrap();
        assert_eq!(cutoff, None);
        assert!(receipts.claim("old-admitted-effect").unwrap());
        let active = owner.path().to_path_buf();
        owner.finish().unwrap();
        drop(receipts);
        drop(owner);
        let header = fs::read(active.with_extension("lock")).unwrap();
        let journal = fs::read(active.with_extension("receipts")).unwrap();
        Self {
            _temp: temp,
            root,
            active,
            header,
            journal,
        }
    }

    fn authorize(
        &self,
        old: u8,
        new: u8,
        cutoff: u64,
        revision: u64,
    ) -> Result<CredentialTransition, &'static str> {
        RunFence::authorize_credential_transition(
            &self.root, ORIGIN, TUNNEL, [old; 32], [new; 32], cutoff, revision,
        )
    }

    fn unchanged_prefixes(&self) {
        assert!(fs::read(self.active.with_extension("lock"))
            .unwrap()
            .starts_with(&self.header));
        assert!(fs::read(self.active.with_extension("receipts"))
            .unwrap()
            .starts_with(&self.journal));
    }
}

#[test]
fn credential_transition_keeps_old_admissions_and_requires_latest_binding() {
    let f = Fixture::new();
    assert_eq!(
        f.authorize(1, 2, 100, 0).unwrap(),
        CredentialTransition {
            revision: 1,
            cutoff_seconds: 100,
            replayed: false
        }
    );
    f.unchanged_prefixes();
    assert_eq!(
        fs::read(f.active.with_extension("receipts")).unwrap(),
        f.journal
    );
    assert!(matches!(
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [1; 32], None, |_| true),
        Err("tunnel_protected_history_binding_changed")
    ));
    let (owner, receipts, cutoff) =
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [2; 32], None, |_| true).unwrap();
    assert_eq!(cutoff, Some(100));
    assert!(!receipts.claim("old-admitted-effect").unwrap());
    assert!(receipts.claim("new-effect").unwrap());
    owner.finish().unwrap();
    drop(owner);
    drop(receipts);
    f.unchanged_prefixes();
    assert_eq!(f.authorize(2, 3, 200, 1).unwrap().revision, 2);
    let (owner, receipts, cutoff) =
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [3; 32], None, |_| true).unwrap();
    assert_eq!(cutoff, Some(200));
    assert!(!receipts.claim("old-admitted-effect").unwrap());
    assert!(!receipts.claim("new-effect").unwrap());
    owner.finish().unwrap();
}

#[test]
fn only_exact_tail_retry_is_idempotent_and_all_guards_preserve_bytes() {
    let f = Fixture::new();
    f.authorize(1, 2, 100, 0).unwrap();
    let history = fs::read(f.active.with_extension("lock")).unwrap();
    assert!(f.authorize(1, 2, 100, 0).unwrap().replayed);
    for (old, new, cutoff, revision) in [
        (1, 2, 101, 0),
        (1, 3, 100, 0),
        (3, 2, 100, 0),
        (1, 2, 100, 1),
        (2, 3, 200, 0),
        (2, 3, 100, 1),
        (2, 3, 99, 1),
        (2, 2, 200, 1),
    ] {
        assert!(f.authorize(old, new, cutoff, revision).is_err());
        assert_eq!(fs::read(f.active.with_extension("lock")).unwrap(), history);
        assert_eq!(
            fs::read(f.active.with_extension("receipts")).unwrap(),
            f.journal
        );
    }
    assert_eq!(f.authorize(2, 3, 200, 1).unwrap().revision, 2);
    assert!(
        f.authorize(1, 2, 100, 0).is_err(),
        "an older transition is never a current retry"
    );
}

#[test]
fn transitions_never_initialize_a_namespace_or_bypass_an_active_marker() {
    let temp = tempfile::tempdir().unwrap();
    let missing_root = temp.path().join("not-created");
    assert_eq!(
        RunFence::authorize_credential_transition(
            &missing_root,
            ORIGIN,
            TUNNEL,
            [1; 32],
            [2; 32],
            100,
            0
        )
        .unwrap_err(),
        "tunnel_protected_history_missing"
    );
    assert!(!missing_root.exists());
    let f = Fixture::new();
    let (owner, receipts, _) =
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [1; 32], None, |_| true).unwrap();
    assert_eq!(
        f.authorize(1, 2, 100, 0).unwrap_err(),
        "tunnel_owner_active"
    );
    drop(receipts);
    drop(owner);
    assert_eq!(
        f.authorize(1, 2, 100, 0).unwrap_err(),
        "tunnel_transition_active_owner"
    );
    assert_eq!(fs::read(f.active.with_extension("lock")).unwrap(), f.header);
}

#[test]
fn missing_corrupt_or_incomplete_receipts_never_authorize_or_repair_history() {
    for kind in ["missing", "corrupt", "incomplete"] {
        let f = Fixture::new();
        let path = f.active.with_extension("receipts");
        match kind {
            "missing" => fs::remove_file(&path).unwrap(),
            "corrupt" => {
                let mut bytes = f.journal.clone();
                bytes[40] ^= 1;
                private_options()
                    .truncate(true)
                    .open(&path)
                    .unwrap()
                    .write_all(&bytes)
                    .unwrap();
            }
            _ => {
                let mut file = private_options().open(&path).unwrap();
                file.seek(SeekFrom::End(0)).unwrap();
                file.write_all(b"torn").unwrap();
            }
        }
        let before = fs::read(&path).ok();
        assert!(f.authorize(1, 2, 100, 0).is_err());
        assert_eq!(fs::read(&path).ok(), before);
        assert_eq!(fs::read(f.active.with_extension("lock")).unwrap(), f.header);
    }
}

#[test]
fn interrupted_transition_append_is_retained_and_startup_fails_closed() {
    for partial_length in [0, 1, 8, 16, 48, 80, 88, 120, RECORD_BYTES - 1] {
        let f = Fixture::new();
        let (_, mut lock) = lock(&f.root, ORIGIN, TUNNEL).unwrap();
        let history = read_history(&mut lock).unwrap().unwrap();
        let requested = Transition {
            old_binding: [1; 32],
            new_binding: [2; 32],
            cutoff_seconds: 100,
            expected_revision: 0,
        };
        let record = encode(&requested, history.tail_hash);
        publish_intent(&f.active, 0, &record).unwrap();
        assert_eq!(
            append_with(&mut lock, &record, 0, |file, bytes| {
                file.write_all(&bytes[..partial_length])?;
                Err(std::io::Error::other("injected append failure"))
            })
            .unwrap_err(),
            "tunnel_transition_uncertain"
        );
        drop(lock);
        let bytes = fs::read(f.active.with_extension("lock")).unwrap();
        assert!(bytes.starts_with(&f.header));
        assert!(f.authorize(1, 3, 100, 0).is_err());
        assert!(f.authorize(1, 2, 101, 0).is_err());
        for binding in [[1; 32], [2; 32]] {
            assert!(
                RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, binding, None, |_| true)
                    .is_err()
            );
        }
        assert_eq!(fs::read(f.active.with_extension("lock")).unwrap(), bytes);
        assert_eq!(
            fs::read(f.active.with_extension("receipts")).unwrap(),
            f.journal
        );
        assert!(f.authorize(1, 2, 100, 0).unwrap().replayed);
        let recovered = fs::read(f.active.with_extension("lock")).unwrap();
        assert!(recovered.starts_with(&bytes));
        assert_eq!(&recovered[PROTECTED_HISTORY_BYTES..], &record);
        let (owner, receipts, cutoff) =
            RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [2; 32], None, |_| true)
                .unwrap();
        assert_eq!(cutoff, Some(100));
        assert!(!receipts.claim("old-admitted-effect").unwrap());
        owner.finish().unwrap();
    }
}

#[test]
fn complete_append_with_uncertain_sync_can_be_confirmed_by_exact_retry() {
    let f = Fixture::new();
    let (_, mut lock) = lock(&f.root, ORIGIN, TUNNEL).unwrap();
    let history = read_history(&mut lock).unwrap().unwrap();
    let requested = Transition {
        old_binding: [1; 32],
        new_binding: [2; 32],
        cutoff_seconds: 100,
        expected_revision: 0,
    };
    let record = encode(&requested, history.tail_hash);
    publish_intent(&f.active, 0, &record).unwrap();
    assert!(append_with(&mut lock, &record, 0, |file, bytes| {
        file.write_all(bytes)?;
        Err(std::io::Error::other("injected sync uncertainty"))
    })
    .is_err());
    drop(lock);
    let bytes = fs::read(f.active.with_extension("lock")).unwrap();
    assert!(f.authorize(1, 2, 100, 0).unwrap().replayed);
    assert_eq!(fs::read(f.active.with_extension("lock")).unwrap(), bytes);
}

#[test]
fn unpublished_staging_blocks_startup_but_can_be_replaced_by_explicit_authorization() {
    let f = Fixture::new();
    let directory = intent_directory(&f.active);
    private_directory(&directory).unwrap();
    private_options()
        .create_new(true)
        .open(directory.join(".prepare"))
        .unwrap()
        .write_all(b"WCTTRN01unfinished preparation")
        .unwrap();
    assert!(matches!(
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [1; 32], None, |_| true),
        Err("tunnel_transition_pending")
    ));
    let result = f.authorize(1, 2, 100, 0).unwrap();
    assert!(
        !result.replayed,
        "unpublished staging is not prior authorization"
    );
    assert!(!directory.join(".prepare").exists());
    assert!(directory.join(intent_name(0)).exists());
    f.unchanged_prefixes();
}

#[test]
fn missing_or_corrupt_published_intent_never_repairs_history() {
    for corrupt in [false, true] {
        let f = Fixture::new();
        f.authorize(1, 2, 100, 0).unwrap();
        let intent = intent_directory(&f.active).join(intent_name(0));
        if corrupt {
            let mut bytes = fs::read(&intent).unwrap();
            bytes[48] ^= 1;
            private_options()
                .truncate(true)
                .open(&intent)
                .unwrap()
                .write_all(&bytes)
                .unwrap();
        } else {
            fs::remove_file(&intent).unwrap();
        }
        let history = fs::read(f.active.with_extension("lock")).unwrap();
        assert!(f.authorize(1, 2, 100, 0).is_err());
        assert!(
            RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [2; 32], None, |_| true)
                .is_err()
        );
        assert_eq!(fs::read(f.active.with_extension("lock")).unwrap(), history);
        assert_eq!(
            fs::read(f.active.with_extension("receipts")).unwrap(),
            f.journal
        );
    }
}

#[test]
fn partial_record_needs_exact_full_intent_and_matching_existing_prefix() {
    for with_intent in [false, true] {
        let f = Fixture::new();
        let (_, mut lock) = lock(&f.root, ORIGIN, TUNNEL).unwrap();
        let history = read_history(&mut lock).unwrap().unwrap();
        let requested = Transition {
            old_binding: [1; 32],
            new_binding: [2; 32],
            cutoff_seconds: 100,
            expected_revision: 0,
        };
        let mut record = encode(&requested, history.tail_hash);
        if with_intent {
            publish_intent(&f.active, 0, &record).unwrap();
            record[40] ^= 1;
        }
        lock.seek(SeekFrom::End(0)).unwrap();
        lock.write_all(&record[..57]).unwrap();
        drop(lock);
        let before = fs::read(f.active.with_extension("lock")).unwrap();
        let expected = if with_intent {
            "tunnel_transition_partial_mismatch"
        } else {
            "tunnel_transition_intent_missing"
        };
        assert_eq!(f.authorize(1, 2, 100, 0).unwrap_err(), expected);
        assert_eq!(fs::read(f.active.with_extension("lock")).unwrap(), before);
        assert_eq!(
            fs::read(f.active.with_extension("receipts")).unwrap(),
            f.journal
        );
    }
}

#[test]
fn checksum_and_predecessor_corruption_never_changes_evidence() {
    for corrupt_predecessor in [false, true] {
        let f = Fixture::new();
        f.authorize(1, 2, 100, 0).unwrap();
        let path = f.active.with_extension("lock");
        let mut bytes = fs::read(&path).unwrap();
        if corrupt_predecessor {
            bytes[PROTECTED_HISTORY_BYTES + 88] ^= 1;
            let checksum =
                Sha256::digest(&bytes[PROTECTED_HISTORY_BYTES..PROTECTED_HISTORY_BYTES + 120]);
            bytes[PROTECTED_HISTORY_BYTES + 120..].copy_from_slice(&checksum);
        } else {
            bytes[PROTECTED_HISTORY_BYTES + 48] ^= 1;
        }
        private_options()
            .truncate(true)
            .open(&path)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        assert!(f.authorize(1, 2, 100, 0).is_err());
        assert!(
            RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [2; 32], None, |_| true)
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(
            fs::read(f.active.with_extension("receipts")).unwrap(),
            f.journal
        );
    }
}

#[test]
fn archive_lineage_survives_transitions_but_unrelated_binding_is_rejected() {
    let f = Fixture::new();
    let (old, receipts, _) =
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [1; 32], None, |_| true).unwrap();
    drop(old);
    drop(receipts);
    let (old, receipts, _) =
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [1; 32], None, |_| true).unwrap();
    old.finish().unwrap();
    drop(old);
    drop(receipts);
    let directory = archive_directory(&f.active).unwrap();
    let first = fs::read_dir(&directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let first_bytes = fs::read(&first).unwrap();
    f.authorize(1, 2, 100, 0).unwrap();
    let (new, receipts, cutoff) =
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [2; 32], None, |_| true).unwrap();
    assert_eq!(cutoff, Some(100));
    drop(new);
    drop(receipts);
    let (new, receipts, _) =
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [2; 32], None, |_| true).unwrap();
    new.finish().unwrap();
    drop(new);
    drop(receipts);
    f.authorize(2, 3, 200, 1).unwrap();
    assert_eq!(fs::read(&first).unwrap(), first_bytes);
    let mut unrelated: RunIdentity = serde_json::from_slice(&first_bytes).unwrap();
    unrelated.run_id = uuid::Uuid::new_v4().to_string();
    unrelated.receipt_binding = Some(format!("{:x}", Sha256::digest([99; 32])));
    private_options()
        .create_new(true)
        .open(directory.join(&unrelated.run_id))
        .unwrap()
        .write_all(&serde_json::to_vec(&unrelated).unwrap())
        .unwrap();
    assert_eq!(
        f.authorize(3, 4, 300, 2).unwrap_err(),
        "tunnel_archive_binding_changed"
    );
    assert!(matches!(
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [3; 32], None, |_| true),
        Err("tunnel_archive_binding_changed")
    ));
    f.unchanged_prefixes();
}

#[test]
fn transition_capacity_is_bounded_without_history_eviction() {
    let f = Fixture::new();
    for revision in 0..MAX_TRANSITIONS as u64 {
        f.authorize(
            revision as u8 + 1,
            revision as u8 + 2,
            revision + 1,
            revision,
        )
        .unwrap();
    }
    let path = f.active.with_extension("lock");
    let bytes = fs::read(&path).unwrap();
    assert_eq!(
        bytes.len(),
        PROTECTED_HISTORY_BYTES + MAX_TRANSITIONS * RECORD_BYTES
    );
    assert_eq!(
        f.authorize(17, 18, 17, 16).unwrap_err(),
        "tunnel_transition_capacity"
    );
    assert!(f.authorize(16, 17, 16, 15).unwrap().replayed);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let (owner, receipts, cutoff) =
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [17; 32], None, |_| true).unwrap();
    assert_eq!(cutoff, Some(16));
    assert!(!receipts.claim("old-admitted-effect").unwrap());
    owner.finish().unwrap();
    f.unchanged_prefixes();
}

#[test]
fn returning_to_an_earlier_credential_is_a_new_revision_with_a_later_cutoff() {
    let f = Fixture::new();
    f.authorize(1, 2, 100, 0).unwrap();
    let (owner, receipts, cutoff) =
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [2; 32], None, |_| true).unwrap();
    assert_eq!(cutoff, Some(100));
    assert!(receipts.claim("admitted-under-b").unwrap());
    owner.finish().unwrap();
    drop(receipts);
    drop(owner);
    let returned = f.authorize(2, 1, 200, 1).unwrap();
    assert_eq!(returned.revision, 2);
    assert!(!returned.replayed);
    let (owner, receipts, cutoff) =
        RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [1; 32], None, |_| true).unwrap();
    assert_eq!(cutoff, Some(200));
    assert!(!receipts.claim("old-admitted-effect").unwrap());
    assert!(!receipts.claim("admitted-under-b").unwrap());
    owner.finish().unwrap();
    f.unchanged_prefixes();
}

#[test]
fn historical_or_unknown_active_binding_never_becomes_an_automatic_archive() {
    for unknown in [false, true] {
        let f = Fixture::new();
        let (owner, receipts, _) =
            RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [1; 32], None, |_| true)
                .unwrap();
        let mut saved = read(&f.active).unwrap().unwrap();
        owner.finish().unwrap();
        drop(owner);
        drop(receipts);
        f.authorize(1, 2, 100, 0).unwrap();
        if unknown {
            saved.receipt_binding = Some(format!("{:x}", Sha256::digest([99; 32])));
        }
        let bytes = serde_json::to_vec(&saved).unwrap();
        private_options()
            .create_new(true)
            .open(&f.active)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        for binding in [[1; 32], [2; 32]] {
            assert!(matches!(
                RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, binding, None, |_| panic!(
                    "invalid active binding must fail before owner recovery"
                )),
                Err("tunnel_restart_uncertain")
            ));
        }
        assert_eq!(fs::read(&f.active).unwrap(), bytes);
        assert!(!archive_directory(&f.active).unwrap().exists());
        f.unchanged_prefixes();
    }
}

#[test]
fn surviving_intent_directory_blocks_missing_or_empty_header_and_journal() {
    for empty_intent_directory in [false, true] {
        for empty_header in [false, true] {
            for legacy in [false, true] {
                let f = Fixture::new();
                f.authorize(1, 2, 100, 0).unwrap();
                let intent_directory = f.active.with_extension("transitions");
                let intent_path = intent_directory.join("00.intent");
                let intent_bytes = fs::read(&intent_path).unwrap();
                if empty_intent_directory {
                    fs::remove_file(&intent_path).unwrap();
                }
                let lock_path = f.active.with_extension("lock");
                let journal_path = f.active.with_extension("receipts");
                fs::remove_file(&lock_path).unwrap();
                fs::remove_file(&journal_path).unwrap();
                if empty_header {
                    private_options().create_new(true).open(&lock_path).unwrap();
                }
                let result = if legacy {
                    RunFence::acquire(&f.root, ORIGIN, TUNNEL).map(|_| ())
                } else {
                    RunFence::acquire_restartable(&f.root, ORIGIN, TUNNEL, [2; 32], None, |_| {
                        panic!("surviving intents must block initialization")
                    })
                    .map(|_| ())
                };
                assert_eq!(result, Err("tunnel_protected_history_missing"));
                // The common lock helper may create an empty lock when absent;
                // neither startup path may initialize protected history or a journal.
                assert_eq!(fs::read(&lock_path).unwrap(), Vec::<u8>::new());
                assert!(!journal_path.exists());
                assert!(!f.active.exists());
                assert!(!archive_directory(&f.active).unwrap().exists());
                assert!(intent_directory.is_dir());
                if empty_intent_directory {
                    assert_eq!(fs::read_dir(&intent_directory).unwrap().count(), 0);
                } else {
                    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
                    assert_eq!(fs::read_dir(&intent_directory).unwrap().count(), 1);
                }
            }
        }
    }
}
