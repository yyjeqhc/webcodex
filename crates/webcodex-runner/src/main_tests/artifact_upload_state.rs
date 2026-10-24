use super::*;

fn write_active_upload(
    store_root: &Path,
    root: &Path,
    directory: &Path,
    upload_id: &str,
    max_bytes: usize,
) {
    std::fs::create_dir_all(directory).unwrap();
    let relative_file = directory
        .strip_prefix(root)
        .unwrap()
        .join(format!("{upload_id}.bin"));
    let path = relative_file
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let (part, sidecar) = upload_paths(directory, upload_id);
    std::fs::write(part, b"").unwrap();
    write_upload_state(
        &sidecar,
        &ArtifactUploadState {
            path,
            expected_bytes: None,
            expected_sha256: None,
            mime_type: None,
            overwrite: false,
            max_bytes,
        },
    )
    .unwrap();
    write_upload_reservation(store_root, root, directory, upload_id, max_bytes).unwrap();
}

#[test]
fn restart_adopts_cross_directory_active_upload_limit_before_new_begin() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    let store_root = tmp.path().join("registry");
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    for index in 0..MAX_ACTIVE_ARTIFACT_UPLOADS_PER_PROJECT {
        let directory = root.join(format!("artifacts/set-{index}"));
        write_active_upload(
            &store_root,
            &root,
            &directory,
            &format!("wc_upload_restart_count_{index}"),
            1,
        );
    }
    let target = root.join("artifacts/new");
    std::fs::create_dir_all(&target).unwrap();

    let mut runtime = ArtifactUploadRuntimeState::new();
    runtime
        .prepare_directory(&root, &target, Some(&store_root))
        .unwrap();
    let error = runtime
        .check_begin_admission(&root, 1, Some(&store_root))
        .unwrap_err();
    assert!(error.contains("active upload limit"), "{error}");
}

#[test]
fn restart_adopts_cross_directory_reserved_byte_quota_before_new_begin() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    let store_root = tmp.path().join("registry");
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    for index in 0..2 {
        let directory = root.join(format!("artifacts/quota-{index}"));
        write_active_upload(
            &store_root,
            &root,
            &directory,
            &format!("wc_upload_restart_quota_{index}"),
            MAX_ARTIFACT_UPLOAD_BYTES,
        );
    }
    let target = root.join("artifacts/new");
    std::fs::create_dir_all(&target).unwrap();

    let mut runtime = ArtifactUploadRuntimeState::new();
    runtime
        .prepare_directory(&root, &target, Some(&store_root))
        .unwrap();
    let error = runtime
        .check_begin_admission(&root, 1, Some(&store_root))
        .unwrap_err();
    assert!(error.contains("reserved byte quota exceeded"), "{error}");
}

#[test]
fn persisted_tracking_is_recovered_and_release_removes_reservation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    let store_root = tmp.path().join("registry");
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let directory = root.join("artifacts/persisted");
    std::fs::create_dir_all(&directory).unwrap();
    let upload_id = "wc_upload_persisted";
    let (part, sidecar) = upload_paths(&directory, upload_id);
    std::fs::write(&part, b"").unwrap();
    write_upload_state(
        &sidecar,
        &ArtifactUploadState {
            path: "artifacts/persisted/file.bin".to_string(),
            expected_bytes: None,
            expected_sha256: None,
            mime_type: None,
            overwrite: false,
            max_bytes: 17,
        },
    )
    .unwrap();

    let mut runtime = ArtifactUploadRuntimeState::new();
    runtime
        .track_persisted(&root, &directory, upload_id, 17, Some(&store_root))
        .unwrap();
    assert!(reservation_path(&store_root, &root, upload_id).exists());

    let target = root.join("artifacts/new");
    std::fs::create_dir_all(&target).unwrap();
    let mut recovered = ArtifactUploadRuntimeState::new();
    recovered
        .prepare_directory(&root, &target, Some(&store_root))
        .unwrap();
    assert_eq!(
        recovered.project_usage(&root).unwrap(),
        ArtifactUploadProjectUsage {
            active_uploads: 1,
            reserved_bytes: 17,
        }
    );

    recovered
        .release(&root, upload_id, Some(&store_root))
        .unwrap();
    assert!(!reservation_path(&store_root, &root, upload_id).exists());
}

