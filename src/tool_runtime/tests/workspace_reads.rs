use super::*;
use crate::tool_runtime::ToolRuntime;
use std::time::Duration;

fn resolved() -> ResolvedProject {
    ResolvedProject {
        input: "short-alias".into(),
        resolved_id: "agent:runner:project".into(),
        config: crate::projects::ProjectConfig {
            path: "/exact/root".into(),
            client_id: "runner".into(),
            allow_patch: true,
        },
        root_fingerprint: Some("root-incarnation".into()),
        knowledge_association: None,
    }
}

#[test]
fn revision_target_keeps_all_six_retarget_fences() {
    let project = resolved();
    let target = WorkspaceReadRuntime::revision_target(&project, "a.rs", "instance");
    assert_eq!(
        target,
        ReadRevisionTarget {
            project_id: "agent:runner:project".into(),
            path: "a.rs".into(),
            client_id: "runner".into(),
            runner_instance_id: "instance".into(),
            project_root: "/exact/root".into(),
            root_fingerprint: Some("root-incarnation".into()),
        }
    );
    let reads = WorkspaceReadRuntime::default();
    let revision = reads.observe_revision(target, "old-sha");
    for field in 0..6 {
        let mut changed = project.clone();
        let mut path = "a.rs";
        let mut instance = "instance";
        let expected = match field {
            0 => {
                changed.resolved_id.push_str("-other");
                ReadRevisionLookupError::ProjectMismatch
            }
            1 => {
                path = "b.rs";
                ReadRevisionLookupError::PathMismatch
            }
            2 => {
                changed.config.client_id.push_str("-other");
                ReadRevisionLookupError::OwnerMismatch
            }
            3 => {
                instance = "replacement";
                ReadRevisionLookupError::OwnerMismatch
            }
            4 => {
                changed.config.path.push_str("-other");
                ReadRevisionLookupError::OwnerMismatch
            }
            _ => {
                changed.root_fingerprint = None;
                ReadRevisionLookupError::OwnerMismatch
            }
        };
        assert_eq!(
            reads.resolve_revision(
                revision,
                &WorkspaceReadRuntime::revision_target(&changed, path, instance)
            ),
            Err(expected),
            "field {field}"
        );
    }
}

#[test]
fn cloned_views_share_read_state_but_not_deadline_policy() {
    let runtime = ToolRuntime::new_for_tests();
    let original_deadline = runtime.read_files_deadline;
    let clone = runtime
        .clone()
        .with_read_files_deadline(Duration::from_millis(7));
    assert!(Arc::ptr_eq(&runtime.reads, &clone.reads));
    assert_eq!(runtime.read_files_deadline, original_deadline);
    assert_eq!(clone.read_files_deadline, Duration::from_millis(7));
    let target = WorkspaceReadRuntime::revision_target(&resolved(), "a.rs", "instance");
    let old = runtime.reads.observe_revision(target.clone(), "old-sha");
    assert_eq!(
        clone.reads.resolve_revision(old, &target).unwrap(),
        "old-sha"
    );
    let new = clone.reads.observe_revision(target.clone(), "new-sha");
    assert_ne!(new, old);
    // Retained old handles still resolve: only a fresh Runner SHA guard can
    // decide if the old content is current, including external filesystem writes.
    assert_eq!(
        runtime.reads.resolve_revision(old, &target).unwrap(),
        "old-sha"
    );
    assert_eq!(
        runtime.reads.resolve_revision(new, &target).unwrap(),
        "new-sha"
    );
    let independent = ToolRuntime::new_for_tests();
    assert_eq!(
        independent.reads.resolve_revision(old, &target),
        Err(ReadRevisionLookupError::Unknown)
    );
    let weak = Arc::downgrade(&runtime.reads);
    drop(runtime);
    assert!(weak.upgrade().is_some());
    drop(clone);
    assert!(weak.upgrade().is_none());
}
