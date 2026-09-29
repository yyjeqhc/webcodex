use super::*;
use std::fs;

#[test]
fn read_is_side_effect_free_and_save_is_complete_revision_fenced() {
    let root = tempfile::tempdir().unwrap();
    let store = ManagedInstructions::new(root.path().into());
    let missing = store.read().unwrap();
    assert!(!missing.exists);
    assert_eq!(missing.revision, "missing");
    assert!(!root.path().join("instructions").exists());
    let text = "完整规则\n".repeat(12_000); // Well beyond observation preview budgets.
    let saved = store
        .save(SaveRequest {
            expected_revision: missing.revision,
            content: text.clone(),
        })
        .unwrap();
    assert_eq!(saved.content, text);
    assert_eq!(store.read().unwrap().content, text);
    fs::write(store.path(), "external VS Code edit").unwrap();
    let error = store
        .save(SaveRequest {
            expected_revision: saved.revision,
            content: "stale draft".into(),
        })
        .unwrap_err();
    assert_eq!(error.code, "managed_instructions_conflict");
    assert_eq!(store.read().unwrap().content, "external VS Code edit");
    assert_eq!(
        fs::read_dir(root.path().join("instructions"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn size_utf8_missing_and_empty_are_distinct() {
    let root = tempfile::tempdir().unwrap();
    let store = ManagedInstructions::new(root.path().into());
    let empty = store.ensure("missing").unwrap();
    assert!(empty.exists);
    assert_ne!(empty.revision, "missing");
    let maximum = store
        .save(SaveRequest {
            expected_revision: empty.revision,
            content: "a".repeat(MAX_BYTES),
        })
        .unwrap();
    assert_eq!(maximum.content.len(), MAX_BYTES);
    assert!(store
        .save(SaveRequest {
            expected_revision: maximum.revision.clone(),
            content: "a".repeat(MAX_BYTES + 1)
        })
        .is_err());
    assert_eq!(store.read().unwrap().revision, maximum.revision);
    fs::write(store.path(), [0xff, 0xfe]).unwrap();
    assert!(store.read().is_err());
    fs::write(store.path(), "b".repeat(MAX_BYTES + 1)).unwrap();
    assert!(store.read().is_err());
}

#[test]
fn enable_preserves_existing_content_and_fixed_path_apis_are_closed() {
    let root = tempfile::tempdir().unwrap();
    let store = ManagedInstructions::new(root.path().into());
    let saved = store
        .save(SaveRequest {
            expected_revision: "missing".into(),
            content: "my rules".into(),
        })
        .unwrap();
    assert_eq!(store.ensure(&saved.revision).unwrap().content, "my rules");
    for key in ["path", "file", "root", "source_scope"] {
        let mut value = serde_json::json!({"expected_revision":"missing","content":"text"});
        value[key] = serde_json::json!("/arbitrary/target");
        assert!(serde_json::from_value::<SaveRequest>(value).is_err());
    }
}

#[cfg(unix)]
#[test]
fn managed_root_directory_file_symlinks_and_special_files_fail_closed() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("AGENTS.md"), "outside untouched").unwrap();
    let linked_root = temp.path().join("root");
    symlink(outside.path(), &linked_root).unwrap();
    assert!(ManagedInstructions::new(linked_root).read().is_err());
    let root = temp.path().join("owned");
    fs::create_dir(&root).unwrap();
    let store = ManagedInstructions::new(root.clone());
    symlink(outside.path(), root.join("instructions")).unwrap();
    assert!(store.read().is_err());
    assert!(store.ensure("missing").is_err());
    fs::remove_file(root.join("instructions")).unwrap();
    fs::create_dir(root.join("instructions")).unwrap();
    symlink(outside.path().join("AGENTS.md"), store.path()).unwrap();
    assert!(store.read().is_err());
    assert!(store
        .save(SaveRequest {
            expected_revision: "missing".into(),
            content: "bad".into()
        })
        .is_err());
    fs::remove_file(store.path()).unwrap();
    let path = std::ffi::CString::new(store.path().to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    assert!(store.read().is_err()); // Nonblocking open must not hang on FIFO.
    assert_eq!(
        fs::read_to_string(outside.path().join("AGENTS.md")).unwrap(),
        "outside untouched"
    );
}

#[cfg(windows)]
#[test]
fn managed_directory_junction_is_not_followed() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("AGENTS.md"), "outside").unwrap();
    let junction = root.path().join("instructions");
    let result = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(outside.path())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(result.status.success(), "could not create junction fixture");
    let store = ManagedInstructions::new(root.path().into());
    assert!(store.read().is_err());
    assert!(store.ensure("missing").is_err());
    fs::remove_dir(junction).unwrap();
    assert_eq!(
        fs::read_to_string(outside.path().join("AGENTS.md")).unwrap(),
        "outside"
    );
}

#[test]
fn parallel_editors_do_not_silently_overwrite_one_another() {
    let root = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(ManagedInstructions::new(root.path().into()));
    let original = store.ensure("missing").unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut handles = Vec::new();
    for content in ["one", "two"] {
        let store = store.clone();
        let barrier = barrier.clone();
        let revision = original.revision.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            store.save(SaveRequest {
                expected_revision: revision,
                content: content.into(),
            })
        }));
    }
    barrier.wait();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .next()
            .unwrap()
            .code,
        "managed_instructions_conflict"
    );
}
