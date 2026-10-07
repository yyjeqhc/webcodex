use super::*;

struct FrameFactory(Arc<Mutex<String>>, usize);
impl BackendFactory for FrameFactory {
    fn available(&self) -> bool {
        true
    }
    fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
        let mut backend = FakeBackend::new();
        backend.snapshot_node_count = self.1;
        backend.frame_state = Some(self.0.clone());
        Ok(Box::new(backend))
    }
}

#[test]
fn iframe_actions_and_snapshot_generation_use_the_same_authority_path() {
    let state = Arc::new(Mutex::new("frame-loader-document-1".into()));
    let supervisor = BrowserSupervisor::with_factory(Arc::new(FrameFactory(state.clone(), 1)));
    let browser = supervisor.launch().unwrap().browser_id;
    let page = supervisor.pages(&browser, 8).unwrap().remove(0).page_id;
    let snapshot = supervisor
        .snapshot(&browser, &page, SnapshotMode::Full, 32, 32)
        .unwrap();
    let ids = snapshot
        .nodes
        .iter()
        .map(|n| n.element_id.as_ref().unwrap())
        .collect::<Vec<_>>();
    supervisor
        .input_text(&browser, &page, ids[0], "Alice")
        .unwrap();
    supervisor
        .select_option(&browser, &page, ids[2], "a")
        .unwrap();
    supervisor
        .set_value(&browser, &page, ids[3], "2026-10")
        .unwrap();
    supervisor.click(&browser, &page, ids[4]).unwrap();
    assert_eq!(
        supervisor.click(&browser, &page, ids[2]).unwrap_err().kind,
        "element_action_unsupported"
    );
    *state.lock().unwrap() = "frame-loader-document-2".into();
    assert_eq!(
        supervisor
            .input_text(&browser, &page, ids[0], "Bob")
            .unwrap_err()
            .kind,
        "stale_element"
    );
    let fresh = supervisor
        .snapshot(&browser, &page, SnapshotMode::Full, 32, 32)
        .unwrap();
    let fresh_id = fresh.nodes[0].element_id.as_ref().unwrap();
    supervisor
        .input_text(&browser, &page, fresh_id, "Bob")
        .unwrap();
    supervisor
        .snapshot(&browser, &page, SnapshotMode::Full, 32, 32)
        .unwrap();
    assert_eq!(
        supervisor
            .input_text(&browser, &page, fresh_id, "Carol")
            .unwrap_err()
            .kind,
        "stale_element"
    );
    let output = serde_json::to_string(&fresh).unwrap();
    for private in [
        "frame-loader-document",
        "frame_fence",
        "backend_node",
        "private-target",
    ] {
        assert!(!output.contains(private));
    }
}

#[test]
fn iframe_disabled_and_read_only_controls_publish_no_effect_authority() {
    let state = Arc::new(Mutex::new("frame-loader-document-1".into()));
    let supervisor =
        BrowserSupervisor::with_factory(Arc::new(IframeControlStateFactory(state.clone())));
    let browser = supervisor.launch().unwrap().browser_id;
    let page = supervisor.pages(&browser, 8).unwrap().remove(0).page_id;
    let snapshot = supervisor
        .snapshot(&browser, &page, SnapshotMode::Full, 32, 32)
        .unwrap();
    let find = |name: &str| {
        snapshot
            .nodes
            .iter()
            .find(|node| node.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("missing {name}"))
    };
    assert_eq!(find("Qty").disabled, Some(true));
    assert!(find("Qty").actions.is_empty());
    assert!(find("Qty").element_id.is_none());
    assert!(!find("Qty").actionable);
    assert!(find("Pick").actions.is_empty());
    assert!(find("Pick").element_id.is_none());
    assert!(find("Alpha").actions.is_empty());
    assert!(find("Alpha").element_id.is_none());
    let notes = find("Notes");
    assert_eq!(notes.read_only, Some(true));
    assert_eq!(notes.actions, ["click"]);
    let notes_id = notes.element_id.clone().unwrap();
    supervisor.click(&browser, &page, &notes_id).unwrap();
    let rejected = supervisor
        .input_text(&browser, &page, &notes_id, "later")
        .unwrap_err();
    assert_eq!(rejected.kind, "element_action_unsupported");
    assert_eq!(rejected.execution_state, ExecutionState::NotStarted);
    assert_eq!(rejected.recovery_action, None);
    let name_id = find("Name").element_id.clone().unwrap();
    assert_eq!(find("Name").actions, ["click", "input_text", "set_value"]);
    supervisor
        .input_text(&browser, &page, &name_id, "Ada")
        .unwrap();
    assert_eq!(find("Amount").disabled, Some(false));
    assert_eq!(find("Amount").actions, ["set_value"]);
    supervisor
        .set_value(
            &browser,
            &page,
            find("Amount").element_id.as_ref().unwrap(),
            "4",
        )
        .unwrap();
    assert_eq!(find("Count").disabled, None);
    assert_eq!(find("Count").actions, ["set_value"]);
    assert_eq!(find("Kind").actions, ["select_option"]);
    *state.lock().unwrap() = "frame-loader-document-2".into();
    let stale = supervisor.click(&browser, &page, &notes_id).unwrap_err();
    assert_eq!(stale.kind, "stale_element");
    assert_eq!(stale.execution_state, ExecutionState::NotStarted);
    assert_eq!(stale.recovery_action, Some("snapshot"));

    let interactive = supervisor
        .snapshot(
            &browser,
            &page,
            SnapshotMode::Interactive,
            MAX_SNAPSHOT_NODES,
            DEFAULT_SNAPSHOT_DEPTH,
        )
        .unwrap();
    let kept = |name: &str| {
        interactive
            .nodes
            .iter()
            .any(|node| node.name.as_deref() == Some(name))
    };
    assert!(kept("Qty") && kept("Pick") && kept("Notes") && kept("Beta"));
    assert!(!kept("Alpha") && !kept("Static"));
    assert!(interactive.nodes.iter().any(|node| {
        node.name.as_deref() == Some("Qty") && node.element_id.is_none() && node.actions.is_empty()
    }));
}

struct IframeControlStateFactory(Arc<Mutex<String>>);
impl BackendFactory for IframeControlStateFactory {
    fn available(&self) -> bool {
        true
    }
    fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
        Ok(Box::new(FakeBackend::with_iframe_control_state(
            self.0.clone(),
        )))
    }
}

#[test]
fn iframe_nodes_share_page_projection_limits() {
    let state = Arc::new(Mutex::new("frame-loader-document-1".into()));
    let supervisor = BrowserSupervisor::with_factory(Arc::new(FrameFactory(state, 256)));
    let browser = supervisor.launch().unwrap().browser_id;
    let page = supervisor.pages(&browser, 8).unwrap().remove(0).page_id;
    let snapshot = supervisor
        .snapshot(&browser, &page, SnapshotMode::Full, 2, 32)
        .unwrap();
    assert_eq!(snapshot.nodes.len(), 2);
    assert!(snapshot.truncated);
    let snapshot = supervisor
        .snapshot(&browser, &page, SnapshotMode::Full, MAX_SNAPSHOT_NODES, 32)
        .unwrap();
    assert!(snapshot.truncated);
    assert!(snapshot.nodes.len() < MAX_SNAPSHOT_NODES);
    assert!(
        snapshot
            .nodes
            .iter()
            .map(|n| serde_json::to_vec(n).unwrap().len())
            .sum::<usize>()
            <= MAX_SNAPSHOT_BYTES
    );
}
