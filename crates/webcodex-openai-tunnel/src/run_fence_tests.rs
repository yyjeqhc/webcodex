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
