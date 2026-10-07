use super::*;
use crate::BrowserSnapshotQuery;

struct QueryFactory(usize);
impl BackendFactory for QueryFactory {
    fn available(&self) -> bool {
        true
    }
    fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
        let mut backend = FakeBackend::new();
        backend.query_nodes = Some(
            (0..self.0)
                .map(|index| {
                    if index % 300 == 0 {
                        fixture_node(
                            "textbox",
                            &format!("School {index}"),
                            Some(index as i64 + 1),
                            ControlCapability::text_input(),
                        )
                    } else {
                        fixture_node(
                            "StaticText",
                            "Decoration",
                            None,
                            ControlCapability::default(),
                        )
                    }
                })
                .collect(),
        );
        Ok(Box::new(backend))
    }
}
fn setup(count: usize) -> (BrowserSupervisor, String, String) {
    let supervisor = BrowserSupervisor::with_factory(Arc::new(QueryFactory(count)));
    let browser = supervisor.launch().unwrap().browser_id;
    let page = supervisor.pages(&browser, 8).unwrap().remove(0).page_id;
    (supervisor, browser, page)
}

#[test]
fn fields_across_snapshot_pages_share_one_generation_and_batch_authority() {
    let (supervisor, browser, page) = setup(901);
    let query = BrowserSnapshotQuery {
        fields_only: true,
        ..Default::default()
    };
    let result = supervisor
        .snapshot_query(&browser, &page, SnapshotMode::Auto, 32, 32, 0, Some(&query))
        .unwrap();
    assert_eq!(result.node_count, 4);
    assert_eq!(result.nodes[3].name.as_deref(), Some("School 900"));
    assert!(!result.truncated);
    assert!(!result.auto_compacted);
    let operations = result
        .nodes
        .iter()
        .map(|node| BatchOperation::InputText {
            element_id: node.element_id.clone().unwrap(),
            text: "Synthetic School".into(),
        })
        .collect::<Vec<_>>();
    let batch = supervisor.batch(&browser, &page, &operations).unwrap();
    assert_eq!(batch.completed_count, 4);
    // Even the identical query is a fresh snapshot, never a reusable element cache.
    supervisor
        .snapshot_query(&browser, &page, SnapshotMode::Auto, 32, 32, 0, Some(&query))
        .unwrap();
    let stale = supervisor.batch(&browser, &page, &operations).unwrap();
    assert_eq!(stale.completed_count, 0);
    assert_eq!(stale.remaining_count, 4);
}

#[test]
fn absent_target_terminates_and_scan_limit_is_not_reported_as_absence() {
    for (count, incomplete) in [(901, false), (5000, true)] {
        let (supervisor, browser, page) = setup(count);
        let query = BrowserSnapshotQuery {
            text: Some("Missing".into()),
            ..Default::default()
        };
        let result = supervisor
            .snapshot_query(&browser, &page, SnapshotMode::Auto, 32, 32, 0, Some(&query))
            .unwrap();
        assert!(result.nodes.is_empty());
        assert_eq!(result.next_node_offset, None);
        assert_eq!(result.truncated, incomplete);
    }
}

#[test]
fn query_filters_before_windowing_and_keeps_text_in_auto_mode() {
    let (supervisor, browser, page) = setup(901);
    let query = BrowserSnapshotQuery {
        fields_only: true,
        text: Some("sCHool".into()),
        role: Some("TEXTBOX".into()),
        ..Default::default()
    };
    let result = supervisor
        .snapshot_query(&browser, &page, SnapshotMode::Auto, 2, 32, 1, Some(&query))
        .unwrap();
    assert_eq!(result.nodes[0].name.as_deref(), Some("School 300"));
    assert_eq!(result.next_node_offset, Some(3));
    let query = BrowserSnapshotQuery {
        text: Some("Decoration".into()),
        ..Default::default()
    };
    let result = supervisor
        .snapshot_query(&browser, &page, SnapshotMode::Auto, 2, 32, 0, Some(&query))
        .unwrap();
    assert_eq!(result.node_count, 2);
    assert!(result.nodes.iter().all(|node| node.element_id.is_none()));
}