#[test]
fn restart_recovery_does_not_recurse_into_unindexed_project_children() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    let store_root = tmp.path().join("registry");
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let sibling = root.join("artifacts/unindexed");
    let target = root.join("artifacts/new");
    std::fs::create_dir_all(&sibling).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    let upload_id = "wc_upload_unindexed_sibling";
    let (part, sidecar) = upload_paths(&sibling, upload_id);
    std::fs::write(&part, b"").unwrap();
    write_upload_state(
        &sidecar,
        &ArtifactUploadState {
            path: "artifacts/unindexed/file.bin".to_string(),
            expected_bytes: None,
            expected_sha256: None,
            mime_type: None,
            overwrite: false,
            max_bytes: MAX_ARTIFACT_UPLOAD_BYTES,
        },
    )
    .unwrap();

    let mut runtime = ArtifactUploadRuntimeState::new();
    runtime
        .prepare_directory(&root, &target, Some(&store_root))
        .unwrap();

    assert_eq!(
        runtime.project_usage(&root).unwrap(),
        ArtifactUploadProjectUsage::default()
    );
    assert!(part.exists());
    assert!(sidecar.exists());
}

struct UploadFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    store: PathBuf,
    runtime: ArtifactUploadRuntimeState,
}

impl UploadFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir(&root).unwrap();
        Self {
            root: std::fs::canonicalize(root).unwrap(),
            store: temp.path().join("registry"),
            _temp: temp,
            runtime: ArtifactUploadRuntimeState::new(),
        }
    }

    fn call(&mut self, action: &str, path: &str, payload: Value) -> Value {
        let request = RunnerFilePayload {
            cwd: Some(self.root.to_string_lossy().into_owned()),
            path: path.to_string(),
            content: Some(payload.to_string()),
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
        };
        let handler = match action {
            "begin" => handle_artifact_upload_begin,
            "chunk" => handle_artifact_upload_chunk,
            "finish" => handle_artifact_upload_finish,
            "abort" => handle_artifact_upload_abort,
            _ => panic!("unknown test operation"),
        };
        let result = handler(
            &request,
            &self.root.join(path),
            Instant::now(),
            &mut self.runtime,
            Some(&self.store),
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        serde_json::from_str(result.stdout.as_deref().unwrap()).unwrap()
    }

    fn begin(&mut self, path: &str, expected: Option<usize>, max: usize) -> String {
        let result = self.call(
            "begin",
            path,
            json!({"expected_bytes": expected, "max_bytes": max}),
        );
        assert!(result.get("error").is_none(), "{result}");
        assert_eq!(result["max_bytes"], expected.unwrap_or(max));
        result["upload_id"].as_str().unwrap().to_string()
    }

    fn usage(&self, count: usize, bytes: usize) {
        assert_eq!(
            self.runtime.project_usage(&self.root).unwrap(),
            ArtifactUploadProjectUsage {
                active_uploads: count,
                reserved_bytes: bytes,
            }
        );
        let index = reservation_project_dir(&self.store, &self.root);
        let reservations: Vec<_> = std::fs::read_dir(index)
            .unwrap()
            .map(|entry| {
                read_upload_reservation_file(&entry.unwrap().path())
                    .unwrap()
                    .max_bytes
            })
            .collect();
        assert_eq!(reservations.len(), count);
        assert_eq!(reservations.iter().sum::<usize>(), bytes);
    }

    fn paths(&self, path: &str, id: &str) -> (PathBuf, PathBuf) {
        upload_paths(self.root.join(path).parent().unwrap(), id)
    }

    fn expire(&self, path: &str, id: &str) {
        let old = SystemTime::now() - Duration::from_secs(ARTIFACT_UPLOAD_IDLE_TTL_SECS + 60);
        let (part, sidecar) = self.paths(path, id);
        for file in [part, sidecar] {
            File::options()
                .write(true)
                .open(file)
                .unwrap()
                .set_modified(old)
                .unwrap();
        }
    }
}