#[test]
fn query_validation_and_group_filters_fail_closed() {
    let (supervisor, browser, page) = setup(901);
    assert_eq!(
        supervisor
            .snapshot_query(
                &browser,
                &page,
                SnapshotMode::Auto,
                32,
                32,
                0,
                Some(&BrowserSnapshotQuery::default()),
            )
            .unwrap_err()
            .kind,
        "invalid_request"
    );
    for text in [" ".into(), "x".repeat(129)] {
        let query = BrowserSnapshotQuery {
            text: Some(text),
            ..Default::default()
        };
        assert_eq!(
            supervisor
                .snapshot_query(&browser, &page, SnapshotMode::Auto, 32, 32, 0, Some(&query))
                .unwrap_err()
                .kind,
            "invalid_request"
        );
    }
    let query = BrowserSnapshotQuery {
        group: Some("Education".into()),
        ..Default::default()
    };
    assert!(supervisor
        .snapshot_query(&browser, &page, SnapshotMode::Auto, 32, 32, 0, Some(&query))
        .unwrap()
        .nodes
        .is_empty());
}

#[test]
fn semantic_group_and_section_filters_are_conjunctive_and_preserve_disabled_fields() {
    let mut node = fixture_node("textbox", "", Some(1), ControlCapability::default());
    node.disabled = Some(true);
    let context = crate::FormContext {
        field_signature: "synthetic".into(),
        dom_tag: "input".into(),
        input_type: Some("text".into()),
        html_name: None,
        placeholder: None,
        autocomplete: None,
        nearby_label: Some("学校名称".into()),
        group_label: Some("Education 2".into()),
        section_label: Some("Academic history".into()),
        group_index: None,
        group_size: None,
        component_hint: None,
        aria_invalid: None,
        validation_hint: None,
        option_count: None,
    };
    let mut query = BrowserSnapshotQuery {
        fields_only: true,
        text: Some("学校".into()),
        group: Some("education 2".into()),
        section: Some("academic".into()),
        ..Default::default()
    };
    assert!(matches_snapshot_query(&node, Some(&context), &query));
    query.group = Some("Education 1".into());
    assert!(!matches_snapshot_query(&node, Some(&context), &query));
    query.group = None;
    query.text = Some("synthetic".into());
    assert!(!matches_snapshot_query(&node, Some(&context), &query));
}

#[test]
fn inferred_card_uses_only_published_click_authority_and_stales_on_refresh() {
    struct CardFactory;
    impl BackendFactory for CardFactory {
        fn available(&self) -> bool {
            true
        }
        fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
            let mut backend = FakeBackend::new();
            // CDP inference is exercised in cdp/tests/clickable.rs. Its output
            // enters the same supervisor projection as every native control.
            backend.query_nodes = Some(vec![fixture_node(
                "generic",
                "Graduate engineer",
                Some(12),
                ControlCapability::click(),
            )]);
            Ok(Box::new(backend))
        }
    }
    let supervisor = BrowserSupervisor::with_factory(Arc::new(CardFactory));
    let browser = supervisor.launch().unwrap().browser_id;
    let page = supervisor.pages(&browser, 8).unwrap().remove(0).page_id;
    let query = BrowserSnapshotQuery {
        text: Some("engineer".into()),
        ..Default::default()
    };
    let snapshot = supervisor
        .snapshot_query(&browser, &page, SnapshotMode::Auto, 32, 32, 0, Some(&query))
        .unwrap();
    let card = &snapshot.nodes[0];
    assert_eq!(card.actions, ["click"]);
    let id = card.element_id.as_deref().unwrap();
    assert_eq!(
        supervisor
            .input_text(&browser, &page, id, "x")
            .unwrap_err()
            .kind,
        "element_action_unsupported"
    );
    supervisor.click(&browser, &page, id).unwrap();
    supervisor
        .snapshot(&browser, &page, SnapshotMode::Auto, 32, 32)
        .unwrap();
    assert_eq!(
        supervisor.click(&browser, &page, id).unwrap_err().kind,
        "stale_element"
    );
}