#[test]
fn artifact_upload_small_files_repeated_finish_and_abort_release_all_state() {
    let mut fixture = UploadFixture::new();
    let data = vec![b'x'; 8192];
    for index in 0..40 {
        let path = format!("artifacts/small-{index}.bin");
        let id = fixture.begin(&path, Some(data.len()), MAX_ARTIFACT_UPLOAD_BYTES);
        fixture.usage(1, data.len());
        let chunk = fixture.call("chunk", &path, json!({
            "upload_id": id, "offset": 0, "content_base64": general_purpose::STANDARD.encode(&data),
        }));
        assert_eq!(chunk["received_bytes"], data.len(), "{chunk}");
        fixture.usage(1, data.len());
        let action = if index % 2 == 0 { "finish" } else { "abort" };
        let result = fixture.call(action, &path, json!({"upload_id": id}));
        assert_eq!(
            result[if action == "finish" {
                "committed"
            } else {
                "aborted"
            }],
            true,
            "{result}"
        );
        fixture.usage(0, 0);
        let (part, sidecar) = fixture.paths(&path, &id);
        assert!(!part.exists() && !sidecar.exists());
        if action == "finish" {
            assert_eq!(std::fs::read(fixture.root.join(path)).unwrap(), data);
        } else {
            assert!(!fixture.root.join(path).exists());
        }
    }
}

#[test]
fn artifact_upload_exact_byte_boundary_unknown_size_and_empty_restart() {
    let mut fixture = UploadFixture::new();
    fixture.begin("a/unknown.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.begin("b/bounded.bin", None, MAX_ARTIFACT_UPLOAD_BYTES - 8192);
    let small = fixture.begin("c/small.bin", Some(8192), MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.usage(3, MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT);
    let denied = fixture.call("begin", "d/over.bin", json!({"expected_bytes": 1}));
    assert!(denied["error"]
        .as_str()
        .unwrap()
        .contains("reserved byte quota exceeded"));
    // Empty uploads consume a count slot, but fit even when the byte quota is full.
    let empty = fixture.begin("e/empty.bin", Some(0), 1);
    fixture.usage(4, MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT);
    fixture.runtime = ArtifactUploadRuntimeState::new();
    fixture
        .runtime
        .prepare_directory(&fixture.root, &fixture.root, Some(&fixture.store))
        .unwrap();
    fixture.usage(4, MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT);
    let rejected = fixture.call(
        "chunk",
        "e/empty.bin",
        json!({
            "upload_id": empty, "offset": 0, "content_base64": "eA==",
        }),
    );
    assert_eq!(rejected["error"], "upload exceeds max_bytes");
    let finished = fixture.call("finish", "e/empty.bin", json!({"upload_id": empty}));
    assert_eq!(finished["committed"], true, "{finished}");
    assert_eq!(finished["bytes"], 0);
    assert_eq!(
        std::fs::metadata(fixture.root.join("e/empty.bin"))
            .unwrap()
            .len(),
        0
    );
    fixture.usage(3, MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT);
    let too_large = fixture.call("chunk", "c/small.bin", json!({
        "upload_id": small, "offset": 0, "content_base64": general_purpose::STANDARD.encode(vec![0; 8193]),
    }));
    assert_eq!(too_large["error"], "upload exceeds max_bytes");
}

#[test]
fn artifact_upload_pressure_reclaims_expired_cross_directory_pairs_and_keeps_recent_chunk() {
    let mut fixture = UploadFixture::new();
    let stale = fixture.begin("stale/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    let live = fixture.begin("live/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.expire("stale/file.bin", &stale);
    fixture.expire("live/file.bin", &live);
    let chunk = fixture.call(
        "chunk",
        "live/file.bin",
        json!({
            "upload_id": live, "offset": 0, "content_base64": "eA==",
        }),
    );
    assert_eq!(chunk["received_bytes"], 1);
    // The final target, if any, is never part of reservation cleanup.
    std::fs::write(fixture.root.join("stale/file.bin"), b"keep").unwrap();
    fixture.begin(
        "stale/replacement.bin",
        Some(8192),
        MAX_ARTIFACT_UPLOAD_BYTES,
    );
    fixture.usage(2, MAX_ARTIFACT_UPLOAD_BYTES + 8192);
    let (part, sidecar) = fixture.paths("stale/file.bin", &stale);
    assert!(!part.exists() && !sidecar.exists());
    let (part, sidecar) = fixture.paths("live/file.bin", &live);
    assert!(part.exists() && sidecar.exists());
    assert_eq!(
        std::fs::read(fixture.root.join("stale/file.bin")).unwrap(),
        b"keep"
    );
}

#[test]
fn artifact_upload_zero_byte_count_pressure_reclaims_only_expired_slots() {
    let mut fixture = UploadFixture::new();
    let mut uploads = Vec::new();
    for index in 0..MAX_ACTIVE_ARTIFACT_UPLOADS_PER_PROJECT {
        let path = format!("set-{index}/empty.bin");
        let id = fixture.begin(&path, Some(0), 1);
        uploads.push((path, id));
    }
    fixture.usage(MAX_ACTIVE_ARTIFACT_UPLOADS_PER_PROJECT, 0);
    let denied = fixture.call("begin", "new/empty.bin", json!({"expected_bytes": 0}));
    assert!(denied["error"]
        .as_str()
        .unwrap()
        .contains("active upload limit"));
    fixture.expire(&uploads[0].0, &uploads[0].1);
    fixture.begin("new/empty.bin", Some(0), 1);
    fixture.usage(MAX_ACTIVE_ARTIFACT_UPLOADS_PER_PROJECT, 0);
    for (path, id) in &uploads[1..] {
        let (part, sidecar) = fixture.paths(path, id);
        assert!(part.exists() && sidecar.exists());
    }
}

#[test]
fn artifact_upload_legacy_reservations_keep_cap_until_expiry_after_restart() {
    let mut fixture = UploadFixture::new();
    for index in 0..2 {
        let id = format!("wc_upload_legacy_{index}");
        let directory = fixture.root.join(format!("old-{index}"));
        write_active_upload(
            &fixture.store,
            &fixture.root,
            &directory,
            &id,
            MAX_ARTIFACT_UPLOAD_BYTES,
        );
        let (_, sidecar) = upload_paths(&directory, &id);
        let mut state = read_upload_state_file(&sidecar).unwrap();
        state.expected_bytes = Some(8192);
        // This is exactly the old sidecar and reservation format, without new fields.
        std::fs::write(sidecar, serde_json::to_vec(&state).unwrap()).unwrap();
    }
    let denied = fixture.call("begin", "new/file.bin", json!({"expected_bytes": 8192}));
    assert!(denied["error"]
        .as_str()
        .unwrap()
        .contains("reserved byte quota exceeded"));
    fixture.usage(2, MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT);
    fixture.expire("old-0/wc_upload_legacy_0.bin", "wc_upload_legacy_0");
    fixture.begin("new/file.bin", Some(8192), MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.usage(2, MAX_ARTIFACT_UPLOAD_BYTES + 8192);
}

#[test]
fn artifact_upload_failed_validation_keeps_charge_and_abort_cleanup_failure_is_retryable() {
    let mut fixture = UploadFixture::new();
    let path = "failure/file.bin";
    let id = fixture.begin(path, Some(8192), MAX_ARTIFACT_UPLOAD_BYTES);
    let failed = fixture.call("finish", path, json!({"upload_id": id}));
    assert_eq!(
        failed["error"],
        "uploaded byte count does not match expected_bytes"
    );
    fixture.usage(1, 8192);
    let (part, sidecar) = fixture.paths(path, &id);
    std::fs::remove_file(&part).unwrap();
    std::fs::create_dir(&part).unwrap();
    let failed = fixture.call("abort", path, json!({"upload_id": id}));
    assert_eq!(failed["aborted"], false);
    assert!(failed["error"].as_str().unwrap().contains("cleanup failed"));
    fixture.usage(1, 8192);
    assert!(sidecar.exists());
    std::fs::remove_dir(&part).unwrap();
    std::fs::write(&part, b"").unwrap();
    let aborted = fixture.call("abort", path, json!({"upload_id": id}));
    assert_eq!(aborted["aborted"], true, "{aborted}");
    fixture.usage(0, 0);
}

#[test]
fn artifact_upload_reservation_cleanup_failure_retains_charge_until_orphan_recovery() {
    let mut fixture = UploadFixture::new();
    let path = "failure/file.bin";
    let id = fixture.begin(path, None, MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.begin("live/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    let reservation = reservation_path(&fixture.store, &fixture.root, &id);
    let data = std::fs::read(&reservation).unwrap();
    std::fs::remove_file(&reservation).unwrap();
    std::fs::create_dir(&reservation).unwrap();
    assert!(fixture
        .runtime
        .release(&fixture.root, &id, Some(&fixture.store))
        .is_err());
    assert_eq!(
        fixture
            .runtime
            .project_usage(&fixture.root)
            .unwrap()
            .reserved_bytes,
        MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT
    );
    let (part, _) = fixture.paths(path, &id);
    std::fs::rename(part, fixture.root.join(path)).unwrap();
    std::fs::remove_dir(&reservation).unwrap();
    std::fs::write(&reservation, data).unwrap();
    fixture.begin("new/file.bin", Some(8192), MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.usage(2, MAX_ARTIFACT_UPLOAD_BYTES + 8192);
    let (_, sidecar) = fixture.paths(path, &id);
    assert!(!sidecar.exists());
    assert!(fixture.root.join(path).exists());
}

#[test]
fn artifact_upload_index_error_fails_closed_without_deleting_live_pairs() {
    let mut fixture = UploadFixture::new();
    let id = fixture.begin("a/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.begin("b/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    let reservation = reservation_path(&fixture.store, &fixture.root, &id);
    std::fs::write(&reservation, b"invalid").unwrap();
    let denied = fixture.call("begin", "c/file.bin", json!({"expected_bytes": 8192}));
    assert!(denied["error"]
        .as_str()
        .unwrap()
        .contains("invalid upload reservation"));
    assert_eq!(
        fixture
            .runtime
            .project_usage(&fixture.root)
            .unwrap()
            .reserved_bytes,
        MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT
    );
    let (part, sidecar) = fixture.paths("a/file.bin", &id);
    assert!(part.exists() && sidecar.exists() && reservation.exists());
}

#[test]
fn artifact_upload_begin_reservation_failure_removes_temp_file() {
    let mut fixture = UploadFixture::new();
    let parent = fixture.root.join("failure");
    std::fs::create_dir(&parent).unwrap();
    fixture
        .runtime
        .prepare_directory(&fixture.root, &parent, Some(&fixture.store))
        .unwrap();
    std::fs::write(&fixture.store, b"not a directory").unwrap();
    let failed = fixture.call("begin", "failure/file.bin", json!({"expected_bytes": 8192}));
    assert!(failed["error"]
        .as_str()
        .unwrap()
        .contains("upload reservation"));
    assert_eq!(
        std::fs::read_dir(fixture.root.join("failure"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fixture.runtime.project_usage(&fixture.root).unwrap(),
        ArtifactUploadProjectUsage::default()
    );
    std::fs::remove_file(&fixture.store).unwrap();
    fixture.begin("failure/file.bin", Some(8192), MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.usage(1, 8192);
}

#[test]
fn artifact_upload_recovery_uses_newest_mtime_and_exact_24_hour_ttl() {
    let mut fixture = UploadFixture::new();
    let id = fixture.begin("ttl/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    let (part, sidecar) = fixture.paths("ttl/file.bin", &id);
    let modified = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    File::options()
        .write(true)
        .open(&part)
        .unwrap()
        .set_modified(modified - Duration::from_secs(3600))
        .unwrap();
    File::options()
        .write(true)
        .open(&sidecar)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let ttl = Duration::from_secs(ARTIFACT_UPLOAD_IDLE_TTL_SECS);
    for now in [
        modified - Duration::from_secs(1),
        modified + ttl - Duration::from_secs(1),
    ] {
        let active =
            recover_artifact_upload_reservations(&fixture.store, &fixture.root, now, ttl).unwrap();
        assert_eq!(active, vec![(id.clone(), MAX_ARTIFACT_UPLOAD_BYTES)]);
    }
    let active =
        recover_artifact_upload_reservations(&fixture.store, &fixture.root, modified + ttl, ttl)
            .unwrap();
    assert!(active.is_empty());
    assert!(!part.exists() && !sidecar.exists());
}

#[test]
fn artifact_upload_pressure_is_project_scoped_and_index_scan_stays_bounded() {
    let mut fixture = UploadFixture::new();
    let stale = fixture.begin("a/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.begin("b/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.expire("a/file.bin", &stale);
    let original = fixture.root.clone();
    fixture.root = fixture._temp.path().join("other-project");
    std::fs::create_dir(&fixture.root).unwrap();
    fixture.root = std::fs::canonicalize(&fixture.root).unwrap();
    fixture.begin("a/file.bin", Some(8192), MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.usage(1, 8192);
    fixture.root = original;
    fixture.usage(2, MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT);
    assert!(fixture.paths("a/file.bin", &stale).0.exists());
    let index = reservation_project_dir(&fixture.store, &fixture.root);
    for i in 0..MAX_ARTIFACT_UPLOAD_RESERVATION_SCAN_ENTRIES {
        std::fs::write(index.join(format!("unrecognized-{i}")), b"").unwrap();
    }
    let denied = fixture.call("begin", "c/file.bin", json!({"expected_bytes": 8192}));
    assert!(denied["error"]
        .as_str()
        .unwrap()
        .contains("index has more than"));
    assert_eq!(
        fixture
            .runtime
            .project_usage(&fixture.root)
            .unwrap()
            .reserved_bytes,
        MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT
    );
}

#[test]
fn artifact_upload_restart_cleans_partial_finish_and_abort_without_touching_final_files() {
    for missing in ["part", "sidecar", "both"] {
        let mut fixture = UploadFixture::new();
        let path = "old/file.bin";
        let id = fixture.begin(path, None, MAX_ARTIFACT_UPLOAD_BYTES);
        let (part, sidecar) = fixture.paths(path, &id);
        std::fs::write(fixture.root.join(path), b"committed-or-preexisting").unwrap();
        if missing != "sidecar" {
            std::fs::remove_file(&part).unwrap();
        }
        if missing != "part" {
            std::fs::remove_file(&sidecar).unwrap();
        }
        fixture.runtime = ArtifactUploadRuntimeState::new();
        fixture.begin("new/file.bin", Some(8192), MAX_ARTIFACT_UPLOAD_BYTES);
        fixture.usage(1, 8192);
        assert!(!part.exists() && !sidecar.exists());
        assert_eq!(
            std::fs::read(fixture.root.join(path)).unwrap(),
            b"committed-or-preexisting"
        );
    }
}

#[test]
fn artifact_upload_legacy_unindexed_pair_is_adopted_when_directory_is_used() {
    let mut fixture = UploadFixture::new();
    let directory = fixture.root.join("legacy");
    let id = "wc_upload_pre_index";
    write_active_upload(
        &fixture.store,
        &fixture.root,
        &directory,
        id,
        MAX_ARTIFACT_UPLOAD_BYTES,
    );
    remove_upload_reservation(&fixture.store, &fixture.root, id).unwrap();
    fixture.begin("legacy/new.bin", Some(8192), MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.usage(2, MAX_ARTIFACT_UPLOAD_BYTES + 8192);
    let (part, sidecar) = upload_paths(&directory, id);
    assert!(part.exists() && sidecar.exists());
}

#[cfg(unix)]
#[test]
fn artifact_upload_pressure_rejects_redirected_reservation_directory() {
    let mut fixture = UploadFixture::new();
    let id = fixture.begin("redirect/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.begin("live/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.expire("redirect/file.bin", &id);
    let moved = fixture._temp.path().join("outside");
    std::fs::rename(fixture.root.join("redirect"), &moved).unwrap();
    std::os::unix::fs::symlink(&moved, fixture.root.join("redirect")).unwrap();
    let denied = fixture.call("begin", "new/file.bin", json!({"expected_bytes": 8192}));
    assert!(denied["error"]
        .as_str()
        .unwrap()
        .contains("escapes project root"));
    fixture.usage(2, MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT);
    let (part, sidecar) = upload_paths(&moved, &id);
    assert!(part.exists() && sidecar.exists());
}

#[test]
fn artifact_upload_pressure_releases_reservation_for_removed_directory() {
    let mut fixture = UploadFixture::new();
    fixture.begin("removed/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.begin("live/file.bin", None, MAX_ARTIFACT_UPLOAD_BYTES);
    std::fs::remove_dir_all(fixture.root.join("removed")).unwrap();
    fixture.begin("new/file.bin", Some(8192), MAX_ARTIFACT_UPLOAD_BYTES);
    fixture.usage(2, MAX_ARTIFACT_UPLOAD_BYTES + 8192);
}
